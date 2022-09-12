use byteorder::*;
use log::{info, warn, error};
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::io::{Error, ErrorKind};
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::UdpSocket;
use std::rc::Rc;
use std::time::{Instant, Duration};

mod nametree;
mod tokengen;

struct CacheEntry {
    a: Ipv4Addr,
    expiry: Instant,
}

impl CacheEntry {
    fn new(a: &Ipv4Addr, ttl: u64) -> CacheEntry {
	CacheEntry{
	    a: a.clone(),
	    expiry: Instant::now() + Duration::from_secs(ttl),
	}
    }
    fn get_ttl(&self) -> u64 {
	return (self.expiry - Instant::now()).as_secs();
    }
    fn is_valid(&self) -> bool {
	return self.expiry > Instant::now();
    }
}

struct Cache {
    table: HashMap<String, CacheEntry>,
}

impl Cache {
    fn new() -> Cache {
	Cache{table: HashMap::new()}
    }

    fn insert(&mut self, name: &str, a: &Ipv4Addr, ttl: u64) {
	self.table.insert(name.to_lowercase().to_owned(),
			  CacheEntry::new(a, ttl));
    }

    fn get_(&mut self, name: &str) -> Option<&CacheEntry> {
	match self.table.get(&name.to_lowercase()) {
	    Some(entry) => Some(entry),
	    None => None,
	}
    }

    fn get(&mut self, name: &str) -> Option<(Ipv4Addr, u64)> {
	if let Entry::Occupied(e) = self.table.entry(name.to_lowercase()) {
            if !e.get().is_valid() {
		e.remove_entry();
		return None;
            }
	    return Some((e.get().a.clone(), e.get().get_ttl()));
	}
	return None;
    }
}

struct BitCursor {
    value: u8,
    cur: usize,
}

impl BitCursor {
    fn new<T>(c: &mut std::io::Cursor<T>) -> Result<BitCursor, std::io::Error> where T: AsRef<[u8]> {
        let mut value = [0; 1];
        c.read_exact(&mut value)?;
        return Ok(BitCursor{
            value: value[0],
            cur: 8,
        });
    }
    
    fn read(&mut self, bits: usize) -> Result<u8, std::io::Error> {
        if bits > self.cur {
            return Err(Error::new(ErrorKind::Other, "BitCursor overflow"));
        }
        let mask = (0x1 << bits) - 1;
        let result = (self.value >> (self.cur - bits)) & mask;
        self.cur -= bits;
        return Ok(result);
    }
}

#[derive(Debug, Clone)]
struct Question {
    name: String,
    typ: u32,
    class: u32, 
}

#[derive(Debug, Clone)]
struct SoaData {
    mname: String,
    rname: String,
    serial: u32,
    refresh: u32,
    retry: u32,
    expire: u32,
    minimum: u32,
}

#[derive(Debug, Clone)]
enum ResourceData {
    IPv4(Ipv4Addr),
    IPv6(Ipv6Addr),
    CName(String),
    Soa(SoaData),
    Other(u32),
}

#[derive(Debug, Clone)]
struct ResourceRecord {
    name: String,
    typ: u32,
    class: u32,
    ttl: u32,
    data: ResourceData,
}

#[derive(Debug, Clone)]
enum Opt {
    Other(u32),
}

#[derive(Debug, Clone)]
struct OptData {
    opts: Vec<Opt>,
}

struct MessageParser<'a> {
    c: Cursor<&'a mut [u8]>,
    nr: nametree::NameReader,
}

