use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub type FSResult<T, E = FSError> = Result<T, E>;

#[derive(Error, Debug)]
pub enum PolyfuseError {
    #[error(transparent)]
    DecodeError(#[from] polyfuse::op::DecodeError),

    #[error("error occured while calling Request::reply_err")]
    ReplyErrError(std::io::Error),

    #[error("error occured while calling Request::reply")]
    ReplyError(std::io::Error),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    PolyfuseError(#[from] PolyfuseError),

    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

/// This type represents an error that occured in the filesystem struct itself.
#[derive(Error, Debug)]
pub enum FSError {
    #[error("No such file or directory exists")]
    NoEntry,

    #[error("Not a file")]
    NotFile,

    #[error("Not a directory")]
    NotDirectory,

    #[error("Function not implemented")]
    NotImplemented,
}

impl FSError {
    pub const fn to_libc_error(self) -> i32 {
        match self {
            Self::NoEntry => libc::ENOENT,
            Self::NotFile => libc::EINVAL, // TODO is this the proper error to return?
            Self::NotDirectory => libc::ENOTDIR,
            Self::NotImplemented => libc::ENOSYS,
        }
    }
}
