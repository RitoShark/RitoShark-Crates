use std::io::{Read, Seek};

use rs_io::{Parse, ReaderExt};
use rs_math::{Aabb, Sphere, Vec3};

use crate::error::{Error, Result};
use crate::skinned::{
    SkinnedMesh, SkinnedMeshRange, SkinnedMeshVertex, SkinnedMeshVertexType, MAGIC,
};

impl Parse for SkinnedMesh {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u32()?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic);
        }

        let major = reader.read_u16()?;
        let minor = reader.read_u16()?;
        if !matches!(major, 0 | 1 | 2 | 4) || minor != 1 {
            return Err(Error::UnsupportedVersion(
                (u32::from(major) << 16) | u32::from(minor),
            ));
        }

        let mut flags = 0u32;
        let mut vertex_type = SkinnedMeshVertexType::Basic;
        let mut bounding_box = Aabb::new(Vec3::ZERO, Vec3::ZERO);
        let mut bounding_sphere = Sphere::new(Vec3::ZERO, 0.0);

        let index_count;
        let vertex_count;
        let ranges;

        if major == 0 {
            index_count = reader.read_u32()?;
            vertex_count = reader.read_u32()?;
            ranges = vec![SkinnedMeshRange::new(
                "Base",
                0,
                vertex_count,
                0,
                index_count,
            )];
        } else {
            let range_count = reader.read_u32()? as usize;
            let mut r = Vec::with_capacity(range_count);
            for _ in 0..range_count {
                r.push(read_range(reader)?);
            }
            ranges = r;

            if major == 4 {
                flags = reader.read_u32()?;
            }

            index_count = reader.read_u32()?;
            vertex_count = reader.read_u32()?;

            if major == 4 {
                let vertex_size = reader.read_u32()?;
                let raw_type = reader.read_u32()?;
                vertex_type = SkinnedMeshVertexType::from_u32(raw_type)
                    .ok_or(Error::InvalidVertexType(raw_type))?;
                if vertex_size != vertex_type.vertex_size() {
                    return Err(Error::InvalidVertexType(raw_type));
                }
                bounding_box = Aabb::new(reader.read_vec3()?, reader.read_vec3()?);
                bounding_sphere = Sphere::new(reader.read_vec3()?, reader.read_f32()?);
            }
        }

        if index_count % 3 != 0 {
            return Err(Error::BadIndexCount(index_count));
        }

        let mut indices = Vec::with_capacity(index_count as usize);
        for _ in 0..index_count {
            indices.push(reader.read_u16()?);
        }

        let mut vertices = Vec::with_capacity(vertex_count as usize);
        for _ in 0..vertex_count {
            vertices.push(read_vertex(reader, vertex_type)?);
        }

        Ok(Self {
            major,
            minor,
            flags,
            vertex_type,
            bounding_box,
            bounding_sphere,
            ranges,
            indices,
            vertices,
        })
    }
}

fn read_range<R: Read>(reader: &mut R) -> Result<SkinnedMeshRange> {
    let name = reader.read_fixed_string::<64>()?;
    Ok(SkinnedMeshRange {
        name,
        vertex_start: reader.read_u32()?,
        vertex_count: reader.read_u32()?,
        index_start: reader.read_u32()?,
        index_count: reader.read_u32()?,
    })
}

fn read_vertex<R: Read>(
    reader: &mut R,
    vertex_type: SkinnedMeshVertexType,
) -> Result<SkinnedMeshVertex> {
    let position = reader.read_vec3()?;
    let blend_indices = reader.read_array::<4>()?;
    let blend_weights = [
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ];
    let normal = reader.read_vec3()?;
    let uv = reader.read_vec2()?;

    let mut color = None;
    let mut tangent = None;
    if matches!(
        vertex_type,
        SkinnedMeshVertexType::Color | SkinnedMeshVertexType::Tangent
    ) {
        color = Some(reader.read_array::<4>()?);
        if vertex_type == SkinnedMeshVertexType::Tangent {
            tangent = Some(reader.read_vec4()?);
        }
    }

    Ok(SkinnedMeshVertex {
        position,
        blend_indices,
        blend_weights,
        normal,
        uv,
        color,
        tangent,
    })
}
