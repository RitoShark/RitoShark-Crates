#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("animation export failed in every allowed format: {0}")]
    AnimationExportFailed(String),

    #[error(
        "CANM key fitting could not meet requested accuracy (worst normalized error {error_ratio}; maximum 1)"
    )]
    CompressionFit { error_ratio: f32 },

    #[error(
        "CANM clock precision for joint {joint_hash:#010x} {channel}: times {seconds:?} share tick {tick}, requiring error >= {error}, tolerance {tolerance}"
    )]
    CompressionTimePrecision {
        joint_hash: u32,
        channel: &'static str,
        tick: u16,
        seconds: [f32; 2],
        error: f32,
        tolerance: f32,
    },

    #[error(transparent)]
    Io(#[from] rs_io::Error),

    #[error("invalid magic: {0:?}")]
    InvalidMagic([u8; 8]),

    #[error("unsupported version: {0}")]
    UnsupportedVersion(u32),

    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    #[error("invalid data: {0}")]
    InvalidData(&'static str),

    #[error(
        "compression error for joint {joint_hash:#010x} {channel} at sample {frame}: {error} exceeds tolerance {tolerance}"
    )]
    CompressionTolerance {
        joint_hash: u32,
        channel: &'static str,
        frame: usize,
        error: f32,
        tolerance: f32,
    },
}

pub type Result<T> = core::result::Result<T, Error>;
