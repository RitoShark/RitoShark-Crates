import struct

import pytest

import ritoshark


def dds_bytes(fourcc=0, dxgi=None):
    flags = 0x41 if fourcc == 0 else 4
    header = [124, 0x100F, 4, 4, 16, 0, 1] + [0] * 11
    header += [32, flags, fourcc, 32, 0xFF0000, 0xFF00, 0xFF, 0xFF000000]
    header += [0x1000, 0, 0, 0, 0]
    extension = struct.pack('<5I', dxgi, 3, 0, 1, 0) if dxgi else b''
    payload = bytes([30, 20, 10, 40]) * 16 if fourcc == 0 else bytes(16)
    return b'DDS ' + struct.pack('<31I', *header) + extension + payload


def test_rgba_channels_alpha_and_path(tmp_path):
    data = dds_bytes()
    path = tmp_path / 'texture.dds'
    path.write_bytes(data)
    for image in (ritoshark.Dds.from_bytes(data), ritoshark.Dds.from_path(str(path))):
        assert (image.width, image.height) == (4, 4)
        assert image.rgba == bytes([10, 20, 30, 40]) * 16


@pytest.mark.parametrize('fourcc,dxgi', [
    (b'DXT1', None), (b'DXT3', None), (b'DXT5', None),
    (b'DX10', 83), (b'DX10', 98),
])
def test_compressed_formats(fourcc, dxgi):
    image = ritoshark.Dds.from_bytes(dds_bytes(int.from_bytes(fourcc, 'little'), dxgi))
    assert (image.width, image.height) == (4, 4)
    assert len(image.rgba) == 64


@pytest.mark.parametrize('data', [b'', b'not a dds', dds_bytes()[:128]])
def test_invalid_input(data):
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Dds.from_bytes(data)


def test_missing_path(tmp_path):
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Dds.from_path(str(tmp_path / 'missing.dds'))
