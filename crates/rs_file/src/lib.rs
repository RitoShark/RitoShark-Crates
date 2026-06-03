/*!
rs_file identifies a League of Legends file format from the magic bytes at the start of a file.
`detect` inspects a leading byte slice and returns the matching [`FileKind`], or [`FileKind::Unknown`]
when nothing matches or the slice is too short for the candidate magic. `detect_path` reads the
leading bytes of a file and defers to `detect`. Detection is ordered so that longer, higher-entropy
tags are tested before their shorter prefixes (the `r3d2*` family, the `RW` two-byte WAD tag).
*/

#![forbid(unsafe_code)]

use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    PropBin,
    PatchBin,
    Wad,
    Tex,
    Dds,
    SkinnedMesh,
    Skeleton,
    AnimUncompressed,
    AnimCompressed,
    StaticMeshBinary,
    StaticMeshText,
    MapGeo,
    Rst,
    Rman,
    Wpk,
    Bnk,
    Unknown,
}

const fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/** Identifies the file format from the magic at the start of `bytes`. Longer `r3d2*` tags are
matched before the bare `r3d2` WPK tag, and offset-4 signatures (skeleton) are guarded by a length
check so short slices fall through to [`FileKind::Unknown`] instead of panicking. */
pub fn detect(bytes: &[u8]) -> FileKind {
    let len = bytes.len();

    if len >= 8 {
        match &bytes[0..8] {
            b"r3d2anmd" => return FileKind::AnimUncompressed,
            b"r3d2canm" => return FileKind::AnimCompressed,
            b"r3d2Mesh" => return FileKind::StaticMeshBinary,
            /* Bare four-byte `r3d2` head, once the longer r3d2* tags above are ruled out. */
            [b'r', b'3', b'd', b'2', ..] => return FileKind::Wpk,
            _ => {}
        }
    }

    if len >= 13 && &bytes[0..13] == b"[ObjectBegin]" {
        return FileKind::StaticMeshText;
    }

    if len >= 4 {
        match &bytes[0..4] {
            b"PROP" => return FileKind::PropBin,
            b"PTCH" => return FileKind::PatchBin,
            b"OEGM" => return FileKind::MapGeo,
            b"RMAN" => return FileKind::Rman,
            b"BKHD" => return FileKind::Bnk,
            b"DDS " => return FileKind::Dds,
            [0x54, 0x45, 0x58, 0x00] => return FileKind::Tex,
            [0x33, 0x22, 0x11, 0x00] => return FileKind::SkinnedMesh,
            _ => {}
        }
    }

    if len >= 3 && &bytes[0..3] == b"RST" {
        return FileKind::Rst;
    }

    if len >= 2 && &bytes[0..2] == b"RW" {
        return FileKind::Wad;
    }

    /* Skeleton carries its tag at offset 4, not the head, so it is the lowest-confidence match
    and runs only after every head-anchored magic has been ruled out. */
    if len >= 8 && u32_le(&bytes[4..8]) == 0x22FD_4FC3 {
        return FileKind::Skeleton;
    }

    FileKind::Unknown
}

/** Reads the first 16 bytes of the file at `path` and identifies its format with [`detect`].
Fewer than 16 bytes is not an error; the available prefix is passed through and may still match a
short magic or yield [`FileKind::Unknown`]. */
pub fn detect_path(path: impl AsRef<Path>) -> std::io::Result<FileKind> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 16];
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(detect(&buf[..filled]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_prop_bin() {
        assert_eq!(detect(b"PROP\x01\x00\x00\x00"), FileKind::PropBin);
    }

    #[test]
    fn detects_wad() {
        assert_eq!(detect(b"RW\x03\x00"), FileKind::Wad);
    }

    #[test]
    fn detects_mapgeo() {
        assert_eq!(detect(b"OEGM"), FileKind::MapGeo);
    }

    #[test]
    fn detects_tex() {
        let bytes = [0x54u8, 0x45, 0x58, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(detect(&bytes), FileKind::Tex);
    }

    #[test]
    fn short_slice_is_unknown() {
        assert_eq!(detect(b"xx"), FileKind::Unknown);
    }

    #[test]
    fn detects_patch_bin() {
        assert_eq!(detect(b"PTCH\x00\x00\x00\x00"), FileKind::PatchBin);
    }

    #[test]
    fn detects_skinned_mesh() {
        let bytes = [0x33u8, 0x22, 0x11, 0x00, 0x04, 0x00, 0x01, 0x00];
        assert_eq!(detect(&bytes), FileKind::SkinnedMesh);
    }

    #[test]
    fn detects_skeleton_at_offset_four() {
        let bytes = [0x00u8, 0x00, 0x00, 0x00, 0xC3, 0x4F, 0xFD, 0x22];
        assert_eq!(detect(&bytes), FileKind::Skeleton);
    }

    #[test]
    fn detects_anim_variants() {
        assert_eq!(detect(b"r3d2anmd"), FileKind::AnimUncompressed);
        assert_eq!(detect(b"r3d2canm"), FileKind::AnimCompressed);
    }

    #[test]
    fn detects_static_mesh_binary() {
        assert_eq!(detect(b"r3d2Mesh"), FileKind::StaticMeshBinary);
    }

    #[test]
    fn detects_static_mesh_text() {
        assert_eq!(detect(b"[ObjectBegin]\r\n"), FileKind::StaticMeshText);
    }

    #[test]
    fn detects_wpk_distinct_from_longer_r3d2_tags() {
        let bytes = [b'r', b'3', b'd', b'2', 0x01, 0x00, 0x00, 0x00];
        assert_eq!(detect(&bytes), FileKind::Wpk);
    }

    #[test]
    fn detects_dds_rst_rman_bnk() {
        assert_eq!(detect(b"DDS \x00\x00\x00\x00"), FileKind::Dds);
        assert_eq!(detect(b"RST\x05"), FileKind::Rst);
        assert_eq!(detect(b"RMAN\x00\x00\x00\x00"), FileKind::Rman);
        assert_eq!(detect(b"BKHD\x00\x00\x00\x00"), FileKind::Bnk);
    }

    #[test]
    fn empty_slice_is_unknown() {
        assert_eq!(detect(&[]), FileKind::Unknown);
    }
}
