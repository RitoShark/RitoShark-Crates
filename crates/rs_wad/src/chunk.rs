#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WadCompression {
    None = 0,
    Gzip = 1,
    Satellite = 2,
    Zstd = 3,
    ZstdMulti = 4,
}

impl WadCompression {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Gzip),
            2 => Some(Self::Satellite),
            3 => Some(Self::Zstd),
            4 => Some(Self::ZstdMulti),
            _ => None,
        }
    }
}

/** One table-of-contents entry describing a single packed file. The low nibble of the on-disk
type byte selects the [`WadCompression`]; the high nibble carries `subchunk_count`. `data_offset`
is absolute into the archive and `checksum` holds the first eight bytes of the chunk's xxh3-64. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WadChunk {
    pub path_hash: u64,
    pub data_offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub compression: WadCompression,
    pub is_duplicated: bool,
    pub subchunk_count: u8,
    pub subchunk_start: u32,
    pub checksum: u64,
}
