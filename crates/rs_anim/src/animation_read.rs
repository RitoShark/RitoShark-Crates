use std::io::{Read, Seek, SeekFrom};

use rs_io::{Parse, ReaderExt};
use rs_math::{Quat, Vec3};

use crate::animation::{AnimFrame, AnimTrack, Animation};
use crate::quantized::{decompress_quat, decompress_vec3};
use crate::{Error, Result};

#[derive(Clone, Copy, Default)]
struct SparsePose {
    rotation: Option<Quat>,
    translation: Option<Vec3>,
    scale: Option<Vec3>,
}

fn elf(name: &str) -> u32 {
    let mut hash: u32 = 0;
    for &b in name.as_bytes() {
        hash = (hash << 4).wrapping_add(b as u32);
        let high = hash & 0xF000_0000;
        if high != 0 {
            hash ^= high >> 24;
        }
        hash &= !high;
    }
    hash
}

fn section_count(size: i32, element: i32) -> usize {
    if size <= 0 || element <= 0 {
        0
    } else {
        (size / element) as usize
    }
}

impl Animation {
    fn read_uncompressed<R: Read + Seek>(reader: &mut R, version: u32) -> Result<Self> {
        match version {
            5 => Self::read_v5(reader),
            4 => Self::read_v4(reader),
            3 => Self::read_v3(reader),
            _ => Err(Error::UnsupportedVersion(version)),
        }
    }

    fn read_v5<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let _resource_size = reader.read_u32()?;
        let _format_token = reader.read_u32()?;
        let _flags1 = reader.read_u32()?;
        let _flags2 = reader.read_u32()?;

        let _track_count = reader.read_u32()? as usize;
        let frame_count = reader.read_u32()? as usize;
        let frame_duration = reader.read_f32()?;
        let fps = if frame_duration != 0.0 {
            1.0 / frame_duration
        } else {
            0.0
        };

        let joint_hashes_offset = reader.read_i32()?;
        let _asset_name_offset = reader.read_i32()?;
        let _time_offset = reader.read_i32()?;
        let vecs_offset = reader.read_i32()?;
        let quats_offset = reader.read_i32()?;
        let frames_offset = reader.read_i32()?;

        if joint_hashes_offset <= 0 || vecs_offset <= 0 || quats_offset <= 0 || frames_offset <= 0 {
            return Err(Error::Unsupported("anm v5 missing data section"));
        }

        let joint_hash_count = section_count(frames_offset - joint_hashes_offset, 4);
        let vec_count = section_count(quats_offset - vecs_offset, 12);
        let quat_count = section_count(joint_hashes_offset - quats_offset, 6);

