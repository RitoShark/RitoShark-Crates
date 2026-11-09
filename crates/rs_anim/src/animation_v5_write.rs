//! Dense ANMD v5: float32 vectors, 48-bit rotations and frame-major palette indices.
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use rs_io::WriterExt;

use crate::{Animation, Error, Result, quantized::compress_quat};

/// Validate the sampled representation before a writer can silently discard timing or tracks.
pub(crate) fn validate_samples(anim: &Animation) -> Result<usize> {
    if !anim.fps.is_finite() || anim.fps <= 0.0 || !(1.0 / anim.fps).is_finite() {
        return Err(Error::InvalidData(
            "animation fps must be finite and positive",
        ));
    }
    let count = anim.frame_count();
    if anim.tracks.is_empty() || count == 0 {
        return Err(Error::InvalidData(
            "animation requires nonempty sampled tracks",
        ));
    }
    let dt = 1.0 / anim.fps;
    let mut hashes = HashSet::new();
    for track in &anim.tracks {
        if !hashes.insert(track.joint_hash) {
            return Err(Error::InvalidData("duplicate animation joint hash"));
        }
        if track.frames.len() != count {
            return Err(Error::InvalidData(
                "animation tracks must share a sample count",
            ));
        }
        for (index, frame) in track.frames.iter().enumerate() {
            let expected = index as f32 * dt;
            let epsilon = (expected.abs() * 4.0 * f32::EPSILON).max(1e-6);
            if !expected.is_finite()
                || !frame.time.is_finite()
                || (frame.time - expected).abs() > epsilon
            {
                return Err(Error::InvalidData(
                    "animation requires samples at index / fps, starting at zero",
                ));
            }
            if !frame.translation.is_finite()
                || !frame.scale.is_finite()
                || !frame.rotation.is_finite()
                || !frame.rotation.length_squared().is_finite()
                || frame.rotation.length_squared() < 1e-20
            {
                return Err(Error::InvalidData(
                    "animation contains a nonfinite transform or invalid quaternion",
                ));
            }
        }
    }
    Ok(count)
}

fn intern<const N: usize>(
    bytes: [u8; N],
    map: &mut HashMap<[u8; N], u16>,
    values: &mut Vec<[u8; N]>,
) -> Result<u16> {
    if let Some(&index) = map.get(&bytes) {
        return Ok(index);
    }
    let index = u16::try_from(values.len())
        .map_err(|_| Error::Unsupported("anm v5 palette exceeds 65536 entries"))?;
    values.push(bytes);
    map.insert(bytes, index);
    Ok(index)
}

impl Animation {
    /// Encode current samples as ANMD v5, ignoring retained source bytes without changing them.
    /// Translation/scale remain float32; normalized rotations use 48-bit smallest-three packing.
    /// All frames are retained. Both palettes allow 65536 entries (indices 0..=65535).
    /// Requires finite, uniformly sampled, equally sized nonempty tracks with unique hashes.
    pub fn to_v5_bytes(&self) -> Result<Vec<u8>> {
        let count = validate_samples(self)?;
        let tracks = u32::try_from(self.tracks.len())
            .map_err(|_| Error::Unsupported("anm v5 track count exceeds u32"))?;
        let frames = u32::try_from(count)
            .map_err(|_| Error::Unsupported("anm v5 frame count exceeds u32"))?;
        let row_bytes = u64::from(tracks)
            .checked_mul(u64::from(frames))
            .and_then(|n| n.checked_mul(6))
            .ok_or(Error::Unsupported("anm v5 frame size overflow"))?;
        if row_bytes > i32::MAX as u64 - 76 {
            return Err(Error::Unsupported(
                "anm v5 exceeds signed 32-bit section limits",
            ));
        }
        let (mut vectors, mut rotations) = (Vec::new(), Vec::new());
        let (mut vi, mut qi) = (HashMap::new(), HashMap::new());
        let mut rows = Vec::with_capacity(row_bytes as usize);
        for frame in 0..count {
            for track in &self.tracks {
                let sample = &track.frames[frame];
                for vector in [sample.translation, sample.scale] {
                    let mut bytes = [0; 12];
                    for (axis, value) in vector.to_array().iter().enumerate() {
                        bytes[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
                    }
                    rows.write_u16(intern(bytes, &mut vi, &mut vectors)?)?;
                }
                rows.write_u16(intern(
                    compress_quat(sample.rotation.normalize()),
                    &mut qi,
                    &mut rotations,
                )?)?;
            }
        }
        let vectors_offset = 76u64;
        let rotations_offset = vectors_offset + vectors.len() as u64 * 12;
        let hashes_offset = rotations_offset + rotations.len() as u64 * 6;
        let frames_offset = hashes_offset + u64::from(tracks) * 4;
        let size = frames_offset + row_bytes;
        if size > i32::MAX as u64 {
            return Err(Error::Unsupported(
                "anm v5 exceeds signed 32-bit section limits",
            ));
        }
        let mut bytes = Vec::with_capacity(size as usize);
        bytes.write_all(b"r3d2anmd").map_err(rs_io::Error::from)?;
        bytes.write_u32(5)?;
        bytes.write_u32((size - 12) as u32)?;
        bytes.write_u32(0)?; // Runtime format token is not required on disk.
        bytes.write_u32(0)?;
        bytes.write_u32(0)?;
        bytes.write_u32(tracks)?;
        bytes.write_u32(frames)?;
        bytes.write_f32(1.0 / self.fps)?;
        for offset in [
            hashes_offset - 12,
            0,
            0,
            vectors_offset - 12,
            rotations_offset - 12,
            frames_offset - 12,
        ] {
            bytes.write_i32(offset as i32)?;
        }
        bytes.write_all(&[0; 12]).map_err(rs_io::Error::from)?;
        for value in vectors {
            bytes.write_all(&value).map_err(rs_io::Error::from)?;
        }
        for value in rotations {
            bytes.write_all(&value).map_err(rs_io::Error::from)?;
        }
        for track in &self.tracks {
            bytes.write_u32(track.joint_hash)?;
        }
        bytes.write_all(&rows).map_err(rs_io::Error::from)?;
        debug_assert_eq!(bytes.len(), size as usize);
        Ok(bytes)
    }

    /// Finish validation and encoding before touching the destination writer.
    pub fn write_v5<W: Write>(&self, writer: &mut W) -> Result<()> {
        let bytes = self.to_v5_bytes()?;
        writer.write_all(&bytes).map_err(rs_io::Error::from)?;
        Ok(())
    }

    /// Finish validation and encoding before opening the destination file.
    pub fn to_v5_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = self.to_v5_bytes()?;
        std::fs::write(path, bytes).map_err(rs_io::Error::from)?;
        Ok(())
    }
}
