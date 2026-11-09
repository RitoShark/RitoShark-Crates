//! Sample-grid and hierarchy checks around the sparse CANM encoder.
use crate::{AnimFrame, AnimTrack, Animation, CompressionOptions, Error, Result, Skeleton};
use rs_math::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};

pub(crate) fn encode(
    source: &Animation,
    options: &CompressionOptions,
    skeleton: Option<&Skeleton>,
) -> Result<Vec<u8>> {
    if source.tracks.is_empty() || source.tracks.len() > 16384 {
        return Err(Error::Unsupported(
            "compressed animation requires 1..=16384 tracks",
        ));
    }
    if [
        options.rotation_error,
        options.translation_error,
        options.scale_error,
    ]
    .iter()
    .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(Error::InvalidData(
            "compression tolerances must be finite and nonnegative",
        ));
    }
    let count = crate::animation_v5_write::validate_samples(source)?;
    if options.sample_subdivisions == 0 || options.sample_subdivisions > 64 {
        return Err(Error::InvalidData(
            "compression sample_subdivisions must be in 1..=64",
        ));
    }
    if !options.model_space_error.is_finite() || options.model_space_error < 0.0 {
        return Err(Error::InvalidData(
            "compression model_space_error must be finite and nonnegative",
        ));
    }
    let steps = options.sample_subdivisions as usize;
    let dense_count = (count - 1).checked_mul(steps).and_then(|n| n.checked_add(1))
        .filter(|&n| n <= 65536)
        .ok_or(Error::Unsupported("compression validation grid exceeds 65536 samples; lower sample_subdivisions explicitly or use ANMD v5"))?;
    let fps = source.fps * steps as f32;
    if !fps.is_finite() || !(1.0 / fps).is_finite() {
        return Err(Error::InvalidData(
            "compression validation rate is not representable",
        ));
    }
    let mut dense = Animation::new(fps);
    for track in &source.tracks {
        let mut frames = Vec::with_capacity(dense_count);
        for i in 0..dense_count {
            let index = i / steps;
            let a = track.frames[index];
            let b = track.frames[(index + 1).min(count - 1)];
            let blend = (i % steps) as f32 / steps as f32;
            frames.push(AnimFrame::new(
                i as f32 / fps,
                a.rotation
                    .normalize()
                    .slerp(b.rotation.normalize(), blend)
                    .normalize(),
                a.translation.lerp(b.translation, blend),
                a.scale.lerp(b.scale, blend),
            ));
        }
        dense.tracks.push(AnimTrack {
            joint_hash: track.joint_hash,
            frames,
        });
    }
    let hierarchy = skeleton.map(|s| Hierarchy::new(s, &dense)).transpose()?;
    let mut last_error = None;
    // Tighten reduction, never the caller's permitted quantization error. The last pass retains
    // every grid sample, proving whether the requested bound is attainable with this packing.
    for factor in [1.0, 0.25, 0.0625, 0.015625, -1.0] {
        let bytes = crate::compressed_write::encode_samples(&dense, options, source.fps, factor)?;
        if let Some(hierarchy) = &hierarchy {
            let mut reader = std::io::Cursor::new(&bytes);
            reader.set_position(12);
            let decoded =
                crate::compressed::Compressed::from_reader(&mut reader)?.bake_at_rate(fps)?;
            match hierarchy.check(&dense.tracks, &decoded, options.model_space_error) {
                Ok(()) => return Ok(bytes),
                Err(error @ Error::CompressionTolerance { .. }) => last_error = Some(error),
                Err(error) => return Err(error),
            }
        } else {
            return Ok(bytes);
        }
    }
    Err(last_error.expect("a rejected hierarchy validation records an error"))
}

pub(crate) struct Hierarchy<'a> {
    skeleton: &'a Skeleton,
    order: Vec<usize>,
    tracks: Vec<Option<usize>>,
}

