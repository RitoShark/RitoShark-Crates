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
fn skeleton_joint_index_section_sorted_by_hash_ascending() {
    // Joints whose hashes are deliberately out of id order. The joint-id-hash section must be
    // emitted ordered by hash ascending (matching the C# RigResource writer), independent of the
    // joint declaration order, and the file must round-trip byte-exactly.
    let mut skl = Skeleton::new();
    skl.joints = vec![
        sample_joint("a", 0, -1, 0x3000_0000),
        sample_joint("b", 1, 0, 0x1000_0000),
        sample_joint("c", 2, 0, 0x2000_0000),
    ];
    skl.influences = vec![0, 1, 2];

    let bytes = skl.to_bytes().expect("write skl");

    // Locate the joint-id-hash section: header(64) + joints(3*100) = 364.
    let section = &bytes[364..364 + 3 * 8];
    let hashes: Vec<u32> = (0..3)
        .map(|i| u32::from_le_bytes(section[i * 8 + 4..i * 8 + 8].try_into().unwrap()))
        .collect();
    assert_eq!(
        hashes,
        vec![0x1000_0000, 0x2000_0000, 0x3000_0000],
        "joint-id-hash section must be ascending by hash"
    );

    let parsed = Skeleton::from_bytes(&bytes).expect("read skl");
    assert_eq!(skl, parsed);
    let bytes2 = parsed.to_bytes().expect("rewrite skl");
    assert_eq!(bytes, bytes2, "skl round-trip is byte-exact");
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
    assert!(
        (anim.fps - parsed.fps).abs() < 1e-3,
        "fps preserved within tolerance"
    );
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
fn compressed_animation_unknown_version_is_unsupported() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2canm");
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 64]);
    let err = Animation::from_bytes(&bytes).unwrap_err();
    assert!(matches!(err, rs_anim::Error::UnsupportedVersion(7)));
}

/// A hand-built minimal `r3d2canm` (version 3): one joint, three sparse keyframes (rotation at
/// frame 0, scale at frame 1, translation at frame 2). The reader must decode the header,
/// dequantize each component, and evaluate three explicit output frames per joint.
const SYNTHETIC_CANM: &[u8] = &[
    0x72, 0x33, 0x64, 0x32, 0x63, 0x61, 0x6e, 0x6d, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40,
    0x00, 0x00, 0x20, 0x41, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x20, 0x41, 0x00, 0x00, 0x00, 0x40,
    0x00, 0x00, 0x20, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40,
    0x00, 0x00, 0x00, 0x40, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x74, 0x00, 0x00, 0x00,
    0xdd, 0xcc, 0xbb, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x20, 0x00, 0x70, 0xff, 0xff,
    0x00, 0x40, 0xff, 0x7f, 0xff, 0x7f, 0xff, 0x7f, 0xff, 0x7f, 0x00, 0x80, 0xff, 0x7f, 0xff, 0x7f,
    0xff, 0x7f,
];

#[test]
fn compressed_animation_parses() {
    let anim = Animation::from_bytes(SYNTHETIC_CANM).expect("read synthetic compressed anm");

    assert_eq!(anim.fps, 2.0);
    assert_eq!(anim.tracks().len(), 1, "one joint expected");
    let track = &anim.tracks()[0];
    assert_eq!(track.joint_hash, 0xAABB_CCDD);

    // max_time(1.0) * fps(2.0) = 2 -> 3 output frames (0, 1, 2).
    assert_eq!(track.frames.len(), 3, "three evaluated frames expected");

    // Frame 0 holds the rotation key (identity quaternion).
    let r = track.frames[0].rotation;
    assert!(
        (r.w.abs() - 1.0).abs() < 1e-3,
        "frame 0 rotation should be ~identity, got {r:?}"
    );

    // Translation key sits at frame 2 with all channels at midpoint of [0, 2] => ~1.0.
    let t = track.frames[2].translation;
    assert!(
        (t.x - 1.0).abs() < 0.05 && (t.y - 1.0).abs() < 0.05 && (t.z - 1.0).abs() < 0.05,
        "frame 2 translation should be ~(1,1,1), got {t:?}"
    );

    // Scale key at frame 1, midpoint of [0, 2] => ~1.0 on every channel.
    let s = track.frames[1].scale;
    assert!(
        (s.x - 1.0).abs() < 0.05 && (s.y - 1.0).abs() < 0.05 && (s.z - 1.0).abs() < 0.05,
        "frame 1 scale should be ~(1,1,1), got {s:?}"
    );
}

