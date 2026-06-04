# rs_mapgeo

Reads and writes League of Legends `.mapgeo` (**OEGM**) environment geometry.

This crate supports **OEGM versions 5, 6, 7, 9, 11, 12, 13, 14, 15, 17 and 18** — the full version
matrix of the C# `LeagueToolkit` oracle. Versions 8, 10 and 16 are not defined by the oracle and are
reported as `Error::UnsupportedVersion(version)` rather than mis-parsed. The reader covers the
**entire** file — including the trailing bucketed scene graphs and planar reflectors — and the
writer is its byte-exact inverse, so a read → write round-trip of a supported file is byte-identical
over the whole file (all three real samples round-trip in full; the remaining versions are validated
synthetically).

## Format overview (OEGM)

All integers are little-endian. Strings are a `u32` length prefix followed by that many UTF-8
bytes (no NUL terminator).

```
magic            : "OEGM" (4 bytes)
version          : u32                       // accepts 5,6,7,9,11,12,13,14,15,17,18
separate_lights  : u8 bool                   // present only if version < 7

shader texture overrides
  count          : u32
  [ index: u32, name: string ] * count

vertex declarations
  count          : u32
  per declaration:
    usage        : u32                       // 0 Static, 1 Dynamic, 2 Stream
    element_count: u32                       // 0..=15
    [ name: u32, format: u32 ] * element_count
    padding      : the remaining (15 - element_count) slots, each a default
                   element (name = Position = 0, format = XYZW_Float32 = 3)

vertex buffers
  count          : u32
  per buffer:    layer: u8, size: u32, raw bytes[size]

index buffers
  count          : u32
  per buffer:    layer: u8, size: u32, u16 indices[size / 2]

models (meshes)
  count          : u32
  per model:
    vertex_count          : u32
    vertex_buffer_count   : u32
    vertex_description_id  : u32
    vertex_buffer_ids      : i32 * vertex_buffer_count
    index_count            : u32
    index_buffer_id        : i32
    layer                  : u8              // visibility flags
    visibility_controller  : u32             // scene-graph path hash
    submeshes
      count                : u32
      [ hash: u32, name: string, index_start: u32,
        index_count: u32, min_vertex: u32, max_vertex: u32 ] * count
    disable_backface_culling : bool (u8)
    bounds                 : Vec3 min, Vec3 max
    transform              : 16 × f32 (4×4 matrix, row-major on disk)
    quality                : u8              // environment-quality filter bitmask
    layer_transition       : u8              // visibility-transition behavior (0/1/2)
    render_flags           : u16
    baked_light            : channel         // string path, Vec2 scale, Vec2 bias
    stationary_light       : channel
    texture_overrides
      count                : u32
      [ index: u32, path: string ] * count
    baked_paint_scale_offset : 4 × f32       // scale (Vec2) then bias (Vec2)
```

### Trailing sections

After the model list the reader continues through the rest of the file:

- the **bucketed scene graphs** (`version >= 15`: a count then one quad-tree per entry; earlier
  versions: a single implicit graph). Each carries its bounds, bucket grid, vertex/index arrays,
  per-bucket records and optional per-face visibility flags. Version 18 prefixes one extra `f32`.
- the **planar reflectors** (`version >= 13`): a counted list of `(transform, plane, normal)`.

Both are fully parsed and re-emitted, so `tests/real_files.rs` asserts the re-serialized output
equals the **entire** source file byte-for-byte.

### Per-version layout deltas

Every gate below mirrors the C# oracle (`EnvironmentAsset` / `EnvironmentAssetMesh` /
`BucketedGeometry`). All deltas are version-gated inside the single reader/writer pair — there is no
parallel reader per version.

