/*!
Wraps the `.skn` skinned mesh and `.scb`/`.sco` static mesh. Geometry is exposed as one packed
buffer per attribute rather than per-vertex objects, since a Python object per vertex would cost
more than parsing the file. `.sco` is read-only: the game dropped the format and `rs_mesh` writes
only the binary `.scb` form. `Scb`/`Sco` share one underlying `StaticMesh`, which is what
`StaticMesh::from_path`/`from_bytes` auto-detect between by magic; face materials and per-corner
UVs travel as `ScbFace` values rather than packed buffers since each face is small and irregular.
*/

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::math::{Vec2, Vec3};
use ritoshark::mesh::{
    SkinnedMesh, SkinnedMeshRange, SkinnedMeshVertex, SkinnedMeshVertexType, StaticMesh,
    StaticMeshFace,
};
use ritoshark::prelude::{Parse, Serialize};

use crate::convert::{
    expect_count, pack_f32, pack_u32, pack_vec2, pack_vec3, unpack_f32, unpack_u32, unpack_vec2,
    unpack_vec3,
};
use crate::error::{parse_err, write_err};

#[pyclass]
#[derive(Clone)]
pub struct Submesh {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub vertex_start: u32,
    #[pyo3(get)]
    pub vertex_count: u32,
    #[pyo3(get)]
    pub index_start: u32,
    #[pyo3(get)]
    pub index_count: u32,
}

#[pymethods]
impl Submesh {
    #[new]
    fn new(
        name: String,
        vertex_start: u32,
        vertex_count: u32,
        index_start: u32,
        index_count: u32,
    ) -> Self {
        Self {
            name,
            vertex_start,
            vertex_count,
            index_start,
            index_count,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Submesh(name={:?}, vertex_count={}, index_count={})",
            self.name, self.vertex_count, self.index_count
        )
    }
}

#[pyclass]
pub struct Skn {
    inner: SkinnedMesh,
}

