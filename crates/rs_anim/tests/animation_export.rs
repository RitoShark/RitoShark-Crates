use rs_anim::{
    AnimFrame, AnimTrack, Animation, AnimationExportOptions, AnimationFormat, Error, Joint,
    Skeleton,
};
use rs_io::{Parse, Serialize};
use rs_math::{Quat, Vec3};

fn clip(frames: usize) -> Animation {
    let mut a = Animation::new(30.);
    a.tracks.push(AnimTrack {
        joint_hash: 1,
        frames: (0..frames)
            .map(|i| {
                let t = i as f32 / 30.;
                AnimFrame::new(
                    t,
                    Quat::from_rotation_z(t * 0.4),
                    Vec3::new(t.sin() * 2., 2., 3.),
                    Vec3::ONE,
                )
            })
            .collect(),
    });
    a
}
fn skeleton() -> Skeleton {
    let mut s = Skeleton::new();
    s.joints = vec![Joint {
        name: "root".into(),
        id: 0,
        parent_id: -1,
        hash: 1,
        flags: 0,
        radius: 1.,
        local_translation: Vec3::ZERO,
        local_scale: Vec3::ONE,
        local_rotation: Quat::IDENTITY,
        inverse_bind_translation: Vec3::ZERO,
        inverse_bind_scale: Vec3::ONE,
        inverse_bind_rotation: Quat::IDENTITY,
    }];
    s
}
fn version(raw: &[u8]) -> u32 {
    u32::from_le_bytes(raw[8..12].try_into().unwrap())
}

#[test]
fn selected_canm_encodes_and_reports_checked_motion() {
    let a = clip(31);
    let s = skeleton();
    let opts = AnimationExportOptions::new(AnimationFormat::Canm);
    let result = a.export_bytes(&opts, Some(&s)).unwrap();
    assert_eq!(result.requested_format, AnimationFormat::Canm);
    assert_eq!(result.format, AnimationFormat::Canm);
    assert!(result.fallbacks.is_empty());
    assert_eq!(&result.bytes[..8], b"r3d2canm");
    assert_eq!(version(&result.bytes), 1);
    assert!(result.metrics.rotation <= opts.rotation_error);
    assert!(result.metrics.model_space.unwrap() <= opts.model_space_error);
    let parsed = Animation::from_bytes(&result.bytes).unwrap();
    assert_eq!(parsed.frame_count(), 31);
    assert_eq!(parsed.tracks[0].joint_hash, 1);
    assert_eq!(result.bytes, a.export_bytes(&opts, Some(&s)).unwrap().bytes);
}

#[test]
fn clock_failure_falls_back_to_verified_v5() {
    let mut a = clip(2);
    a.tracks[0].frames[1].translation.x = 1_000_000.;
    let opts = AnimationExportOptions::new(AnimationFormat::Canm);
    let out = a.export_bytes(&opts, None).unwrap();
    assert_eq!(out.format, AnimationFormat::V5);
    assert_eq!(version(&out.bytes), 5);
    assert_eq!(out.fallbacks.len(), 1);
    assert_eq!(out.fallbacks[0].format, AnimationFormat::Canm);
    assert!(out.fallbacks[0].reason.contains("clock precision"));
    assert!(out.metrics.model_space.is_none());
    assert!(out.metrics.translation <= opts.translation_error);
}

#[test]
fn v5_quantization_failure_falls_back_to_full_precision_v4() {
    let mut a = clip(2);
    for f in &mut a.tracks[0].frames {
        f.rotation = Quat::from_rotation_y(0.713578);
    }
    let mut opts = AnimationExportOptions::new(AnimationFormat::V5);
    opts.rotation_error = 0.000001;
    let out = a.export_bytes(&opts, None).unwrap();
    assert_eq!(out.format, AnimationFormat::V4);
    assert_eq!(version(&out.bytes), 4);
    assert_eq!(out.fallbacks.len(), 1);
    assert_eq!(out.fallbacks[0].format, AnimationFormat::V5);
    assert!(out.metrics.rotation <= opts.rotation_error);
}

#[test]
fn canm_can_reach_v4_without_changing_tolerances() {
    let mut a = clip(2);
    a.tracks[0].frames[1].translation.x = 1_000_000.;
    for f in &mut a.tracks[0].frames {
        f.rotation = Quat::from_rotation_y(0.713578);
    }
    let mut opts = AnimationExportOptions::new(AnimationFormat::Canm);
    opts.rotation_error = 0.000001;
    let out = a.export_bytes(&opts, None).unwrap();
    assert_eq!(out.format, AnimationFormat::V4);
    assert_eq!(
        out.fallbacks.iter().map(|f| f.format).collect::<Vec<_>>(),
        vec![AnimationFormat::Canm, AnimationFormat::V5]
    );
    assert!(out.metrics.rotation <= opts.rotation_error);
}

