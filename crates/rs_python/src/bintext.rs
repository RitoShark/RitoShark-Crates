/*!
Converts `.bin` documents to and from the `#PROP_text` ritobin form. `rs_bin::to_text`/
`from_text` already guarantee `bin -> text -> bin` is byte-identical, so this module only
moves bytes across the FFI boundary and maps errors — it must not reimplement any of that
logic. `from_text` accepts a `HashMapper` parameter upstream but its parser ignores it, so
`text_to_bin_*` takes no `hashes` argument: exposing one would claim a capability that does
not exist.
*/

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::bin::{Bin, from_text, to_text};
use ritoshark::hash::HashMapper;
use ritoshark::prelude::{Parse, Serialize};

use crate::error::{parse_err, write_err};

fn load_mapper(hashes: Option<&str>) -> PyResult<Option<HashMapper>> {
    match hashes {
        Some(path) => HashMapper::load_file(path).map(Some).map_err(parse_err),
        None => Ok(None),
    }
}

#[pyfunction]
#[pyo3(signature = (path, hashes=None))]
fn bin_to_text(path: std::path::PathBuf, hashes: Option<&str>) -> PyResult<String> {
    let mapper = load_mapper(hashes)?;
    let bin = Bin::from_path(path).map_err(parse_err)?;
    Ok(to_text(&bin, mapper.as_ref()))
}

#[pyfunction]
#[pyo3(signature = (data, hashes=None))]
fn bin_to_text_bytes(data: &[u8], hashes: Option<&str>) -> PyResult<String> {
    let mapper = load_mapper(hashes)?;
    let bin = Bin::from_bytes(data).map_err(parse_err)?;
    Ok(to_text(&bin, mapper.as_ref()))
}

#[pyfunction]
fn text_to_bin_path(path: std::path::PathBuf, text: &str) -> PyResult<()> {
    let bin = from_text(text, None).map_err(parse_err)?;
    bin.to_path(path).map_err(write_err)
}

#[pyfunction]
fn text_to_bin_bytes<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyBytes>> {
    let bin = from_text(text, None).map_err(parse_err)?;
    let data = bin.to_bytes().map_err(write_err)?;
    Ok(PyBytes::new(py, &data))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(bin_to_text, m)?)?;
    m.add_function(wrap_pyfunction!(bin_to_text_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(text_to_bin_path, m)?)?;
    m.add_function(wrap_pyfunction!(text_to_bin_bytes, m)?)?;
    Ok(())
}
