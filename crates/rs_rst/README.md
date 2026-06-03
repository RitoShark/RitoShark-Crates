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

The hash is `xxh3-64` of the ASCII-lowercased key, truncated to the version's mask. Versions
outside 2–5 are rejected with `Error::UnsupportedVersion`.

> **Hash algorithm (decided): xxh3-64, not plain XXHash64.** Verified directly against the real
> `lol.stringtable`: the key `item_1001_name` (the in-game "Boots" entry) resolves only via
> `xxh3-64(lower) & ((1<<38)-1) = 0x1_09f4_cdf6`; plain XXHash64 matched **0** of a dozen probed
> keys. Some references (cdragon, the Rust/C# `XxHash64Ext` path) describe RST keys as plain
> XXHash64 with 39 bits — that is the *old* RST era and does not parse current files. pyRitoFile
> agrees with us (xxh3-64, 38 bits). See `docs/real-files-report.md` for the probe.

> Note on the width: current-era v4/v5 files in the wild use **38** hash bits, not 39, verified
> empirically against the real samples and matching pyRitoFile.

### Legacy encrypted entries (pre-v5)

Pre-v5 files whose `mode` byte is non-zero may store some entries as an encrypted payload —
`0xFF`, a `u16` little-endian length, then that many raw (non-UTF-8) bytes — instead of a
NUL-terminated string. rs_rst now decodes that framing into `RstValue::Encrypted(Vec<u8>)`,
keeping the ciphertext verbatim so the file round-trips byte-for-byte. The plaintext cannot be
recovered without the per-file key, so `get`/`get_by_hash` report encrypted entries as absent
text; reach their raw bytes with `value_by_hash`. When `mode` is zero a leading `0xFF` is treated
as ordinary string content, never as an encryption marker.

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
- `get(key) -> Option<&str>` — hash `key`, then look up the decoded string (encrypted → `None`).
- `get_by_hash(hash) -> Option<&str>` — look up the decoded string by raw masked hash.
- `value_by_hash(hash) -> Option<&RstValue>` — look up the full value, exposing encrypted bytes.
- `hash_bits()` / `hash_bits_for(v)` / `hash_mask_for(v)` / `hash_key(v, key)` — hashing helpers.

The public `entries: Vec<(u64, RstValue)>` field holds every `(hash, value)` pair in file order,
where `RstValue` is either `Text(String)` or `Encrypted(Vec<u8>)`.
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
