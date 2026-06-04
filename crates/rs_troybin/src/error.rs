#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error(transparent)]
    StdIo(#[from] std::io::Error),

    #[error("unsupported troybin version {0}")]
    UnsupportedVersion(u8),

    #[error("unsupported troybin value bucket bit {0}")]
    UnsupportedBucket(u8),

    #[error("trailing bytes after troybin body ({0} bytes)")]
    TrailingBytes(usize),

    #[error("value type does not match the target bucket (flag bit {0})")]
    ValueTypeMismatch(u8),

    #[error("no flag bucket carries this value type")]
    NoBucketForType,

    #[error("strings blob exceeds the 64 KiB the u16 offsets can address")]
    StringsTooLarge,
}

pub type Result<T> = core::result::Result<T, Error>;
