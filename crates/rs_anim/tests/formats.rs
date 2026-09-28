use rs_anim::{AnimFrame, AnimTrack, Animation, Skeleton};
use rs_io::{Parse, Serialize};
use rs_math::{Mat4, Quat, Vec3};

fn legacy(version: u32, parent: i32, influences: &[u32]) -> Vec<u8> {
    let mut bytes = b"r3d2sklt".to_vec();
    for value in [version, 123, 2] {
        bytes.extend(value.to_le_bytes());
    }
    let root = Mat4::from_scale_rotation_translation(
        Vec3::splat(2.0),
        Quat::from_rotation_z(0.5),
        Vec3::new(3.0, 4.0, 5.0),
    );
    let child = root * Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
    for (name, parent, transform) in [("Root", -1i32, root), ("Child", parent, child)] {
        let mut padded = [0; 32];
        padded[..name.len()].copy_from_slice(name.as_bytes());
        bytes.extend(padded);
        bytes.extend(parent.to_le_bytes());
        bytes.extend(2.5f32.to_le_bytes());
        let columns = transform.to_cols_array_2d();
        for row in 0..3 {
            for column in columns {
                bytes.extend(column[row].to_le_bytes());
            }
        }
    }
    if version == 2 {
        bytes.extend((influences.len() as u32).to_le_bytes());
        for influence in influences {
            bytes.extend(influence.to_le_bytes());
        }
    }
    bytes
}

#[test]
fn legacy_skeleton_versions_decode_and_preserve_bytes() {
    for version in [1, 2] {
        let bytes = legacy(version, 0, &[1, 0]);
        let skeleton = Skeleton::from_bytes(&bytes).unwrap();
        assert_eq!(skeleton.joints[0].hash, rs_hash::elf_lower("Root"));
        assert_eq!(skeleton.joints[1].parent_id, 0);
        assert!(
            skeleton.joints[1]
                .local_translation
                .abs_diff_eq(Vec3::new(1.0, 2.0, 3.0), 1e-5)
        );
        let global = skeleton.joints[0].local_transform() * skeleton.joints[1].local_transform();
        assert!(
            (global * skeleton.joints[1].inverse_bind_transform())
                .abs_diff_eq(Mat4::IDENTITY, 1e-5)
        );
        assert_eq!(
            skeleton.influences,
            if version == 1 { vec![0, 1] } else { vec![1, 0] }
        );
        assert_eq!(skeleton.to_bytes().unwrap(), bytes);
    }
}

#[test]
fn legacy_edit_writes_modern_skeleton() {
    let mut skeleton = Skeleton::from_bytes(&legacy(2, 0, &[])).unwrap();
    assert!(skeleton.influences.is_empty());
    skeleton.joints[1].local_translation = Vec3::new(7.0, 8.0, 9.0);
    let bytes = skeleton.to_bytes().unwrap();
    assert_eq!(&bytes[4..8], &Skeleton::MAGIC.to_le_bytes());
    assert_eq!(Skeleton::from_bytes(&bytes).unwrap(), skeleton);
}

#[test]
fn legacy_rejects_bad_hierarchy_influences_and_truncation() {
    for parent in [-2, 1, 32768] {
        assert!(Skeleton::from_bytes(&legacy(2, parent, &[])).is_err());
    }
    assert!(Skeleton::from_bytes(&legacy(2, 0, &[2])).is_err());
    let bytes = legacy(2, 0, &[0, 1]);
    for end in 0..bytes.len() {
        assert!(
            Skeleton::from_bytes(&bytes[..end]).is_err(),
            "accepted {end} bytes"
        );
    }
}

#[test]
fn modern_preserves_reserved_fields_and_detects_edits() {
    let mut skeleton = Skeleton::from_bytes(&legacy(1, 0, &[])).unwrap();
    skeleton.name = "modern".into();
    let mut bytes = skeleton.to_bytes().unwrap();
    bytes[44..48].copy_from_slice(&123u32.to_le_bytes());
    bytes.extend([12, 34, 56]);
    let mut parsed = Skeleton::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
    parsed.name = "edited".into();
    let edited = parsed.to_bytes().unwrap();
    assert_eq!(Skeleton::from_bytes(&edited).unwrap().name, "edited");
}

