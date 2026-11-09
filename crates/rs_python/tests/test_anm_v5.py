"""Independent v5 layout/palette decoding and explicit writer integration."""
import math
import struct
import pytest
import ritoshark as rs
from test_anm_compressed import make_clip, angular


def test_v5_independent_decoding_and_explicit_save_policy(tmp_path):
    source = make_clip(61)
    raw = source.to_v5_bytes()
    assert raw[:8] == b'r3d2anmd'
    version, size, token, flags1, flags2, joints, frames = struct.unpack_from('<7I', raw, 8)
    assert (version, size, token, flags1, flags2, joints, frames) == (5,len(raw)-12,0,0,0,3,61)
    dt, = struct.unpack_from('<f',raw,36)
    hashes, _, _, vectors, rotations, rows = struct.unpack_from('<6i',raw,40)
    hashes, vectors, rotations, rows = [v+12 for v in (hashes,vectors,rotations,rows)]
    assert vectors == 76 and rows+6*joints*frames == len(raw)
    assert (rotations-vectors)%12 == 0 and (hashes-rotations)%6 == 0
    for j,track in enumerate(source.tracks):
        assert struct.unpack_from('<I',raw,hashes+j*4)[0] == track.joint_hash
        for f,sample in enumerate(track.frames):
            t,s,r = struct.unpack_from('<3H',raw,rows+(f*joints+j)*6)
            assert vectors+t*12 < rotations and vectors+s*12 < rotations
            assert rotations+r*6 < hashes
            assert struct.unpack_from('<3f',raw,vectors+t*12) == sample.translation
            assert struct.unpack_from('<3f',raw,vectors+s*12) == sample.scale
            bits = int.from_bytes(raw[rotations+r*6:rotations+r*6+6],'little')
            q = [((bits>>shift)&32767)*math.sqrt(2)/32767-1/math.sqrt(2) for shift in (30,15,0)]
            q.insert((bits>>45)&3,math.sqrt(max(0,1-sum(x*x for x in q))))
            assert angular(q,sample.rotation) < .0002
            assert abs(f*dt-sample.time)<1e-6
    parsed = rs.Anm.from_bytes(raw)
    assert parsed.is_byte_exact and parsed.to_bytes()==raw
    assert parsed.to_v5_bytes()[:8] == b'r3d2anmd'
    assert parsed.is_byte_exact and parsed.to_bytes()==raw
    parsed.make_editable()
    assert struct.unpack_from('<I',parsed.to_bytes(),8)[0]==4
    dest = tmp_path/'v5.anm'
    source.to_v5_path(str(dest));assert dest.read_bytes()==raw
    dest.write_bytes(b'preserve')
    with pytest.raises(rs.WriteError): rs.Anm.new(0,source.tracks).to_v5_path(str(dest))
    assert dest.read_bytes()==b'preserve'


@pytest.mark.parametrize('steps',[0,65])
def test_invalid_compression_grid(steps):
    with pytest.raises(rs.WriteError): make_clip().to_compressed_bytes(sample_subdivisions=steps)


def test_skeleton_argument_is_validated_before_writing(tmp_path):
    # Empty skeleton must not silently imply successful model-space verification.
    skeleton=rs.Skl.new([], [])
    source=make_clip()
    with pytest.raises(rs.WriteError,match='skeleton is empty'):
        source.to_compressed_bytes(skeleton=skeleton)
    target=tmp_path/'protected.anm';target.write_bytes(b'original')
    with pytest.raises(rs.WriteError):source.to_compressed_path(str(target),skeleton=skeleton)
    assert target.read_bytes()==b'original'
