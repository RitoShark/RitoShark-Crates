# RitoShark

A Rust workspace for reading and writing League of Legends game file formats.

RitoShark is built around three goals: **correctness** (lossless, byte-exact round-trips on real
game files), **consistency** (every format crate has the same shape and API), and **performance
without unsafe** (memory-mapped reads, single-pass parsing, pre-sized buffers). It is organized as
small, focused per-format crates behind a single umbrella crate, so you can depend on exactly what
you need.

```rust
use ritoshark::prelude::*;          // Parse / Serialize / ReaderExt / WriterExt
use ritoshark::bin::Bin;

let bin = Bin::from_path("Aatrox.bin")?;   // mmaps + parses
let text = ritoshark::bin::to_text(&bin, None); // #PROP_text
let bytes = bin.to_bytes()?;               // byte-exact re-encode
```

## Format support

| Format | Ext | Crate | Read | Write | Notes |
|---|---|---|:--:|:--:|---|
| Property bin | `.bin` | `rs_bin` | ✅ | ✅ | PROP + PTCH; binary **and** `#PROP_text` (parse + print) |
| WAD archive | `.wad`, `.wad.client` | `rs_wad` | ✅ | ✅ | zstd / zstd-multi / gzip; lookup, bulk extract, subchunk TOC |
| Texture | `.tex`, `.dds` | `rs_tex` | ✅ | ✅ | decode BC1–BC7 / ETC / RGBA; encode BC1/3/5/7 + mips; cubemaps |
| Skinned mesh | `.skn` | `rs_mesh` | ✅ | ✅ | versions 1 / 2 / 4 |
| Static mesh | `.scb` | `rs_mesh` | ✅ | ✅ | lossless incl. trailing per-face data |
| Skeleton | `.skl` | `rs_anim` | ✅ | ✅ | modern rig format |
| Animation | `.anm` | `rs_anim` | ✅ | ✅ | uncompressed v3/4/5 + compressed `r3d2canm` |
| Map geometry | `.mapgeo` | `rs_mapgeo` | ✅ | ✅ | OEGM versions 5–7, 9, 11–15, 17, 18 |
| String table | `.stringtable` | `rs_rst` | ✅ | ✅ | RST v2–v5 |
| Release manifest | `.manifest` | `rs_rman` | ✅ | — | read-only by design (manifests are server-authored) |
| Audio container | `.wpk`, `.bnk` | `rs_audio` | ✅ | ✅ | container-level: extract / repack `.wem` |

Every writeable format is verified with **byte-exact round-trip tests on real game files**. See
[`docs/formats.md`](docs/formats.md) for the detailed matrix, versions, and known limits.

## Workspace layout

```
ritoshark/        umbrella crate — re-exports every format module behind a feature
crates/
  rs_io/          byte/stream I/O, the Parse/Serialize traits, mmap-backed reads
  rs_hash/        FNV-1a, XXH64/XXH3, SystemV ELF, and hash-name dictionaries
  rs_math/        vector / matrix / colour / bounds primitives (over glam)
  rs_file/        format detection by magic bytes
  rs_bin/         .bin (PROP/PTCH) + #PROP_text
  rs_wad/         .wad archives
  rs_tex/         .tex + .dds textures
  rs_mesh/        .skn skinned + .scb static meshes
  rs_anim/        .skl skeletons + .anm animations
  rs_mapgeo/      .mapgeo environment geometry
  rs_rst/         .stringtable string tables
  rs_rman/        .manifest release manifests (read-only)
  rs_audio/       .wpk / .bnk audio containers
  rs_cli/         the `ritoshark` command-line tool
```

Each crate has its own `README.md` describing its format and API.

## Using it

Add the umbrella crate and pull in the formats you want (all are on by default):

```toml
[dependencies]
ritoshark = "0.1"
# or trim to what you need:
# ritoshark = { version = "0.1", default-features = false, features = ["bin", "wad", "tex"] }
```

Every format type implements the same interface, so the verbs never change:

```rust
use ritoshark::prelude::*;
use ritoshark::wad::Wad;

let wad = Wad::from_path("Aatrox.wad.client")?;   // also: from_bytes / from_reader
for chunk in &wad.chunks {
    println!("{:016x}  {} -> {} bytes", chunk.path_hash, chunk.compressed_size, chunk.uncompressed_size);
}
```

### Command-line tool

```
cargo run -p rs_cli -- detect <file>            # identify a format by magic bytes
cargo run -p rs_cli -- bin to-text <in> [--hashes <dict>] [--output <out>]
cargo run -p rs_cli -- wad list <archive>
cargo run -p rs_cli -- tex to-png <in> <out>
```

## Building and testing

```
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
```

Requires a recent stable Rust toolchain (edition 2024). Tests that exercise real game files look
for them under `sample-files/` and **skip cleanly** when they are absent, so the suite is always
green out of the box. Those files are copyrighted game data and are never committed.

## Design

A short version of the principles (full detail in [`docs/architecture.md`](docs/architecture.md)):

- **One shape for every crate.** Same file layout, same `Parse`/`Serialize` traits, same verb
  vocabulary (`from_reader` / `from_bytes` / `from_path`, `to_writer` / `to_bytes` / `to_path`).
- **Lossless round-trip is the contract.** `read → write` reproduces the original bytes for every
  writeable format; this is the headline test for each crate.
- **No panics in library code.** Readers return `Result`; malformed input is an error, never a
  crash.
- **Safe first.** `#![forbid(unsafe_code)]` everywhere except a single audited memory-map in
  `rs_io`. Speed comes from algorithm and layout, not from `unsafe`.
- **No placeholder code.** Where a capability genuinely doesn't apply (e.g. authoring a manifest),
  it is simply absent — there are no stub functions that pretend to work.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Notes

RitoShark is an unofficial, fan-made library and is **not affiliated with or endorsed by Riot
Games**. League of Legends and all related assets are property of Riot Games, Inc. The library
parses file formats; it ships no game data. Thanks to the League of Legends modding community for
years of open format research.
