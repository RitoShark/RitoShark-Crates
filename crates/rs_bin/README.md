# rs_bin

Reader and writer for the League of Legends **PROP / `.bin`** property format and its
human-editable **`#PROP_text`** representation. The headline contract is a **byte-exact binary
round-trip**: `read → write` reproduces the original file bytes for any real input.

## The PROP format

A `.bin` file is a flat list of *entries* (top-level objects), each a struct identified by a
path hash and a class hash, holding an ordered list of typed fields.

### File layout (all little-endian)

```
magic        : "PROP"  (or "PTCH" + 8 header bytes + "PROP" for override/patch bins)
version      : u32
[version>=2] linkedCount : u32, then that many length-prefixed strings (linked/dependency files)
entryCount   : u32
entryClasses : u32 * entryCount          (class hash per entry, in entry order)
entries      : for each entry:
   length    : u32                        (byte size of the entry body that follows)
   pathHash  : u32
   fieldCount: u16
   fields    : fieldCount * { nameHash:u32, type:u8, value:... }
```

### Value type tags

```
NONE=0 BOOL=1 I8=2 U8=3 I16=4 U16=5 I32=6 U32=7 I64=8 U64=9 F32=10
VEC2=11 VEC3=12 VEC4=13 MTX44=14 RGBA=15 STRING=16 HASH=17 FILE=18
LIST=0x80 LIST2=0x81 POINTER=0x82 EMBED=0x83 LINK=0x84 OPTION=0x85 MAP=0x86 FLAG=0x87
```

Encoding facts:

- **Strings**: `u16` length prefix + UTF-8 bytes (no NUL).
- **Struct / embed field count**: `u16`. **List / map element count**: `u32`.
- **Container / struct sizes**: a `u32` byte-size field precedes the body of `LIST`, `LIST2`,
  `MAP`, `POINTER` (non-null), and `EMBED`. It is *not* present for `OPTION`.
- `LIST` / `LIST2` / `OPTION`: one element type tag; `MAP`: a key tag + a value tag.
- **Container nesting rule**: a `LIST`/`LIST2`/`OPTION` element and a `MAP` value may be any tag
  *except* another `LIST`/`LIST2`/`MAP`/`OPTION` (these four are the "containers"). `POINTER`,
  `EMBED`, `LINK`, and `FLAG` are legal elements. A `MAP` key must be a **primitive** (tag `0..=18`).
- **Null pointer**: a `POINTER` with class hash `0` has no size field and no body.
- **Option**: a `u8` count of `0` (absent) or `1` (present), then the value if present.

Hashes are stored as raw integers and are the source of truth for writing; names are resolved
(via `rs_hash::HashMapper`) only at print time. Field hashes, class hashes, `Hash`, and `Link`
are **FNV1a-32**; `File` values are **XXH64**.

### LIST vs LIST2

`LIST` (`0x80`) and `LIST2` (`0x81`) share an identical byte layout but use different tags. The
distinction is preserved (`BinValue::List { is_list2, .. }`) so it round-trips.

## Public API

```rust
use rs_bin::{Bin, BinEntry, BinType, BinValue};
use rs_io::{Parse, Serialize};

let bin = Bin::from_path("aatrox.bin")?;   // also from_bytes / from_reader
let bytes = bin.to_bytes()?;               // also to_path / to_writer  (byte-exact)

let text = rs_bin::to_text(&bin, None);    // #PROP_text printer (optional HashMapper)
```

- [`Bin`] — a parsed document: `is_patch`, `patch_header` (the 8 raw `PTCH` bytes),
  `version`, `linked` (ordered linked-file paths), `entries` (ordered).
- [`BinEntry`] — `path_hash`, `class_hash`, `fields: IndexMap<u32, BinValue>` (field order preserved).
- [`BinValue`] — the owned value enum (see [`src/bin.rs`]); map entries are an ordered
  `Vec<(key, value)>` so duplicate keys and order survive.
- [`BinType`] — the on-disk type tag, with `is_container()` / `is_primitive()` helpers.

The crate implements the workspace-standard `Parse` / `Serialize` traits, which provide
`from_bytes` / `from_path` / `to_bytes` / `to_path` for free.

## Supported / unsupported

| Capability | Status |
|---|---|
| PROP read + byte-exact write | supported |
| All primitive + container value types | supported |
| LIST/LIST2 distinction, ordered maps, null pointers, options | supported |
| Linked files (version >= 2) | supported |
| PTCH magic + 8 header bytes round-trip | supported |
| **PTCH trailing patches / data-overrides section** | **not yet** — see `docs/real-files-report.md` |
| `to_text` (`#PROP_text` printer) | supported (display only) |
| `from_text` (`#PROP_text` parser) | **stubbed** — returns `Error::Unsupported` |

## Tests

```bash
cargo test -p rs_bin
```

- `tests/roundtrip.rs` — hand-built PROP buffers pinning the exact on-disk layout, plus
  null-pointer, PTCH-header, and text-printer checks.
- `tests/real_files.rs` — parses and byte-exact round-trips the real sample `.bin` files and
  exercises the text printer. Sample files live in `sample-files/` at the workspace root and are
  **gitignored**; the tests skip (and print `skip`) when a file is absent, so the suite stays green
  without them.

Drop real `.bin` files into `RitoShark-Crates/sample-files/` to exercise the real-file suite.
