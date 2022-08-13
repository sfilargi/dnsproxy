use byteorder::{BigEndian, ReadBytesExt};
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
    fn new<T>(c: &mut std::io::Cursor<T>) -> BitCursor where T: AsRef<[u8]> {
	let mut value = [0; 1];
	c.read_exact(&mut value).expect("oops");
	println!("Flags: {:#04x}", value[0]);
	return BitCursor{
	    value: value[0],
	    cur: 8,
	};
    }

    fn read(&mut self, bits: usize) -> u8 {
	if bits > self.cur {
	    panic!("oops {}, {}", self.cur, bits);
	}
	let mask = (0x1 << bits) - 1;
	let result = (self.value >> (self.cur - bits)) & mask;
	self.cur -= bits;
	return result;
    }
}

fn read_name<T>(c: &mut std::io::Cursor<T>) where T: AsRef<[u8]> {
    loop {
	print!("<{}>", c.position());
	let mut len = [0; 1];
	c.read_exact(&mut len).expect("oops");
	if len[0] & 0xc0 != 0 {
	    let mut len2 = [0; 1];
	    c.read_exact(&mut len2).expect("oops");
	    let mut len = len[0] as u64;
	    len &= 0x3f;
	    len <<= 8;
	    len += len2[0] as u64;
	    println!("Pointer to {}", len);
	    let pos = c.position();
	    c.seek(SeekFrom::Start(len)).expect("oops");
	    read_name(c);
	    c.seek(SeekFrom::Start(pos)).expect("oops");
	    break;
	}
	let len = len[0] as u64;
	if len == 0 {
	    break;
	}
	//println!("Length: {}\n", len);
	let mut name = Vec::<u8>::new();
	c.take(len).read_to_end(&mut name).expect("oops");
	let name = String::from_utf8_lossy(&name);
	print!("{}.", name);
    }
    println!();
}

fn read_rdata<T>(c: &mut std::io::Cursor<T>) -> Vec::<u8> where T: AsRef<[u8]> {
    let rdlength = c.read_u16::<BigEndian>().expect("oops") as u64;
    println!("RDLENGTH: {}", rdlength);
    let mut rdata = Vec::<u8>::new();
    c.take(rdlength).read_to_end(&mut rdata).expect("oops");
    return rdata;
}

