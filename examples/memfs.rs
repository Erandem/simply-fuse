use simply_fuse::attrs::{FileAttributes, SetFileAttributes};
use simply_fuse::basic::*;
use simply_fuse::error::{FSError, FSResult as Result};
use simply_fuse::*;

use std::ffi::OsStr;
use std::io::BufRead;

const TEST_MSG: &str = "hello_world!";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut fs = MemFS::new();
    fs.inodes
        .push_entry(ROOT_INODE, "test".into(), Directory::default());

    fs.inodes
        .push_entry(2u64.into(), "test2".into(), Directory::default());

    fs.inodes
        .push_entry(3u64.into(), "test3".into(), Directory::default());

    fs.inodes
        .push_entry(1u64.into(), "root2".into(), Directory::default());

    fs.inodes.push_entry(
        ROOT_INODE,
        "file".into(),
        File::new(TEST_MSG.as_bytes().into()),
    );

    let mut r = Runner::new(fs, "./mount");
    println!("{:#?}", r);
    r.run_block()?;

    Ok(())
}

#[derive(Debug)]
pub struct File {
    pub data: Vec<u8>,
    pub attrs: FileAttributes,
}

impl File {
    fn new(data: Vec<u8>) -> File {
        File {
            attrs: FileAttributes::builder()
                .size(data.len() as u64)
                .mode(libc::S_IFREG | 0o755)
                .build(),

            data,
        }
    }

    fn size(&self) -> usize {
        self.attrs.size() as usize
    }
}

impl Attributable for File {
    fn getattrs(&self) -> FileAttributes {
        self.attrs
    }
}

impl Filelike for File {}

#[derive(Debug)]
struct MemFS {
    inodes: INodeTable<File>,
}

impl MemFS {
    fn new() -> MemFS {
        MemFS {
            inodes: INodeTable::default(),
        }
    }
}

impl Filesystem for MemFS {
    fn lookup(&mut self, parent: INode, name: &OsStr) -> Result<Lookup> {
        let parent = self
            .inodes
            .get(parent)
            .ok_or(FSError::NoEntry)
            .and_then(|x| x.as_dir().ok_or(FSError::NotDirectory))?;

        let (child_ino, child) = parent
            .get(name)
            // get the inode entry and then map it into (inode, &entry)
            .and_then(|ino| self.inodes.get(*ino).map(|x| (*ino, x)))
            .ok_or(FSError::NoEntry)?;

        Ok(Lookup::builder()
            .attributes(child.getattrs())
            .inode(child_ino)
            .build())
    }

    fn getattr(&mut self, inode: INode) -> Result<FileAttributes> {
        let entry = self.inodes.get(inode).ok_or(FSError::NoEntry)?;

        Ok(entry.getattrs())
    }

    fn readdir(&mut self, dir_ino: INode, offset: u64) -> Result<Vec<DirEntry>> {
        let dir_main = self.inodes.get(dir_ino).ok_or(FSError::NoEntry)?;
        let dir = dir_main.as_dir().ok_or(FSError::NotDirectory)?;

        let dots = [
            DirEntry::builder()
                .name(".".into())
                .inode(dir_ino)
                .typ(FileType::Directory)
                .offset(1)
                .build(),
            DirEntry::builder()
                .name("..".into())
                .inode(dir_main.parent().unwrap_or(2u64.into()))
                .typ(FileType::Directory)
                .offset(2)
                .build(),
        ]
        .into_iter()
        .map(|x| x.clone());

        Ok(dots
            .into_iter()
            .chain(
                dir.children()
                    .enumerate()
                    .map(
                        |(off, v)| (off + 3, v), // add 3 to skip 0 and the two dots
                    )
                    .map(|(offset, (name, inode))| {
                        DirEntry::builder()
                            .name(name.clone())
                            .offset(offset as u64)
                            .inode(inode)
                            .typ(self.inodes.get(inode).unwrap().file_type())
                            .build()
                    }),
            )
            .skip(offset as usize)
            .collect())
    }

    fn read(&mut self, ino: INode, offset: u64, size: u32) -> Result<&[u8]> {
        let file = self.inodes.get(ino).ok_or(FSError::NoEntry)?;
        let file = file.as_file().ok_or(FSError::NotFile)?;

        let offset = offset as usize;
        let size = size as usize;

        let content = file.data.get(offset..).unwrap_or(&[]);
        let content = &content[..std::cmp::min(file.size(), size)];

        Ok(content)
    }

    fn write<T: BufRead>(&mut self, ino: INode, offset: u64, size: u32, mut buf: T) -> Result<u32> {
        let file = self.inodes.get_mut(ino).ok_or(FSError::NoEntry)?;
        let file = file.as_file_mut().ok_or(FSError::NotFile)?;

        let offset = offset as usize;
        let size = size as usize;

        file.data
            .resize(std::cmp::max(file.size(), offset + size), 0);

        buf.read_exact(&mut file.data[offset..offset + size])
            .unwrap();

        file.attrs = file.attrs.set_size((offset + size) as u64);

        Ok(size as u32)
    }

    fn setattr(&mut self, ino: INode, attrs: SetFileAttributes) -> Result<FileAttributes> {
        let entry = self.inodes.get_mut(ino).ok_or(FSError::NoEntry)?;

        Ok(match entry.kind_mut() {
            INodeKind::Directory(dir) => dir.apply_attrs(attrs),
            INodeKind::File(file) => file.attrs.apply_attrs(attrs),
        })
    }
}
