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

## Gap analysis vs C#

Measured `rs_wad`'s public surface against the C# `WadFile`/`WadChunk` oracle:

| C# capability | rs_wad before | rs_wad now |
|---|---|---|
| `FindChunk(ulong)` | scan `wad.chunks` by hand | `chunk_by_hash(u64) -> Option<&WadChunk>` |
| `FindChunk(string)` (XXH64 lowercased) | none | `chunk_by_path(&str)` (hashes via `rs_hash::xxh64`) |
| `Subchunks` / `LoadChunkDecompressed` for `ZstdChunked` via `.subchunktoc` | streaming heuristic only | explicit `.subchunktoc` parse + per-sub-chunk decode |
| bulk `LoadChunkDecompressed` over many chunks | none | `extract_all` / `extract_selected` (rayon-gated) |
| 32-byte v3 TOC layout (`is_duplicated` + u16 index) | already corrected (prior pass) | unchanged, matches oracle |

The C# reader derives the `.subchunktoc` path from the WAD's own location under `Game/`
(`Path.ChangeExtension(relativePath, "subchunktoc")`) and looks it up by XXH64. We confirmed this on
the real Azir archive: the chunk `data/final/champions/azir.wad.subchunktoc` exists and
`chunk_by_path` resolves it. The decode (stored sub-chunk ⇒ copy, else one zstd frame) mirrors the
oracle's `DecompressZstdChunkedChunk`.

No new TOC/v3 bug was found versus the oracle; the previously-fixed single-layout v3 entry remains
correct, and a synthetic mixed stored/zstd sub-chunk test guards the explicit path.

## What I implemented

- **Lookup API.** `Wad::chunk_by_hash(u64)` and `Wad::chunk_by_path(&str)` (XXH64 of the lowercased
  path), both `Option<&WadChunk>`, never panicking.
- **`.subchunktoc` support.** `WadSubchunk` (16-byte entry), `Wad::parse_subchunk_toc(&chunk)` and
  `Wad::subchunk_toc_for_path(&str)` to load the table, `Wad::chunk_data_with_toc(&chunk, &toc)` and
  the free `decompress_zstd_multi_with_toc` to decode a multi chunk with explicit sub-chunk sizes.
- **Bulk extractor.** `Wad::extract_all()` and `Wad::extract_selected(hashes)` returning
  `HashMap<u64, Vec<u8>>`, gated behind a new `parallel` feature (rayon); sequential by default, no
  `unsafe`.
- **Real-file validation.** New tests in `tests/real_files.rs`: `azir_lookup_and_extract` (look up
  by hash + by the real subchunktoc path, decompress, assert lengths, bulk-extract a subset) and
  `azir_subchunktoc_decode` (decode 200 real zstd-multi chunks via the explicit TOC and assert
  byte-identical to the heuristic). Verified on real data: the Azir TOC has 3243 entries (matching
  the highest sub-chunk index referenced) and explicit-vs-heuristic mismatches were 0.

## Remaining gaps / TODO

1. **mmap-backed zero-copy reads.** `from_reader` copies the whole data section into a `Vec`. For
   the `from_path` route, borrow from the mmap and slice chunk ranges directly to avoid the
   multi-MB copy, while keeping the owned form for the in-memory editing case. The bulk extractor
   would then decode straight from the borrowed slices.
2. **`.subchunktoc` path derivation.** `subchunk_toc_for_path` takes the caller-supplied lowercased
   path because the archive stores only hashes and the base name comes from the WAD's location under
   `Game/`. A higher-level mount that knows the on-disk path (like C#'s `WadFile`) could derive and
   load it automatically. A hash-dictionary-driven scan for any chunk whose resolved name ends in
   `.subchunktoc` is the other option once `rs_hash::HashMapper` dictionaries are wired in.
3. **`chunk_by_hash` is a linear scan.** Fine for the few-thousand-entry archives here and it keeps
   the round-trip-preserving `Vec` order as the single source of truth, but a mount layer could
   build a `HashMap` index for hot repeated lookups.
4. **Lookup duplicate handling.** The TOC may legally contain duplicate path hashes (`is_duplicated`
   chunks); `chunk_by_hash` returns the first. C# rejects duplicates outright. Neither sample has
   any, so this is untested against real data.

## Earlier changes (prior pass)

- **Fixed the v3.4 TOC layout.** Removed the fabricated 24-bit subchunk-start reading/writing and
  unified all v3 minors on the oracle's `is_duplicated (u8) + first_subchunk_index (u16)` layout.
  Round-trip remains byte-exact; the synthetic v3.4 round-trip test still passes unchanged.
- **Added `tests/real_files.rs`** with a skip-if-missing helper: parses both real archives, asserts
  a whole-file byte-exact round-trip, and decompresses a sample of chunks.
- **Added the crate `README.md` and this report.**

All `cargo test -p rs_wad` pass (default and `--features parallel`): 13 roundtrip/unit + 4 real-file,
and `cargo clippy -p rs_wad --all-targets -- -D warnings` is clean in both configurations.