impl<'a> Hierarchy<'a> {
    pub(crate) fn new(skeleton: &'a Skeleton, animation: &Animation) -> Result<Self> {
        let joints = &skeleton.joints;
        if joints.is_empty() {
            return Err(Error::InvalidData("compression skeleton is empty"));
        }
        let mut hashes = HashSet::new();
        for joint in joints {
            if !hashes.insert(joint.hash) {
                return Err(Error::InvalidData(
                    "compression skeleton has duplicate joint hashes",
                ));
            }
            if joint.parent_id < -1
                || joint.parent_id as usize >= joints.len() && joint.parent_id != -1
            {
                return Err(Error::InvalidData(
                    "compression skeleton parent index is out of range",
                ));
            }
            if !joint.local_translation.is_finite()
                || !joint.local_scale.is_finite()
                || !joint.local_rotation.is_finite()
                || !joint.local_rotation.length_squared().is_finite()
                || joint.local_rotation.length_squared() < 1e-20
            {
                return Err(Error::InvalidData(
                    "compression skeleton has an invalid rest transform",
                ));
            }
        }
        // Iterative topological walk supports forests and out-of-order joint arrays without
        // accepting cycles or relying on recursion depth for user-authored skeletons.
        let mut state = vec![0u8; joints.len()];
        let mut order = Vec::with_capacity(joints.len());
        for start in 0..joints.len() {
            let mut chain = Vec::new();
            let mut next = Some(start);
            while let Some(i) = next {
                if state[i] == 2 {
                    break;
                }
                if state[i] == 1 {
                    return Err(Error::InvalidData(
                        "compression skeleton contains a parent cycle",
                    ));
                }
                state[i] = 1;
                chain.push(i);
                next = (joints[i].parent_id >= 0).then_some(joints[i].parent_id as usize);
            }
            for i in chain.into_iter().rev() {
                state[i] = 2;
                order.push(i);
            }
        }
        let map: HashMap<_, _> = animation
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.joint_hash, i))
            .collect();
        let tracks: Vec<_> = joints.iter().map(|j| map.get(&j.hash).copied()).collect();
        if tracks.iter().all(Option::is_none) {
            return Err(Error::InvalidData(
                "compression skeleton matches no animation tracks",
            ));
        }
        Ok(Self {
            skeleton,
            order,
            tracks,
        })
    }

    fn check(&self, source: &[AnimTrack], decoded: &[AnimTrack], limit: f32) -> Result<()> {
        let count = source[0].frames.len();
        if decoded.len() != source.len() || decoded.iter().any(|t| t.frames.len() != count) {
            return Err(Error::InvalidData(
                "compressed animation validation grid changed",
            ));
        }
        let mut expected = vec![Mat4::IDENTITY; self.order.len()];
        let mut actual = expected.clone();
        let mut worst = None;
        let mut maximum = limit;
        for frame in 0..count {
            for &i in &self.order {
                let joint = &self.skeleton.joints[i];
                for (tracks, globals) in [(source, &mut expected), (decoded, &mut actual)] {
                    let local = if let Some(track) = self.tracks[i] {
                        let sample = tracks[track].frames[frame];
                        Mat4::from_scale_rotation_translation(
                            sample.scale,
                            sample.rotation.normalize(),
                            sample.translation,
                        )
                    } else {
                        Mat4::from_scale_rotation_translation(
                            joint.local_scale,
                            joint.local_rotation.normalize(),
                            joint.local_translation,
                        )
                    };
                    globals[i] = if joint.parent_id < 0 {
                        local
                    } else {
                        globals[joint.parent_id as usize] * local
                    };
                    if !globals[i].is_finite() {
                        return Err(Error::InvalidData(
                            "compression model-space transform overflow",
                        ));
                    }
                }
                for probe in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
                    let error = expected[i]
                        .transform_point3(probe)
                        .distance(actual[i].transform_point3(probe));
                    if !error.is_finite() {
                        return Err(Error::InvalidData("compression model-space error overflow"));
                    }
                    if error > maximum {
                        maximum = error;
                        worst = Some(Error::CompressionTolerance {
                            joint_hash: joint.hash,
                            channel: "model-space probe",
                            frame,
                            error,
                            tolerance: limit,
                        });
                    }
                }
            }
        }
        worst.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Joint;
    use rs_math::Quat;

    fn joint(hash: u32, parent: i16, length: f32) -> Joint {
        Joint {
            name: hash.to_string(),
            flags: 0,
            id: hash as i16,
            parent_id: parent,
            hash,
            radius: 1.0,
            local_translation: Vec3::X * length,
            local_scale: Vec3::ONE,
            local_rotation: Quat::IDENTITY,
            inverse_bind_translation: Vec3::ZERO,
            inverse_bind_rotation: Quat::IDENTITY,
            inverse_bind_scale: Vec3::ONE,
        }
    }
    fn fixture() -> (Animation, Skeleton) {
        let mut a = Animation::new(30.0);
        a.tracks.push(AnimTrack {
            joint_hash: 1,
            frames: (0..31)
                .map(|i| {
                    AnimFrame::new(
                        i as f32 / 30.0,
                        Quat::from_rotation_z((i as f32 * 0.2).sin() * 0.002),
                        Vec3::ZERO,
                        Vec3::ONE,
                    )
                })
                .collect(),
        });
        let mut s = Skeleton::new();
        // Unanimated child before its animated parent.
        s.joints = vec![joint(2, 1, 100.0), joint(1, -1, 0.0)];
        (a, s)
    }
    fn decoded(bytes: &[u8], fps: f32) -> Vec<AnimTrack> {
        let mut reader = std::io::Cursor::new(bytes);
        reader.set_position(12);
        crate::compressed::Compressed::from_reader(&mut reader)
            .unwrap()
            .bake_at_rate(fps)
            .unwrap()
    }

    #[test]
    fn quarter_frames_are_checked_against_the_source_interpolation() {
        let mut a = Animation::new(30.0);
        a.tracks.push(AnimTrack {
            joint_hash: 1,
            frames: [0.0, 1.0, 0.0]
                .into_iter()
                .enumerate()
                .map(|(i, x)| {
                    AnimFrame::new(
                        i as f32 / 30.0,
                        Quat::from_rotation_y(x),
                        Vec3::X * x,
                        Vec3::ONE,
                    )
                })
                .collect(),
        });
        let options = CompressionOptions {
            translation_error: 0.001,
            ..Default::default()
        };
        let bytes = a.to_compressed_bytes(&options).unwrap();
        let output = decoded(&bytes, 120.0);
        assert_eq!(output[0].frames.len(), 9);
        for (i, f) in output[0].frames.iter().enumerate() {
            let x = if i <= 4 { i } else { 8 - i } as f32 / 4.0;
            assert!((f.translation.x - x).abs() <= 0.001);
            let expected = Quat::from_rotation_y(x);
            assert!(
                (f.rotation - expected)
                    .length()
                    .min((f.rotation + expected).length())
                    < 0.0006
            );
        }
    }

    #[test]
    fn hierarchy_error_refines_ancestor_curves_and_keeps_unanimated_children() {
        let (a, s) = fixture();
        let options = CompressionOptions {
            rotation_error: 0.05,
            model_space_error: 0.02,
            ..Default::default()
        };
        let plain = a.to_compressed_bytes(&options).unwrap();
        let hierarchy = Hierarchy::new(&s, &a).unwrap();
        assert!(
            hierarchy
                .check(&a.tracks, &decoded(&plain, 30.0), options.model_space_error)
                .is_err()
        );
        let checked = a.to_compressed_bytes_with_skeleton(&options, &s).unwrap();
        assert!(checked.len() > plain.len());
        hierarchy
            .check(
                &a.tracks,
                &decoded(&checked, 30.0),
                options.model_space_error,
            )
            .unwrap();
    }

    #[test]
    fn impossible_hierarchy_accuracy_is_an_error_and_does_not_touch_destination() {
        let (a, mut s) = fixture();
        s.joints[0].local_translation.x = 10000.0;
        let options = CompressionOptions {
            model_space_error: 0.000001,
            ..Default::default()
        };
        let mut out = vec![7, 8, 9];
        let error = a
            .write_compressed_with_skeleton(&mut out, &options, &s)
            .unwrap_err();
        assert!(matches!(
            error,
            Error::CompressionTolerance {
                channel: "model-space probe",
                ..
            }
        ));
        assert_eq!(out, [7, 8, 9]);
    }

    #[test]
    fn invalid_skeletons_and_validation_grids_fail_explicitly() {
        let (a, s) = fixture();
        let options = CompressionOptions::default();
        for kind in 0..5 {
            let mut bad = s.clone();
            match kind {
                0 => bad.joints[1].parent_id = 0,
                1 => bad.joints[0].parent_id = 90,
                2 => bad.joints[1].hash = 2,
                3 => bad.joints[0].local_scale.x = f32::NAN,
                _ => bad.joints.iter_mut().for_each(|j| j.hash += 9),
            }
            assert!(a.to_compressed_bytes_with_skeleton(&options, &bad).is_err());
        }
        for steps in [0, 65] {
            assert!(
                a.to_compressed_bytes(&CompressionOptions {
                    sample_subdivisions: steps,
                    ..options
                })
                .is_err()
            );
        }
    }
}
