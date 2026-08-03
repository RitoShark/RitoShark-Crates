# rs_audio — real-files report

Validation of `rs_audio` against real League `.bnk` and `.wpk` samples in `sample-files/`.
Tests live in `crates/rs_audio/tests/real_files.rs` and skip gracefully when a file is absent.

## Codec census — what League actually ships

Measured against a live install by extracting WADs with `rs_cli wad extract` and classifying
every embedded payload. Sample: Aatrox, Ahri, Yunara, Zaahen (base and `en_US`), Map11, Common,
Global, Companions — **1,398 banks and packages, 13,420 embedded wems**.

| Codec id | fmt chunk | Count | Share |
|---|---|---|---|
| `0xFFFF` Wwise Vorbis | 66 bytes | 13,420 | 100% |

No Opus, no PCM, no ADPCM anywhere in the game. Channel counts are 1 or 2 only. Sample rates
are **not** uniformly 44.1 kHz — 44100 dominates, but 48000, 36000, 32000, 24000, 16000, 12000,
8000 and 6000 all occur. Nothing may assume a rate.

Bank header revisions in circulation: **145** (1,113 banks) and **134** (201 banks).

## Per-file results

| File | Size | Section tags | Round-trip |
|---|---|---|---|
| `aatrox_base_sfx_audio.bnk` | 3,017,860 B | `BKHD`, `DIDX`, `DATA` | byte-exact (170 wems) |
| `aatrox_base_sfx_events.bnk` | 30,821 B | `BKHD`, `HIRC` | byte-exact |
| `olaf_base_vo_audio.bnk` | 48 B | `BKHD` | byte-exact |
| `olaf_base_vo_events.bnk` | 4,692 B | `BKHD`, `HIRC` | byte-exact |
| `bank_v145_audio.bnk` | 23,211 B | `BKHD`, `DIDX`, `DATA` | byte-exact |
| `bank_v134_audio.bnk` | 108,066 B | `BKHD`, `DIDX`, `DATA` | byte-exact |
| `bank_v145_events.bnk` | 577 B | `BKHD`, `HIRC` | byte-exact |
| `bank_v134_events.bnk` | 1,983 B | `BKHD`, `HIRC` | byte-exact |
| `bank_v145_bare.bnk` | 48 B | `BKHD` | byte-exact |
| `bank_v134_bare.bnk` | 32 B | `BKHD` | byte-exact |
| `bank_v145_init.bnk` | 80,170 B | `BKHD`, `INIT`, `STMG`, `HIRC`, `ENVS`, `PLAT` | byte-exact |
| `audio_package_37.wpk` | 25,922,933 B | 37 entries | byte-exact |
| `audio_package_4.wpk` | 544,983 B | 4 entries | byte-exact |

The init bank matters disproportionately: `INIT`/`STMG`/`ENVS`/`PLAT` appear nowhere else, so it
is the file that proves unknown sections survive verbatim rather than being dropped.

`olaf_base_vo_audio.bnk` is 48 bytes and holds only `BKHD` — no `DIDX`/`DATA`, so zero embedded
wems despite the `_audio` name. The `_audio` / `_events` filename split is a convention, not a
structural guarantee; code must key off the sections actually present, which `wems()` does.

## The WPK alignment bug — found and fixed

Earlier versions of this report listed "no real `.wpk` sample" as the one unproven gap. It was
not merely unproven; **the model was wrong**, and synthetic tests could not have caught it.

Riot pads three regions up to an **eight-byte boundary**: the offset table, each entry record,
and each audio blob. The previous model stored a per-entry `align: u32` capturing padding before
each *blob*, measured against a cursor that assumed zero padding in the header and record block.
The record-block padding was therefore misattributed, and records were written to the wrong
offsets — 28/66/104/142 where the real file has 32/72/112/152. Total file length still matched,
because the first blob's `align` absorbed the difference. A length assertion would have passed.

The fix removes machinery rather than adding it. `WemEntry::align` is **deleted**; layout is
derived from the single alignment rule in `Wpk::layout()`, which both the writer and the reader's
bounds checks consume so they cannot drift apart. Validated against **29 real packages** drawn
from eight champions' `en_US` WADs — all 29 re-serialize byte for byte.

`dead_slots` stays. Offset-table slots of `0` are real, and reproducing their positions is what
keeps the table the right length.

## Cross-check vs the Python reference (`bnk.py`)

| File | reference tags | reference wems | agreement |
|---|---|---|---|
| `aatrox_base_sfx_audio.bnk` | BKHD, DIDX, DATA | 170 | matches |
| `olaf_base_vo_audio.bnk` | BKHD | 0 | matches |
| `aatrox_base_sfx_events.bnk` | — | — | reference **crashes** parsing HIRC |
| `olaf_base_vo_events.bnk` | — | — | reference **crashes** parsing HIRC |

Section list and wem count agree exactly on both banks the reference can read. Its deep HIRC
decoder throws (`unpack requires a buffer of N bytes`) on version-145 banks — its hierarchy
parser predates this revision. Keeping HIRC as opaque bytes at the container level reads and
round-trips those banks losslessly, so we are strictly more robust here.

Section framing matches the reference: `4-byte tag + u32 size + body`, walked to EOF. DIDX
entries are `(id, offset, size)` u32 triples; DATA offsets are relative to the start of the DATA
body — identical to what `wems()` slices.

The Python reference also drops WPK dead slots during read, which is lossy; we preserve them.

## Robustness

No panics in library code; malformed input returns `Err`. Both readers bound every declared size
and offset against the actual input length before allocating or slicing, so truncated sections, a
DIDX size not a multiple of 12, offsets past EOF, a near-`u32::MAX` section or slot count, and
zero-length DATA all yield a clean `Err` or an empty result. The giant-allocation path — where a
malformed size reached `vec![0u8; n]` and attempted a multi-gigabyte allocation before the read
failed — is closed by the bound checks.

## Remaining gaps

1. **cargo-fuzz targets.** Coverage here is unit tests, not a `cargo-fuzz` harness. Proper
   `fuzz_targets/{bnk,wpk}_reader.rs` (CLAUDE.md §11) remains a foundation-level add.
2. **WPK version 1 only.** Every observed package is version 1; the reader rejects anything else
   rather than guessing at a layout it has never seen.
