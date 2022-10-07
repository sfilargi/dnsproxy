use byteorder::*;
use std::io::Cursor;
use std::io::Write;
use std::io::Read;

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

    fn search_recursive<'a>(node: &PartNode, parts: &'a[&str], cur: usize) -> Option<(&'a [&'a str], &'a [&'a str], usize)> {
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
        let leftover: &[&str];
        let found: &[&str];
        let pointer: Option<usize>;
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
            c.write_u16::<BigEndian>(i as u16 | 0xc000 as u16).expect("oops");
        } else {
            c.write_u8(0 as u8).expect("oops");
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
