#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),
    #[error("invalid WAD magic: expected \"RW\" (0x5752)")]
    InvalidMagic,
    #[error("unsupported WAD version {0}.{1}")]
    UnsupportedVersion(u8, u8),
    #[error("unsupported WAD chunk compression {0}")]
    UnsupportedCompression(u8),
    #[error("decompression failed: {0}")]
    Decompress(String),
}

pub type Result<T> = core::result::Result<T, Error>;