fn main() {
    let socket = UdpSocket::bind("127.0.0.1:3553").expect("ooops");

    loop {
        let mut buf = [0; 512];
        let (amt, _src) = socket.recv_from(&mut buf).expect("ooops");
	println!("+++++++++++++++++++");
	println!("+++++++++++++++++++");
	for i in 0..amt {
	    if i % 8 == 0 && i != 0 {
		println!();
	    }
	    print!("{:02x} ", buf[i]);
	}
	println!();

	let mut rdr = Cursor::new(buf);
	let id = rdr.read_u16::<BigEndian>().expect("oops");
	print!("ID: {}, ", id);
	let mut flags = BitCursor::new(&mut rdr);
	print!("QR: {}, ", flags.read(1));
	print!("OPCODE: {}, ", flags.read(4));
	print!("AA: {}, ", flags.read(1));
	print!("TC: {}, ", flags.read(1));
	print!("RD: {}, ", flags.read(1));
	let mut flags = BitCursor::new(&mut rdr);
	print!("RA: {}, ", flags.read(1));
	print!("Z: {}, ", flags.read(1));
	print!("AD: {}, ", flags.read(1));
	print!("CD: {}, ", flags.read(1));
	print!("RCODE: {}, ", flags.read(4));
	let qdcount = rdr.read_u16::<BigEndian>().expect("oops");
	print!("QDCOUNT: {}, ", qdcount);
	let ancount = rdr.read_u16::<BigEndian>().expect("oops");
	print!("ANCOUNT: {}, ", ancount);
	let nscount = rdr.read_u16::<BigEndian>().expect("oops");
	print!("NSCOUNT: {}, ", nscount);
	let arcount = rdr.read_u16::<BigEndian>().expect("oops");
	println!("ARCOUNT: {}, ", arcount);
	println!("-- Questions --");
	for _ in 0..qdcount {
	    read_name(&mut rdr);
	    let qtype = rdr.read_u16::<BigEndian>().expect("oops");
	    println!("QTYPE: {}", qtype);
	    let qclass = rdr.read_u16::<BigEndian>().expect("oops");
	    println!("QCLASS: {}", qclass);
	}
	println!("-- Responses --");
	for _ in 0..ancount{
	    read_name(&mut rdr);
	    let type_ = rdr.read_u16::<BigEndian>().expect("oops");
	    println!("TYPE: {}", type_);
	    let class = rdr.read_u16::<BigEndian>().expect("oops");
	    println!("CLASS: {}", class);
	    let ttl = rdr.read_u32::<BigEndian>().expect("oops");
	    println!("TTL: {}", ttl);
	    let rdlength = rdr.read_u16::<BigEndian>().expect("oops") as u64;
	    if type_ == 1 {
		let mut data: [u8; 4] = [0; 4];
		rdr.read_exact(&mut data).expect("oops");
		let addr = Ipv4Addr::new(data[0], data[1], data[2], data[3]);
		println!("IP: {}", addr);
	    } else if type_ == 2 {
		println!("NSDNAME");
		read_name(&mut rdr);
	    } else if type_ == 5 {
		println!("CNAME");
		read_name(&mut rdr);
	    } else if type_ == 6 {
		println!("SOA");
		read_name(&mut rdr);
		read_name(&mut rdr);
		print!("Serial: {}, ", rdr.read_u32::<BigEndian>().expect("oops"));
		print!("Refresh: {}, ", rdr.read_u32::<BigEndian>().expect("oops"));
		print!("Retry: {}, ", rdr.read_u32::<BigEndian>().expect("oops"));
		println!("Expire: {}", rdr.read_u32::<BigEndian>().expect("oops"));
	    } else if type_ == 28 {
		let mut data: [u8; 16] = [0; 16];
		rdr.read_exact(&mut data).expect("oops");
		let addr = Ipv6Addr::from(data);
		println!("IPv6: {}", addr);
	    } else if type_ == 65 {
		// ignore
	    } else {
		panic!("Unknown type {}", type_);
	    }
	}
	println!("-- Authority Records --");
	for _ in 0..nscount {
	    read_name(&mut rdr);
	    let type_ = rdr.read_u16::<BigEndian>().expect("oops");
	    println!("TYPE: {}", type_);
	    let class = rdr.read_u16::<BigEndian>().expect("oops");
	    println!("CLASS: {}", class);
	    let ttl = rdr.read_u32::<BigEndian>().expect("oops");
	    println!("TTL: {}", ttl);
	    let data = read_rdata(&mut rdr);
	    if type_ == 1 {
		let data:[u8;4] = data.try_into().expect("oops");
		let addr = Ipv4Addr::from(data);
		println!("IP: {}", addr);
	    } else if type_ == 5 {
		let cname = String::from_utf8_lossy(&data);
		println!("CNAME: {}", cname);
	    } else if type_ == 6 {
		let mut rdr = Cursor::new(data);
		println!("SOA");
		//read_name(&mut rdr);
		//read_name(&mut rdr);
		print!("Serial: {}, ", rdr.read_u32::<BigEndian>().expect("oops"));
		print!("Refresh: {}, ", rdr.read_u32::<BigEndian>().expect("oops"));
		print!("Retry: {}, ", rdr.read_u32::<BigEndian>().expect("oops"));
		println!("Expire: {}", rdr.read_u32::<BigEndian>().expect("oops"));
	    } else if type_ == 28 {
		let data:[u8;16] = data.try_into().expect("oops");
		let addr = Ipv6Addr::from(data);
		println!("IPv6: {}", addr);
	    } else if type_ == 65 {
		// ignore
	    } else {
		panic!("Unknown type {}", type_);
	    }
	}
    }
}
