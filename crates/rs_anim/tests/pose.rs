use std::path::PathBuf;

use rs_anim::{Animation, Pose, Skeleton};
use rs_io::Parse;
use rs_math::Mat4;

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const SKELETONS: &[&str] = &["azir.skl", "azir_small.skl", "azirpair.skl"];

const ANIMATIONS: &[&str] = &[
    "aatrox__skin07_ult_attack1.anm",
    "aatrox_sheath_run_haste.anm",
    "dance_windup.anm",
    "compressed_507c1f34b053b389.anm",
    "compressed_e890878834c561be.anm",
    "compressed_e63f4f2e8c074937.anm",
];

/** Deviation of a skinning matrix from identity, split into its rotation/scale block and its
translation column. They need separate tolerances: the block holds unit-scale numbers while the
column holds model-space distances in the hundreds, so one absolute threshold over all sixteen
components would either be meaningless for the block or unmeetable for the column. */
fn identity_error(matrix: Mat4, position_magnitude: f32) -> (f32, f32) {
    let d = (matrix - Mat4::IDENTITY).to_cols_array();
    let block = d[0..3]
        .iter()
        .chain(&d[4..7])
        .chain(&d[8..11])
        .fold(0.0f32, |worst, v| worst.max(v.abs()));
    let translation = d[12..15].iter().fold(0.0f32, |worst, v| worst.max(v.abs()));
    (block, translation / position_magnitude.max(1.0))
}

/** A joint's inverse bind transform is by definition the inverse of its composed model-space bind
transform, so posing a skeleton in its own rest pose must yield identity skinning matrices. On a real
rig this cross-checks the parent-chain composition against data Riot stored independently: any error
in the walk shows up immediately as a non-identity product.

The residual is not zero because the inverse bind is stored decomposed as float32 translation, scale
and rotation rather than as a matrix, so recomposing it cannot return the exact inverse, and the
error accumulates with hierarchy depth. Measured on Azir it stays under 2e-5 relative on translation
and 5e-3 on the rotation block, against 7.75e-7 for a single-joint rig with no chain at all. */
#[test]
fn rest_pose_skinning_matrices_are_identity_on_real_rigs() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .skl pose tests");
        return;
    };

    let mut checked = 0;
    for name in SKELETONS {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        let skeleton = Skeleton::from_path(&path).expect("parse skl");
        assert!(!skeleton.joints().is_empty(), "{name}: no joints");

        let pose = Pose::rest(&skeleton);
        let globals = pose.global_transforms(&skeleton);
        let skinning = pose.skinning_matrices(&skeleton);
        assert_eq!(skinning.len(), skeleton.joints().len(), "{name}: length");

        let (mut worst_block, mut worst_translation) = (0.0f32, 0.0f32);
        for (index, matrix) in skinning.iter().enumerate() {
            let magnitude = globals[index].w_axis.truncate().length();
            let (block, translation) = identity_error(*matrix, magnitude);
            let joint = &skeleton.joints()[index];
            assert!(
                block < 5e-3,
                "{name}: joint {index} ({}) rotation block is not identity, error {block}",
                joint.name
            );
            assert!(
                translation < 1e-4,
                "{name}: joint {index} ({}) translation column is not identity, relative error {translation}",
                joint.name
            );
            worst_block = worst_block.max(block);
            worst_translation = worst_translation.max(translation);
        }

        eprintln!(
            "{name}: {} joints, worst rest skinning error - block {worst_block:.2e}, translation {worst_translation:.2e} relative",
            skeleton.joints().len()
        );
        checked += 1;
    }

    if checked == 0 {
        eprintln!("no skeleton fixtures present; nothing verified");
    }
}

/// Every composed model-space transform on a real rig must be finite and invertible.
#[test]
fn rest_pose_globals_are_finite_on_real_rigs() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping");
        return;
    };

    for name in SKELETONS {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let skeleton = Skeleton::from_path(&path).expect("parse skl");
        for (index, matrix) in Pose::rest(&skeleton)
            .global_transforms(&skeleton)
            .iter()
            .enumerate()
        {
            assert!(
                matrix.to_cols_array().iter().all(|v| v.is_finite()),
                "{name}: joint {index} has a non-finite global transform"
            );
            assert!(
                matrix.determinant().abs() > 1e-9,
                "{name}: joint {index} global transform is degenerate"
            );
        }
    }
}

/** Sampling a real clip between its keyframes must stay well-formed, and sampling exactly on a
keyframe must reproduce that keyframe rather than drifting through the interpolator. */
#[test]
fn sampling_real_animations_is_well_formed() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .anm sampling tests");
        return;
    };

    let mut checked = 0;
    for name in ANIMATIONS {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        let anim = Animation::from_path(&path).expect("parse anm");
        let duration = anim.duration();
        assert!(duration > 0.0, "{name}: zero duration");

        for track in anim.tracks() {
            for frame in &track.frames {
                let sampled = track.sample(frame.time).expect("sample on keyframe");
                let drift = (sampled.translation - frame.translation).length();
                assert!(
                    drift < 1e-3,
                    "{name}: sampling on keyframe {} drifted {drift}",
                    frame.time
                );
            }

            for step in 0..=64 {
                let time = duration * step as f32 / 64.0;
                let sampled = track.sample(time).expect("sample mid-clip");
                assert!(
                    (sampled.rotation.length() - 1.0).abs() < 1e-3,
                    "{name}: non-unit rotation at t={time}"
                );
                assert!(
                    sampled.translation.is_finite() && sampled.scale.is_finite(),
                    "{name}: non-finite sample at t={time}"
                );
            }

            let before = track.sample(-1.0).expect("clamped below");
            let after = track.sample(duration * 10.0).expect("clamped above");
            assert_eq!(before.translation, track.frames[0].translation);
            assert_eq!(
                after.translation,
                track.frames.last().unwrap().translation,
                "{name}: sampling past the end must clamp to the last keyframe"
            );
        }

        eprintln!(
            "{name}: {} tracks, {duration:.3}s, sampled 65 times per track",
            anim.tracks().len()
        );
        checked += 1;
    }

    if checked == 0 {
        eprintln!("no animation fixtures present; nothing verified");
    }
}

/** Posing a rig with a clip authored for a different rig must still produce a complete, valid pose:
every joint the clip does not drive falls back to its bind transform. */
#[test]
fn posing_with_a_foreign_clip_keeps_the_bind_pose() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping");
        return;
    };

    let skeleton_path = dir.join("azir.skl");
    let anim_path = dir.join("dance_windup.anm");
    if !skeleton_path.is_file() || !anim_path.is_file() {
        eprintln!("missing azir.skl or dance_windup.anm; skipping");
        return;
    }

    let skeleton = Skeleton::from_path(&skeleton_path).expect("parse skl");
    let anim = Animation::from_path(&anim_path).expect("parse anm");
    let rest = Pose::rest(&skeleton);
    let posed = Pose::sample(&skeleton, &anim, anim.duration() * 0.5);

    assert_eq!(posed.locals().len(), skeleton.joints().len());

    let driven = skeleton
        .joints()
        .iter()
        .filter(|joint| anim.track(joint.hash).is_some())
        .count();

    for (index, joint) in skeleton.joints().iter().enumerate() {
        if anim.track(joint.hash).is_none() {
            assert_eq!(
                posed.local(index),
                rest.local(index),
                "{}: undriven joint drifted off its bind pose",
                joint.name
            );
        }
    }

    eprintln!(
        "azir.skl posed by dance_windup.anm: {driven}/{} joints driven, remainder held at bind pose",
        skeleton.joints().len()
    );
}
