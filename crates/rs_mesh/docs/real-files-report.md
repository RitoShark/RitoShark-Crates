# rs_mesh — real-files report

Verification of the `.scb` static-mesh reader against the real sample files in
`sample-files/`, cross-checked against the C# LeagueToolkit oracle, the Rust `ltk_mesh`
crate, and the `pyritofile` `so.py` reader.

## Per-file results

All four samples are `r3d2Mesh` version **3.2**. Every file parses with no error; all faces
reference in-range vertices; vertex-color count (where present) matches the vertex count.

| File | Version | Vertices | Faces | Materials | Vertex colors | Parse |
|---|---|---|---|---|---|---|
| `aatrox_base_q_cone_blast.scb` | 3.2 | 120 | 200 | 1 | 120 (vtype 1) | OK |
| `aatrox_skin11_swipemesh02.scb` | 3.2 | 102 | 100 | 1 | none | OK |
| `blitzcrank_skin47_lighting_cyl_02.scb` | 3.2 | 120 | 120 | 1 | none | OK |
| `floorslash.scb` | 3.2 | 50 | 48 | 1 | none | OK |

No `.sco` (text) and no `.skn` (skinned) samples are present, so those paths are covered only
by synthetic fixtures in `tests/roundtrip.rs`.

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

## Discrepancies found

1. **`HasVcp` / trailing face data is not parsed.** `blitzcrank_skin47_lighting_cyl_02.scb` has
   `flags == 5` (bit 0 `HasVcp` + bit 1 set). After the face list it carries **1104 trailing
   bytes** that rs_mesh does not read. The oracle, when `HasVcp` is set, reads `faceCount * 9`
   RGB bytes (= 1080 here), which does not equal 1104 either — so this file's tail is not a clean
   per-face RGB block in any of the reference readers. Because rs_mesh stops at the face list, the
   counts and indices it returns are correct; only the (unmodelled) trailing data is dropped.

2. **`pyritofile` rejects `blitzcrank` outright** because its `SOFlag(IntEnum)` cannot represent
   `flags == 5`. rs_mesh treats `flags` as an opaque `u32`, so it parses the file. This is a point
   where rs_mesh is strictly more robust than `pyritofile`.

3. **Degenerate faces.** The oracle and `pyritofile` skip faces where two indices are equal;
   rs_mesh keeps every face as written. This is a deliberate lossless choice (face count then
   matches the on-disk header), but it is a behavioral difference to be aware of when comparing
   index buffers. None of the four samples actually contain degenerate faces, so counts still
   matched across all readers.

## Improvements / TODO

- **Obtain real `.skn` samples** and at least one `.sco` to exercise those readers against
  game data; today only synthetic fixtures cover them.
- **Add a `.scb` (and `.sco`) writer** to enable the byte-exact round-trip contract that the
  skinned format already satisfies. This is the largest missing piece for the static side.
- **Model `HasVcp` / trailing post-face data.** Determine the exact layout of the 1104-byte tail
  in files like `blitzcrank` (against the C# oracle on more samples) and parse it instead of
  ignoring it, so the static reader becomes lossless.

## What I changed

- Added `tests/real_files.rs`: a skip-if-missing harness that parses every `.scb` sample and
  asserts vertex/face counts are non-zero, all face indices are in range, and any vertex-color
  block matches the vertex count.
- No source changes were required: all four real `.scb` files already parse correctly, and the
  observed counts match the `pyritofile` oracle. Added this report and `README.md`.
