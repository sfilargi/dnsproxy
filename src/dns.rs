use std::net::Ipv4Addr;
use std::net::Ipv6Addr;

#[derive(Debug, Clone, Copy)]
pub enum RecordType {
    A,
    NS,
    CNAME,
    SOA,
    PTR,
    MX,
    TXT,
    AAAA,
    OPT,
}

impl TryFrom<u16> for RecordType {
    type Error = std::io::Error;

    fn try_from(value: u16) -> Result<RecordType, Self::Error> {
	match value {
	    1  => Ok(RecordType::A),
	    2  => Ok(RecordType::NS),
	    5  => Ok(RecordType::CNAME),
	    6  => Ok(RecordType::SOA),
	    12 => Ok(RecordType::PTR),
	    15 => Ok(RecordType::MX),
	    16 => Ok(RecordType::TXT),
	    28 => Ok(RecordType::AAAA),
	    41 => Ok(RecordType::OPT),
	    _  => Err(std::io::Error::new(std::io::ErrorKind::Other, "Unsupported RecordType")),
	}
    }
}

impl From<RecordType> for u16 {
    fn from(rtype: RecordType) -> u16 {
	match rtype {
	    RecordType::A => 1,
	    RecordType::NS => 2,	
	    RecordType::CNAME => 5,
	    RecordType::SOA => 6,	
	    RecordType::PTR => 12,
	    RecordType::MX => 15,
	    RecordType::TXT => 16,
	    RecordType::AAAA => 28,
	    RecordType::OPT => 41,
	}
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RecordClass {
    IN,
}

impl TryFrom<u16> for RecordClass {
    type Error = std::io::Error;

    fn try_from(value: u16) -> Result<RecordClass, Self::Error> {
	match value {
	    1  => Ok(RecordClass::IN),
	    _  => Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Unsupported RecordClass {:?}", value))),
	}
    }
}

impl From<RecordClass> for u16 {
    fn from(class: RecordClass) -> u16 {
	match class {
	    RecordClass::IN => 1,
	}
    }
}

#[derive(Debug, Clone)]
pub struct ResourceRecord {
    pub name: String,
    pub rtype: RecordType,
    pub class: RecordClass,
    pub ttl: u32,
    pub data: ResourceData,
}

#[derive(Debug, Clone)]
pub enum ResourceData {
    IPv4(Ipv4Addr),
    IPv6(Ipv6Addr),
    CName(String),
    Ns(String),
    Soa(Soa),
    Ptr(String),
    Mx(Mx),
    Txt(String),
    Unimplemented(u32),
}

#[derive(Debug, Clone)]
pub struct Soa {
     pub mname: String,
     pub rname: String,
     pub serial: u32,
     pub refresh: u32,
     pub retry: u32,
     pub expire: u32,
     pub minimum: u32,
}

#[derive(Debug, Clone)]
pub struct Mx {
    pub preference: u16,
    pub exchange: String,
}
