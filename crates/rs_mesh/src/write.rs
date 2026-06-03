use std::io::Write;

use rs_io::{Serialize, WriterExt};

use crate::error::{Error, Result};
use crate::skinned::{SkinnedMesh, SkinnedMeshRange, SkinnedMeshVertex, SkinnedMeshVertexType, MAGIC};

impl Serialize for SkinnedMesh {
    type Error = Error;

    fn to_writer<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_u32(MAGIC)?;
        w.write_u16(self.major)?;
        w.write_u16(self.minor)?;

        if self.major == 0 {
            w.write_u32(self.indices.len() as u32)?;
            w.write_u32(self.vertices.len() as u32)?;
        } else {
            w.write_u32(self.ranges.len() as u32)?;
            for range in &self.ranges {
                write_range(w, range)?;
            }

            if self.major == 4 {
                w.write_u32(self.flags)?;
            }

            w.write_u32(self.indices.len() as u32)?;
            w.write_u32(self.vertices.len() as u32)?;

            if self.major == 4 {
                w.write_u32(self.vertex_type.vertex_size())?;
                w.write_u32(self.vertex_type.to_u32())?;
                w.write_vec3(self.bounding_box.min)?;
                w.write_vec3(self.bounding_box.max)?;
                w.write_vec3(self.bounding_sphere.center)?;
                w.write_f32(self.bounding_sphere.radius)?;
            }
        }

        for &index in &self.indices {
            w.write_u16(index)?;
        }

        for vertex in &self.vertices {
            write_vertex(w, vertex, self.vertex_type)?;
        }

        Ok(())
    }
}

fn write_range<W: Write>(w: &mut W, range: &SkinnedMeshRange) -> Result<()> {
    write_fixed_string::<_, 64>(w, &range.name)?;
    w.write_u32(range.vertex_start)?;
    w.write_u32(range.vertex_count)?;
    w.write_u32(range.index_start)?;
    w.write_u32(range.index_count)?;
    Ok(())
}

fn write_vertex<W: Write>(
    w: &mut W,
    vertex: &SkinnedMeshVertex,
    vertex_type: SkinnedMeshVertexType,
) -> Result<()> {
    w.write_vec3(vertex.position)?;
    w.write_bytes(&vertex.blend_indices)?;
    for weight in vertex.blend_weights {
        w.write_f32(weight)?;
    }
    w.write_vec3(vertex.normal)?;
    w.write_vec2(vertex.uv)?;

    if matches!(
        vertex_type,
        SkinnedMeshVertexType::Color | SkinnedMeshVertexType::Tangent
    ) {
        let color = vertex.color.unwrap_or([0; 4]);
        w.write_bytes(&color)?;
        if vertex_type == SkinnedMeshVertexType::Tangent {
            let tangent = vertex.tangent.unwrap_or(rs_math::Vec4::ZERO);
            w.write_vec4(tangent)?;
        }
    }

    Ok(())
}

fn write_fixed_string<W: Write, const N: usize>(w: &mut W, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    let len = bytes.len().min(N);
    let mut buf = [0u8; N];
    buf[..len].copy_from_slice(&bytes[..len]);
    w.write_bytes(&buf).map_err(Error::from)
}
