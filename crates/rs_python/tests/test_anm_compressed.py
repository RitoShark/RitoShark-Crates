"""Compressed writing, including an independent channel-based file evaluator.

No fixture assets or DCC installation are needed. The reference evaluator groups raw keys
by channel instead of using the Rust reader's shared stream cursor and hot windows.
"""
import bisect
import math
import struct

import pytest
import ritoshark as rs


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def make_clip(count=91):
    tracks = []
    for j in range(3):
        frames = []
        for i in range(count):
            t = i / 30
            angle = math.sin(t * (j + 1)) * 1.2
            sign = -1 if i % 2 else 1
            q = (0, sign * math.sin(angle / 2), 0, sign * math.cos(angle / 2))
            frames.append(rs.AnimFrame(t, q, (j * 3, math.sin(t) * 5, t), (1, 1 + .1 * math.cos(t), 1)))
        tracks.append(rs.AnimTrack(j + 1, frames))
    return rs.Anm.new(30, tracks)


def normalized(q):
    length = math.sqrt(sum(x*x for x in q))
    return tuple(x / length for x in q)


def angular(a, b):
    a, b = normalized(a), normalized(b)
    minus = math.sqrt(sum((x-y)**2 for x, y in zip(a, b)))
    plus = math.sqrt(sum((x+y)**2 for x, y in zip(a, b)))
    return 4 * math.atan2(min(minus, plus), max(minus, plus))


def decode_channels(raw):
    assert raw[:8] == b'r3d2canm'
    version, size, token, flags, joints, keys, caches = struct.unpack_from('<7I', raw, 8)
    assert version == 1 and size == len(raw) - 12
    assert token == int.from_bytes(b'canm', 'little') and flags == 7
    assert caches == 1
    duration, fps = struct.unpack_from('<2f', raw, 36)
    tmin, tmax, smin, smax = [struct.unpack_from('<3f', raw, i) for i in (68, 80, 92, 104)]
    stream, cache, hashes = [v + 12 for v in struct.unpack_from('<3i', raw, 116)]
    assert stream == 128 and cache == stream + keys * 10
    width = 2 if keys <= 65536 else 4
    assert hashes == cache + joints * 12 * width
    assert len(raw) == hashes + joints * 4
    channels = {(joint, channel): [] for joint in range(joints) for channel in range(3)}
    for i in range(keys):
        time, tag, data = struct.unpack_from('<HH6s', raw, stream + i*10)
        joint, channel = tag & 0x3fff, tag >> 14
        assert joint < joints and channel < 3
        if channel == 0:
            bits = int.from_bytes(data, 'little')
            omitted = bits >> 45 & 3
            components = [((bits >> shift) & 32767) * math.sqrt(2) / 32767 - 1 / math.sqrt(2)
                          for shift in (30, 15, 0)]
            components.insert(omitted, math.sqrt(max(0, 1-sum(v*v for v in components))))
            value = tuple(components)
        else:
            lo, hi = (tmin, tmax) if channel == 1 else (smin, smax)
            value = tuple(a + (b-a)*q/65535 for a,b,q in zip(lo,hi,struct.unpack('<3H',data)))
        channels[joint, channel].append((time, value))
    for joint in range(joints):
        for channel in range(3):
            for slot in range(4):
                offset = cache + ((joint*3+channel)*4+slot)*width
                key = int.from_bytes(raw[offset:offset+width], 'little')
                assert key < keys
                tag = struct.unpack_from('<H', raw, stream+key*10+2)[0]
                assert tag == joint | channel << 14
    assert all(all(a[0] <= b[0] for a,b in zip(v,v[1:])) for v in channels.values())
    return duration, fps, channels


def evaluate(channel, time, rotation):
    times = [k[0] for k in channel]
    i = max(0, bisect.bisect_right(times, time) - 1)
    indices = [max(0, i-1), i, min(i+1,len(channel)-1), min(i+2,len(channel)-1)]
    points = [channel[k] for k in indices]
    if points[1][0] == points[2][0]:
        return points[1][1]
    u = (time-points[1][0])/(points[2][0]-points[1][0])
    p = [list(v) for _,v in points]
    if rotation:
        for j in range(1,4):
            if sum(a*b for a,b in zip(p[0],p[j])) < 0:
                p[j] = [-x for x in p[j]]
    span = points[2][0]-points[1][0]
    u = (time-points[1][0])/(span+1e-6)
    ease_in = span/(points[2][0]-points[0][0]+1e-6)
    ease_out = span/(points[3][0]-points[1][0]+1e-6)
    # Hermite form, independently organized from the Rust evaluator's cubic weights.
    h00, h10, h01, h11 = 2*u**3-3*u*u+1, u**3-2*u*u+u, -2*u**3+3*u*u, u**3-u*u
    value = tuple(h00*b+h10*ease_in*(c-a)+h01*c+h11*ease_out*(d-b)
                  for a,b,c,d in zip(*p))
    return normalized(value) if rotation else value


