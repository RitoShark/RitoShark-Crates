//! Streaming precision checks; no tracks-by-subframes pose cache is allocated.
use super::*;
use rs_io::Parse;

pub(super) fn check_options(o: &AnimationExportOptions, skeleton: bool) -> Quality {
    Quality {
        local: [o.rotation_error, o.translation_error, o.scale_error],
        model: if skeleton {
            o.model_space_error
        } else {
            f32::INFINITY
        },
    }
}

struct Check<'a, 'b> {
    src: &'a Source<'b>,
    skl: &'a Skeleton,
    rig: Rig,
    q: Quality,
    metrics: AnimationExportMetrics,
    reference: Vec<[Vec4; 3]>,
    worlds: [Vec<Mat4>; 2],
}
impl<'a, 'b> Check<'a, 'b> {
    fn new(src: &'a Source<'b>, skl: &'a Skeleton, q: Quality, has_skeleton: bool) -> Self {
        Self {
            src,
            skl,
            rig: Rig::new(src, skl),
            q,
            metrics: AnimationExportMetrics {
                model_space: has_skeleton.then_some(0.),
                ..Default::default()
            },
            reference: vec![[Vec4::ZERO; 3]; src.hashes.len()],
            worlds: [
                vec![Mat4::IDENTITY; skl.joints.len()],
                vec![Mat4::IDENTITY; skl.joints.len()],
            ],
        }
    }
    fn sample(&mut self, seconds: f32, actual: &[[Vec4; 3]]) -> Result<()> {
        if actual.len() != self.src.hashes.len() {
            return Err(Error::InvalidData("export changed track count"));
        }
        for (i, output) in actual.iter().enumerate() {
            for (c, &value) in output.iter().enumerate() {
                let expected = self.src.sample_dense_seconds(i * 3 + c, seconds);
                if !expected.is_finite() {
                    return Err(Error::InvalidData("source interpolation overflow"));
                }
                self.reference[i][c] = expected;
                let error = finite_error(kind(c), expected, value);
                let metric = match c {
                    0 => &mut self.metrics.rotation,
                    1 => &mut self.metrics.translation,
                    _ => &mut self.metrics.scale,
                };
                *metric = metric.max(error);
                if error > self.q.local[c] {
                    return Err(tolerance_error(
                        self.src.hashes[i],
                        kind(c),
                        (seconds * self.src.fps).round() as usize,
                        error,
                        self.q.local[c],
                    ));
                }
            }
        }
        for &j in &self.rig.order {
            let joint = &self.skl.joints[j];
            for (side, samples) in [&self.reference[..], actual].iter().enumerate() {
                let local = self.rig.tracks[j]
                    .map_or_else(|| joint.local_transform(), |i| matrix(samples[i]));
                self.worlds[side][j] = if joint.parent_id < 0 {
                    local
                } else {
                    self.worlds[side][joint.parent_id as usize] * local
                };
            }
            if !self.worlds[0][j].is_finite() {
                return Err(Error::InvalidData("source hierarchy transform overflow"));
            }
            for p in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
                let error = self.worlds[0][j]
                    .transform_point3(p)
                    .distance(self.worlds[1][j].transform_point3(p));
                let error = if error.is_finite() {
                    error
                } else {
                    f32::INFINITY
                };
                if let Some(max) = self.metrics.model_space.as_mut() {
                    *max = max.max(error);
                }
                if error > self.q.model {
                    return Err(Error::CompressionTolerance {
                        joint_hash: joint.hash,
                        channel: "model-space probe",
                        frame: (seconds * self.src.fps).round() as usize,
                        error,
                        tolerance: self.q.model,
                    });
                }
            }
        }
        Ok(())
    }
}

fn audit_times(src: &Source, clock_edges: bool) -> Result<Vec<f32>> {
    let count = src
        .frames
        .checked_sub(1)
        .and_then(|n| n.checked_mul(193))
        .and_then(|n| n.checked_add(if clock_edges { 131072 } else { 2 }))
        .ok_or(Error::Unsupported(
            "export validation sample count overflow",
        ))?;
    let mut times = Vec::new();
    times
        .try_reserve(count)
        .map_err(|_| Error::Unsupported("insufficient memory for export validation times"))?;
    for steps in [96, 97] {
        let rate = src.fps * steps as f32;
        for i in 0..(src.duration * rate).round() as usize + 1 {
            times.push((i as f32 * (1. / rate)).min(src.duration));
        }
    }
    times.extend([0., src.duration]);
    if clock_edges && src.duration > 0. {
        for tick in 0..65535u32 {
            for fraction in [0.001, 0.999] {
                times.push(((tick as f64 + fraction) / 65535. * src.duration as f64) as f32);
            }
        }
    }
    times.sort_unstable_by(f32::total_cmp);
    times.dedup();
    Ok(times)
}

