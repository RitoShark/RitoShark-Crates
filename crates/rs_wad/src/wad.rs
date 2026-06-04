use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};

use rs_io::{ReaderExt, WriterExt};

use crate::chunk::{WadChunk, WadCompression, WadSubchunk};
use crate::decoder::{decompress, decompress_zstd_multi_with_toc};
use crate::error::{Error, Result};

pub(crate) const MAGIC: u16 = 0x5752;
pub(crate) const CHUNK_ENTRY_LEN: usize = 32;
pub(crate) const V3_TRAILER_LEN: usize = 256 + 8;
const V2_TRAILER_LEN: usize = 1 + 83 + 8 + 2 + 2;

/** A mounted WAD archive: the `version` major/minor, the parsed chunk table, the verbatim header
bytes that sit between the version and the chunk count (ECDSA signature and data checksum), and the
verbatim data section that follows the table. Keeping both raw spans lets the writer reproduce the
file byte-for-byte, since chunk `data_offset`s are absolute and the header and table keep their
size across a round-trip. */
#[derive(Debug, Clone)]
pub struct Wad {
    pub version: (u8, u8),
    pub chunks: Vec<WadChunk>,
    pub header_trailer: Vec<u8>,
    pub data: Vec<u8>,
}

impl Wad {
    /** Parses the header and chunk table from `reader`, then captures the remaining data section
    so the archive can be written back unchanged. Supports versions 2 and 3; any other major
    yields [`Error::UnsupportedVersion`]. The data section is read from the end of the table to the
    end of the stream, so chunk byte ranges stay addressable by their absolute offsets. */
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u16()?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic);
        }
        let major = reader.read_u8()?;
        let minor = reader.read_u8()?;

        let trailer_len = match major {
            2 => V2_TRAILER_LEN,
            3 => V3_TRAILER_LEN,
            _ => return Err(Error::UnsupportedVersion(major, minor)),
        };
        let header_trailer = reader.read_bytes(trailer_len)?;

        let chunk_count = reader.read_u32()? as usize;
        let mut chunks = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            chunks.push(read_chunk(reader)?);
        }

        let data = read_to_end(reader)?;

        Ok(Self {
            version: (major, minor),
            chunks,
            header_trailer,
            data,
        })
    }

    /// Writes the header, chunk table, and data section, reproducing the input bytes exactly.
    pub fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16(MAGIC)?;
        writer.write_u8(self.version.0)?;
        writer.write_u8(self.version.1)?;
        writer.write_bytes(&self.header_trailer)?;
        writer.write_u32(self.chunks.len() as u32)?;
        for chunk in &self.chunks {
            write_chunk(writer, chunk)?;
        }
        writer.write_bytes(&self.data)?;
        Ok(())
    }

    /// Byte offset of the data section, i.e. the end of the chunk table.
    pub fn data_start(&self) -> u64 {
        (4 + self.header_trailer.len() + 4 + self.chunks.len() * CHUNK_ENTRY_LEN) as u64
    }

    /// Returns the raw (still-compressed) bytes of `chunk` from the captured data section.
    pub fn chunk_raw<'a>(&'a self, chunk: &WadChunk) -> Result<&'a [u8]> {
        let start = (chunk.data_offset as u64)
            .checked_sub(self.data_start())
            .ok_or_else(|| Error::Decompress(String::from("chunk offset precedes data section")))?
            as usize;
        let end = start
            .checked_add(chunk.compressed_size as usize)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| Error::Decompress(String::from("chunk range exceeds data section")))?;
        Ok(&self.data[start..end])
    }

    /// Reads and decompresses `chunk` from the captured data section.
    pub fn chunk_data(&self, chunk: &WadChunk) -> Result<Vec<u8>> {
        let raw = self.chunk_raw(chunk)?;
        decompress(raw, chunk.compression, chunk.uncompressed_size as usize)
    }

    /// Returns the chunk whose path hash equals `path_hash`, if present.
    pub fn chunk_by_hash(&self, path_hash: u64) -> Option<&WadChunk> {
        self.chunks.iter().find(|c| c.path_hash == path_hash)
    }

    /** Returns the chunk for `path`, hashing the lowercased path with XXH64 (seed 0) — the same
    convention the archive uses for its path hashes — then looking it up by hash. */
    pub fn chunk_by_path(&self, path: &str) -> Option<&WadChunk> {
        self.chunk_by_hash(rs_hash::xxh64(path))
    }

    /** Parses the archive's `.subchunktoc` chunk into the explicit per-sub-chunk size table that
    [`WadCompression::ZstdMulti`] chunks index by `subchunk_start`/`subchunk_count`. The chunk is
    located by the XXH64 of the lowercased path ending in `.subchunktoc`; because the archive only
    stores hashes, the path is supplied by the caller (its base name varies by archive). Returns
    `Ok(None)` when no chunk matches that path. Each on-disk entry is 16 bytes. */
    pub fn subchunk_toc_for_path(&self, subchunktoc_path: &str) -> Result<Option<Vec<WadSubchunk>>> {
        match self.chunk_by_path(subchunktoc_path) {
            Some(chunk) => Ok(Some(self.parse_subchunk_toc(chunk)?)),
            None => Ok(None),
        }
    }

    /// Parses the `.subchunktoc` `chunk` itself (already located) into its sub-chunk size table.
    pub fn parse_subchunk_toc(&self, chunk: &WadChunk) -> Result<Vec<WadSubchunk>> {
        let bytes = self.chunk_data(chunk)?;
        if bytes.len() % WadSubchunk::ENTRY_LEN != 0 {
            return Err(Error::Decompress(String::from(
                "subchunktoc length is not a multiple of the 16-byte entry size",
            )));
        }
        let mut toc = Vec::with_capacity(bytes.len() / WadSubchunk::ENTRY_LEN);
        for entry in bytes.chunks_exact(WadSubchunk::ENTRY_LEN) {
            let compressed_size = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]);
            let uncompressed_size = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]);
            let checksum = u64::from_le_bytes([
                entry[8], entry[9], entry[10], entry[11], entry[12], entry[13], entry[14], entry[15],
            ]);
            toc.push(WadSubchunk {
                compressed_size,
                uncompressed_size,
                checksum,
            });
        }
        Ok(toc)
    }

    /** Decompresses `chunk` using an explicit sub-chunk table for [`WadCompression::ZstdMulti`]
    chunks. The `subchunk_toc` is the full table parsed by [`Wad::parse_subchunk_toc`]; the chunk's
    `subchunk_start`/`subchunk_count` slice its own run out of it. Non-multi chunks ignore the table
    and decode normally, so this is a safe drop-in for [`Wad::chunk_data`] when a TOC is available. */
    pub fn chunk_data_with_toc(
        &self,
        chunk: &WadChunk,
        subchunk_toc: &[WadSubchunk],
    ) -> Result<Vec<u8>> {
        if chunk.compression != WadCompression::ZstdMulti {
            return self.chunk_data(chunk);
        }
        let start = chunk.subchunk_start as usize;
        let end = start
            .checked_add(chunk.subchunk_count as usize)
            .filter(|&end| end <= subchunk_toc.len())
            .ok_or_else(|| {
                Error::Decompress(String::from("chunk subchunk range exceeds the subchunk toc"))
            })?;
        let raw = self.chunk_raw(chunk)?;
        decompress_zstd_multi_with_toc(raw, chunk.uncompressed_size as usize, &subchunk_toc[start..end])
    }

    /** Extracts and decompresses every chunk, returning a map from path hash to decompressed bytes.
    Duplicated chunks share data and resolve to identical bytes. When the `parallel` feature is on
    the chunks decode across a thread pool; otherwise the work is sequential. */
    pub fn extract_all(&self) -> Result<HashMap<u64, Vec<u8>>> {
        self.extract_selected(self.chunks.iter().map(|c| c.path_hash))
    }

    /** Extracts and decompresses the chunks named by `path_hashes`, returning a map from path hash
    to decompressed bytes. Unknown hashes are skipped. When the `parallel` feature is on the chunks
    decode across a thread pool; otherwise the work is sequential. */
    pub fn extract_selected(
        &self,
        path_hashes: impl IntoIterator<Item = u64>,
    ) -> Result<HashMap<u64, Vec<u8>>> {
        let targets: Vec<&WadChunk> = path_hashes
            .into_iter()
            .filter_map(|h| self.chunk_by_hash(h))
            .collect();

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            targets
                .par_iter()
                .map(|c| Ok((c.path_hash, self.chunk_data(c)?)))
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            let mut out = HashMap::with_capacity(targets.len());
            for c in targets {
                out.insert(c.path_hash, self.chunk_data(c)?);
            }
            Ok(out)
        }
    }
}

