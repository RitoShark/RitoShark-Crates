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

Public fields expose the parsed structure directly: `version`, `flags`, `manifest_id`,
`bundles` (each with `chunks`), `files` (`name`, `size`, `directory_id`, `chunk_ids`,
`link`, `permissions`), and `directories` (`id`, `parent_id`, `name`).

## Writing

**Not implemented.** RMAN is consumed read-only by our tooling, and the format is a
download descriptor rather than an editable asset, so `Serialize::to_writer` returns
`Error::Unsupported`. Rebuilding a manifest (FlatBuffer emission plus re-bundling)
would be a large, separate effort; see `docs/real-files-report.md`.

## Fixtures

Real `.manifest` files are copyrighted game data and are **gitignored**. Drop them in
the workspace `sample-files/` directory; `tests/real_files.rs` skips cleanly when they
are absent.
