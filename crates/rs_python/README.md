# rs_python

Python bindings for RitoShark, published as the `ritoshark` module (pyo3 + maturin). Each
format wraps its `rs_*` type in a `#[pyclass]` and exposes bulk geometry as tightly packed
little-endian buffers, so a Blender or Maya plugin moves vertex data with one memcpy instead of
a per-vertex Python loop.

## Public surface

```
AnimFrame, AnimTrack, Anm, FormatError, Joint, MapGeo, MapModel, MapSubmesh, ParseError,
Scb, ScbFace, Sco, Skl, Skn, Submesh, Tex, UnsupportedVersion, Wad, WadChunk, WriteError,
read_bin, read_bin_bytes, wad_hash
```

plus `__version__`. `ParseError`, `UnsupportedVersion`, and `WriteError` all derive from
`FormatError`, so callers can catch either the specific exception or the base class.

## Installation

Two channels, because a plugin's host interpreter is not always the system one:

1. **`pip install ritoshark`** — for standalone scripting against a normal Python install.
2. **Vendoring** — Blender and Maya both bundle their own interpreter, and telling an end user
   to `pip install` into a DCC's private Python is a support burden. Instead, drop the built
   `.pyd` (renamed `ritoshark.pyd` / `ritoshark.so`) straight into the plugin's own folder and
   `import ritoshark` from there — no interpreter-wide install needed.

The wheel is built `cp39-abi3`: one wheel covers CPython 3.9 through 3.14+, which is what makes
vendoring practical across hosts that each embed a different minor version — Maya 2023 ships
Python 3.9, Maya 2024/2025 ship 3.10/3.11, and Blender 4.x ships 3.11. The same `.pyd` loads in
all of them.

## Support matrix

| Format | Read | Write |
|---|---|---|
| `.skn` skinned mesh | yes | yes |
| `.skl` skeleton | yes | yes |
| `.anm` animation | yes | yes |
| `.scb` static mesh | yes | yes |
| `.mapgeo` map geometry | yes | yes (re-emit only, no construction) |
| `.sco` static mesh | yes | **no** — Riot removed the format; `rs_mesh` writes no `.sco` |
| `.tex` texture | yes | no |
| `.wad` archive | yes | yes |
| `.bin` property bin | yes (plain Python values) | no |

## Buffer layouts

This is the contract callers write DCC code against. Every geometry buffer is tightly packed
little-endian bytes; unpack with `struct`, `array`, or `memoryview(...).cast(...)`.

| Buffer | Layout |
|---|---|
| `Skn.positions` / `Skn.normals` | 3 x float32 per vertex |
| `Skn.uvs` | 2 x float32 per vertex |
| `Skn.blend_indices` | 4 x uint32 per vertex |
| `Skn.blend_weights` | 4 x float32 per vertex |
| `Skn.indices` | 1 x uint32 per corner (narrowed to uint16 on write; values above 65535 raise `WriteError`) |
| `Scb.positions` | 3 x float32 per vertex |
| `Tex.rgba` | 4 x uint8 per pixel, row-major, top-down |
| `Tex.rgba_f32` | 4 x float32 per pixel in 0..1, row-flipped bottom-up to match Blender's `image.pixels` |
| `MapModel.positions()` | 3 x float32 per vertex |
| `MapModel.indices()` | 1 x uint32 per corner |

`MapModel.positions()` and `MapModel.indices()` are **methods**, not properties — `MapModel`
doesn't own its vertex data, it references a shared interleaved buffer on the parent `MapGeo`
and de-interleaves it on every call. Everything else in the table is a property.

## Building a `.wad`

```python
import ritoshark

data = ritoshark.build_wad({
    "assets/characters/aatrox/aatrox.skn": skn_bytes,
    "assets/characters/aatrox/aatrox.dds": dds_bytes,
})
ritoshark.build_wad_to_path("aatrox.wad.client", {...})
```

Chunk keys are either the in-WAD path (hashed with `wad_hash` internally) or an
already-computed path hash — mix both in the same dict if convenient. There is no
callback form: the builder pulls each chunk's bytes twice internally (once to size it,
once to write it), so the whole mapping is taken eagerly rather than as a generator or
file-like callback that could return different bytes on the second pass.

## Blender example

```python
import ritoshark

skn = ritoshark.Skn.from_path("aatrox.skn")
mesh = bpy.data.meshes.new("aatrox")
mesh.vertices.add(skn.vertex_count)
mesh.vertices.foreach_set("co", memoryview(skn.positions).cast("f"))
```

`foreach_set` reads directly from the packed buffer, so there's no per-vertex Python loop
between the parsed file and the mesh data.

## Maya example

```python
import struct
import ritoshark
from maya.api.OpenMaya import MFloatPointArray

skn = ritoshark.Skn.from_path("aatrox.skn")
floats = struct.unpack(f"<{skn.vertex_count * 3}f", skn.positions)
points = MFloatPointArray()
for i in range(skn.vertex_count):
    points.append((floats[i * 3], floats[i * 3 + 1], floats[i * 3 + 2], 1.0))
```

Maya's API takes Python sequences rather than a flat buffer, so `struct.unpack` is the
practical middle step there instead of a zero-copy `memoryview` cast.

## `.bin` reading is lossy

`read_bin` / `read_bin_bytes` return a plain Python `dict`. This is a deliberate, lossy view,
not a bug:

- Field, class, and entry names come back as raw FNV1a-32 integers. `rs_bin` stores no names on
  disk — resolving a hash to a name is a hash-dictionary concern that lives outside this crate.
- The `LIST` vs `LIST2` tag distinction is dropped.
- The pointer-vs-embed struct distinction is dropped.
- Duplicate map keys are dropped (Python dicts can't represent them).

There is no `write_bin`. Writing requires an editable tree that preserves all of the above,
which is a separate future design, not an oversight here.

## `Anm` byte-exactness

An unedited `Anm.from_path(...).to_bytes()` round-trip is byte-exact — the original source
bytes are reproduced verbatim. Calling `make_editable()` switches the instance to an editable
track representation and drops that byte-exact passthrough permanently: subsequent writes
re-emit as uncompressed `r3d2anmd` (v4), not the original encoding.

`make_editable()` is of limited use today: tracks are exposed as read-only clones via the
`tracks` property, so there is no way to mutate a track in place after calling it. The only way
to build a new animation is `Anm.new(fps, tracks)` from scratch — `make_editable()` does not
currently unlock in-place editing of an existing one.

## Tests

```bash
cd crates/rs_python
python -m pytest tests/ -v
```

Fixtures come from the repo-root `Sample-Files/` directory, gitignored per CLAUDE.md §11 since
real game assets are never committed. No `.skl` or `.sco` fixture exists there, so those tests
skip by design rather than fail.

`tests/dcc/` holds smoke scripts that load the built wheel inside an actual Blender or Maya
interpreter. They are not collected by pytest — see `tests/dcc/README.md` for how to run them.
The abi3 wheel has been verified to load under Maya 2023's `mayapy` (Python 3.9.7, the abi3
floor) and build a mesh from a real `.skn`; the Blender script is written and reviewed but has
not been executed on this machine, since Blender isn't installed here.
