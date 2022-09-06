use byteorder::*;
use log::{info, warn, error};
use std::io::{Error, ErrorKind};
use std::io::Cursor;
use std::io::Write;
use std::io::Read;
use std::io::BufRead;

struct PartNode {
    part: String,
    index: usize,
    children: Vec<PartNode>,
}

pub struct NameTree {
    root: PartNode,
}

pub struct PartPos {
    part: String,
    pos: usize,
}

impl PartPos {
    pub fn new(part: &str, pos: usize) -> PartPos {
        PartPos {
            part: part.to_owned(),
            pos,
        }
    }
}

impl NameTree {
    pub fn new() -> NameTree {
        NameTree{
            root: PartNode{
                part: "__root__".to_owned(),
                index: 0,
                children: Vec::new(),
            }
        }
    }

    fn find_child_mut<'a>(parent: &'a mut PartNode, part: &str) -> Option<&'a mut PartNode> {
        for c in parent.children.iter_mut() {
            if c.part == part {
                return Some(c);
            }
        }
        return None;
    }

    fn insert_recursive(parent: &mut PartNode, parts: &[PartPos]) {
        if parts.len() == 0 {
            return;
        }
        let ppos = &parts.last().expect("not possible");
        let c = match Self::find_child_mut(parent, &ppos.part) {
            None => {
                let c = PartNode{
                    part: ppos.part.to_owned(),
                    index: ppos.pos,
                    children: Vec::new(),
                };
                parent.children.push(c);
                parent.children.last_mut().expect("not possible")
            },
            Some(c) => c,
        };
        Self::insert_recursive(c, &parts[..parts.len() - 1]);
    }

    pub fn insert(&mut self, parts: &[PartPos]) {
        Self::insert_recursive(&mut self.root, &parts);
    }

    fn find_recursive_<'a>(parent: &'a mut PartNode, location: &[&str]) -> &'a mut PartNode {
        if location.len() == 0 {
            return parent;
        }
        if let Some(c) = Self::find_child_mut(parent, location[location.len() - 1]) {
            return Self::find_recursive_(c, &location[..location.len() - 1]);
        } else {
            panic!("oops");
        }
    }

    pub fn insert_at(&mut self, parts: &[PartPos], location: &[&str]) {
        let parent = Self::find_recursive_(&mut self.root, location);
        Self::insert_recursive(parent, parts);
    }

    fn find_child<'a>(node: &'a PartNode, part: &str) -> Option<&'a PartNode> {
        for c in &node.children {
            if c.part == part {
                return Some(c);
            }
        }
        None
    }

    fn search_recursive<'a>(node: &PartNode, parts: &'a[&str], mut cur: usize) -> Option<(&'a [&'a str], &'a [&'a str], usize)> {
        match Self::find_child(node, parts[cur]) {
            Some(node) =>
                if cur > 0 {
                    Self::search_recursive(node, parts, cur - 1)
                } else {
                    Some((&[], parts, node.index))
                },
            None => Some((&parts[..cur + 1], &parts[cur + 1..], node.index))
        }
    }

    // Returns (leftover parts, found parts, index)
    pub fn search<'a>(&self, parts: &'a [&'a str]) -> Option<(&'a [&'a str], &'a [&'a str], usize)> {
        assert!(parts.len() != 0);
        if let None = Self::find_child(&self.root, parts[parts.len() -1]) {
            return None;
        }
        Self::search_recursive(&self.root, parts, parts.len() - 1)
    }
}

pub struct NameWriter {
    tree: NameTree,
}

impl NameWriter {
    pub fn new() -> NameWriter {
        NameWriter{
            tree: NameTree::new(),
        }
    }

    fn search<'a>(&self, parts: &'a[&'a str]) -> (&'a [&'a str], &'a [&'a str], Option<usize>) {
        let mut leftover: &[&str];
        let mut found: &[&str];
        let mut pointer: Option<usize>;
        if let Some((l, f, i)) = self.tree.search(&parts) {
            leftover = l;
            found = f;
            pointer = Some(i);
        } else {
            leftover = parts;
            found = &[];
            pointer = None;
        }
        (leftover, found, pointer)
    }

    pub fn write<T>(&mut self, c: &mut Cursor<T>, name: &str) -> Result<(), std::io::Error> 
    where std::io::Cursor<T>: std::io::Write {
        // split the name into vector or parts
        let parts: &[&str] = &name.split(".").filter(|p| *p != "").collect::<Vec<&str>>();
        // search for the parts
        let (leftover, found, pointer) = self.search(parts);

        let mut additions: Vec<PartPos> = Vec::new();     
        for l in leftover {
            let length: u8 = l.len().try_into().expect("ooops");
            let pos = c.position() as usize;
            c.write_u8(length).expect("ooops");
            c.write_all(&l.as_bytes()).expect("ooops");
            additions.push(PartPos::new(l, pos));
        }
        if let Some(i) = pointer {
            c.write_u16::<BigEndian>(i as u16 | 0xc000 as u16);
        } else {
            c.write_u8(0 as u8);
        }
        if additions.len() > 0 {
            self.tree.insert_at(&additions, found);
        }
        Ok(())
    }
}

#[derive(Debug,Clone)]
struct ReadNode {
    part: String,
    next: Option<usize>,
}

struct ReadTree {
    parts: Vec<Option<ReadNode>>,
}

impl ReadTree {
    pub fn new() -> ReadTree {
        ReadTree{
            parts: Vec::new(),
        }
    }
    pub fn insert(&mut self, parts: &[PartPos], next: Option<usize>) {
        for i in 0..parts.len() {
            let p = &parts[i];
            if p.pos >= self.parts.len() {
                self.parts.resize(p.pos + 1, None);
            }
            let np = if i + 1 < parts.len() {
                Some(parts[i + 1].pos)
            } else {
                next
            };
            self.parts[p.pos] = Some(ReadNode{
                part: p.part.to_owned(),
                next: np,
            });
        }
    }
    pub fn load(&self, pos: usize) -> String {
        let mut parts = Vec::<&str>::new();
        let mut p = pos;
        loop {
            if let Some(n) = &self.parts[p] {
                parts.push(&n.part);
                if let Some(np) = n.next {
                    p = np;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        return parts.join(".") + ".";
    }
}

pub struct NameReader {
    tree: ReadTree
}

enum PartOrPointer {
    Part(usize, String),
    Pointer(usize),
    End,
}

impl NameReader {
    pub fn new() -> NameReader {
        NameReader{
            tree: ReadTree::new(),
        }
    }

    fn read_part<T>(c: &mut Cursor<T>) -> Result<PartOrPointer, std::io::Error>
    where std::io::Cursor<T>: std::io::BufRead {
        let pos = c.position() as usize;
        let len = c.read_u8()?;
        if len == 0x0 {
            return Ok(PartOrPointer::End);
        } else if len & 0xc0 == 0xc0 {
            // it's pointer
            let len2 = c.read_u8()?;
            return Ok(PartOrPointer::Pointer((len as usize & !0xc0) << 8 | len2 as usize));
        } else {
            let mut data = Vec::<u8>::new();
            c.take(len as u64).read_to_end(&mut data)?;
            return Ok(PartOrPointer::Part(pos, String::from_utf8_lossy(&data).to_string()));
        }
    }

    pub fn read<T>(&mut self, c: &mut Cursor<T>) -> Result<String, std::io::Error>
    where std::io::Cursor<T>: std::io::BufRead {
        let mut parts = Vec::<PartPos>::new();
        let mut next: Option<usize> = None;
        let start = c.position() as usize;
        loop {
            match Self::read_part(c)? {
                PartOrPointer::Part(pos, part) => {
                    parts.push(PartPos::new(&part, pos));
                },
                PartOrPointer::Pointer(pos) => {
                    next = Some(pos);
                    break;
                },
                PartOrPointer::End => {
                    break;
                },
            }
        }
        
        if parts.len() != 0 {
            self.tree.insert(&parts, next);
            return Ok(self.tree.load(start));
        } else if let Some(p) = next {
            return Ok(self.tree.load(p));
        } else {
            return Ok(".".to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_reader() {
        let mut rt = ReadTree{
            parts: Vec::new(),
        };
        rt.parts.push(Some(ReadNode{part: "test".to_owned(), next: None}));
        let x = 10;
        if x >= rt.parts.len() {
            rt.parts.resize(x + 1, None);
        }
        rt.parts[10] = None;
    }

    #[test]
    fn test_reader2() {
        let mut rt = ReadTree::new();
        rt.insert(&[PartPos::new("a", 0), PartPos::new("b", 2)], None);
    }

    #[test]
    fn test_writer3() {
        let mut buf = vec![
            0x6, 's' as u8, 'i' as u8, 'm' as u8, 'p' as u8, 'l' as u8, 'e' as u8, 
            0x4, 't' as u8, 'e' as u8, 's' as u8, 't' as u8,
            0x3, 'c' as u8, 'o' as u8, 'm' as u8, 0x0, 
            0x7, 'e' as u8, 'x' as u8, 'a' as u8, 'm' as u8, 'p' as u8, 'l' as u8, 'e' as u8, 0xc0, 0xc, 
            0x5, 'e' as u8, 'x' as u8, 't' as u8, 'r' as u8, 'a' as u8, 0xc0, 0x7];
        let mut c = Cursor::new(&mut buf);
        let mut nr = NameReader::new();
        assert!(matches!(nr.read(&mut c), Ok(x) if x == "simple.test.com."));
        assert!(matches!(nr.read(&mut c), Ok(x) if x == "example.com."));
        assert!(matches!(nr.read(&mut c), Ok(x) if x == "extra.test.com."));
    }

    #[test]
    fn test_tree() {
        let mut nt = NameTree::new();
        nt.insert(&vec![PartPos::new("test", 9), PartPos::new("net", 14)]);
        nt.insert(&vec![PartPos::new("test", 10), PartPos::new("com", 15)]);
        nt.insert(&vec![PartPos::new("example", 1), PartPos::new("test", 9), 
            PartPos::new("com", 11)]);
        assert!(nt.search(&["ok", "test", "com"]) == Some((&["ok"], &["test", "com"], 10)));
        assert!(nt.search(&["test", "com"]) == Some((&[], &["test", "com"], 10)));
        assert!(nt.search(&["com"]) == Some((&[], &["com"], 15)));
    }

    #[test]
    fn test_writer() {
        let mut nw = NameWriter::new();
        let mut buf = Vec::<u8>::new();
        let mut c = Cursor::new(&mut buf);
        nw.write(&mut c, "simple.test.com.");
        nw.write(&mut c, "example.com.");
        nw.write(&mut c, "extra.test.com.");
        let expected = vec![
            0x6, 's' as u8, 'i' as u8, 'm' as u8, 'p' as u8, 'l' as u8, 'e' as u8, 
            0x4, 't' as u8, 'e' as u8, 's' as u8, 't' as u8,
            0x3, 'c' as u8, 'o' as u8, 'm' as u8, 0x0, 
            0x7, 'e' as u8, 'x' as u8, 'a' as u8, 'm' as u8, 'p' as u8, 'l' as u8, 'e' as u8, 0xc0, 0xc, 
            0x5, 'e' as u8, 'x' as u8, 't' as u8, 'r' as u8, 'a' as u8, 0xc0, 0x7];
        assert!(expected == buf);
    }
}
