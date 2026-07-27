# DCC smoke tests

These verify the abi3 wheel loads inside the host interpreter and that the packed
buffers land correctly in real scene data. They are not run by pytest.

Build the wheel first:

    cd crates/rs_python && maturin build --release

## Blender

Targets **Blender 4.x only**. `MeshPolygon.loop_total` is read-only from Blender 4.0
onward (confirmed via Blender's own API docs, and via the exact `AttributeError` a real
4.x run raised against an earlier version of this script), so the script only sets
`loop_start` (evenly spaced by 3) and never touches `loop_total`. This does not work on
Blender 3.x, which requires `loop_total` to be set explicitly. No version-branching is
implemented; if 3.x support is ever needed, that is a separate script.

**What is documentation-level confidence, not execution-verified:** whether a
`loop_start`-only polygons array actually makes Blender derive every polygon's loop
count correctly, in particular for the *last* polygon. Blender's internal face-offset
representation is understood to use N+1 offset entries for N faces, the last being a
sentinel equal to the total corner count — whether `mesh.update()` reconstructs that
sentinel correctly from a `loop_start`-only array was not confirmed by any source
consulted while writing this script. If it does not, the last triangle specifically is
the one at risk of being malformed or dropped, which would still print something that
looks almost right on a casual glance. The script calls `mesh.validate(verbose=True)`
after `update()` and asserts both the polygon count and every polygon's `loop_total`
(via `foreach_get`) equal 3, specifically to catch that failure mode loudly. Treat the
first real run on Blender 4.x as the actual test of this design, not this script's mere
existence.

Install the wheel into Blender's bundled Python, then:

    blender --background --python tests/dcc/blender_smoke.py -- Sample-Files/aatrox.skn

Expected: `OK blender vertices=N triangles=M`

Status on this machine: unverified. Blender is not installed here, so this script has
only been checked by inspection (buffer layouts, API usage, the `loop_total` read-only
change) — it has never actually been executed. Run it on a machine with Blender 4.x
before trusting it as a passing gate, and watch specifically for a `validate()` warning
or a wrong last-`loop_total` assertion failure, since that is the one part of this
script's design that documentation alone cannot confirm.

## Maya

Maya 2023 lives at `C:\Program Files\Autodesk\Maya2023`. Install the wheel into
`mayapy`, then:

    "C:\Program Files\Autodesk\Maya2023\bin\mayapy.exe" -m pip install --force-reinstall --no-deps target\wheels\ritoshark-0.1.0-cp39-abi3-win_amd64.whl
    "C:\Program Files\Autodesk\Maya2023\bin\mayapy.exe" tests/dcc/maya_smoke.py Sample-Files/aatrox.skn

Expected: `OK maya vertices=N`

The script checks both vertex count and vertex 0's world-space position (against the
first `positions` triple, tolerance `1e-4`) so a mesh with the right count but scrambled
or misaligned data still fails.

Status on this machine: verified. Ran against `Sample-Files/aatrox.skn` (5498 vertices)
under Maya 2023's `mayapy` (Python 3.9.7, the abi3 floor) and printed
`OK maya vertices=5498`.

Maya is the oracle for Maya behaviour. A failure here is real even when every
pytest passes, because the system interpreter is not the interpreter that ships
in these applications.
