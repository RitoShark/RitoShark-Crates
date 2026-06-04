# rs_anim — real-files report

Generated against the samples in `sample-files/` (gitignored). Reproduce with:

```
cargo test -p rs_anim --test real_files -- --nocapture
```

## Per-file results

| File | Magic | Version | Result | Tracks × Frames |
|---|---|---|---|---|
| `aatrox__skin07_ult_attack1.anm` | `r3d2anmd` | 5 | parsed | 136 × 81 |
| `aatrox_sheath_run_haste.anm`    | `r3d2anmd` | 5 | parsed | 111 × 28 |
| `dance_windup.anm`               | `r3d2anmd` | 5 | parsed | 107 × 21 |

**All three samples are uncompressed `r3d2anmd` version 5** — contrary to the working hypothesis
that the available samples would be compressed. Our reader parses every one, with track and frame
counts matching the header and the section offsets (`vecs(@64) -> quats -> joint_hashes -> frames`).
Derived section sizes (`vecCount = (quats-vecs)/12`, `quatCount = (jointHashes-quats)/6`,
`jointHashCount = (frames-jointHashes)/4`) line up exactly with the declared counts, so the v5
section-order assumption in the reader is confirmed against real data.

No `.skl` sample ships, and no compressed `.anm` exists anywhere in the sample set or in the
reference libraries on disk — so the **compressed reader and the skeleton reader have no real-file
coverage** and are validated only by synthetic tests (see below).

## Cross-check vs references

The reader was compared against three sources:

- **C# LeagueToolkit** (`CompressedAnimationAsset.cs`, `UncompressedAnimationAsset.cs`,
  `CompressedFrame.cs`, `ErrorMetric.cs`) — the trusted behavioral oracle.
- **`pyritofile`** (`anm.py`) — a second independent reader/writer.
- **Rust `ltk_anim`**.

Findings:

1. **Uncompressed v3/v4/v5** — our layout, offsets, and quantization match both the C# oracle and
   `pyritofile`. The v5 frame palette indices are `(translate, scale, rotate)` u16 triples; v4
   embeds the joint hash per row plus a padding u16. Confirmed correct.
2. **48-bit quantized quaternion** — `quantized.rs` matches `anm.py` bit-for-bit (same shift
   layout, same `SQRT2/32767` scale, same `1/√2` bias, same max-index reconstruction).
3. **Compressed `r3d2canm`** — header layout matches the C# oracle exactly: resourceSize,
   formatToken, flags, joint/frame/jump-cache counts, max-time + fps, **three** error metrics
   (2 floats each), translation `min`/`max`, scale `min`/`max`, then `(frames, jumpCaches,
   jointHashes)` offsets. Frame record is `time:u16, (jointId|type):u16, value:u8[6]` with the
   joint id in the low 14 bits and the transform type in the top 2 bits.

### Discrepancies / decisions

- **Evaluation strategy.** The C# oracle does proper streaming curve evaluation with hot-frames
  and Catmull-Rom-style tangents (and an optional keyframe-parametrization flag), driven by the
  jump-cache table. `pyritofile` instead does the simpler thing: collect sparse per-component keys
  and lerp/slerp them onto integer frames. Our `Animation` model stores explicit
  rotation+translation+scale per frame (not curves), so we adopt the `pyritofile` resampling
  approach. This is **lossy relative to League's exact sampler** (linear vs spline interpolation
  between keys) but produces a faithful, usable dense animation and matches what most modding
  tools expect. The jump-cache table is skipped (it only accelerates seeking).
- **`pyritofile` vs C# fps/duration.** `pyritofile` computes `duration = (max_time + 1) * fps` and
  pose time as `compressed_time/65535 * max_time * fps`. The C# `Duration` field is the raw
  `max_time`. We follow the C# header meaning (the f32 we read is `max_time`) and use
  `pyritofile`'s frame-domain time mapping for resampling. Output frame count is
  `round(max_time * fps) + 1`.

