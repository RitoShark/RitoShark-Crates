import struct
import pytest
import ritoshark as rs


POSITIONS = struct.pack('<9f', 10, 20, 30, 40, 20, 30, 10, 50, 30)


def mesh(**kwargs):
    return rs.Scb.new('authored', POSITIONS, [rs.ScbFace('body.part', (0, 1, 2), ((0, 0), (1, 0), (0, 1)))], **kwargs)


def test_scb_metadata_colors_tail_and_bounds():
    tail = bytes(range(9)) + struct.pack('<6f', 1, 2, 3, 4, 5, 6)
    colors = bytes([7, 63, 129, 210] * 3)
    source = mesh(central=(12, 23, 34), colors=colors, flags=5, trailing=tail)
    data = source.to_bytes()
    result = rs.Scb.from_bytes(data)
    assert result.central == (12, 23, 34)
    assert result.flags == 5 and result.colors == colors and result.trailing == tail
    assert result.version == (3, 2) and result.to_bytes() == data
    assert struct.unpack_from('<6f', data, 152) == (10, 20, 30, 40, 50, 30)


@pytest.mark.parametrize('version', [(2, 1), (2, 2), (3, 1), (3, 2)])
def test_legacy_scb_versions(version):
    source = mesh(version=version)
    assert rs.Scb.from_bytes(source.to_bytes()).version == version


@pytest.mark.parametrize('kwargs', [
    {'colors': b'bad'}, {'central': (float('nan'), 0, 0)},
    {'colors': bytes(12), 'version': (2, 2)}, {'flags': 1, 'trailing': b'short'},
    {'version': (7, 2)},
])
def test_invalid_scb_metadata_rejected(kwargs):
    with pytest.raises(rs.WriteError):
        mesh(**kwargs)


def test_replace_geometry_failure_is_atomic():
    source = mesh(flags=1, trailing=bytes(9))
    before = source.to_bytes()
    with pytest.raises(rs.WriteError, match='face layout'):
        source.replace_geometry(POSITIONS, [rs.ScbFace('x', (2, 1, 0), ((0, 0), (0, 0), (0, 0)))])
    assert source.to_bytes() == before


def test_reader_rejects_bad_indices_types_and_truncation():
    good = mesh().to_bytes()
    for offset, value in [(176, 9), (228, 900)]:
        bad = bytearray(good)
        struct.pack_into('<I', bad, offset, value)
        with pytest.raises(rs.FormatError):
            rs.Scb.from_bytes(bytes(bad))
    with pytest.raises(rs.FormatError):
        rs.Scb.from_bytes(good[:200])