def test_independent_decoder_and_motion_accuracy():
    source = make_clip()
    raw = source.to_compressed_bytes()
    duration, fps, channels = decode_channels(raw)
    for j, track in enumerate(source.tracks):
        for i, frame in enumerate(track.frames):
            seconds = f32(i*f32(1/fps))
            time = int(max(0,min(65535,f32(f32(seconds/duration)*65535))))
            rotation = evaluate(channels[j,0], time, True)
            translation = evaluate(channels[j,1], time, False)
            scale = evaluate(channels[j,2], time, False)
            assert angular(rotation,frame.rotation) < .001001
            assert max(abs(a-b) for a,b in zip(translation,frame.translation)) < .050001
            assert max(abs(a-b) for a,b in zip(scale,frame.scale)) < .001001
    assert len(raw) < len(source.to_bytes()) / 2
    assert raw == source.to_compressed_bytes()


def test_independent_subframe_motion_accuracy():
    source = make_clip(31)
    duration, fps, channels = decode_channels(source.to_compressed_bytes())
    for j, track in enumerate(source.tracks):
        for index in range((len(track.frames)-1)*4+1):
            i, fraction = divmod(index,4)
            a, b = track.frames[i], track.frames[min(i+1,len(track.frames)-1)]
            u = fraction/4
            qa, qb = normalized(a.rotation), normalized(b.rotation)
            dot = sum(x*y for x,y in zip(qa,qb))
            if dot < 0: qb=tuple(-x for x in qb); dot=-dot
            angle = math.acos(min(1,dot))
            wa, wb = (1-u,u) if angle < 1e-6 else (math.sin((1-u)*angle)/math.sin(angle),math.sin(u*angle)/math.sin(angle))
            reference = normalized(tuple(wa*x+wb*y for x,y in zip(qa,qb)))
            seconds=f32(index*f32(1/f32(fps*4)))
            time=int(max(0,min(65535,f32(f32(seconds/duration)*65535))))
            assert angular(evaluate(channels[j,0],time,True),reference)<.001001
            for channel,left,right,limit in [(1,a.translation,b.translation,.050001),(2,a.scale,b.scale,.001001)]:
                expected=tuple(x+(y-x)*u for x,y in zip(left,right))
                actual=evaluate(channels[j,channel],time,False)
                assert max(abs(x-y) for x,y in zip(expected,actual))<limit


def test_explicit_compression_keeps_normal_save_contract(tmp_path):
    raw = make_clip().to_bytes()
    source = rs.Anm.from_bytes(raw)
    path = tmp_path/'motion.anm'
    source.to_compressed_path(str(path))
    assert source.is_byte_exact and source.to_bytes() == raw
    assert path.read_bytes() == source.to_compressed_bytes()
    compressed = rs.Anm.from_path(str(path))
    assert compressed.to_bytes() == path.read_bytes()
    compressed.make_editable()
    assert compressed.to_bytes()[:12] == b'r3d2anmd\x04\0\0\0'


@pytest.mark.parametrize('option', ['rotation_error','translation_error','scale_error'])
@pytest.mark.parametrize('value', [-1, math.inf, math.nan])
def test_invalid_accuracy_leaves_destination_unchanged(tmp_path, option, value):
    path = tmp_path/'existing.anm'
    path.write_bytes(b'keep this')
    with pytest.raises(rs.WriteError):
        make_clip().to_compressed_path(str(path), **{option:value})
    assert path.read_bytes() == b'keep this'


def test_custom_tolerances_and_quantization_floor():
    source = make_clip()
    fine = source.to_compressed_bytes(rotation_error=.0002, translation_error=.005, scale_error=.0001)
    coarse = source.to_compressed_bytes(rotation_error=.02, translation_error=.5, scale_error=.02)
    assert len(fine) >= len(coarse)
    with pytest.raises(rs.WriteError, match='tolerance'):
        source.to_compressed_bytes(rotation_error=0)


def test_single_frame_constant_reconstructs_instead_of_defaults():
    frame = rs.AnimFrame(0, (0,0,0,1), (25,-10,50), (2,3,4))
    source = rs.Anm.new(30,[rs.AnimTrack(1,[frame])])
    back = rs.Anm.from_bytes(source.to_compressed_bytes())
    assert back.frame_count == 1
    assert back.tracks[0].frames[0].translation == pytest.approx(frame.translation)
    assert back.tracks[0].frames[0].scale == pytest.approx(frame.scale)
