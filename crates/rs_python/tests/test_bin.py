import glob
from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
BIN = FIXTURES / "aatrox.bin"

_multi_matches = glob.glob(str(FIXTURES / "aatrox_multi_skins_skin0_*.bin"))
MULTI_BIN = Path(_multi_matches[0]) if _multi_matches else None

PLAIN_TYPES = (type(None), bool, int, float, str, tuple, list, dict)


def _walk(v, depth=0):
    max_depth = depth
    assert isinstance(v, PLAIN_TYPES), type(v)
    if isinstance(v, dict):
        for x in v.values():
            max_depth = max(max_depth, _walk(x, depth + 1))
    elif isinstance(v, list):
        for x in v:
            max_depth = max(max_depth, _walk(x, depth + 1))
    elif isinstance(v, tuple):
        for x in v:
            assert isinstance(x, (int, float))
    return max_depth


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.read_bin_bytes(b"not a bin file")


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_document_shape():
    doc = ritoshark.read_bin(str(BIN))
    assert isinstance(doc, dict)
    assert isinstance(doc["version"], int)
    assert isinstance(doc["is_patch"], bool)
    assert isinstance(doc["linked"], list)
    assert all(isinstance(s, str) for s in doc["linked"])
    assert isinstance(doc["entries"], dict)
    assert all(isinstance(k, int) for k in doc["entries"])


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_entry_keys_are_raw_hashes():
    doc = ritoshark.read_bin(str(BIN))
    assert all(isinstance(k, int) for k in doc["entries"])
    entry = next(iter(doc["entries"].values()))
    assert isinstance(entry, dict)
    assert isinstance(entry["__class__"], int)
    assert all(isinstance(k, int) for k in entry if k != "__class__")


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_entries_are_plain_values():
    doc = ritoshark.read_bin(str(BIN))
    for entry in doc["entries"].values():
        _walk(entry)


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_read_bin_bytes_matches_read_bin():
    from_path = ritoshark.read_bin(str(BIN))
    from_bytes = ritoshark.read_bin_bytes(BIN.read_bytes())
    assert from_path["version"] == from_bytes["version"]
    assert from_path["is_patch"] == from_bytes["is_patch"]
    assert set(from_path["entries"].keys()) == set(from_bytes["entries"].keys())


@pytest.mark.skipif(
    not (BIN.exists() or MULTI_BIN is not None), reason="no fixture present"
)
def test_vec_value_appears_as_float_tuple():
    found = []

    def search(v):
        if isinstance(v, tuple):
            found.append(v)
        elif isinstance(v, dict):
            for x in v.values():
                search(x)
        elif isinstance(v, list):
            for x in v:
                search(x)

    for path in (BIN, MULTI_BIN):
        if path is None or not path.exists():
            continue
        doc = ritoshark.read_bin(str(path))
        for entry in doc["entries"].values():
            search(entry)

    vec_like = [t for t in found if len(t) in (2, 3, 4) and all(isinstance(x, float) for x in t)]
    assert vec_like, "expected at least one Vec2/Vec3/Vec4 tuple of floats in available fixtures"


@pytest.mark.skipif(MULTI_BIN is None, reason="multi-skin fixture not present")
def test_large_multi_skin_bin_parses():
    doc = ritoshark.read_bin(str(MULTI_BIN))
    assert isinstance(doc["entries"], dict)
    assert len(doc["entries"]) > 0

    max_depth = 0
    for entry in doc["entries"].values():
        max_depth = max(max_depth, _walk(entry))

    print(
        f"\n{MULTI_BIN.name}: {len(doc['entries'])} entries, max nesting depth {max_depth}"
    )
