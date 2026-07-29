import struct
from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
SKN = FIXTURES / "aatrox.skn"

pytestmark = pytest.mark.skipif(not SKN.exists(), reason="fixture not present")


def test_read_buffers_are_consistent():
    skn = ritoshark.Skn.from_path(str(SKN))
    n = skn.vertex_count
    assert n > 0
    assert len(skn.positions) == n * 3 * 4
    assert len(skn.normals) == n * 3 * 4
    assert len(skn.uvs) == n * 2 * 4
    assert len(skn.blend_indices) == n * 4 * 4
    assert len(skn.blend_weights) == n * 4 * 4
    assert len(skn.indices) % (3 * 4) == 0
    assert len(skn.submeshes) > 0
    assert isinstance(skn.submeshes[0].name, str)


def test_indices_are_in_range():
    skn = ritoshark.Skn.from_path(str(SKN))
    count = len(skn.indices) // 4
    indices = struct.unpack(f"<{count}I", skn.indices)
    assert max(indices) < skn.vertex_count


def test_reimport_equivalence():
    src = ritoshark.Skn.from_path(str(SKN))
    rebuilt = ritoshark.Skn.new(
        positions=src.positions,
        normals=src.normals,
        uvs=src.uvs,
        blend_indices=src.blend_indices,
        blend_weights=src.blend_weights,
        indices=src.indices,
        submeshes=[
            ritoshark.Submesh(s.name, s.vertex_start, s.vertex_count, s.index_start, s.index_count)
            for s in src.submeshes
        ],
    )
    back = ritoshark.Skn.from_bytes(rebuilt.to_bytes())
    assert back.vertex_count == src.vertex_count
    assert back.positions == src.positions
    assert back.normals == src.normals
    assert back.uvs == src.uvs
    assert back.indices == src.indices
    assert [s.name for s in back.submeshes] == [s.name for s in src.submeshes]


def test_uvs_roundtrip_exactly():
    src = ritoshark.Skn.from_path(str(SKN))
    rebuilt = ritoshark.Skn.new(
        positions=src.positions,
        normals=src.normals,
        uvs=src.uvs,
        blend_indices=src.blend_indices,
        blend_weights=src.blend_weights,
        indices=src.indices,
        submeshes=[
            ritoshark.Submesh(s.name, s.vertex_start, s.vertex_count, s.index_start, s.index_count)
            for s in src.submeshes
        ],
    )
    back = ritoshark.Skn.from_bytes(rebuilt.to_bytes())
    assert back.uvs == src.uvs


def test_blend_indices_roundtrip_and_are_byte_range():
    src = ritoshark.Skn.from_path(str(SKN))
    rebuilt = ritoshark.Skn.new(
        positions=src.positions,
        normals=src.normals,
        uvs=src.uvs,
        blend_indices=src.blend_indices,
        blend_weights=src.blend_weights,
        indices=src.indices,
        submeshes=[
            ritoshark.Submesh(s.name, s.vertex_start, s.vertex_count, s.index_start, s.index_count)
            for s in src.submeshes
        ],
    )
    back = ritoshark.Skn.from_bytes(rebuilt.to_bytes())
    assert back.blend_indices == src.blend_indices
    count = len(src.blend_indices) // 4
    values = struct.unpack(f"<{count}I", src.blend_indices)
    assert all(v < 256 for v in values)


def test_write_rejects_mismatched_buffer_lengths():
    with pytest.raises(ritoshark.WriteError):
        ritoshark.Skn.new(
            positions=b"\x00" * 12,
            normals=b"\x00" * 24,
            uvs=b"\x00" * 8,
            blend_indices=b"\x00" * 16,
            blend_weights=b"\x00" * 16,
            indices=b"\x00" * 12,
            submeshes=[ritoshark.Submesh("body", 0, 1, 0, 3)],
        )


def test_write_rejects_index_over_u16():
    with pytest.raises(ritoshark.WriteError) as exc:
        ritoshark.Skn.new(
            positions=b"\x00" * 12,
            normals=b"\x00" * 12,
            uvs=b"\x00" * 8,
            blend_indices=b"\x00" * 16,
            blend_weights=b"\x00" * 16,
            indices=struct.pack("<3I", 0, 1, 70000),
            submeshes=[ritoshark.Submesh("body", 0, 1, 0, 3)],
        )
    assert "65535" in str(exc.value)


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Skn.from_bytes(b"not a skn file at all")
