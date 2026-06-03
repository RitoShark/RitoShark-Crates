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

## Gap analysis vs pyritofile

Comparing `rs_audio` against `pyritofile`'s `wpk.py` / `bnk.py` (the primary reference) surfaced
the following:

| Area | pyritofile | rs_audio before | Gap |
|---|---|---|---|
| WPK dead (offset 0) slots | dropped during read (lossy) | not handled | round-trip would shrink the offset table |
| WPK blob alignment / gaps | not modelled (writer packs tight) | not modelled | real inter-blob padding lost |
| WPK wem id | parses `int` from `"<id>.wem"` | kept only raw name string | id accessor absent |
| WPK `wems()` API | id / offset / size per entry | no `wems()` on `Wpk` at all | API parity gap |
| BNK truncated section size | would raise on short read | `read_bytes` could pre-alloc up to 4 GiB before failing | OOM / abort risk on malformed `size` |
| WPK out-of-range offset/size | trusts the file | seeked + `read_bytes` unbounded | same allocation / EOF risk |

The BNK section framing itself (tag + `u32` size + body, walked to EOF; DIDX `(id,offset,size)`
triples; DATA offsets relative to the DATA body) already matched the reference exactly, which the
four real samples confirm byte-for-byte.

## What I implemented

- **WPK losslessness model.** `Wpk` gained `dead_slots: Vec<u32>` (positions of zero offset-table
  slots) and `WemEntry` gained `align: u32` (padding before each blob, measured against the
  canonical packing cursor). The reader captures both; the writer reproduces the full-length
  offset table with zeros in place and re-emits the alignment padding, so a real layout
  re-serializes byte-exact even where naive canonical packing would diverge. Added a synthetic
  test (`wpk_round_trips_dead_slots_and_alignment`) that hand-builds bytes with interleaved dead
  slots **and** per-blob padding and asserts byte-exact round-trip, plus an all-dead-slots case.
- **`wems()` API parity + id accessor.** `Wpk::wems() -> Vec<(Option<u32>, &str, &[u8])>` mirrors
  the reference (id parsed from `"<id>.wem"`, `None` for non-conforming names), and
  `WemEntry::new` / `WemEntry::id()` / `Wpk::push` round out the construction surface.
- **Robustness / no-panic.** Both `from_reader`s now read the stream length up front and bound
  every declared `size`/offset against it before allocating or slicing; a new `Error::Truncated`
  variant covers past-EOF cases. Fixed the latent **OOM/abort risk**: a malformed near-`u32::MAX`
  BNK section size (or WPK data size / slot count) previously reached `vec![0u8; n]` and would
  attempt a multi-gigabyte allocation before the read failed — now it `Err`s on the bound check.
  No outright panic was reachable in the old code, but the giant-allocation path was a latent
  crash; it is closed. Added fuzz-style unit tests: truncated section, DIDX size not a multiple
  of 12, DIDX offset past DATA, zero-length DATA, partial/empty input, bad version, table/data
  offset past EOF, and giant slot count — each asserts a clean `Err` or empty result.

## Remaining gaps

1. **No real `.wpk` sample (the one unproven gap).** WPK round-trip is validated by synthetic
   data only. The model can express dead slots and blob alignment, but a real Riot `.wpk` could
   in principle use a layout we have not anticipated (e.g. padding *inside* the entry-record
   block, or a non-zero version). If a real sample surfaces, drop it in `sample-files/`, extend
   `tests/real_files.rs`, and confirm — fixing any residual mismatch then.
2. **cargo-fuzz target.** The fuzz-style coverage here is unit tests, not a `cargo-fuzz` harness.
   A proper `fuzz_targets/{bnk,wpk}_reader.rs` (CLAUDE.md §11) remains a foundation-level add.
3. **Typed BKHD accessors.** Exposing BKHD version / bank id read-only (body kept verbatim) is
   still a nice-to-have for downstream version branching.
