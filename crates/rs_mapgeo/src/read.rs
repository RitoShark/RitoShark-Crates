/*!
The reader targets OEGM version 17, the current shipping map-geometry format. It parses the full
top-level structure — texture overrides, vertex declarations, vertex/index buffer descriptions,
and the model list (each model's buffer references, submeshes, transform, bounding box, layer and
flags). Vertex buffers are kept as raw bytes plus their declaration so callers can decode them.
The trailing bucketed scene graph and planar reflectors are not part of this MVP; parsing stops
cleanly after the model list. Any other version returns `Error::UnsupportedVersion`.
*/

use std::io::{Read, Seek};

use rs_io::ReaderExt;
use rs_math::{Aabb, Vec2, Vec3};

use crate::error::{Error, Result};
use crate::mapgeo::{
    AssetChannel, ElementFormat, ElementName, IndexBuffer, MapGeometry, MapModel, Submesh,
    TextureOverride, VertexBuffer, VertexDescription, VertexElement, VertexUsage,
};

const TARGET_VERSION: u32 = 17;
const MAX_VERTEX_ELEMENTS: usize = 15;

impl MapGeometry {
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_array::<4>()?;
        if &magic != MapGeometry::magic() {
            return Err(Error::InvalidMagic);
        }

        let version = reader.read_u32()?;
        if version != TARGET_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }

        let texture_overrides = read_texture_overrides(reader)?;
        let vertex_descriptions = read_vertex_descriptions(reader)?;
        let vertex_buffers = read_vertex_buffers(reader)?;
        let index_buffers = read_index_buffers(reader)?;
        let models = read_models(reader)?;

        Ok(MapGeometry {
            version,
            texture_overrides,
            vertex_descriptions,
            vertex_buffers,
            index_buffers,
            models,
        })
    }
}

fn read_texture_overrides<R: Read>(reader: &mut R) -> Result<Vec<TextureOverride>> {
    let count = reader.read_u32()? as usize;
    let mut overrides = Vec::with_capacity(count);
    for _ in 0..count {
        let index = reader.read_u32()?;
        let path = reader.read_string_u32()?;
        overrides.push(TextureOverride { index, path });
    }
    Ok(overrides)
}

fn read_vertex_descriptions<R: Read>(reader: &mut R) -> Result<Vec<VertexDescription>> {
    let count = reader.read_u32()? as usize;
    let mut descriptions = Vec::with_capacity(count);
    for _ in 0..count {
        let usage = VertexUsage::from_u32(reader.read_u32()?);
        let element_count = reader.read_u32()? as usize;
        if element_count > MAX_VERTEX_ELEMENTS {
            return Err(Error::Unsupported("vertex declaration element count > 15"));
        }

        let mut elements = Vec::with_capacity(element_count);
        for _ in 0..element_count {
            let name_raw = reader.read_u32()?;
            let format_raw = reader.read_u32()?;
            let name =
                ElementName::from_u32(name_raw).ok_or(Error::Unsupported("vertex element name"))?;
            let format = ElementFormat::from_u32(format_raw)
                .ok_or(Error::Unsupported("vertex element format"))?;
            elements.push(VertexElement { name, format });
        }

        let padding = (MAX_VERTEX_ELEMENTS - element_count) * 8;
        reader.read_bytes(padding)?;

        descriptions.push(VertexDescription { usage, elements });
    }
    Ok(descriptions)
}

fn read_vertex_buffers<R: Read>(reader: &mut R) -> Result<Vec<VertexBuffer>> {
    let count = reader.read_u32()? as usize;
    let mut buffers = Vec::with_capacity(count);
    for _ in 0..count {
        let layer = reader.read_u8()?;
        let size = reader.read_u32()? as usize;
        let data = reader.read_bytes(size)?;
        buffers.push(VertexBuffer { layer, data });
    }
    Ok(buffers)
}