| Field | applies when |
|---|---|
| leading `separate_point_lights` byte | version < 7 |
| shader overrides: implicit bare strings (v9 sampler, v11 second) | 9 ≤ version < 17 |
| shader overrides: counted `[index, name]` list | version ≥ 17 |
| per-buffer visibility layer byte (vertex + index buffers) | version ≥ 13 |
| embedded per-mesh name (sized string) | version < 12 |
| mesh layer byte (before submeshes) | version ≥ 13 |
| mesh layer byte (after transform) | 7 ≤ version ≤ 12 |
| mesh `unknown_v18` `u32` | version ≥ 18 |
| mesh visibility-controller hash `u32` | version ≥ 15 |
| backface-culling byte | version ≠ 5 |
| mesh render flags: bare `u8`, no transition byte | 11 ≤ version ≤ 13 |
| mesh render flags: transition byte + `u8` | 14 ≤ version ≤ 15 |
| mesh render flags: transition byte + `u16` | version ≥ 16 |
| per-mesh point light (`Vec3`) | version < 7 and `separate_point_lights` |
| nine spherical-harmonics coefficients (then baked-light only) | version < 9 |
| stationary-light channel | version ≥ 9 |
| single baked-paint channel | 12 ≤ version < 17 |
| counted texture overrides + scale/offset | version ≥ 17 |
| scene graphs: single implicit graph | version < 15 |
| scene graphs: counted list, each with visibility hash | version ≥ 15 |
| scene graph leading `f32` | version ≥ 18 |
| planar reflectors (counted list) | version ≥ 13 |

## Versioning

Versions `5, 6, 7, 9, 11, 12, 13, 14, 15, 17, 18` are accepted — the complete matrix listed by the
C# `LeagueToolkit` oracle. Only `14`, `17` and `18` appear among the real samples (see
`docs/real-files-report.md`); the rest are validated synthetically in `tests/synthetic_versions.rs`
by building a minimal file per version straight from the oracle's layout and asserting a byte-exact
round-trip. Versions `8`, `10` and `16` are absent from the oracle and report `UnsupportedVersion`.

## API

The crate follows the workspace's universal shape. `MapGeometry` implements `rs_io::Parse` and
`rs_io::Serialize`, giving these methods:

```rust
use rs_io::{Parse, Serialize};
use rs_mapgeo::MapGeometry;

let geo = MapGeometry::from_path("Map12.mapgeo")?;   // mmaps the file
let geo = MapGeometry::from_bytes(&bytes)?;           // from a slice

let bytes = geo.to_bytes()?;                          // serialize
geo.to_path("out.mapgeo")?;
# Ok::<(), rs_mapgeo::Error>(())
```

Public types: `MapGeometry`, `MapModel`, `Submesh`, `VertexDescription`, `VertexElement`,
`VertexBuffer`, `IndexBuffer`, `TextureOverride`, `AssetChannel`, `SceneGraph`, `GeometryBucket`,
`PlanarReflector`, and the `ElementName`, `ElementFormat`, `VertexUsage` enums. Vertex buffers are
kept as raw bytes plus their declaration so callers can decode them with
`VertexDescription::vertex_size` and the per-element byte sizes.

## Fixtures

Real `.mapgeo` files are copyrighted game assets and are **gitignored**. Sample files live in the
workspace-level `Sample-Files/` directory (a sibling of `crates/`). The real-file tests join
`../../Sample-Files` relative to the crate and **skip cleanly** when the files are absent, so the
suite is green on a fresh checkout.

## Testing

- `tests/smoke.rs` — a hand-built minimal v17 file: parse, reject bad magic / unsupported
  version (16), byte-exact round-trip, and a truncation test (must `Err`, never panic).
- `tests/synthetic_versions.rs` — a minimal file built per version (5/6/7/9/11/12/13/15) from the
  oracle's layout, each asserting a byte-exact `read → to_bytes → ==`, plus a check that the
  undocumented versions 8/10/16 report `UnsupportedVersion`.
- `tests/real_files.rs` — for each sample: print magic + version, then parse; supported versions
  must yield a non-empty model list and round-trip the **whole** file byte-for-byte, other
  versions must report `UnsupportedVersion` carrying the exact on-disk version.

```bash
cargo test -p rs_mapgeo
```
