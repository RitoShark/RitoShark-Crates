from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
SCB = FIXTURES / "floorslash.scb"


def test_sco_has_no_writer():
    assert not hasattr(ritoshark.Sco, "to_path")
    assert not hasattr(ritoshark.Sco, "to_bytes")


def test_face_construction():
    f = ritoshark.ScbFace("mat", (0, 1, 2), ((0.0, 0.0), (1.0, 0.0), (1.0, 1.0)))
    assert f.material == "mat"
    assert f.indices == (0, 1, 2)
    assert len(f.uvs) == 3


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Scb.from_bytes(b"neither scb nor sco")


@pytest.mark.skipif(not SCB.exists(), reason="fixture not present")
def test_read_scb():
    scb = ritoshark.Scb.from_path(str(SCB))
    assert len(scb.positions) % 12 == 0
    assert len(scb.faces) > 0
    assert isinstance(scb.faces[0].material, str)


@pytest.mark.skipif(not SCB.exists(), reason="fixture not present")
def test_reimport_equivalence():
    src = ritoshark.Scb.from_path(str(SCB))
    rebuilt = ritoshark.Scb.new(
        name=src.name,
        positions=src.positions,
        faces=[ritoshark.ScbFace(f.material, f.indices, f.uvs) for f in src.faces],
    )
    back = ritoshark.Scb.from_bytes(rebuilt.to_bytes())
    assert back.positions == src.positions
    assert [f.material for f in back.faces] == [f.material for f in src.faces]
    assert [f.indices for f in back.faces] == [f.indices for f in src.faces]


def test_new_rejects_out_of_range_face_index():
    positions = ritoshark._pack_vec3_test([(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)])
    with pytest.raises(ritoshark.WriteError):
        ritoshark.Scb.new(
            name="bad",
            positions=positions,
            faces=[
                ritoshark.ScbFace(
                    "mat", (0, 1, 3), ((0.0, 0.0), (1.0, 0.0), (1.0, 1.0))
                )
            ],
        )


@pytest.mark.skipif(not SCB.exists(), reason="fixture not present")
def test_reimport_preserves_face_uvs():
    src = ritoshark.Scb.from_path(str(SCB))
    rebuilt = ritoshark.Scb.new(
        name=src.name,
        positions=src.positions,
        faces=[ritoshark.ScbFace(f.material, f.indices, f.uvs) for f in src.faces],
    )
    back = ritoshark.Scb.from_bytes(rebuilt.to_bytes())
    assert len(back.faces) == len(src.faces)
    for a, b in zip(back.faces, src.faces):
        assert a.uvs == b.uvs


@pytest.mark.skipif(not SCB.exists(), reason="fixture not present")
def test_replace_geometry_round_trips_byte_exact():
    """Scb.new cannot reproduce an imported file: it has no way to carry the flag word, the
    vertexType word, per-vertex colors or the trailing VCP/pivot block, and it recomputes bounds
    that real files do not always agree with. replace_geometry keeps all of it."""
    source = SCB.read_bytes()
    scb = ritoshark.Scb.from_path(str(SCB))
    scb.replace_geometry(scb.positions, list(scb.faces))
    assert scb.to_bytes() == source


@pytest.mark.skipif(not SCB.exists(), reason="fixture not present")
def test_replace_geometry_keeps_disagreeing_bounds_unless_asked():
    """floorslash.scb ships a bounding box whose min.y (10.0) exceeds its max.y (0.0).
    Recomputing it by default would silently rewrite the file, so it is opt-in."""
    scb = ritoshark.Scb.from_path(str(SCB))
    kept = scb.central
    scb.replace_geometry(scb.positions, list(scb.faces))
    assert scb.central == kept

    scb.replace_geometry(scb.positions, list(scb.faces), recompute_bounds=True)
    assert scb.central != kept


@pytest.mark.skipif(not SCB.exists(), reason="fixture not present")
def test_replace_geometry_rejects_out_of_range_face_index():
    scb = ritoshark.Scb.from_path(str(SCB))
    count = len(scb.positions) // 12
    with pytest.raises(ritoshark.WriteError):
        scb.replace_geometry(
            scb.positions,
            [ritoshark.ScbFace("mat", (0, 1, count), ((0.0, 0.0), (1.0, 0.0), (1.0, 1.0)))],
        )
