# rs_audio — real-files report

Validation of `rs_audio` against the real League `.bnk` samples in `sample-files/`.
Tests live in `crates/rs_audio/tests/real_files.rs` (skip-if-missing). All four samples
are BKHD version 145 (0x91).

## Per-file results

| File | Size | Section tags | Round-trip | wems |
|---|---|---|---|---|
| `aatrox_base_sfx_audio.bnk` | 3,017,860 B | `BKHD`, `DIDX`, `DATA` | byte-exact OK | 170 |
| `aatrox_base_sfx_events.bnk` | 30,821 B | `BKHD`, `HIRC` | byte-exact OK | 0 |
| `olaf_base_vo_audio.bnk` | 48 B | `BKHD` | byte-exact OK | 0 |
| `olaf_base_vo_events.bnk` | 4,692 B | `BKHD`, `HIRC` | byte-exact OK | 0 |

All four containers serialize back **byte-for-byte identical** to the input. The HIRC
section in the two `_events` banks is kept verbatim as an opaque body, which is exactly
what preserves the round-trip without needing to decode the Wwise object graph.

### Embedded wem sanity (aatrox_base_sfx_audio.bnk)

- 170 embedded wems via `DIDX`/`DATA`.
- wem body sizes range 3,560 – 179,841 bytes; sum 3,014,515 B, which fits inside the
  3,015,748 B `DATA` section. Every `wems()` entry has a non-empty, in-range body.

### Notes on the other "audio" bank

`olaf_base_vo_audio.bnk` is 48 bytes and contains only a `BKHD` chunk (8-byte chunk header
+ 40-byte body) — **no `DIDX`/`DATA`, so 0 embedded wems** despite the `_audio` name. The
audio for that bank evidently lives elsewhere (likely a companion `.wpk`). This confirms the
`_audio` / `_events` filename split is a convention, not a structural guarantee; code must
key off the actual sections present, which `wems()` does.

## Cross-check vs the Python reference (`bnk.py`)

Ran the reference SoundBank reader against the same files (loaded directly to bypass an
unrelated optional-dependency import in the package init):

| File | reference tags | reference wems | agreement |
|---|---|---|---|
| `aatrox_base_sfx_audio.bnk` | BKHD, DIDX, DATA | 170 | matches |
| `olaf_base_vo_audio.bnk` | BKHD | 0 | matches |
| `aatrox_base_sfx_events.bnk` | — | — | reference **crashes** parsing HIRC |
| `olaf_base_vo_events.bnk` | — | — | reference **crashes** parsing HIRC |

- Section list and wem count agree exactly on both banks the reference can read.
- The reference's deep HIRC object decoder throws (`unpack requires a buffer of N bytes`)
  on the version-145 `_events` banks — its hierarchy parser predates this BKHD version.
  `rs_audio`'s container-level, verbatim-HIRC approach reads and round-trips them losslessly,
  so we are strictly more robust here. This is the central design validation: not decoding
  HIRC is what keeps us correct and lossless across versions.
- Section framing matches the reference: `4-byte tag + u32 size + body`, walked until EOF.
  DIDX entries are `(id, offset, size)` u32 triples; DATA offsets are relative to the start
  of the DATA body — identical to what `wems()` slices.

## What I changed

- Added `tests/real_files.rs`: per-file byte-exact round-trip on all four `.bnk` samples,
  asserts the first section is `BKHD`, and for the audio bank asserts `wems()` returns ≥1
  entries with non-empty bodies. Skips cleanly when samples are absent.
- Added this report and `README.md`.
- **No source changes were required** — every real sample already round-trips byte-exact and
  the DIDX/DATA wem extraction matches the reference. The parser/writer were left untouched.

## Improvements / TODO

1. **Obtain real `.wpk` samples.** There are no `.wpk` files in `sample-files/`, so the WPK
   reader/writer is currently proven only by synthetic round-trip tests. WPK round-trip is
   *not* guaranteed byte-exact against real files: the writer emits a canonical layout
   (offset table immediately after the header, entries packed, blobs last), and the reader
   discards per-entry padding/alignment and the original entry ordering / data-offset gaps
   that a real Riot `.wpk` may contain. Acquire real `.wpk` files (e.g. champion VO packages),
   add them to `sample-files/`, and verify — then fix any layout/padding mismatch. This is the
   biggest correctness gap in the crate.
2. **WPK loses the original wem id / name semantics.** The reference stores the numeric wem id
   parsed from the `<id>.wem` name; `rs_audio` keeps the raw UTF-16 name string instead. Once
   real samples exist, confirm names round-trip exactly (including any non-`<id>.wem` names).
3. **Fuzz `Bnk::from_reader` / `Wpk::from_reader`.** Per CLAUDE.md §11, every `from_reader`
   should be fuzzed to prove it only ever returns `Err`, never panics, on malformed input
   (e.g. truncated chunk sizes, DIDX size not a multiple of 12, oversized offsets).
4. **Consider exposing BKHD version / bank id** as typed accessors (read-only) so downstream
   tools can branch on version without re-parsing the BKHD body, while still keeping the body
   verbatim for round-trip.
