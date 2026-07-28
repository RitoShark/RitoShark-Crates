import array
import struct
from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
MAPGEO = FIXTURES / "bloom.mapgeo"
ALL_MAPGEOS = [
    FIXTURES / "bloom.mapgeo",
    FIXTURES / "spectator_only_banners.mapgeo",
    FIXTURES / "ultbook.mapgeo",
]

MAX_VERTEX_ELEMENTS = 15


def _first_model_vertex_count_offset(data: bytes) -> int:
    pos = 4
    (version,) = struct.unpack_from("<I", data, pos)
    pos += 4

    if version < 7:
        pos += 1

    if version >= 17:
        (count,) = struct.unpack_from("<I", data, pos)
        pos += 4
        for _ in range(count):
            pos += 4
            (length,) = struct.unpack_from("<I", data, pos)
            pos += 4 + length
    else:
        if version >= 9:
            (length,) = struct.unpack_from("<I", data, pos)
            pos += 4 + length
        if version >= 11:
            (length,) = struct.unpack_from("<I", data, pos)
            pos += 4 + length

    (desc_count,) = struct.unpack_from("<I", data, pos)
    pos += 4
    for _ in range(desc_count):
        pos += 4
        (element_count,) = struct.unpack_from("<I", data, pos)
        pos += 4 + element_count * 8
        pos += (MAX_VERTEX_ELEMENTS - element_count) * 8

    (vb_count,) = struct.unpack_from("<I", data, pos)
    pos += 4
    for _ in range(vb_count):
        if version >= 13:
            pos += 1
        (size,) = struct.unpack_from("<I", data, pos)
        pos += 4 + size

    (ib_count,) = struct.unpack_from("<I", data, pos)
    pos += 4
    for _ in range(ib_count):
        if version >= 13:
            pos += 1
        (size,) = struct.unpack_from("<I", data, pos)
        pos += 4 + size

    pos += 4  # model count
    if version < 12:
        (name_len,) = struct.unpack_from("<I", data, pos)
        pos += 4 + name_len
    return pos


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.MapGeo.from_bytes(b"not a mapgeo")


def test_submesh_name_does_not_collide():
    assert ritoshark.MapSubmesh is not ritoshark.Submesh


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_read_models():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    assert mg.version >= 5
    assert len(mg.models) > 0
    model = mg.models[0]
    assert isinstance(model.name, str)
    assert len(model.transform) == 16
    assert model.vertex_count > 0
    assert isinstance(model.submeshes, list)


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_positions_match_vertex_count():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    for model in mg.models:
        positions = model.positions()
        assert len(positions) == model.vertex_count * 12


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_indices_in_range():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    for model in mg.models:
        indices = model.indices()
        icount = len(indices) // 4
        if icount == 0:
            continue
        values = struct.unpack(f"<{icount}I", indices)
        assert max(values) < model.vertex_count


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_write_roundtrip_is_byte_exact():
    raw = MAPGEO.read_bytes()
    mg = ritoshark.MapGeo.from_bytes(raw)
    assert mg.to_bytes() == raw


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_all_fixtures_parse_and_roundtrip(path):
    if not path.exists():
        pytest.skip("fixture not present")
    raw = path.read_bytes()
    mg = ritoshark.MapGeo.from_bytes(raw)
    assert mg.version >= 5
    assert len(mg.models) > 0
    assert mg.to_bytes() == raw
    print(f"{path.name}: version={mg.version} models={len(mg.models)}")


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_all_fixtures_deinterleave_correctly(path):
    if not path.exists():
        pytest.skip("fixture not present")
    mg = ritoshark.MapGeo.from_path(str(path))
    for model in mg.models:
        positions = model.positions()
        assert len(positions) == model.vertex_count * 12
        indices = model.indices()
        icount = len(indices) // 4
        if icount == 0:
            continue
        values = struct.unpack(f"<{icount}I", indices)
        assert max(values) < model.vertex_count


def test_mapsubmesh_is_not_submesh():
    assert ritoshark.MapSubmesh is not ritoshark.Submesh
    assert ritoshark.MapSubmesh.__name__ == "MapSubmesh"


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_model_fields():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    model = mg.models[0]
    assert isinstance(model.layer, int)
    assert isinstance(model.disable_backface_culling, bool)
    minb, maxb = model.bounds
    assert len(minb) == 3
    assert len(maxb) == 3
    for ov in model.texture_overrides:
        assert isinstance(ov[0], int)
        assert isinstance(ov[1], str)
    for sm in model.submeshes:
        assert isinstance(sm.hash, int)
        assert isinstance(sm.name, str)
        assert isinstance(sm.index_start, int)
        assert isinstance(sm.index_count, int)
        assert isinstance(sm.min_vertex, int)
        assert isinstance(sm.max_vertex, int)
        repr(sm)
    repr(model)


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_mapgeo_len_and_repr():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    assert len(mg) == len(mg.models)
    repr(mg)


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_vertex_count_offset_is_located_correctly(path):
    if not path.exists():
        pytest.skip("fixture not present")
    raw = bytearray(path.read_bytes())
    offset = _first_model_vertex_count_offset(bytes(raw))
    (found,) = struct.unpack_from("<I", raw, offset)
    mg = ritoshark.MapGeo.from_path(str(path))
    assert found == mg.models[0].vertex_count


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_oversized_vertex_count_raises_format_error_instead_of_aborting(path):
    if not path.exists():
        pytest.skip("fixture not present")
    raw = bytearray(path.read_bytes())
    offset = _first_model_vertex_count_offset(bytes(raw))
    struct.pack_into("<I", raw, offset, 0xFFFFFF00)

    mg = ritoshark.MapGeo.from_bytes(bytes(raw))
    with pytest.raises(ritoshark.FormatError):
        mg.models[0].positions()