fn palette_animation(count: usize, rotations: bool) -> Animation {
    let mut animation = Animation::new(30.0);
    animation.tracks.push(AnimTrack {
        joint_hash: 123,
        frames: (0..count)
            .map(|i| {
                let value = i as f32;
                AnimFrame::new(
                    i as f32 / 30.0,
                    if rotations {
                        Quat::from_xyzw(value, 0.0, 0.0, 1.0)
                    } else {
                        Quat::IDENTITY
                    },
                    if rotations {
                        Vec3::ONE
                    } else {
                        Vec3::new(value, 0.0, 0.0)
                    },
                    if rotations { Vec3::ONE } else { Vec3::ZERO },
                )
            })
            .collect(),
    });
    animation
}

#[test]
fn palettes_accept_65536_entries_and_reject_65537() {
    for rotations in [false, true] {
        let animation = palette_animation(65536, rotations);
        let parsed = Animation::from_bytes(&animation.to_bytes().unwrap()).unwrap();
        assert_eq!(parsed.frame_count(), 65536);
        let expected = animation.tracks[0].frames.last().unwrap();
        let actual = parsed.tracks[0].frames.last().unwrap();
        assert_eq!(actual.translation, expected.translation);
        assert_eq!(actual.rotation, expected.rotation);
        assert!(palette_animation(65537, rotations).to_bytes().is_err());
    }
}

#[test]
fn frame_count_is_not_limited_to_u16() {
    let mut animation = Animation::new(30.0);
    animation.tracks.push(AnimTrack {
        joint_hash: 42,
        frames: vec![AnimFrame::new(0.0, Quat::IDENTITY, Vec3::ZERO, Vec3::ONE); 70000],
    });
    let parsed = Animation::from_bytes(&animation.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.frame_count(), 70000);
}

fn v5() -> Vec<u8> {
    let mut bytes = b"r3d2anmd".to_vec();
    for value in [5u32, 0, 0, 0, 0, 2, 2] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0.5f32.to_le_bytes());
    for offset in [106i32, 0, 0, 64, 100, 110] {
        bytes.extend(offset.to_le_bytes());
    }
    bytes.extend([0; 12]);
    for vector in [Vec3::ZERO, Vec3::ONE, Vec3::splat(7.0)] {
        for component in vector.to_array() {
            bytes.extend(component.to_le_bytes());
        }
    }
    bytes.extend(rs_anim::quantized::compress_quat(Quat::IDENTITY));
    bytes.extend(123u32.to_le_bytes());
    for row in [[0u16, 1, 0], [1, 1, 0], [2, 1, 0], [0, 1, 0]] {
        for index in row {
            bytes.extend(index.to_le_bytes());
        }
    }
    bytes
}

#[test]
fn v5_consumes_unnamed_tracks_without_shifting_next_frame() {
    let bytes = v5();
    let animation = Animation::from_bytes(&bytes).unwrap();
    assert_eq!(animation.tracks.len(), 1);
    assert_eq!(animation.tracks[0].frames[0].translation, Vec3::ZERO);
    assert_eq!(animation.tracks[0].frames[1].translation, Vec3::splat(7.0));
    assert_eq!(animation.to_bytes().unwrap(), bytes);
}

#[test]
fn oversized_counts_and_reversed_sections_return_errors() {
    let mut bytes = v5();
    bytes[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Animation::from_bytes(&bytes).is_err());
    let mut bytes = v5();
    bytes[56..60].copy_from_slice(&1i32.to_le_bytes());
    assert!(Animation::from_bytes(&bytes).is_err());
}

#[test]
fn skeleton_joint_ids_are_independent_of_influence_slots() {
    let source = Skeleton::from_bytes(&legacy(1, 0, &[])).unwrap();
    let mut skeleton = Skeleton::new();
    skeleton.joints = (0..32768)
        .map(|index| {
            let mut joint = source.joints[0].clone();
            joint.id = index as i16;
            joint
        })
        .collect();
    for influences in [vec![], vec![32767]] {
        skeleton.influences = influences;
        let parsed = Skeleton::from_bytes(&skeleton.to_bytes().unwrap()).unwrap();
        assert_eq!(parsed.joints.len(), 32768);
        assert_eq!(parsed.influences, skeleton.influences);
    }
    skeleton.joints.push(source.joints[0].clone());
    assert!(skeleton.to_bytes().is_err());
}
