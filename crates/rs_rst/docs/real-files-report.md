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

## Cross-check vs references

| Implementation | Hash algo (key → hash) | Lowercase? | v4/v5 bits | bootstrap / lol entries |
|----------------|------------------------|-----------|-----------|--------------------------|
| **rs_rst** (this crate) | xxh3-64       | yes (ASCII) | **38**  | 201 / 121,837            |
| pyRitoFile (CDTB-derived) | xxh3-64     | yes        | **38**    | 201 / 121,837 ✓          |
| cdragon-rst    | XxHash64 (plain)       | **no**    | 39        | — (read-only, no writer) |
| ltk_rst (Rust, C# port) | XxHash64      | yes        | 39        | —                        |
| C# LeagueToolkit (oracle) | XxHash64    | yes        | 40/39     | —                        |

Observations:

- **Hash width**: pyRitoFile agrees with our fixed **38-bit** v5 width and reports identical
  entry counts (201 and 121,837). cdragon and ltk use 39, which does **not** parse these real
  files — so for the v5 era, 38 is correct and the 39-based references are stale.
- **Hash algorithm**: references disagree on the key-hashing function. cdragon and ltk/C# use
  plain `XXHash64`; pyRitoFile and rs_rst use `xxh3-64`. This only affects `add`/`get`
  (computing a hash from a string key) — it does **not** affect reading existing files, which
  store pre-computed hashes, nor byte-exact round-trip. CLAUDE.md §7 specifies **xxh3-64** for
  RST, so rs_rst follows the project decision. This divergence is flagged below as a correctness
  item to resolve with the C# oracle on a key/hash test vector.
- **Lowercasing**: rs_rst lowercases ASCII only; cdragon does not lowercase at all. To be
  resolved alongside the hash-algorithm question.

## Improvements / TODO

1. **Resolve the key-hashing algorithm and case-folding** (highest priority for `add`/`get`
   correctness). Confirm against the C# `LeagueToolkit` oracle whether RST keys hash with
   `xxh3-64` or plain `XXHash64`, and whether folding is ASCII-only or full Unicode lowercase.
   Pin it with a known `(key, hash)` test vector. (Does not affect current round-trip results.)
2. **Legacy encrypted entries** (pre-v5). The `mode`/`has_trenc` byte is preserved, but
   `0xFF`-prefixed encrypted payloads are read as raw strings rather than decoded. Add a raw
   accessor (`get_raw`) and decode path; needed for full v2–v4 fidelity. Larger change — noted
   only, not implemented.
3. **Acquire v2/v3/v4 fixtures** to validate the 40-bit path, the font-config block, and the
   mode byte on real data (only v5 samples are currently available). Add them to the real-files
   test matrix.

## What I changed

- `src/rst.rs`: `hash_bits_for` v4/v5 `39 → 38`; added internal `blob_order` field; manual
  `PartialEq`/`Eq` excluding `blob_order`; updated doc comments.
- `src/read.rs`: capture `blob_order` (distinct strings in ascending-offset order); minor clippy
  fix (`ok_or_else` → `ok_or`).
- `src/write.rs`: emit blob using `blob_order` first, then append new strings.
- `src/lib.rs`: module doc width note `39 → 38`.
- `tests/real_files.rs`: **added** — skip-if-missing real-file parse + byte-exact round-trip +
  lookup checks for both `.stringtable` samples.
- `README.md`, `docs/real-files-report.md`: **added**.

All 10 tests pass (`cargo test -p rs_rst`); clippy clean with `-D warnings`.
