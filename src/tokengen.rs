use std::mem;

struct TokenGen {
    bitmap: Vec<u64>,
}

impl TokenGen {
    pub fn new() -> TokenGen {
	TokenGen{
	    bitmap: Vec::new(),
	}
    }

    pub fn acquire(&mut self) -> u32 {
	for (i, v) in self.bitmap.iter_mut().enumerate() {
	    if *v != u64::MAX {
		let next = v.trailing_ones();
		*v |= 1 << next;
		return next + (i as u32 * 8 * mem::size_of::<u64>() as u32);
	    }
	}
	let nv: u64 = 0x1;
	let i = (self.bitmap.len() as u32 * 8 * mem::size_of::<u64>() as u32); 
	self.bitmap.push(nv);
	return i as u32;
    }

    pub fn release(&mut self, token: u32) {
	let i = (token / (8 * mem::size_of::<u64>() as u32)) as usize;
	let o = token % (8 * mem::size_of::<u64>() as u32);
	let v = self.bitmap[i];
	self.bitmap[i] = v & (!((0x1 as u64) << o) as u64);
	loop {
	    if self.bitmap.len() > 0 && self.bitmap[self.bitmap.len() - 1] == 0 {
		self.bitmap.pop();
	    } else {
		break;
	    }
	}
    }

    pub fn len(&self) -> usize {
	self.bitmap.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
	let mut tg = TokenGen::new();
	for j in 0..10 {
	    for i in 0..64 {
		assert!(tg.acquire() == (j * 64) + i);
	    }
	    assert!(tg.len() == (j + 1) as usize);
	}
	for j in (0..10).rev() {
	    for i in 0..64 {
		tg.release((j * 64) + i);
	    }
	    assert!(tg.len() == j as usize);
	}
    }
}
