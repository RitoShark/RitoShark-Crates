# rs_rst — real-files report

Validation of `rs_rst` against real League of Legends `.stringtable` files and cross-checks
against reference implementations.

## Samples

Located in the gitignored workspace `sample-files/` directory.

| File                     | Size       | Magic | Version |
|--------------------------|-----------:|-------|---------|
| `bootstrap.stringtable`  | 14,381 B   | `RST` | 5       |
| `lol.stringtable`        | 18,430,243 B | `RST` | 5     |

## Per-file results (after fix)

| File                    | Version | Entries | Parse | Byte-exact round-trip | Lookups |
|-------------------------|---------|--------:|-------|-----------------------|---------|
| `bootstrap.stringtable` | 5       | 201     | OK    | **OK** (14,381 B identical) | 16/16 non-empty |
| `lol.stringtable`       | 5       | 121,837 | OK    | **OK** (18,430,243 B identical) | 15/16 non-empty (1 legitimately empty string) |

All entries in both files decode as valid UTF-8 and every entry round-trips byte-for-byte.

## Bugs found and fixed

### 1. Wrong hash width for v4/v5 (the blocker)

Before the fix, v4/v5 used a **39-bit** hash mask. Both real v5 files failed to parse with
`InvalidUtf8`: the 39-bit split put the offset one bit short, so offsets landed in the middle of
UTF-8 sequences. Empirical sweep over the real entry tables:

| Mask  | bootstrap bad entries | lol bad entries | max offset reached vs blob size |
|-------|----------------------:|----------------:|----------------------------------|
| 39    | 107 / 201             | 28 / 121,837    | ~half the blob (offsets truncated) |
| **38**| **0 / 201**           | **0 / 121,837** | reaches end of blob (correct)    |

The correct width for current-era v4/v5 is **38 bits**. Fixed in `src/rst.rs`
(`hash_bits_for`: `4 | 5 => 38`), with the doc comments in `src/lib.rs` and `src/rst.rs` updated
to match. v2/v3 remain 40 bits.

### 2. Blob layout not preserved (round-trip fidelity)

After the width fix, parsing succeeded but the naive writer rebuilt the string blob in entry
order, whereas real files lay distinct strings out in a **different, blob-specific order** (the
first entry pointed at offset 32, not 0). Lengths matched but bytes did not.

Verified that the real blob is exactly the distinct strings tiled contiguously in **ascending
original-offset order** (sum of distinct string bytes == blob size; no gaps or overlap). The
reader now records that ordering in an internal `blob_order` hint, and the writer emits those
strings first (then appends any string not present in it, e.g. ones added via `add`). This makes
both real files round-trip byte-exact while keeping in-memory construction working. `blob_order`
is excluded from `PartialEq` since it is a layout hint, not logical content.

## Gap analysis vs C#

The C# `LeagueToolkit` checkout in this workspace no longer ships an RST reader/writer (only
`Hashing/XxHash64Ext.cs` remains, a generic plain-`XxHash64` helper with no lowercasing and no RST
coupling). So for RST the *authoritative oracle is the real game file itself*, cross-read against
cdragon-rst, ltk_rst, and pyRitoFile. Three things were in doubt going in:

1. **Key-hash algorithm — xxh3-64 vs plain XXHash64.** Resolved empirically against
   `lol.stringtable` (121,837 entries). Building the file's `hash → string` map and hashing a dozen
   well-known item keys both ways (38-bit mask):

   | key             | xxh3-64 (lower) & 38 | found? | XXHash64 (lower) & 38 | found? |
   |-----------------|----------------------|--------|-----------------------|--------|
   | `item_1001_name`| `0x1_09f4_cdf6`      | ✓ "Boots" | `0x3376eae1da`     | ✗      |
   | `item_3020_name`| `0x2_648e_9039e`*    | ✓ "Sorcerer's Shoes" | —        | ✗      |
   | …11 of 12       |                      | ✓      |                       | ✗ (0/12) |

   `xxh3-64` matched 11/12 (the 12th key is simply not in this table); plain `XXHash64` matched
   **0/12**. cdragon's own doctest hash for `item_1001_name` (`0x3376eae1da`) *is* the XXHash64
   value — it is stale, from the older 39-bit XXHash64 RST era, and does not occur in current files.
   **Decision: xxh3-64 of the ASCII-lowercased key, 38-bit mask — the current code was already
   correct.** Pinned as a test (`pinned_key_hash_vector_v5`).

2. **Case-folding.** RST keys are ASCII identifiers; `to_ascii_lowercase` matches pyRitoFile's
   `.lower()` for the only characters that occur. Pinned: `ITEM_1001_NAME` hashes identically.

