//! Encoder for the v1 compressed animation container.
//!
//! Key selection uses decoded, quantized values and the playback interpolator. Refining a
//! segment invalidates its neighbours too, because a cubic depends on four keys. Stream entries
//! are ordered by when their channel consumes them, rather than by their stored key time.

use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::ops::Bound::{Excluded, Unbounded};
use std::path::Path;

use rs_io::WriterExt;
use rs_math::{Quat, Vec3, Vec4};

use crate::quantized::{compress_quat, decompress_quat, decompress_vec3};
use crate::{Animation, Error, Result, Skeleton};

/// Maximum reconstruction errors on the validation grid, including quantization.
///
/// Rotation is the angular difference in radians; translation and scale use the largest
/// absolute component difference. Translation uses the caller's animation units (normally
/// League units). The reference between frames is linear translation/scale and shortest-path
/// spherical rotation interpolation. Bounds apply at the chosen grid, not continuously.
/// A tolerance below the format's quantization error returns an error instead of silently
/// exceeding the requested accuracy. All tolerances must be finite and nonnegative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressionOptions {
    pub rotation_error: f32,
    pub translation_error: f32,
    pub scale_error: f32,
    /// Intervals per source frame used for fitting and validation (default 4).
    /// The denser grid must fit the format's 65536 distinct time values. Use 1 explicitly
    /// to validate only original samples when exporting very long clips.
    pub sample_subdivisions: u32,
    /// With a skeleton, maximum Euclidean model-space error at joint origins and unit-axis
    /// probes. Ignored without a skeleton. This is not a skinned-mesh surface error bound.
    pub model_space_error: f32,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            rotation_error: 0.001,
            translation_error: 0.05,
            scale_error: 0.001,
            sample_subdivisions: 4,
            model_space_error: 0.1,
        }
    }
}

#[derive(Clone, Copy)]
enum ChannelKind {
    Rotation = 0,
    Translation = 1,
    Scale = 2,
}

impl ChannelKind {
    fn name(self) -> &'static str {
        match self {
            Self::Rotation => "rotation",
            Self::Translation => "translation",
            Self::Scale => "scale",
        }
    }

    fn error(self, a: Vec4, b: Vec4) -> f32 {
        if matches!(self, Self::Rotation) {
            // 4 atan2(||q-r||, ||q+r||) avoids the loss of precision of acos(dot) near 1.
            // Use the equivalent quaternion hemisphere with the smaller difference.
            let a = a.to_array().map(f64::from);
            let b = b.to_array().map(f64::from);
            let minus: f64 = (0..4).map(|i| (a[i] - b[i]).powi(2)).sum();
            let plus: f64 = (0..4).map(|i| (a[i] + b[i]).powi(2)).sum();
            (4.0 * minus.min(plus).sqrt().atan2(minus.max(plus).sqrt())) as f32
        } else {
            (a - b).abs().truncate().max_element()
        }
    }

    fn sample(self, time: u16, times: [u16; 4], values: [Vec4; 4]) -> Vec4 {
        if matches!(self, Self::Rotation) {
            crate::compressed::sample_parametrized_rotation(
                time,
                times,
                values.map(Quat::from_vec4),
            )
            .into()
        } else {
            crate::compressed::sample_parametrized_vector(time, times, values.map(Vec4::truncate))
                .extend(0.0)
        }
    }
}

#[derive(Clone, Copy)]
struct Key {
    time: u16,
    bytes: [u8; 6],
}

struct Channel {
    joint: u16,
    kind: ChannelKind,
    keys: Vec<Key>,
}

#[derive(Clone, Copy)]
struct Entry {
    key: Key,
    joint_and_kind: u16,
}

