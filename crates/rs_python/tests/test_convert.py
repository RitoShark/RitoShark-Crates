import struct

import pytest

import ritoshark


def test_pack_roundtrip_is_tightly_packed():
    packed = ritoshark._pack_vec3_test([(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)])
    assert isinstance(packed, bytes)
    assert len(packed) == 2 * 3 * 4
    assert struct.unpack("<6f", packed) == (1.0, 2.0, 3.0, 4.0, 5.0, 6.0)


def test_unpack_rejects_misaligned_length():
    with pytest.raises(ritoshark.WriteError) as exc:
        ritoshark._unpack_vec3_test(b"\x00" * 13)
    assert "positions" in str(exc.value)