fn read_index_buffers<R: Read>(reader: &mut R) -> Result<Vec<IndexBuffer>> {
    let count = reader.read_u32()? as usize;
    let mut buffers = Vec::with_capacity(count);
    for _ in 0..count {
        let layer = reader.read_u8()?;
        let size = reader.read_u32()? as usize;
        let mut indices = Vec::with_capacity(size / 2);
        for _ in 0..size / 2 {
            indices.push(reader.read_u16()?);
        }
        buffers.push(IndexBuffer { layer, indices });
    }
    Ok(buffers)
}

fn read_models<R: Read>(reader: &mut R) -> Result<Vec<MapModel>> {
    let count = reader.read_u32()? as usize;
    let mut models = Vec::with_capacity(count);
    for id in 0..count {
        models.push(read_model(reader, id)?);
    }
    Ok(models)
}

fn read_model<R: Read>(reader: &mut R, id: usize) -> Result<MapModel> {
    let name = format!("MapGeo_Instance_{id}");

    let vertex_count = reader.read_u32()?;
    let vertex_buffer_count = reader.read_u32()? as usize;
    let vertex_description_id = reader.read_u32()?;

    let mut vertex_buffer_ids = Vec::with_capacity(vertex_buffer_count);
    for _ in 0..vertex_buffer_count {
        vertex_buffer_ids.push(reader.read_i32()?);
    }

    let index_count = reader.read_u32()?;
    let index_buffer_id = reader.read_i32()?;

    let layer = reader.read_u8()?;
    let bucket_grid_hash = reader.read_u32()?;

    let submesh_count = reader.read_u32()? as usize;
    let mut submeshes = Vec::with_capacity(submesh_count);
    for _ in 0..submesh_count {
        let hash = reader.read_u32()?;
        let name = reader.read_string_u32()?;
        let index_start = reader.read_u32()?;
        let index_count = reader.read_u32()?;
        let min_vertex = reader.read_u32()?;
        let max_vertex = reader.read_u32()?;
        submeshes.push(Submesh {
            hash,
            name,
            index_start,
            index_count,
            min_vertex,
            max_vertex,
        });
    }

    let disable_backface_culling = reader.read_bool()?;

    let bounds = Aabb::new(read_vec3(reader)?, read_vec3(reader)?);
    let transform = read_mat4(reader)?;

    let quality = reader.read_u8()?;
    let is_bush = reader.read_bool()?;
    let render_flags = reader.read_u16()?;

    let baked_light = read_channel(reader)?;
    let stationary_light = read_channel(reader)?;

    let model_texture_count = reader.read_u32()? as usize;
    let mut texture_overrides = Vec::with_capacity(model_texture_count);
    for _ in 0..model_texture_count {
        let index = reader.read_u32()?;
        let path = reader.read_string_u32()?;
        texture_overrides.push(TextureOverride { index, path });
    }
    let baked_paint_scale_offset = [
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ];

    Ok(MapModel {
        name,
        vertex_count,
        vertex_description_id,
        vertex_buffer_ids,
        index_count,
        index_buffer_id,
        layer,
        bucket_grid_hash,
        submeshes,
        disable_backface_culling,
        bounds,
        transform,
        quality,
        is_bush,
        render_flags,
        baked_light,
        stationary_light,
        texture_overrides,
        baked_paint_scale_offset,
    })
}

fn read_channel<R: Read>(reader: &mut R) -> Result<AssetChannel> {
    let path = reader.read_string_u32()?;
    let scale = read_vec2(reader)?;
    let offset = read_vec2(reader)?;
    Ok(AssetChannel {
        path,
        scale,
        offset,
    })
}

fn read_vec2<R: Read>(reader: &mut R) -> Result<Vec2> {
    Ok(Vec2::new(reader.read_f32()?, reader.read_f32()?))
}

fn read_vec3<R: Read>(reader: &mut R) -> Result<Vec3> {
    Ok(Vec3::new(
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ))
}

fn read_mat4<R: Read>(reader: &mut R) -> Result<rs_math::Mat4> {
    let m = reader.read_mtx44()?;
    Ok(rs_math::Mat4::from_cols_array(&m))
}
