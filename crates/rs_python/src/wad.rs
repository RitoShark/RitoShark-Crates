/*!
Reads `.wad` archives: lists the table of contents and decompresses chunks on demand. `wad_hash`
is exported so callers never hash a path without lowercasing it first; `ritoshark::hash::xxh64`
already lowercases internally, so the binding must not do it again.

`build_wad`/`build_wad_to_path` wrap [`ritoshark::wad::WadBuilder`], which drives its `provide`
callback twice per chunk (once to measure, once to write) and requires it to be reproducible
across both calls. A Python callback naturally reading from a file handle or generator would
silently break that contract on the second pass and produce a corrupt archive, so no callback
crosses the Python boundary: callers hand over a `dict[int, bytes]` or `dict[str, bytes]` up
front, every chunk's bytes already resident in memory, and reproducibility follows from there
being only one copy of the data to hand back on either pass. A free function rather than a
builder class matches `read_bin`'s precedent and fits the eager-dict shape, since there is
nothing incremental left to do once the whole mapping is in hand.
*/

use std::collections::HashMap;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyDict, PyMemoryView};
use ritoshark::prelude::Parse;
use ritoshark::wad::{DEFAULT_ZSTD_LEVEL, Wad as RsWad, WadBuilder, WadChunk};

use crate::error::{parse_err, write_err};

#[pyfunction]
pub fn wad_hash(path: &str) -> u64 {
    ritoshark::hash::xxh64(path)
}

fn extract_chunk_bytes(key: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(bytes) = value.downcast::<PyBytes>() {
        return Ok(bytes.as_bytes().to_vec());
    }
    if let Ok(bytearray) = value.downcast::<PyByteArray>() {
        return Ok(bytearray.to_vec());
    }
    if let Ok(view) = value.downcast::<PyMemoryView>() {
        let owned: Vec<u8> = view.call_method0("tobytes")?.extract()?;
        return Ok(owned);
    }
    Err(PyTypeError::new_err(format!(
        "chunk value for key {key} must be bytes, bytearray, or memoryview, got {}",
        value.get_type().name()?
    )))
}

fn collect_chunks(chunks: &Bound<'_, PyDict>) -> PyResult<HashMap<u64, Vec<u8>>> {
    let mut out = HashMap::with_capacity(chunks.len());
    for (key, value) in chunks.iter() {
        if key.is_instance_of::<PyBool>() {
            return Err(PyTypeError::new_err(format!(
                "chunk key {key} must be str (path) or int (path hash), not bool"
            )));
        }
        let path_hash = if let Ok(path) = key.extract::<String>() {
            wad_hash(&path)
        } else if let Ok(hash) = key.extract::<u64>() {
            hash
        } else {
            return Err(PyTypeError::new_err(
                "chunk keys must be str (path) or int (path hash)",
            ));
        };
        let data = extract_chunk_bytes(&key, &value)?;
        out.insert(path_hash, data);
    }
    Ok(out)
}

fn build_wad_bytes(chunks: &Bound<'_, PyDict>, zstd_level: i32) -> PyResult<Vec<u8>> {
    let entries = collect_chunks(chunks)?;
    let mut builder = WadBuilder::new().with_zstd_level(zstd_level);
    for &path_hash in entries.keys() {
        builder = builder.with_chunk_hash(path_hash);
    }
    builder
        .build_to_bytes(|path_hash, w| {
            let data = entries
                .get(&path_hash)
                .expect("builder requested an unregistered chunk");
            w.write_all(data).map_err(ritoshark::io::Error::from)?;
            Ok(())
        })
        .map_err(write_err)
}

#[pyfunction]
#[pyo3(signature = (chunks, zstd_level=DEFAULT_ZSTD_LEVEL))]
pub fn build_wad<'py>(
    py: Python<'py>,
    chunks: &Bound<'py, PyDict>,
    zstd_level: i32,
) -> PyResult<Bound<'py, PyBytes>> {
    let data = build_wad_bytes(chunks, zstd_level)?;
    Ok(PyBytes::new(py, &data))
}

#[pyfunction]
#[pyo3(signature = (path, chunks, zstd_level=DEFAULT_ZSTD_LEVEL))]
pub fn build_wad_to_path(
    path: std::path::PathBuf,
    chunks: &Bound<'_, PyDict>,
    zstd_level: i32,
) -> PyResult<()> {
    let data = build_wad_bytes(chunks, zstd_level)?;
    std::fs::write(path, data).map_err(write_err)
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
    m.add_function(wrap_pyfunction!(build_wad, m)?)?;
    m.add_function(wrap_pyfunction!(build_wad_to_path, m)?)?;
    Ok(())
}
