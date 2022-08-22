    struct PartNode {
        part: String,
        index: u64,
        children: Vec<PartNode>,
    }

    pub struct NameTree {
        root: PartNode,
    }

    pub struct PartPos {
        part: String,
        pos: u64,
    }

    impl PartPos {
        pub fn new(part: &str, pos: u64) -> PartPos {
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
                    part: "".to_owned(),
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

        fn find_child<'a>(node: &'a PartNode, part: &str) -> Option<&'a PartNode> {
            for c in &node.children {
                if c.part == part {
                    return Some(c);
                }
            }
            None
        }

        fn search_recursive<'a>(node: &PartNode, parts: &'a[&str], mut cur: usize) -> Option<(&'a [&'a str], &'a [&'a str], u64)> {
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
        pub fn search<'a>(&self, parts: &'a [&'a str]) -> Option<(&'a [&'a str], &'a [&'a str], u64)> {
            assert!(parts.len() != 0);
            if let None = Self::find_child(&self.root, parts[parts.len() -1]) {
                return None;
            }
            Self::search_recursive(&self.root, parts, parts.len() - 1)
        }
    }


    struct NameWriter {
        tree: NameTree,
    }


    impl NameWriter {
        pub fn new() -> NameWriter {
            NameWriter{
                tree: NameTree::new(),
            }
        }

        pub fn write(&mut self, buf: &[u8], name: &str) -> Result<(), std::io::Error> {
            self.tree.insert(&[PartPos::new("test", 10), PartPos::new("com", 15)]);
            let parts: &[&str] = &name.split(".").filter(|p| *p != "").collect::<Vec<&str>>();
            let mut leftover: &[&str] = &[];
            let mut found: &[&str] = parts;
            let mut pointer: Option<u64> = None;
            if let Some((l, f, i)) = self.tree.search(&parts) {
                leftover = l;
                found = f;
                pointer = Some(i);
            }
            println!("Stavros - {:?}, {:?}, {:?}", leftover, found, pointer);
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        
        fn print_recursive(node: &PartNode, level: u32) {
            println!("Level -> {}, Node -> {}, Children -> {}", 
                level, node.part, node.children.len());
            for c in &node.children {
                print!("{} -> {}", c.part, c.index);
            }
            println!();
            for c in &node.children {
                print_recursive(c, level + 1);
            }
        }
        
        #[test]
        fn test_basic() {
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
            nw.write(&mut buf, "simple.test.com.");
        }
    }
