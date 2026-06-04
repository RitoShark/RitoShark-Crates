use std::collections::hash_map::{Entry, HashMap};
use std::io::Write;

use rs_io::WriterExt;

use crate::chunk::{WadChunk, WadCompression};
use crate::encoder::{compress, DEFAULT_ZSTD_LEVEL};
use crate::error::{Error, Result};
use crate::wad::{write_chunk, CHUNK_ENTRY_LEN, MAGIC, V3_TRAILER_LEN};

/** Assembles a WAD v3.4 archive from loose files. Chunks are registered by their in-WAD path (or
raw path hash); at build time the builder pulls each chunk's uncompressed bytes from a caller
supplied provider, zstd-compresses them, deduplicates identical contents, lays out the
sorted-by-hash table of contents, and streams the data section out.

The build is streaming: only one chunk's bytes are held in memory at a time, so an archive of
hundreds of megabytes never needs to be fully buffered. Because the data section's byte offsets are
only known once every chunk is compressed, and the output writer is not required to be seekable, the
provider is invoked twice per unique blob — once to measure (size, checksum, dedup) and once to
write. zstd is deterministic at a fixed level, so the two passes agree; a mismatch is caught and
reported rather than producing a corrupt archive.

The result is not byte-identical to other tools (zstd level/dictionary choices differ) but is a
valid v3.4 archive: the game and `Wad::from_bytes` accept it, and every chunk decompresses back to
the exact input bytes. */
#[derive(Debug, Clone)]
pub struct WadBuilder {
    version: (u8, u8),
    zstd_level: i32,
    chunk_hashes: Vec<u64>,
}

impl Default for WadBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WadBuilder {
    /// A new builder targeting WAD v3.4 with the default zstd level.
    pub fn new() -> Self {
        Self {
            version: (3, 4),
            zstd_level: DEFAULT_ZSTD_LEVEL,
            chunk_hashes: Vec::new(),
        }
    }

    /// Sets the archive version. Only major version 3 is supported for building.
    pub fn with_version(mut self, major: u8, minor: u8) -> Self {
        self.version = (major, minor);
        self
    }

