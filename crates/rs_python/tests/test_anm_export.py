"""Public export report, fallback and existing-save compatibility."""
import math
import struct
import pytest
import ritoshark as rs


def clip(extreme=False):
    q = (0., math.sin(.713578 / 2), 0., math.cos(.713578 / 2))
    frames = [rs.AnimFrame(i / 30, q, (1_000_000. if extreme and i else 0., 2., 3.), (1., 1., 1.)) for i in range(2)]
    return rs.Anm.new(30., [rs.AnimTrack(1, frames)])


def test_export_report_and_unchanged_save(tmp_path):
    original = clip().to_v5_bytes()
    animation = rs.Anm.from_bytes(original)
    result = animation.export('v4')
    assert isinstance(result, rs.AnmExport)
    assert result.requested_format == result.format == 'v4'
    assert result.fallbacks == []
    assert struct.unpack_from('<I', result.bytes, 8)[0] == 4
    assert result.metrics['model_space'] is None
    assert result.metrics['rotation'] <= .034906585
    target = tmp_path / 'output.anm'
    result.to_path(str(target))
    assert target.read_bytes() == result.bytes
    assert animation.is_byte_exact and animation.to_bytes() == original


def test_clock_failure_returns_verified_v5():
    result = clip(True).export('canm')
    assert result.requested_format == 'canm' and result.format == 'v5'
    assert result.fallbacks[0][0] == 'canm'
    assert 'clock precision' in result.fallbacks[0][1]
    assert result.metrics['translation'] <= .5
    assert struct.unpack_from('<I', result.bytes, 8)[0] == 5


def test_full_chain_can_reach_v4():
    result = clip(True).export('canm', rotation_error=.000001)
    assert result.format == 'v4'
    assert [x[0] for x in result.fallbacks] == ['canm', 'v5']
    assert result.metrics['rotation'] <= .000001


def test_canm_is_available_without_a_skeleton():
    result = clip().export('canm')
    assert result.format == 'canm' and not result.fallbacks
    assert result.bytes[:8] == b'r3d2canm'
    assert rs.Anm.from_bytes(result.bytes).frame_count == 2


def test_invalid_input_is_not_a_fallback():
    with pytest.raises(ValueError, match='format must be'):
        clip().export('auto')
    with pytest.raises(rs.WriteError, match='nonnegative'):
        clip().export('canm', scale_error=-1)
    with pytest.raises(rs.WriteError, match='skeleton is empty'):
        clip().export('canm', skeleton=rs.Skl.new([], []))
    with pytest.raises(rs.WriteError):
        rs.Anm.new(0., clip().tracks).export('canm')
