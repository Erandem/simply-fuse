use std::time::Duration;

use typed_builder::TypedBuilder;

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

    pub fn set_mode(&mut self, mode: u32) {
        self.mode = mode;
    }

    pub fn set_size(&mut self, size: u64) {
        self.size = size;
    }

    pub fn set_nlink(&mut self, nlink: u32) {
        self.nlink = nlink;
    }

    pub fn set_uid(&mut self, uid: u32) {
        self.uid = uid;
    }

    pub fn set_gid(&mut self, gid: u32) {
        self.gid = gid;
    }

    pub fn set_rdev(&mut self, rdev: u32) {
        self.rdev = rdev;
    }

    pub fn set_blksize(&mut self, blksize: u32) {
        self.blksize = blksize;
    }
    pub fn set_blocks(&mut self, blocks: u64) {
        self.blocks = blocks;
    }

    pub fn set_atime(&mut self, atime: Duration) {
        self.atime = atime;
    }
    pub fn set_mtime(&mut self, mtime: Duration) {
        self.mtime = mtime;
    }
    pub fn set_ctime(&mut self, ctime: Duration) {
        self.ctime = ctime;
    }
    pub fn set_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }

    #[deny(unused_variables)]
    pub fn apply_attrs(&mut self, attrs: SetFileAttributes) {
        // Here's a cool trick: By denying unused variables for this function and unpacking the
        // struct below, this function will fail to compile if we update SetFileAttributes without
        // modifying this function. Sure, there's reasons we might want to do that in the future,
        // but we also want to make sure we always modify these variables
        let SetFileAttributes {
            mode,
            size,
            uid,
            gid,
            atime,
            mtime,
            ctime,
        } = attrs;

        // TODO convert this to macro_rules! maybe
        macro copy_attr($name:ident) {
            if let Some(attr) = $name {
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
    }
}

#[derive(Copy, Clone, Debug, TypedBuilder)]
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
