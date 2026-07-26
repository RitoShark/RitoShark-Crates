/*!
Reads `.wad` archives: lists the table of contents and decompresses chunks on demand. Building
archives is deliberately absent, since packaging belongs to the tools that ship mods rather than to
a DCC plugin. `wad_hash` is exported so callers never hash a path without lowercasing it first;
`ritoshark::hash::xxh64` already lowercases internally, so the binding must not do it again.
*/

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::prelude::Parse;
use ritoshark::wad::{Wad as RsWad, WadChunk};

use crate::error::parse_err;

#[pyfunction]
pub fn wad_hash(path: &str) -> u64 {
    ritoshark::hash::xxh64(path)
}

#[pyclass(name = "WadChunk")]
#[derive(Clone)]
pub struct WadChunkInfo {
    #[pyo3(get)]
    pub path_hash: u64,
    #[pyo3(get)]
    pub compressed_size: u32,
    #[pyo3(get)]
    pub uncompressed_size: u32,
    #[pyo3(get)]
    pub compression: String,
    #[pyo3(get)]
    pub is_duplicated: bool,
}

impl From<&WadChunk> for WadChunkInfo {
    fn from(c: &WadChunk) -> Self {
        Self {
            path_hash: c.path_hash,
            compressed_size: c.compressed_size,
            uncompressed_size: c.uncompressed_size,
            compression: format!("{:?}", c.compression),
            is_duplicated: c.is_duplicated,
        }
    }
}

#[pymethods]
impl WadChunkInfo {
    fn __repr__(&self) -> String {
        format!(
            "WadChunk(path_hash={:#018x}, compression={:?}, uncompressed_size={})",
            self.path_hash, self.compression, self.uncompressed_size
        )
    }
}

#[pyclass]
pub struct Wad {
    inner: RsWad,
}

#[pymethods]
impl Wad {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        RsWad::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        RsWad::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[getter]
    fn version(&self) -> (u8, u8) {
        self.inner.version
    }

    #[getter]
    fn chunks(&self) -> Vec<WadChunkInfo> {
        self.inner.chunks.iter().map(WadChunkInfo::from).collect()
    }

    fn read<'py>(&self, py: Python<'py>, path_hash: u64) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let Some(chunk) = self.inner.chunk_by_hash(path_hash) else {
            return Ok(None);
        };
        let data = self.inner.chunk_data(chunk).map_err(parse_err)?;
        Ok(Some(PyBytes::new(py, &data)))
    }

    fn read_path<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.read(py, wad_hash(path))
    }

    fn __len__(&self) -> usize {
        self.inner.chunks.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Wad(version={:?}, chunks={})",
            self.inner.version,
            self.inner.chunks.len()
        )
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Wad>()?;
    m.add_class::<WadChunkInfo>()?;
    m.add_function(wrap_pyfunction!(wad_hash, m)?)?;
    Ok(())
}
