#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),

    #[error("invalid rman magic: expected RMAN, got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("unsupported rman version: {0}.{1}")]
    UnsupportedVersion(u8, u8),

    #[error("failed to decompress rman body: {0}")]
    Decompress(String),

    #[error("malformed rman body: {0}")]
    Malformed(&'static str),

    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = core::result::Result<T, Error>;