## What I changed

- **Implemented the compressed `r3d2canm` reader** (`animation_read.rs`,
  `Animation::read_compressed`) for versions 1–3. It parses the full header, dequantizes the
  sparse rotation/translation/scale keys, and resamples them into explicit per-frame
  `AnimFrame`s, reusing `decompress_quat` and the new `decompress_vec3`. Malformed input returns
  `Err` (bad joint ids and unknown transform types are handled; no panics, no unwraps on file
  data).
- **Added `quantized::decompress_vec3`** (min/max linear dequantization of a 48-bit vector),
  mirroring the existing quaternion codec.
- **Added `tests/real_files.rs`** — skip-if-missing helper, prints magic+version per file, asserts
  non-empty tracks/frames for parsed files, round-trips the parsed structure through the writer,
  and asserts `Unsupported` only for genuinely compressed inputs (none in this sample set).
- **Added a synthetic compressed-parse test** (`compressed_animation_parses` in
  `tests/roundtrip.rs`) with a hand-built one-joint `r3d2canm` buffer, since no real compressed
  sample exists. It checks the identity rotation, midpoint translation, and midpoint scale decode
  to ~(1,1,1).
- Updated the old `compressed_animation_is_unsupported` test (a zeroed header now parses) to
  `compressed_animation_unknown_version_is_unsupported`, asserting an out-of-range version errors.
- Refreshed the crate/module docs to state that compressed anm is now decoded.

## Gap analysis vs C#

Cross-read against `UncompressedAnimationAsset.cs`, `CompressedAnimationAsset.cs`,
`CompressedFrame.cs`, `Animation.cs`, `ErrorMetric.cs`, `QuantizedQuaternion.cs`, and `RigResource.cs`.

- **Uncompressed v5 layout** matches the oracle field-for-field. The only thing the C# reader
  discards that we now keep is the *exact byte form* of each section: it normalizes the quaternion
  palette on read (`Quaternion.Normalize(QuantizedQuaternion.Decompress(...))`) and never writes v5
  back, so it cannot byte-round-trip a v5 file at all. We close that by preserving the raw sections.
- **Physical layout.** Real v5 files place the first data section at byte 76 — a 64-byte header
  plus a **12-byte zero pad** (the unused asset-name/time region; both offsets are 0). Section
  offsets are stored relative to byte 12. Our writer reproduces the pad and the
  `vecs -> quats -> jointHashes -> frames` order exactly.
- **Compressed `r3d2canm`.** Header, `CompressedFrame` (`time:u16`, `jointId|type:u16`,
  `value:ushort[3]`), `DecompressVector3` (`(max-min) * v/65535 + min`), and
  `QuantizedQuaternion.Decompress` all match. `Duration` is the raw `max_time` f32; we map key time
  to frames via `compressed_time/65535 * max_time * fps`. The quaternion codec (`quantized.rs`)
  matches `QuantizedQuaternion` bit-for-bit (`a = v/32767*√2 - 1/√2`, max-index reconstruction).
- **Skeleton joint-id-hash ordering (bug fixed).** The C# writer emits the joint-id-hash table
  ordered by `Elf.HashLower(name)` **ascending** (`.OrderBy`). Our writer previously sorted
  **descending**, which would corrupt byte-exactness on any real `.skl` with more than one joint.
  Now sorted ascending. Validated by `skeleton_joint_index_section_sorted_by_hash_ascending`.
- **ELF hash ownership (done).** The League bone-name ELF hash now lives in `rs_hash`
  (`elf` / `elf_lower`, alongside FNV1a/XXH64). `animation_read.rs` calls `rs_hash::elf_lower` for v3
  track names; the inlined copy was removed. `elf_lower` lowercases ASCII first, matching the C#
  `Elf.HashLower` used by `RigResource`/`Joint` and `pyritofile`'s `Elf`.

