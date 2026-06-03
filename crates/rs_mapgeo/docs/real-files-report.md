# rs_mapgeo — real-file report

Results of running `rs_mapgeo` against the real `.mapgeo` samples in the workspace
`sample-files/` directory. The crate parses **OEGM version 17 only**; everything else is reported
as `Error::UnsupportedVersion`.

## Per-file results

| File | Magic | OEGM version | Parsed? | Result |
|---|---|---|---|---|
| `bloom.mapgeo` | OEGM | **17** | yes | 1551 models, 1722 vertex buffers, 676 index buffers; byte-exact prefix round-trip over 38,302,200 of 42,881,463 bytes |
| `spectator_only_banners.mapgeo` | OEGM | **18** | no | `UnsupportedVersion(18)` |
| `ultbook.mapgeo` | OEGM | **14** | no | `UnsupportedVersion(14)` |

The version byte is the `u32` directly after the 4-byte `OEGM` magic. Only `bloom.mapgeo` is the
v17 format this crate targets; the other two are different on-disk versions and are rejected
cleanly (no panic, the error carries the exact version).

The ~4.5 MB tail of `bloom.mapgeo` that is not round-tripped is the bucketed scene graph and
planar reflectors, which this MVP intentionally does not parse (see README *Scope*). The writer
reproduces everything up to and including the model list byte-for-byte.

## Versions we are missing (needed by the samples)

The samples alone require two more versions on top of v17:

- **v18** (`spectator_only_banners.mapgeo`)
- **v14** (`ultbook.mapgeo`)

The trusted C# oracle accepts the full set `5, 6, 7, 9, 11, 12, 13, 14, 15, 17, 18`. The two
versions the samples exercise are the priority.

## Cross-check vs the C# oracle (v17 path)

Field-by-field, our v17 reader/writer matches the oracle's v17 path exactly:

- **Header** — magic + `u32` version. ✔
- **Shader texture overrides** — `count: u32`, then `[index: i32, name: sized-string]`. ✔
- **Vertex declarations** — `usage: u32`, `element_count: u32`, `[name: u32, format: u32]`, then
  the unused tail up to 15 slots. ✔ (see *What I changed* — padding content was the first bug)
- **Vertex buffers** — `layer: u8` (present because v17 ≥ 13), `size: u32`, raw bytes. ✔
- **Index buffers** — `layer: u8`, `size: u32`, `u16` indices. ✔
- **Model** — vertex_count, vertex_buffer_count, vertex_description_id, buffer ids, index_count,
  index_buffer_id, `layer: u8` (≥ 13), `visibility_controller_hash: u32` (≥ 15), submeshes,
  `disable_backface_culling: bool`, bounds (Box), transform (row-major 4×4), `quality: u8`,
  `layer_transition: u8` (≥ 14), `render_flags: u16` (`u16` because v17 ≥ 16), baked-light and
  stationary-light channels, texture-override list (≥ 17), then baked-paint scale + bias Vec2s. ✔

Discrepancies found and what they meant:

1. **Vertex-declaration padding is not zero.** The 15-slot element table fills its unused tail
   with the default element `(Position, XYZW_Float32)` = `(0, 3)`, not zero bytes. Our writer was
   emitting zeros, so round-trip diverged at byte 108. The oracle's writer emits exactly this
   default element.
2. **`layer_transition` is a byte enum, not a bool.** The field after `quality` is the
   visibility-transition behavior (values 0/1/2 in the oracle). Our model stored it as a `bool`
   named `is_bush`, which collapsed the value `2` seen in `bloom.mapgeo` down to `1` on write,
   diverging at byte ~37.99 M. It is now a `u8`.

Both were within-crate model/serialization bugs, not layout misunderstandings; the byte positions
of every field already matched the oracle.

## How versions differ (for the next implementer)

From the C# oracle, the deltas that matter for the sampled versions:

- **v18 vs v17** — after the per-mesh `layer` byte (and before the visibility-controller hash)
  there is an extra `u32` (the oracle calls it `UnknownVersion18Int`). Everything else in the mesh
  body matches v17. The top-level layout (scene graphs ≥ 15, planar reflectors ≥ 13) is the same.
- **v14 vs v17** — shader texture overrides are **not** a counted list: v9 adds one implicit
  `BAKED_DIFFUSE_TEXTURE` sampler string and v11 adds `BAKED_DIFFUSE_TEXTURE_ALPHA`, both read as
  bare sized-strings (no index, no count). Mesh body differences: no `visibility_controller_hash`
  (that is ≥ 15); `render_flags` is a **`u8`** (the `u16` form starts at v16); the field after
  `quality` is still the transition byte at ≥ 14; per-mesh texture overrides use the older single
  baked-paint channel (≥ 12, < 17) rather than the counted override list (≥ 17), and there is no
  trailing baked-paint scale/bias pair. The scene graph for < 15 is a single graph, not a counted
  list.

## Improvements / TODO (priority order)

1. **Add OEGM v18** — the closest delta to v17 (one extra mesh `u32`). Unlocks
   `spectator_only_banners.mapgeo`. Gate the field on `version >= 18` in both reader and writer
   and add a v18 fixture round-trip.
2. **Add OEGM v14** — larger delta (implicit sampler strings, `u8` render flags, single baked-paint
   channel, no controller hash, single scene graph). Unlocks `ultbook.mapgeo`.
3. **Parse (and round-trip) the trailing scene graph + planar reflectors** so v17 files round-trip
   in full, not just up to the model list. This is the largest remaining correctness gap against
   the lossless-round-trip contract; today ~4.5 MB of `bloom.mapgeo` is dropped on write.

## What I changed

- Added `tests/real_files.rs`: skip-if-missing helper joining `../../sample-files`; prints magic +
  version for every sample, asserts v17 parses with a non-empty model list and round-trips its
  prefix byte-for-byte, and asserts other versions report `UnsupportedVersion(version)`.
- **Fixed the vertex-declaration writer** to emit the default `(Position, XYZW_Float32)` element
  for each unused slot instead of zero bytes (`src/write.rs`).
- **Changed the model's `is_bush: bool` field to `layer_transition: u8`** across the data type,
  reader, and writer so the visibility-transition byte round-trips losslessly
  (`src/mapgeo.rs`, `src/read.rs`, `src/write.rs`).
- Updated `tests/smoke.rs` so the hand-built minimal file uses the real default-element padding,
  keeping the byte-exact round-trip test valid.

After these changes `cargo test -p rs_mapgeo` is green (3 real-file tests + 5 smoke tests), and
`bloom.mapgeo` (v17) round-trips byte-for-byte over its entire parsed prefix.
