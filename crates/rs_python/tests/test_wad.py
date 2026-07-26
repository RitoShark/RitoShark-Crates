from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
WAD = FIXTURES / "Azir.wad.client"
DATA_WAD = FIXTURES / "DATA.wad.client"

KNOWN_COMPRESSIONS = {"None", "Gzip", "Satellite", "Zstd", "ZstdMulti"}


def test_wad_hash_lowercases_first():
    assert ritoshark.wad_hash("Assets/Foo.SKN") == ritoshark.wad_hash("assets/foo.skn")


def test_parse_error_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.Wad.from_bytes(b"not a wad")


@pytest.mark.skipif(not WAD.exists(), reason="fixture not present")
def test_list_chunks():
    wad = ritoshark.Wad.from_path(str(WAD))
    assert len(wad.chunks) > 0
    c = wad.chunks[0]
    assert isinstance(c.path_hash, int)
    assert c.uncompressed_size >= 0


@pytest.mark.skipif(not WAD.exists(), reason="fixture not present")
def test_read_chunk_matches_declared_size():
    wad = ritoshark.Wad.from_path(str(WAD))
    c = wad.chunks[0]
    data = wad.read(c.path_hash)
    assert isinstance(data, bytes)
    assert len(data) == c.uncompressed_size


@pytest.mark.skipif(not WAD.exists(), reason="fixture not present")
def test_read_path_returns_none_for_missing():
    wad = ritoshark.Wad.from_path(str(WAD))
    assert wad.read_path("no/such/file/at/all.bin") is None


@pytest.mark.skipif(not WAD.exists(), reason="fixture not present")
def test_chunk_compression_is_known():
    wad = ritoshark.Wad.from_path(str(WAD))
    for c in wad.chunks:
        assert c.compression in KNOWN_COMPRESSIONS


@pytest.mark.skipif(not WAD.exists(), reason="fixture not present")
def test_read_compressed_chunk_matches_declared_size():
    wad = ritoshark.Wad.from_path(str(WAD))
    compressed = [c for c in wad.chunks if c.compression != "None"]
    if not compressed:
        pytest.skip("fixture contains no compressed chunks")
    c = compressed[0]
    data = wad.read(c.path_hash)
    assert isinstance(data, bytes)
    assert len(data) == c.uncompressed_size


@pytest.mark.skipif(
    not (WAD.exists() and DATA_WAD.exists()), reason="fixtures not present"
)
@pytest.mark.parametrize("path", [WAD, DATA_WAD])
def test_both_fixtures_parse(path):
    wad = ritoshark.Wad.from_path(str(path))
    major, minor = wad.version
    assert 0 <= major <= 10
    assert 0 <= minor <= 100
    assert len(wad.chunks) > 0