## What I implemented

- **Shared ELF hash.** Removed the inlined `fn elf` from `animation_read.rs` and switched the v3
  track-name → joint-hash derivation to `rs_hash::elf_lower`. This is a **correctness fix**: the old
  inlined hash did *not* lowercase, but the C# oracle (`RigResource`/`Joint` → `Elf.HashLower`) and
  `pyritofile` (`Elf`) both lowercase first, so any mixed-case v3 track name previously produced the
  wrong joint hash. The `.skl` joint-id-hash table already sorts ascending by the stored (lowercased
  ELF) hash, matching `RigResource`'s `.OrderBy(Elf.HashLower)`. A new
  `joint_hash_matches_oracle_hash_lower` test pins `elf_lower("root") == 0x00079664` and
  case-insensitivity.
- **Format-preserving writers for every accepted container — no lossy normalization.** Replaced the
  v5-only `raw::RawV5` with `raw::RawAnim`, which holds the **complete source byte buffer**.
  `Animation::from_reader` reads the whole stream into that buffer, decodes the tracks from a cursor
  over it, and keeps the buffer. An unedited `read -> write` replays it verbatim, so the round-trip
  is byte-exact for uncompressed **v3, v4, v5** *and* compressed **`r3d2canm`**. The previous write
  path that re-emitted v3/v4/compressed as uncompressed v4 is gone for unmodified files; there is no
  `Unsupported` stub on the write path. `make_editable()` drops the buffer to opt into a v4 re-emit
  from the decoded tracks (full quaternions, no quantization loss); `is_byte_exact()` reports whether
  the buffer is still present.
- **Tests.** Kept the three real v5 samples byte-exact (`tests/real_files.rs` now asserts
  byte-exactness for *every* parsed file, not just v5). Added synthetic `r3d2anmd` v3 and v4 builders
  and a compressed `r3d2canm` builder, each with a `read -> to_bytes -> ==` byte-exact test
  (`uncompressed_v3_round_trips_byte_exact`, `uncompressed_v4_round_trips_byte_exact`,
  `compressed_round_trips_byte_exact`), plus `make_editable_drops_byte_exact_layout` and the
  joint-hash oracle test.
- **Stronger compressed validation.** `compressed_animation_recovers_known_rotation` builds a
  `r3d2canm` buffer from `compress_quat` of a non-identity rotation and asserts the reader recovers
  it (within the codec's quantization).
- **Skeleton joint-id-hash sort fix** (ascending by hash) + a regression test (from a prior pass).

## Remaining gaps

- No real `.skl` or compressed `.anm` sample exists; those paths remain synthetic-only. Byte-exact
  passthrough of an *unmodified* compressed file is guaranteed by source-byte preservation, but
  re-emitting an *edited* compressed model (re-quantizing + re-deriving jump caches/error metrics) is
  not implemented — `make_editable` falls back to uncompressed v4, which is lossless for the values.
- Compressed resampling on read is lerp/slerp, not League's spline/hot-frame sampler (lossy vs
  exact; documented, acceptable for modding). This only affects the decoded tracks, never the
  byte-exact passthrough.
- Skeleton legacy `r3d2sklt` reads still return `UnsupportedVersion` (honest — no sample exists).

## Improvements / TODO (priority order)

1. **Add real fixtures for the uncovered paths.** There is no `.skl` sample and no real compressed
   `.anm` sample. The skeleton reader/writer and the compressed reader/passthrough are only
   synthetically tested. Drop a real `.skl` and a real compressed `.anm` into `sample-files/` and
   extend `real_files.rs`.
2. **Edited-compressed re-emit.** If round-tripping a *modified* compressed animation back to
   `r3d2canm` (rather than falling back to v4) becomes a requirement, implement a compressed writer
   (component-keyed quantization, jump-cache + error-metric synthesis). Today edits emit v4, which is
   lossless for the decoded values.
