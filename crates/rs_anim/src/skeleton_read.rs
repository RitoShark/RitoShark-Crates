use std::io::{Cursor, Read, Seek, SeekFrom};

use rs_io::{Parse, ReaderExt};

use crate::raw::check_range;
use crate::skeleton::{Joint, RawSkeleton, Skeleton};
use crate::{Error, Result};
use rs_math::Mat4;

fn read_name_at<R: Read + Seek>(reader: &mut R, abs_offset: u64) -> Result<String> {
    let here = reader.stream_position().map_err(rs_io::Error::from)?;
    reader
        .seek(SeekFrom::Start(abs_offset))
        .map_err(rs_io::Error::from)?;
    let name = reader.read_cstring()?;
    reader
        .seek(SeekFrom::Start(here))
        .map_err(rs_io::Error::from)?;
    Ok(name)
}

impl Skeleton {
    fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let file_size = reader.read_u32()?;
        let magic = reader.read_u32()?;
        if magic != Self::MAGIC {
            let mut bytes = [0u8; 8];
            bytes[0..4].copy_from_slice(&file_size.to_le_bytes());
            bytes[4..8].copy_from_slice(&magic.to_le_bytes());
            return Err(Error::InvalidMagic(bytes));
        }
        let version = reader.read_u32()?;
        if version != 0 {
            return Err(Error::UnsupportedVersion(version));
        }

        let flags = reader.read_u16()?;
        let joint_count = reader.read_u16()? as usize;
        let influence_count = reader.read_u32()? as usize;

        let joints_offset = reader.read_i32()?;
        let _joint_indices_offset = reader.read_i32()?;
        let influences_offset = reader.read_i32()?;
        let name_offset = reader.read_i32()?;
        let asset_offset = reader.read_i32()?;
        let _bone_names_offset = reader.read_i32()?;
        for _ in 0..5 {
            let _reserved = reader.read_i32()?;
        }

        if joint_count > 0 {
            if joints_offset <= 0 {
                return Err(Error::InvalidData("missing skeleton joints"));
            }
            check_range(reader, joints_offset as u64, joint_count, 100)?;
        }
        if influence_count > 0 {
            if influences_offset <= 0 {
                return Err(Error::InvalidData("missing skeleton influences"));
            }
            check_range(reader, influences_offset as u64, influence_count, 2)?;
        }
        let mut joints = Vec::with_capacity(joint_count);
        if joints_offset > 0 && joint_count > 0 {
            reader
                .seek(SeekFrom::Start(joints_offset as u64))
                .map_err(rs_io::Error::from)?;
            for _ in 0..joint_count {
                joints.push(read_joint(reader)?);
            }
        }

        let mut influences = Vec::with_capacity(influence_count);
        if influences_offset > 0 && influence_count > 0 {
            reader
                .seek(SeekFrom::Start(influences_offset as u64))
                .map_err(rs_io::Error::from)?;
            for _ in 0..influence_count {
                influences.push(reader.read_u16()?);
            }
        }

        let name = if name_offset > 0 {
            read_name_at(reader, name_offset as u64)?
        } else {
            String::new()
        };
        let asset = if asset_offset > 0 {
            read_name_at(reader, asset_offset as u64)?
        } else {
            String::new()
        };

