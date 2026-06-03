# rs_wad

Reads and writes League of Legends `.wad` / `.wad.client` archives with a byte-exact
round-trip guarantee.

## What a WAD is

A WAD is a flat archive: a header, a fixed-size table of contents (TOC), and a packed data
section. Each TOC entry names one file by its **XXH64** path hash (lowercased path, seed 0) and
points at a byte range inside the data section, possibly compressed. There are no directory
records; file names live only as hashes and are resolved externally against a hash dictionary.

### Header

All integers are little-endian.

| Field | Size | Notes |
|---|---|---|
| magic | 2 | `"RW"` (`0x5752`) |
| major | 1 | supported: `2`, `3` |
| minor | 1 | `0`–`4` seen in the wild |

After the version comes a version-specific header span that this crate captures verbatim:

- **v2**: `ecdsaLength` (1) + ECDSA signature (83) + data checksum (8) + `tocStartOffset` (2) +
  `tocFileEntrySize` (2) = 96 bytes.
- **v3**: ECDSA signature (256) + data checksum (8) = 264 bytes. The full v3 header is therefore
  272 bytes (`4 + 264 + 4` including the magic/version and the trailing chunk count).

Then a `u32` chunk count, followed by that many 32-byte TOC entries.

### TOC entry (32 bytes, identical for every v3 minor)

| Field | Size | Notes |
|---|---|---|
| path hash | 8 | XXH64 of the lowercased path |
| data offset | 4 | absolute byte offset into the file |
| compressed size | 4 | bytes occupied in the data section |
| uncompressed size | 4 | size after decompression |
| type / subchunk count | 1 | low nibble = compression, high nibble = subchunk count |
| is duplicated | 1 | non-zero if this chunk's data is shared with an earlier chunk |
| first subchunk index | 2 | `u16`, only meaningful for the zstd-multi format |
| checksum | 8 | first 8 bytes of the chunk's XXH3-64 |

Every v3 minor (including **v3.4**) uses exactly this layout. There is no 24-bit subchunk field;
the duplicate flag and the 16-bit first-subchunk index are always present.

### Compression types (low nibble of the type byte)

| Value | Name | Supported |
|---|---|---|
| 0 | None (stored) | yes |
| 1 | Gzip | yes (flate2) |
| 2 | Satellite (string redirect) | no — returns an error |
| 3 | Zstd | yes |
| 4 | ZstdMulti (split sub-chunks) | yes |

**ZstdMulti** stores one independent zstd frame per sub-chunk, concatenated end to end
(optionally preceded by a stored prefix). The crate copies any pre-frame bytes verbatim and then
streams the concatenated frames in order; the zstd reader decodes them one after another until the
input is exhausted, so the external `.subchunktoc` table is not required to reassemble the chunk.

## API

The type implements the workspace `Parse` / `Serialize` traits, so it gains the standard
convenience methods.

```rust
use rs_io::{Parse, Serialize};
use rs_wad::Wad;

// Mount / parse the TOC (mmaps the file via from_path).
let wad = Wad::from_path("Azir.wad.client")?;
let wad = Wad::from_bytes(&bytes)?;

// List chunks.
println!("v{}.{}, {} chunks", wad.version.0, wad.version.1, wad.chunks.len());
for chunk in &wad.chunks {
    println!("{:016x} {:?} {} -> {}",
        chunk.path_hash, chunk.compression,
        chunk.compressed_size, chunk.uncompressed_size);
}

// Extract + decompress one chunk.
let bytes = wad.chunk_data(&wad.chunks[0])?;

// Get the still-compressed bytes without decoding.
let raw = wad.chunk_raw(&wad.chunks[0])?;

// Byte-exact write back.
wad.to_path("out.wad.client")?;
```

`rs_wad::decompress(raw, compression, uncompressed_size)` decodes a raw chunk payload directly
without a `Wad`.

### Round-trip contract

`Wad` keeps the version-specific header span and the entire data section verbatim, and chunk
offsets are absolute, so `read → write` reproduces the input file byte-for-byte. This is enforced
by `tests/roundtrip.rs` (synthetic v3.3 and v3.4 archives) and `tests/real_files.rs` (the real
sample archives).

## Supported versions

- **v2** and **v3** (all minors, including **v3.4**). Other major versions return
  `Error::UnsupportedVersion`.

## Tests / fixtures

Real game archives are gitignored. Drop `.wad.client` files into `../../sample-files/` (workspace
`sample-files/`); `tests/real_files.rs` skips automatically when a sample is absent. See
`docs/real-files-report.md` for measured results and cross-library comparison.
