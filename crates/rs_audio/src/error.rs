#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),
    #[error("invalid magic")]
    InvalidMagic,
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = core::result::Result<T, Error>;
