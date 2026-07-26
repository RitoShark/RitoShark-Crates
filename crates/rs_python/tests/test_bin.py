import glob
import struct
from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
BIN = FIXTURES / "aatrox.bin"

_multi_matches = glob.glob(str(FIXTURES / "aatrox_multi_skins_*.bin"))
MULTI_BINS = [Path(p) for p in _multi_matches]
MULTI_BIN = MULTI_BINS[0] if MULTI_BINS else None

PLAIN_TYPES = (type(None), bool, int, float, str, tuple, list, dict)

EMBED = 0x83


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


def _fields(body: bytes) -> bytes:
    return struct.pack("<H", 1) + struct.pack("<I", 1) + bytes([EMBED]) + body


def _embed(class_hash: int, fields_bytes: bytes) -> bytes:
    return (
        struct.pack("<I", class_hash)
        + struct.pack("<I", len(fields_bytes))
        + fields_bytes
    )


def _build_deeply_nested_bin(depth: int) -> bytes:
    value = _embed(1, struct.pack("<H", 0))
    for _ in range(depth):
        value = _embed(1, _fields(value))

    entry_fields = _fields(value)
    entry_body = struct.pack("<I", 1) + entry_fields
    out = bytearray()
    out += b"PROP"
    out += struct.pack("<I", 3)
    out += struct.pack("<I", 0)
    out += struct.pack("<I", 1)
    out += struct.pack("<I", 1)
    out += struct.pack("<I", len(entry_body))
    out += entry_body
    return bytes(out)


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.read_bin_bytes(b"not a bin file")


def test_deeply_nested_bin_raises_instead_of_crashing():
    data = _build_deeply_nested_bin(1000)
    with pytest.raises(ritoshark.FormatError):
        ritoshark.read_bin_bytes(data)


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


@pytest.mark.skipif(
    not (BIN.exists() or MULTI_BINS), reason="no fixture present"
)
def test_real_fixtures_parse_within_depth_guard():
    for path in [BIN, *MULTI_BINS]:
        if not path.exists():
            continue
        doc = ritoshark.read_bin(str(path))
        for entry in doc["entries"].values():
            _walk(entry)