impl Animation {
    /// Re-encode the current tracks as compressed `r3d2canm` v1, ignoring preserved source bytes.
    ///
    /// Tracks must contain the same nonzero number of samples, starting at time zero at `fps`.
    /// This matches the decoded representation returned by this crate. Quaternions are
    /// normalized before encoding. Empty tracks, duplicate hashes, invalid timing/nonfinite
    /// transforms, more than 16384 tracks, and colliding 16-bit sample times are rejected.
    /// The animation and its normal byte-exact save behaviour are not changed.
    pub fn to_compressed_bytes(&self, options: &CompressionOptions) -> Result<Vec<u8>> {
        crate::compressed_quality::encode(self, options, None)
    }

    /// Additionally check composed model-space motion, tightening key reduction if needed.
    /// Unanimated joints use skeleton rest transforms; unmatched animation tracks are retained.
    pub fn to_compressed_bytes_with_skeleton(
        &self,
        options: &CompressionOptions,
        skeleton: &Skeleton,
    ) -> Result<Vec<u8>> {
        crate::compressed_quality::encode(self, options, Some(skeleton))
    }

    /// Encode with hierarchy validation before touching the destination writer.
    pub fn write_compressed_with_skeleton<W: Write>(
        &self,
        writer: &mut W,
        options: &CompressionOptions,
        skeleton: &Skeleton,
    ) -> Result<()> {
        let bytes = self.to_compressed_bytes_with_skeleton(options, skeleton)?;
        writer.write_all(&bytes).map_err(rs_io::Error::from)?;
        Ok(())
    }

    /// Encode with hierarchy validation before opening the destination file.
    pub fn to_compressed_path_with_skeleton(
        &self,
        path: impl AsRef<Path>,
        options: &CompressionOptions,
        skeleton: &Skeleton,
    ) -> Result<()> {
        let bytes = self.to_compressed_bytes_with_skeleton(options, skeleton)?;
        rs_io::write_atomic(path, |writer| {
            writer.write_all(&bytes).map_err(rs_io::Error::from)
        })?;
        Ok(())
    }

    /// Encode completely before touching the destination writer.
    pub fn write_compressed<W: Write>(
        &self,
        writer: &mut W,
        options: &CompressionOptions,
    ) -> Result<()> {
        let bytes = self.to_compressed_bytes(options)?;
        writer.write_all(&bytes).map_err(rs_io::Error::from)?;
        Ok(())
    }

    /// Validate and encode before opening the file, preserving an existing file on encode errors.
    pub fn to_compressed_path(
        &self,
        path: impl AsRef<Path>,
        options: &CompressionOptions,
    ) -> Result<()> {
        let bytes = self.to_compressed_bytes(options)?;
        rs_io::write_atomic(path, |writer| {
            writer.write_all(&bytes).map_err(rs_io::Error::from)
        })?;
        Ok(())
    }
}

fn tolerance_error(hash: u32, kind: ChannelKind, frame: usize, error: f32, limit: f32) -> Error {
    Error::CompressionTolerance {
        joint_hash: hash,
        channel: kind.name(),
        frame,
        error,
        tolerance: limit,
    }
}

fn pack_vector(value: Vec3, min: Vec3, max: Vec3) -> [u8; 6] {
    let mut bytes = [0; 6];
    for axis in 0..3 {
        let span = f64::from(max[axis]) - f64::from(min[axis]);
        let step = if span == 0.0 {
            0
        } else {
            (((f64::from(value[axis]) - f64::from(min[axis])) / span * 65535.0).round())
                .clamp(0.0, 65535.0) as u16
        };
        bytes[axis * 2..axis * 2 + 2].copy_from_slice(&step.to_le_bytes());
    }
    bytes
}

