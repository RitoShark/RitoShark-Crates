# rs_mesh — real-files report

Verification of the `.skn` skinned-mesh and `.scb` static-mesh readers/writers against the real
sample files in `Sample-Files/`, cross-checked against the C# LeagueToolkit oracle (the primary
base), the Rust `ltk_mesh` crate, and the `pyritofile` `skn.py` / `so.py` readers.

> `.sco` (text static mesh) was removed from the game (it crashes the client) and is intentionally
> not a target: there is no `.sco` writer and no `.sco` sample. The reader is retained but
> de-prioritized.

## `.skn` (skinned) results

Three real `.skn` samples were added; all are `r3d2 SKN` magic `0x00112233`, version **4.1**, with
the **Basic** (52-byte) vertex layout and a 5–6 entry submesh range table. Each one parses, has an
index count that is a multiple of three, ranges that stay inside the shared vertex/index buffers,
and a 12-byte zero "end tab" after the vertex buffer. **All three round-trip byte-for-byte.**

| File | Version | Ranges | Vertices | Indices | Vertex type | End-tab | Round-trip |
|---|---|---|---|---|---|---|---|
| `aatrox.skn` | 4.1 | 5 | 5498 | 25782 | Basic | 12 B | byte-exact |
| `aatrox_skin01.skn` | 4.1 | 5 | 7016 | 33972 | Basic | 12 B | byte-exact |
| `aatrox_skin02.skn` | 4.1 | 6 | 23532 | 36210 | Basic | 12 B | byte-exact |

## `.scb` (static) results

All four samples are `r3d2Mesh` version **3.2**. Every file parses with no error; all faces
reference in-range vertices; vertex-color count (where present) matches the vertex count. **All
four now round-trip byte-for-byte** through the new `.scb` writer.

| File | Version | Vertices | Faces | Materials | Vertex colors | Parse |
|---|---|---|---|---|---|---|
| `aatrox_base_q_cone_blast.scb` | 3.2 | 120 | 200 | 1 | 120 (vtype 1) | OK |
| `aatrox_skin11_swipemesh02.scb` | 3.2 | 102 | 100 | 1 | none | OK |
| `blitzcrank_skin47_lighting_cyl_02.scb` | 3.2 | 120 | 120 | 1 | none | OK |
| `floorslash.scb` | 3.2 | 50 | 48 | 1 | none | OK |

The `flags=5` blitzcrank file additionally carries 1104 post-face bytes (the `HasVcp` block plus a
local-origin/pivot pair); these are now preserved verbatim (see below).

## Cross-check vs references

`pyritofile so.py` was run over the same files. Vertex/face/color counts agree exactly with
rs_mesh on every file it could read:

| File | rs_mesh verts/faces | pyritofile verts/faces | Agreement |
|---|---|---|---|
| aatrox_base_q_cone_blast | 120 / 200 | 120 / 200 | match (colors 120, vtype 1) |
| aatrox_skin11_swipemesh02 | 102 / 100 | 102 / 100 | match |
| blitzcrank_skin47_lighting_cyl_02 | 120 / 120 | **read error** | rs_mesh more robust |
| floorslash | 50 / 48 | 50 / 48 | match |

Format details confirmed against the C# oracle (`StaticMesh.ReadBinary`) and `ltk_mesh`:

- **Version gate.** Oracle accepts `major in {2,3}` or `minor == 1` (covers 2.1, 3.1, 3.2, and
  legacy 1.1). rs_mesh whitelists exactly `(2,1) | (3,1) | (3,2)`; correct for the observed and
  documented versions, slightly stricter than the oracle's broad predicate (it would reject a
  hypothetical 1.1).
- **Vertex-color gate.** Oracle reads the color flag for `major >= 3 && minor >= 2`; rs_mesh
  uses `major == 3 && minor == 2`. Equivalent across the supported version set.
- **UV layout.** Oracle and rs_mesh agree: three U floats then three V floats per face, paired
  as `(u0,v0),(u1,v1),(u2,v2)`. rs_mesh is correct here.
- **Color format.** Oracle reads per-vertex colors as BGRA `u8`; rs_mesh stores the raw 4 bytes
  (`[u8; 4]`) without naming a channel order, which is lossless for read-back.

## Gap analysis vs C#

Comparing rs_mesh to the C# `SkinnedMesh.ReadFromSimpleSkin` / `WriteSimpleSkin` and
`StaticMesh.ReadBinary` / `WriteBinary` oracles, three things were missing for true lossless
round-trips:

1. **`.skn` end-tab dropped.** The C# writer emits a trailing 12-byte zero "end tab" after the
   vertex buffer (`endTab`), and every real v4.1 file contains it. The old rs_mesh reader stopped
   at the last vertex, so `read → write` was 12 bytes short and *not* byte-exact on real files.

2. **`.scb` had no writer.** The static side was read-only, so the byte-exact contract the skinned
   format claimed did not exist for `.scb`.

3. **`.scb` header fields + post-face tail dropped.** The reader discarded the `flags` word, the
   bounding box, and all post-face bytes. For `blitzcrank` (`flags == 5`) that tail is **1104
   bytes** = `faceCount*9` (= 1080) `HasVcp` RGB values + a 24-byte local-origin/pivot pair. The
   C# oracle only reads the 1080-byte RGB block and ignores the remaining 24, so no reference
   reader fully models this tail; rs_mesh now keeps it raw instead of guessing.

Confirmed *correct* already (no change needed): the UV "u u u / v v v" layout, the `major==3 &&
minor==2` vertex-color gate (equivalent to the oracle's `>=3 && >=2` over the supported set), the
opaque-`u32` flags handling (more robust than `pyritofile`, which rejects `flags == 5`), and the
decision to keep degenerate faces (lossless vs the oracle/`pyritofile`, which drop them).

## What I implemented

- **`.skn` end-tab preservation.** `SkinnedMesh` gained a `trailing: Vec<u8>` field; the reader
  `read_to_end`s the post-vertex bytes and the writer emits them. Real v4.1 files now round-trip
  byte-for-byte.
- **`.scb` writer.** `StaticMesh::to_scb_writer` + a `Serialize` impl that reproduces magic,
  version, NUL-padded name, counts, flags, bounds, `vertexType`, positions, optional colors,
  central point, faces, and the raw tail. `StaticMesh` now also stores `version`, `flags`,
  `bounding_box`, `vertex_type`, and a `trailing` blob so nothing is lost on read.
- **`.scb` post-face tail preservation.** The `HasVcp` RGB block and local-origin/pivot data are
  captured into `trailing` and written back verbatim, making the static form lossless without
  needing to fully decode the (reference-ambiguous) layout.
- **Tests.** `tests/real_files.rs` now asserts `.skn` v4.1 + Basic layout, in-bounds ranges, the
  12-byte tab, the 1104-byte blitzcrank tail, and a byte-exact round-trip on all 3 `.skn` and all
  4 `.scb`; `tests/roundtrip.rs` adds byte-exact `.scb` writer coverage.

## Remaining gaps

- The `HasVcp` per-face colors and the local-origin/pivot vectors are preserved as opaque bytes but
  not surfaced as typed fields. Decoding them (verified against more `flags`-bit-0 samples) would
  let callers edit them, not just round-trip them.
- The version gate is `(2,1)|(3,1)|(3,2)` for `.scb` and `{0,1,2,4}.1` for `.skn`; the oracle's
  predicates are looser (e.g. a hypothetical `.scb` 1.1). No such sample exists, so this is
  unverified rather than known-broken.
- `.sco` remains read-only by design (removed from the game).