/// Builds a one-joint `r3d2canm` (version 3) from a list of `(compressed_time, transform_type,
/// value[6])` records, mirroring the C# CompressedAnimationAsset on-disk layout: a 12-byte common
/// header, then joint/frame/jump-cache counts, duration + fps, three error metrics, translation and
/// scale min/max, and the `(frames, jumpCaches, jointHashes)` offsets, all relative to byte 12.
fn build_canm(
    joint_hash: u32,
    max_time: f32,
    fps: f32,
    tr_bounds: (Vec3, Vec3),
    sc_bounds: (Vec3, Vec3),
    records: &[(u16, u16, [u8; 6])],
) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(b"r3d2canm");
    header.extend_from_slice(&3u32.to_le_bytes()); // version
    header.extend_from_slice(&0u32.to_le_bytes()); // resource size
    header.extend_from_slice(&0u32.to_le_bytes()); // format token
    header.extend_from_slice(&0u32.to_le_bytes()); // flags
    header.extend_from_slice(&1i32.to_le_bytes()); // joint count
    header.extend_from_slice(&(records.len() as i32).to_le_bytes()); // frame count
    header.extend_from_slice(&1i32.to_le_bytes()); // jump cache count
    header.extend_from_slice(&max_time.to_le_bytes());
    header.extend_from_slice(&fps.to_le_bytes());
    for _ in 0..6 {
        header.extend_from_slice(&0f32.to_le_bytes()); // error metrics
    }
    for v in [tr_bounds.0, tr_bounds.1, sc_bounds.0, sc_bounds.1] {
        header.extend_from_slice(&v.x.to_le_bytes());
        header.extend_from_slice(&v.y.to_le_bytes());
        header.extend_from_slice(&v.z.to_le_bytes());
    }

    // Offsets are relative to byte 12. The fields written above occupy bytes 0..116, then the
    // 3-entry offset table occupies 116..128, so the frame stream begins at absolute 128 => 116
    // relative.
    let frames_rel = 116i32;
    let frames_len = records.len() as i32 * 10;
    let jump_rel = frames_rel + frames_len;
    let jump_len = 24i32; // one jump cache, one joint, u16 frame ids
    let joints_rel = jump_rel + jump_len;
    header.extend_from_slice(&frames_rel.to_le_bytes());
    header.extend_from_slice(&jump_rel.to_le_bytes());
    header.extend_from_slice(&joints_rel.to_le_bytes());

    for &(time, bits, value) in records {
        header.extend_from_slice(&time.to_le_bytes());
        header.extend_from_slice(&bits.to_le_bytes());
        header.extend_from_slice(&value);
    }
    header.extend_from_slice(&[0u8; 24]); // jump cache (ignored by the reader)
    header.extend_from_slice(&joint_hash.to_le_bytes());
    header
}

#[test]
fn compressed_animation_recovers_known_rotation() {
    use rs_anim::quantized::compress_quat;

    // A non-identity rotation, round-tripped through the 48-bit codec.
    let rotation = Quat::from_xyzw(0.5, 0.5, 0.5, 0.5);
    let quant = compress_quat(rotation);

    let records = [(0u16, 0u16, quant)]; // time 0, transform type 0 (rotation), joint 0
    let buf = build_canm(
        0x1234_5678,
        1.0,
        1.0,
        (Vec3::ZERO, Vec3::ONE),
        (Vec3::ZERO, Vec3::ONE),
        &records,
    );

    let anim = Animation::from_bytes(&buf).expect("read built compressed anm");
    assert_eq!(anim.tracks().len(), 1);
    let track = &anim.tracks()[0];
    assert_eq!(track.joint_hash, 0x1234_5678);
    assert!(!track.frames.is_empty());

    // Every frame holds the single rotation key. Compare against the quantized round-trip of the
    // input (the codec is lossy, so we compare to the dequantized expectation, not the raw input).
    let got = track.frames[0].rotation;
    let expected = rs_anim::quantized::decompress_quat(&quant).normalize();
    let dot =
        (got.x * expected.x + got.y * expected.y + got.z * expected.z + got.w * expected.w).abs();
    assert!(
        dot > 0.9999,
        "rotation key not recovered: got {got:?} expected {expected:?}"
    );
}