impl MessageParser<'_> {
    fn new<'a>(data: &'a mut [u8]) -> MessageParser<'a> {
        MessageParser{
            c: Cursor::new(data),
            nr: nametree::NameReader::new(),
        }
    }

    fn parse_opt(&mut self, rdlen: u64) -> Result<(), std::io::Error> {
        if rdlen == 0 {
            return Ok(());
        }
        let mut data = Vec::<u8>::new();
        self.c.get_ref().take(rdlen).read_to_end(&mut data)?;
        let mut c = Cursor::new(data);
        loop {
            if c.position() == rdlen {
                break;
            }
            let code = c.read_u16::<BigEndian>()? as u32;
            let len = c.read_u16::<BigEndian>()? as u64;
            let data = Vec::<u8>::new();
            c.seek(SeekFrom::Current(len as i64))?;
        }
        return Ok(());
    }
    
    fn parse_name(&mut self) -> Result<String, std::io::Error> {
        let mut name: String = String::new();
        loop {
            let mut len = [0; 1];
            self.c.read_exact(&mut len)?;
            // check if it's a pointer
            if len[0] & 0xc0 != 0 { 
                let mut off = [0; 1];
                self.c.read_exact(&mut off)?;
                let off = (((len[0] & 0x3f) as u64) << 8) + (off[0] as u64);
                let cur = self.c.position();
                self.c.seek(SeekFrom::Start(off))?;
                name.push_str(&self.parse_name()?);
                self.c.seek(SeekFrom::Start(cur))?;
                return Ok(name);
            }
            let len = len[0] as u64;
            if len == 0 {
                return Ok(name);
            }
            let mut data = Vec::<u8>::new();
            self.c.get_ref().take(len).read_to_end(&mut data)?;
            name.push_str(&String::from_utf8_lossy(&data));
            name.push_str(".");
        }
    }
    
    fn parse_question(&mut self) -> Result<Question, std::io::Error> {
        let name = self.nr.read(&mut self.c)?;
        let typ = self.c.read_u16::<BigEndian>()? as u32;
        let class = self.c.read_u16::<BigEndian>()? as u32;
        return Ok(Question{
            name: name,
            typ: typ,
            class: class,
        });
    }
    fn parse_questions(&mut self, count: u32) -> Result<Vec<Question>, std::io::Error> {
        let mut qs = Vec::<Question>::new();
        for _ in 0..count {
            qs.push(self.parse_question()?);
        }
        return Ok(qs);
    }
    
    fn parse_ipv4(&mut self) -> Result<Ipv4Addr, std::io::Error> {
        let mut data: [u8; 4] = [0; 4];
        self.c.read_exact(&mut data)?;
        return Ok(Ipv4Addr::new(data[0], data[1], data[2], data[3]));
    }
    
    fn parse_cname(&mut self) -> Result<String, std::io::Error> {
        return Ok(self.nr.read(&mut self.c)?);
    }
    
    fn parse_ipv6(&mut self) -> Result<Ipv6Addr, std::io::Error> {
        let mut data: [u8; 16] = [0; 16];
        self.c.read_exact(&mut data)?;
        return Ok(Ipv6Addr::from(data));
    }
    
    fn parse_unknown(&mut self, typ: u32, len: u64) -> Result<u32, std::io::Error> {
        let mut data = Vec::<u8>::new();
        self.c.get_ref().take(len).read_to_end(&mut data)?;
        return Ok(typ);
    }
    
    fn parse_rdata(&mut self, typ: u32, len: u64) -> Result<ResourceData, std::io::Error> {
        return match typ {
            1 => Ok(ResourceData::IPv4(self.parse_ipv4()?)),
            5 => Ok(ResourceData::CName(self.parse_cname()?)),
            28 => Ok(ResourceData::IPv6(self.parse_ipv6()?)),
            41 => {self.parse_opt(len); Ok(ResourceData::Other(41))},
            _ => Ok(ResourceData::Other(self.parse_unknown(typ, len)?)),
        };
    }
    
    fn parse_resource(&mut self) -> Result<ResourceRecord, std::io::Error> {
        let name = self.nr.read(&mut self.c)?;
        let typ = self.c.read_u16::<BigEndian>()? as u32;
        let class = self.c.read_u16::<BigEndian>()? as u32;
        let ttl = self.c.read_u32::<BigEndian>()? as u32;
        let rdlen = self.c.read_u16::<BigEndian>()? as u64;
        return Ok(ResourceRecord{
            name: name,
            typ: typ,
            class: class,
            ttl: ttl,
            data: self.parse_rdata(typ, rdlen)?,
        });
    }
    
    fn parse_resources(&mut self, count: u32) -> Result<Vec<ResourceRecord>, std::io::Error> {
        let mut rs = Vec::<ResourceRecord>::new();
        for _ in 0..count {
            rs.push(self.parse_resource()?);
        }
        return Ok(rs);
    }
    
    fn parse(&mut self) -> Result<Message, std::io::Error> {
        let id = self.c.read_u16::<BigEndian>()? as u32;
        let mut flags = BitCursor::new(&mut self.c)?;
        let qr = flags.read(1)?;
        let opcode = flags.read(4)? as u32;
        let aa = flags.read(1)?;
        let tc = flags.read(1)?;
        let rd = flags.read(1)?;
        let mut flags = BitCursor::new(&mut self.c)?;
        let ra = flags.read(1)?;
        let _z = flags.read(1)?;
        let ad = flags.read(1)?;
        let cd = flags.read(1)?;
        let rcode = flags.read(4)? as u32;
        let qcount = self.c.read_u16::<BigEndian>()? as u32;
        let ancount = self.c.read_u16::<BigEndian>()? as u32;
        let nscount = self.c.read_u16::<BigEndian>()? as u32;
        let arcount = self.c.read_u16::<BigEndian>()? as u32;
        let questions = self.parse_questions(qcount)?;
        let answers = self.parse_resources(ancount)?;
        let nameservers = self.parse_resources(nscount)?;
        let additional = self.parse_resources(arcount)?;
        return Ok(Message{
            id: id,
            qr: qr,
            opcode: opcode,
            aa: aa,
            tc: tc,
            rd: rd,
            ra: ra,
            ad: ad,
            cd: cd,
            rcode: rcode,
            questions: questions,
            answers: answers,
            nameservers: nameservers,
            additional: additional,
        });
    }
}

