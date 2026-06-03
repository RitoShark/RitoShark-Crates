# rs_mapgeo

Reads and writes League of Legends `.mapgeo` (**OEGM**) environment geometry.

This crate supports **OEGM versions 14, 17 and 18**. Any other on-disk version is reported as
`Error::UnsupportedVersion(version)` rather than mis-parsed. The reader covers the **entire** file —
including the trailing bucketed scene graphs and planar reflectors — and the writer is its
byte-exact inverse, so a read → write round-trip of a supported file is byte-identical over the
whole file (all three real samples round-trip in full).

## Format overview (OEGM)

All integers are little-endian. Strings are a `u32` length prefix followed by that many UTF-8
bytes (no NUL terminator).

```
magic            : "OEGM" (4 bytes)
version          : u32                       // this crate accepts 14, 17, 18

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

| Field | v14 | v17 | v18 |
|---|---|---|---|
| shader overrides | implicit bare strings (v9/v11 samplers) | counted `[index, name]` | counted `[index, name]` |
| mesh extra `u32` after layer | — | — | `unknown_v18` |
| mesh visibility-controller hash | — | `u32` | `u32` |
| mesh render flags | `u8` | `u16` | `u16` |
| mesh paint data | single baked-paint channel | counted overrides + scale/bias | counted overrides + scale/bias |
| scene graph list | single implicit graph | counted | counted, each with leading `f32` |

## Versioning

Versions `14`, `17` and `18` are accepted; all three appear in the wild (see
`docs/real-files-report.md`). The C# oracle additionally lists `5, 6, 7, 9, 11, 12, 13, 15`, which
this crate does not yet exercise against real samples and reports as `UnsupportedVersion`.

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
  version, byte-exact round-trip, and a truncation test (must `Err`, never panic).
- `tests/real_files.rs` — for each sample: print magic + version, then parse; supported versions
  (14/17/18) must yield a non-empty model list and round-trip the **whole** file byte-for-byte,
  other versions must report `UnsupportedVersion` carrying the exact on-disk version.

```bash
cargo test -p rs_mapgeo
```
