#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),
    #[error("invalid RST magic: expected \"RST\"")]
    InvalidMagic,
    #[error("unsupported RST version {0}")]
    UnsupportedVersion(u8),
}

pub type Result<T> = core::result::Result<T, Error>;
