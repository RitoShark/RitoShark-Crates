# rs_anim

Reads and writes League skeletons (`.skl`) and animations (`.anm`), samples tracks, and
computes poses. See [format layouts and limits](docs/formats-and-limits.md) for frame
counts, joint counts, influence addressing, and binary field sizes.

## Supported formats

| Container | Versions | Decode | Unchanged write | Edited/new write |
|---|---|---|---|---|
| SKL `0x22FD4FC3` | 0 | Yes | Original bytes | Modern v0 |
| SKL `r3d2sklt` | 1, 2 | Yes | Original bytes | Modern v0 |
| ANM `r3d2anmd` | 3, 4, 5 | Yes | Original bytes | V4 default; explicit v5 |
| ANM `r3d2canm` | 1, 2, 3 | Yes, baked curves | Original bytes | Uncompressed v4 by default; compressed v1 via explicit API |

Unknown versions return `UnsupportedVersion`. These are the versions implemented by the
reviewed references, not a promise of undocumented future variants. There is no encoder
for uncompressed v3 ANM or legacy SKL.

## API

```rust,no_run
use rs_anim::{Animation, Skeleton, Pose};
use rs_io::{Parse, Serialize};

let skeleton = Skeleton::from_path("champion.skl")?;
let mut animation = Animation::from_path("idle.anm")?;
let pose = Pose::sample(&skeleton, &animation, 0.5);
let matrices = pose.skinning_matrices(&skeleton);
let original = animation.to_bytes()?;

animation.make_editable();
animation.tracks[0].frames[0].translation.x += 1.0;
animation.to_path("edited.anm")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Parse` supplies `from_reader`, `from_bytes`, and `from_path`; `Serialize` supplies
`to_writer`, `to_bytes`, and `to_path`.

| Type | Main fields |
|---|---|
| `Animation` | `fps`, `tracks` |
| `AnimTrack` | `joint_hash`, `frames` |
| `AnimFrame` | `time` in seconds, `rotation`, `translation`, `scale` |
| `Skeleton` | `flags`, `name`, `asset`, `joints`, `influences` |
| `Joint` | name/hash, signed IDs, radius, local and inverse-bind transforms |

Construct skeletons with `Skeleton::new()` and assign their public fields. A private source
snapshot preserves legacy matrices, reserved fields, padding, and original bytes.
Skeleton serialization detects field edits automatically and rebuilds modern v0 when changed.
Skeleton equality compares decoded fields, excluding the source snapshot.

Animation serialization retains its explicit editing contract: call `make_editable()` before
changing tracks or fps. Otherwise writing replays the original buffer even after public fields
change. `is_byte_exact()` reports whether that buffer remains attached.

V4 output shares one vector palette between translation and scale and a separate quaternion
palette. Each supports 65,536 distinct entries; overflow returns an error. Short tracks hold
their last frame, empty tracks use identity, and frame times are rebuilt from frame index/fps.
The writer does not preserve irregular timestamps. Rotations remain full `f32x4`.

## Format selection with fallback

`Animation::export_bytes` and `export_to_path` use the adaptive frame-input writer
and return the actual format, bytes, measured errors and fallback reasons. Selection
is explicit: CANM tries CANM -> v5 -> v4; v5 tries v5 -> v4; v4 tries only v4. Each
candidate must meet the requested quality. Invalid input is an error, not a fallback.

See [the export API guide](docs/animation-export.md) for Rust/Python usage and
[the CANM authoring findings](docs/canm-authoring-findings.md) for the experiments,
known precision limits and remaining size outliers. Existing strict methods below
retain their behavior and defaults.

## Dense v5 export

`Animation::to_v5_bytes()`, `write_v5(writer)` and `to_v5_path(path)` explicitly encode
ANMD v5. Every frame is retained; translation and scale remain exact float32 values and
normalized rotations use 48-bit packing. Each palette supports 65,536 entries. Duplicate
packed rotations share entries, including equivalent quaternion signs. Input must be
finite, uniformly sampled from time zero, with equally sized nonempty tracks and unique
hashes. Invalid data or palette overflow is an error before the destination is touched.
These methods use current tracks without discarding retained source bytes or changing the
default v4 writer. Python exposes `Anm.to_v5_bytes()` and `Anm.to_v5_path(path)`.

## Compressed export

