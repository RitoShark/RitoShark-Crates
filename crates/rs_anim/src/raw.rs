use rs_math::Vec3;

/** Captures the exact on-disk form of an uncompressed `r3d2anmd` v5 animation so the writer can
reproduce the original bytes verbatim. The decoded [`crate::Animation`] keeps human-editable tracks,
but the quaternion palette is normalized on read and the palette ordering is not recoverable from the
decoded poses, so the raw sections are retained alongside to guarantee a byte-exact round-trip. */
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RawV5 {
    pub format_token: u32,
    pub flags1: u32,
    pub flags2: u32,
    pub track_count: u32,
    pub frame_count: u32,
    pub frame_duration: f32,
    pub asset_name_offset: i32,
    pub time_offset: i32,
    pub vecs: Vec<Vec3>,
    pub quats: Vec<[u8; 6]>,
    pub joint_hashes: Vec<u32>,
    pub frame_indices: Vec<[u16; 3]>,
}
