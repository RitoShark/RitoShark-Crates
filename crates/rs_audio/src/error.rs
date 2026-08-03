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
    #[error("truncated or out-of-range: a declared size or offset runs past the end of input")]
    Truncated,
    #[error("malformed wem: {0}")]
    Wem(&'static str),
    #[error("unsupported wem codec 0x{0:04X}")]
    UnsupportedCodec(u16),
    #[error("codebook {0} is not in the library")]
    UnknownCodebook(u32),
}

pub type Result<T> = core::result::Result<T, Error>;
