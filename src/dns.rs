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
    HTTPS,
    UNKNOWN(u16),
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
	    65 => Ok(RecordType::HTTPS),
	    rt => Ok(RecordType::UNKNOWN(rt)),
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
	    RecordType::HTTPS => 65,
	    RecordType::UNKNOWN(rt) => rt,
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
    Ns(String),
    CName(String),
    Soa(Soa),
    Ptr(String),
    Mx(Mx),
    Txt(String),
    Https(Svcb),
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

#[derive(Debug, Clone, Copy)]
pub enum SvcbParamKey {
    ALPN,
    NODEFAULTALPN,
    PORT,
    IPV4HINT,
    ECHCONFIG,
    IPV6HINT,
    KEY(u16),
}

impl TryFrom<u16> for SvcbParamKey {
    type Error = std::io::Error;

    fn try_from(value: u16) -> Result<SvcbParamKey, Self::Error> {
	match value {
	    0 => panic!("oops"),
	    1 => Ok(SvcbParamKey::ALPN),
	    2 => Ok(SvcbParamKey::NODEFAULTALPN),
	    3 => Ok(SvcbParamKey::PORT),
	    4 => Ok(SvcbParamKey::IPV4HINT),
	    5 => Ok(SvcbParamKey::ECHCONFIG),
	    6 => Ok(SvcbParamKey::IPV6HINT),
	    n => Ok(SvcbParamKey::KEY(n)),
	}
    }
}

impl From<SvcbParamKey> for u16 {
    fn from(param: SvcbParamKey) -> u16 {
	match param {
	    SvcbParamKey::ALPN => 1,
	    SvcbParamKey::NODEFAULTALPN => 2,
	    SvcbParamKey::PORT => 3,
	    SvcbParamKey::IPV4HINT => 4,
	    SvcbParamKey::ECHCONFIG => 5,
	    SvcbParamKey::IPV6HINT => 6,
	    SvcbParamKey::KEY(n) => n,
	}
    }
}

#[derive(Debug, Clone)]
pub struct Svcb {
    pub domain_name: String,
    pub form: SvcbForm,
}

#[derive(Debug, Clone)]
pub enum SvcbForm {
    ALIASFORM,
    SERVICEFORM(SvcbServiceForm),
}

#[derive(Debug, Clone)]
pub struct SvcbServiceForm {
    pub field_priority: u16,
    pub params: Vec<SvcbParam>,
}

#[derive(Debug, Clone)]
pub struct SvcbParam {
    pub key: SvcbParamKey,
    pub value: Vec<u8>,
}

