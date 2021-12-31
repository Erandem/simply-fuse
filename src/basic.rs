use crate::{FileAttributes, FileType, INode};

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};

pub type DirChildren = HashMap<OsString, INode>;

#[derive(Debug)]
pub struct File {}

#[derive(Debug, Default)]
pub struct Directory {
    children: DirChildren,
}

impl Directory {
    pub fn get(&self, name: &OsStr) -> Option<&INode> {
        self.children.get(name)
    }

    pub fn children(&self) -> DirIter<'_> {
        DirIter {
            iter: self.children.iter(),
        }
    }
}

pub struct DirIter<'a> {
    iter: std::collections::hash_map::Iter<'a, OsString, INode>,
}

impl<'a> Iterator for DirIter<'a> {
    type Item = (&'a OsString, INode);

    fn next(&mut self) -> Option<Self::Item> {
        // Deref since INode is cheap to copy
        self.iter.next().map(|(name, ino)| (name, *ino))
    }
}

#[derive(Debug)]
pub struct INodeEntry {
    parent: Option<INode>,
    kind: INodeKind,
}

impl INodeEntry {
    pub fn new_directory(parent: INode, children: Option<DirChildren>) -> INodeEntry {
        INodeEntry {
            parent: Some(parent),
            kind: INodeKind::Directory(Directory {
                children: children.unwrap_or_default(),
            }),
        }
    }

    pub fn kind(&self) -> &INodeKind {
        &self.kind
    }

    pub fn file_type(&self) -> FileType {
        match self.kind() {
            INodeKind::Directory(_) => FileType::Directory,
            INodeKind::File(_) => FileType::Regular,
        }
    }

    pub const fn parent(&self) -> Option<INode> {
        self.parent
    }

    pub fn as_dir(&self) -> Option<&Directory> {
        match self.kind() {
            INodeKind::Directory(ref dir) => Some(dir),
            _ => None,
        }
    }

    pub fn as_dir_mut(&mut self) -> Option<&mut Directory> {
        match &mut self.kind {
            INodeKind::Directory(dir) => Some(dir),
            _ => None,
        }
    }

    pub fn children(&self) -> Option<&DirChildren> {
        match self.kind() {
            INodeKind::Directory(dir) => Some(&dir.children),
            _ => None,
        }
    }

    pub fn getattr(&self) -> FileAttributes {
        match self.kind() {
            INodeKind::Directory(_) => FileAttributes::builder()
                .mode(libc::S_IFDIR | 0o755)
                .build(),
            _ => todo!("other getattr functions"),
        }
    }
}

#[derive(Debug)]
pub enum INodeKind {
    Directory(Directory),
    File(File),
}

#[derive(Debug)]
pub struct INodeTable {
    map: HashMap<INode, INodeEntry>,
    cur_ino: INode,
}

impl INodeTable {
    const ROOT: INode = INode(1);

    pub fn add_entry(&mut self, name: OsString, entry: INodeEntry) -> INode {
        let ino = self.next_open_inode();
        let parent = self
            .map
            .get_mut(&entry.parent.unwrap())
            .unwrap()
            .as_dir_mut()
            .unwrap();

        parent.children.insert(name, ino);
        self.map.insert(ino, entry);

        ino
    }

    pub fn get<T: Into<INode>>(&self, ino: T) -> Option<&INodeEntry> {
        self.map.get(&ino.into())
    }

    fn next_open_inode(&mut self) -> INode {
        let ino = self.cur_ino;
        self.cur_ino = ino.next_inode();
        ino
    }
}

impl Default for INodeTable {
    fn default() -> INodeTable {
        let mut h = HashMap::with_capacity(24);
        h.insert(
            Self::ROOT,
            INodeEntry {
                parent: None,
                kind: INodeKind::Directory(Directory::default()),
            },
        );

        INodeTable {
            map: h,
            cur_ino: Self::ROOT.next_inode(),
        }
    }
}
