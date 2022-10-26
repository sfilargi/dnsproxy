use byteorder::*;
use std::io::Cursor;
use std::io::Write;
use std::io::Read;

struct LabelNode {
    label: String,
    index: usize,
    children: Vec<LabelNode>,
}

pub struct NameTree {
    root: LabelNode,
}

pub struct LabelPos {
    label: String,
    pos: usize,
}

impl LabelPos {
    pub fn new(label: &str, pos: usize) -> LabelPos {
        LabelPos {
            label: label.to_owned(),
            pos,
        }
    }
}

impl NameTree {
    pub fn new() -> NameTree {
        NameTree{
            root: LabelNode{
                label: "__root__".to_owned(),
                index: 0,
                children: Vec::new(),
            }
        }
    }

    fn find_child_mut<'a>(parent: &'a mut LabelNode, label: &str) -> Option<&'a mut LabelNode> {
        for c in parent.children.iter_mut() {
            if c.label == label {
                return Some(c);
            }
        }
        return None;
    }

    fn insert_recursive(parent: &mut LabelNode, labels: &[LabelPos]) {
        if labels.len() == 0 {
	    // we don't add the root in the nametree
            return;
        }
        let ppos = &labels.last().expect("not possible");
        let c = match Self::find_child_mut(parent, &ppos.label) {
            None => {
                let c = LabelNode{
                    label: ppos.label.to_owned(),
                    index: ppos.pos,
                    children: Vec::new(),
                };
                parent.children.push(c);
                parent.children.last_mut().expect("not possible")
            },
            Some(c) => c,
        };
        Self::insert_recursive(c, &labels[..labels.len() - 1]);
    }

    fn find_recursive_<'a>(parent: &'a mut LabelNode, location: &[&str]) -> &'a mut LabelNode {
        if location.len() == 0 {
            return parent;
        }
        if let Some(c) = Self::find_child_mut(parent, location[location.len() - 1]) {
            return Self::find_recursive_(c, &location[..location.len() - 1]);
        } else {
            panic!("oops");
        }
    }

    pub fn insert_at(&mut self, labels: &[LabelPos], location: &[&str]) {
        let parent = Self::find_recursive_(&mut self.root, location);
        Self::insert_recursive(parent, labels);
    }

    fn find_child<'a>(node: &'a LabelNode, label: &str) -> Option<&'a LabelNode> {
        for c in &node.children {
            if c.label == label {
                return Some(c);
            }
        }
        None
    }

    fn search_recursive<'a>(node: &LabelNode, labels: &'a[&str], cur: usize) -> Option<(&'a [&'a str], &'a [&'a str], usize)> {
        match Self::find_child(node, labels[cur]) {
            Some(node) =>
                if cur > 0 {
                    Self::search_recursive(node, labels, cur - 1)
                } else {
                    Some((&[], labels, node.index))
                },
            None => Some((&labels[..cur + 1], &labels[cur + 1..], node.index))
        }
    }

    // Returns (leftover labels, found labels, index)
    pub fn search<'a>(&self, labels: &'a [&'a str]) -> Option<(&'a [&'a str], &'a [&'a str], usize)> {
        if labels.len() == 0 {
	    return None;
	}
        if let None = Self::find_child(&self.root, labels[labels.len() -1]) {
            return None;
        }
        Self::search_recursive(&self.root, labels, labels.len() - 1)
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

    fn search<'a>(&self, labels: &'a[&'a str]) -> (&'a [&'a str], &'a [&'a str], Option<usize>) {
        let leftover: &[&str];
        let found: &[&str];
        let pointer: Option<usize>;
        if let Some((l, f, i)) = self.tree.search(&labels) {
            leftover = l;
            found = f;
            pointer = Some(i);
        } else {
            leftover = labels;
            found = &[];
            pointer = None;
        }
        (leftover, found, pointer)
    }

    pub fn size_of(&mut self, name: &str) -> usize {
	// split the name into vector or labels
        let labels: &[&str] = &name.split(".").filter(|p| *p != "").collect::<Vec<&str>>();
        // search for the labels
        let (leftover, _, pointer) = self.search(labels);

	// The total size is 2 bytes for pointer, if there is one,
	// plus 1 byte + str len for each leftoever
	let mut size = 0;
	if let Some(_) = pointer {
	    size += 2;
	} else {
	    size += 1; // For the terminating NULL
	}
	for l in leftover {
	    size += 1;
	    size += l.len();
	}
	size
    }

    pub fn write<T>(&mut self, c: &mut Cursor<T>, name: &str) -> Result<(), std::io::Error> 
    where std::io::Cursor<T>: std::io::Write {
        // split the name into vector or labels
        let labels: &[&str] = &name.split(".").filter(|p| *p != "").collect::<Vec<&str>>();
        // search for the labels
        let (leftover, found, pointer) = self.search(labels);

        let mut additions: Vec<LabelPos> = Vec::new();     
        for l in leftover {
            let length: u8 = l.len().try_into().expect("ooops");
            let pos = c.position() as usize;
            c.write_u8(length).expect("ooops");
            c.write_all(&l.as_bytes()).expect("ooops");
            additions.push(LabelPos::new(l, pos));
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
    label: String,
    next: Option<usize>,
}

struct ReadTree {
    labels: Vec<Option<ReadNode>>,
}

impl ReadTree {
    pub fn new() -> ReadTree {
        ReadTree{
            labels: Vec::new(),
        }
    }
    pub fn insert(&mut self, labels: &[LabelPos], next: Option<usize>) {
        for i in 0..labels.len() {
            let p = &labels[i];
            if p.pos >= self.labels.len() {
                self.labels.resize(p.pos + 1, None);
            }
            let np = if i + 1 < labels.len() {
                Some(labels[i + 1].pos)
            } else {
                next
            };
            self.labels[p.pos] = Some(ReadNode{
                label: p.label.to_owned(),
                next: np,
            });
        }
    }
    pub fn load(&self, pos: usize) -> String {
        let mut labels = Vec::<&str>::new();
        let mut p = pos;
        loop {
            if let Some(n) = &self.labels[p] {
                labels.push(&n.label);
                if let Some(np) = n.next {
                    p = np;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        return labels.join(".") + ".";
    }
}

pub struct NameReader {
    tree: ReadTree
}

enum LabelOrPointer {
    Label(usize, String),
    Pointer(usize),
    End,
}

impl NameReader {
    pub fn new() -> NameReader {
        NameReader{
            tree: ReadTree::new(),
        }
    }

    fn read_label<T>(c: &mut Cursor<T>) -> Result<LabelOrPointer, std::io::Error>
    where std::io::Cursor<T>: std::io::BufRead {
        let pos = c.position() as usize;
        let len = c.read_u8()?;
        if len == 0x0 {
            return Ok(LabelOrPointer::End);
        } else if len & 0xc0 == 0xc0 {
            // it's pointer
            let len2 = c.read_u8()?;
            return Ok(LabelOrPointer::Pointer((len as usize & !0xc0) << 8 | len2 as usize));
        } else {
            let mut data = Vec::<u8>::new();
            c.take(len as u64).read_to_end(&mut data)?;
            return Ok(LabelOrPointer::Label(pos, String::from_utf8_lossy(&data).to_string()));
        }
    }

    pub fn read<T>(&mut self, c: &mut Cursor<T>) -> Result<String, std::io::Error>
    where std::io::Cursor<T>: std::io::BufRead {
        let mut labels = Vec::<LabelPos>::new();
        let mut next: Option<usize> = None;
        let start = c.position() as usize;
        loop {
            match Self::read_label(c)? {
                LabelOrPointer::Label(pos, label) => {
                    labels.push(LabelPos::new(&label, pos));
                },
                LabelOrPointer::Pointer(pos) => {
                    next = Some(pos);
                    break;
                },
                LabelOrPointer::End => {
                    break;
                },
            }
        }
        
        if labels.len() != 0 {
            self.tree.insert(&labels, next);
            return Ok(self.tree.load(start));
        } else if let Some(p) = next {
            return Ok(self.tree.load(p));
        } else {
            return Ok(".".to_owned());
        }
    }
}
