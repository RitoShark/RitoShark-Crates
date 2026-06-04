#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),

    #[error("not a Lua bytecode file (bad signature)")]
    InvalidSignature,

    #[error("unsupported Lua version {0:#04x} (expected 0x51)")]
    UnsupportedVersion(u8),

    #[error("unknown Lua constant type {0}")]
    UnknownConstant(u8),

    #[error("unsupported luabin layout: {0}")]
    Unsupported(&'static str),

    #[error("malformed luabin: {0}")]
    Malformed(&'static str),

    #[error("trailing bytes after luabin body ({0} bytes)")]
    TrailingBytes(usize),
}

pub type Result<T> = core::result::Result<T, Error>;
