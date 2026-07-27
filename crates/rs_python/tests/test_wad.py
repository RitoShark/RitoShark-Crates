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


def _synthetic_chunks():
    return {
        "assets/empty.bin": b"",
        "assets/small.txt": b"hello world",
        "assets/repeating.bin": b"ab" * 51200,
    }


def test_build_wad_round_trips_chunk_data():
    chunks = _synthetic_chunks()
    data = ritoshark.build_wad(chunks)
    wad = ritoshark.Wad.from_bytes(data)
    for path, expected in chunks.items():
        assert wad.read_path(path) == expected


def test_build_wad_version_and_chunk_count():
    chunks = _synthetic_chunks()
    data = ritoshark.build_wad(chunks)
    wad = ritoshark.Wad.from_bytes(data)
    assert wad.version == (3, 4)
    assert len(wad) == len(chunks)


def test_build_wad_path_and_hash_keys_agree():
    chunks = _synthetic_chunks()
    by_path = ritoshark.build_wad(chunks)
    by_hash = ritoshark.build_wad(
        {ritoshark.wad_hash(path): data for path, data in chunks.items()}
    )
    wad_path = ritoshark.Wad.from_bytes(by_path)
    wad_hash = ritoshark.Wad.from_bytes(by_hash)
    assert sorted(c.path_hash for c in wad_path.chunks) == sorted(
        c.path_hash for c in wad_hash.chunks
    )


def test_build_wad_compresses_repetitive_data():
    chunks = _synthetic_chunks()
    data = ritoshark.build_wad(chunks)
    wad = ritoshark.Wad.from_bytes(data)
    repeating_hash = ritoshark.wad_hash("assets/repeating.bin")
    chunk = next(c for c in wad.chunks if c.path_hash == repeating_hash)
    assert chunk.compressed_size < chunk.uncompressed_size


def test_build_wad_to_path_writes_file(tmp_path):
    chunks = _synthetic_chunks()
    out = tmp_path / "out.wad.client"
    ritoshark.build_wad_to_path(str(out), chunks)
    wad = ritoshark.Wad.from_path(str(out))
    for path, expected in chunks.items():
        assert wad.read_path(path) == expected


def test_build_wad_rejects_bad_key_type():
    with pytest.raises(TypeError):
        ritoshark.build_wad({1.5: b"data"})


def test_build_wad_rejects_bad_value_type():
    with pytest.raises(TypeError):
        ritoshark.build_wad({"assets/foo.bin": "not bytes"})


def test_build_wad_rejects_list_value():
    with pytest.raises(TypeError):
        ritoshark.build_wad({"a": [104, 105]})


def test_build_wad_rejects_tuple_value():
    with pytest.raises(TypeError):
        ritoshark.build_wad({"a": (65, 66, 67)})


def test_build_wad_rejects_str_value():
    with pytest.raises(TypeError):
        ritoshark.build_wad({"a": "hi"})


def test_build_wad_accepts_bytearray_value():
    data = ritoshark.build_wad({"a": bytearray(b"hi")})
    wad = ritoshark.Wad.from_bytes(data)
    assert wad.read_path("a") == b"hi"


def test_build_wad_accepts_memoryview_value():
    data = ritoshark.build_wad({"a": memoryview(b"hi")})
    wad = ritoshark.Wad.from_bytes(data)
    assert wad.read_path("a") == b"hi"


def test_build_wad_rejects_bool_key_true():
    with pytest.raises(TypeError):
        ritoshark.build_wad({True: b"x"})


def test_build_wad_rejects_bool_key_false():
    with pytest.raises(TypeError):
        ritoshark.build_wad({False: b"x"})


def test_build_wad_negative_int_key_still_raises():
    with pytest.raises(TypeError):
        ritoshark.build_wad({-1: b"x"})


def test_build_wad_str_and_int_keys_still_work():
    chunks = {"assets/foo.bin": b"hi", ritoshark.wad_hash("assets/bar.bin"): b"bye"}
    data = ritoshark.build_wad(chunks)
    wad = ritoshark.Wad.from_bytes(data)
    assert wad.read_path("assets/foo.bin") == b"hi"
    assert wad.read_path("assets/bar.bin") == b"bye"


@pytest.mark.skipif(not WAD.exists(), reason="fixture not present")
def test_build_wad_round_trips_real_archive():
    original = ritoshark.Wad.from_path(str(WAD))
    chunks = {}
    for c in original.chunks:
        data = original.read(c.path_hash)
        assert data is not None
        chunks[c.path_hash] = data

    rebuilt_bytes = ritoshark.build_wad(chunks)
    rebuilt = ritoshark.Wad.from_bytes(rebuilt_bytes)

    assert len(rebuilt) == len(original)
    for c in original.chunks:
        assert rebuilt.read(c.path_hash) == chunks[c.path_hash]
