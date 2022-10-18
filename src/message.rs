use byteorder::*;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::io::{Error, ErrorKind};
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;

// Implemented Resource Data:

// 1 -> A
// 2 -> NS
// 5 -> CNAME
// 6 -> SOA
// 12 -> PTR
// 15 -> MX
// 16 -> TXT
// 28 -> AAAA 

use crate::nametree;
use crate::dns;

#[derive(Debug, Clone)]
pub struct Message {
    pub id: u32,
    pub qr: u8,
    pub opcode: u32,
    pub aa: u8,
    pub tc: u8,
    pub rd: u8,
    pub ra: u8,
    pub ad: u8,
    pub cd: u8,
    pub rcode: u32,
    pub questions: Vec<Question>,
    pub answers: Vec<dns::ResourceRecord>,
    pub nameservers: Vec<dns::ResourceRecord>,
    pub additional: Vec<dns::ResourceRecord>,
}

#[derive(Debug, Clone)]
pub struct Question {
    pub name: String,
    pub qtype: dns::RecordType,
    pub class: dns::RecordClass, 
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
    
    fn parse_question(&mut self) -> Result<Question, std::io::Error> {
        let name = self.nr.read(&mut self.c)?;
        let qtype = dns::RecordType::try_from(self.c.read_u16::<BigEndian>()?)?;
        let class = dns::RecordClass::try_from(self.c.read_u16::<BigEndian>()?)?;	
	println!("Name: {:?}, Type: {:?}, Class: {:?}", name, qtype, class);
        return Ok(Question{
            name,
            qtype,
            class,
        });
    }
    fn parse_questions(&mut self, count: u32) -> Result<Vec<Question>, std::io::Error> {
	println!("Parsing {:?} questions", count);
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

    fn parse_ns(&mut self) -> Result<String, std::io::Error> {
	return Ok(self.nr.read(&mut self.c)?);
    }
    
    fn parse_cname(&mut self) -> Result<String, std::io::Error> {
        return Ok(self.nr.read(&mut self.c)?);
    }

    fn parse_soa(&mut self) -> Result<dns::Soa, std::io::Error> {
	let mname = self.nr.read(&mut self.c)?;
	let rname = self.nr.read(&mut self.c)?;
	let serial = self.c.read_u32::<BigEndian>()?;
	let refresh = self.c.read_u32::<BigEndian>()?;
	let retry = self.c.read_u32::<BigEndian>()?;
	let expire = self.c.read_u32::<BigEndian>()?;
	let minimum = self.c.read_u32::<BigEndian>()?;
	Ok(dns::Soa{
	    mname,
	    rname,
	    serial,
	    refresh,
	    retry,
	    expire,
	    minimum,
	})
    }

    fn parse_ptr(&mut self) -> Result<String, std::io::Error> {
        Ok(self.nr.read(&mut self.c)?)
    }

    fn parse_mx(&mut self) -> Result<dns::Mx, std::io::Error> {
	let preference = self.c.read_u16::<BigEndian>()?;
	let exchange = self.nr.read(&mut self.c)?;
	Ok(dns::Mx{
	    preference,
	    exchange,
	})
    }

    fn parse_txt(&mut self) -> Result<String, std::io::Error> {
	let len = self.c.read_u8()?;
	let mut data = Vec::<u8>::new();
	std::io::Read::by_ref(&mut self.c).take(len as u64).read_to_end(&mut data)?;
	Ok(String::from_utf8_lossy(&data).to_string())
    }
    
    fn parse_ipv6(&mut self) -> Result<Ipv6Addr, std::io::Error> {
        let mut data: [u8; 16] = [0; 16];
        self.c.read_exact(&mut data)?;
        Ok(Ipv6Addr::from(data))
    }
    
    fn parse_unknown(&mut self, rtype: u16, len: u64) -> Result<u32, std::io::Error> {
        let mut data = Vec::<u8>::new();
        self.c.get_ref().take(len).read_to_end(&mut data)?;
        Ok(rtype.into())
    }
    
    fn parse_rdata(&mut self, rtype: dns::RecordType, len: u64) -> Result<dns::ResourceData, std::io::Error> {
        return match rtype {
            dns::RecordType::A => Ok(dns::ResourceData::IPv4(self.parse_ipv4()?)),
	    dns::RecordType::NS => Ok(dns::ResourceData::Ns(self.parse_ns()?)),
            dns::RecordType::CNAME => Ok(dns::ResourceData::CName(self.parse_cname()?)),
	    dns::RecordType::SOA => Ok(dns::ResourceData::Soa(self.parse_soa()?)),
	    dns::RecordType::PTR => Ok(dns::ResourceData::Ptr(self.parse_ptr()?)),
	    dns::RecordType::MX => Ok(dns::ResourceData::Mx(self.parse_mx()?)),
	    dns::RecordType::TXT => Ok(dns::ResourceData::Txt(self.parse_txt()?)),
            dns::RecordType::AAAA => Ok(dns::ResourceData::IPv6(self.parse_ipv6()?)),
            _ => Ok(dns::ResourceData::Unimplemented(self.parse_unknown(u16::from(rtype), len)?)),
        };
    }
    
    fn parse_resource(&mut self) -> Result<dns::ResourceRecord, std::io::Error> {
        let name = self.nr.read(&mut self.c)?;
        let rtype = dns::RecordType::try_from(self.c.read_u16::<BigEndian>()?)?;
	println!("Resource: Name: {:?}, Type: {:?}", name, rtype);
	let _ = self.c.read_u16::<BigEndian>()?; // OPT overloads this, and pisses me off.
        let class = dns::RecordClass::IN;
        let ttl = self.c.read_u32::<BigEndian>()? as u32;
        let rdlen = self.c.read_u16::<BigEndian>()? as u64;
	println!("Resource: Name: {:?}, Type: {:?}, Class: {:?}, TTL: {:?}, RDLEN: {:?}",
		 name, rtype, class, ttl, rdlen);
        return Ok(dns::ResourceRecord{
            name: name,
            rtype: rtype,
            class: class,
            ttl: ttl,
            data: self.parse_rdata(rtype, rdlen)?,
        });
    }
    
    fn parse_resources(&mut self, count: u32) -> Result<Vec<dns::ResourceRecord>, std::io::Error> {
        let mut rs = Vec::<dns::ResourceRecord>::new();
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

    pub fn write_a(&mut self, addr: &Ipv4Addr) -> Result<(), std::io::Error> {
	self.c.write_u16::<BigEndian>(4 as u16).expect("oops");
        self.c.write_all(&addr.octets()).expect("oops");
	Ok(())
    }

    pub fn write_ns(&mut self, name: &String) -> Result<(), std::io::Error> {
	let size = self.nw.size_of(&name);
	self.c.write_u16::<BigEndian>(size.try_into().unwrap())?;
	self.nw.write(&mut self.c, name)?;
	Ok(())
    }

    pub fn write_cname(&mut self, name: &String) -> Result<(), std::io::Error> {
	let size = self.nw.size_of(&name);
	self.c.write_u16::<BigEndian>(size.try_into().unwrap())?;
	self.nw.write(&mut self.c, name)?;
	Ok(())
    } 

    pub fn write_soa(&mut self, soa: &dns::Soa) -> Result<(), std::io::Error> {
	let len_pos = self.c.position();
	self.c.write_u16::<BigEndian>(0)?;
	let mut size = self.nw.size_of(&soa.mname);
	self.nw.write(&mut self.c, &soa.mname)?;
	size += self.nw.size_of(&soa.rname);
	self.nw.write(&mut self.c, &soa.rname)?;
	size += 20; // 20 bytes for 5 32 bits below
	self.c.write_u32::<BigEndian>(soa.serial)?;
	self.c.write_u32::<BigEndian>(soa.refresh)?;	
	self.c.write_u32::<BigEndian>(soa.retry)?;
	self.c.write_u32::<BigEndian>(soa.expire)?;
	self.c.write_u32::<BigEndian>(soa.minimum)?;
	// back write the size
	let end_pos = self.c.position();
	self.c.set_position(len_pos);
	self.c.write_u16::<BigEndian>(size.try_into().unwrap())?;
	self.c.set_position(end_pos);
	Ok(())
    }

    pub fn write_ptr(&mut self, name: &String) -> Result<(), std::io::Error> {
	let size = self.nw.size_of(name);
	self.c.write_u16::<BigEndian>(size.try_into().unwrap())?;
	self.nw.write(&mut self.c, name)?;
	Ok(())
    }

    pub fn write_mx(&mut self, mx: &dns::Mx) -> Result<(), std::io::Error> {
	let size = self.nw.size_of(&mx.exchange) + 2;
	self.c.write_u16::<BigEndian>(size.try_into().unwrap())?;
	self.c.write_u16::<BigEndian>(mx.preference)?;
	self.nw.write(&mut self.c, &mx.exchange)?;
	Ok(())
    }

    pub fn write_txt(&mut self, txt: &String) -> Result<(), std::io::Error> {
	let size = txt.len() + 1;
	self.c.write_u16::<BigEndian>(size.try_into().unwrap())?;
	self.c.write_u8(txt.len().try_into().unwrap())?;
	self.c.write_all(&txt.as_bytes())?;
	Ok(())
    }
    
    pub fn write_aaaa(&mut self, addr: &Ipv6Addr) -> Result<(), std::io::Error> {
	self.c.write_u16::<BigEndian>(16 as u16).expect("oops");
	self.c.write_all(&addr.octets()).expect("oops");
	Ok(())
    }
    
    pub fn into_bytes(&mut self) -> Result<(), std::io::Error> {
        self.c.write_u16::<BigEndian>(self.m.id as u16).expect("oops");
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
        self.c.write_u16::<BigEndian>(self.m.questions.len() as u16).expect("oops");
        self.c.write_u16::<BigEndian>(self.m.answers.len() as u16).expect("oops"); // an
        self.c.write_u16::<BigEndian>(0u16).expect("oops"); // ns
        self.c.write_u16::<BigEndian>(0u16).expect("oops"); // ad
        //Self::write_query(&mut c, &self.questions);
        for q in &self.m.questions {
            self.nw.write(&mut self.c, &q.name)?;
            self.c.write_u16::<BigEndian>(u16::from(q.qtype)).expect("oops");
            self.c.write_u16::<BigEndian>(u16::from(q.class)).expect("oops");
        }
        for a in &self.m.answers {
            self.nw.write(&mut self.c, &a.name)?;
            self.c.write_u16::<BigEndian>(u16::from(a.rtype)).expect("oops");
            self.c.write_u16::<BigEndian>(u16::from(a.class)).expect("oops");
            self.c.write_u32::<BigEndian>(a.ttl).expect("oops");
	    match &a.data {
		dns::ResourceData::IPv4(addr) => self.write_a(&addr)?,
		dns::ResourceData::Ns(name) => self.write_ns(&name)?,
		dns::ResourceData::CName(name) => self.write_cname(&name)?,
		dns::ResourceData::Soa(soa) => self.write_soa(&soa)?,
		dns::ResourceData::Ptr(name) => self.write_ptr(&name)?,
		dns::ResourceData::Mx(mx) => self.write_mx(&mx)?,
		dns::ResourceData::Txt(txt) => self.write_txt(&txt)?,
		dns::ResourceData::IPv6(addr) => self.write_aaaa(&addr)?,
		_ => panic!("oops"),
	    }
        }
        Ok(())
    }
}

impl Message {
    
    pub fn from(data: &mut [u8]) -> Result<Message, std::io::Error> {
        MessageParser::new(data).parse()
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
