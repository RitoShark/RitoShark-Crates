use rs_math::{Vec3, Vec4};

use crate::{Error, Geometry, MapGeometry, Result};

impl Geometry {
    pub fn submesh_triangles(&self, index: usize) -> Result<impl Iterator<Item = [u16; 3]> + '_> {
        let submesh = self
            .submeshes
            .get(index)
            .ok_or_else(|| Error::InvalidData(format!("no submesh at index {index}")))?;
        if !self.positions.len().is_multiple_of(3) {
            return Err(Error::InvalidData(
                "positions must contain three floats per vertex".into(),
            ));
        }
        if submesh.index_start % 3 != 0 || submesh.index_count % 3 != 0 {
            return Err(Error::InvalidData(format!(
                "submesh {index} is not aligned to complete triangles"
            )));
        }
        let start = submesh.index_start as usize;
        let end = start
            .checked_add(submesh.index_count as usize)
            .ok_or_else(|| Error::InvalidData("submesh index range overflows usize".into()))?;
        let indices = self.indices.get(start..end).ok_or_else(|| {
            Error::InvalidData(format!("submesh {index} exceeds the index buffer"))
        })?;
        if indices
            .iter()
            .any(|&i| usize::from(i) >= self.vertex_count())
        {
            return Err(Error::InvalidData(format!(
                "submesh {index} references a missing vertex"
            )));
        }
        Ok(indices
            .chunks_exact(3)
            .map(|triangle| [triangle[0], triangle[1], triangle[2]]))
    }
}

impl MapGeometry {
    pub fn geometry_world(&self, index: usize) -> Result<Geometry> {
        let model = self
            .models
            .get(index)
            .ok_or_else(|| Error::InvalidData(format!("no model at index {index}")))?;
        let transform = model.transform;
        if !transform.is_finite() || transform.row(3) != Vec4::W {
            return Err(Error::InvalidData(
                "world geometry requires a finite affine transform".into(),
            ));
        }
        let determinant = transform.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return Err(Error::InvalidData(
                "world geometry requires an invertible transform".into(),
            ));
        }
        let mut geometry = self.geometry(index)?;
        if geometry.vertex_count() != model.vertex_count as usize {
            return Err(Error::InvalidData(
                "decoded position count differs from the model vertex count".into(),
            ));
        }
        geometry.validate()?;
        if geometry.indices.len() % 3 != 0 {
            return Err(Error::InvalidData(
                "index buffer contains an incomplete triangle".into(),
            ));
        }
        for submesh in 0..geometry.submeshes.len() {
            let _ = geometry.submesh_triangles(submesh)?;
        }
        if geometry
            .positions
            .iter()
            .chain(&geometry.normals)
            .chain(&geometry.uvs)
            .any(|v| !v.is_finite())
        {
            return Err(Error::InvalidData(
                "geometry contains non-finite attributes".into(),
            ));
        }
        let normal_transform = transform.inverse().transpose();
        if !normal_transform.is_finite() {
            return Err(Error::InvalidData(
                "normal transform exceeds finite float precision".into(),
            ));
        }
        for position in geometry.positions.chunks_exact_mut(3) {
            let result = transform.transform_point3(Vec3::from_slice(position));
            if !result.is_finite() {
                return Err(Error::InvalidData(
                    "transformed position is not finite".into(),
                ));
            }
            position.copy_from_slice(&result.to_array());
        }
        for normal in geometry.normals.chunks_exact_mut(3) {
            let result = normal_transform.transform_vector3(Vec3::from_slice(normal));
            if !result.is_finite() {
                return Err(Error::InvalidData(
                    "transformed normal is not finite".into(),
                ));
            }
            let magnitude = result.abs().max_element();
            let normalized = if magnitude == 0.0 {
                Vec3::ZERO
            } else {
                (result / magnitude).normalize()
            };
            normal.copy_from_slice(&normalized.to_array());
        }
        if determinant < 0.0 {
            for triangle in geometry.indices.chunks_exact_mut(3) {
                triangle.swap(1, 2);
            }
        }
        Ok(geometry)
    }
}
