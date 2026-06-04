# rs_rman

Reader for the League of Legends **RMAN** release-manifest format (`.manifest`).

An RMAN file describes a game release: the **bundles** and **chunks** that hold the
compressed payload on the CDN, the **files** that make up the install, and the
**directory** tree those files live in. `rs_rman` parses the header, decompresses the
zstd body, and walks the body's FlatBuffer-style offset tables into owned Rust values.

## Format

```
Header (28 bytes, little-endian):
  magic              "RMAN"        (4 bytes)
  version major      u8            (2)
  version minor      u8            (0 or 1 in the wild)
  flags              u16           (bit 9 set on game manifests)
  body offset        u32           (28; bytes from start to the zstd body)
  compressed size    u32           (length of the zstd body)
  manifest id        u64           (matches the filename, e.g. 7D6C65378829C6AA)
  uncompressed size  u32

Body (zstd-compressed FlatBuffer):
  i32 header length + that many skipped bytes
  four self-relative table offsets: bundles, flags, files, directories
  each table: u32 count, then count self-relative entry offsets
  each entry: a vtable (self-relative i32) + indexed fields
```

The **flags table** lists the locale/platform tags a release ships (`en_US`, `ko_KR`,
`windows`, `macos`, ...), each with a small numeric id. Every file may carry a `u64`
flags mask whose set bit `1 << id` selects the matching tag, which is how a single
manifest serves many locales: tooling filters files down to the locale/platform it wants.

Each chunk lives at a running compressed byte offset inside its bundle (chunks are
concatenated in bundle order). A file is rebuilt by decompressing its ordered chunks and
concatenating the results; the uncompressed sizes sum to the file's declared size.

Files are reassembled by concatenating the decompressed contents of their ordered
chunk ids; chunks live inside bundles. Full file paths are reconstructed by joining a
file's basename onto its directory's chain of parents up to the root.

## Supported versions

RMAN **major version 2** (the only major Riot has shipped). Both minor `2.0` and the
current `2.1` game manifests parse identically — the body layout is unchanged across
minor versions, so the reader accepts any `2.x` and records the exact minor it saw.
A non-2 major version is rejected with `Error::UnsupportedVersion`.

## API

```rust
use rs_io::Parse;
use rs_rman::Rman;

let rman = Rman::from_path("7D6C65378829C6AA.manifest")?;
assert_eq!(rman.version.0, 2);

// Full (path, extracted-size) pairs for every file in the release.
for (path, size) in rman.file_paths() {
    println!("{path}  ({size} bytes)");
}
```

`Rman` implements the workspace `Parse` trait, giving the standard constructors:

| Method | Purpose |
|---|---|
| `Rman::from_reader(&mut r)` | parse from any `Read + Seek` |
| `Rman::from_bytes(&[u8])` | parse from an in-memory slice |
| `Rman::from_path(path)` | mmap a file and parse it |
| `rman.file_paths() -> Vec<(String, u64)>` | full file paths with extracted sizes |

### Flags (locale / platform)

| Method | Purpose |
|---|---|
| `rman.file_flags` | the parsed flags table: `Vec<FileFlag { id, name }>` |
| `file.flags_mask` | `Option<u64>` bitmask referencing flag ids via `1 << id` |
| `rman.file_flag_names(&file) -> Vec<&str>` | the tags active on one file |
| `rman.files_with_flag("en_US") -> Vec<&FileEntry>` | filter files by a locale/platform tag |

### Chunk byte-ranges (the gateway to extraction)

| Method | Purpose |
|---|---|
| `rman.file_chunks(&file) -> Vec<ChunkRange>` | ordered chunks rebuilding the file |
| `rman.chunk_index() -> HashMap<u64, ChunkRange>` | build the chunk lookup once |
| `Rman::file_chunks_for(&file, &index)` | resolve a file against a prebuilt index |

```rust
for c in rman.file_chunks(&file) {
    // download bytes [offset_in_bundle .. offset_in_bundle + compressed_size]
    // from bundle `c.bundle_id`, decompress to `c.uncompressed_size` bytes.
}
```

`ChunkRange` carries `bundle_id`, `chunk_id`, `offset_in_bundle`, `compressed_size`,
`uncompressed_size`. Chunks are returned in file order; their `uncompressed_size`s sum to
`file.size`. For bulk work build `chunk_index()` once and reuse it via `file_chunks_for`.

Public fields expose the parsed structure directly: `version`, `flags`, `manifest_id`,
`bundles` (each with `chunks`), `files` (`name`, `size`, `directory_id`, `chunk_ids`,
`link`, `permissions`, `flags_mask`, `extra`), `directories` (`id`, `parent_id`, `name`),
and `file_flags`.

## Writing

**Supported**, via the workspace `Serialize` trait (`to_writer` / `to_bytes` / `to_path`).
The writer rebuilds the FlatBuffer body from the owned model — every table and field index
the reader walks (bundles + chunks, the flags table, files, directories) — compresses it,
and re-emits the 28-byte header.

```rust
use rs_io::{Parse, Serialize};
use rs_rman::Rman;

let rman = Rman::from_path("7D6C65378829C6AA.manifest")?;
let bytes = rman.to_bytes()?;          // valid RMAN container
let again = Rman::from_bytes(&bytes)?; // identical logical model
assert_eq!(rman, again);
```

### Round-trip contract: semantic, not byte-exact

Unlike the other format crates, RMAN's contract is a **semantic** round-trip, not a
byte-identical one: `read → write → read` yields an identical logical `Rman` (bundles,
files — including the preserved uninterpreted fields — directories, flags, and
`file_paths()`). Byte-exact reproduction of Riot's own output is **intentionally not
attempted** because two layers are non-deterministic encoder choices we do not mirror:

1. **zstd compression.** The body is zstd-compressed; our compressor and Riot's produce
   different (both valid) byte streams for the same input.
2. **FlatBuffer layout.** Field packing order, vtable sharing, and alignment padding are
   free choices in the FlatBuffer encoding; many distinct bodies decode to the same model.

The tests therefore assert model equality on the three real manifests, not byte equality.

### Preserved uninterpreted fields

File entries carry FlatBuffer fields the reader does not interpret (indices 5, 6, 8, 10,
11). On real manifests field 11 (a `u16`, the localized-WAD marker) is present on hundreds
of files. These are read into `FileEntry::extra` (`FileExtra`) and re-emitted verbatim so a
write loses nothing.

## Fixtures

Real `.manifest` files are copyrighted game data and are **gitignored**. Drop them in
the workspace `sample-files/` directory; `tests/real_files.rs` skips cleanly when they
are absent.
