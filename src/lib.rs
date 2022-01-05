#![feature(decl_macro)]
pub mod basic;
pub mod error;

use crate::error::{FSError, FSResult, PolyfuseError, Result};

use std::ffi::{OsStr, OsString};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::Duration;

use polyfuse::{op, reply, KernelConfig, Operation, Request, Session};
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

#[derive(Copy, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(default, setter(into)))]
pub struct FileAttributes {
    #[builder(!default, setter(!strip_option))]
    mode: u32,
    #[builder(default = 4096, setter(!strip_option))]
    size: u64,
    nlink: u32,

    uid: u32,
    gid: u32,

    rdev: u32,
    blksize: u32,
    blocks: u64,

    atime: Duration,
    mtime: Duration,
    ctime: Duration,

    #[builder(default = Duration::from_secs(1))]
    ttl: Duration,
}

impl FileAttributes {
    pub fn mode(&self) -> u32 {
        self.mode
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn nlink(&self) -> u32 {
        self.nlink
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    pub fn gid(&self) -> u32 {
        self.gid
    }

    pub fn rdev(&self) -> u32 {
        self.rdev
    }

    pub fn blksize(&self) -> u32 {
        self.blksize
    }

    pub fn blocks(&self) -> u64 {
        self.blocks
    }

    pub fn atime(&self) -> Duration {
        self.atime
    }

    pub fn mtime(&self) -> Duration {
        self.mtime
    }

    pub fn ctime(&self) -> Duration {
        self.ctime
    }

    /// # Note
    /// `ttl` means `time to live`
    /// This is **not** time to live, such as you'd go live on Twitch. It means time to live,
    /// as in the remaining time you have left alive. I spent way too long misunderstanding
    /// that.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn set_mode(mut self, mode: u32) -> FileAttributes {
        self.mode = mode;
        self
    }

    pub fn set_size(mut self, size: u64) -> FileAttributes {
        self.size = size;
        self
    }

    pub fn set_nlink(mut self, nlink: u32) -> FileAttributes {
        self.nlink = nlink;
        self
    }

    pub fn set_uid(mut self, uid: u32) -> FileAttributes {
        self.uid = uid;
        self
    }

    pub fn set_gid(mut self, gid: u32) -> FileAttributes {
        self.gid = gid;
        self
    }

    pub fn set_rdev(mut self, rdev: u32) -> FileAttributes {
        self.rdev = rdev;
        self
    }

    pub fn set_blksize(mut self, blksize: u32) -> FileAttributes {
        self.blksize = blksize;
        self
    }

    pub fn set_blocks(mut self, blocks: u64) -> FileAttributes {
        self.blocks = blocks;
        self
    }

    pub fn set_atime(mut self, atime: Duration) -> FileAttributes {
        self.atime = atime;
        self
    }

    pub fn set_mtime(mut self, mtime: Duration) -> FileAttributes {
        self.mtime = mtime;
        self
    }

    pub fn set_ctime(mut self, ctime: Duration) -> FileAttributes {
        self.ctime = ctime;
        self
    }

    pub fn set_ttl(mut self, ttl: Duration) -> FileAttributes {
        self.ttl = ttl;
        self
    }

    pub fn apply_attrs(&mut self, attrs: SetFileAttributes) -> FileAttributes {
        // TODO convert this to macro_rules! maybe
        macro copy_attr($name:ident) {
            if let Some(attr) = attrs.$name {
                self.$name = attr;
            }
        }

        copy_attr!(mode);
        copy_attr!(size);
        copy_attr!(uid);
        copy_attr!(gid);
        copy_attr!(atime);
        copy_attr!(mtime);
        copy_attr!(ctime);

        *self
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SetFileAttributes {
    mode: Option<u32>,
    size: Option<u64>,

    uid: Option<u32>,
    gid: Option<u32>,

    atime: Option<Duration>,
    mtime: Option<Duration>,
    ctime: Option<Duration>,
}

impl SetFileAttributes {
    pub fn mode(&self) -> Option<u32> {
        self.mode
    }

    pub fn size(&self) -> Option<u64> {
        self.size
    }

    pub fn uid(&self) -> Option<u32> {
        self.uid
    }

    pub fn gid(&self) -> Option<u32> {
        self.gid
    }

    pub fn atime(&self) -> Option<Duration> {
        self.atime
    }

    pub fn mtime(&self) -> Option<Duration> {
        self.mtime
    }

    pub fn ctime(&self) -> Option<Duration> {
        self.ctime
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

#[derive(Debug)]
pub struct Runner<T>
where
    T: Filesystem,
{
    mountpoint: PathBuf,
    fs: T,
}

impl<T: Filesystem> Runner<T> {
    pub fn new<P: AsRef<Path>>(fs: T, mountpoint: P) -> Runner<T> {
        Runner {
            mountpoint: mountpoint.as_ref().to_path_buf(),
            fs,
        }
    }

    pub fn run_block(&mut self) -> Result<()> {
        let session = Session::mount(self.mountpoint.to_path_buf(), KernelConfig::default())?;

        while let Some(req) = session.next_request()? {
            match req.operation().map_err(PolyfuseError::DecodeError)? {
                Operation::Lookup(op) => self.handle_lookup(&req, op)?,
                Operation::Getattr(op) => self.handle_getattr(&req, op)?,
                Operation::Setattr(op) => self.handle_setattr(&req, op)?,
                Operation::Readdir(op) => self.handle_readdir(&req, op)?,
                Operation::Read(op) => self.handle_read(&req, op)?,
                Operation::Write(op, buf) => self.handle_write(&req, op, buf)?,
                op => {
                    eprintln!("unimplemented: {:?}", op);
                    req.reply_error(FSError::NotImplemented.to_libc_error())
                        .map_err(PolyfuseError::ReplyErrError)?;
                }
            }
        }

        todo!()
    }

    fn handle_lookup(&mut self, req: &Request, op: op::Lookup<'_>) -> Result<(), PolyfuseError> {
        match self.fs.lookup(op.parent().into(), op.name()) {
            Ok(obj) => {
                let mut res = reply::EntryOut::default();
                res.ino(obj.inode.to_u64());

                if let Some(generation) = obj.generation {
                    res.generation(generation)
                }

                if let Some(attr_timeout) = obj.attr_timeout {
                    res.ttl_attr(attr_timeout);
                }

                if let Some(entry_timeout) = obj.entry_timeout {
                    res.ttl_entry(entry_timeout);
                }

                self.copy_file_attr(&obj.attributes, obj.inode, res.attr());
                req.reply(res).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                eprintln!("lookup err: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }
        Ok(())
    }

    fn handle_getattr(&mut self, req: &Request, op: op::Getattr<'_>) -> Result<(), PolyfuseError> {
        match self.fs.getattr(op.ino().into()) {
            Ok(obj) => {
                let mut conv: reply::AttrOut = reply::AttrOut::default();
                conv.attr().ino(op.ino()); // FileAttribute does not keep the inode
                conv.ttl(obj.ttl);

                self.copy_file_attr(&obj, op.ino().into(), conv.attr());
                req.reply(conv).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                eprintln!("getattr err: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }
        Ok(())
    }

    fn handle_setattr(&mut self, req: &Request, op: op::Setattr<'_>) -> Result<(), PolyfuseError> {
        let to_duration = |spec: op::SetAttrTime| {
            use op::SetAttrTime;

            match spec {
                SetAttrTime::Timespec(dur) => Some(dur),
                SetAttrTime::Now => Some(std::time::UNIX_EPOCH.elapsed().unwrap()),
                spec => {
                    eprintln!(
                        "Unknown timespec {:#?} encountered. Returning 'None' for now!",
                        spec
                    );
                    None
                }
            }
        };

        let attrs = SetFileAttributes {
            mode: op.mode(),
            size: op.size(),

            uid: op.uid(),
            gid: op.gid(),

            atime: op.atime().and_then(to_duration),
            mtime: op.mtime().and_then(to_duration),
            ctime: op.ctime(),
        };

        match self.fs.setattr(op.ino().into(), attrs) {
            Ok(obj) => {
                let mut conv: reply::AttrOut = reply::AttrOut::default();
                conv.attr().ino(op.ino());
                conv.ttl(obj.ttl);

                self.copy_file_attr(&obj, op.ino().into(), conv.attr());
                req.reply(conv).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                eprintln!("setattr err: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_readdir(&mut self, req: &Request, op: op::Readdir<'_>) -> Result<(), PolyfuseError> {
        // TODO implement readdir plus support
        // readdirplus doesn't seem to be documented by polyfuse plus, so we just force it to error
        // currently
        if op.mode() == op::ReaddirMode::Plus {
            req.reply_error(FSError::NotImplemented.to_libc_error())
                .map_err(PolyfuseError::ReplyErrError)?;
            return Ok(());
        }

        match self.fs.readdir(op.ino().into(), op.offset()) {
            Ok(entries) => {
                let mut rep = reply::ReaddirOut::new(op.size() as usize);

                // use take_while as a for_each_while
                entries
                    .into_iter()
                    .take_while(|x| {
                        rep.entry(
                            &x.name,
                            x.inode.to_u64(),
                            x.typ.to_libc_type() as u32,
                            x.offset,
                        )
                    })
                    .for_each(|_| {});

                req.reply(rep).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                eprintln!("readdir err: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_read(&mut self, req: &Request, op: op::Read<'_>) -> Result<(), PolyfuseError> {
        match self.fs.read(op.ino().into(), op.offset(), op.size()) {
            Ok(data) => {
                req.reply(data).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                eprintln!("read err: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_write<B: BufRead>(
        &mut self,
        req: &Request,
        op: op::Write<'_>,
        buf: B,
    ) -> Result<(), PolyfuseError> {
        match self.fs.write(op.ino().into(), op.offset(), op.size(), buf) {
            Ok(len) => {
                let mut rep = reply::WriteOut::default();
                rep.size(len);

                req.reply(rep).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                eprintln!("write err: {:#?}", e);

                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    /// Copies the attributes from a `FileAttribute` plus inode to a polyfuse `FileAttr`
    /// Passing the inode is required as `FileAttribute`s do not keep track of the inodes
    fn copy_file_attr(&self, from: &FileAttributes, inode: INode, to: &mut reply::FileAttr) {
        to.ino(inode.to_u64());

        to.mode(from.mode);
        to.size(from.size);
        to.nlink(from.nlink);
        to.uid(from.uid);
        to.gid(from.gid);
        to.rdev(from.rdev);
        to.blksize(from.blksize);
        to.blocks(from.blocks);
        to.atime(from.atime);
        to.mtime(from.mtime);
        to.ctime(from.ctime);
    }
}

impl<T: Filesystem + Send + 'static> Runner<T> {
    /// Runs `self.run_block()` by spawning a new thread and returning the join handle.
    pub fn run(mut self) -> JoinHandle<(Runner<T>, Result<()>)> {
        std::thread::spawn(move || {
            let result = self.run_block();
            (self, result)
        })
    }
}
