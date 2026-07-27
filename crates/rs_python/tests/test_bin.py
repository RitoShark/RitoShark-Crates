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


ALL_FIXTURES = [p for p in [BIN, *MULTI_BINS] if p.exists()]


def _walk_editable(v, visit):
    visit(v)
    if isinstance(v, dict) and "__type__" in v:
        t = v["__type__"]
        if t in ("list", "list2"):
            for item in v["items"]:
                _walk_editable(item, visit)
        elif t == "map":
            for k, val in v["entries"]:
                _walk_editable(k, visit)
                _walk_editable(val, visit)
        elif t in ("pointer", "embed"):
            for val in v["fields"].values():
                _walk_editable(val, visit)
        elif t == "option":
            if v["value"] is not None:
                _walk_editable(v["value"], visit)
    elif isinstance(v, dict):
        for val in v.values():
            _walk_editable(val, visit)
    elif isinstance(v, list):
        for item in v:
            _walk_editable(item, visit)


def _find(doc, predicate):
    found = []

    def visit(v):
        if predicate(v):
            found.append(v)

    for entry in doc["entries"]:
        _walk_editable(entry["fields"], visit)
    return found


@pytest.mark.skipif(not ALL_FIXTURES, reason="no fixture present")
@pytest.mark.parametrize("path", ALL_FIXTURES, ids=lambda p: p.name)
def test_editable_round_trip_is_byte_exact(path):
    original = path.read_bytes()
    doc = ritoshark.read_bin_editable(str(path))
    rebuilt = ritoshark.write_bin_bytes(doc)
    assert rebuilt == original


@pytest.mark.skipif(not ALL_FIXTURES, reason="no fixture present")
def test_editable_document_shape():
    doc = ritoshark.read_bin_editable(str(ALL_FIXTURES[0]))
    assert isinstance(doc["version"], int)
    assert isinstance(doc["is_patch"], bool)
    assert isinstance(doc["patch_header"], bytes)
    assert len(doc["patch_header"]) == 8
    assert isinstance(doc["linked"], list)
    assert isinstance(doc["entries"], list)
    assert isinstance(doc["patches"], list)
    for entry in doc["entries"]:
        assert isinstance(entry["hash"], int)
        assert isinstance(entry["class"], int)
        assert isinstance(entry["fields"], dict)
        assert all(isinstance(k, int) for k in entry["fields"])


@pytest.mark.skipif(not ALL_FIXTURES, reason="no fixture present")
def test_list2_is_not_demoted_to_list():
    found_list2 = False
    for path in ALL_FIXTURES:
        doc = ritoshark.read_bin_editable(str(path))
        matches = _find(doc, lambda v: isinstance(v, dict) and v.get("__type__") == "list2")
        if matches:
            found_list2 = True
    assert found_list2, "expected at least one list2 in the fixtures"


@pytest.mark.skipif(not ALL_FIXTURES, reason="no fixture present")
def test_pointer_is_not_flattened_to_embed():
    found_pointer = False
    for path in ALL_FIXTURES:
        doc = ritoshark.read_bin_editable(str(path))
        matches = _find(doc, lambda v: isinstance(v, dict) and v.get("__type__") == "pointer")
        if matches:
            found_pointer = True
    assert found_pointer, "expected at least one pointer in the fixtures"


@pytest.mark.skipif(not ALL_FIXTURES, reason="no fixture present")
def test_option_present_distinct_from_absent():
    found_present = False
    found_absent = False
    for path in ALL_FIXTURES:
        doc = ritoshark.read_bin_editable(str(path))
        options = _find(doc, lambda v: isinstance(v, dict) and v.get("__type__") == "option")
        for opt in options:
            if opt["value"] is None:
                found_absent = True
            else:
                found_present = True
    assert found_present, "expected at least one present option in the fixtures"
    assert found_absent, "expected at least one absent option in the fixtures"


def test_field_order_is_preserved():
    doc = ritoshark.read_bin_editable(str(BIN)) if BIN.exists() else None
    if doc is None:
        pytest.skip("fixture not present")
    for entry in doc["entries"]:
        keys = list(entry["fields"].keys())
        assert keys == sorted(keys, key=keys.index)

    entry = doc["entries"][0]
    fields = entry["fields"]
    ordered_keys = list(fields.keys())
    rebuilt = ritoshark.write_bin_bytes(doc)
    redoc = ritoshark.read_bin_editable_bytes(rebuilt)
    reentry = next(e for e in redoc["entries"] if e["hash"] == entry["hash"])
    assert list(reentry["fields"].keys()) == ordered_keys


