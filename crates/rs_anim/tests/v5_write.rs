use rs_anim::{AnimFrame, AnimTrack, Animation};
use rs_io::{Parse, Serialize};
use rs_math::{Quat, Vec3};

fn clip(count: usize) -> Animation {
    let mut a = Animation::new(29.97);
    a.tracks.push(AnimTrack {
        joint_hash: 0xf1234567,
        frames: (0..count)
            .map(|i| {
                AnimFrame::new(
                    i as f32 / a.fps,
                    Quat::from_rotation_z(i as f32 * 0.17),
                    Vec3::new(i as f32, -0.0, 38156.7),
                    Vec3::new(1.0, 1.5, 0.8),
                )
            })
            .collect(),
    });
    a
}
fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

#[test]
fn v5_layout_motion_and_source_preservation() {
    let a = clip(301);
    let bytes = a.to_v5_bytes().unwrap();
    assert_eq!(&bytes[..8], b"r3d2anmd");
    assert_eq!(u32_at(&bytes, 8), 5);
    assert_eq!(u32_at(&bytes, 12), bytes.len() - 12);
    assert_eq!(u32_at(&bytes, 52) + 12, 76);
    let hashes = u32_at(&bytes, 40) + 12;
    let rows = u32_at(&bytes, 60) + 12;
    assert_eq!(rows, hashes + 4);
    assert_eq!(bytes.len() - rows, 301 * 6);
    let mut parsed = Animation::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    assert_eq!(parsed.to_v5_bytes().unwrap(), bytes);
    assert!((parsed.fps - a.fps).abs() < 1e-5); // v5 stores frame duration, not fps.
    for (x, y) in a.tracks[0].frames.iter().zip(&parsed.tracks[0].frames) {
        assert_eq!(
            x.translation.to_array().map(f32::to_bits),
            y.translation.to_array().map(f32::to_bits)
        );
        assert_eq!(x.scale, y.scale);
        let d = (x.rotation - y.rotation)
            .length()
            .min((x.rotation + y.rotation).length());
        assert!(d < 0.0001);
    }
    parsed.tracks[0].frames[0].translation.x = 42.0;
    let changed = parsed.to_v5_bytes().unwrap();
    assert_ne!(changed, bytes);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    parsed.make_editable();
    assert_eq!(u32_at(&parsed.to_bytes().unwrap(), 8), 4);
    assert!(bytes.len() < a.to_bytes().unwrap().len());
}

#[test]
fn quaternion_signs_and_nonunit_input_share_packed_values() {
    let mut a = clip(3);
    let q = Quat::from_rotation_y(0.7);
    for (frame, r) in a.tracks[0].frames.iter_mut().zip([q, -q, q * 2.0]) {
        frame.rotation = r;
    }
    let bytes = a.to_v5_bytes().unwrap();
    assert_eq!(u32_at(&bytes, 40) - u32_at(&bytes, 56), 6);
    let output = Animation::from_bytes(&bytes).unwrap();
    assert!(
        output.tracks[0]
            .frames
            .iter()
            .all(|f| (f.rotation.length() - 1.0).abs() < 1e-6)
    );
}

#[test]
fn exact_u16_palette_boundary_does_not_wrap() {
    let mut a = clip(65535); // unique positions plus one unique scale = 65536
    for f in &mut a.tracks[0].frames {
        f.rotation = Quat::IDENTITY;
    }
    let bytes = a.to_v5_bytes().unwrap();
    assert_eq!((u32_at(&bytes, 56) - u32_at(&bytes, 52)) / 12, 65536);
    let out = Animation::from_bytes(&bytes).unwrap();
    assert_eq!(out.tracks[0].frames.last().unwrap().translation.x, 65534.0);
    let mut last = *a.tracks[0].frames.last().unwrap();
    last.time = 65535.0 / a.fps;
    last.translation.x = 65535.0;
    a.tracks[0].frames.push(last);
    assert!(a.to_v5_bytes().unwrap_err().to_string().contains("65536"));
}

#[test]
fn invalid_samples_fail_before_touching_output() {
    let a = clip(2);
    for kind in 0..8 {
        let mut bad = a.clone();
        match kind {
            0 => bad.fps = 0.0,
            1 => bad.tracks.clear(),
            2 => bad.tracks[0].frames.clear(),
            3 => bad.tracks.push(bad.tracks[0].clone()),
            4 => bad.tracks[0].frames[1].time = 99.0,
            5 => bad.tracks[0].frames[0].translation.x = f32::NAN,
            6 => bad.tracks[0].frames[0].rotation = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
            _ => {
                let mut t = bad.tracks[0].clone();
                t.joint_hash = 3;
                t.frames.pop();
                bad.tracks.push(t);
            }
        }
        let mut dest = vec![1, 2, 3];
        assert!(bad.write_v5(&mut dest).is_err());
        assert_eq!(dest, [1, 2, 3]);
    }
    let path = std::env::temp_dir().join(format!("v5-protected-{}.anm", std::process::id()));
    std::fs::write(&path, b"original").unwrap();
    assert!(Animation::new(30.0).to_v5_path(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    std::fs::remove_file(path).unwrap();
}