3. **Legacy encrypted entries.** Implemented (below).

## What I implemented

- **Legacy pre-v5 encrypted entries.** Entry values are now `RstValue::{Text(String),
  Encrypted(Vec<u8>)}`. On read, when `version < 5 && mode != 0` and a blob entry starts with
  `0xFF`, it is decoded as `[0xFF][u16 length][length raw bytes]` (the cdragon-rst `has_trenc`
  scheme) and kept verbatim as `Encrypted`; otherwise it is a NUL-terminated UTF-8 `Text`. The
  writer re-emits each variant with the exact framing, so encrypted entries round-trip
  byte-for-byte even though their plaintext is unrecoverable without the per-file key. When `mode`
  is zero a leading `0xFF` stays ordinary string content. `get`/`get_by_hash` return `None` for
  encrypted entries; `value_by_hash` exposes the raw bytes.
- **Pinned key-hash vector** and a synthetic encrypted-entry round-trip test.

No version/mask bug remained vs the references: 38-bit v4/v5 and 40-bit v2/v3 are confirmed correct
for the current era; the 39-bit references are stale.

## Cross-check vs references

| Implementation | Hash algo (key → hash) | Lowercase? | v4/v5 bits | Matches real lol.stringtable? |
|----------------|------------------------|-----------|-----------|-------------------------------|
| **rs_rst** (this crate) | **xxh3-64**  | yes (ASCII) | **38**  | **✓ 11/12 probed keys**       |
| pyRitoFile (CDTB-derived) | xxh3-64     | yes        | **38**    | ✓ (same algo + width)         |
| cdragon-rst    | XxHash64 (plain)       | **no**    | 39        | ✗ 0/12 (stale, old era)       |
| ltk_rst (Rust, C# port) | XxHash64      | yes        | 39        | ✗ (stale)                     |
| C# `XxHash64Ext` (no RST class today) | XxHash64 | no | n/a | ✗ (generic helper, not RST)   |

Observations (now resolved, not open questions):

- **Hash algorithm**: settled empirically on **xxh3-64** — it reproduces real stored hashes
  (`item_1001_name → 0x1_09f4_cdf6 → "Boots"`), plain XXHash64 reproduces none. The XXHash64
  references are from the older 39-bit RST era. rs_rst already used xxh3-64, matching CLAUDE.md §7.
- **Hash width**: **38** bits for v4/v5, confirmed by the same probe and by entry counts; the
  39-bit references are stale.
- **Lowercasing**: ASCII-lowercase, which equals pyRitoFile's `.lower()` for the ASCII-only keys
  that occur. Pinned by a test.

## Remaining gaps

1. **Acquire v2/v3/v4 fixtures.** Only v5 samples exist here, so the 40-bit path, the v2
   font-config block, and a *real* encrypted entry are exercised only by synthetic tests. The
   encrypted-entry framing is derived from cdragon-rst, not yet confirmed against a real pre-v5
   file.
2. **Encryption plaintext is unrecoverable** by design — the per-file key/scheme is not part of
   the format; rs_rst preserves the ciphertext for lossless round-trip but cannot decrypt it.

## Foundation needs

None new. rs_rst stays on `rs_hash::xxh3_64` and `rs_io`'s `ReaderExt`/`WriterExt`; no shared
primitive was missing for this work.

## What I changed (this pass)

- **Resolved the key-hash algorithm**: empirically confirmed **xxh3-64 / 38-bit / ASCII-lower**
  against the real file; current code was already correct (no fix needed there). Pinned vector
  test `pinned_key_hash_vector_v5`.
- `src/rst.rs`: added `RstValue::{Text, Encrypted}`; `entries` is now `Vec<(u64, RstValue)>`;
  `add` takes `impl Into<RstValue>`; added `value_by_hash`; `get`/`get_by_hash` return text only.
- `src/read.rs`: decode `0xFF`-framed legacy encrypted payloads (gated by `version < 5 && mode != 0`)
  into `Encrypted`, with bounds-checked length; `blob_order` now holds `RstValue`.
- `src/write.rs`: re-emit `Text` (NUL-terminated) and `Encrypted` (`0xFF`+`u16`+bytes) with exact
  framing; dedup on `RstValue`.
- `src/lib.rs`: re-export `RstValue`; module doc updated.
- `tests/roundtrip.rs`: **added** `pinned_key_hash_vector_v5`,
  `legacy_encrypted_entry_round_trips`, `mode_zero_does_not_decode_encryption`.

All 13 tests pass (`cargo test -p rs_rst`); clippy clean with `-D warnings`. Both real v5 files
still round-trip byte-exact.
