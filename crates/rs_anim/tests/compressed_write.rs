use rs_anim::{AnimFrame, AnimTrack, Animation, CompressionOptions, Error};
use rs_io::{Parse, Serialize};
use rs_math::{Quat, Vec3};

fn clip(joints: usize, frames: usize) -> Animation {
    let mut a = Animation::new(30.0);
    for j in 0..joints {
        a.tracks.push(AnimTrack {
            joint_hash: j as u32 + 1,
            frames: (0..frames)
                .map(|i| {
                    let x = i as f32 / 30.0;
                    let phase = j as f32 * 0.17;
                    let q = Quat::from_rotation_x((x * 2.0 + phase).sin() * 0.8)
                        * Quat::from_rotation_y(x * 0.7);
                    AnimFrame::new(
                        x,
                        if i % 3 == 0 { -q } else { q },
                        Vec3::new((x + phase).sin() * 3.0, j as f32, x * 0.2),
                        Vec3::splat(1.0 + (x + phase).sin() * 0.05),
                    )
                })
                .collect(),
        });
    }
    a
}

fn angular(a: Quat, b: Quat) -> f64 {
    let a = a.normalize().to_array().map(f64::from);
    let b = b.normalize().to_array().map(f64::from);
    let dot: f64 = (0..4).map(|i| a[i] * b[i]).sum();
    let aa: f64 = a.iter().map(|x| x * x).sum();
    let bb: f64 = b.iter().map(|x| x * x).sum();
    2.0 * (dot.abs() / (aa * bb).sqrt()).clamp(0.0, 1.0).acos()
}

fn verify(a: &Animation, bytes: &[u8], options: &CompressionOptions) {
    assert_eq!(&bytes[..8], b"r3d2canm");
    assert_eq!(u32_at(bytes, 8), 1);
    assert_eq!(u32_at(bytes, 12) as usize, bytes.len() - 12);
    let b = Animation::from_bytes(bytes).unwrap();
    assert_eq!(b.fps, a.fps);
    assert_eq!(b.tracks.len(), a.tracks.len());
    assert_eq!(b.to_bytes().unwrap(), bytes);
    for (x, y) in a.tracks.iter().zip(&b.tracks) {
        assert_eq!(x.joint_hash, y.joint_hash);
        assert_eq!(x.frames.len(), y.frames.len());
        for (i, (f, g)) in x.frames.iter().zip(&y.frames).enumerate() {
            assert!((f.time - g.time).abs() < 1e-4);
            assert!(
                (f.translation - g.translation).abs().max_element() <= options.translation_error
            );
            assert!((f.scale - g.scale).abs().max_element() <= options.scale_error);
            assert!(
                angular(f.rotation, g.rotation) <= options.rotation_error as f64 + 1e-7,
                "joint {} sample {}",
                x.joint_hash,
                i
            );
        }
    }
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

#[test]
fn smooth_channels_reduce_keys_and_preserve_motion() {
    let a = clip(6, 301);
    let opts = CompressionOptions::default();
    let bytes = a.to_compressed_bytes(&opts).unwrap();
    verify(&a, &bytes, &opts);
    assert!(
        bytes.len() < a.to_bytes().unwrap().len() / 2,
        "CANM {} vs v4 {}",
        bytes.len(),
        a.to_bytes().unwrap().len()
    );
    assert_eq!(bytes, a.to_compressed_bytes(&opts).unwrap());
    // Every seeded frame resolves to the right joint and channel, with four valid keys.
    let frames = u32_at(&bytes, 116) as usize + 12;
    let cache = u32_at(&bytes, 120) as usize + 12;
    for joint in 0..6 {
        for channel in 0..3 {
            for slot in 0..4 {
                let pos = cache + joint * 24 + channel * 8 + slot * 2;
                let index = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
                let tag = u16::from_le_bytes(
                    bytes[frames + index * 10 + 2..frames + index * 10 + 4]
                        .try_into()
                        .unwrap(),
                );
                assert_eq!(tag, joint as u16 | ((channel as u16) << 14));
            }
        }
    }
}

#[test]
fn constants_single_sample_and_zero_spans() {
    for count in [1, 2, 180] {
        let mut a = clip(2, count);
        for track in &mut a.tracks {
            for f in &mut track.frames {
                f.rotation = Quat::IDENTITY;
                f.translation = Vec3::new(-25.0, 12.0, 0.0);
                f.scale = Vec3::new(1.0, 0.0, -1.0);
            }
        }
        let opts = CompressionOptions::default();
        let bytes = a.to_compressed_bytes(&opts).unwrap();
        verify(&a, &bytes, &opts);
        assert_eq!(u32_at(&bytes, 28), if count == 1 { 18 } else { 6 });
    }
}

#[test]
fn discontinuities_and_long_rotations_do_not_disappear() {
    let mut a = clip(1, 240);
    for (i, f) in a.tracks[0].frames.iter_mut().enumerate() {
        f.rotation = Quat::from_rotation_z(i as f32 * 0.07);
        f.translation.x = if i % 13 == 0 { 5.0 } else { 0.0 };
    }
    let opts = CompressionOptions::default();
    verify(&a, &a.to_compressed_bytes(&opts).unwrap(), &opts);
}

#[test]
fn normalization_and_negative_quaternion_hemispheres() {
    let mut a = clip(2, 70);
    for t in &mut a.tracks {
        for f in &mut t.frames {
            f.rotation = rs_math::Quat::from_vec4(rs_math::Vec4::from(f.rotation) * 2.5);
        }
    }
    let opts = CompressionOptions::default();
    verify(&a, &a.to_compressed_bytes(&opts).unwrap(), &opts);
}

#[test]
fn compression_is_explicit_and_does_not_change_passthrough() {
    let opts = CompressionOptions::default();
    let mut a = Animation::from_bytes(&clip(2, 45).to_bytes().unwrap()).unwrap();
    let original = a.to_bytes().unwrap();
    a.tracks[0].frames[4].translation.x = 10.0;
    let compressed = a.to_compressed_bytes(&opts).unwrap();
    verify(&a, &compressed, &opts);
    assert!(a.is_byte_exact());
    assert_eq!(a.to_bytes().unwrap(), original);
    a.make_editable();
    assert_eq!(&a.to_bytes().unwrap()[..8], b"r3d2anmd");
}

#[test]
fn rejects_invalid_inputs_and_unattainable_quantization_accuracy() {
    let a = clip(1, 4);
    let opts = CompressionOptions::default();
    let mut bad = a.clone();
    bad.fps = 0.0;
    assert!(bad.to_compressed_bytes(&opts).is_err());
    bad.fps = f32::NAN;
    assert!(bad.to_compressed_bytes(&opts).is_err());
    bad = a.clone();
    bad.tracks.clear();
    assert!(bad.to_compressed_bytes(&opts).is_err());
    bad = a.clone();
    bad.tracks[0].frames.clear();
    assert!(bad.to_compressed_bytes(&opts).is_err());
    bad = a.clone();
    bad.tracks.push(bad.tracks[0].clone());
    assert!(bad.to_compressed_bytes(&opts).is_err());
    bad.tracks[1].joint_hash = 999;
    bad.tracks[1].frames.pop();
    assert!(bad.to_compressed_bytes(&opts).is_err());
    bad = a.clone();
    bad.tracks[0].frames[1].time = 10.0;
    assert!(bad.to_compressed_bytes(&opts).is_err());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        bad = a.clone();
        bad.tracks[0].frames[0].translation.x = value;
        assert!(bad.to_compressed_bytes(&opts).is_err());
    }
    bad = a.clone();
    bad.tracks[0].frames[0].rotation = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    assert!(bad.to_compressed_bytes(&opts).is_err());
    for value in [-1.0, f32::NAN, f32::INFINITY] {
        let o = CompressionOptions {
            translation_error: value,
            ..opts
        };
        assert!(a.to_compressed_bytes(&o).is_err());
    }
    bad = a.clone();
    for (i, f) in bad.tracks[0].frames.iter_mut().enumerate() {
        f.translation = Vec3::splat([0.0, 1.0, 100000.0, 50000.0][i]);
    }
    assert!(matches!(
        bad.to_compressed_bytes(&opts),
        Err(Error::CompressionTolerance {
            channel: "translation",
            ..
        })
    ));
}

