# DCC smoke tests

These verify the abi3 wheel loads inside the host interpreter and that the packed
buffers land correctly in real scene data. They are not run by pytest.

Build the wheel first:

    cd crates/rs_python && maturin build --release

## Blender

Install the wheel into Blender's bundled Python, then:

    blender --background --python tests/dcc/blender_smoke.py -- Sample-Files/aatrox.skn

Expected: `OK blender vertices=N triangles=M`

Status on this machine: unverified. Blender is not installed here, so this script has
only been checked by inspection (buffer layouts, API usage) — it has never actually been
executed. Run it on a machine with Blender before trusting it as a passing gate.

## Maya

Maya 2023 lives at `C:\Program Files\Autodesk\Maya2023`. Install the wheel into
`mayapy`, then:

    "C:\Program Files\Autodesk\Maya2023\bin\mayapy.exe" -m pip install --force-reinstall --no-deps target\wheels\ritoshark-0.1.0-cp39-abi3-win_amd64.whl
    "C:\Program Files\Autodesk\Maya2023\bin\mayapy.exe" tests/dcc/maya_smoke.py Sample-Files/aatrox.skn

Expected: `OK maya vertices=N`

Status on this machine: verified. Ran against `Sample-Files/aatrox.skn` (5498 vertices)
under Maya 2023's `mayapy` (Python 3.9.7, the abi3 floor) and printed
`OK maya vertices=5498`.

Maya is the oracle for Maya behaviour. A failure here is real even when every
pytest passes, because the system interpreter is not the interpreter that ships
in these applications.