        Ok(Self {
            flags,
            name,
            asset,
            joints,
            influences,
            raw: None,
        })
    }

    fn read_legacy<R: Read + Seek>(reader: &mut R, version: u32) -> Result<Self> {
        if !matches!(version, 1 | 2) {
            return Err(Error::UnsupportedVersion(version));
        }
        let _skeleton_id = reader.read_u32()?;
        let count = reader.read_u32()? as usize;
        if count > i16::MAX as usize + 1 {
            return Err(Error::Unsupported(
                "legacy skeleton exceeds 32768 joint ids",
            ));
        }
        check_range(reader, 20, count, 88)?;
        let mut skeleton = Self::new();
        let mut globals: Vec<Mat4> = Vec::with_capacity(count);
        for index in 0..count {
            let name = reader.read_fixed_string::<32>()?;
            let parent = reader.read_i32()?;
            if parent < -1 || parent >= index as i32 {
                return Err(Error::InvalidData(
                    "legacy skeleton parent must precede its child",
                ));
            }
            let radius = reader.read_f32()?;
            let mut columns = Mat4::IDENTITY.to_cols_array_2d();
            for row in 0..3 {
                for column in &mut columns {
                    column[row] = reader.read_f32()?;
                }
            }
            let global = Mat4::from_cols_array_2d(&columns);
            if !global.is_finite() || global.determinant() == 0.0 {
                return Err(Error::InvalidData(
                    "legacy skeleton has a singular bind transform",
                ));
            }
            let inverse = global.inverse();
            let local = if parent == -1 {
                global
            } else {
                globals[parent as usize].inverse() * global
            };
            let (local_scale, local_rotation, local_translation) =
                local.to_scale_rotation_translation();
            let (inverse_bind_scale, inverse_bind_rotation, inverse_bind_translation) =
                inverse.to_scale_rotation_translation();
            skeleton.joints.push(Joint {
                hash: rs_hash::elf_lower(&name),
                name,
                flags: 0,
                id: index as i16,
                parent_id: parent as i16,
                radius,
                local_translation,
                local_scale,
                local_rotation,
                inverse_bind_translation,
                inverse_bind_scale,
                inverse_bind_rotation,
            });
            globals.push(global);
        }
        if version == 1 {
            skeleton.influences = (0..count as u16).collect();
        } else {
            let influence_count = reader.read_u32()?;
            let offset = reader.stream_position().map_err(rs_io::Error::from)?;
            check_range(reader, offset, influence_count as usize, 4)?;
            for _ in 0..influence_count {
                let influence = reader.read_u32()?;
                if influence as usize >= count {
                    return Err(Error::InvalidData(
                        "legacy skeleton influence is out of range",
                    ));
                }
                skeleton.influences.push(influence as u16);
            }
        }
        Ok(skeleton)
    }
}

fn read_joint<R: Read + Seek>(reader: &mut R) -> Result<Joint> {
    let flags = reader.read_u16()?;
    let id = reader.read_i16()?;
    let parent_id = reader.read_i16()?;
    let _pad = reader.read_u16()?;
    let hash = reader.read_u32()?;
    let radius = reader.read_f32()?;

    let local_translation = reader.read_vec3()?;
    let local_scale = reader.read_vec3()?;
    let local_rotation = reader.read_quat()?;

    let inverse_bind_translation = reader.read_vec3()?;
    let inverse_bind_scale = reader.read_vec3()?;
    let inverse_bind_rotation = reader.read_quat()?;

    let name_offset = reader.read_i32()?;
    let return_pos = reader.stream_position().map_err(rs_io::Error::from)?;
    let name_abs = (return_pos as i64 - 4 + name_offset as i64) as u64;
    let name = read_name_at(reader, name_abs)?;
    reader
        .seek(SeekFrom::Start(return_pos))
        .map_err(rs_io::Error::from)?;

    Ok(Joint {
        name,
        flags,
        id,
        parent_id,
        radius,
        hash,
        local_translation,
        local_scale,
        local_rotation,
        inverse_bind_translation,
        inverse_bind_scale,
        inverse_bind_rotation,
    })
}

impl Parse for Skeleton {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map_err(rs_io::Error::from)?;
        let reader = &mut Cursor::new(&bytes);
        reader
            .seek(SeekFrom::Start(4))
            .map_err(rs_io::Error::from)?;
        let magic = reader.read_u32()?;
        reader
            .seek(SeekFrom::Start(0))
            .map_err(rs_io::Error::from)?;
        let skeleton = if magic == Self::MAGIC {
            Self::read(reader)
        } else {
            let signature = reader.read_byte_array::<8>()?;
            if &signature == b"r3d2sklt" {
                let version = reader.read_u32()?;
                Self::read_legacy(reader, version)
            } else {
                Err(Error::InvalidMagic(signature))
            }
        }?;
        let raw = RawSkeleton {
            bytes,
            decoded: skeleton.clone(),
        };
        Ok(Self {
            raw: Some(Box::new(raw)),
            ..skeleton
        })
    }
}
