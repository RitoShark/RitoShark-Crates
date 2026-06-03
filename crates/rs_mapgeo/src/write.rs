/*!
The writer is the byte-exact inverse of the version 17 reader for the top-level structure it
parses: header, texture overrides, vertex declarations, vertex/index buffers, and the model list.
It reproduces the same field order and padding, so a parsed file re-serializes identically up to
the model list. It does not emit the bucketed scene graph or planar reflectors, which the reader
does not consume either. Any non-target version is rejected with `Error::UnsupportedVersion`.
*/

use std::io::Write;

use rs_io::WriterExt;
use rs_math::{Vec2, Vec3};

use crate::error::{Error, Result};
use crate::mapgeo::{AssetChannel, MapGeometry, MapModel, VertexDescription};

const TARGET_VERSION: u32 = 17;
const MAX_VERTEX_ELEMENTS: usize = 15;

impl MapGeometry {
    pub fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        if self.version != TARGET_VERSION {
            return Err(Error::UnsupportedVersion(self.version));
        }

        writer.write_bytes(MapGeometry::magic())?;
        writer.write_u32(self.version)?;

        writer.write_u32(self.texture_overrides.len() as u32)?;
        for ov in &self.texture_overrides {
            writer.write_u32(ov.index)?;
            writer.write_string_u32(&ov.path)?;
        }

        writer.write_u32(self.vertex_descriptions.len() as u32)?;
        for desc in &self.vertex_descriptions {
            write_vertex_description(writer, desc)?;
        }

        writer.write_u32(self.vertex_buffers.len() as u32)?;
        for vb in &self.vertex_buffers {
            writer.write_u8(vb.layer)?;
            writer.write_u32(vb.data.len() as u32)?;
            writer.write_bytes(&vb.data)?;
        }

        writer.write_u32(self.index_buffers.len() as u32)?;
        for ib in &self.index_buffers {
            writer.write_u8(ib.layer)?;
            writer.write_u32((ib.indices.len() * 2) as u32)?;
            for &index in &ib.indices {
                writer.write_u16(index)?;
            }
        }

        writer.write_u32(self.models.len() as u32)?;
        for model in &self.models {
            write_model(writer, model)?;
        }

        Ok(())
    }
}

fn write_vertex_description<W: Write>(writer: &mut W, desc: &VertexDescription) -> Result<()> {
    let element_count = desc.elements.len();
    if element_count > MAX_VERTEX_ELEMENTS {
        return Err(Error::Unsupported("vertex declaration element count > 15"));
    }

    writer.write_u32(desc.usage as u32)?;
    writer.write_u32(element_count as u32)?;
    for element in &desc.elements {
        writer.write_u32(element.name as u32)?;
        writer.write_u32(element.format as u32)?;
    }

    let padding = (MAX_VERTEX_ELEMENTS - element_count) * 8;
    writer.write_bytes(&vec![0u8; padding])?;
    Ok(())
}

fn write_model<W: Write>(writer: &mut W, model: &MapModel) -> Result<()> {
    writer.write_u32(model.vertex_count)?;
    writer.write_u32(model.vertex_buffer_ids.len() as u32)?;
    writer.write_u32(model.vertex_description_id)?;
    for &id in &model.vertex_buffer_ids {
        writer.write_i32(id)?;
    }

    writer.write_u32(model.index_count)?;
    writer.write_i32(model.index_buffer_id)?;

    writer.write_u8(model.layer)?;
    writer.write_u32(model.bucket_grid_hash)?;

    writer.write_u32(model.submeshes.len() as u32)?;
    for submesh in &model.submeshes {
        writer.write_u32(submesh.hash)?;
        writer.write_string_u32(&submesh.name)?;
        writer.write_u32(submesh.index_start)?;
        writer.write_u32(submesh.index_count)?;
        writer.write_u32(submesh.min_vertex)?;
        writer.write_u32(submesh.max_vertex)?;
    }

    writer.write_bool(model.disable_backface_culling)?;

    write_vec3(writer, model.bounds.min)?;
    write_vec3(writer, model.bounds.max)?;
    writer.write_mtx44(&model.transform.to_cols_array())?;

    writer.write_u8(model.quality)?;
    writer.write_bool(model.is_bush)?;
    writer.write_u16(model.render_flags)?;

    write_channel(writer, &model.baked_light)?;
    write_channel(writer, &model.stationary_light)?;

    writer.write_u32(model.texture_overrides.len() as u32)?;
    for ov in &model.texture_overrides {
        writer.write_u32(ov.index)?;
        writer.write_string_u32(&ov.path)?;
    }
    for &v in &model.baked_paint_scale_offset {
        writer.write_f32(v)?;
    }

    Ok(())
}

fn write_channel<W: Write>(writer: &mut W, channel: &AssetChannel) -> Result<()> {
    writer.write_string_u32(&channel.path)?;
    write_vec2(writer, channel.scale)?;
    write_vec2(writer, channel.offset)?;
    Ok(())
}

fn write_vec2<W: Write>(writer: &mut W, v: Vec2) -> Result<()> {
    writer.write_f32(v.x)?;
    writer.write_f32(v.y)?;
    Ok(())
}

fn write_vec3<W: Write>(writer: &mut W, v: Vec3) -> Result<()> {
    writer.write_f32(v.x)?;
    writer.write_f32(v.y)?;
    writer.write_f32(v.z)?;
    Ok(())
}
