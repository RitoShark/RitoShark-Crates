use super::*;
use crate::{AnimFrame, AnimTrack};

fn moving() -> Animation {
    let mut a = Animation::new(30.);
    a.tracks.push(AnimTrack {
        joint_hash: 7,
        frames: vec![
            AnimFrame::new(0., Quat::IDENTITY, Vec3::ZERO, Vec3::ONE),
            AnimFrame::new(
                1. / 30.,
                Quat::from_rotation_x(0.5),
                Vec3::X * 10.,
                Vec3::splat(1.5),
            ),
        ],
    });
    a
}

#[test]
fn one_channel_cache_preserves_midpoint_values_and_endpoints() {
    let a = moving();
    let src = Source::new(&a);
    let oracle = Oracle::new(&src);
    for c in 0..3 {
        let expected: Vec<_> = [0, 1, 123, 32768, 65534, 65535]
            .iter()
            .map(|&t| oracle.sample(c, t))
            .collect();
        oracle.prepare_channel(c);
        for (&t, value) in [0, 1, 123, 32768, 65534, 65535].iter().zip(expected) {
            assert_eq!(oracle.sample(c, t), value);
            assert_eq!(oracle.sample(c, t), value);
        }
        assert_eq!(oracle.sample(c, 0), src.sample_dense_seconds(c, 0.));
        assert_eq!(
            oracle.sample(c, 65535),
            src.sample_dense_seconds(c, src.duration)
        );
        assert_eq!(
            oracle.cache.borrow().as_ref().unwrap().1.len() * std::mem::size_of::<Vec4>(),
            1_048_576
        );
    }
}

#[test]
fn neutral_scale_alignment_preserves_range_and_exact_one() {
    let mut b = [Vec3::ZERO, Vec3::ONE, Vec3::splat(0.05), Vec3::splat(8.7)];
    let before = b;
    align_neutral_scale(&mut b);
    assert!(b[2].cmple(before[2]).all());
    assert!(b[3].cmpge(before[3]).all());
    let bytes = pack_vector(Vec3::ONE, b[2], b[3]);
    assert_eq!(decompress_vec3(b[2], b[3], &bytes), Vec3::ONE);
}

#[test]
fn final_decoder_rejects_valid_stream_with_wrong_motion() {
    let a = moving();
    let mut src = Source::new(&a);
    let oracle = Oracle::new(&src);
    src.bounds = oracle.bounds();
    let channels: Vec<_> = (0..3)
        .map(|c| Channel {
            joint: 0,
            kind: kind(c),
            keys: vec![packed(&src, &oracle, c, 0)],
        })
        .collect();
    let raw = src.emit(&channels).unwrap();
    assert!(matches!(
        validate_canm(
            &src,
            &raw,
            &AnimationExportOptions::new(crate::AnimationFormat::Canm),
            &Skeleton::new(),
            false
        ),
        Err(Error::CompressionTolerance { .. })
    ));
}

#[test]
fn serializer_uses_wide_seek_indices_and_seeks_both_directions() {
    let mut channels = Vec::new();
    let hashes = [1, 2, 3, 4];
    let bounds = [Vec3::ZERO, Vec3::ONE, Vec3::ONE, Vec3::ONE];
    for joint in 0..4 {
        channels.push(Channel {
            joint,
            kind: ChannelKind::Rotation,
            keys: vec![Key {
                time: 0,
                bytes: compress_quat(Quat::IDENTITY),
            }],
        });
        let mut keys: Vec<_> = (0..16385u32)
            .map(|i| {
                let time = (i * 65535 / 16384) as u16;
                Key {
                    time,
                    bytes: pack_vector(Vec3::splat(time as f32 / 65535.), bounds[0], bounds[1]),
                }
            })
            .collect();
        keys.push(*keys.last().unwrap());
        channels.push(Channel {
            joint,
            kind: ChannelKind::Translation,
            keys,
        });
        channels.push(Channel {
            joint,
            kind: ChannelKind::Scale,
            keys: vec![Key {
                time: 0,
                bytes: pack_vector(Vec3::ONE, Vec3::ONE, Vec3::ONE),
            }],
        });
    }
    let raw = serialize_channels(&channels, &hashes, 1., 30., 31, bounds).unwrap();
    let u = |p| u32::from_le_bytes(raw[p..p + 4].try_into().unwrap()) as usize;
    assert!(u(28) > 65536);
    assert_eq!(u(124) - u(120), u(32) * hashes.len() * 48);
    let mut reader = Cursor::new(&raw);
    reader.set_position(12);
    let decoder = crate::compressed::Compressed::from_reader(&mut reader).unwrap();
    decoder
        .visit_samples([0., 0.93, 0.2, 1., 0.5].into_iter(), |time, values| {
            for v in values {
                assert!((v[1].truncate() - Vec3::splat(time)).abs().max_element() < 0.0001);
            }
            Ok(())
        })
        .unwrap();
}
