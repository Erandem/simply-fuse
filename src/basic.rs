use crate::{FileAttributes, FileType, INode, SetFileAttributes};

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;

pub type DirChildren = HashMap<OsString, INode>;
pub const ROOT_INODE: INode = INode(1);

pub trait Attributable {
    fn getattrs(&self) -> FileAttributes;
}

/// Represents an object that acts like a file on the filesystem
pub trait Filelike: Attributable {}

#[derive(Debug)]
pub struct Directory {
    children: DirChildren,
    attrs: FileAttributes,
}

impl Directory {
    pub fn apply_attrs(&mut self, attrs: SetFileAttributes) {
        self.attrs.apply_attrs(attrs)
    }
}

impl Default for Directory {
    fn default() -> Directory {
        Directory {
            children: DirChildren::default(),
            attrs: FileAttributes::builder()
                .mode(libc::S_IFDIR)
                .size(std::mem::size_of::<Directory>() as u64)
                .build(),
        }
    }
}

impl Attributable for Directory {
    fn getattrs(&self) -> FileAttributes {
        self.attrs
    }
}

impl Directory {
    pub fn get(&self, name: &OsStr) -> Option<INode> {
        self.children.get(name).map(|x| *x)
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
    pub fn kind(&self) -> &INodeKind<F> {
        &self.kind
    }

    pub fn kind_mut(&mut self) -> &mut INodeKind<F> {
        &mut self.kind
    }

    pub fn file_type(&self) -> FileType {
        match self.kind() {
            INodeKind::Directory(_) => FileType::Directory,
            INodeKind::File(_) => FileType::Regular,
        }
    }

    pub fn parent(&self) -> Option<INode> {
        self.parent
    }

    pub fn as_dir(&self) -> Option<&Directory> {
        match self.kind() {
            INodeKind::Directory(ref dir) => Some(dir),
            _ => None,
        }
    }

    pub fn as_dir_mut(&mut self) -> Option<&mut Directory> {
        match self.kind_mut() {
            INodeKind::Directory(dir) => Some(dir),
            _ => None,
        }
    }

    pub fn as_file(&self) -> Option<&F> {
        match &self.kind() {
            INodeKind::File(file) => Some(file),
            _ => None,
        }
    }

    pub fn as_file_mut(&mut self) -> Option<&mut F> {
        match self.kind_mut() {
            INodeKind::File(file) => Some(file),
            _ => None,
        }
    }

    pub fn children(&self) -> Option<&DirChildren> {
        match self.kind() {
            INodeKind::Directory(dir) => Some(&dir.children),
            _ => None,
        }
    }
}

impl<T: Attributable> INodeEntry<T> {
    pub fn getattrs(&self) -> FileAttributes {
        match self.kind() {
            INodeKind::Directory(dir) => dir.getattrs(),
            INodeKind::File(file) => file.getattrs(),
        }
    }
}

pub trait IntoINodeEntry<F> {
    fn with_parent(self, parent: INode) -> INodeEntry<F>;
}

impl<F: Filelike> IntoINodeEntry<F> for F {
    fn with_parent(self, parent: INode) -> INodeEntry<F> {
        INodeEntry {
            parent: Some(parent),
            kind: INodeKind::File(self),
        }
    }
}

impl<F> IntoINodeEntry<F> for Directory {
    fn with_parent(self, parent: INode) -> INodeEntry<F> {
        INodeEntry {
            parent: Some(parent),
            kind: INodeKind::Directory(self),
        }
    }
}

impl<F> IntoINodeEntry<F> for INodeEntry<F> {
    fn with_parent(mut self, parent: INode) -> INodeEntry<F> {
        self.parent = Some(parent);
        self
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
    pub fn push_entry<E: IntoINodeEntry<F>>(
        &mut self,
        parent: INode,
        name: OsString,
        entry: E,
    ) -> Option<INode> {
        let ino = self.next_open_inode();
        let parent_dir = self.map.get_mut(&parent)?.as_dir_mut()?;

        parent_dir.children.insert(name, ino);
        self.map.insert(ino, entry.with_parent(parent));

        Some(ino)
    }

    pub fn get<T: Into<INode>>(&self, ino: T) -> Option<&INodeEntry<F>> {
        self.map.get(&ino.into())
    }

    pub fn get_mut<T: Into<INode>>(&mut self, ino: T) -> Option<&mut INodeEntry<F>> {
        self.map.get_mut(&ino.into())
    }

    /// Looks up a path. Will function with or without a leading slash
    /// ```
    /// # use simply_fuse::basic::{ROOT_INODE, INodeTable, Directory, INodeEntry};
    /// let mut tbl = INodeTable::<()>::default();
    /// let test_dir_inode = tbl.push_entry(ROOT_INODE, "example directory".into(),
    /// Directory::default()).unwrap();
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
                    parent_ino = parent.as_dir()?.get(path.as_os_str())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Debug)]
    struct BlankFile {}

