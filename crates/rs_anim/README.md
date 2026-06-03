# rs_anim

Reads and writes League of Legends skeleton (`.skl`) and animation (`.anm`) files.

## Public API

Every type follows the workspace-wide `Parse` / `Serialize` shape (see the root `CLAUDE.md`):

```rust
use rs_anim::{Animation, Skeleton, AnimTrack, AnimFrame, Joint};

let skl  = Skeleton::from_path("aatrox.skl")?;
let anim = Animation::from_path("aatrox_idle.anm")?;

let bytes = anim.to_bytes()?;     // v5 in -> byte-exact v5 out; otherwise emits v4
skl.to_path("out.skl")?;
```

Constructors: `from_reader`, `from_bytes`, `from_path`. Serializers: `to_writer`, `to_bytes`,
`to_path`. Accessors use bare field names (`tracks()`, `joints()`, `influences()`).

| Type | Fields of note |
|---|---|
| `Animation` | `fps: f32`, `tracks: Vec<AnimTrack>` |
| `AnimTrack` | `joint_hash: u32`, `frames: Vec<AnimFrame>` |
| `AnimFrame` | `time`, `rotation: Quat`, `translation: Vec3`, `scale: Vec3` |
| `Skeleton` | `flags`, `name`, `asset`, `joints: Vec<Joint>`, `influences: Vec<u16>` |
| `Joint` | name, ids, parent, radius, hash, local + inverse-bind transforms |

`quantized` is a public module exposing the bit-level codecs (`decompress_quat`/`compress_quat`,
`decompress_vec3`) for callers that need them directly.

## Skeleton (`.skl`) format

Two on-disk shapes exist; this crate implements the **modern** one and rejects the legacy one.

- **Modern** — magic `0x22FD4FC3` at byte offset 4 (offset 0 holds the file size), version `0`.
  Header carries `flags`, joint count, influence count, and section offsets (joints, joint
  indices, influences, skeleton name, asset name, bone names, plus reserved slots). Each joint
  record is 100 bytes: flags/id/parent, a 4-byte hash, a radius, a local
  (translation, scale, rotation) transform and an inverse-bind transform, then a relative offset
  to its NUL-terminated name. Influences are a flat `u16` list of joint ids (the C# oracle reads
  these as signed `i16`; the bytes are identical). The joint-id-hash section is emitted ordered by
  hash **ascending**, matching the C# `RigResource` writer. Round-trips byte-exactly.
- **Legacy** — magic `r3d2sklt` (versions 1/2). Returned as `Error::UnsupportedVersion`.

> No `.skl` sample ships in `sample-files/`, so the skeleton path is currently exercised only by
> the synthetic round-trip test, not by a real game asset. **This is a known coverage gap** — drop
> a real `.skl` next to the `.anm` samples to close it.

## Animation (`.anm`) format

The 8-byte magic selects the container; a `u32` version follows.

### Uncompressed — `r3d2anmd` (versions 3, 4, 5)

A header records track count, frame count, and frame duration (fps = 1 / duration), plus byte
offsets (relative to byte 12) into shared data sections.

- **v5** — sections `vecs -> quats -> joint_hashes -> frames`. Vectors are raw `Vec3`; quaternions
  are **48-bit quantized** (see below). Each frame row is three `u16` palette indices
  (translation, scale, rotation) per track.
- **v4** — sections `vecs -> quats -> frames`; quaternions are full `f32x4`. Frame rows embed the
  joint hash per track plus the three indices and a padding `u16`.
- **v3 (legacy)** — per-track fixed 32-byte name (hashed with ELF), then a full
  rotation+translation per frame; scale is implicitly `(1,1,1)`.

**Writing is format-preserving for v5.** When a file is read from uncompressed v5, the reader keeps
the raw sections (vector palette, the 48-bit quantized quaternion palette *as bytes*, joint hashes,
and the per-frame palette-index triples) plus the header fields and the exact physical section
order (`vecs -> quats -> jointHashes -> frames`, with the 12-byte post-header pad). The writer
replays them, so `read -> write` is **byte-identical** for v5 (verified on the three real samples).

If you mutate `tracks` on a v5-sourced animation, call `Animation::make_editable()` first to drop
the preserved layout; the writer then rebuilds from the decoded tracks and emits **v4** (full
quaternions, no quantization loss). Animations constructed in memory, or read from v3/v4/compressed,
have no preserved layout and always write as v4. `Animation::is_byte_exact()` reports whether the
preserved v5 layout is present.

### Compressed — `r3d2canm` (versions 1, 2, 3)  ✅ supported

Real League animations are frequently compressed. The header carries joint/frame/jump-cache
counts, a max-time and fps, three error metrics, and per-component `min`/`max` bounds for
translation and scale, followed by offsets to the frame stream, jump caches, and joint hashes.

The frame stream is a flat list of **sparse** keyframes. Each 10-byte record is:

```
u16 compressed_time   // time = compressed_time / 65535 * max_time * fps   (in frames)
u16 bits              // low 14 bits = joint id; high 2 bits = transform type
u8[6] value           // quantized payload
```

Transform type `0` = rotation (48-bit quantized quaternion), `1` = translation, `2` = scale (both
48-bit quantized `Vec3` against the header `min`/`max`). Because each component is keyed
independently and sparsely, the reader collects per-joint keys and **resamples** every integer
output frame by linear-interpolating translation/scale and spherically interpolating rotation
between the surrounding keys, producing the same explicit `AnimFrame` layout as the uncompressed
path. The jump-cache table (a seek-acceleration structure for streaming playback) is not needed
for full decode and is skipped.

#### 48-bit quantized quaternion

Two bits pick the dropped largest-magnitude component; three 15-bit fields hold the rest mapped to
`[-1/√2, 1/√2]`; the dropped component is rebuilt as `sqrt(1 - a² - b² - c²)`. See
`quantized::decompress_quat` / `compress_quat`.

## Supported / unsupported matrix

| Container | Versions | Read | Write |
|---|---|---|---|
| `.skl` modern `0x22FD4FC3` | 0 | yes (byte-exact) | yes (byte-exact) |
| `.skl` legacy `r3d2sklt` | 1, 2 | `UnsupportedVersion` | no |
| `.anm` `r3d2anmd` v5 | 5 | yes | **byte-exact** (v4 after `make_editable`) |
| `.anm` `r3d2anmd` v3/v4 | 3, 4 | yes | as v4 |
| `.anm` `r3d2canm` | 1, 2, 3 | yes (resampled) | no (re-emit as v4) |
| `.anm` `r3d2canm` | other | `UnsupportedVersion` | — |

## Tests & fixtures

Real game assets are gitignored. Drop samples into `../../sample-files/` (workspace
`sample-files/`); `tests/real_files.rs` skips cleanly when the directory is absent.

```
cargo test -p rs_anim
```

## Attribution

The binary layouts and quantization constants were cross-checked against the C# LeagueToolkit
(behavioral oracle), the Rust `ltk_anim` crate, and `pyritofile`. See `NOTICE`.
