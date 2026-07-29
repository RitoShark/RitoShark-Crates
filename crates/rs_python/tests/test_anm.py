from pathlib import Path

import pytest

import ritoshark

FIXTURES = Path(__file__).parent.parent.parent.parent / "Sample-Files"
ANM = FIXTURES / "dance_windup.anm"
COMPRESSED = sorted(FIXTURES.glob("compressed_*.anm"))

pytestmark = pytest.mark.skipif(not ANM.exists(), reason="fixture not present")


def test_read_tracks():
    anm = ritoshark.Anm.from_path(str(ANM))
    assert anm.fps > 0
    assert len(anm.tracks) > 0
    t = anm.tracks[0]
    assert isinstance(t.joint_hash, int)
    assert len(t.frames) > 0
    f = t.frames[0]
    assert len(f.translation) == 3
    assert len(f.scale) == 3
    assert len(f.rotation) == 4


def test_unedited_write_is_byte_exact():
    raw = ANM.read_bytes()
    anm = ritoshark.Anm.from_bytes(raw)
    assert anm.is_byte_exact
    assert anm.to_bytes() == raw


def test_make_editable_drops_byte_exactness():
    anm = ritoshark.Anm.from_path(str(ANM))
    assert anm.is_byte_exact
    anm.make_editable()
    assert not anm.is_byte_exact


def test_reimport_equivalence_after_edit():
    src = ritoshark.Anm.from_path(str(ANM))
    rebuilt = ritoshark.Anm.new(
        fps=src.fps,
        tracks=[
            ritoshark.AnimTrack(
                t.joint_hash,
                [
                    ritoshark.AnimFrame(f.time, f.rotation, f.translation, f.scale)
                    for f in t.frames
                ],
            )
            for t in src.tracks
        ],
    )
    back = ritoshark.Anm.from_bytes(rebuilt.to_bytes())
    assert len(back.tracks) == len(src.tracks)
    assert sorted(t.joint_hash for t in back.tracks) == sorted(t.joint_hash for t in src.tracks)
    assert back.frame_count == src.frame_count

    src_first = src.tracks[0]
    back_first = next(t for t in back.tracks if t.joint_hash == src_first.joint_hash)
    for a, b in zip(src_first.frames, back_first.frames):
        assert a.translation == pytest.approx(b.translation, abs=1e-4)
        assert a.scale == pytest.approx(b.scale, abs=1e-4)


def test_editability_contract_end_to_end():
    raw = ANM.read_bytes()
    anm = ritoshark.Anm.from_bytes(raw)
    assert anm.is_byte_exact
    assert anm.to_bytes() == raw

    anm.make_editable()
    assert not anm.is_byte_exact

    rebuilt = anm.to_bytes()
    assert rebuilt != raw

    back = ritoshark.Anm.from_bytes(rebuilt)
    assert len(back.tracks) == len(anm.tracks)
    assert sorted(t.joint_hash for t in back.tracks) == sorted(
        t.joint_hash for t in anm.tracks
    )

    src_first = anm.tracks[0]
    back_first = next(t for t in back.tracks if t.joint_hash == src_first.joint_hash)
    assert len(back_first.frames) == len(src_first.frames)
    for a, b in zip(src_first.frames, back_first.frames):
        assert a.translation == pytest.approx(b.translation, abs=1e-4)


@pytest.mark.skipif(not COMPRESSED, reason="no compressed fixtures present")
@pytest.mark.parametrize("path", COMPRESSED, ids=lambda p: p.name)
def test_compressed_unedited_write_is_byte_exact(path):
    raw = path.read_bytes()
    anm = ritoshark.Anm.from_bytes(raw)
    assert anm.is_byte_exact
    assert anm.to_bytes() == raw
