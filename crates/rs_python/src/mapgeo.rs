/*!
Wraps `.mapgeo` (OEGM) environment geometry. Unlike `.skn`, a `MapModel` does not own its
vertex/index data: it holds indices into the document's shared `vertex_buffers`/`index_buffers`,
and the interleaved layout of each vertex buffer is described separately by a `VertexDescription`.
`MapModel` therefore keeps an `Arc` handle to the parsed document plus its own index, and
`positions()`/`indices()` are methods rather than getters because each call walks the vertex
description and de-interleaves the referenced buffer. Constructing a `.mapgeo` from scratch is out
of scope: the format carries scene graphs, bucketed geometry, planar reflectors and per-version
lighting that a DCC does not author, so only read and byte-exact re-emission are exposed.
*/

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::mapgeo::{ElementFormat, ElementName, MapGeometry, Submesh as RsSubmesh};
use ritoshark::prelude::{Parse, Serialize};

use crate::error::{parse_err, write_err};

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

    fn positions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let model = self.model();
        let description = self
            .doc
            .vertex_descriptions
            .get(model.vertex_description_id as usize)
            .ok_or_else(|| parse_err("vertex_description_id is out of range"))?;

        let mut offset = 0usize;
        let mut position_offset = None;
        for element in &description.elements {
            if element.name == ElementName::Position {
                if element.format != ElementFormat::XyzFloat32 {
                    return Err(parse_err(format!(
                        "position element has format {:?}, expected XyzFloat32",
                        element.format
                    )));
                }
                position_offset = Some(offset);
                break;
            }
            offset += element.format.byte_size();
        }
        let Some(position_offset) = position_offset else {
            return Err(parse_err("vertex description has no Position element"));
        };
        let stride = description.vertex_size();

        let Some(&buffer_id) = model.vertex_buffer_ids.first() else {
            return Ok(PyBytes::new(py, &[]));
        };
        if buffer_id < 0 {
            return Ok(PyBytes::new(py, &[]));
        }
        let Some(buffer) = self.doc.vertex_buffers.get(buffer_id as usize) else {
            return Err(parse_err(
                "vertex_buffer_ids references an out-of-range buffer",
            ));
        };
        if stride == 0 {
            return Ok(PyBytes::new(py, &[]));
        }

        let vertex_count = model.vertex_count as usize;
        if vertex_count > 0 {
            let last_vertex_end = vertex_count
                .checked_sub(1)
                .and_then(|last| last.checked_mul(stride))
                .and_then(|span| span.checked_add(position_offset))
                .and_then(|end| end.checked_add(12))
                .ok_or_else(|| parse_err("vertex_count overflows while validating buffer size"))?;
            if last_vertex_end > buffer.data.len() {
                return Err(parse_err(format!(
                    "vertex buffer is too short for its description: vertex_count={} needs at \
                     least {} bytes, buffer has {}",
                    vertex_count,
                    last_vertex_end,
                    buffer.data.len()
                )));
            }
        }

        let mut out = Vec::with_capacity(vertex_count * 12);
        for i in 0..vertex_count {
            let base = i * stride + position_offset;
            let Some(slice) = buffer.data.get(base..base + 12) else {
                return Err(parse_err("vertex buffer is too short for its description"));
            };
            out.extend_from_slice(slice);
        }
        Ok(PyBytes::new(py, &out))
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
