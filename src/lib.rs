pub mod attrs;
pub mod basic;
pub mod error;
mod runner;

pub use crate::runner::Runner;

use crate::attrs::*;
use crate::error::{FSError, FSResult};

use std::ffi::{OsStr, OsString};
use std::io::BufRead;
use std::time::Duration;

use typed_builder::TypedBuilder;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct INode(u64);

impl INode {
    pub const fn to_u64(self) -> u64 {
        self.0
    }

    const fn next_inode(self) -> INode {
        INode(self.0 + 1)
    }
}

impl From<u64> for INode {
    fn from(i: u64) -> INode {
        INode(i)
    }
}

#[derive(Debug, TypedBuilder)]
pub struct Lookup {
    attributes: FileAttributes,
    inode: INode,

    #[builder(default = None)]
    generation: Option<u64>,

    #[builder(default = Some(Duration::from_secs(1)))]
    attr_timeout: Option<Duration>,

    #[builder(default = Some(Duration::from_secs(1)))]
    entry_timeout: Option<Duration>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Filehandle(u64);

impl Filehandle {
    pub const fn from_raw(old: u64) -> Self {
        Self(old)
    }

    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, TypedBuilder)]
pub struct OpenFile {
    handle: Filehandle,

    #[builder(default = true)]
    direct_io: bool,

    #[builder(default = false)]
    keep_cache: bool,

    #[builder(default = true)]
    seekable: bool,
}

#[derive(Debug, Copy, Clone)]
pub enum FileType {
    FIFO,
    Unknown,
    Regular,
    Directory,
    Socket,
    Char,
    Block,
    Link,
}

impl FileType {
    pub const fn to_libc_type(self) -> u8 {
        match self {
            Self::FIFO => libc::DT_FIFO,
            Self::Unknown => libc::DT_UNKNOWN,
            Self::Regular => libc::DT_REG,
            Self::Directory => libc::DT_DIR,
            Self::Socket => libc::DT_SOCK,
            Self::Char => libc::DT_CHR,
            Self::Block => libc::DT_BLK,
            Self::Link => libc::DT_LNK,
        }
    }
}

#[derive(Debug, TypedBuilder, Clone)]
pub struct DirEntry {
    name: OsString,
    inode: INode,
    typ: FileType,
    offset: u64,
}

pub trait Filesystem {
    fn open(&mut self, _ino: INode, _flags: u32) -> FSResult<OpenFile> {
        Err(FSError::NotImplemented)
    }

    fn lookup(&mut self, _parent: INode, _name: &OsStr) -> FSResult<Lookup> {
        Err(FSError::NotImplemented)
    }

    fn getattr(&mut self, _inode: INode) -> FSResult<FileAttributes> {
        Err(FSError::NotImplemented)
    }

    fn setattr(&mut self, _inode: INode, _attr: SetFileAttributes) -> FSResult<FileAttributes> {
        Err(FSError::NotImplemented)
    }

    /// Reads a directory.
    ///
    /// # Warning
    /// This method **must** include the "." and ".." directories, as well as properly accounting
    /// for `offset`. If not, some operations may get stuck in an infinite loop while trying to
    /// read a directory.
    fn readdir(&mut self, _dir: INode, _offset: u64) -> FSResult<Vec<DirEntry>> {
        Err(FSError::NotImplemented)
    }

    fn read(&mut self, _ino: INode, _offset: u64, _size: u32) -> FSResult<&[u8]> {
        Err(FSError::NotImplemented)
    }

    /// Returns the amount of bytes written
    fn write<T: BufRead>(
        &mut self,
        _ino: INode,
        _offset: u64,
        _size: u32,
        _buf: T,
    ) -> FSResult<u32> {
        Err(FSError::NotImplemented)
    }
}
