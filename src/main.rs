use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Error, ErrorKind};
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::UdpSocket;

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

#[derive(Debug)]
struct Question {
	name: String,
	typ: u32,
	class: u32, 
}

#[derive(Debug)]
struct SoaData {
	mname: String,
	rname: String,
	serial: u32,
	refresh: u32,
	retry: u32,
    expire: u32,
	minimum: u32,
}

#[derive(Debug)]
enum ResourceData {
	IPv4(Ipv4Addr),
	IPv6(Ipv6Addr),
	CName(String),
	Soa(SoaData),
	Other(u32),
}

#[derive(Debug)]
struct ResourceRecord {
	name: String,
	typ: u32,
	class: u32,
	ttl: u32,
	data: ResourceData,
}

enum Opt {
	Other(u32),
}

struct OptData {
	opts: Vec<Opt>,
}

#[derive(Debug)]
struct Header {
	id: u32,
	qr: bool,
	opcode: u32,
	aa: bool,
	tc: bool,
	rd: bool,
	ra: bool,
	ad: bool,
    cd: bool,
    rcode: u32,
	questions: Vec<Question>,
    answers: Vec<ResourceRecord>,
    nameservers: Vec<ResourceRecord>,
	additional: Vec<ResourceRecord>,
}

impl Header {

	fn parse_opt<T>(c: &mut std::io::Cursor<T>, rdlen: u64) -> Result<(), std::io::Error> where T: AsRef<[u8]> {
		let mut data = Vec::<u8>::new();
		c.take(rdlen).read_to_end(&mut data).expect("ooops");
		let mut c = Cursor::new(data);
		println!("LEEEN: {}", rdlen);
		loop {
			if c.position() == rdlen {
				break;
			}
			let code = c.read_u16::<BigEndian>().expect("OOOOPS") as u32;
			let len = c.read_u16::<BigEndian>().expect("OOOOOOOOOOO") as u64;
			let data = Vec::<u8>::new();
			println!("XXXXXXXXXOptCode: {}", code);
			c.seek(SeekFrom::Current(len as i64))?;
			println!("position {}/{}", c.position(), rdlen);
		}
		return Ok(())
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
		println!("Rdata length: {} for type {}", rdlen, typ);
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

	pub fn from(data: &mut [u8]) -> Result<Header, std::io::Error> {
		let mut c = Cursor::new(data);
		let id = c.read_u16::<BigEndian>()? as u32;
		let mut flags = BitCursor::new(&mut c)?;
		let qr = flags.read(1)? != 0;
		let opcode = flags.read(4)? as u32;
		let aa = flags.read(1)? != 0;
		let tc = flags.read(1)? != 0;
		let rd = flags.read(1)? != 0;
		let mut flags = BitCursor::new(&mut c)?;
		let ra = flags.read(1)? != 0;
		let _z = flags.read(1)?;
		let ad = flags.read(1)? != 0;
		let cd = flags.read(1)? != 0;
		let rcode = flags.read(4)? as u32;
		let qcount = c.read_u16::<BigEndian>()? as u32;
		let ancount = c.read_u16::<BigEndian>()? as u32;
		let nscount = c.read_u16::<BigEndian>()? as u32;
		let arcount = c.read_u16::<BigEndian>()? as u32;
		let questions = Self::parse_questions(&mut c, qcount)?;
		let answers = Self::parse_resources(&mut c, ancount)?;
		let nameservers = Self::parse_resources(&mut c, nscount)?;
		let additional = Self::parse_resources(&mut c, arcount)?;
		return Ok(Header{
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

fn main() {
    let socket = UdpSocket::bind("0.0.0.0:3553").expect("oops");

    loop {
        let mut buf = [0; 512];
        let (amt, _src) = socket.recv_from(&mut buf).expect("oops");

		for i in 0..amt {
			if i % 8 == 0 && i != 0 {
				println!();
			}
			print!("{:02x} ", buf[i]);
		}
		println!();

		let hdr = Header::from(&mut buf[..amt]).expect("oops");
		println!("{:#?}", hdr);
    }
}
