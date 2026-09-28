# rs_anim

Reads and writes League skeletons (`.skl`) and animations (`.anm`), samples tracks, and
computes poses. See [format layouts and limits](docs/formats-and-limits.md) for frame
counts, joint counts, influence addressing, and binary field sizes.

## Supported formats

| Container | Versions | Decode | Unchanged write | Edited/new write |
|---|---|---|---|---|
| SKL `0x22FD4FC3` | 0 | Yes | Original bytes | Modern v0 |
| SKL `r3d2sklt` | 1, 2 | Yes | Original bytes | Modern v0 |
| ANM `r3d2anmd` | 3, 4, 5 | Yes | Original bytes | Uncompressed v4 |
| ANM `r3d2canm` | 1, 2, 3 | Yes, baked curves | Original bytes | Uncompressed v4 |

Unknown versions return `UnsupportedVersion`. These are the versions implemented by the
reviewed references, not a promise of undocumented future variants. There is no encoder
for newly authored compressed ANM, v3/v5 ANM, or legacy SKL.

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
Local game fixtures are gitignored under `../../sample-files/`; missing fixtures are skipped.
See the [fixture report](docs/real-files-report.md) for coverage and its limits.

## References

Layouts were checked against [C# LeagueToolkit](https://github.com/LeagueToolkit/LeagueToolkit)
and [Rust ltk_anim](https://github.com/LeagueToolkit/league-toolkit/tree/main/crates/ltk_anim).
Exact revisions and source files appear in the format document. See also `NOTICE`.