#[pymethods]
impl Skn {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        SkinnedMesh::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        SkinnedMesh::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    #[pyo3(signature = (positions, normals, uvs, blend_indices, blend_weights, indices, submeshes))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        positions: &[u8],
        normals: &[u8],
        uvs: &[u8],
        blend_indices: &[u8],
        blend_weights: &[u8],
        indices: &[u8],
        submeshes: Vec<Submesh>,
    ) -> PyResult<Self> {
        let positions = unpack_vec3(positions, "positions")?;
        let normals = unpack_vec3(normals, "normals")?;
        let uvs = unpack_vec2(uvs, "uvs")?;
        let blend_indices = unpack_u32(blend_indices, "blend_indices")?;
        let blend_weights = unpack_f32(blend_weights, "blend_weights")?;
        let indices = unpack_u32(indices, "indices")?;

        let n = positions.len();
        expect_count(normals.len(), n, "normals")?;
        expect_count(uvs.len(), n, "uvs")?;
        expect_count(blend_indices.len(), n * 4, "blend_indices")?;
        expect_count(blend_weights.len(), n * 4, "blend_weights")?;

        if indices.len() % 3 != 0 {
            return Err(write_err(format!(
                "indices: {} is not a multiple of 3",
                indices.len()
            )));
        }
        if let Some(bad) = indices.iter().find(|&&i| i > u16::MAX as u32) {
            return Err(write_err(format!(
                "indices: {bad} exceeds the .skn limit of 65535 vertices per mesh"
            )));
        }
        if let Some(bad) = indices.iter().find(|&&i| i as usize >= n) {
            return Err(write_err(format!(
                "indices: {bad} is out of range for {n} vertices"
            )));
        }
        if let Some(bad) = blend_indices.iter().find(|&&i| i > u8::MAX as u32) {
            return Err(write_err(format!(
                "blend_indices: {bad} exceeds the .skn joint limit of 255"
            )));
        }

        let vertices = (0..n)
            .map(|i| {
                SkinnedMeshVertex::new(
                    positions[i],
                    [
                        blend_indices[i * 4] as u8,
                        blend_indices[i * 4 + 1] as u8,
                        blend_indices[i * 4 + 2] as u8,
                        blend_indices[i * 4 + 3] as u8,
                    ],
                    [
                        blend_weights[i * 4],
                        blend_weights[i * 4 + 1],
                        blend_weights[i * 4 + 2],
                        blend_weights[i * 4 + 3],
                    ],
                    normals[i],
                    uvs[i],
                )
            })
            .collect::<Vec<_>>();

        let ranges = submeshes
            .into_iter()
            .map(|s| {
                SkinnedMeshRange::new(
                    s.name,
                    s.vertex_start,
                    s.vertex_count,
                    s.index_start,
                    s.index_count,
                )
            })
            .collect();

        let inner = SkinnedMesh {
            major: 4,
            minor: 1,
            flags: 0,
            vertex_type: SkinnedMeshVertexType::Basic,
            bounding_box: bounds_of(&positions),
            bounding_sphere: sphere_of(&positions),
            ranges,
            indices: indices.iter().map(|&i| i as u16).collect(),
            vertices,
            trailing: vec![0u8; 12],
        };
        Ok(Self { inner })
    }

    #[getter]
    fn version(&self) -> (u16, u16) {
        self.inner.version()
    }

    #[getter]
    fn vertex_count(&self) -> usize {
        self.inner.vertices.len()
    }

    #[getter]
    fn positions<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(
            py,
            &pack_vec3(self.inner.vertices.iter().map(|v| v.position)),
        )
    }

    #[getter]
    fn normals<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &pack_vec3(self.inner.vertices.iter().map(|v| v.normal)))
    }

    #[getter]
    fn uvs<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &pack_vec2(self.inner.vertices.iter().map(|v| v.uv)))
    }

    #[getter]
    fn blend_indices<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let flat = self.inner.vertices.iter().flat_map(|v| {
            v.blend_indices
                .iter()
                .map(|&b| b as u32)
                .collect::<Vec<_>>()
        });
        PyBytes::new(py, &pack_u32(flat))
    }

    #[getter]
    fn blend_weights<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let flat = self
            .inner
            .vertices
            .iter()
            .flat_map(|v| v.blend_weights.into_iter());
        PyBytes::new(py, &pack_f32(flat))
    }

    #[getter]
    fn indices<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &pack_u32(self.inner.indices.iter().map(|&i| i as u32)))
    }

    #[getter]
    fn submeshes(&self) -> Vec<Submesh> {
        self.inner
            .ranges
            .iter()
            .map(|r| Submesh {
                name: r.name.clone(),
                vertex_start: r.vertex_start,
                vertex_count: r.vertex_count,
                index_start: r.index_start,
                index_count: r.index_count,
            })
            .collect()
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self.inner.to_bytes().map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    fn to_path(&self, path: std::path::PathBuf) -> PyResult<()> {
        self.inner.to_path(path).map_err(write_err)
    }

    fn __repr__(&self) -> String {
        let (major, minor) = self.inner.version();
        format!(
            "Skn(version=({major}, {minor}), vertices={}, submeshes={})",
            self.inner.vertices.len(),
            self.inner.ranges.len()
        )
    }
}

#[pyclass]
#[derive(Clone)]
pub struct ScbFace {
    #[pyo3(get)]
    pub material: String,
    #[pyo3(get)]
    pub indices: (u32, u32, u32),
    #[pyo3(get)]
    pub uvs: ((f32, f32), (f32, f32), (f32, f32)),
}

#[pymethods]
impl ScbFace {
    #[new]
    fn new(
        material: String,
        indices: (u32, u32, u32),
        uvs: ((f32, f32), (f32, f32), (f32, f32)),
    ) -> Self {
        Self {
            material,
            indices,
            uvs,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ScbFace(material={:?}, indices={:?})",
            self.material, self.indices
        )
    }
}

fn face_to_py(f: &StaticMeshFace) -> ScbFace {
    ScbFace {
        material: f.material.clone(),
        indices: (f.indices[0], f.indices[1], f.indices[2]),
        uvs: (
            (f.uvs[0].x, f.uvs[0].y),
            (f.uvs[1].x, f.uvs[1].y),
            (f.uvs[2].x, f.uvs[2].y),
        ),
    }
}