    impl IntoINodeEntry<BlankFile> for BlankFile {
        fn with_parent(self, parent: INode) -> INodeEntry<BlankFile> {
            INodeEntry {
                parent: Some(parent),
                kind: INodeKind::File(self),
            }
        }
    }

    fn blank_table() -> INodeTable<BlankFile> {
        INodeTable::<BlankFile>::default()
    }

    #[test]
    fn omit_root_slash_lookup() {
        let mut fs = blank_table();
        let _ = fs.push_entry(ROOT_INODE, "root file".into(), BlankFile::default());
        let _ = fs.push_entry(ROOT_INODE, "root dir".into(), Directory::default());

        let file = fs.lookup("/root file").unwrap();

        assert_eq!(
            fs.lookup("root file").unwrap().0,
            file.0,
            "omitting / from paths returns different results"
        );

        assert!(
            file.1.as_file().is_some(),
            "expected file, returned directory"
        );
    }

    #[test]
    fn check_proper_parenting() {
        let mut fs = blank_table();

        let dir_ino = fs
            .push_entry(ROOT_INODE, "dir".into(), Directory::default())
            .unwrap();

        let file_ino = fs
            .push_entry(dir_ino, "file".into(), BlankFile::default())
            .unwrap();

        let file = fs
            .get(file_ino)
            .expect("file was not added to the inode map");

        assert!(file.parent().is_some(), "file has no parent set");

        assert_eq!(
            fs.get(file_ino).unwrap().parent().unwrap(),
            dir_ino,
            "file's parent is not set correctly"
        );
    }

    #[test]
    fn default_table_has_root() {
        let fs = INodeTable::<()>::default();

        assert!(fs.get(ROOT_INODE).is_some(), "ROOT_INODE does not exist");

        assert_eq!(
            fs.get(ROOT_INODE).unwrap().parent(),
            None,
            "ROOT_INODE does not have a parent"
        );
    }

    /// This test should never fail. If it does, we likely have some much bigger problems somewhere
    #[test]
    fn ensure_lookup_equals_lookup_mut() {
        let mut fs = blank_table();

        {
            // ensure we don't use these later
            let dir1 = fs
                .push_entry(ROOT_INODE, "dir1".into(), Directory::default())
                .unwrap();

            let dir2 = fs
                .push_entry(dir1, "dir2".into(), Directory::default())
                .unwrap();

            let dir3 = fs
                .push_entry(dir2, "dir3".into(), Directory::default())
                .unwrap();

            let _file1 = fs
                .push_entry(dir3, "file1".into(), BlankFile::default())
                .unwrap();
        };

        // totally didn't write this solely to mess around with macros
        macro_rules! check {
            ($path:expr) => {{
                let path: OsString = ($path).into();

                assert!(fs.lookup(&path).is_some(), concat!(stringify!($path), " could not be looked up"));

                assert_eq!(
                    fs.lookup(&path).unwrap().0,
                    fs.lookup_mut(&path).unwrap().0,
                    concat!(stringify!($path), " differs between immutable and mutable lookups")
                )
            }};

            [$path:expr, $($others:expr),*$(,)?] => {{
                check!($path);
                check!($($others),+);
            }};
        }

        check![
            "/dir1",
            "/dir1/dir2",
            "/dir1/dir2/dir3",
            "/dir1/dir2/dir3/file1",
            "dir1",
            "dir1/dir2",
            "dir1/dir2/dir3",
            "dir1/dir2/dir3/file1",
        ];
    }
}
