/*!
Wraps `.mapgeo` (OEGM) environment geometry. Unlike `.skn`, a `MapModel` does not own its
vertex/index data: it holds indices into the document's shared `vertex_buffers`/`index_buffers`,
and the interleaved layout of each vertex buffer is described separately by a `VertexDescription`.
`MapModel` therefore keeps an `Arc` handle to the parsed document plus its own index, and
`positions()`/`indices()` are methods rather than getters because each call walks the vertex
description and de-interleaves the referenced buffer.

A file that was read can be edited: geometry replaced, models appended and removed, transforms and
layers set. Constructing one from scratch stays out of scope, because the scene graphs, bucketed
geometry, planar reflectors and per-version baked lighting a `.mapgeo` carries are not derivable
from a mesh — an edit keeps them from the file it started with.
*/

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::mapgeo::{ElementName, Geometry, MapGeometry, Submesh as RsSubmesh, SubmeshRange};
use ritoshark::math::Mat4;
use ritoshark::prelude::{Parse, Serialize};

use crate::error::{parse_err, write_err};

/** Reinterprets a tightly packed little-endian `f32` buffer as floats. Geometry crosses the binding
as `bytes` rather than a Python list because a map model runs to tens of thousands of vertices and
`array.array('f').tobytes()` is what a DCC already has. */
fn unpack_floats(data: &[u8], name: &str) -> PyResult<Vec<f32>> {
    if data.len() % 4 != 0 {
        return Err(write_err(format!(
            "{name}: expected whole float32 values, got {} bytes",
            data.len()
        )));
    }
    Ok(data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn unpack_indices(data: &[u8]) -> PyResult<Vec<u16>> {
    if data.len() % 2 != 0 {
        return Err(write_err(format!(
            "indices: expected whole uint16 values, got {} bytes",
            data.len()
        )));
    }
    Ok(data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect())
}

fn pack_floats(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/** Builds the library's [`Geometry`] from the plain buffers a DCC supplies. `submeshes` is a list of
`(name, index_start, index_count)`; an empty list means one submesh named `name` covering every
index, which is what a single-material mesh wants. */
fn build_geometry(
    name: &str,
    positions: &[u8],
    normals: &[u8],
    uvs: &[u8],
    indices: &[u8],
    submeshes: Vec<(String, u32, u32)>,
) -> PyResult<Geometry> {
    let indices = unpack_indices(indices)?;
    let ranges = if submeshes.is_empty() {
        vec![SubmeshRange {
            name: name.to_string(),
            index_start: 0,
            index_count: indices.len() as u32,
        }]
    } else {
        submeshes
            .into_iter()
            .map(|(name, index_start, index_count)| SubmeshRange {
                name,
                index_start,
                index_count,
            })
            .collect()
    };
    Ok(Geometry {
        positions: unpack_floats(positions, "positions")?,
        normals: unpack_floats(normals, "normals")?,
        uvs: unpack_floats(uvs, "uvs")?,
        indices,
        submeshes: ranges,
    })
}

#[pyclass(name = "MapSubmesh")]
#[derive(Clone)]
pub struct PyMapSubmesh {
    #[pyo3(get)]
    pub hash: u32,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub index_start: u32,
    #[pyo3(get)]
    pub index_count: u32,
    #[pyo3(get)]
    pub min_vertex: u32,
    #[pyo3(get)]
    pub max_vertex: u32,
}

impl From<&RsSubmesh> for PyMapSubmesh {
    fn from(s: &RsSubmesh) -> Self {
        Self {
            hash: s.hash,
            name: s.name.clone(),
            index_start: s.index_start,
            index_count: s.index_count,
            min_vertex: s.min_vertex,
            max_vertex: s.max_vertex,
        }
    }
}

#[pymethods]
impl PyMapSubmesh {
    fn __repr__(&self) -> String {
        format!(
            "MapSubmesh(name={:?}, index_start={}, index_count={})",
            self.name, self.index_start, self.index_count
        )
    }
}

#[pyclass(name = "MapModel")]
pub struct PyMapModel {
    doc: Arc<MapGeometry>,
    index: usize,
}

impl PyMapModel {
    fn model(&self) -> &ritoshark::mapgeo::MapModel {
        &self.doc.models[self.index]
    }

    /** De-interleaves one attribute out of whichever of the model's vertex buffers carries it.

    A model stores one `vertex_description_id` but may reference several buffers, and the
    descriptions are consumed consecutively from that id rather than the stored one describing all
    of them: Riot splits a vertex into streams, positions and normals in the first and texture
    coordinates in the next. An attribute no stream declares comes back empty rather than as an
    error, since that is a normal state for a `.mapgeo`. */
    fn attribute<'py>(&self, py: Python<'py>, name: ElementName) -> PyResult<Bound<'py, PyBytes>> {
        let model = self.model();
        let vertex_count = model.vertex_count as usize;
        let first = model.vertex_description_id as usize;

        for (offset, &id) in model.vertex_buffer_ids.iter().enumerate() {
            let Some(description) = self.doc.vertex_descriptions.get(first + offset) else {
                continue;
            };
            let Some(buffer) = usize::try_from(id)
                .ok()
                .and_then(|id| self.doc.vertex_buffers.get(id))
            else {
                continue;
            };
            if let Some(values) = description
                .read_attribute(&buffer.data, name, vertex_count)
                .map_err(parse_err)?
            {
                return Ok(PyBytes::new(py, &pack_floats(&values)));
            }
        }
        Ok(PyBytes::new(py, &[]))
    }
}

#[pymethods]
impl PyMapModel {
    #[getter]
    fn name(&self) -> &str {
        &self.model().name
    }

    #[getter]
    fn vertex_count(&self) -> u32 {
        self.model().vertex_count
    }

    #[getter]
    fn layer(&self) -> u8 {
        self.model().layer
    }

    #[getter]
    fn transform(&self) -> [f32; 16] {
        self.model().transform.to_cols_array()
    }

    #[getter]
    fn bounds(&self) -> ((f32, f32, f32), (f32, f32, f32)) {
        let b = &self.model().bounds;
        ((b.min.x, b.min.y, b.min.z), (b.max.x, b.max.y, b.max.z))
    }

    #[getter]
    fn disable_backface_culling(&self) -> bool {
        self.model().disable_backface_culling
    }

    #[getter]
    fn texture_overrides(&self) -> Vec<(u32, String)> {
        self.model()
            .texture_overrides
            .iter()
            .map(|t| (t.index, t.path.clone()))
            .collect()
    }

    #[getter]
    fn submeshes(&self) -> Vec<PyMapSubmesh> {
        self.model()
            .submeshes
            .iter()
            .map(PyMapSubmesh::from)
            .collect()
    }

    /** Per-vertex normals as 3 x float32, or empty when this model's layout carries none. Packed
    normal formats are decoded to the normalised `0..=1` range they are stored in. */
    fn normals<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.attribute(py, ElementName::Normal)
    }

    /** Per-vertex texture coordinates for channel 0 as 2 x float32, or empty when this model's
    layout carries none. */
    fn uvs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.attribute(py, ElementName::Texcoord0)
    }

    /// Per-vertex positions as 3 x float32.
    fn positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.attribute(py, ElementName::Position)
    }

    fn indices<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let model = self.model();
        if model.index_buffer_id < 0 {
            return Ok(PyBytes::new(py, &[]));
        }
        let Some(buffer) = self.doc.index_buffers.get(model.index_buffer_id as usize) else {
            return Err(parse_err("index_buffer_id is out of range"));
        };
        let mut out = Vec::with_capacity(buffer.indices.len() * 4);
        for &i in &buffer.indices {
            out.extend_from_slice(&(i as u32).to_le_bytes());
        }
        Ok(PyBytes::new(py, &out))
    }

    fn __repr__(&self) -> String {
        let model = self.model();
        format!(
            "MapModel(name={:?}, vertex_count={}, submeshes={})",
            model.name,
            model.vertex_count,
            model.submeshes.len()
        )
    }
}