        reader
            .seek(SeekFrom::Start(joint_hashes_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        let mut joint_hashes = Vec::with_capacity(joint_hash_count);
        for _ in 0..joint_hash_count {
            joint_hashes.push(reader.read_u32()?);
        }

        reader
            .seek(SeekFrom::Start(vecs_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        let mut vecs = Vec::with_capacity(vec_count);
        for _ in 0..vec_count {
            vecs.push(reader.read_vec3()?);
        }

        reader
            .seek(SeekFrom::Start(quats_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        let mut quats = Vec::with_capacity(quat_count);
        for _ in 0..quat_count {
            let bytes = reader.read_array::<6>()?;
            quats.push(decompress_quat(&bytes).normalize());
        }

        let mut tracks: Vec<AnimTrack> = joint_hashes
            .iter()
            .map(|&h| AnimTrack {
                joint_hash: h,
                frames: Vec::with_capacity(frame_count),
            })
            .collect();

        reader
            .seek(SeekFrom::Start(frames_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        for frame_id in 0..frame_count {
            let time = frame_id as f32 * frame_duration;
            for track in tracks.iter_mut() {
                let translate_id = reader.read_u16()? as usize;
                let scale_id = reader.read_u16()? as usize;
                let rotate_id = reader.read_u16()? as usize;
                track.frames.push(AnimFrame {
                    time,
                    rotation: quats.get(rotate_id).copied().unwrap_or(Quat::IDENTITY),
                    translation: vecs.get(translate_id).copied().unwrap_or(Vec3::ZERO),
                    scale: vecs.get(scale_id).copied().unwrap_or(Vec3::ONE),
                });
            }
        }

        Ok(Self { fps, tracks })
    }

    fn read_v4<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let _resource_size = reader.read_u32()?;
        let _format_token = reader.read_u32()?;
        let _flags1 = reader.read_u32()?;
        let _flags2 = reader.read_u32()?;

        let track_count = reader.read_u32()? as usize;
        let frame_count = reader.read_u32()? as usize;
        let frame_duration = reader.read_f32()?;
        let fps = if frame_duration != 0.0 {
            1.0 / frame_duration
        } else {
            0.0
        };

        let _joint_hashes_offset = reader.read_i32()?;
        let _asset_name_offset = reader.read_i32()?;
        let _time_offset = reader.read_i32()?;
        let vecs_offset = reader.read_i32()?;
        let quats_offset = reader.read_i32()?;
        let frames_offset = reader.read_i32()?;

        if vecs_offset <= 0 || quats_offset <= 0 || frames_offset <= 0 {
            return Err(Error::Unsupported("anm v4 missing data section"));
        }

        let vec_count = section_count(quats_offset - vecs_offset, 12);
        let quat_count = section_count(frames_offset - quats_offset, 16);

        reader
            .seek(SeekFrom::Start(vecs_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        let mut vecs = Vec::with_capacity(vec_count);
        for _ in 0..vec_count {
            vecs.push(reader.read_vec3()?);
        }

        reader
            .seek(SeekFrom::Start(quats_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        let mut quats = Vec::with_capacity(quat_count);
        for _ in 0..quat_count {
            quats.push(reader.read_quat()?);
        }

        let mut tracks: Vec<AnimTrack> = Vec::with_capacity(track_count);
        let mut hash_to_index: std::collections::HashMap<u32, usize> =
            std::collections::HashMap::with_capacity(track_count);

        reader
            .seek(SeekFrom::Start(frames_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        for frame_id in 0..frame_count {
            let time = frame_id as f32 * frame_duration;
            for _ in 0..track_count {
                let joint_hash = reader.read_u32()?;
                let translate_id = reader.read_u16()? as usize;
                let scale_id = reader.read_u16()? as usize;
                let rotate_id = reader.read_u16()? as usize;
                let _pad = reader.read_u16()?;

                let index = *hash_to_index.entry(joint_hash).or_insert_with(|| {
                    tracks.push(AnimTrack {
                        joint_hash,
                        frames: Vec::with_capacity(frame_count),
                    });
                    tracks.len() - 1
                });

                tracks[index].frames.push(AnimFrame {
                    time,
                    rotation: quats.get(rotate_id).copied().unwrap_or(Quat::IDENTITY),
                    translation: vecs.get(translate_id).copied().unwrap_or(Vec3::ZERO),
                    scale: vecs.get(scale_id).copied().unwrap_or(Vec3::ONE),
                });
            }
        }

        Ok(Self { fps, tracks })
    }

    fn read_v3<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let _skeleton_id = reader.read_u32()?;
        let track_count = reader.read_u32()? as usize;
        let frame_count = reader.read_u32()? as usize;
        let fps = reader.read_u32()? as f32;
        let frame_duration = if fps != 0.0 { 1.0 / fps } else { 0.0 };

        let mut tracks = Vec::with_capacity(track_count);
        for _ in 0..track_count {
            let name = reader.read_fixed_string::<32>()?;
            let joint_hash = elf(&name);
            let _flags = reader.read_u32()?;

            let mut frames = Vec::with_capacity(frame_count);
            for frame_id in 0..frame_count {
                let rotation = reader.read_quat()?;
                let translation = reader.read_vec3()?;
                frames.push(AnimFrame {
                    time: frame_id as f32 * frame_duration,
                    rotation,
                    translation,
                    scale: Vec3::ONE,
                });
            }
            tracks.push(AnimTrack { joint_hash, frames });
        }

        Ok(Self { fps, tracks })
    }
}

fn sample_quat(keys: &[(f32, Quat)], frame: f32) -> Option<Quat> {
    sample_keyed(keys, frame, |a, b, t| a.slerp(b, t))
}

fn sample_vec3(keys: &[(f32, Vec3)], frame: f32) -> Option<Vec3> {
    sample_keyed(keys, frame, |a, b, t| a.lerp(b, t))
}

fn sample_keyed<T: Copy>(
    keys: &[(f32, T)],
    frame: f32,
    interp: impl Fn(T, T, f32) -> T,
) -> Option<T> {
    if keys.is_empty() {
        return None;
    }
    let mut left: Option<(f32, T)> = None;
    let mut right: Option<(f32, T)> = None;
    for &(time, value) in keys {
        if time <= frame {
            left = Some((time, value));
        }
        if time >= frame && right.is_none() {
            right = Some((time, value));
        }
    }
    match (left, right) {
        (Some((lt, lv)), Some((rt, rv))) => {
            if rt <= lt {
                Some(lv)
            } else {
                Some(interp(lv, rv, (frame - lt) / (rt - lt)))
            }
        }
        (Some((_, lv)), None) => Some(lv),
        (None, Some((_, rv))) => Some(rv),
        (None, None) => None,
    }
}

impl Animation {
    fn read_compressed<R: Read + Seek>(reader: &mut R, version: u32) -> Result<Self> {
        if !matches!(version, 1..=3) {
            return Err(Error::UnsupportedVersion(version));
        }

        let _resource_size = reader.read_u32()?;
        let _format_token = reader.read_u32()?;
        let _flags = reader.read_u32()?;

        let joint_count = reader.read_i32()?;
        let frame_count = reader.read_i32()?;
        let _jump_cache_count = reader.read_i32()?;

        let max_time = reader.read_f32()?;
        let fps = reader.read_f32()?;

        for _ in 0..6 {
            let _error_metric = reader.read_f32()?;
        }

        let translation_min = reader.read_vec3()?;
        let translation_max = reader.read_vec3()?;
        let scale_min = reader.read_vec3()?;
        let scale_max = reader.read_vec3()?;

        let frames_offset = reader.read_i32()?;
        let _jump_caches_offset = reader.read_i32()?;
        let joint_hashes_offset = reader.read_i32()?;

        if joint_count <= 0 || frame_count <= 0 {
            return Err(Error::Unsupported("compressed anm has no joints or frames"));
        }
        if frames_offset <= 0 || joint_hashes_offset <= 0 {
            return Err(Error::Unsupported("compressed anm missing data section"));
        }
        let joint_count = joint_count as usize;
        let frame_count = frame_count as usize;

        reader
            .seek(SeekFrom::Start(joint_hashes_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        let mut joint_hashes = Vec::with_capacity(joint_count);
        for _ in 0..joint_count {
            joint_hashes.push(reader.read_u32()?);
        }

        let mut sparse: Vec<std::collections::BTreeMap<u32, SparsePose>> =
            vec![std::collections::BTreeMap::new(); joint_count];

        reader
            .seek(SeekFrom::Start(frames_offset as u64 + 12))
            .map_err(rs_io::Error::from)?;
        for _ in 0..frame_count {
            let compressed_time = reader.read_u16()?;
            let bits = reader.read_u16()?;
            let transform = reader.read_array::<6>()?;

            let joint_id = (bits & 0x3FFF) as usize;
            if joint_id >= joint_count {
                continue;
            }
            let time_in_frames = (compressed_time as f32 / 65535.0) * max_time * fps;
            let key = (time_in_frames * 256.0) as u32;
            let pose = sparse[joint_id].entry(key).or_default();

            match bits >> 14 {
                0 => pose.rotation = Some(decompress_quat(&transform).normalize()),
                1 => {
                    pose.translation = Some(decompress_vec3(
                        translation_min,
                        translation_max,
                        &transform,
                    ))
                }
                2 => pose.scale = Some(decompress_vec3(scale_min, scale_max, &transform)),
                _ => return Err(Error::Unsupported("compressed anm unknown transform type")),
            }
        }

        let out_frames = ((max_time * fps).round() as usize + 1).max(1);
        let mut tracks = Vec::with_capacity(joint_count);
        for (joint_id, poses) in sparse.into_iter().enumerate() {
            let mut rot_keys = Vec::new();
            let mut tr_keys = Vec::new();
            let mut sc_keys = Vec::new();
            for (&key, pose) in poses.iter() {
                let time = key as f32 / 256.0;
                if let Some(r) = pose.rotation {
                    rot_keys.push((time, r));
                }
                if let Some(t) = pose.translation {
                    tr_keys.push((time, t));
                }
                if let Some(s) = pose.scale {
                    sc_keys.push((time, s));
                }
            }

            let frame_duration = if fps != 0.0 { 1.0 / fps } else { 0.0 };
            let mut frames = Vec::with_capacity(out_frames);
            for f in 0..out_frames {
                let frame = f as f32;
                frames.push(AnimFrame {
                    time: frame * frame_duration,
                    rotation: sample_quat(&rot_keys, frame).unwrap_or(Quat::IDENTITY),
                    translation: sample_vec3(&tr_keys, frame).unwrap_or(Vec3::ZERO),
                    scale: sample_vec3(&sc_keys, frame).unwrap_or(Vec3::ONE),
                });
            }

            tracks.push(AnimTrack {
                joint_hash: joint_hashes[joint_id],
                frames,
            });
        }

        Ok(Self { fps, tracks })
    }
}

impl Parse for Animation {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_array::<8>()?;
        let version = reader.read_u32()?;
        match &magic {
            b"r3d2anmd" => Self::read_uncompressed(reader, version),
            b"r3d2canm" => Self::read_compressed(reader, version),
            _ => Err(Error::InvalidMagic(magic)),
        }
    }
}
