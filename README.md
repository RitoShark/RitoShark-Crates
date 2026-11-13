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
Animation APIs include [CANM, v5 and v4 export with automatic fallback](crates/rs_anim/docs/animation-export.md),
configurable accuracy and optional skeleton validation, available to Rust and Python callers.

## Install

```toml
[dependencies]
ritoshark = "0.1"   # or: default-features = false, features = ["bin", "wad", "tex"]
```

## Command-line tool (`rs_cli`)

The release workflow builds Windows binaries for the [GitHub Releases] page — download `rs_cli`
and (optionally) drop a `hashes/` folder of CDTB dictionaries next to it for name resolution.

```
rs_cli read <file> [--json] [--hashes <dir>]          # auto-detect, print info
rs_cli detect <file> [--json]                          # just the format
rs_cli transform <in> [out] [-r] [--keep-hashed]       # convert between formats

rs_cli bin convert <in> [out] [-r] [--keep-hashed]
rs_cli bin diff <a> <b> [-C <n>] [--no-color]
rs_cli wad list <archive>... [-F table|json|csv|flat] [--stats]
rs_cli wad extract <archive>... -o <dir> [-f <type>...] [-x <regex>] [--overwrite]
rs_cli tex info <in> [--json]
rs_cli tex decode <in> [-o <out>] [--mip <n>]
rs_cli tex encode <in> -f <bc1|bc3|bc5|bc7|bgra8> [-m] [-o <out>]
rs_cli rst list <in> [--json]
rs_cli audio extract <wpk|bnk> -o <dir>
```

The tool runs entirely in-process through the RitoShark crates; it calls no external program.
Full reference in [docs/cli.md](docs/cli.md).

### Install (prebuilt binary)

Download the Windows `rs_cli` binary from the [GitHub Releases] page. No runtime dependencies are
required. To enable hash resolution, place a `hashes/` directory containing CDTB dictionary files
beside the binary.

[GitHub Releases]: https://github.com/RitoShark/RitoShark-Crates/releases

## Build & test

```
cargo build --workspace --exclude rs_python --locked
cargo test --workspace --exclude rs_python --locked
cargo clippy --workspace --exclude rs_python --all-targets --all-features --locked -- -D warnings
```

Use the pinned toolchain in `rust-toolchain.toml`. Python bindings have a separate wheel/pytest workflow. Real-file tests can skip when game files (which are never committed) are absent; a passing suite does not imply full corpus coverage. See [the CI matrix and local checks](docs/testing.md).

## Documentation

- [docs/architecture.md](docs/architecture.md) — design, the `Parse`/`Serialize` interface, the crate layout, performance and safety, the round-trip contract.
- [docs/formats.md](docs/formats.md) — per-format support detail, versions, and limits.
- [docs/ptch.md](docs/ptch.md) — existing PTCH support, examples, and application limits.
- [docs/testing.md](docs/testing.md) — explicit CI coverage and fixture limitations.
- Crate-level API documentation lives under `crates/` in Rust docs and the available READMEs.

## License

Dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

## Notes

RitoShark is an unofficial, fan-made library and is **not affiliated with or endorsed by Riot Games**. It parses file formats and ships no game data. Thanks to the League of Legends modding community for years of open format research.