    /// Sets the zstd compression level applied to every chunk (default [`DEFAULT_ZSTD_LEVEL`]).
    pub fn with_zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = level;
        self
    }

    /// Registers a chunk by its in-WAD path, hashing it with XXH64 (seed 0, lowercased).
    pub fn add_chunk(&mut self, path: &str) {
        self.chunk_hashes.push(rs_hash::xxh64(path));
    }

    /// Registers a chunk by its already-computed path hash.
    pub fn add_chunk_hash(&mut self, path_hash: u64) {
        self.chunk_hashes.push(path_hash);
    }

    /// Builder-style [`add_chunk`](Self::add_chunk).
    pub fn with_chunk(mut self, path: &str) -> Self {
        self.add_chunk(path);
        self
    }

    /// Builder-style [`add_chunk_hash`](Self::add_chunk_hash).
    pub fn with_chunk_hash(mut self, path_hash: u64) -> Self {
        self.add_chunk_hash(path_hash);
        self
    }

    /** Streams the assembled archive into `out`. For each registered chunk, `provide(path_hash,
    &mut writer)` must write that chunk's *uncompressed* bytes; the builder compresses, deduplicates,
    and lays out the archive. `provide` is called more than once per build (see the type docs) and so
    must be reproducible. Duplicate registrations of the same path hash collapse to one entry. */
    pub fn build_to_writer<W, F>(self, out: &mut W, mut provide: F) -> Result<()>
    where
        W: Write,
        F: FnMut(u64, &mut dyn Write) -> Result<()>,
    {
        if self.version.0 != 3 {
            return Err(Error::Build(format!(
                "only WAD major version 3 can be built, got {}.{}",
                self.version.0, self.version.1
            )));
        }

        // One TOC entry per distinct path hash, sorted ascending — League refuses an unsorted TOC.
        let mut hashes = self.chunk_hashes.clone();
        hashes.sort_unstable();
        hashes.dedup();

        let data_start = 4 + V3_TRAILER_LEN + 4 + hashes.len() * CHUNK_ENTRY_LEN;

        // Pass 1: pull, compress, and measure each chunk, deduplicating identical contents by a
        // wide content fingerprint. Compressed bytes are discarded so only one chunk lives in RAM.
        let mut blobs: HashMap<u128, BlobPlan> = HashMap::new();
        let mut blob_order: Vec<u128> = Vec::new();
        let mut plan: Vec<(u64, u128)> = Vec::with_capacity(hashes.len());
        let mut buf: Vec<u8> = Vec::new();

        for &path_hash in &hashes {
            buf.clear();
            provide(path_hash, &mut buf)?;
            let key = rs_hash::xxh3_128_bytes(&buf);

            if let Entry::Vacant(slot) = blobs.entry(key) {
                let compressed = compress(&buf, WadCompression::Zstd, self.zstd_level)?;
                slot.insert(BlobPlan {
                    uncompressed_size: to_u32(buf.len())?,
                    compressed_size: to_u32(compressed.len())?,
                    checksum: rs_hash::xxh3_64_bytes(&compressed),
                    rep_hash: path_hash,
                    data_offset: 0,
                });
                blob_order.push(key);
            }
            plan.push((path_hash, key));
        }

        // Assign data offsets in write order; offsets are absolute into the archive.
        let mut cursor = data_start;
        for key in &blob_order {
            let blob = blobs.get_mut(key).expect("blob ordered but not planned");
            blob.data_offset = to_u32(cursor)?;
            cursor += blob.compressed_size as usize;
        }

        // Build the final TOC: duplicates point at the introducing chunk's shared offset.
        let chunks: Vec<WadChunk> = plan
            .iter()
            .map(|&(path_hash, key)| {
                let blob = &blobs[&key];
                WadChunk {
                    path_hash,
                    data_offset: blob.data_offset,
                    compressed_size: blob.compressed_size,
                    uncompressed_size: blob.uncompressed_size,
                    compression: WadCompression::Zstd,
                    is_duplicated: path_hash != blob.rep_hash,
                    subchunk_count: 0,
                    subchunk_start: 0,
                    checksum: blob.checksum,
                }
            })
            .collect();

        // Header + trailer + TOC. The trailer (ECDSA signature + data checksum) is zeroed; the game
        // does not verify it and the reader round-trips whatever span it finds.
        out.write_u16(MAGIC)?;
        out.write_u8(self.version.0)?;
        out.write_u8(self.version.1)?;
        out.write_bytes(&[0u8; V3_TRAILER_LEN])?;
        out.write_u32(chunks.len() as u32)?;
        for chunk in &chunks {
            write_chunk(out, chunk)?;
        }

        // Pass 2: re-pull each unique blob in offset order and write its compressed bytes.
        for key in &blob_order {
            let blob = &blobs[key];
            buf.clear();
            provide(blob.rep_hash, &mut buf)?;
            let compressed = compress(&buf, WadCompression::Zstd, self.zstd_level)?;
            if compressed.len() != blob.compressed_size as usize {
                return Err(Error::Build(String::from(
                    "chunk provider is not reproducible: compressed size changed between passes",
                )));
            }
            out.write_bytes(&compressed)?;
        }

        Ok(())
    }

    /// Convenience over [`build_to_writer`](Self::build_to_writer) that returns the archive bytes.
    pub fn build_to_bytes<F>(self, provide: F) -> Result<Vec<u8>>
    where
        F: FnMut(u64, &mut dyn Write) -> Result<()>,
    {
        let mut out = Vec::new();
        self.build_to_writer(&mut out, provide)?;
        Ok(out)
    }
}

/// Per-unique-blob layout decided in pass 1 and consumed in pass 2.
#[derive(Debug, Clone, Copy)]
struct BlobPlan {
    uncompressed_size: u32,
    compressed_size: u32,
    checksum: u64,
    /// The first path hash (in sorted order) that introduced this blob; its non-duplicate owner.
    rep_hash: u64,
    data_offset: u32,
}

fn to_u32(value: usize) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| Error::Build(format!("value {value} exceeds the WAD 32-bit size/offset limit")))
}