pub(crate) fn validate_dense(
    source: &Animation,
    raw: &[u8],
    options: &AnimationExportOptions,
    skeleton: Option<&Skeleton>,
) -> Result<AnimationExportMetrics> {
    let decoded = Animation::from_bytes(raw)?;
    if decoded.frame_count() != source.frame_count()
        || decoded.tracks.len() != source.tracks.len()
        || decoded
            .tracks
            .iter()
            .zip(&source.tracks)
            .any(|(a, b)| a.joint_hash != b.joint_hash)
    {
        return Err(Error::InvalidData("dense export changed animation layout"));
    }
    let empty = Skeleton::new();
    let skl = skeleton.unwrap_or(&empty);
    let src = Source::new(source);
    let output = Source::new(&decoded);
    let mut check = Check::new(
        &src,
        skl,
        check_options(options, skeleton.is_some()),
        skeleton.is_some(),
    );
    let mut values = vec![[Vec4::ZERO; 3]; src.hashes.len()];
    for seconds in audit_times(&src, false)? {
        for (i, value) in values.iter_mut().enumerate() {
            *value = std::array::from_fn(|c| output.sample_dense_seconds(i * 3 + c, seconds));
        }
        check.sample(seconds, &values)?;
    }
    Ok(check.metrics)
}

pub(super) fn validate_canm(
    src: &Source,
    raw: &[u8],
    options: &AnimationExportOptions,
    skl: &Skeleton,
    has_skeleton: bool,
) -> Result<AnimationExportMetrics> {
    let mut reader = Cursor::new(raw);
    reader.set_position(12);
    let decoded = crate::compressed::Compressed::from_reader(&mut reader)?;
    if decoded.joint_hashes() != src.hashes {
        return Err(Error::InvalidData("CANM export changed track hashes"));
    }
    let mut check = Check::new(src, skl, check_options(options, has_skeleton), has_skeleton);
    decoded.visit_samples(audit_times(src, true)?.into_iter(), |seconds, values| {
        check.sample(seconds, values)
    })?;
    Ok(check.metrics)
}

/// Two source times mapped to the same CANM tick must share one decoded pose.
/// Half their separation is a necessary lower bound, not proof of successful compression.
pub(super) fn check_clock(src: &Source, skl: &Skeleton, q: Quality) -> Result<()> {
    if src.duration == 0. {
        return Ok(());
    }
    let rig = Rig::new(src, skl);
    let mut locals = [
        vec![[Vec4::ZERO; 3]; src.hashes.len()],
        vec![[Vec4::ZERO; 3]; src.hashes.len()],
    ];
    let mut worlds = [
        vec![Mat4::IDENTITY; skl.joints.len()],
        vec![Mat4::IDENTITY; skl.joints.len()],
    ];
    for tick in 0..65535u32 {
        let times = [0.05, 0.95].map(|x| ((tick as f64 + x) / 65535. * src.duration as f64) as f32);
        if times
            .iter()
            .any(|&t| crate::compressed::compress_time(t, src.duration) != tick as u16)
        {
            continue;
        }
        for (i, &hash) in src.hashes.iter().enumerate() {
            for (c, _) in q.local.iter().enumerate() {
                for side in 0..2 {
                    locals[side][i][c] = src.sample_dense_seconds(i * 3 + c, times[side]);
                }
                let error = finite_error(kind(c), locals[0][i][c], locals[1][i][c]) * 0.5;
                if !error.is_finite() {
                    return Err(Error::InvalidData("source interpolation overflow"));
                }
                if error > q.local[c] {
                    return Err(Error::CompressionTimePrecision {
                        joint_hash: hash,
                        channel: kind(c).name(),
                        tick: tick as u16,
                        seconds: times,
                        error,
                        tolerance: q.local[c],
                    });
                }
            }
        }
        for &j in &rig.order {
            let joint = &skl.joints[j];
            for side in 0..2 {
                let local = rig.tracks[j]
                    .map_or_else(|| joint.local_transform(), |i| matrix(locals[side][i]));
                worlds[side][j] = if joint.parent_id < 0 {
                    local
                } else {
                    worlds[side][joint.parent_id as usize] * local
                };
            }
            for p in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
                let error = worlds[0][j]
                    .transform_point3(p)
                    .distance(worlds[1][j].transform_point3(p))
                    * 0.5;
                if !error.is_finite() {
                    return Err(Error::InvalidData("source hierarchy transform overflow"));
                }
                if error > q.model {
                    return Err(Error::CompressionTimePrecision {
                        joint_hash: joint.hash,
                        channel: "model-space probe",
                        tick: tick as u16,
                        seconds: times,
                        error,
                        tolerance: q.model,
                    });
                }
            }
        }
    }
    Ok(())
}
