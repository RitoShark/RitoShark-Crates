import glob
from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
BIN = FIXTURES / "aatrox.bin"

_multi_matches = glob.glob(str(FIXTURES / "aatrox_multi_skins_*.bin"))
MULTI_BINS = [Path(p) for p in _multi_matches]

ALL_BINS = [p for p in [BIN, *MULTI_BINS] if p.exists()]


@pytest.mark.skipif(not ALL_BINS, reason="no fixtures present")
@pytest.mark.parametrize("path", ALL_BINS, ids=lambda p: p.name)
def test_text_round_trip_is_byte_exact(path):
    original = path.read_bytes()
    text = ritoshark.bin_to_text(str(path))
    rebuilt = ritoshark.text_to_bin_bytes(text)
    assert rebuilt == original


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_bin_to_text_returns_prop_text_str():
    text = ritoshark.bin_to_text(str(BIN))
    assert isinstance(text, str)
    assert text.startswith("#PROP_text")
    assert len(text.splitlines()) > 1


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_bin_to_text_bytes_matches_bin_to_text():
    from_path = ritoshark.bin_to_text(str(BIN))
    from_bytes = ritoshark.bin_to_text_bytes(BIN.read_bytes())
    assert from_path == from_bytes


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_text_to_bin_path_matches_text_to_bin_bytes(tmp_path):
    text = ritoshark.bin_to_text(str(BIN))
    data = ritoshark.text_to_bin_bytes(text)
    out = tmp_path / "roundtrip.bin"
    ritoshark.text_to_bin_path(str(out), text)
    assert out.read_bytes() == data


def test_text_to_bin_bytes_raises_on_garbage():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.text_to_bin_bytes("not ritobin at all")


def test_bin_to_text_raises_on_missing_file():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.bin_to_text("nonexistent.bin")


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_bin_to_text_raises_on_missing_hashes():
    with pytest.raises(ritoshark.FormatError):
        ritoshark.bin_to_text(str(BIN), hashes="nonexistent.txt")


@pytest.mark.skipif(not BIN.exists(), reason="fixture not present")
def test_text_level_edit_round_trips():
    text = ritoshark.bin_to_text(str(BIN))

    doc = ritoshark.read_bin(str(BIN))
    target = None
    for entry in doc["entries"].values():
        for key, value in entry.items():
            if key == "__class__":
                continue
            if isinstance(value, str) and len(value) > 0:
                target = value
                break
        if target is not None:
            break
    assert target is not None, "expected at least one non-empty string field in fixture"

    needle = f'"{target}"'
    assert needle in text, "expected the string value to appear as a quoted literal in text"

    replacement_body = ("x" * len(target)) if target else target
    replacement = f'"{replacement_body}"'
    assert len(replacement) == len(needle)

    edited_text = text.replace(needle, replacement, 1)
    assert edited_text != text

    edited_bytes = ritoshark.text_to_bin_bytes(edited_text)
    edited_doc = ritoshark.read_bin_bytes(edited_bytes)

    found_replacement = False

    def search(v):
        nonlocal found_replacement
        if isinstance(v, str):
            if v == replacement_body:
                found_replacement = True
        elif isinstance(v, dict):
            for x in v.values():
                search(x)
        elif isinstance(v, list):
            for x in v:
                search(x)

    for entry in edited_doc["entries"].values():
        search(entry)

    assert found_replacement, "expected the edited string to appear in the rebuilt bin"
