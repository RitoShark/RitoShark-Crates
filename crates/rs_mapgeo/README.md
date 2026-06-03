# rs_mapgeo

Reads and writes League of Legends `.mapgeo` (**OEGM**) environment geometry.

This crate targets **OEGM version 17 only** — the current shipping format. Any other on-disk
version is reported as `Error::UnsupportedVersion(version)` rather than mis-parsed. The reader
covers the full top-level structure and the writer is its byte-exact inverse for everything it
parses, so a read → write round-trip of a v17 file is byte-identical up to where parsing stops
(see *Scope* below).

## Format overview (OEGM)

All integers are little-endian. Strings are a `u32` length prefix followed by that many UTF-8
bytes (no NUL terminator).

```
magic            : "OEGM" (4 bytes)
version          : u32                       // this crate accepts 17

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

### Scope (what is parsed vs skipped)

Parsing stops cleanly **after the model list**. The trailing sections are intentionally **not**
read or written by this MVP:

- the **bucketed scene graph** (`version >= 15`: a count then one quad-tree per entry; earlier
  versions: a single graph),
- the **planar reflectors** (`version >= 13`),
- the per-mesh **light grid** that older versions embed.

Because of this, a full-file byte round-trip reproduces the original exactly **up to that point**;
the bytes after the model list are dropped. The crate's `tests/real_files.rs` asserts the
re-serialized output equals the source file's prefix byte-for-byte.

## Versioning

Only `version == 17` is accepted. Every sampled real file confirms the on-disk version varies by
map and patch (v14, v17 and v18 all appear in the wild — see `docs/real-files-report.md`), so
callers should expect `UnsupportedVersion` for many inputs until more versions are implemented.

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
`VertexBuffer`, `IndexBuffer`, `TextureOverride`, `AssetChannel`, and the `ElementName`,
`ElementFormat`, `VertexUsage` enums. Vertex buffers are kept as raw bytes plus their declaration
so callers can decode them with `VertexDescription::vertex_size` and the per-element byte sizes.

## Fixtures

Real `.mapgeo` files are copyrighted game assets and are **gitignored**. Drop sample files in the
workspace-level `sample-files/` directory (a sibling of `crates/`). The real-file tests join
`../../sample-files` relative to the crate and **skip cleanly** when the files are absent, so the
suite is green on a fresh checkout.

## Testing

- `tests/smoke.rs` — a hand-built minimal v17 file: parse, reject bad magic / unsupported
  version, byte-exact round-trip, and a truncation test (must `Err`, never panic).
- `tests/real_files.rs` — for each sample: print magic + version, then parse; v17 must yield a
  non-empty model list and round-trip its prefix byte-for-byte, other versions must report
  `UnsupportedVersion` carrying the exact on-disk version.

```bash
cargo test -p rs_mapgeo
```
