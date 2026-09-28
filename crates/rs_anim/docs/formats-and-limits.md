# Skeleton and animation layouts and limits

All integers and floats below are little-endian. These are representational limits inferred
from file fields and reference implementations, not measured League client limits. Memory,
renderers, authoring tools, and file size can impose much smaller ceilings.

## Quick answers

| Question | Answer |
|---|---|
| Is animation limited to 65,535 frames? | No. Uncompressed frame counts occupy 32 bits. A 70,000-frame clip is covered by a round-trip test. |
| What does the 16-bit animation limit apply to? | V4/v5 palette indices: 65,536 vector entries and 65,536 quaternion entries. Repeated values reuse indices across frames and tracks. |
| How many joints can a modern skeleton identify? | 32,768 nonnegative signed-16-bit IDs, numbered 0 through 32,767. Its unsigned-16-bit count field alone can hold 65,535, but that exceeds the nonnegative ID namespace. |
| How many joints can deform a standard SKN? | Its byte-sized blend indices address 256 influence slots, each mapping to a skeleton joint. The stored influence table may be longer. |
| How many weights can one SKN vertex have? | Four index/weight pairs; fewer nonzero weights are allowed. |
| Do joints without influences count toward the skeleton limit? | Yes. Helper, attachment, and parent joints use the same ID namespace. They need not appear in the influence table. |
| Does removing the influence table increase the joint limit? | No. A viewer using direct indices still has only 256 byte-addressable joint slots; the remaining rig joints can still be animated or parent other joints. |
| How many joints can compressed ANM keys address? | 16,384, using 14 bits per key. This is separate from SKL joint IDs. |

An animation track is identified by a 32-bit lowercase ELF joint-name hash, not an influence
index. Hash collisions can make different names ambiguous. `Pose` binds tracks by hash and
retains rest transforms for joints without matching tracks.

## Skeletons and influences

### Modern SKL v0

The 64-byte header starts with `file_size:u32`, magic `0x22FD4FC3:u32`, and `version:u32`.
Then come `flags:u16`, `joint_count:u16`, `influence_count:u32`, six signed 32-bit section
offsets, and five reserved signed offsets. Offsets are absolute from the file start.

Each 100-byte joint record contains:

| Field | Bytes |
|---|---:|
| Flags, joint ID, parent ID, padding | 2 + 2 + 2 + 2 |
| Name hash and radius | 4 + 4 |
| Local translation, scale, quaternion | 12 + 12 + 16 |
| Inverse-bind translation, scale, quaternion | 12 + 12 + 16 |
| Relative name offset, measured from this offset field | 4 |

Joint and parent IDs are `i16`; parent `-1` marks a root. The joint-index section uses
8-byte `(id:i16, padding:i16, hash:u32)` records sorted by hash. Influence entries occupy
two bytes each (`u16` in this crate, `i16` in the references).

The influence count field can encode 4,294,967,295 entries. That is only a field-width
ceiling: storage, signed offsets, valid joint IDs, and the consuming mesh impose tighter
constraints. An influence entry identifies a joint; duplicate entries do not create joints.
The canonical writer limits total output to `i32::MAX` bytes so signed offsets remain
representable and rejects more than 32,768 joints. Source-preserving writes replay accepted
original layouts without rebuilding them.

For standard SKN skinning:

```text
vertex.blend_indices[k] -> skeleton.influences[slot] -> joint ID
vertex.blend_weights[k] -> weight for that joint, k = 0..3
```

Each blend index is `u8`, so only slots 0 through 255 are directly addressable. The mapping
can select joint IDs above 255: slot 0 can refer to joint 360. This does not limit the stored
influence table to 256. The local `janna_skin67.skl` fixture has 361 joints and 297 influence
entries. Its extra entries are retained; their presence alone does not establish a wider
SKN blend-index encoding.

With an empty influence table, Jade's viewer interprets blend indices as joint slots
directly. That is application behavior, not proof of an alternative League format rule.
A skeleton without a mesh needs no influences to be posed. Unweighted parents still affect
weighted descendants through hierarchy transforms.

### Legacy SKL v1/v2

Header: `r3d2sklt`, `version:u32`, `skeleton_id:u32`, `joint_count:u32` (20 bytes).
Each joint is 88 bytes: fixed 32-byte name, `parent:i32`, `radius:f32`, and twelve floats
representing a global affine bind matrix. Stored order is three rows of four values.

- V1 has no influence section; every joint is an implicit influence in joint order.
- V2 follows joints with `influence_count:u32` and that many `u32` joint IDs.
- On-disk counts are 32-bit, but this crate's shared `Joint` model uses signed 16-bit IDs.
  Legacy reads accept at most 32,768 joints and reject out-of-range influence IDs.
- Parents must precede children. Local transforms are `inverse(parent_global) * global`
  in the crate's column-vector convention; inverse binds are `inverse(global)`.

Original legacy bytes are retained. After edits, output is modern v0, so skeleton IDs,
matrix shear, legacy layout, and other legacy-only representation details are not re-encoded.

## Uncompressed animation

All variants begin with `r3d2anmd` and a 32-bit version.

### V3

The next words are skeleton ID, track count, frame count, and integer fps. Each track
contains a fixed 32-byte joint name and 4-byte flags, followed by `frame_count` records of
28 bytes: quaternion `f32x4` then translation `f32x3`. Scale is one. There are no shared
16-bit palette indices, so this layout has no v4/v5 unique-value ceiling.

### V4 and v5

After magic/version come resource size, format token, two format/flag words, track count,
frame count, and frame duration, followed by six signed 32-bit offsets. Data offsets are
relative to byte 12. The usual data start is byte 76, after twelve additional reserved bytes.

