use rs_anim::{AnimFrame, AnimTrack, Animation, Joint, Skeleton};
use rs_io::{Parse, Serialize};
use rs_math::{Quat, Vec3};

fn sample_joint(name: &str, id: i16, parent: i16, hash: u32) -> Joint {
    Joint {
        name: name.to_string(),
        flags: 0,
        id,
        parent_id: parent,
        radius: 2.5,
        hash,
        local_translation: Vec3::new(1.0, 2.0, 3.0),
        local_scale: Vec3::new(1.0, 1.0, 1.0),
        local_rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 1.0),
        inverse_bind_translation: Vec3::new(-1.0, -2.0, -3.0),
        inverse_bind_scale: Vec3::new(1.0, 1.0, 1.0),
        inverse_bind_rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 1.0),
    }
}

#[test]
fn skeleton_round_trip() {
    let mut skl = Skeleton::new();
    skl.flags = 0;
    skl.joints = vec![
        sample_joint("root", 0, -1, 0x1111_2222),
        sample_joint("child", 1, 0, 0x3333_4444),
    ];
    skl.influences = vec![0, 1];

    let bytes = skl.to_bytes().expect("write skl");
    let parsed = Skeleton::from_bytes(&bytes).expect("read skl");
    assert_eq!(skl, parsed);

    let bytes2 = parsed.to_bytes().expect("rewrite skl");
    assert_eq!(bytes, bytes2, "skl write is deterministic / byte-exact");
}

#[test]
fn skeleton_legacy_is_unsupported() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2sklt");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    let err = Skeleton::from_bytes(&bytes).unwrap_err();
    assert!(matches!(err, rs_anim::Error::UnsupportedVersion(2)));
}

#[test]
fn animation_round_trip() {
    let mut anim = Animation::new(30.0);
    anim.tracks = vec![
        AnimTrack {
            joint_hash: 0xAABB_CCDD,
            frames: vec![
                AnimFrame::new(
                    0.0,
                    Quat::from_xyzw(0.0, 0.0, 0.0, 1.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ),
                AnimFrame::new(
                    1.0 / 30.0,
                    Quat::from_xyzw(0.5, 0.5, 0.5, 0.5),
                    Vec3::new(1.0, 2.0, 3.0),
                    Vec3::new(2.0, 2.0, 2.0),
                ),
            ],
        },
        AnimTrack {
            joint_hash: 0x1234_5678,
            frames: vec![
                AnimFrame::new(
                    0.0,
                    Quat::from_xyzw(0.0, 0.0, 0.0, 1.0),
                    Vec3::new(5.0, 5.0, 5.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ),
                AnimFrame::new(
                    1.0 / 30.0,
                    Quat::from_xyzw(0.0, 1.0, 0.0, 0.0),
                    Vec3::new(6.0, 7.0, 8.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ),
            ],
        },
    ];

    let bytes = anim.to_bytes().expect("write anm");
    let parsed = Animation::from_bytes(&bytes).expect("read anm");

    // The on-disk lossless quantity is the frame duration (1/fps); fps itself is recovered as
    // 1/(1/fps), so allow a small tolerance on the derived value.
    assert!((anim.fps - parsed.fps).abs() < 1e-3, "fps preserved within tolerance");
    assert_eq!(anim.tracks.len(), parsed.tracks.len());
    for original in &anim.tracks {
        let got = parsed
            .tracks
            .iter()
            .find(|t| t.joint_hash == original.joint_hash)
            .expect("track present after round-trip");
        assert_eq!(original, got);
    }

    let bytes2 = parsed.to_bytes().expect("rewrite anm");
    assert_eq!(bytes, bytes2, "anm write is deterministic");
}

#[test]
fn compressed_animation_is_unsupported() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2canm");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 64]);
    let err = Animation::from_bytes(&bytes).unwrap_err();
    assert!(matches!(err, rs_anim::Error::Unsupported("compressed anm")));
}
