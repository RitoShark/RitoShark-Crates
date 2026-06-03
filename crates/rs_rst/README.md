# rs_rst

Reads and writes the League of Legends **RST string table** (`.stringtable`): a localization
table mapping truncated key hashes to UTF-8 strings. Reading and writing are lossless — a real
file read back and re-serialized is **byte-identical** to the original.

## Format

```
"RST"                 3 bytes magic
version               u8
[v2 only] has_config  u8 (bool); if 1, config = u32 length + UTF-8 bytes
count                 u32
entries[count]        u64 each: (offset << hash_bits) | (hash & hash_mask)
[version < 5] mode    u8   (legacy "translation encrypted" flag)
blob                  remaining bytes: NUL-terminated UTF-8 strings
```

Each entry packs a key hash and the offset of its string into the blob into one little-endian
`u64`. The low `hash_bits` are the hash; the rest is the offset. Strings are resolved by reading
from `blob[offset]` up to the next NUL. Distinct strings are deduplicated and tiled contiguously
in the blob; the entry table indexes into them and is ordered independently of the blob.

### Supported versions and hash widths

| Version | Hash bits | Mask           | Extra header     | Mode byte |
|---------|-----------|----------------|------------------|-----------|
| 2       | 40        | `(1<<40)-1`    | optional font config | yes   |
| 3       | 40        | `(1<<40)-1`    | —                | yes       |
| 4       | 38        | `(1<<38)-1`    | —                | yes       |
| 5       | 38        | `(1<<38)-1`    | —                | no        |

The hash is `xxh3-64` of the lowercased key, truncated to the version's mask. Versions outside
2–5 are rejected with `Error::UnsupportedVersion`.

> Note on the width: current-era v4/v5 files in the wild use **38** hash bits, not 39. This was
> verified empirically against real `bootstrap.stringtable` / `lol.stringtable` samples and
> matches the CDTB-derived reference (pyRitoFile). See `docs/real-files-report.md`.

### Legacy encryption (not supported for content)

Pre-v5 files can mark individual entries as encrypted (a leading `0xFF` byte plus a `u16`
length, gated by the `mode` byte). rs_rst preserves the `mode` byte for round-trip but does not
decrypt entry payloads; such entries are read as raw strings. This is noted as future work.

## API

`Rst` implements the workspace `Parse` / `Serialize` traits, giving the standard verbs:

```rust
use rs_rst::Rst;
use rs_io::{Parse, Serialize};

// Read
let rst = Rst::from_path("lol.stringtable")?;   // also from_bytes / from_reader
println!("{} entries (v{})", rst.entries.len(), rst.version);

// Look up
let text = rst.get("game_hud_announcement");     // hashes the key for this version
let text = rst.get_by_hash(0x123cbc779);          // raw, already-masked hash

// Build / edit
let mut t = Rst::new();                            // defaults to v5
t.add("game_client_quit", "Quit");                 // hashes + appends, returns the hash

// Write
let bytes = rst.to_bytes()?;                       // also to_path / to_writer
```

Key methods:

- `Rst::new()` / `Rst::with_version(v)` — construct empty (v5 by default).
- `add(key, value) -> Option<u64>` — hash `key` for the table's version and append the entry,
  preserving insertion order; returns the masked hash, or `None` for an unsupported version.
- `get(key) -> Option<&str>` — hash `key`, then look up.
- `get_by_hash(hash) -> Option<&str>` — look up by raw masked hash.
- `hash_bits()` / `hash_bits_for(v)` / `hash_mask_for(v)` / `hash_key(v, key)` — hashing helpers.

The public `entries: Vec<(u64, String)>` field holds every `(hash, string)` pair in file order.
An internal blob-layout hint is retained from reads so byte-exact round-trip is preserved; it is
excluded from equality (two tables are equal when version, font config, mode, and entries match).

## Tests

- `tests/roundtrip.rs` — in-memory round-trip per version (v2 font config, v4 mode byte, v5),
  dedup, lookups, error cases.
- `tests/real_files.rs` — parses real `.stringtable` samples from `../../sample-files`
  (skipped if absent), asserts entry count > 0, byte-exact round-trip, and non-empty lookups.

Drop real fixtures into the gitignored workspace `sample-files/` directory to exercise them.

```
cargo test -p rs_rst
```