| Version | Sections | Bytes per frame per track |
|---|---|---:|
| V4 | Vectors (`f32x3`), rotations (`f32x4`), frame records | 12: hash `u32`, translation/scale/rotation indices `u16`, padding `u16` |
| V5 | Vectors (`f32x3`), rotations (6-byte quantized), hashes (`u32`), frame records | 6: translation/scale/rotation indices `u16` |

V5's declared track count controls row stride. If fewer hashes name tracks, remaining track
records must still be consumed on each frame. Palette counts come from section boundaries;
integer division permits alignment bytes between sections.

Quantized quaternions occupy 48 bits: two bits select the omitted largest component and
three 15-bit values encode the others in `[-1/sqrt(2), 1/sqrt(2)]`. The remaining component
is recovered from unit length. See `quantized::{compress_quat,decompress_quat}`.

Each palette has 65,536 addressable entries (indices 0..65,535). Translation and scale share
the vector palette. This limits distinct stored values, not frame count, duration, or joint
count. A long constant animation uses only a few palette entries.

Count fields have 32 bits: an unsigned interpretation can hold 4,294,967,295 frames or
tracks. The C# reference reads these counts as signed `i32`, limiting its positive range to
2,147,483,647; this crate and Rust LTK read uncompressed counts as `u32`. The unsigned
maximum is not a verified client-supported frame count.

This crate's canonical v4 output size is:

```text
76 + 12 * vector_entries + 16 * quaternion_entries + 12 * tracks * frames
```

It must fit `u32`. With at least one track, file size is tighter than the frame count field.
Given actual palette sizes `V` and `Q` and track count `T > 0`:

```text
frames <= floor((4,294,967,295 - 76 - 12*V - 16*Q) / (12*T))
```

This is a representable output bound, not a practical allocation target. Section starts must
also fit positive `i32`; the bounded v4 palettes keep this writer's starts well below that
limit. V5 uses six bytes per track/frame but still has signed section offsets. V3 has no
resource-size field or section-offset table.

Uniform sample labels are `frame_index / fps`; the last sample is `(frames - 1) / fps`.
`Animation::duration()` returns the last sample time, while the C# uncompressed resource
reports a period of `frames / fps`. Looping code should choose the intended meaning.
Float timestamps lose resolution at large values: `f32` cannot represent every consecutive
integer beyond 16,777,216, so field capacity does not imply distinct sample times throughout.

## Compressed animation v1/v2/v3

The 128-byte `r3d2canm` header contains resource size, format token, flags, three signed
32-bit counts (joints, sparse keys, jump caches), duration/fps floats, six error-metric floats,
translation/scale minima/maxima, and three signed offsets relative to byte 12.

Each sparse key occupies 10 bytes:

| Field | Meaning |
|---|---|
| `time:u16` | `time / 65535 * duration` seconds |
| `bits:u16`, low 14 bits | Joint/track index, 0..16,383 |
| `bits:u16`, high 2 bits | 0 rotation, 1 translation, 2 scale; 3 rejected |
| Six payload bytes | Quantized quaternion or three unsigned 16-bit vector components |

The signed key-count field can hold 2,147,483,647 records, but ten-byte records, resource
size and offsets impose lower storage limits. It counts sparse component keys across all
joints, not playback frames. A key's 16-bit time gives 65,536 positions over the whole clip;
it is not a 65,535-frame or 65,535-second ceiling.

Jump caches store four key indices for each of three channels per joint. Up to 65,536 sparse
keys, each cache/joint record is 24 bytes (`12 * u16`); above that it is 48 bytes (`12 * u32`).
Out-of-range cache indices can indicate absent channels and must not advance the shared
stream cursor. This threshold is separate from palette capacity.

The decoder uses four-key interpolation, with time parametrization controlled by flag bit 2.
It bakes `round(duration * fps) + 1` samples per track, includes the endpoint, and clamps
evaluation time to duration. It requires finite nonnegative duration, finite positive fps,
and a baked sample count fitting `u32`; allocation may still fail. Dense baking can consume
much more memory than the source. Later sampling of baked frames uses linear vectors and
spherical quaternions, so arbitrary-time results can differ from the original spline between
baked samples.

## Reference review

Reviewed on 2026-09-28:

- [C# RigResource](https://github.com/LeagueToolkit/LeagueToolkit/blob/76bf57bf4fd195343dec610bf0f912a06b910914/src/LeagueToolkit/Core/Animation/RigResource.cs): modern and legacy skeleton fields, transforms, influence mapping.
- [C# UncompressedAnimationAsset](https://github.com/LeagueToolkit/LeagueToolkit/blob/76bf57bf4fd195343dec610bf0f912a06b910914/src/LeagueToolkit/Core/Animation/UncompressedAnimationAsset.cs) and [CompressedAnimationAsset](https://github.com/LeagueToolkit/LeagueToolkit/blob/76bf57bf4fd195343dec610bf0f912a06b910914/src/LeagueToolkit/Core/Animation/CompressedAnimationAsset.cs): counts, palettes, timing and interpolation.
- [Rust ltk_anim](https://github.com/LeagueToolkit/league-toolkit/tree/2aad8168214910b7d1b693a0ddd90410642e0cb7/crates/ltk_anim/src): v5 unnamed tracks, modern rig fields, legacy matrix layout, compressed caches. Its rig reader currently accepts modern rigs only, despite defining a legacy-joint reader.

Jade's `replace-backend-parsers-with-ritoshark` branch was pulled and reviewed.
Its `src-tauri/src/core/mesh/{anm,skl}.rs` delegates to `ritoshark::anim`;
`src/lib/babylon/meshBuilder.ts` remaps influences and uses direct indices for absent slots.
Jade currently pins revision `24fe6b3665edbf5b67977918571c8fa6769c74c2`.
This code review does not update that pin or validate client playback.
