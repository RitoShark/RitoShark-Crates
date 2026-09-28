# Animation fixture coverage

Verified 2026-09-28 with `cargo test -p rs_anim -- --nocapture`: 46 tests passed.
Game assets remain local and gitignored under `sample-files/`.

## Real files

All six animation fixtures decode and round-trip byte-for-byte. Their sampled rotations
are unit length within tolerance and translation/scale components remain finite.

| Animation | Container | Tracks | FPS |
|---|---|---:|---:|
| `aatrox__skin07_ult_attack1.anm` | Uncompressed v5 | 136 | 30 |
| `aatrox_sheath_run_haste.anm` | Uncompressed v5 | 111 | 30 |
| `dance_windup.anm` | Uncompressed v5 | 107 | 30 |
| `compressed_507c1f34b053b389.anm` | Compressed | 119 | 30 |
| `compressed_e890878834c561be.anm` | Compressed | 25 | 30 |
| `compressed_e63f4f2e8c074937.anm` | Compressed | 21 | 30 |

All four modern skeletons decode and round-trip byte-for-byte:

| Skeleton | Joints | Influence entries |
|---|---:|---:|
| `azir.skl` | 183 | 155 |
| `azirpair.skl` | 154 | 133 |
| `azir_small.skl` | 1 | 1 |
| `janna_skin67.skl` | 361 | 297 |

Pose tests also exercise bind-pose skinning, partial clips, and arbitrary-time sampling.
Small discrepancies in real inverse-bind transforms are tolerated and reported by tests.
Byte-exact preservation alone does not prove every decoded transform is correct.

## Synthetic coverage

- Legacy skeleton v1 implicit influences and v2 explicit/empty influences.
- Rotated/scaled parent bind matrices, local reconstruction and inverse binds.
- Legacy source preservation and edited conversion to modern v0.
- Modern reserved/trailing-byte preservation and automatic edit detection.
- ANM v3/v4 and compressed v1/v2/v3 source preservation.
- V5 unnamed tracks without frame-row misalignment.
- 65,536 distinct vector/quaternion entries accepted; entry 65,537 rejected.
- 70,000 playback frames, independent of palette capacity.
- 32,768 joints with and without influences; 32,769 rejected by the writer.
- Truncated legacy records, invalid hierarchy/influences, oversized animation counts,
  reversed sections, invalid compressed timing, and truncated sparse streams.
- Compressed constant channels, parametrized interpolation and absent-channel sentinels.

## Remaining coverage limits

Legacy SKL and ANM v3/v4 coverage uses synthetic fixtures. No executable C# differential
test was run; reference review compared source layouts and algorithms. These tests do not
establish League client limits or validate Jade rendering. Unknown versions remain
unsupported. See [formats and limits](formats-and-limits.md) for field-width bounds.
