# CANM authoring findings

Recorded 9 November 2026. These findings concern writing fresh animations from
uniformly sampled frames. They do not describe Riot's proprietary authoring tools.

## Reference motion

The comparison's **frames** column uses decoded frame samples, linear translation
and scale interpolation, and normalized shortest-path spherical rotation
interpolation. This is the relevant input for ordinary baked DCC exports.

The **curves** column used evaluations of the original CANM between those frames.
It did not copy its original key layout, key budget or quantization bounds. It is
a different motion reference: successful curve fitting does not prove that the
frame-baked version can be reproduced at the same error or size.

Compression errors against frames exclude motion already lost during frame baking.
Normal unedited byte-exact saves remain the appropriate way to preserve an existing
file without changing it.

## Reviewed frame-input results

The last two isolated experiments tested 16 clips: five Evelynn skin66, five Kayn
skin20, two Hwei base, two Mordekaiser base and two Yuumi base. Fourteen passed as
CANM; Evelynn recall and Mordekaiser recall required verified dense v5 alternatives.

The limits remained 2 degrees local rotation, 0.5 maximum translation-component
error, 0.005 maximum scale-component error, and 1 unit of model-space error at joint
origins and unit-axis probes. Translation and model-space units come from the input
(centimeters in the Maya comparisons). This is not a skinned-mesh surface bound.
Tracks absent from the supplied skeleton remain in the output and receive local
checks, but cannot receive a meaningful hierarchy check.

Selected reviewed sizes, in decimal kB:

| Clip | Riot original | Frame-input CANM | Dense v5 alternative |
| --- | ---: | ---: | ---: |
| Hwei recall | 190.35 | 159.57 | 601.66 |
| Hwei R | 103.56 | 82.61 | 252.73 |
| Mordekaiser recall | 199.23 | rejected | 422.13 |
| Mordekaiser Q | 46.40 | 49.64 | 46.26 |
| Yuumi recall | 256.66 | 159.28 | 553.98 |
| Yuumi dance | 168.33 | 97.64 | 275.02 |
| Evelynn recall | 654.46 | rejected | 1383.09 |
| Evelynn dance | 1141.04 | 2362.33 | palette overflow |
| Kayn recall | 163.96 | 173.92 | — |

These are the frozen experimental writer's measurements, retained as the baseline
for the library integration. Smaller than Riot does not imply better fidelity;
larger does not by itself imply incorrect motion. CANM is not always smaller than
v5, especially when sparse-key and seek-cache overhead outweigh reduction.

## Format limits that fitting cannot remove

CANM stores time in a 16-bit clock. Distinct continuous source times can map to
one tick and therefore one decoded pose. Half the distance between those source
poses is a necessary lower bound on the worst achievable error.

For baked Evelynn recall, two positions of the Glass probe sharing a tick were
38.40681 units apart, establishing a 19.203405-unit lower bound against the
1-unit target. Mordekaiser recall produced a 1.698292-unit lower bound on R_Pinky1.
Keeping more keys cannot solve either witness. The original CANM curves are a
different reference, so this does not imply that Riot's original file is invalid.

Dense v5 uses separate 16-bit rotation and combined translation/scale palettes;
each supports 65,536 entries, not just 65,536 total frames. V4 also uses 16-bit
palettes. V4 can avoid v5's packed-rotation error but cannot solve every palette
overflow. If all allowed formats fail, export must return an error.

## Retained fitting and verification behavior

- Fit source motion at each CANM tick's midpoint; preserve the clip endpoints.
- Use the playback cubic and quantized values when removing or relocating keys.
- Tighten tolerances only in affected time windows and ancestor channels.
- Keep neutral scale exactly representable where the bounds permit it.
- Restore both endpoints when a formerly constant channel becomes animated.
- Preserve every animation track and every transform component, including props.
- Reject known clock-precision failures before fitting.
- Check continuous source motion on 96 intervals per frame and near both edges of
  every clock cell; decode the final bytes on both 96- and 97-interval grids.
- Use the runtime's final four-entry interpolation window, including its floating
  point behavior, rather than assuming the final decoded value is a simple clamp.

The library integration streams final decoding and caches only the channel being
fitted. The research writer's full tick cache needed approximately 883 MB for
337 tracks before other fitting/validation allocations; the replacement channel
cache is 1 MiB independent of the number of tracks. Overall process memory also
includes input frames, key candidates, decoder data and hierarchy checks.

