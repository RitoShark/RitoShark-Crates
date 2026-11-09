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
use ritoshark::math::{Vec2, Vec3, Vec4};
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
    #[pyo3(signature = (positions, normals, uvs, blend_indices, blend_weights, indices, submeshes, *, colors = None, tangents = None, flags = 0, prefix_block = None, trailing = None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        positions: &[u8],
        normals: &[u8],
        uvs: &[u8],
        blend_indices: &[u8],
        blend_weights: &[u8],
        indices: &[u8],
        submeshes: Vec<Submesh>,
        colors: Option<&[u8]>,
        tangents: Option<&[u8]>,
        flags: u32,
        prefix_block: Option<&[u8]>,
        trailing: Option<&[u8]>,
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
        if let Some(bad) = indices.iter().find(|&&i| i as usize >= n) {
            return Err(write_err(format!(
                "indices: {bad} is out of range for {n} vertices"
            )));
        }
        /* A blend index is a u8 slot in the skeleton's influence table, not a joint id. The
        table itself holds u16 joint ids, so a skeleton may carry far more than 256 joints as
        long as no single mesh is weighted to more than 256 of them. */
        if let Some(bad) = blend_indices.iter().find(|&&i| i > u8::MAX as u32) {
            return Err(write_err(format!(
                "blend_indices: {bad} exceeds 255. A blend index selects a slot in the \
                 skeleton's influence table, so a mesh can use at most 256 influences, though \
                 the skeleton itself may have more joints than that."
            )));
        }

        if positions.iter().any(|v| !v.is_finite())
            || normals.iter().any(|v| !v.is_finite())
            || uvs.iter().any(|v| !v.is_finite())
            || blend_weights.iter().any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(write_err("geometry must be finite and weights nonnegative"));
        }
        if let Some(colors) = colors {
            expect_count(colors.len(), n * 4, "colors")?;
        }
        let tangents = tangents.map(|b| unpack_f32(b, "tangents")).transpose()?;
        if let Some(t) = &tangents {
            expect_count(t.len(), n * 4, "tangents")?;
            if colors.is_none() || t.iter().any(|v| !v.is_finite()) {
                return Err(write_err(
                    "tangents require colors and finite float32 values",
                ));
            }
        }
        let prefix_block = prefix_block.unwrap_or_default();
        if prefix_block.len() > u16::MAX as usize
            || (!prefix_block.is_empty() && flags & SkinnedMesh::FLAG_PREFIX_BLOCK == 0)
        {
            return Err(write_err(
                "prefix_block requires flag 1 and at most 65535 bytes",
            ));
        }
        let mut disk_indices = indices.clone();
        let mut covered = vec![false; indices.len()];
        for range in &submeshes {
            let vs = range.vertex_start as usize;
            let ve = vs
                .checked_add(range.vertex_count as usize)
                .ok_or_else(|| write_err("vertex range overflow"))?;
            let start = range.index_start as usize;
            let end = start
                .checked_add(range.index_count as usize)
                .ok_or_else(|| write_err("index range overflow"))?;
            if ve > n
                || end > indices.len()
                || start % 3 != 0
                || range.index_count % 3 != 0
                || range.name.len() > 64
                || range.name.contains('\0')
            {
                return Err(write_err(
                    "invalid submesh range or name (maximum 64 UTF-8 bytes)",
                ));
            }
            for i in start..end {
                if covered[i] || indices[i] < range.vertex_start || indices[i] as usize >= ve {
                    return Err(write_err(
                        "submesh indices overlap or refer outside its vertex range",
                    ));
                }
                covered[i] = true;
                if flags & SkinnedMesh::FLAG_RELATIVE_INDICES != 0 {
                    disk_indices[i] -= range.vertex_start;
                }
            }
        }
        if flags & SkinnedMesh::FLAG_RELATIVE_INDICES != 0 && covered.iter().any(|v| !v) {
            return Err(write_err(
                "relative indices require submeshes covering every index",
            ));
        }
        if disk_indices.iter().any(|&i| i > u16::MAX as u32) {
            return Err(write_err("indices exceed the 65535 index limit"));
        }

        let vertices = (0..n)
            .map(|i| {
                let mut vertex = SkinnedMeshVertex::new(
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
                );
                vertex.color = colors.map(|c| [c[i * 4], c[i * 4 + 1], c[i * 4 + 2], c[i * 4 + 3]]);
                vertex.tangent = tangents
                    .as_ref()
                    .map(|t| Vec4::new(t[i * 4], t[i * 4 + 1], t[i * 4 + 2], t[i * 4 + 3]));
                vertex
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
            flags,
            vertex_type: if tangents.is_some() {
                SkinnedMeshVertexType::Tangent
            } else if colors.is_some() {
                SkinnedMeshVertexType::Color
            } else {
                SkinnedMeshVertexType::Basic
            },
            bounding_box: bounds_of(&positions),
            bounding_sphere: sphere_of(&positions),
            ranges,
            indices: disk_indices.iter().map(|&i| i as u16).collect(),
            vertices,
            prefix_block: prefix_block.to_vec(),
            trailing: trailing.map_or_else(|| vec![0u8; 12], |b| b.to_vec()),
        };
        Ok(Self { inner })
    }

    #[getter]
    fn flags(&self) -> u32 {
        self.inner.flags
    }

    #[getter]
    fn colors<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        if self.inner.vertex_type == SkinnedMeshVertexType::Basic {
            return None;
        }
        let values: Vec<u8> = self
            .inner
            .vertices
            .iter()
            .flat_map(|v| v.color.unwrap_or([0; 4]))
            .collect();
        Some(PyBytes::new(py, &values))
    }

    #[getter]
    fn tangents<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        if self.inner.vertex_type != SkinnedMeshVertexType::Tangent {
            return None;
        }
        Some(PyBytes::new(
            py,
            &pack_f32(
                self.inner
                    .vertices
                    .iter()
                    .flat_map(|v| v.tangent.unwrap_or(Vec4::ZERO).to_array()),
            ),
        ))
    }

    #[getter]
    fn prefix_block<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.prefix_block)
    }

    #[getter]
    fn trailing<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.trailing)
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
        PyBytes::new(py, &pack_u32(self.inner.absolute_indices().into_iter()))
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
    #[pyo3(signature = (name, positions, faces, *, central=None, colors=None, flags=0, trailing=None, version=(3, 2)))]
    #[allow(clippy::too_many_arguments)] // Keep the established Python constructor signature.
    fn new(
        name: String,
        positions: &[u8],
        faces: Vec<ScbFace>,
        central: Option<(f32, f32, f32)>,
        colors: Option<&[u8]>,
        flags: u32,
        trailing: Option<&[u8]>,
        version: (u16, u16),
    ) -> PyResult<Self> {
        let positions = unpack_vec3(positions, "positions")?;
        let central = central
            .map(|c| Vec3::new(c.0, c.1, c.2))
            .unwrap_or_else(|| bounds_of(&positions).center());
        let colors = colors
            .map(|data| {
                if data.len() != positions.len() * 4 {
                    return Err(write_err(
                        "SCB colors must contain four BGRA bytes per vertex",
                    ));
                }
                Ok(data
                    .chunks_exact(4)
                    .map(|c| [c[0], c[1], c[2], c[3]])
                    .collect())
            })
            .transpose()?;
        let inner = StaticMesh {
            name,
            version,
            flags,
            bounding_box: bounds_of(&positions),
            vertex_type: if version == (3, 2) {
                Some(u32::from(colors.is_some()))
            } else {
                None
            },
            central,
            positions,
            colors,
            faces: faces.iter().map(face_from_py).collect(),
            trailing: trailing.unwrap_or_default().to_vec(),
        };
        inner.validate_scb().map_err(write_err)?;
        Ok(Self { inner })
    }

    #[getter]
    fn version(&self) -> (u16, u16) {
        self.inner.version
    }

    #[getter]
    fn flags(&self) -> u32 {
        self.inner.flags
    }

    #[getter]
    fn colors<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner
            .colors
            .as_ref()
            .map(|colors| PyBytes::new(py, &colors.iter().flatten().copied().collect::<Vec<_>>()))
    }

    #[getter]
    fn trailing<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.trailing)
    }

    /** Replaces the geometry while keeping everything the caller cannot reconstruct: the
    container version, the raw flag word, the `vertexType` word, per-vertex colors, and the
    opaque trailing block that carries the per-face VCP data and the local-origin/pivot vectors.
    Editing a file read from disk goes through here; `new` is for meshes built from scratch.

    The stored bounding box and central point are kept as-is unless `recompute_bounds` is set,
    because real files do not always agree with their own geometry: `floorslash.scb` ships a box
    whose `min.y` (10.0) exceeds its `max.y` (0.0). Recomputing by default would silently rewrite
    that and break byte-exact round-trips, so widening the bounds is opt-in. */
    #[pyo3(signature = (positions, faces, recompute_bounds = false))]
    fn replace_geometry(
        &mut self,
        positions: &[u8],
        faces: Vec<ScbFace>,
        recompute_bounds: bool,
    ) -> PyResult<()> {
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
        if let Some(colors) = &self.inner.colors {
            if colors.len() != n {
                return Err(write_err(format!(
                    "positions: this mesh carries {} per-vertex colors, so the replacement must \
                     have the same vertex count, got {n}",
                    colors.len()
                )));
            }
        }
        let mut replacement = self.inner.clone();
        replacement.positions = positions;
        replacement.faces = faces.iter().map(face_from_py).collect();
        if !self.inner.trailing.is_empty()
            && self
                .inner
                .faces
                .iter()
                .map(|f| f.indices)
                .collect::<Vec<_>>()
                != replacement
                    .faces
                    .iter()
                    .map(|f| f.indices)
                    .collect::<Vec<_>>()
        {
            return Err(write_err(
                "SCB opaque trailing data requires unchanged face layout",
            ));
        }
        if recompute_bounds {
            replacement.bounding_box = bounds_of(&replacement.positions);
            replacement.central = replacement.bounding_box.center();
        }
        replacement.validate_scb().map_err(write_err)?;
        self.inner = replacement;
        Ok(())
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[setter]
    fn set_name(&mut self, name: String) {
        self.inner.name = name;
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
        self.inner.to_path(path).map_err(write_err)
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
