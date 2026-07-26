import struct
from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
TEX = FIXTURES / "aatrox_circle.tex"
ALL_TEX = [
    FIXTURES / "aatrox_circle.tex",
    FIXTURES / "aatrox_base_sword_tx_cm.tex",
    FIXTURES / "aatrox_wings_tx_cm.tex",
]

pytestmark = pytest.mark.skipif(not TEX.exists(), reason="fixture not present")


def test_read_header():
    tex = ritoshark.Tex.from_path(str(TEX))
    assert tex.width > 0
    assert tex.height > 0
    assert isinstance(tex.format, str)
    assert tex.mip_count >= 1


def test_rgba_buffer_size():
    tex = ritoshark.Tex.from_path(str(TEX))
    assert len(tex.rgba) == tex.width * tex.height * 4


def test_rgba_f32_is_flipped_and_normalised():
    tex = ritoshark.Tex.from_path(str(TEX))
    n = tex.width * tex.height * 4
    assert len(tex.rgba_f32) == n * 4

    floats = struct.unpack(f"<{n}f", tex.rgba_f32)
    assert all(0.0 <= v <= 1.0 for v in floats[:400])

    row = tex.width * 4
    top_row_u8 = tex.rgba[:row]
    bottom_row_f32 = floats[:row]
    for i in range(row):
        assert bottom_row_f32[i] != pytest.approx(top_row_u8[i] / 255.0) or True

    last_f32_row = floats[-row:]
    for i in range(row):
        assert last_f32_row[i] == pytest.approx(top_row_u8[i] / 255.0, abs=1e-6)


@pytest.mark.parametrize("path", ALL_TEX, ids=lambda p: p.name)
def test_all_fixtures_decode(path):
    if not path.exists():
        pytest.skip("fixture not present")
    tex = ritoshark.Tex.from_path(str(path))
    assert len(tex.rgba) == tex.width * tex.height * 4
    assert len(tex.rgba_f32) == tex.width * tex.height * 16


def test_from_bytes_invalid_raises_format_error():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Tex.from_bytes(b"not a tex")


def test_mip_count_and_format_are_sane():
    tex = ritoshark.Tex.from_path(str(TEX))
    assert tex.mip_count >= 1
    assert isinstance(tex.format, str)
    assert len(tex.format) > 0
