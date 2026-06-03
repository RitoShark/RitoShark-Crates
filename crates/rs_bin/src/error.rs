#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),

    #[error("invalid bin magic: expected PROP or PTCH, got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("invalid bin value type id: {0}")]
    InvalidType(u8),

    #[error("a container element type may not itself be a container: {0}")]
    NestedContainer(u8),

    #[error("declared size {declared} does not match {actual} bytes consumed")]
    SizeMismatch { declared: usize, actual: usize },

    #[error("invalid option count {0}, expected 0 or 1")]
    InvalidOptionCount(u8),

    #[error("value too large to serialize: {0}")]
    TooLarge(&'static str),

    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    #[error("text parse error at line {line}: {message}")]
    TextParse { line: usize, message: String },
}

pub type Result<T> = core::result::Result<T, Error>;
