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

## Improvements / TODO (priority order)

1. ~~**Compressed `r3d2canm` reading**~~ — **done.** Remaining follow-up: validate against a real
   compressed file once one is available, and consider matching League's spline interpolation
   instead of lerp/slerp for exact fidelity.
2. **Add real fixtures for the uncovered paths.** There is no `.skl` sample and no compressed
   `.anm` sample. The skeleton reader/writer and the new compressed reader are only synthetically
   tested. Drop a real `.skl` and a real compressed `.anm` into `sample-files/` and extend
   `real_files.rs`.
3. **Compressed/full byte-exact round-trip is not yet a contract.** Writing always re-emits as
   uncompressed v4, so `read → write` is not byte-identical for v3/v5/compressed inputs. If
   lossless byte round-trip per container becomes a requirement, add format-preserving writers
   (v5 palette writer, compressed writer) rather than always normalizing to v4.
