use crate::attrs::{FileAttributes, SetFileAttributes};
use crate::error::{FSError, PolyfuseError, Result};
use crate::{Filesystem, INode, Lookup, SetXAttrFlags};

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use polyfuse::{op, reply, KernelConfig, Operation, Request, Session};
use tracing::{error, warn};

impl Lookup {
    fn apply_attrs_to(&self, attrs: &mut reply::FileAttr) {
        self.attributes.copy_attrs_to(self.inode, attrs);
    }
}

impl FileAttributes {
    /// Copies the attributes from our own `FileAttributes` to a polyfuse `FileAttr`.
    fn copy_attrs_to(&self, ino: INode, attrs: &mut reply::FileAttr) {
        attrs.ino(ino.to_u64());

        attrs.size(self.size());
        attrs.mode(self.mode());
        attrs.nlink(self.nlink());
        attrs.uid(self.uid());
        attrs.gid(self.gid());
        attrs.rdev(self.rdev());
        attrs.blksize(self.blksize());
        attrs.blocks(self.blocks());

        attrs.atime(self.atime());
        attrs.mtime(self.mtime());
        attrs.ctime(self.ctime());
    }
}

impl From<Lookup> for reply::EntryOut {
    fn from(val: Lookup) -> Self {
        let mut out = reply::EntryOut::default();
        out.ino(val.inode.to_u64());

        if let Some(x) = val.generation {
            out.generation(x)
        }

        if let Some(x) = val.attr_timeout {
            out.ttl_attr(x)
        }

        if let Some(x) = val.entry_timeout {
            out.ttl_entry(x)
        }

        val.apply_attrs_to(out.attr());

        out
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
                Operation::Open(op) => self.handle_open(&req, op)?,
                Operation::Opendir(op) => self.handle_opendir(&req, op)?,

                Operation::Setxattr(op) => self.handle_setxattr(&req, op)?,
                Operation::Getxattr(op) => self.handle_getxattr(&req, op)?,
                Operation::Listxattr(op) => self.handle_listxattr(&req, op)?,

                Operation::Lookup(op) => self.handle_lookup(&req, op)?,
                Operation::Getattr(op) => self.handle_getattr(&req, op)?,
                Operation::Setattr(op) => self.handle_setattr(&req, op)?,
                Operation::Readdir(op) => self.handle_readdir(&req, op)?,
                Operation::Read(op) => self.handle_read(&req, op)?,
                Operation::Write(op, buf) => self.handle_write(&req, op, buf)?,
                op => {
                    error!("unimplemented: {:?}", op);
                    req.reply_error(FSError::NotImplemented.to_libc_error())
                        .map_err(PolyfuseError::ReplyErrError)?;
                }
            }
        }

        todo!()
    }

    fn handle_open(&mut self, req: &Request, op: op::Open<'_>) -> Result<(), PolyfuseError> {
        match self.fs.open(op.ino().into(), op.flags()) {
            Ok(obj) => {
                let mut res = reply::OpenOut::default();

                res.fh(obj.handle.to_raw());
                res.direct_io(obj.direct_io);
                res.keep_cache(obj.keep_cache);
                res.nonseekable(!obj.seekable);
                res.cache_dir(false); // I think this only works for readdir

                req.reply(res).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                warn!("open error occured: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_opendir(&mut self, req: &Request, op: op::Opendir<'_>) -> Result<(), PolyfuseError> {
        match self.fs.open_dir(op.ino().into(), op.flags()) {
            Ok(obj) => {
                let mut res = reply::OpenOut::default();

                res.fh(obj.handle.to_raw());
                res.direct_io(obj.direct_io);
                res.keep_cache(obj.keep_cache);
                res.nonseekable(!obj.seekable);
                res.cache_dir(obj.cache_dir);

                req.reply(res).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                warn!("opendir error occured: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_setxattr(
        &mut self,
        req: &Request,
        op: op::Setxattr<'_>,
    ) -> Result<(), PolyfuseError> {
        let flags = SetXAttrFlags::from_libc_type(op.flags() as i32)
            .ok_or_else(|| FSError::InvalidFlags(op.flags() as u32));

        if flags.is_err() {
            req.reply_error(libc::EINVAL)
                .map_err(PolyfuseError::ReplyErrError)?;

            return Ok(());
        }

        match self
            .fs
            .setxattr(op.ino().into(), op.name(), op.value(), flags.unwrap())
        {
            Ok(_) => {
                req.reply(()).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                warn!("setxattr error occured: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_getxattr(
        &mut self,
        req: &Request,
        op: op::Getxattr<'_>,
    ) -> Result<(), PolyfuseError> {
        let size = op.size();

        match self.fs.getxattr(op.ino().into(), op.name(), size) {
            Ok(obj) => {
                if size == 0 {
                    // When op.size() == 0, polyfuse wants us to return the length of the attribute
                    let mut res = reply::XattrOut::default();
                    res.size(obj.full_len() as u32);
                    req.reply(res).map_err(PolyfuseError::ReplyError)?;
                } else {
                    assert!(
                        obj.data.len() <= size as usize,
                        "cannot return data larger than requested"
                    );

                    req.reply(obj.data).map_err(PolyfuseError::ReplyError)?;
                }
            }
            Err(e) => {
                warn!("getxattr error occured: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_listxattr(
        &mut self,
        req: &Request,
        op: op::Listxattr<'_>,
    ) -> Result<(), PolyfuseError> {
        let size = op.size();

        match self.fs.listxattrs(op.ino().into(), size) {
            Ok(obj) => {
                if size == 0 {
                    let mut res = reply::XattrOut::default();
                    res.size(obj.1);
                    req.reply(res).map_err(PolyfuseError::ReplyError)?;
                } else {
                    debug_assert!(
                        obj.0.len() <= size as usize,
                        "returned string larger than the buffer size"
                    );

                    req.reply(obj.0).map_err(PolyfuseError::ReplyError)?;
                }
            }
            Err(e) => {
                warn!("listxattr error occured: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
    }

    fn handle_lookup(&mut self, req: &Request, op: op::Lookup<'_>) -> Result<(), PolyfuseError> {
        match self.fs.lookup(op.parent().into(), op.name()) {
            Ok(obj) => {
                let res = reply::EntryOut::from(obj);

                req.reply(res).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                warn!("lookup error occured: {:#?}", e);
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

                conv.ttl(obj.ttl());
                obj.copy_attrs_to(op.ino().into(), conv.attr());

                req.reply(conv).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                warn!("getattr error occured: {:#?}", e);
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
                    error!(
                        "Unknown timespec \"{:#?}\" encountered. Assuming `None` for now!",
                        spec
                    );

                    None
                }
            }
        };

        let attrs = SetFileAttributes::builder()
            .mode(op.mode())
            .size(op.size())
            .uid(op.uid())
            .gid(op.gid())
            .atime(op.atime().and_then(to_duration))
            .mtime(op.mtime().and_then(to_duration))
            .ctime(op.ctime())
            .build();

        match self.fs.setattr(op.ino().into(), attrs) {
            Ok(obj) => {
                let mut conv: reply::AttrOut = reply::AttrOut::default();

                conv.ttl(obj.ttl());
                obj.copy_attrs_to(op.ino().into(), conv.attr());

                req.reply(conv).map_err(PolyfuseError::ReplyError)?;
            }
            Err(e) => {
                warn!("setattr error occured: {:#?}", e);
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
                warn!("readdir error occured: {:#?}", e);
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
                warn!("read error occured: {:#?}", e);
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
                warn!("write error occured: {:#?}", e);
                req.reply_error(e.to_libc_error())
                    .map_err(PolyfuseError::ReplyErrError)?;
            }
        }

        Ok(())
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
