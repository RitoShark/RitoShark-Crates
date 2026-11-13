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

### TOC entry (32 bytes)

| Field | Size | Notes |
|---|---|---|
| path hash | 8 | XXH64 of the lowercased path |
| data offset | 4 | absolute byte offset into the file |
| compressed size | 4 | bytes occupied in the data section |
| uncompressed size | 4 | size after decompression |
| type / subchunk count | 1 | low nibble = compression, high nibble = subchunk count |
| duplicate flag / subchunk index | 3 | layout depends on the version; see below |
| checksum | 8 | first 8 bytes of the chunk's XXH3-64 |

Before v3.4, those three bytes contain a duplicate flag followed by a little-endian
16-bit first-subchunk index. In **v3.4+**, all three encode a 24-bit first-subchunk
index in high/low/middle byte order, with no duplicate flag. The checksum remains
at the same offset. `tests/builder.rs` covers both layouts.

### Compression types (low nibble of the type byte)

| Value | Name | Supported |
|---|---|---|
| 0 | None (stored) | yes |
| 1 | Gzip | yes (flate2) |
| 2 | Satellite (string redirect) | no — returns an error |
| 3 | Zstd | yes |
| 4 | ZstdMulti (split sub-chunks) | yes |

**ZstdMulti** stores one independent zstd frame per sub-chunk, concatenated end to end
(optionally preceded by a stored prefix). Two decode paths are provided:

- **Streaming heuristic** (`chunk_data`): copy any pre-frame bytes verbatim, then stream the
  concatenated frames in order, letting the zstd reader decode them one after another until the
  input is exhausted. No external table is needed; it assumes the data section is exactly the
  concatenated frames (true for every real chunk tested).
- **Explicit `.subchunktoc`** (`chunk_data_with_toc`): size each sub-chunk from a parsed
  `.subchunktoc` table, matching the C# oracle exactly. This handles arbitrary layouts (e.g. a
  stored sub-chunk between two compressed frames) that the heuristic cannot.

### The `.subchunktoc`

The sub-chunk table is itself a chunk inside the WAD — a file whose lowercased path ends in
`.subchunktoc` (the base name comes from the WAD's own path under `Game/`, so the caller supplies
it). Its decompressed body is an array of 16-byte entries:

| Field | Size | Notes |
|---|---|---|
| compressed size | 4 | sub-chunk bytes in the parent chunk's data section |
| uncompressed size | 4 | sub-chunk size after decoding (equal to compressed ⇒ stored) |
| checksum | 8 | sub-chunk's XXH3-64 |

A zstd-multi chunk's `first subchunk index` + `subchunk count` select its run of this table.

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

### Lookup

```rust
// By path hash (XXH64 of the lowercased path).
let chunk = wad.chunk_by_hash(0x431712194fe05916);

// By path: hashed for you via XXH64(lowercased).
let chunk = wad.chunk_by_path("data/final/champions/azir.skl");
```

Both return `Option<&WadChunk>` and never panic on a missing key.

These one-off lookups scan the table. For repeated lookups, build a borrowed index once:

```rust
let index = wad.chunk_index();
if let Some(chunk) = index.get(&0x431712194fe05916) {
    let bytes = wad.chunk_data(chunk)?;
}
```

`chunk_index()` takes O(n) time and memory, then provides expected O(1) lookups. It
works on unsorted tables and TOC-only mounts. Duplicate hashes select the first
entry, matching `chunk_by_hash`. The references borrow the archive, preventing
table mutations while the index is in use; rebuild it after editing the table.

### Bulk extraction

```rust
use std::collections::HashMap;

// Decompress every chunk: path-hash -> decompressed bytes.
let all: HashMap<u64, Vec<u8>> = wad.extract_all()?;

// Decompress a chosen subset (unknown hashes are skipped).
let some = wad.extract_selected([0x431712194fe05916, 0x0123_4567_89AB_CDEF])?;
```

Enable the `parallel` feature to decode chunks across a thread pool; the API is identical and the
crate stays `#![forbid(unsafe_code)]`.

Selection takes expected O(n + m) time for n table entries and m requested hashes,
plus decoding. Each selected hash is decoded once; repeated requests are collapsed,
unknown hashes are skipped, and duplicate table hashes use their first entry.
`extract_all()` uses the same selection path and avoids repeated table searches.

### Explicit sub-chunk table

```rust
// Parse the archive's .subchunktoc (caller supplies the lowercased path).
if let Some(toc) = wad.subchunk_toc_for_path("data/final/champions/azir.wad.subchunktoc")? {
    // Decode a zstd-multi chunk with explicit per-sub-chunk sizes (oracle-exact).
    let bytes = wad.chunk_data_with_toc(&wad.chunks[0], &toc)?;
}
```

`chunk_data_with_toc` falls back to the normal path for non-multi chunks, so it is a safe drop-in
for `chunk_data` whenever a TOC is available.

`rs_wad::decompress(raw, compression, uncompressed_size)` decodes a raw chunk payload directly
without a `Wad`; `rs_wad::decompress_zstd_multi_with_toc(raw, uncompressed_size, &subchunks)`
decodes a multi chunk from an explicit sub-chunk slice.

### Round-trip contract

`Wad` keeps the version-specific header span and the entire data section verbatim, and chunk
offsets are absolute, so `read → write` reproduces the input file byte-for-byte. This is enforced
by `tests/roundtrip.rs` (synthetic v3.3 and v3.4 archives) and `tests/real_files.rs` (the real
sample archives).

## Supported versions

- **v2** and **v3** (all minors, including **v3.4**). Other major versions return
  `Error::UnsupportedVersion`.

## Tests / fixtures

Both full and TOC-only readers check that the declared chunk table fits in the
remaining stream before allocating its entries. TOC-only mounts still leave payloads
on disk. See the [test and performance baseline](../../docs/testing.md#wad-performance-baseline)
for synthetic mutation checks and the asset-free lookup/extraction benchmark.

Real game archives are gitignored. Drop `.wad.client` files into `../../Sample-Files/` (workspace
`Sample-Files/`); `tests/real_files.rs` skips automatically when a sample is absent. See
`docs/real-files-report.md` for measured results and cross-library comparison.