#[test]
fn encode_failure_preserves_destination() {
    let mut a = clip(1, 4);
    a.fps = -1.0;
    let mut destination = vec![1, 2, 3];
    assert!(
        a.write_compressed(&mut destination, &CompressionOptions::default())
            .is_err()
    );
    assert_eq!(destination, [1, 2, 3]);
    let path = std::env::temp_dir().join(format!("canm-protected-{}.anm", std::process::id()));
    std::fs::write(&path, b"original").unwrap();
    assert!(
        a.to_compressed_path(&path, &CompressionOptions::default())
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn wide_jump_cache_indices_and_maximum_joint_id() {
    let opts = CompressionOptions::default();
    let mut a = Animation::new(30.0);
    for j in 0..8192 {
        a.tracks.push(AnimTrack {
            joint_hash: j,
            frames: vec![
                AnimFrame::new(0.0, Quat::IDENTITY, Vec3::ZERO, Vec3::ONE),
                AnimFrame::new(
                    1.0 / 30.0,
                    Quat::from_rotation_x(1.0),
                    Vec3::ONE,
                    Vec3::splat(2.0),
                ),
            ],
        });
    }
    let bytes = a.to_compressed_bytes(&opts).unwrap();
    assert!(u32_at(&bytes, 28) > 65536);
    assert_eq!(u32_at(&bytes, 124) - u32_at(&bytes, 120), 8192 * 48);
    verify(&a, &bytes, &opts);
    for t in &mut a.tracks {
        t.frames.truncate(1);
    }
    let mut extra = a.tracks.clone();
    for t in &mut extra {
        t.joint_hash += 8192;
    }
    a.tracks.extend(extra);
    verify(&a, &a.to_compressed_bytes(&opts).unwrap(), &opts);
    a.tracks.push(AnimTrack {
        joint_hash: 16384,
        frames: a.tracks[0].frames.clone(),
    });
    assert!(a.to_compressed_bytes(&opts).is_err());
}

#[test]
fn fractional_fps_and_time_grid_limits() {
    let opts = CompressionOptions::default();
    let mut a = clip(1, 90);
    a.fps = 29.97;
    for (i, f) in a.tracks[0].frames.iter_mut().enumerate() {
        f.time = i as f32 / a.fps;
    }
    verify(&a, &a.to_compressed_bytes(&opts).unwrap(), &opts);
    let first = a.tracks[0].frames[0];
    a.tracks[0].frames.resize(65537, first);
    assert!(a.to_compressed_bytes(&opts).is_err());
}
