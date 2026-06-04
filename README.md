# RitoShark

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](https://www.rust-lang.org)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](docs/architecture.md#safety)
[![round-trip](https://img.shields.io/badge/round--trip-byte--exact-brightgreen.svg)](docs/architecture.md#correctness-and-the-round-trip-contract)

A Rust workspace for reading and writing League of Legends game file formats — correct (lossless, byte-exact round-trips on real game files), consistent (every format crate exposes the same API), and fast without `unsafe`.

```rust
use ritoshark::prelude::*;
use ritoshark::bin::Bin;

let bin = Bin::from_path("Aatrox.bin")?;         // mmaps + parses
let text = ritoshark::bin::to_text(&bin, None);  // #PROP_text
let bytes = bin.to_bytes()?;                     // byte-exact re-encode
```

## Format support

| Format | Ext | Crate | Read | Write |
|---|---|---|:--:|:--:|
| Property bin | `.bin` | `rs_bin` | ✅ | ✅ |
| WAD archive | `.wad`, `.wad.client` | `rs_wad` | ✅ | ✅ |
| Texture | `.tex`, `.dds` | `rs_tex` | ✅ | ✅ |
| Skinned mesh | `.skn` | `rs_mesh` | ✅ | ✅ |
| Static mesh | `.scb` | `rs_mesh` | ✅ | ✅ |
| Skeleton | `.skl` | `rs_anim` | ✅ | ✅ |
| Animation | `.anm` | `rs_anim` | ✅ | ✅ |
| Map geometry | `.mapgeo` | `rs_mapgeo` | ✅ | ✅ |
| String table | `.stringtable` | `rs_rst` | ✅ | ✅ |
| Release manifest | `.manifest` | `rs_rman` | ✅ | — |
| Audio container | `.wpk`, `.bnk` | `rs_audio` | ✅ | ✅ |

Versions, capabilities, and known limits per format are in [docs/formats.md](docs/formats.md).

## Install

```toml
[dependencies]
ritoshark = "0.1"   # or: default-features = false, features = ["bin", "wad", "tex"]
```

## Command-line tool

```
cargo run -p rs_cli -- detect <file>
cargo run -p rs_cli -- bin to-text <in> [--hashes <dict>] [--output <out>]
cargo run -p rs_cli -- wad list <archive>
cargo run -p rs_cli -- tex to-png <in> <out>
```

## Build & test

```
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Needs a recent stable toolchain (edition 2024). Real-file tests skip cleanly when the game files (which are never committed) are absent, so the suite is green out of the box.

## Documentation

- [docs/architecture.md](docs/architecture.md) — design, the `Parse`/`Serialize` interface, the crate layout, performance and safety, the round-trip contract.
- [docs/formats.md](docs/formats.md) — per-format support detail, versions, and limits.
- Each crate under `crates/` has its own `README.md`.

## License

Dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

## Notes

RitoShark is an unofficial, fan-made library and is **not affiliated with or endorsed by Riot Games**. It parses file formats and ships no game data. Thanks to the League of Legends modding community for years of open format research.
