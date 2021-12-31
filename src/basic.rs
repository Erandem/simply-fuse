use crate::{FileAttributes, FileType, INode};

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;

pub type DirChildren = HashMap<OsString, INode>;
pub const ROOT_INODE: INode = INode(1);

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
pub struct INodeEntry<F> {
    parent: Option<INode>,
    kind: INodeKind<F>,
}

impl<F> INodeEntry<F> {
    pub fn new_directory(parent: INode, children: Option<DirChildren>) -> INodeEntry<F> {
        INodeEntry {
            parent: Some(parent),
            kind: INodeKind::Directory(Directory {
                children: children.unwrap_or_default(),
            }),
        }
    }

    pub fn kind(&self) -> &INodeKind<F> {
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
pub enum INodeKind<F> {
    Directory(Directory),
    File(F),
}

/// A generic INodeTable which allows indexing by paths and inodes
///
/// Maps `F` as a "File" type
#[derive(Debug)]
pub struct INodeTable<F> {
    map: HashMap<INode, INodeEntry<F>>,
    cur_ino: INode,
}

impl<F> INodeTable<F> {
    pub fn add_entry(&mut self, name: OsString, entry: INodeEntry<F>) -> INode {
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

    pub fn get<T: Into<INode>>(&self, ino: T) -> Option<&INodeEntry<F>> {
        self.map.get(&ino.into())
    }

    pub fn get_mut<T: Into<INode>>(&mut self, ino: T) -> Option<&mut INodeEntry<F>> {
        self.map.get_mut(&ino.into())
    }

    /// Looks up a path. Will function with or without a leading slash
    /// ```
    /// # use polyfuse_fs::basic::{ROOT_INODE, INodeTable, Directory, INodeEntry};
    /// let mut tbl = INodeTable::<()>::default();
    /// let test_dir_inode = tbl.add_entry("example directory".into(), INodeEntry::new_directory(ROOT_INODE,
    /// None));
    ///
    /// let root = tbl.lookup("/").unwrap();
    /// let test_dir = tbl.lookup("example directory").unwrap();
    ///
    /// assert_eq!(root.0, ROOT_INODE);
    /// assert_eq!(test_dir.0, test_dir_inode);
    /// ```
    pub fn lookup<T: AsRef<Path>>(&self, path: T) -> Option<(INode, &INodeEntry<F>)> {
        let mut parent_ino = ROOT_INODE;
        let mut parent = self.get(ROOT_INODE)?;

        for component in path.as_ref().components() {
            let path: &Path = component.as_ref();
            let path_str = path.to_string_lossy();

            match path_str.as_ref() {
                "/" if parent_ino == ROOT_INODE => continue, // path starts with "/"
                _ => {
                    parent_ino = *parent.as_dir()?.get(path.as_os_str())?;
                    parent = self.get(parent_ino)?;
                }
            }
        }

        let ino = parent_ino;
        let entry = parent;

        Some((ino, entry))
    }

    /// See `lookup` for details
    pub fn lookup_mut<T: AsRef<Path>>(&mut self, path: T) -> Option<(INode, &mut INodeEntry<F>)> {
        let inode = self.lookup(path).map(|x| x.0);

        inode
            .and_then(|ino| self.get_mut(ino))
            .map(|x| (inode.unwrap(), x))
    }

    fn next_open_inode(&mut self) -> INode {
        let ino = self.cur_ino;
        self.cur_ino = ino.next_inode();
        ino
    }
}

impl<F> Default for INodeTable<F> {
    fn default() -> INodeTable<F> {
        let mut h = HashMap::with_capacity(24);
        h.insert(
            ROOT_INODE,
            INodeEntry {
                parent: None,
                kind: INodeKind::Directory(Directory::default()),
            },
        );

        INodeTable {
            map: h,
            cur_ino: ROOT_INODE.next_inode(),
        }
    }
}