fn select_keys(
    times: &[u16],
    original: &[Vec4],
    quantized: &[Vec4],
    kind: ChannelKind,
    tolerance: f32,
    hash: u32,
    reduction_factor: f32,
) -> Result<Vec<usize>> {
    let mut quantization_floor = 0.0f32;
    for (frame, (&a, &b)) in original.iter().zip(quantized).enumerate() {
        let error = kind.error(a, b);
        if !error.is_finite() || error > tolerance {
            return Err(tolerance_error(hash, kind, frame, error, tolerance));
        }
        quantization_floor = quantization_floor.max(error);
    }
    if reduction_factor < 0.0 {
        return Ok((0..original.len()).collect());
    }
    let tolerance = (tolerance * reduction_factor).max(quantization_floor);
    if original
        .iter()
        .all(|&v| kind.error(v, quantized[0]) <= tolerance)
    {
        return Ok(vec![0]);
    }
    let last = original.len() - 1;
    let mut keys = BTreeSet::from([0, last]);
    let mut dirty = BTreeSet::from([0]);
    while let Some(start) = dirty.pop_first() {
        let Some(&end) = keys.range((Excluded(start), Unbounded)).next() else {
            continue;
        };
        let previous = keys.range(..start).next_back().copied().unwrap_or(start);
        let after = keys
            .range((Excluded(end), Unbounded))
            .next()
            .copied()
            .unwrap_or(end);
        let window = [previous, start, end, after];
        let mut worst = tolerance;
        let mut insert = None;
        for frame in start + 1..end {
            let value = kind.sample(
                times[frame],
                window.map(|i| times[i]),
                window.map(|i| quantized[i]),
            );
            let error = kind.error(original[frame], value);
            if !value.is_finite() || !error.is_finite() {
                // A pathological quaternion window may cancel out. Splitting it removes the
                // degeneracy; validation has already checked every individual retained key.
                insert = Some(frame);
                break;
            }
            if error > worst {
                worst = error;
                insert = Some(frame);
            }
        }
        if let Some(frame) = insert {
            keys.insert(frame);
            dirty.extend([previous, start, frame, end]);
        }
    }
    Ok(keys.into_iter().collect())
}