def test_map_with_duplicate_keys_round_trips_in_memory():
    doc = {
        "version": 3,
        "is_patch": False,
        "patch_header": b"\x00" * 8,
        "linked": [],
        "entries": [
            {
                "hash": 0x1,
                "class": 0x2,
                "fields": {
                    0x3: {
                        "__type__": "map",
                        "key": "u32",
                        "value": "string",
                        "entries": [
                            (7, "first"),
                            (7, "second"),
                        ],
                    }
                },
            }
        ],
        "patches": [],
    }
    data = ritoshark.write_bin_bytes(doc)
    redoc = ritoshark.read_bin_editable_bytes(data)
    field = redoc["entries"][0]["fields"][0x3]
    assert field["__type__"] == "map"
    assert field["entries"] == [(7, "first"), (7, "second")]


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_mutation_changes_only_the_targeted_value():
    doc = ritoshark.read_bin_editable(str(BIN))

    target_entry = None
    target_key = None
    for entry in doc["entries"]:
        for k, v in entry["fields"].items():
            if isinstance(v, str) and v:
                target_entry, target_key = entry, k
                break
        if target_entry is not None:
            break
    assert target_entry is not None, "expected at least one plain string field"

    original_value = target_entry["fields"][target_key]
    new_value = original_value + "_mutated"
    target_entry["fields"][target_key] = new_value

    data = ritoshark.write_bin_bytes(doc)
    redoc = ritoshark.read_bin_editable_bytes(data)

    original_doc = ritoshark.read_bin_editable(str(BIN))

    diffs = []

    def compare(a, b, path):
        if a == b:
            return
        if isinstance(a, dict) and isinstance(b, dict) and set(a.keys()) == set(b.keys()):
            for k in a:
                compare(a[k], b[k], f"{path}.{k}" if not isinstance(k, int) else f"{path}[{k:#x}]")
        elif isinstance(a, list) and isinstance(b, list) and len(a) == len(b):
            for i, (x, y) in enumerate(zip(a, b)):
                compare(x, y, f"{path}[{i}]")
        else:
            diffs.append((path, a, b))

    compare(original_doc["entries"], redoc["entries"], "entries")

    assert len(diffs) == 1, diffs
    path, before, after = diffs[0]
    assert before == original_value
    assert after == new_value


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_read_bin_and_read_bin_editable_agree_on_shared_values():
    lossy = ritoshark.read_bin(str(BIN))
    editable = ritoshark.read_bin_editable(str(BIN))

    editable_by_hash = {e["hash"]: e for e in editable["entries"]}

    def unwrap(v):
        if isinstance(v, dict) and "__type__" in v:
            t = v["__type__"]
            if t in ("list", "list2"):
                return [unwrap(x) for x in v["items"]]
            if t == "map":
                return {unwrap(k): unwrap(val) for k, val in v["entries"]}
            if t in ("pointer", "embed"):
                return {"__class__": v["class"], **{k: unwrap(val) for k, val in v["fields"].items()}}
            if t == "option":
                return None if v["value"] is None else unwrap(v["value"])
            return v["value"]
        if isinstance(v, dict):
            return {k: unwrap(val) for k, val in v.items()}
        if isinstance(v, list):
            return [unwrap(x) for x in v]
        return v

    for path_hash, lossy_entry in lossy["entries"].items():
        editable_entry = editable_by_hash[path_hash]
        assert lossy_entry["__class__"] == editable_entry["class"]
        for k, v in lossy_entry.items():
            if k == "__class__":
                continue
            assert unwrap(editable_entry["fields"][k]) == v


def test_write_bin_bytes_unknown_type_raises_write_error_with_path():
    doc = {
        "version": 3,
        "is_patch": False,
        "patch_header": b"\x00" * 8,
        "linked": [],
        "entries": [
            {"hash": 1, "class": 2, "fields": {3: {"__type__": "bogus", "value": 1}}}
        ],
        "patches": [],
    }
    with pytest.raises(ritoshark.WriteError) as exc:
        ritoshark.write_bin_bytes(doc)
    assert "entries[0]" in str(exc.value)
    assert "fields[0x3]" in str(exc.value)


