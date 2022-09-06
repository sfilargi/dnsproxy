use std::mem;

#[cfg(test)]
mod tests {
    use super::*;

    fn get_token(x: &mut u64) -> Option<u32> {
	if *x == u64::MAX {
	    None
	} else {
	    let next = x.trailing_ones();
	    *x |= 1 << next;
	    Some(next)
	}
    }

    #[test]
    fn test_test() {
	let mut a: u64 = 0;
	for i in 0..64 {
	    assert!(matches!(get_token(&mut a), Some(t) if t == i));
	}
	assert!(matches!(get_token(&mut a), None));
    }
}
