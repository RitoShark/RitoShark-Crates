# rs_bin — real-files report

Validation of `rs_bin` against real League of Legends `.bin` sample files, cross-checked against
the canonical/oracle implementations (ritobin C++, LeagueToolkit C#, pyritofile Python).

## Step 2 — real-file results

Samples live in `RitoShark-Crates/sample-files/` (gitignored). Tests in
`crates/rs_bin/tests/real_files.rs`.

| File | Magic / version | Entries | Parses? | Byte-exact round-trip? | Text printer? |
|---|---|---|---|---|---|
| `aatrox.bin` | PROP / 3 | 55 | yes | **yes** | yes — valid `#PROP_text` |
| `aatrox_multi_…_skin0…skin8.bin` | PROP / 3 | 3 | yes | **yes** | yes |
| `aatrox_multi_…_skin33…skin39.bin` | PROP / 3 | 64 | yes | **yes** | yes |

All three files parse, round-trip **byte-identically** (`to_bytes` == original file bytes), and
produce sane `#PROP_text` (correct `#PROP_text` header, `version: 3`, `entries: map[hash,embed]`,
nested `pointer` / `link` / `list[string]` / `map[string,string]` fields, hashes printed as
`0x%08x` when unresolved).

`cargo test -p rs_bin` is **green**: 5 unit/roundtrip tests + 4 real-file tests pass.

### Bug found and fixed (step 4)

On the first run, **all three real files failed to parse** with
`a container element type may not itself be a container: 132` (and `130`).

Root cause: `BinType::is_container()` was defined as `tag & 0x80 != 0`, which incorrectly
classified `POINTER` (0x82), `EMBED` (0x83), `LINK` (0x84), and `FLAG` (0x87) as containers. The
reader rejected any list/map/option whose element type was one of those — but those are perfectly
legal element types (e.g. `list[link]`, `map[string,pointer]`, `map[hash,embed]`).

Per the canonical references, the set of "containers" that may **not** nest as an element is only
`LIST | LIST2 | MAP | OPTION`. The fix:

- `is_container()` now matches exactly `{ List, List2, Map, Option }`.
- Added `is_primitive()` (`tag & 0x80 == 0`) and used it for the **map-key** check (map keys must
  be primitive, tag `0..=18`), matching the oracle's `is_primitive(keyType)` assertion.

Files changed: `src/bin.rs` (the two helpers) and `src/read.rs` (map-key check now uses
`is_primitive`). No format/layout bytes changed; only the over-strict validation was corrected.

## Step 3 — cross-check vs references

Compared the binary layout and every `BinType` against ritobin
(`bin_io_binary_read.cpp` / `bin_io_binary_write.cpp`, `bin_types_helper.hpp`) and the C#
`LeagueToolkit` `BinTree`, and ran pyritofile for an independent structural diff.

| Aspect | rs_bin | ritobin (C++) | LeagueToolkit (C#) | Agreement |
|---|---|---|---|---|
| Magic PROP/PTCH | yes | yes | yes | match |
| PTCH header | 8 raw bytes kept opaque | `u64 unk` | `u32 ver=1` + `u32 count` | match (8 bytes round-trip either way) |
| version u32 | yes | yes | yes (1/2/3) | match |
| linked/dependencies (v>=2) | `u32` count + `u16` strings | same | same (`ReadInt16` len) | match |
| entry classes block | `u32` count + `u32[count]` | same | same | match |
| entry: `u32` len, `u32` path, `u16` fieldcount | yes | yes | yes | match |
| field: `u32` name + `u8` type + value | yes | yes | yes | match |
| LIST vs LIST2 distinction | preserved (`is_list2`) | distinct types | distinct types | match |
| Map = ordered `Vec<(k,v)>` | yes | ordered items | (dict) | match (we keep order) |
| Map key must be primitive | enforced (`is_primitive`) | `is_primitive` | n/a explicit | match |
| Container element ≠ container | `{List,List2,Map,Option}` | same | same | match (after fix) |
| Pointer vs Embed | separate variants, shared body | separate | separate | match |
| Null pointer (class 0, no body) | yes | yes (`hash()==0` early return) | yes | match |
| Option present/absent (`u8` 0/1) | yes, no size field | yes | yes | match |
| size fields backfilled on write | yes | yes | yes | match |

**pyritofile cross-check** (`pip install xxhash pyzstd` was required): for all three samples
pyritofile reports the *same* `version 3`, `0` links, and entry counts (`55 / 3 / 64`) as rs_bin,
and pyritofile's own read→write is also byte-identical — independent confirmation of the layout.

### Discrepancies / notes

1. **PTCH trailing patches section (real gap).** Both ritobin (`read_patches`) and C#
   (`DataOverrides`) read an additional section *after* the entries when the file is a `PTCH`
   override: a `u32` count followed by patch/override records. **rs_bin does not read or write
   this section.** A `PTCH` file with a non-empty patches section would therefore lose those
   trailing bytes on round-trip. Our samples are all plain `PROP`, so this is untested by real
   data but is a genuine correctness hole for override bins. (rs_bin currently keeps only the 8
   `PTCH` header bytes opaque, which is correct *for the header*, not the trailer.)

2. **Bool/flag byte normalization (robustness, lives in `rs_io`, out of scope here).**
   `rs_io::read_bool` collapses any nonzero byte to `true` and `write_bool` emits `1`. ritobin
   `memcpy`s the raw bool byte. If a real bin ever stored a bool/flag byte other than 0/1, the
   round-trip would rewrite it to `1`. No sample exhibits this; game bins only ever store 0/1.

3. **Non-UTF-8 strings (robustness, `rs_io`).** `read_string_u16` uses `String::from_utf8` and
   errors on invalid UTF-8, whereas ritobin keeps raw bytes. Real bin strings are ASCII/UTF-8
   paths and all samples round-trip; flagged only for completeness.

## Gap analysis vs C# LeagueToolkit

Comparing `rs_bin` against the **C# `LeagueToolkit`** `BinTree` (primary oracle) and ritobin's
`bin_io_*`:

| Capability | C# LeagueToolkit | ritobin | rs_bin (before) | rs_bin (now) |
|---|---|---|---|---|
| PROP/PTCH read + byte-exact write | yes | yes | yes | yes |
| All 27 value type tags | yes | yes | yes | yes |
| LIST vs LIST2 distinction | yes | yes | yes | yes |
| Ordered maps / duplicate keys | dict | ordered | ordered | ordered |
| Null pointer (class 0) | yes | yes | yes | yes |
| Linked files (v>=2) | yes | yes | yes | yes |
| **PTCH `DataOverrides` / patches trailer** | yes (`DataOverrides`) | yes (`read_patches`) | **missing** | **yes** |
| `#PROP_text` **printer** (`to_text`) | n/a (CLI tool) | yes | yes | yes |
| `#PROP_text` **parser** (`from_text`) | n/a | yes (`bin_io_text_read`) | **stubbed** | **yes (full)** |
| `bin -> text -> bin` lossless contract | n/a | yes | **untested** | **proven on all real files** |

The two confirmed gaps the prior report flagged — the **PTCH patches trailer** and the **`from_text`
parser** — were exactly the two things missing relative to ritobin/C#. Both are now implemented.

## What I implemented this round

1. **`from_text` — the full `#PROP_text` recursive-descent parser** (`src/text/parse.rs`). Matches
   ritobin's `bin_io_text_read.cpp` grammar: header line, `name: type = value` sections, every
   scalar and container value, `list[t]`/`list2[t]`/`option[t]`/`map[k,v]` type syntax,
   `Type { fields }` embeds/pointers, `null` pointers, `#` comments, and `,`-or-newline separators.
   Hashes are read as `0xHEX` or as a bareword/quoted string and hashed locally (FNV1a-32 for
   hash/link/field/class names; XXH64 for `file`). Errors are `Error::TextParse { line, message }`;
   it never panics.

2. **PTCH patches / data-overrides trailer (read + write + text).** Added `BinPatch { key_hash,
   path, value }` and `Bin::patches`. The reader consumes the trailing `u32` count and each
   `key:u32, length:u32, { type:u8, path:string, value }` record after the entries when the file is
   a `PTCH`; the writer re-emits it; the printer/parser carry it as a `patches: map[hash,embed]`
   section of `patch { path = ..., value = ... }` records (ritobin's shape). A PTCH file is now
   recognised as *always* carrying this section (matching ritobin), so the prior "drops the trailer"
   hole is closed.

3. **Printer fix needed for a valid grammar.** The `version` line was printed as `version: N`
   (no type), which is not a legal `name: type = value` section and could not be parsed back. It now
   prints `version: u32 = N`, matching the section grammar the parser (and ritobin) expect.

### Real-file text round-trip results

`tests/real_files.rs` now proves, for each real sample, the full chain
`from_path → to_text → from_text → to_bytes == original file bytes`, plus
`text → from_text → to_text` idempotence:

| File | Binary round-trip | `to_text → from_text` reconstructs bin | Re-serialized bytes == original | Text idempotent |
|---|---|---|---|---|
| `aatrox.bin` (PROP/3, 55 entries) | yes | yes | **yes** | yes |
| `aatrox_multi_…skin0…skin8.bin` (3 entries) | yes | yes | **yes** | yes |
| `aatrox_multi_…skin33…skin39.bin` (64 entries) | yes | yes | **yes** | yes |

`cargo test -p rs_bin` is green (16 tests) and `cargo clippy -p rs_bin --all-targets -- -D warnings`
is clean.

### Remaining gaps

- **PTCH patches are untested by real data** — all three samples are plain `PROP`. The trailer is
  covered by a hand-built PTCH fixture (`ptch_patches_round_trip_binary_and_text`) and matches
  ritobin's read/write byte layout, but a real override `.bin` would strengthen confidence.
- **`#PTCH_text` header bytes are canonicalised on text round-trip.** The text form does not carry
  the 8 raw `PTCH` header bytes, so `from_text` reconstructs the canonical `ver=1,count=0` header
  (`[1,0,0,0,0,0,0,0]`), exactly as ritobin emits. `bin → text → bin` is therefore lossless for any
  real override bin (which carries that canonical header) but would not preserve an artificial header
  with other bytes; the *binary* round-trip still preserves the raw 8 bytes exactly.
- **Bool/flag and non-UTF-8 string normalisation** remain `rs_io` foundation follow-ups (unchanged
  from below); no sample exercises them.

## Improvements / TODO

1. ~~Implement the PTCH patches / data-overrides section.~~ **Done** (read + write + text).
2. ~~Implement `from_text` (the `#PROP_text` recursive-descent parser).~~ **Done**; the text
   round-trip contract `bin → text → bin` is now proven byte-identical on all real samples.
3. **Lossless bool/flag and non-UTF-8 string handling** — would require a small change in `rs_io`
   (store the raw bool byte; keep string bytes), so tracked as a foundation-crate follow-up rather
   than an rs_bin edit. Still open.

## What I changed

- **Fixed** `BinType::is_container()` to mean exactly `{List, List2, Map, Option}` and added
  `BinType::is_primitive()`; switched the map-key validation to `is_primitive`. This fixed the
  parse failure on all three real files (previously rejected `list`/`map`/`option` of
  `pointer`/`embed`/`link`).
- **Added** `crates/rs_bin/tests/real_files.rs`: per-file parse + byte-exact round-trip + text
  printer checks, with a skip-if-missing helper.
- **Added** this report and `crates/rs_bin/README.md`.
- **Implemented `from_text`** (`src/text/parse.rs`) and the **PTCH patches trailer** (read in
  `src/read.rs`, write in `src/write.rs`, text in `src/text/{print,parse}.rs`); added
  `BinPatch` + `Bin::patches` to `src/bin.rs`; fixed the `version` section to print a valid
  `version: u32 = N` form.
- **Extended** `tests/real_files.rs` with full text round-trip + idempotence, and added text /
  PTCH-patches tests to `tests/roundtrip.rs`.
- No changes to source outside `rs_bin`. No new `rs_io`/`rs_hash` helpers were required —
  `from_text` reuses `rs_hash::{fnv1a, xxh64}` and parses scalars with the standard library.
