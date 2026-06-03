# rs_mesh

Readers and writers for the League of Legends mesh formats:

- **`.skn`** — skinned mesh (`r3d2 SKN`, magic `0x00112233`): a shared vertex buffer plus a
  `u16` index buffer carved into per-material ranges.
- **`.scb`** — binary static mesh (`r3d2Mesh`): positions, optional per-vertex colors, and
  per-face triangles carrying their own material name and UVs.
- **`.sco`** — text static mesh (`[ObjectBegin]`): the ASCII sibling of `.scb`.

## Public API

All types follow the workspace conventions (`Parse` / `Serialize` from `rs_io`):

```rust
use rs_io::{Parse, Serialize};
use rs_mesh::{SkinnedMesh, StaticMesh};

let skn = SkinnedMesh::from_path("body.skn")?;
let bytes = skn.to_bytes()?;            // byte-exact for v1/v2/v4

let scb = StaticMesh::from_path("mesh.scb")?;   // dispatches scb vs sco by magic
let n_verts = scb.positions().len();
```

`StaticMesh` exposes `name()`, `positions()`, `faces()`, `colors()`; each
`StaticMeshFace` carries `material`, `indices: [u32; 3]`, and `uvs: [Vec2; 3]`.
`SkinnedMesh` exposes `ranges()`, `indices()`, `vertices()`, `version()`.

`StaticMesh::from_scb_reader` and `StaticMesh::from_sco_str` are available when the format is
known up front; `from_reader` (and therefore `from_bytes` / `from_path`) sniffs the magic and
routes to the right one.

## Supported versions

| Format | Versions read | Versions written | Round-trip |
|---|---|---|---|
| `.skn` | major 0, 1, 2, 4 (minor 1) | same | byte-exact (tested for v1/v2/v4) |
| `.scb` | 2.1, 3.1, 3.2 | — (no writer) | — |
| `.sco` | `[ObjectBegin]` | — (no writer) | — |

### `.scb` (`r3d2Mesh`) layout

```
"r3d2Mesh"            8 bytes magic
major u16, minor u16
name                  128-byte NUL-padded string
vertexCount u32
faceCount   u32
flags       u32       bit0 = HasVcp, bit1 = HasLocalOriginLocatorAndPivot
boundingBox           min Vec3, max Vec3
[vertexType u32]      only when version >= 3.2; >=1 means per-vertex colors follow
positions             vertexCount * Vec3
[colors]              vertexCount * [u8; 4]  (present only when vertexType >= 1)
central               Vec3
faces                 faceCount * { indices [u32;3], material[64], u u u, v v v }
```

Face UVs are stored as three U floats followed by three V floats (not interleaved); the reader
pairs them back into `[(u0,v0),(u1,v1),(u2,v2)]`.

## Limitations / coverage gaps

- **No static-mesh writer.** `.scb` and `.sco` are read-only; there is no byte round-trip for
  the static formats yet (only the skinned format round-trips).
- **No `.skn` sample files** ship in `sample-files/`, so the skinned reader/writer is exercised
  only by synthetic fixtures, not real game assets.
- **`HasVcp` face colors and any trailing post-face data are not parsed.** When `flags` has bit 0
  set, real files carry extra bytes after the face list; the reader stops at the face list and
  ignores them. Counts and indices remain correct, but that data is not surfaced.

## Test fixtures

Real game files are gitignored. Drop `.scb` / `.sco` / `.skn` samples into the workspace
`sample-files/` directory; `tests/real_files.rs` discovers them there and skips cleanly when the
directory is absent.
