# Architecture

RitoShark is a Cargo workspace of small, single-purpose crates. This document describes how the
pieces fit together and the conventions every crate follows.

## Crate graph

```
rs_math ─┐
rs_hash ─┼─► format crates ─► ritoshark (umbrella) ─► rs_cli
rs_io  ──┘        ▲
rs_file ──────────┘
```

- **Foundation** — `rs_io`, `rs_hash`, `rs_math`, `rs_file`. These depend only on third-party
  crates (and, where useful, each other). They define the shared vocabulary every format speaks.
- **Format crates** — `rs_bin`, `rs_wad`, `rs_tex`, `rs_mesh`, `rs_anim`, `rs_mapgeo`, `rs_rst`,
  `rs_rman`, `rs_audio`. Each handles one family of formats and depends only on the foundation.
- **Umbrella** — `ritoshark` re-exports every format module behind a feature flag of the same
  name, plus a `prelude`. Nothing depends on the umbrella except the CLI.
- **CLI** — `rs_cli` builds the `ritoshark` binary.

## The universal interface

Two traits in `rs_io`, implemented by every top-level format type, make the whole workspace
behave identically:

```rust
pub trait Parse: Sized {
    type Error: From<rs_io::Error>;
    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self, Self::Error>;
    // provided: from_bytes(&[u8]), from_path(path)  — from_path memory-maps the file
}

pub trait Serialize {
    type Error: From<rs_io::Error>;
    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<(), Self::Error>;
    // provided: to_bytes() -> Vec<u8>, to_path(path)
}
```

The verb vocabulary is fixed and never varies between crates:

| Intent | Method |
|---|---|
| parse from a reader / bytes / file | `from_reader` / `from_bytes` / `from_path` |
| serialize to a writer / bytes / file | `to_writer` / `to_bytes` / `to_path` |
| construct in memory | `new` / `Foo::builder()…build()` |
| field accessor | `field_name()` (no `get_` prefix) |

`ReaderExt` and `WriterExt` (also in `rs_io`) add little-endian typed reads/writes
(`read_u32`, `read_string_u16`, `read_vec3`, …) on top of any `std::io::Read`/`Write`.

## Per-crate layout

Every format crate has the same shape:

```
crates/rs_<fmt>/
  src/
    lib.rs        crate docs + re-exports
    error.rs      one Error enum (thiserror) + Result<T>
    <type>.rs     the data types
    read.rs       impl Parse
    write.rs      impl Serialize
  tests/
    real_files.rs round-trip against real game files (skips when absent)
  README.md       the format + the crate's API
  docs/real-files-report.md   per-file results and notes
```

## Errors

Each crate defines exactly one `Error` enum via `thiserror` and one `Result<T>` alias. Errors
carry enough context to debug (offsets, expected-vs-got, path hashes) without embedding large
payloads. Library code never prints and never panics on input — malformed data is always an
`Err`. The CLI is the only place errors are rendered for humans.

## Performance

Speed comes from data layout and algorithm, not micro-optimization:

- **Memory-mapped reads.** `from_path` maps the file and parses from the borrowed slice; only leaf
  values (strings, buffers) allocate.
- **Single pass.** Readers consume the stream once and use on-disk size fields as inline bounds
  checks, which also catches corruption early.
- **Pre-sized collections.** Counts from the file drive `Vec::with_capacity` / `IndexMap::with_capacity`.
- **Parallel extraction.** `rs_wad` can extract chunks across threads behind its `parallel` feature.

## Safety

`#![forbid(unsafe_code)]` is set in every crate **except** `rs_io`, whose only `unsafe` is the
`memmap2` call inside `Parse::from_path`, isolated in one function with a written safety note.
There is no other `unsafe`, no `transmute`, and no SIMD anywhere in the workspace.

## Correctness and the round-trip contract

For every writeable format, the primary guarantee is a **byte-exact round-trip**: reading a real
file and writing it back reproduces the original bytes exactly. This is the headline test in each
crate's `tests/`, run against real game files. Additional coverage includes:

- `#PROP_text` round-trips for `rs_bin` (`bin → text → bin` is byte-identical).
- Per-version synthetic round-trips where real samples for a version aren't available.
- Robustness tests that feed truncated / malformed input and assert a clean `Err`.

Two formats are intentionally not byte-exact:

- **`rs_rman`** is read-only — release manifests are produced by Riot's servers and never authored
  on the client side, so there is no writer.
- Authoring a brand-new texture re-compresses pixel data (block compression is lossy), so a
  freshly *encoded* texture is compared approximately; a decoded-then-re-written `.tex` of an
  already-compressed source is byte-exact.

## No placeholder code

The workspace contains no stub functions — nothing that exists only to return "not implemented".
When a capability genuinely does not apply to a format, the corresponding method is simply absent
(for example, `rs_rman` implements `Parse` but not `Serialize`). Genuinely unknown inputs — an
unsupported file version, an unmapped enum value — return a precise error, which is honest
handling rather than a placeholder.

## Hashing reference

`rs_hash` provides the hashes the formats use:

| Hash | Used for |
|---|---|
| FNV-1a (32-bit) | bin field / class / entry names, `Hash`/`Link` values |
| XXH64 | WAD path hashes |
| xxh3-64 | string-table (RST) keys |
| SystemV ELF (`elf` / `elf_lower`) | skeleton & animation joint names |

`HashMapper` loads `<hex> <name>` dictionaries so raw integer hashes can be resolved back to
readable names for display.