def test_write_bin_bytes_odd_length_map_entry_raises_write_error():
    doc = {
        "version": 3,
        "is_patch": False,
        "patch_header": b"\x00" * 8,
        "linked": [],
        "entries": [
            {
                "hash": 1,
                "class": 2,
                "fields": {
                    3: {
                        "__type__": "map",
                        "key": "u32",
                        "value": "string",
                        "entries": [(1, 2, 3)],
                    }
                },
            }
        ],
        "patches": [],
    }
    with pytest.raises(ritoshark.WriteError) as exc:
        ritoshark.write_bin_bytes(doc)
    assert "entries[0]" in str(exc.value)


def test_write_bin_bytes_non_int_field_key_raises_write_error():
    doc = {
        "version": 3,
        "is_patch": False,
        "patch_header": b"\x00" * 8,
        "linked": [],
        "entries": [{"hash": 1, "class": 2, "fields": {"not_an_int": 5}}],
        "patches": [],
    }
    with pytest.raises(ritoshark.WriteError) as exc:
        ritoshark.write_bin_bytes(doc)
    assert "entries[0]" in str(exc.value)


def test_write_bin_bytes_wrong_typed_item_raises_write_error_with_path():
    doc = {
        "version": 3,
        "is_patch": False,
        "patch_header": b"\x00" * 8,
        "linked": [],
        "entries": [
            {
                "hash": 1,
                "class": 2,
                "fields": {
                    3: {
                        "__type__": "list",
                        "item": "string",
                        "items": ["a", "b", 3],
                    }
                },
            }
        ],
        "patches": [],
    }
    with pytest.raises(ritoshark.WriteError) as exc:
        ritoshark.write_bin_bytes(doc)
    message = str(exc.value)
    assert "items[2]" in message
    assert "string" in message


def test_ptch_document_round_trips_in_memory():
    doc = {
        "version": 3,
        "is_patch": True,
        "patch_header": bytes(range(8)),
        "linked": [],
        "entries": [
            {"hash": 0x10, "class": 0x20, "fields": {0x30: "hello"}},
        ],
        "patches": [
            {"key_hash": 0x1234, "path": "some.dotted.path", "value": "replacement"},
            {"key_hash": 0x5678, "path": "other.path", "value": {"__type__": "u32", "value": 42}},
        ],
    }
    data = ritoshark.write_bin_bytes(doc)
    redoc = ritoshark.read_bin_editable_bytes(data)

    assert redoc["is_patch"] is True
    assert redoc["patch_header"] == bytes(range(8))
    assert len(redoc["patches"]) == 2
    assert redoc["patches"][0]["key_hash"] == 0x1234
    assert redoc["patches"][0]["path"] == "some.dotted.path"
    assert redoc["patches"][0]["value"] == "replacement"
    assert redoc["patches"][1]["value"] == {"__type__": "u32", "value": 42}


def test_deeply_nested_editable_doc_raises_instead_of_overflowing():
    value = {"__type__": "embed", "class": 1, "fields": {}}
    for _ in range(1000):
        value = {"__type__": "embed", "class": 1, "fields": {1: value}}
    doc = {
        "version": 3,
        "is_patch": False,
        "patch_header": b"\x00" * 8,
        "linked": [],
        "entries": [{"hash": 1, "class": 1, "fields": {1: value}}],
        "patches": [],
    }
    with pytest.raises(ritoshark.WriteError):
        ritoshark.write_bin_bytes(doc)


def test_deeply_nested_bin_raises_via_editable_reader_too():
    data = _build_deeply_nested_bin(1000)
    with pytest.raises(ritoshark.FormatError):
        ritoshark.read_bin_editable_bytes(data)


def test_write_bin_writes_to_path(tmp_path):
    if not BIN.exists():
        pytest.skip("fixture not present")
    doc = ritoshark.read_bin_editable(str(BIN))
    out_path = tmp_path / "roundtrip.bin"
    ritoshark.write_bin(str(out_path), doc)
    assert out_path.read_bytes() == BIN.read_bytes()
