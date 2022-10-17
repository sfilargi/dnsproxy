use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::time::{Instant, Duration};

#[derive(Debug)]
pub struct CacheEntry {
    a: Ipv4Addr,
    expiry: Instant,
}

impl CacheEntry {
    pub fn new(a: &Ipv4Addr, ttl: u64) -> CacheEntry {
	CacheEntry{
	    a: a.clone(),
	    expiry: Instant::now() + Duration::from_secs(ttl),
	}
    }
    pub fn get_ttl(&self) -> u64 {
	return (self.expiry - Instant::now()).as_secs();
    }
    pub fn is_valid(&self) -> bool {
	return self.expiry > Instant::now();
    }
}

#[derive(Debug)]
struct Cache {
    table: HashMap<String, CacheEntry>,
}

impl Cache {
    pub fn new() -> Cache {
	Cache{table: HashMap::new()}
    }

    pub fn insert(&mut self, name: &str, a: &Ipv4Addr, ttl: u64) {
	self.table.insert(name.to_lowercase().to_owned(),
			  CacheEntry::new(a, ttl));
    }

    fn get_(&mut self, name: &str) -> Option<&CacheEntry> {
	match self.table.get(&name.to_lowercase()) {
	    Some(entry) => Some(entry),
	    None => None,
	}
    }

    pub fn get(&mut self, name: &str) -> Option<(Ipv4Addr, u64)> {
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
