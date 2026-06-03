# rs_mesh

Readers and writers for the League of Legends mesh formats:

- **`.skn`** — skinned mesh (`r3d2 SKN`, magic `0x00112233`): a shared vertex buffer plus a
  `u16` index buffer carved into per-material ranges.
- **`.scb`** — binary static mesh (`r3d2Mesh`): positions, optional per-vertex colors, and
  per-face triangles carrying their own material name and UVs.
- **`.sco`** — text static mesh (`[ObjectBegin]`): read-only legacy sibling of `.scb`. **This
  format was removed from the game and is not a focus**; rs_mesh still parses it but does not
  write it.

## Public API

All types follow the workspace conventions (`Parse` / `Serialize` from `rs_io`):

```rust
use rs_io::{Parse, Serialize};
use rs_mesh::{SkinnedMesh, StaticMesh};

let skn = SkinnedMesh::from_path("body.skn")?;
let bytes = skn.to_bytes()?;            // byte-exact for v1/v2/v4 (incl. real v4.1 game files)

let scb = StaticMesh::from_path("mesh.scb")?;   // dispatches scb vs sco by magic
let out = scb.to_bytes()?;              // byte-exact for r3d2Mesh .scb
let n_verts = scb.positions().len();
```

`StaticMesh` exposes `name()`, `positions()`, `faces()`, `colors()`, `flags()`; each
`StaticMeshFace` carries `material`, `indices: [u32; 3]`, and `uvs: [Vec2; 3]`. It also keeps the
on-disk `version`, `bounding_box`, `vertex_type`, and a raw `trailing` blob (the post-face data)
so the binary form round-trips exactly. `SkinnedMesh` exposes `ranges()`, `indices()`,
`vertices()`, `version()`, plus a raw `trailing` blob for the end-tab.

`StaticMesh::from_scb_reader` and `StaticMesh::from_sco_str` are available when the format is
known up front; `from_reader` (and therefore `from_bytes` / `from_path`) sniffs the magic and
routes to the right one. `StaticMesh::to_scb_writer` (and the `Serialize` impl) writes the binary
`.scb`.

## Supported versions

| Format | Versions read | Versions written | Round-trip |
|---|---|---|---|
| `.skn` | major 0, 1, 2, 4 (minor 1) | same | byte-exact (v1/v2/v4 synthetic + 3 real v4.1 files) |
| `.scb` | 2.1, 3.1, 3.2 | same | byte-exact (synthetic + 4 real 3.2 files) |
| `.sco` | `[ObjectBegin]` | — (removed from game, not written) | — |

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
[trailing]            HasVcp RGB block (faceCount*9) + optional local-origin/pivot; kept raw
```

Face UVs are stored as three U floats followed by three V floats (not interleaved); the reader
pairs them back into `[(u0,v0),(u1,v1),(u2,v2)]`. Real major-4 `.skn` files end with a 12-byte
zero "end tab" after the vertex buffer; rs_mesh keeps it in `SkinnedMesh::trailing`.

## Limitations / coverage gaps

- **`HasVcp` / post-face data is preserved but not decoded.** When `flags` has bit 0 set (and/or
  bit 1 for local origin + pivot), real `.scb` files carry extra bytes after the face list. These
  are captured into `StaticMesh::trailing` and written back verbatim, so the round-trip is lossless,
  but the individual per-face colors and the origin/pivot vectors are not yet surfaced as fields.
- **`.sco` is read-only and de-prioritized.** It was removed from the game (it crashes the client),
  so there is no `.sco` writer and it is exercised only by a synthetic fixture.

## Test fixtures

Real game files are gitignored. Drop `.scb` / `.skn` samples into the workspace `Sample-Files/`
directory; `tests/real_files.rs` discovers them there, asserts versions/counts, checks that
submesh ranges and face indices stay in bounds, and verifies a byte-exact `read → to_bytes → ==`
round-trip on every sample. It skips cleanly when the directory is absent.