def _floats(data: bytes) -> array.array:
    out = array.array("f")
    out.frombytes(data)
    return out


TRIANGLE_POSITIONS = array.array("f", [0, 0, 0, 100, 0, 0, 0, 100, 0]).tobytes()
TRIANGLE_NORMALS = array.array("f", [0, 1, 0, 0, 1, 0, 0, 1, 0]).tobytes()
TRIANGLE_UVS = array.array("f", [0, 0, 1, 0, 0, 1]).tobytes()
TRIANGLE_INDICES = array.array("H", [0, 1, 2]).tobytes()


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_every_model_decodes_all_the_attributes_its_streams_declare(path):
    """A model's buffers use consecutive descriptions from its vertex_description_id, not the
    stored one for all of them, so attributes living in a later stream must still decode."""
    if not path.exists():
        pytest.skip("fixture not present")
    mg = ritoshark.MapGeo.from_path(str(path))
    for model in mg.models:
        if not model.vertex_count:
            continue
        assert len(model.positions()) // 12 == model.vertex_count
        assert len(model.normals()) // 12 in (0, model.vertex_count)
        assert len(model.uvs()) // 8 in (0, model.vertex_count)


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_replace_geometry_keeps_the_rest_of_the_file(path):
    if not path.exists():
        pytest.skip("fixture not present")
    mg = ritoshark.MapGeo.from_path(str(path))
    before = len(mg)
    last = mg.models[-1]
    last_positions = last.positions()
    last_transform = last.transform

    mg.replace_geometry(
        0, "ritoshark_test", TRIANGLE_POSITIONS, TRIANGLE_INDICES,
        TRIANGLE_NORMALS, TRIANGLE_UVS,
    )
    out = ritoshark.MapGeo.from_bytes(mg.to_bytes())

    assert len(out) == before
    assert out.version == mg.version
    edited = out.models[0]
    assert edited.vertex_count == 3
    assert list(_floats(edited.positions())) == [0, 0, 0, 100, 0, 0, 0, 100, 0]
    assert edited.submeshes[0].name == "ritoshark_test"
    assert out.models[-1].positions() == last_positions
    assert out.models[-1].transform == last_transform


@pytest.mark.parametrize("path", ALL_MAPGEOS, ids=lambda p: p.name)
def test_add_and_remove_model(path):
    if not path.exists():
        pytest.skip("fixture not present")
    mg = ritoshark.MapGeo.from_path(str(path))
    before = len(mg)

    index = mg.add_model(
        "RitoShark_Added", TRIANGLE_POSITIONS, TRIANGLE_INDICES,
        TRIANGLE_NORMALS, TRIANGLE_UVS, None, 255,
    )
    assert index == before
    out = ritoshark.MapGeo.from_bytes(mg.to_bytes())
    assert len(out) == before + 1
    added = out.models[index]
    assert added.layer == 255
    assert list(_floats(added.positions())) == [0, 0, 0, 100, 0, 0, 0, 100, 0]
    assert list(_floats(added.normals())) == [0, 1, 0, 0, 1, 0, 0, 1, 0]
    assert list(_floats(added.uvs())) == [0, 0, 1, 0, 0, 1]

    mg.remove_model(index)
    out = ritoshark.MapGeo.from_bytes(mg.to_bytes())
    assert len(out) == before


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_set_transform_and_layer():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    mg.set_transform(0, [2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 10, 20, 30, 1])
    mg.set_layer(0, 7)
    out = ritoshark.MapGeo.from_bytes(mg.to_bytes())
    assert out.models[0].transform[0] == 2.0
    assert out.models[0].transform[12] == 10.0
    assert out.models[0].layer == 7


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
@pytest.mark.parametrize(
    "kwargs",
    [
        {"indices": array.array("H", [0, 1, 9]).tobytes()},
        {"positions": b"\x00\x00\x00"},
        {"normals": array.array("f", [0, 1, 0]).tobytes()},
    ],
)
def test_invalid_geometry_is_rejected(kwargs):
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    call = {
        "positions": TRIANGLE_POSITIONS,
        "indices": TRIANGLE_INDICES,
        "normals": TRIANGLE_NORMALS,
        "uvs": TRIANGLE_UVS,
    }
    call.update(kwargs)
    with pytest.raises(ritoshark.WriteError):
        mg.replace_geometry(0, "bad", **call)


@pytest.mark.skipif(not MAPGEO.exists(), reason="fixture not present")
def test_bad_model_index_is_rejected():
    mg = ritoshark.MapGeo.from_path(str(MAPGEO))
    with pytest.raises(ritoshark.WriteError):
        mg.replace_geometry(999999, "bad", TRIANGLE_POSITIONS, TRIANGLE_INDICES)
    with pytest.raises(ritoshark.WriteError):
        mg.remove_model(999999)