fn face_from_py(f: &ScbFace) -> StaticMeshFace {
    StaticMeshFace::new(
        f.material.clone(),
        [f.indices.0, f.indices.1, f.indices.2],
        [
            Vec2::new(f.uvs.0.0, f.uvs.0.1),
            Vec2::new(f.uvs.1.0, f.uvs.1.1),
            Vec2::new(f.uvs.2.0, f.uvs.2.1),
        ],
    )
}

#[pyclass]
pub struct Scb {
    inner: StaticMesh,
}

#[pymethods]
impl Scb {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        StaticMesh::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        StaticMesh::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    #[pyo3(signature = (name, positions, faces))]
    fn new(name: String, positions: &[u8], faces: Vec<ScbFace>) -> PyResult<Self> {
        let positions = unpack_vec3(positions, "positions")?;
        let n = positions.len();
        for f in &faces {
            for i in [f.indices.0, f.indices.1, f.indices.2] {
                if i as usize >= n {
                    return Err(write_err(format!(
                        "faces: index {i} is out of range for {n} vertices"
                    )));
                }
            }
        }
        let central = if positions.is_empty() {
            Vec3::new(0.0, 0.0, 0.0)
        } else {
            let bb = bounds_of(&positions);
            (bb.min + bb.max) * 0.5
        };
        Ok(Self {
            inner: StaticMesh {
                name,
                version: (3, 2),
                flags: 0,
                bounding_box: bounds_of(&positions),
                vertex_type: None,
                central,
                positions,
                colors: None,
                faces: faces.iter().map(face_from_py).collect(),
                trailing: Vec::new(),
            },
        })
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn central(&self) -> (f32, f32, f32) {
        (
            self.inner.central.x,
            self.inner.central.y,
            self.inner.central.z,
        )
    }

    #[getter]
    fn positions<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &pack_vec3(self.inner.positions.iter().copied()))
    }

    #[getter]
    fn faces(&self) -> Vec<ScbFace> {
        self.inner.faces.iter().map(face_to_py).collect()
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self.inner.to_scb_bytes().map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    fn to_path(&self, path: std::path::PathBuf) -> PyResult<()> {
        let data = self.inner.to_scb_bytes().map_err(write_err)?;
        std::fs::write(path, data).map_err(write_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "Scb(name={:?}, vertices={}, faces={})",
            self.inner.name,
            self.inner.positions.len(),
            self.inner.faces.len()
        )
    }
}

#[pyclass]
pub struct Sco {
    inner: StaticMesh,
}

#[pymethods]
impl Sco {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        StaticMesh::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        StaticMesh::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn central(&self) -> (f32, f32, f32) {
        (
            self.inner.central.x,
            self.inner.central.y,
            self.inner.central.z,
        )
    }

    #[getter]
    fn positions<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &pack_vec3(self.inner.positions.iter().copied()))
    }

    #[getter]
    fn faces(&self) -> Vec<ScbFace> {
        self.inner.faces.iter().map(face_to_py).collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "Sco(name={:?}, vertices={}, faces={})",
            self.inner.name,
            self.inner.positions.len(),
            self.inner.faces.len()
        )
    }
}

fn bounds_of(positions: &[Vec3]) -> ritoshark::math::Aabb {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for p in positions {
        min = min.min(*p);
        max = max.max(*p);
    }
    if positions.is_empty() {
        min = Vec3::new(0.0, 0.0, 0.0);
        max = min;
    }
    ritoshark::math::Aabb::new(min, max)
}

fn sphere_of(positions: &[Vec3]) -> ritoshark::math::Sphere {
    let bb = bounds_of(positions);
    let center = bb.center();
    let radius = positions
        .iter()
        .map(|p| (*p - center).length())
        .fold(0.0f32, f32::max);
    ritoshark::math::Sphere::new(center, radius)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Skn>()?;
    m.add_class::<Submesh>()?;
    m.add_class::<Scb>()?;
    m.add_class::<Sco>()?;
    m.add_class::<ScbFace>()?;
    Ok(())
}