## Public API policy

`Animation::export_bytes` requires an explicit initial format through
`AnimationExportOptions::new(format)`:

- CANM tries CANM, then v5, then v4.
- V5 tries v5, then v4.
- V4 tries only v4.

Each successful candidate must meet the same requested quality checks. Return the
actual format, measured errors and all fallback reasons alongside the bytes.
Invalid input, invalid hierarchy and I/O errors are not compression fallbacks.
Do not change the tolerances automatically or disguise v5/v4 output as CANM.
Selection is a preference chain, not automatic smallest-file selection.

Older format-specific methods remain strict and keep their previous defaults.
The new Python crate API exposes the report for downstream applications. Adding a
selector or other behavior to Aventurine is a separate step.

## Remaining work

Evelynn dance remains a size outlier. Further key placement or joint-aware fitting
may improve it without increasing tolerances. Numerical checks are sampled, and
the newest integration still needs visual/in-game review before release. The
research results do not establish the exact procedure used by Riot's exporter.

## Library integration verification

The public Rust API produced 14 CANM outputs and the expected two v5 fallbacks on
all 16 frame-input fixtures. Every output is byte-for-byte identical to the
reviewed experimental candidate or its reviewed v5 alternative. Finished CANM
bytes now also pass the actual decoder at both edges of every clock cell, alongside
the two subframe grids. Quality limits were not increased.

The standalone export process used approximately 7.8?24.6 MiB peak working memory
in this batch, including its inputs. This is not a measurement of Blender/Maya or
Python process memory. The per-channel fitting cache itself is 1 MiB.

| Frame input | Actual format | Bytes | Export seconds | Peak working MiB |
| --- | --- | ---: | ---: | ---: |
| hwei_01_recall | canm | 159570 | 53.8 | 11.5 |
| hwei_02_spell4_alex | canm | 82606 | 34.9 | 8.6 |
| mordekaiser_01_recall | v5 | 422132 | 1.8 | 9.2 |
| mordekaiser_02_spell_1 | canm | 49638 | 16.9 | 8.3 |
| yuumi_01_yuumi_recall01 | canm | 159278 | 73.0 | 11.5 |
| yuumi_02_yuumi_dance_loop | canm | 97638 | 17.6 | 10.1 |
| evelynn_01_recall | v5 | 1383092 | 4.3 | 17.0 |
| evelynn_02_run | canm | 172958 | 26.0 | 9.2 |
| evelynn_03_attack1 | canm | 141546 | 26.4 | 9.9 |
| evelynn_04_spell4 | canm | 108612 | 20.0 | 8.5 |
| evelynn_05_dance | canm | 2362334 | 234.2 | 24.5 |
| kayn_01_recall | canm | 173918 | 21.9 | 9.2 |
| kayn_02_run | canm | 44748 | 8.5 | 8.6 |
| kayn_03_attack1 | canm | 59320 | 11.0 | 9.1 |
| kayn_04_spell1_circle | canm | 30706 | 7.2 | 8.8 |
| kayn_05_spell2 | canm | 37616 | 7.2 | 7.8 |

Times are observations from this local machine, not isolated performance
benchmarks; other build/check work overlapped parts of the run. Evelynn dance still
took about four minutes and remains the main speed/size outlier.

Final checks of the animation-only publication snapshot passed 431 Rust tests
across the workspace and 82 Python tests. One Rust test and 98 Python tests
requiring optional external fixtures were skipped. Full-workspace build, Clippy
with warnings denied, and formatting checks passed. Tests cover explicit format
selection, both fallback steps, unrecoverable palette overflow, invalid inputs and
hierarchies, retained-source behavior, destination preservation, clock witnesses,
neutral scale, single-frame streams, and wide seek indices. A final built Python
wheel additionally reproduced Hwei R's CANM and Evelynn recall's v5 alternative
byte-for-byte through `Anm.export`.

The publication wheel also loaded under Blender 4.0's bundled Python 3.10 and
Maya 2023's Python 3.9. Each interpreter exercised CANM output and both fallback
steps. These are API/ABI smoke checks; no new Maya or in-game visual review was
performed during library integration.
