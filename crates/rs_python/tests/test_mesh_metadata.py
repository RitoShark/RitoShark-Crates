"""Synthetic format tests: no private Sample-Files directory required."""
import math
import struct
import pytest
import ritoshark as rs


def skin(**overrides):
    values = dict(
        positions=struct.pack('<9f', 10, 20, 30, 110, 20, 30, 10, 120, 30),
        normals=struct.pack('<9f', *([0, 0, 1] * 3)),
        uvs=struct.pack('<6f', 0, 0, 1, 0, 0, 1),
        blend_indices=struct.pack('<12I', *([0] * 12)),
        blend_weights=struct.pack('<12f', *([1, 0, 0, 0] * 3)),
        indices=struct.pack('<3I', 0, 1, 2), submeshes=[rs.Submesh('body', 0, 3, 0, 3)])
    values.update(overrides)
    return rs.Skn.new(**values)


def test_optional_vertex_fields_and_opaque_metadata():
    colors = bytes([7, 80, 190, 255] * 3)
    tangents = struct.pack('<12f', *([1, 0, 0, -1] * 3))
    source = skin(colors=colors, tangents=tangents, flags=3, prefix_block=b'prefix', trailing=b'tail')
    result = rs.Skn.from_bytes(source.to_bytes())
    assert result.colors == colors
    assert result.tangents == tangents
    assert result.flags == 3
    assert result.prefix_block == b'prefix'
    assert result.trailing == b'tail'
    assert result.indices == source.indices


@pytest.mark.parametrize('colors,tangents,stride,kind', [
    (None, None, 52, 0), (bytes(12), None, 56, 1),
    (bytes(12), struct.pack('<12f', *([1, 0, 0, 1] * 3)), 72, 2)])
def test_vertex_layout_and_nonzero_bounds(colors, tangents, stride, kind):
    data = skin(colors=colors, tangents=tangents).to_bytes()
    assert struct.unpack_from('<2I', data, 104) == (stride, kind)
    assert struct.unpack_from('<6f', data, 112) == (10, 20, 30, 110, 120, 30)
    center = struct.unpack_from('<3f', data, 136)
    radius, = struct.unpack_from('<f', data, 148)
    for point in [(10, 20, 30), (110, 20, 30), (10, 120, 30)]:
        assert math.dist(point, center) <= radius + 1e-4


@pytest.mark.parametrize('override', [
    {'colors': b'bad'}, {'tangents': bytes(48)}, {'prefix_block': b'undeclared'},
    {'submeshes': [rs.Submesh('x' * 65, 0, 3, 0, 3)]},
    {'submeshes': [rs.Submesh('body', 0, 4, 0, 3)]},
    {'submeshes': [rs.Submesh('body', 1, 2, 0, 3)]},
    {'blend_weights': struct.pack('<12f', *([-1, 0, 0, 0] * 3))},
    {'positions': struct.pack('<9f', *([float('nan'), 0, 0] * 3))},
])
def test_invalid_mesh_inputs(override):
    with pytest.raises(rs.WriteError):
        skin(**override)


def test_skeleton_flags():
    joint = rs.Joint('root', 0, -1, 3.5, (0, 0, 0), (1, 1, 1), (0, 0, 0, 1),
                     (0, 0, 0), (1, 1, 1), (0, 0, 0, 1), flags=7)
    result = rs.Skl.from_bytes(rs.Skl.new([joint], [0], 'name', 'asset', flags=9).to_bytes())
    assert (result.flags, result.name, result.asset) == (9, 'name', 'asset')
    assert (result.joints[0].flags, result.joints[0].radius) == (7, 3.5)
