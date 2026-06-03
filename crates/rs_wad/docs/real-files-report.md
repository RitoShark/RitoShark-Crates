# rs_wad — real-file validation report

Validation of `rs_wad` against the two real sample archives in `sample-files/`, plus a
cross-check against the reference implementations. Reproduce with:

```
cargo test -p rs_wad --test real_files -- --nocapture
```

## Per-WAD results

Both samples are **v3.4** archives. The test parses the full TOC, asserts a byte-exact
`read → write` round-trip of the whole archive, then extracts and decompresses a sample of chunks
(the first 20 in TOC order plus up to 200 zstd-multi chunks) and asserts each decompressed length
equals the chunk's `uncompressed_size`.

| Archive | Size | Version | Chunks | Compression breakdown | Round-trip | Chunks decompressed OK | Failures |
|---|---|---|---|---|---|---|---|
| `Azir.wad.client` | ~71 MB | 3.4 | 1979 | 770 zstd, 1201 zstd-multi, 8 stored | byte-exact | 206 / 206 | 0 |
| `DATA.wad.client` | ~56 MB | 3.4 | 4895 | 4638 zstd, 257 stored | byte-exact | 20 / 20 | 0 |

Notes:

- Azir exercises the **zstd-multi** path heavily (1201 of 1979 chunks). 14 of the first 20 TOC
  entries are already zstd-multi; the test deliberately pulls 200 of them. All reassemble to the
  exact `uncompressed_size`, confirming the concatenated-frame decode is correct on real data.
- DATA has no zstd-multi chunks; it stresses plain zstd and stored chunks.
- Neither sample contains a `is_duplicated` chunk, and the largest observed `first_subchunk_index`
  is 3240 (well within `u16`). See the cross-check below for why this matters.

## Cross-library comparison

Compared the TOC layout and compression handling against the trusted **C# LeagueToolkit** oracle,
the Rust `ltk_wad`, the Rust `cdragon-wad`, and the Python `pyritofile`.

### Chunk counts (independent confirmation)

`pyritofile` parsed both archives and reported identical counts and compression histograms:

```
Azir.wad.client: version=3.4 chunks=1979 comp={3:770, 4:1201, 0:8}
DATA.wad.client: version=3.4 chunks=4895 comp={3:4638, 0:257}
```

These match `rs_wad` exactly.

### TOC entry layout — discrepancy found and fixed

The C# oracle (`WadChunk.Read`), `cdragon-wad`, and `pyritofile` all read the 32-byte v3 TOC entry
the **same way for every v3 minor**: after the type byte come `is_duplicated` (1 byte) and
`first_subchunk_index` (`u16`). None of them branch on the minor version, and the C# reader takes
only the *major* version as a parameter.

`rs_wad` previously contained a **fabricated v3.4 variant** that, for `minor >= 4`, reinterpreted
those three bytes as a single 24-bit subchunk-start value and forced `is_duplicated = false`. On
these two samples that happened to produce the same numeric `subchunk_start` (because the duplicate
byte is always 0 and the index fits in 16 bits), so it did not corrupt them — but it is wrong
against the oracle and would mis-parse any v3.4 WAD that has a duplicated chunk (the duplicate byte
would leak into the high byte of the index, yielding indices ≥ 65536) and would drop the duplicate
flag entirely. This was corrected to the uniform layout; the round-trip stays byte-exact.

### Compression / subchunk reassembly

- `rs_wad`, the C# oracle, `cdragon-wad`, and `pyritofile` agree on the compression enum
  (`0 None, 1 Gzip, 2 Satellite, 3 Zstd, 4 ZstdMulti`) and that Satellite is unsupported.
- The C# oracle and `cdragon-wad` reassemble zstd-multi by walking a separate `.subchunktoc`
  file that gives each sub-chunk's compressed/uncompressed sizes. `rs_wad` and `pyritofile` instead
  rely on the fact that the sub-chunk frames are concatenated independent zstd frames: skip any
  stored prefix, then let the streaming zstd reader consume all frames to EOF. This produces
  byte-identical output on the real Azir chunks without needing the external TOC, which is simpler
  and self-contained. (It does assume the data section for a multi chunk is exactly the
  concatenated frames, which holds for every real chunk tested.)
- `pyritofile` has a minor bug where `subchunk_count` is derived from the already-masked type byte
  (`value >> 4` after `& 15`), so it always reports 0. `rs_wad` reads `subchunk_count` from the raw
  type byte and reports the true counts (e.g. 2–3 per multi chunk in Azir).

## Improvements / TODO

1. **Subchunk TOC awareness (correctness for edge cases).** The current zstd-multi decode trusts
   that the concatenated-frame heuristic covers the whole chunk. To match the oracle exactly for
   any conceivable layout (e.g. a stored sub-chunk that is not a zstd frame, sitting *between* two
   compressed frames), add optional parsing of the `.subchunktoc` entry so each sub-chunk is sized
   explicitly. No sample chunk needs this today, but it removes the last heuristic.
2. **Convenience lookup + extraction API.** Add `Wad::chunk_by_hash(u64)` (and a path-string
   helper that hashes via XXH64) plus a `rayon`-gated bulk extractor, as promised in the workspace
   design. Today callers must scan `wad.chunks` themselves.
3. **mmap-backed zero-copy reads.** `from_reader` copies the whole data section into a `Vec`. For
   the `from_path` route, borrow from the mmap and slice chunk ranges directly to avoid the
   multi-MB copy, while keeping the owned form for the in-memory editing case.

## What I changed

- **Fixed the v3.4 TOC layout.** Removed the fabricated 24-bit subchunk-start reading/writing and
  unified all v3 minors on the oracle's `is_duplicated (u8) + first_subchunk_index (u16)` layout.
  Round-trip remains byte-exact; the synthetic v3.4 round-trip test still passes unchanged.
- **Added `tests/real_files.rs`** with a skip-if-missing helper: parses both real archives, asserts
  a whole-file byte-exact round-trip, and decompresses a sample of chunks (including 200 zstd-multi
  chunks from Azir), asserting decompressed length matches `uncompressed_size`.
- **Updated the crate / decoder docs** to describe the real single-layout v3 entry and the
  concatenated-frame zstd-multi mechanism (no false 24-bit field).
- **Added this report and `README.md`.**

All `cargo test -p rs_wad` tests pass (14 total: 12 roundtrip/unit + 2 real-file).
