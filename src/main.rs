use byteorder::*;
use log::{info, warn, error};
use std::io::{Error, ErrorKind};
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::UdpSocket;


//type PacketWriter<T> where std::io::Cursor<T>: std::io::Write = std::io::Cursor<T>;
//type ByteCursor<'a> = std::io::Cursor<&'a mtudyn AsRef<[u8]>>;

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
    
    fn parse_opt<T>(c: &mut std::io::Cursor<T>, rdlen: u64) -> Result<(), std::io::Error> where T: AsRef<[u8]> {
        if rdlen == 0 {
            return Ok(());
        }
        let mut data = Vec::<u8>::new();
        c.take(rdlen).read_to_end(&mut data)?;
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
    
    fn parse_name<T>(c: &mut std::io::Cursor<T>) -> Result<String, std::io::Error> where T: AsRef<[u8]> {
        let mut name: String = String::new();
        loop {
            let mut len = [0; 1];
            c.read_exact(&mut len)?;
            // check if it's a pointer
            if len[0] & 0xc0 != 0 { 
                let mut off = [0; 1];
                c.read_exact(&mut off)?;
                let off = (((len[0] & 0x3f) as u64) << 8) + (off[0] as u64);
                let cur = c.position();
                c.seek(SeekFrom::Start(off))?;
                name.push_str(&Self::parse_name(c)?);
                c.seek(SeekFrom::Start(cur))?;
                return Ok(name);
            }
            let len = len[0] as u64;
            if len == 0 {
                return Ok(name);
            }
            let mut data = Vec::<u8>::new();
            c.take(len).read_to_end(&mut data)?;
            name.push_str(&String::from_utf8_lossy(&data));
            name.push_str(".");
        }
    }
    
    fn parse_question<T>(c: &mut std::io::Cursor<T>) -> Result<Question, std::io::Error> where T: AsRef<[u8]> {
        let name = Self::parse_name(c)?;
        let typ = c.read_u16::<BigEndian>()? as u32;
        let class = c.read_u16::<BigEndian>()? as u32;
        return Ok(Question{
            name: name,
            typ: typ,
            class: class,
        });
    }
    fn parse_questions<T>(c: &mut std::io::Cursor<T>, count: u32) -> Result<Vec<Question>, std::io::Error> where T: AsRef<[u8]> {
        let mut qs = Vec::<Question>::new();
        for _ in 0..count {
            qs.push(Self::parse_question(c)?);
        }
        return Ok(qs);
    }
    
    fn parse_ipv4<T>(c: &mut std::io::Cursor<T>) -> Result<Ipv4Addr, std::io::Error> where T: AsRef<[u8]> {
        let mut data: [u8; 4] = [0; 4];
        c.read_exact(&mut data)?;
        return Ok(Ipv4Addr::new(data[0], data[1], data[2], data[3]));
    }
    
    fn parse_cname<T>(c: &mut std::io::Cursor<T>) -> Result<String, std::io::Error> where T: AsRef<[u8]> {
        return Ok(Self::parse_name(c)?);
    }
    
    fn parse_ipv6<T>(c: &mut std::io::Cursor<T>) -> Result<Ipv6Addr, std::io::Error> where T: AsRef<[u8]> {
        let mut data: [u8; 16] = [0; 16];
        c.read_exact(&mut data)?;
        return Ok(Ipv6Addr::from(data));
    }
    
    fn parse_unknown<T>(c: &mut std::io::Cursor<T>, typ: u32, len: u64) -> Result<u32, std::io::Error> where T: AsRef<[u8]> {
        let mut data = Vec::<u8>::new();
        c.take(len).read_to_end(&mut data)?;
        return Ok(typ);
    }
    
    fn parse_rdata<T>(c: &mut std::io::Cursor<T>, typ: u32, len: u64) -> Result<ResourceData, std::io::Error> where T: AsRef<[u8]> {
        return match typ {
            1 => Ok(ResourceData::IPv4(Self::parse_ipv4(c)?)),
            5 => Ok(ResourceData::CName(Self::parse_cname(c)?)),
            28 => Ok(ResourceData::IPv6(Self::parse_ipv6(c)?)),
            41 => {Self::parse_opt(c, len); Ok(ResourceData::Other(41))},
            _ => Ok(ResourceData::Other(Self::parse_unknown(c, typ, len)?)),
        };
    }
    
    fn parse_resource<T>(c: &mut std::io::Cursor<T>) -> Result<ResourceRecord, std::io::Error> where T: AsRef<[u8]> {
        let name = Self::parse_name(c)?;
        let typ = c.read_u16::<BigEndian>()? as u32;
        let class = c.read_u16::<BigEndian>()? as u32;
        let ttl = c.read_u32::<BigEndian>()? as u32;
        let rdlen = c.read_u16::<BigEndian>()? as u64;
        return Ok(ResourceRecord{
            name: name,
            typ: typ,
            class: class,
            ttl: ttl,
            data: Self::parse_rdata(c, typ, rdlen)?,
        });
    }
    
    fn parse_resources<T>(c: &mut std::io::Cursor<T>, count: u32) -> Result<Vec<ResourceRecord>, std::io::Error> where T: AsRef<[u8]> {
        let mut rs = Vec::<ResourceRecord>::new();
        for _ in 0..count {
            rs.push(Self::parse_resource(c)?);
        }
        return Ok(rs);
    }
    
    pub fn from(data: &mut [u8]) -> Result<Message, std::io::Error> {
        let mut c = Cursor::new(data);
        let id = c.read_u16::<BigEndian>()? as u32;
        let mut flags = BitCursor::new(&mut c)?;
        let qr = flags.read(1)?;
        let opcode = flags.read(4)? as u32;
        let aa = flags.read(1)?;
        let tc = flags.read(1)?;
        let rd = flags.read(1)?;
        let mut flags = BitCursor::new(&mut c)?;
        let ra = flags.read(1)?;
        let _z = flags.read(1)?;
        let ad = flags.read(1)?;
        let cd = flags.read(1)?;
        let rcode = flags.read(4)? as u32;
        let qcount = c.read_u16::<BigEndian>()? as u32;
        let ancount = c.read_u16::<BigEndian>()? as u32;
        let nscount = c.read_u16::<BigEndian>()? as u32;
        let arcount = c.read_u16::<BigEndian>()? as u32;
        let questions = Self::parse_questions(&mut c, qcount)?;
        let answers = Self::parse_resources(&mut c, ancount)?;
        let nameservers = Self::parse_resources(&mut c, nscount)?;
        let additional = Self::parse_resources(&mut c, arcount)?;
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


    fn write_something<T>(c: &mut std::io::Cursor<T>) -> Result<(), std::io::Error> where std::io::Cursor<T>: std::io::Write {
        let mut buf = [0u8; 16];
        c.write_all(&buf).expect("test");

        return Ok(());
    }

    pub fn into_bytes(&mut self) -> Result<Vec::<u8>, std::io::Error> {
        let mut data = Vec::<u8>::new();

        let mut c = Cursor::new(&mut data);

        c.write_u16::<BigEndian>(self.id as u16);
        let mut flags = [0u8; 2];
        flags[0] = 
            (self.qr & 0b1) << 7 |
            (self.opcode as u8 & 0b1111) << 3 |
            (self.aa & 0b1) << 2 |
            (self.tc & 0b1) << 1 |
            (self.rd & 0b1) << 0;
        flags[1] = 
            (self.ra & 0b1) << 7 |
            (0 & 0b1) << 6 |
            (self.ad & 0b1) << 5 |
            (self.cd & 0b1) << 4 |
            (self.rcode as u8 & 0b111) << 0;
        c.write_all(&flags).expect("oops");
        c.write_u16::<BigEndian>(self.questions.len() as u16);
        c.write_u16::<BigEndian>(self.answers.len() as u16); // an
        c.write_u16::<BigEndian>(0u16); // ns
        c.write_u16::<BigEndian>(0u16); // ad
        //Self::write_query(&mut c, &self.questions);
        for q in &self.questions {
            for part in q.name.split(".") {
                c.write_u8(part.len() as u8);
                c.write_all(&part.as_bytes());
            }
            c.write_u16::<BigEndian>(q.typ as u16).expect("oops");
            c.write_u16::<BigEndian>(q.class as u16).expect("oops");
        }
        for a in &self.answers {
            for part in a.name.split(".") {
                c.write_u8(part.len() as u8);
                c.write_all(&part.as_bytes());
            }
            c.write_u16::<BigEndian>(a.typ as u16);
            c.write_u16::<BigEndian>(a.class as u16);
            c.write_u32::<BigEndian>(a.ttl);
            if let ResourceData::IPv4(addr) = a.data {
                c.write_u16::<BigEndian>(4 as u16);
                c.write_all(&addr.octets());
            } else {
                panic!("oops");
            }
        }
        return Ok(data);
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
    for i in 0..data.len() {
        print!("{:02x} ", data[i]);
    }
    println!();
    println!("Sending!");
    socket.send(&data).expect("oops");
    let mut buf = [0; 512];
    let amt = socket.recv(&mut buf).expect("ooops");
    println!("hmmmm");
    for i in 0..amt {
        print!("{:02x} ", buf[i]);
    }
    println!();

    let msg = Message::from(&mut buf[..amt]).expect("oops");
    println!("{:#?}", msg);
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

fn main() {
    let socket = UdpSocket::bind("0.0.0.0:3553").expect("oops");
    
    loop {
        let mut buf = [0; 512];
        let (amt, src) = socket.recv_from(&mut buf).expect("oops");
        
        for i in 0..amt {
            print!("{:02x} ", buf[i]);
        }
        println!();

        let msg = Message::from(&mut buf[..amt]).expect("oops");
        if msg.questions.len() != 1 {
            error!("Only 1 query supported!");
            continue;
        }
        if msg.questions[0].typ != 1 {
            error!("Only type 1 questions supported!");
            continue;
        }
        println!("Quering for {}", msg.questions[0].name);
        println!("ID: {}", genid());
        let resp = send_query(&msg.questions[0].name).expect("oops");
        let data = encode_reply(&msg, &resp).expect("oops");
        socket.send_to(&data, src);
    }
}