```rust,no_run
use rs_anim::{Animation, CompressionOptions};
use rs_io::Parse;

let animation = Animation::from_path("idle.anm")?;
let options = CompressionOptions::default();
animation.to_compressed_path("idle-compressed.anm", &options)?;
let bytes = animation.to_compressed_bytes(&options)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

These methods encode the current tracks as `r3d2canm` v1 without changing the animation
or its default save behaviour. `write_compressed` also accepts an `io::Write` destination.
Python exposes the same feature through `Anm.to_compressed_bytes()` and
`Anm.to_compressed_path()`; see the [Python API](../rs_python/README.md#explicit-compressed-export).

The encoder quantizes rotations to 48 bits and vector components to 16 bits, then reduces
keys using keyframe-time parametrized cubic playback interpolation (flags 7). It writes one initial seek cache, with 16-bit
or 32-bit key indices as needed. Random seeks replay from that cache. The completed file
is decoded and compared against a grid of four intervals per source frame before output.
The reference uses linear vectors and shortest-path spherical rotation between source samples;
it cannot recover an original sparse source curve already lost during decoding.

`CompressionOptions` sets maximum errors on that grid, including quantization:

| Field | Default | Measurement |
|---|---|---|
| `rotation_error` | `0.001` | Angular radians, about 0.057 degrees |
| `translation_error` | `0.05` | Largest component difference, in the caller's animation units |
| `scale_error` | `0.001` | Largest component difference in scale factors |
| `sample_subdivisions` | `4` | Validation intervals per source frame; allowed 1 to 64 |
| `model_space_error` | `0.1` | Euclidean joint-origin/unit-axis probe error when a skeleton is supplied |

With a skeleton, use `to_compressed_bytes_with_skeleton(&options, &skeleton)`,
`write_compressed_with_skeleton(writer, &options, &skeleton)` or
`to_compressed_path_with_skeleton(path, &options, &skeleton)`. The encoder composes the
hierarchy, checks joint origins and unit-axis probes, and tightens key reduction if necessary.
Unanimated joints use their rest transforms; unmatched animation tracks are retained. Empty,
cyclic, invalid or wholly unmatched skeletons are rejected. Model-space checks bound these
probes, not the complete skinned mesh surface. `model_space_error` is ignored without a skeleton.

These bounds do not cover the continuous curve between validation points. All tolerances must
be finite and nonnegative. Translation and scale each use clip-wide bounds, so unusually
large ranges can make tight tolerances impossible even with every key retained. Such cases
return `CompressionTolerance` with the joint hash, channel, sample and measured error;
callers can explicitly choose a larger tolerance or use v5/v4. No automatic fallback occurs.

Tracks must have unique hashes and the same nonzero sample count, with timestamps at
`index / fps` starting at zero. Resample irregular tracks first. The encoder supports
1–16,384 tracks and at most 65,536 validation points per track, and rejects colliding
16-bit sample times. Lower `sample_subdivisions` explicitly for long clips. Transforms
must be finite and fps positive and finite. Nonzero quaternions are normalized.
Encoding finishes before a destination file is opened, so an encoding error preserves an
existing file. Output is deterministic for the same input and options.

## Decoding and posing

- Legacy SKL v1 supplies an identity influence table; v2 reads explicit influence IDs.
  Global bind matrices become parent-local and inverse-bind transforms. Parents must precede
  children. Singular transforms and invalid influence IDs return errors. Matrix-to-TRS
  conversion cannot represent arbitrary shear exactly; original bytes survive unchanged writes.
- ANM v3 hashes names with lowercase ELF. V4 embeds hashes in records; v5 keeps a separate
  hash table and quantized rotations. V5 consumes unnamed tracks too, keeping rows aligned.
- Compressed ANM uses four-key Catmull-Rom interpolation, optional time parametrization,
  shortest-path quaternion alignment, and jump caches. It bakes at the declared fps.
  Missing channels retain defaults. Baking loses the original sparse curve structure;
  retained bytes preserve the source file.
- `AnimTrack::sample` interpolates baked frames. `Pose::sample` matches tracks by hash;
  joints absent from the animation keep their bind pose.

## Tests

```text
cargo test -p rs_anim
cargo clippy -p rs_anim --all-targets -- -D warnings
```

Synthetic tests cover every supported version, legacy transforms, unnamed v5 tracks,
65,536-entry palettes, 70,000-frame clips, 32,768-joint skeletons, and malformed counts.
Compressed writer tests cover motion accuracy, key reduction, constant and single-frame
clips, quaternion signs, random seeks, wide cache indices, subframe interpolation, accumulated
hierarchy errors, invalid inputs and protected saves. V5 tests cover exact vector preservation,
independent binary decoding, quaternion packing and palette boundaries.
Python tests also evaluate the output with an independent channel-based decoder.
Local game fixtures are gitignored under `../../Sample-Files/`; missing fixtures are skipped.
See the [fixture report](docs/real-files-report.md) for coverage and its limits.

## References

Layouts were checked against [C# LeagueToolkit](https://github.com/LeagueToolkit/LeagueToolkit)
and [Rust ltk_anim](https://github.com/LeagueToolkit/league-toolkit/tree/main/crates/ltk_anim).
Exact revisions and source files appear in the format document. See also `NOTICE`.