pub(crate) fn encode_samples(
    anim: &Animation,
    options: &CompressionOptions,
    output_fps: f32,
    reduction_factor: f32,
) -> Result<Vec<u8>> {
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
    if !anim.fps.is_finite() || anim.fps <= 0.0 || !(1.0 / anim.fps).is_finite() {
        return Err(Error::InvalidData(
            "compressed animation fps must be finite and positive",
        ));
    }
    if anim.tracks.is_empty() || anim.tracks.len() > 0x4000 {
        return Err(Error::Unsupported(
            "compressed animation requires 1..=16384 tracks",
        ));
    }
    let count = anim.tracks[0].frames.len();
    if count == 0 || count > 65536 {
        return Err(Error::Unsupported(
            "compressed animation requires 1..=65536 samples per track",
        ));
    }
    let dt = 1.0 / anim.fps;
    let duration = (count - 1) as f32 * dt;
    if !duration.is_finite() || (duration * anim.fps).round() as usize + 1 != count {
        return Err(Error::InvalidData(
            "compressed animation duration is not representable",
        ));
    }
    let times: Vec<u16> = (0..count)
        .map(|i| crate::compressed::compress_time(i as f32 * dt, duration))
        .collect();
    if times.windows(2).any(|p| p[0] >= p[1]) {
        return Err(Error::Unsupported(
            "sample times collide in compressed animation's 16-bit time grid",
        ));
    }

    let mut hashes = HashSet::new();
    let mut tmin = Vec3::splat(f32::INFINITY);
    let mut tmax = Vec3::splat(f32::NEG_INFINITY);
    let mut smin = tmin;
    let mut smax = tmax;
    for track in &anim.tracks {
        if !hashes.insert(track.joint_hash) {
            return Err(Error::InvalidData(
                "duplicate compressed animation joint hash",
            ));
        }
        if track.frames.len() != count {
            return Err(Error::InvalidData(
                "compressed animation tracks must share a sample count",
            ));
        }
        for (i, frame) in track.frames.iter().enumerate() {
            let expected = i as f32 * dt;
            let epsilon = (expected.abs() * 4.0 * f32::EPSILON).max(1e-6);
            if !frame.time.is_finite() || (frame.time - expected).abs() > epsilon {
                return Err(Error::InvalidData(
                    "compressed animation requires samples at index / fps, starting at zero",
                ));
            }
            if !frame.translation.is_finite()
                || !frame.scale.is_finite()
                || !frame.rotation.is_finite()
                || !frame.rotation.length_squared().is_finite()
                || frame.rotation.length_squared() < 1e-20
            {
                return Err(Error::InvalidData(
                    "compressed animation contains a nonfinite transform or invalid quaternion",
                ));
            }
            tmin = tmin.min(frame.translation);
            tmax = tmax.max(frame.translation);
            smin = smin.min(frame.scale);
            smax = smax.max(frame.scale);
        }
    }
    if !(tmax - tmin).is_finite() || !(smax - smin).is_finite() {
        return Err(Error::InvalidData(
            "compressed animation vector bounds overflow",
        ));
    }

    let mut channels = Vec::with_capacity(anim.tracks.len() * 3);
    for (joint, track) in anim.tracks.iter().enumerate() {
        for (kind, tolerance) in [
            (ChannelKind::Rotation, options.rotation_error),
            (ChannelKind::Translation, options.translation_error),
            (ChannelKind::Scale, options.scale_error),
        ] {
            let mut original = Vec::with_capacity(count);
            let mut quantized = Vec::with_capacity(count);
            let mut packed = Vec::with_capacity(count);
            for frame in &track.frames {
                let (value, bytes, decoded) = match kind {
                    ChannelKind::Rotation => {
                        let q = frame.rotation.normalize();
                        let bytes = compress_quat(q);
                        (Vec4::from(q), bytes, Vec4::from(decompress_quat(&bytes)))
                    }
                    ChannelKind::Translation => {
                        let bytes = pack_vector(frame.translation, tmin, tmax);
                        (
                            frame.translation.extend(0.0),
                            bytes,
                            decompress_vec3(tmin, tmax, &bytes).extend(0.0),
                        )
                    }
                    ChannelKind::Scale => {
                        let bytes = pack_vector(frame.scale, smin, smax);
                        (
                            frame.scale.extend(0.0),
                            bytes,
                            decompress_vec3(smin, smax, &bytes).extend(0.0),
                        )
                    }
                };
                original.push(value);
                quantized.push(decoded);
                packed.push(bytes);
            }
            let selected = select_keys(
                &times,
                &original,
                &quantized,
                kind,
                tolerance,
                track.joint_hash,
                reduction_factor,
            )?;
            let mut keys: Vec<Key> = selected
                .into_iter()
                .map(|i| Key {
                    time: times[i],
                    bytes: packed[i],
                })
                .collect();
            if keys.len() > 1 {
                keys.push(*keys.last().expect("nonempty selected keys"));
            }
            if count == 1 {
                // Zero-duration playback does not consult the seek cache. Three copies shift
                // the constant into both interpolation slots when consuming from the start.
                keys.resize(3, keys[0]);
            }
            channels.push(Channel {
                joint: joint as u16,
                kind,
                keys,
            });
        }
    }

    // Each channel contributes its initial window, followed by keys sorted on their activation
    // time. This allows the single global read cursor to serve every channel without stalling.
    let mut entries = Vec::new();
    let mut seeds = Vec::with_capacity(channels.len());
    let mut pending = Vec::new();
    for (channel_id, channel) in channels.iter().enumerate() {
        let start = entries.len();
        let prologue = channel.keys.len().min(3);
        for key in &channel.keys[..prologue] {
            entries.push(Entry {
                key: *key,
                joint_and_kind: channel.joint | ((channel.kind as u16) << 14),
            });
        }
        seeds.push([
            start,
            start,
            start + (prologue - 1).min(1),
            start + prologue - 1,
        ]);
        for i in prologue..channel.keys.len() {
            pending.push((channel.keys[i - 2].time, channel_id, i));
        }
    }
    pending.sort_unstable();
    for (_, channel_id, i) in pending {
        let channel = &channels[channel_id];
        entries.push(Entry {
            key: channel.keys[i],
            joint_and_kind: channel.joint | ((channel.kind as u16) << 14),
        });
    }
    let wide = entries.len() > 65536;
    let frames_offset = 128u64;
    let cache_offset = frames_offset + entries.len() as u64 * 10;
    let hash_offset = cache_offset + anim.tracks.len() as u64 * if wide { 48 } else { 24 };
    let size = hash_offset + anim.tracks.len() as u64 * 4;
    if size > i32::MAX as u64 || entries.len() > i32::MAX as usize {
        return Err(Error::Unsupported(
            "compressed animation exceeds signed 32-bit section limits",
        ));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    bytes.write_all(b"r3d2canm").map_err(rs_io::Error::from)?;
    bytes.write_u32(1)?;
    bytes.write_u32((size - 12) as u32)?;
    bytes.write_all(b"canm").map_err(rs_io::Error::from)?;
    bytes.write_u32(7)?; // Cubic interpolation with keyframe-time parametrization.
    bytes.write_i32(anim.tracks.len() as i32)?;
    bytes.write_i32(entries.len() as i32)?;
    bytes.write_i32(1)?; // One complete seek cache at time zero; all seeks can start here.
    bytes.write_f32(duration)?;
    bytes.write_f32(output_fps)?;
    // Conventional encoder metadata found in shipped files. Actual accuracy is controlled by
    // CompressionOptions and verified against decoded samples, not these informational fields.
    for metric in [2.0, 10.0, 2.0, 10.0, 0.01, 0.2] {
        bytes.write_f32(metric)?;
    }
    for bounds in [tmin, tmax, smin, smax] {
        bytes.write_vec3(bounds)?;
    }
    for offset in [frames_offset, cache_offset, hash_offset] {
        bytes.write_i32((offset - 12) as i32)?;
    }
    for entry in entries {
        bytes.write_u16(entry.key.time)?;
        bytes.write_u16(entry.joint_and_kind)?;
        bytes
            .write_all(&entry.key.bytes)
            .map_err(rs_io::Error::from)?;
    }
    for seed in seeds {
        for index in seed {
            if wide {
                bytes.write_u32(index as u32)?;
            } else {
                bytes.write_u16(index as u16)?;
            }
        }
    }
    for track in &anim.tracks {
        bytes.write_u32(track.joint_hash)?;
    }
    debug_assert_eq!(bytes.len(), size as usize);
    // Check the finished file through playback, including the shared cursor and seek seeds.
    // This also catches precision effects at selected knots, not just between them.
    let mut reader = std::io::Cursor::new(&bytes);
    reader.set_position(12);
    let decoded =
        crate::compressed::Compressed::from_reader(&mut reader)?.bake_at_rate(anim.fps)?;
    for (source, output) in anim.tracks.iter().zip(&decoded) {
        if output.frames.len() != count {
            return Err(Error::InvalidData(
                "compressed animation sample count changed",
            ));
        }
        for (frame, (a, b)) in source.frames.iter().zip(&output.frames).enumerate() {
            for (kind, expected, actual, tolerance) in [
                (
                    ChannelKind::Rotation,
                    Vec4::from(a.rotation.normalize()),
                    Vec4::from(b.rotation),
                    options.rotation_error,
                ),
                (
                    ChannelKind::Translation,
                    a.translation.extend(0.0),
                    b.translation.extend(0.0),
                    options.translation_error,
                ),
                (
                    ChannelKind::Scale,
                    a.scale.extend(0.0),
                    b.scale.extend(0.0),
                    options.scale_error,
                ),
            ] {
                let error = kind.error(expected, actual);
                if !actual.is_finite() || !error.is_finite() || error > tolerance {
                    return Err(tolerance_error(
                        source.joint_hash,
                        kind,
                        frame,
                        error,
                        tolerance,
                    ));
                }
            }
        }
    }
    Ok(bytes)
}

#[path = "compressed_adaptive.rs"]
pub(crate) mod adaptive;
