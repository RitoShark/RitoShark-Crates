#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),

    #[error("invalid tex magic: expected {expected:#010x}, got {got:#010x}")]
    InvalidMagic { expected: u32, got: u32 },

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("encode error: {0}")]
    Encode(String),
}

pub type Result<T> = core::result::Result<T, Error>;

impl From<ddsfile::Error> for Error {
    fn from(e: ddsfile::Error) -> Self {
        Error::Decode(format!("dds: {e}"))
    }
}
