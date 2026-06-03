use std::io::{Cursor, Seek, SeekFrom, Write};

use rs_io::{Serialize, WriterExt};

use crate::skeleton::{Joint, Skeleton};
use crate::{Error, Result};

const JOINTS_OFFSET: usize = 64;
const JOINT_RECORD_SIZE: usize = 100;
const JOINT_INDEX_SIZE: usize = 8;

impl Skeleton {
    fn write_to(&self, buf: &mut Cursor<Vec<u8>>) -> Result<()> {
        let joint_count = self.joints.len();
        let influence_count = self.influences.len();

        buf.write_u32(0)?; // file size, patched last
        buf.write_u32(Self::MAGIC)?;
        buf.write_u32(0)?; // version

        buf.write_u16(self.flags)?;
        buf.write_u16(joint_count as u16)?;
        buf.write_u32(influence_count as u32)?;

        let joint_indices_offset = JOINTS_OFFSET + joint_count * JOINT_RECORD_SIZE;
        let influences_offset = joint_indices_offset + joint_count * JOINT_INDEX_SIZE;
        let joint_names_offset = influences_offset + influence_count * 2;

        buf.write_i32(JOINTS_OFFSET as i32)?;
        buf.write_i32(joint_indices_offset as i32)?;
        buf.write_i32(influences_offset as i32)?;

        let name_offset_pos = buf.stream_position().map_err(rs_io::Error::from)?;
        buf.write_i32(0)?; // skeleton name offset, patched if present
        let asset_offset_pos = buf.stream_position().map_err(rs_io::Error::from)?;
        buf.write_i32(0)?; // asset name offset, patched if present
        buf.write_i32(joint_names_offset as i32)?;
        for _ in 0..5 {
            buf.write_u32(0xFFFF_FFFF)?;
        }

        buf.seek(SeekFrom::Start(joint_names_offset as u64))
            .map_err(rs_io::Error::from)?;
        let mut joint_name_offsets = Vec::with_capacity(joint_count);
        for joint in &self.joints {
            joint_name_offsets.push(buf.stream_position().map_err(rs_io::Error::from)?);
            buf.write_cstring(&joint.name)?;
        }

        buf.seek(SeekFrom::Start(JOINTS_OFFSET as u64))
            .map_err(rs_io::Error::from)?;
        for (joint, &name_off) in self.joints.iter().zip(joint_name_offsets.iter()) {
            write_joint(buf, joint, name_off)?;
        }

        buf.seek(SeekFrom::Start(influences_offset as u64))
            .map_err(rs_io::Error::from)?;
        for &inf in &self.influences {
            buf.write_u16(inf)?;
        }

        buf.seek(SeekFrom::Start(joint_indices_offset as u64))
            .map_err(rs_io::Error::from)?;
        let mut hash_ids: Vec<(i16, u32)> = self.joints.iter().map(|j| (j.id, j.hash)).collect();
        hash_ids.sort_by(|a, b| b.1.cmp(&a.1));
        for (id, hash) in hash_ids {
            buf.write_i16(id)?;
            buf.write_i16(0)?;
            buf.write_u32(hash)?;
        }

        let mut name_off = 0i32;
        let mut asset_off = 0i32;
        if !self.name.is_empty() {
            let end = buf.seek(SeekFrom::End(0)).map_err(rs_io::Error::from)?;
            name_off = end as i32;
            buf.write_cstring(&self.name)?;
        }
        if !self.asset.is_empty() {
            let end = buf.seek(SeekFrom::End(0)).map_err(rs_io::Error::from)?;
            asset_off = end as i32;
            buf.write_cstring(&self.asset)?;
        }

        let file_size = buf.seek(SeekFrom::End(0)).map_err(rs_io::Error::from)? as u32;

        if name_off != 0 {
            buf.seek(SeekFrom::Start(name_offset_pos))
                .map_err(rs_io::Error::from)?;
            buf.write_i32(name_off)?;
        }
        if asset_off != 0 {
            buf.seek(SeekFrom::Start(asset_offset_pos))
                .map_err(rs_io::Error::from)?;
            buf.write_i32(asset_off)?;
        }
        buf.seek(SeekFrom::Start(0)).map_err(rs_io::Error::from)?;
        buf.write_u32(file_size)?;
        Ok(())
    }
}

fn write_joint(buf: &mut Cursor<Vec<u8>>, joint: &Joint, name_off: u64) -> Result<()> {
    buf.write_u16(joint.flags)?;
    buf.write_i16(joint.id)?;
    buf.write_i16(joint.parent_id)?;
    buf.write_u16(0)?; // pad
    buf.write_u32(joint.hash)?;
    buf.write_f32(joint.radius)?;

    buf.write_vec3(joint.local_translation)?;
    buf.write_vec3(joint.local_scale)?;
    buf.write_quat(joint.local_rotation)?;

    buf.write_vec3(joint.inverse_bind_translation)?;
    buf.write_vec3(joint.inverse_bind_scale)?;
    buf.write_quat(joint.inverse_bind_rotation)?;

    let pos = buf.stream_position().map_err(rs_io::Error::from)?;
    buf.write_i32((name_off as i64 - pos as i64) as i32)?;
    Ok(())
}

impl Serialize for Skeleton {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        let mut buf = Cursor::new(Vec::new());
        self.write_to(&mut buf)?;
        writer
            .write_all(&buf.into_inner())
            .map_err(rs_io::Error::from)?;
        Ok(())
    }
}