#[pyclass]
pub struct MapGeo {
    inner: Arc<MapGeometry>,
}

#[pymethods]
impl MapGeo {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        MapGeometry::from_path(path)
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        MapGeometry::from_bytes(data)
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(parse_err)
    }

    #[getter]
    fn version(&self) -> u32 {
        self.inner.version
    }

    #[getter]
    fn models(&self) -> Vec<PyMapModel> {
        (0..self.inner.models.len())
            .map(|index| PyMapModel {
                doc: Arc::clone(&self.inner),
                index,
            })
            .collect()
    }

    /** Replaces one model's geometry, keeping its transform, layer, lighting, texture overrides and
    scene-graph association. `positions` and `normals` are 3 x float32 per vertex, `uvs` 2 x float32,
    and `indices` 1 x uint16 per triangle corner, all tightly packed little-endian. `submeshes` is a
    list of `(name, index_start, index_count)`; leaving it empty draws everything as one submesh
    named `name`. Attributes the model's vertex layout does not carry are ignored. */
    #[pyo3(signature = (index, name, positions, indices, normals=None, uvs=None, submeshes=Vec::new()))]
    #[allow(clippy::too_many_arguments)]
    fn replace_geometry(
        &mut self,
        index: usize,
        name: &str,
        positions: &[u8],
        indices: &[u8],
        normals: Option<&[u8]>,
        uvs: Option<&[u8]>,
        submeshes: Vec<(String, u32, u32)>,
    ) -> PyResult<()> {
        let geometry = build_geometry(
            name,
            positions,
            normals.unwrap_or_default(),
            uvs.unwrap_or_default(),
            indices,
            submeshes,
        )?;
        Arc::make_mut(&mut self.inner)
            .replace_geometry(index, &geometry)
            .map_err(write_err)
    }

    /** Appends a model and returns its index. `transform` is 16 floats, column-major, and `layer` is
    the visibility bitmask (255 draws on every layer). The new model carries no baked lighting and no
    scene-graph association, which is what a mesh authored outside Riot's tools can claim. */
    #[pyo3(signature = (name, positions, indices, normals=None, uvs=None, transform=None, layer=255, submeshes=Vec::new()))]
    #[allow(clippy::too_many_arguments)]
    fn add_model(
        &mut self,
        name: &str,
        positions: &[u8],
        indices: &[u8],
        normals: Option<&[u8]>,
        uvs: Option<&[u8]>,
        transform: Option<Vec<f32>>,
        layer: u8,
        submeshes: Vec<(String, u32, u32)>,
    ) -> PyResult<usize> {
        let geometry = build_geometry(
            name,
            positions,
            normals.unwrap_or_default(),
            uvs.unwrap_or_default(),
            indices,
            submeshes,
        )?;
        let matrix = match transform {
            None => Mat4::IDENTITY,
            Some(values) => {
                let values: [f32; 16] = values.try_into().map_err(|v: Vec<f32>| {
                    write_err(format!("transform: expected 16 floats, got {}", v.len()))
                })?;
                Mat4::from_cols_array(&values)
            }
        };
        Arc::make_mut(&mut self.inner)
            .add_model(name, &geometry, matrix, layer)
            .map_err(write_err)
    }

    /// Removes the model at `index`; the ones after it shift down.
    fn remove_model(&mut self, index: usize) -> PyResult<()> {
        Arc::make_mut(&mut self.inner)
            .remove_model(index)
            .map(|_| ())
            .map_err(write_err)
    }

    /// Sets a model's placement matrix from 16 floats, column-major.
    fn set_transform(&mut self, index: usize, transform: Vec<f32>) -> PyResult<()> {
        let values: [f32; 16] = transform.try_into().map_err(|v: Vec<f32>| {
            write_err(format!("transform: expected 16 floats, got {}", v.len()))
        })?;
        let doc = Arc::make_mut(&mut self.inner);
        let model = doc
            .models
            .get_mut(index)
            .ok_or_else(|| write_err(format!("no model at index {index}")))?;
        model.transform = Mat4::from_cols_array(&values);
        Ok(())
    }

    /// Sets a model's visibility layer bitmask.
    fn set_layer(&mut self, index: usize, layer: u8) -> PyResult<()> {
        let doc = Arc::make_mut(&mut self.inner);
        let model = doc
            .models
            .get_mut(index)
            .ok_or_else(|| write_err(format!("no model at index {index}")))?;
        model.layer = layer;
        Ok(())
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self.inner.to_bytes().map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    fn to_path(&self, path: std::path::PathBuf) -> PyResult<()> {
        self.inner.to_path(path).map_err(write_err)
    }

    fn __len__(&self) -> usize {
        self.inner.models.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "MapGeo(version={}, models={})",
            self.inner.version,
            self.inner.models.len()
        )
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MapGeo>()?;
    m.add_class::<PyMapModel>()?;
    m.add_class::<PyMapSubmesh>()?;
    Ok(())
}