fn read_chunk<R: Read>(reader: &mut R) -> Result<WadChunk> {
    let path_hash = reader.read_u64()?;
    let data_offset = reader.read_u32()?;
    let compressed_size = reader.read_u32()?;
    let uncompressed_size = reader.read_u32()?;

    let type_subchunk = reader.read_u8()?;
    let subchunk_count = type_subchunk >> 4;
    let compression = WadCompression::from_u8(type_subchunk & 0x0F)
        .ok_or(Error::UnsupportedCompression(type_subchunk & 0x0F))?;

    let is_duplicated = reader.read_u8()? != 0;
    let subchunk_start = reader.read_u16()? as u32;

    let checksum = reader.read_u64()?;

    Ok(WadChunk {
        path_hash,
        data_offset,
        compressed_size,
        uncompressed_size,
        compression,
        is_duplicated,
        subchunk_count,
        subchunk_start,
        checksum,
    })
}

pub(crate) fn write_chunk<W: Write>(writer: &mut W, chunk: &WadChunk) -> Result<()> {
    writer.write_u64(chunk.path_hash)?;
    writer.write_u32(chunk.data_offset)?;
    writer.write_u32(chunk.compressed_size)?;
    writer.write_u32(chunk.uncompressed_size)?;

    let type_subchunk = (chunk.subchunk_count << 4) | (chunk.compression as u8 & 0x0F);
    writer.write_u8(type_subchunk)?;

    writer.write_u8(chunk.is_duplicated as u8)?;
    writer.write_u16(chunk.subchunk_start as u16)?;

    writer.write_u64(chunk.checksum)?;
    Ok(())
}

fn read_to_end<R: Read + Seek>(reader: &mut R) -> Result<Vec<u8>> {
    let pos = reader.stream_position().map_err(rs_io::Error::from)?;
    let end = reader.seek(SeekFrom::End(0)).map_err(rs_io::Error::from)?;
    reader
        .seek(SeekFrom::Start(pos))
        .map_err(rs_io::Error::from)?;
    let mut buf = Vec::with_capacity((end - pos) as usize);
    reader.read_to_end(&mut buf).map_err(rs_io::Error::from)?;
    Ok(buf)
}