#[test]
fn fitting_failure_reports_fitting_instead_of_a_fictitious_bone() {
    let mut a = clip(2);
    for f in &mut a.tracks[0].frames {
        f.rotation = Quat::from_rotation_y(0.713578);
        f.translation = Vec3::ZERO;
    }
    let mut opts = AnimationExportOptions::new(AnimationFormat::Canm);
    opts.rotation_error = 0.000001;
    let out = a.export_bytes(&opts, None).unwrap();
    assert_eq!(out.format, AnimationFormat::V4);
    assert!(out.fallbacks[0].reason.contains("key fitting"));
    assert!(out.fallbacks[1].reason.contains("rotation"));
}

#[test]
fn v4_selection_ignores_raw_bytes_but_preserves_normal_save() {
    let raw = clip(4).to_v5_bytes().unwrap();
    let mut a = Animation::from_bytes(&raw).unwrap();
    a.tracks[0].frames[0].translation.x = 42.;
    let out = a
        .export_bytes(&AnimationExportOptions::new(AnimationFormat::V4), None)
        .unwrap();
    assert_eq!(out.format, AnimationFormat::V4);
    assert!(out.fallbacks.is_empty());
    assert_eq!(
        Animation::from_bytes(&out.bytes).unwrap().tracks[0].frames[0]
            .translation
            .x,
        42.
    );
    assert!(a.is_byte_exact());
    assert_eq!(a.to_bytes().unwrap(), raw);
}

#[test]
fn single_frame_canm_keeps_constant_pose() {
    let mut a = clip(1);
    a.tracks[0].frames[0].rotation = Quat::from_rotation_x(0.7);
    let out = a
        .export_bytes(&AnimationExportOptions::new(AnimationFormat::Canm), None)
        .unwrap();
    assert_eq!(out.format, AnimationFormat::Canm);
    let b = Animation::from_bytes(&out.bytes).unwrap();
    assert_eq!(b.frame_count(), 1);
    assert_eq!(
        b.tracks[0].frames[0].translation,
        a.tracks[0].frames[0].translation
    );
}

#[test]
fn invalid_samples_and_options_are_not_hidden_by_fallback() {
    let opts = AnimationExportOptions::new(AnimationFormat::Canm);
    for mode in 0..5 {
        let mut a = clip(2);
        match mode {
            0 => a.fps = f32::NAN,
            1 => a.tracks[0].frames[0].rotation = Quat::from_xyzw(0., 0., 0., 0.),
            2 => a.tracks[0].frames[1].time = 3.,
            3 => a.tracks.push(a.tracks[0].clone()),
            _ => a.tracks[0].frames[0].translation.x = f32::INFINITY,
        }
        assert!(matches!(
            a.export_bytes(&opts, None),
            Err(Error::InvalidData(_))
        ));
    }
    for bad in [-1., f32::INFINITY, f32::NAN] {
        let mut opts = opts;
        opts.model_space_error = bad;
        assert!(matches!(
            clip(2).export_bytes(&opts, None),
            Err(Error::InvalidData(_))
        ));
    }
}

#[test]
fn invalid_hierarchies_are_not_hidden_by_fallback() {
    let a = clip(2);
    let opts = AnimationExportOptions::new(AnimationFormat::Canm);
    for mode in 0..4 {
        let mut s = skeleton();
        match mode {
            0 => s.joints[0].parent_id = 0,
            1 => s.joints[0].parent_id = 1,
            2 => s.joints.push(s.joints[0].clone()),
            _ => s.joints[0].hash = 99,
        }
        assert!(matches!(
            a.export_bytes(&opts, Some(&s)),
            Err(Error::InvalidData(_))
        ));
    }
}

#[test]
fn palette_overflow_is_not_misrepresented_as_successful_v4_fallback() {
    let mut a = clip(65537);
    for (i, f) in a.tracks[0].frames.iter_mut().enumerate() {
        f.translation = Vec3::new(i as f32, 0., 0.);
        f.rotation = Quat::IDENTITY;
    }
    let err = a
        .export_bytes(&AnimationExportOptions::new(AnimationFormat::Canm), None)
        .unwrap_err();
    assert!(matches!(err, Error::AnimationExportFailed(_)));
    let msg = err.to_string();
    assert!(msg.contains("CANM:"));
    assert!(msg.contains("v5:"));
    assert!(msg.contains("v4:"));
    assert!(msg.contains("palette"));
}

#[test]
fn failed_export_leaves_destination_untouched() {
    let mut a = clip(2);
    a.fps = 0.;
    let opts = AnimationExportOptions::new(AnimationFormat::Canm);
    let mut output = b"keep me".to_vec();
    assert!(a.export_to_writer(&mut output, &opts, None).is_err());
    assert_eq!(output, b"keep me");
    let path = std::env::temp_dir().join(format!(
        "rs-anim-export-preserve-{}.anm",
        std::process::id()
    ));
    std::fs::write(&path, b"keep me").unwrap();
    assert!(a.export_to_path(&path, &opts, None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
    std::fs::remove_file(path).unwrap();
}
