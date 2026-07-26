import ast
from pathlib import Path

import ritoshark

STUB = Path(__file__).parent.parent / "python" / "ritoshark" / "__init__.pyi"


def test_stub_exists_and_parses():
    assert STUB.exists()
    ast.parse(STUB.read_text(encoding="utf-8"))


def test_stub_covers_every_public_name():
    tree = ast.parse(STUB.read_text(encoding="utf-8"))
    declared = {
        node.name
        for node in tree.body
        if isinstance(node, (ast.ClassDef, ast.FunctionDef))
    }
    declared |= {
        target.id
        for node in tree.body
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name)
        for target in [node.target]
    }

    public = {n for n in dir(ritoshark) if not n.startswith("_")}
    missing = public - declared
    assert not missing, f"stub is missing: {sorted(missing)}"


def test_no_leaked_submodule():
    assert "ritoshark" not in dir(ritoshark), (
        "the compiled extension submodule leaked into the public package namespace"
    )


def test_public_names_match_expected_set():
    expected = {
        "AnimFrame", "AnimTrack", "Anm", "FormatError", "Joint", "MapGeo",
        "MapModel", "MapSubmesh", "ParseError", "Scb", "ScbFace", "Sco",
        "Skl", "Skn", "Submesh", "Tex", "UnsupportedVersion", "Wad",
        "WadChunk", "WriteError", "read_bin", "read_bin_bytes", "wad_hash",
        "__version__",
    }
    public = {n for n in dir(ritoshark) if not n.startswith("_")}
    public.add("__version__")
    assert public == expected