struct MessageWriter<'a> {
    m: &'a Message,
    c: Cursor<&'a mut Vec<u8>>,
    nw: nametree::NameWriter,
}

impl<'a> MessageWriter<'_> {
    pub fn new(msg: &'a Message, data: &'a mut Vec<u8>) -> MessageWriter<'a> {
        MessageWriter {
            m: msg,
            c: Cursor::new(data),
            nw: nametree::NameWriter::new(),
        }
    }
    pub fn into_bytes(&mut self) -> Result<(), std::io::Error> {
        self.c.write_u16::<BigEndian>(self.m.id as u16);
        let mut flags = [0u8; 2];
        flags[0] = 
            (self.m.qr & 0b1) << 7 |
            (self.m.opcode as u8 & 0b1111) << 3 |
            (self.m.aa & 0b1) << 2 |
            (self.m.tc & 0b1) << 1 |
            (self.m.rd & 0b1) << 0;
        flags[1] = 
            (self.m.ra & 0b1) << 7 |
            (0 & 0b1) << 6 |
            (self.m.ad & 0b1) << 5 |
            (self.m.cd & 0b1) << 4 |
            (self.m.rcode as u8 & 0b111) << 0;
        self.c.write_all(&flags).expect("oops");
        self.c.write_u16::<BigEndian>(self.m.questions.len() as u16);
        self.c.write_u16::<BigEndian>(self.m.answers.len() as u16); // an
        self.c.write_u16::<BigEndian>(0u16); // ns
        self.c.write_u16::<BigEndian>(0u16); // ad
        //Self::write_query(&mut c, &self.questions);
        for q in &self.m.questions {
            self.nw.write(&mut self.c, &q.name)?;
            self.c.write_u16::<BigEndian>(q.typ as u16).expect("oops");
            self.c.write_u16::<BigEndian>(q.class as u16).expect("oops");
        }
        for a in &self.m.answers {
            self.nw.write(&mut self.c, &a.name)?;
            self.c.write_u16::<BigEndian>(a.typ as u16);
            self.c.write_u16::<BigEndian>(a.class as u16);
            self.c.write_u32::<BigEndian>(a.ttl);
            if let ResourceData::IPv4(addr) = a.data {
                self.c.write_u16::<BigEndian>(4 as u16);
                self.c.write_all(&addr.octets());
            } else {
                panic!("oops");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Message {
    id: u32,
    qr: u8,
    opcode: u32,
    aa: u8,
    tc: u8,
    rd: u8,
    ra: u8,
    ad: u8,
    cd: u8,
    rcode: u32,
    questions: Vec<Question>,
    answers: Vec<ResourceRecord>,
    nameservers: Vec<ResourceRecord>,
    additional: Vec<ResourceRecord>,
}

impl Message {
    
    pub fn from(data: &mut [u8]) -> Result<Message, std::io::Error> {
        MessageParser::new(data).parse()
    }

    fn write_something<T>(c: &mut std::io::Cursor<T>) -> Result<(), std::io::Error> where std::io::Cursor<T>: std::io::Write {
        let mut buf = [0u8; 16];
        c.write_all(&buf).expect("test");

        Ok(())
    }

    pub fn into_bytes(&mut self) -> Result<Vec::<u8>, std::io::Error> {
        let mut buffer = Vec::<u8>::new();
        MessageWriter::new(&self, &mut buffer).into_bytes()?;
        Ok(buffer)
    }

    pub fn new() -> Message {
        return Message{
            id: 0,
            qr: 0,
            opcode: 0,
            aa: 0,
            tc: 0,
            rd: 0,
            ra: 0,
            ad: 0,
            cd: 0,
            rcode: 0,
            questions: Vec::new(),
            answers: Vec::new(),
            nameservers: Vec::new(),
            additional: Vec::new(),
        };
    }
}

fn genid() -> u16 {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("oops");
    return ((buf[0] as u16) << 8) | (buf[1] as u16);
}

fn send_query_(name: &str) -> mio::net::UdpSocket {
    let socket = mio::net::UdpSocket::bind("0.0.0.0:0".parse().expect("oops")).expect("oops");
    socket.connect("9.9.9.9:53".parse().expect("oops")).expect("oops");
    let mut msg = Message::new();
    msg.id = genid() as u32;
    msg.qr = 0; // query
    msg.opcode = 0; // standard query
    msg.rd = 1; // recursive query
    msg.questions.push(Question{
        name: name.to_owned(),
        typ: 1, // A
        class: 1, // IN
    });
    let data = msg.into_bytes().expect("oops");
    socket.send(&data).expect("oops");
    socket
}

fn send_query(name: &str) -> Result<Message, std::io::Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").expect("oops");
    socket.connect((Ipv4Addr::new(9, 9, 9, 9), 53)).expect("oops");
    let mut msg = Message::new();
    msg.id = genid() as u32;
    msg.qr = 0; // query
    msg.opcode = 0; // standard query
    msg.rd = 1; // recursive query
    msg.questions.push(Question{
        name: name.to_owned(),
        typ: 1, // A
        class: 1, // IN
    });
    let data = msg.into_bytes().expect("oops");
    socket.send(&data).expect("oops");
    let mut buf = [0; 512];
    let amt = socket.recv(&mut buf).expect("ooops");

    let msg = Message::from(&mut buf[..amt]).expect("oops");
    return Ok(msg);
}

fn encode_reply(q: &Message, r: &Message) -> Result<Vec<u8>, std::io::Error> {
    let mut reply = Message::new();
    reply.id = q.id;
    reply.qr = 1; // reply
    reply.opcode = q.opcode;
    reply.aa = r.aa;
    reply.tc = r.tc;
    reply.rd = r.rd;
    reply.ra = r.ra;
    reply.ad = r.ad;
    reply.cd = r.cd;
    reply.rcode = r.rcode;
    for qs in &q.questions {
        reply.questions.push(qs.clone());
    }
    for ans in &r.answers {
        reply.answers.push(ans.clone());
    }
    return Ok(reply.into_bytes().expect("oops"));
}

fn create_response(q: &Message, a: &Ipv4Addr, ttl: u64) -> Message {
    let mut r = Message::new();
    r.id = q.id;
    r.qr = 1;
    r.opcode = q.opcode;
    r.aa = 1; // ?
    r.tc = 0;
    r.rd = q.rd;
    r.ra = 1;
    r.ad = 0;
    r.cd = 0;
    r.rcode = 0;
    assert!(q.questions.len() == 1);
    for qs in &q.questions {
	r.questions.push(qs.clone());
	r.answers.push(ResourceRecord{
	    name: qs.name.clone(),
	    typ: qs.typ,
	    ttl: ttl as u32,
	    class: qs.class,
	    data: ResourceData::IPv4(a.clone()),
	});
    }
    r
}

fn save_debug(packet: &[u8]) {
    let mut f = File::create("packet.dat").expect("oops");
    f.write_all(packet);
}

fn handle_conn(cache: &mut Cache, socket: &mut mio::net::UdpSocket) {
    let mut buf = [0; 512];
    let (amt, src) = socket.recv_from(&mut buf).expect("oops");
    
    let msg = Message::from(&mut buf[..amt]).expect("oops");
    if msg.questions.len() != 1 {
	panic!("Only 1 query supported!");
	return;
    }
    if msg.questions[0].typ != 1 {
	save_debug(&buf);
	panic!("Only type 1 questions supported!");
	return;
    }
    println!("Query for {:?}", msg.questions[0].name);
    let resp = if let Some((a, ttl)) = cache.get(&msg.questions[0].name) {
	println!("Found in cache");
	create_response(&msg, &a, ttl)
    } else {
	println!("Forwarding");
	let resp = send_query(&msg.questions[0].name).expect("oops");
	if resp.rcode != 0 {
	    panic!("Oops, todo!");
	}
	if let ResourceData::IPv4(a) = resp.answers[0].data {
	    cache.insert(&resp.answers[0].name, &a, resp.answers[0].ttl as u64);
	}
	resp
    };
    let data = encode_reply(&msg, &resp).expect("oops");
    socket.send_to(&data, src);    
}

struct PendingQuery {
    query: Message,
    socket: mio::net::UdpSocket,
    source: std::net::SocketAddr,
}

fn handle_conn_(cache: &mut Cache, socket: &mut mio::net::UdpSocket) -> PendingQuery {
    let mut buf = [0; 512];
    let (amt, src) = socket.recv_from(&mut buf).expect("oops");
    
    let msg = Message::from(&mut buf[..amt]).expect("oops");
    if msg.questions.len() != 1 {
	save_debug(&buf);
	panic!("Only 1 query supported!");
    }
    if msg.questions[0].typ != 1 {
	save_debug(&buf);
	panic!("Only type 1 questions supported!");
    }
    println!("Query for {:?}", msg.questions[0].name);
    let s = send_query_(&msg.questions[0].name);
    PendingQuery{
	query: msg,
	socket: s,
	source: src,
    }
}

fn handle_pending_query_(cache: &mut Cache, socket: &mut mio::net::UdpSocket, p: PendingQuery) {
    let mut buf = [0; 512];
    let amt = p.socket.recv(&mut buf).expect("ooops");

    let msg = Message::from(&mut buf[..amt]).expect("oops");

    let data = encode_reply(&p.query, &msg).expect("oops");
    socket.send_to(&data, p.source);
}

struct ObjA {
    name: String
}

struct ObjB {
    value: String
}

struct ObjC {
    just: String
}

impl ObjC {
    fn func(&mut self) {
	println!("c: just: {:?}", self.just);
    }
}

fn obja_func(a: ObjA) {
    println!("a: name: {:?}", a.name);
}

fn obja_mut_func(a: &mut ObjA) {
    println!("a: name: {:?}", a.name);
}

fn objb_func(b: ObjB) {
    println!("b: value: {:?}", b.value);
}

fn objb_mut_func(b: &mut ObjB) {
    println!("b: value: {:?}", b.value);
}

struct Callable<T> {
    obj: T,
    f: fn(&mut T),
}

trait Callback {
    fn cb(&mut self);
}

impl<T> Callback for Callable<T> {
    fn cb(&mut self) {
	(self.f)(&mut self.obj);
    }
}

fn test() {
    let mut cs = Vec::<Box<dyn Callback>>::new();
    let a = ObjA{name: "A".to_owned()};
    let b = ObjB{value: "B".to_owned()};
    let c = ObjC{just: "C".to_owned()};
    cs.push(Box::new(Callable{obj: a, f: obja_mut_func}));
    cs.push(Box::new(Callable{obj: b, f: objb_mut_func}));
    cs.push(Box::new(Callable{obj: c, f: ObjC::func}));
    for c in &mut cs {
	c.cb();
    }
}

struct Watcher<T> {
    o: T,
    cb: fn(&mut T),
}

trait Dispatcher {
    fn dispatch(&mut self);
}

impl<T> Dispatcher for Watcher<T> {
    fn dispatch(&mut self) {
	(self.cb)(&mut self.o);
    }
}

struct Loop {
    poll: mio::Poll,
    watchers: HashMap<mio::Token, Box<dyn Dispatcher>>,
}


impl Loop {
    fn new() -> Loop {
	Loop{
	    poll: mio::Poll::new().expect("oops"),
	    watchers: HashMap::new(),
	}
    }
    fn register<T: 'static, S>(&mut self, src: &mut S, interests: mio::Interest, obj: T, cb: fn(&mut T))
    where S: mio::event::Source + ?Sized {
	self.poll.registry().register(src, mio::Token(0), interests);
	self.watchers.insert(mio::Token(0), Box::new(Watcher{o: obj, cb: cb}));
    }

    fn fake_register<T: 'static>(&mut self, obj: T, cb: fn(&mut T))
    {
	self.watchers.insert(mio::Token(0), Box::new(Watcher{o: obj, cb: cb}));
    }
}

fn test2() {
    let mut l = Loop::new();

    let a = ObjA{name: "A".to_owned()};

    l.fake_register(a, obja_mut_func);
}

fn main() {
    test();
    test2();
    let mut i = 0;
    let mut cache = Cache::new();
    //let socket = UdpSocket::bind("0.0.0.0:3553").expect("oops");
    let mut poll = mio::Poll::new().expect("ooops");
    let mut server = mio::net::UdpSocket::bind("0.0.0.0:3553".parse().expect("oops")).expect("oops");
    let mut pendings: HashMap<mio::Token, PendingQuery> = HashMap::new();

    poll.registry().register(&mut server, mio::Token(0), mio::Interest::READABLE);
    

    let mut events = mio::Events::with_capacity(128);
    loop {
	poll.poll(&mut events, None).expect("ooops");
	for e in events.iter() {
	    match e.token() {
		mio::Token(0) => {
		    let mut p = handle_conn_(&mut cache, &mut server);
		    i += 1;
		    let t = mio::Token(i);
		    poll.registry().register(&mut p.socket, t, mio::Interest::READABLE);
		    pendings.insert(t, p);
		},
		mio::Token(x) => {
		    match pendings.remove(&mio::Token(x)) {
			Some(p) => handle_pending_query_(&mut cache, &mut server, p),
			None => panic!("oops"),
		    }
		},
		_ => unreachable!(),
	    }
	}
    }
}
