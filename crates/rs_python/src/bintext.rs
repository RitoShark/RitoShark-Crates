/*!
Converts `.bin` documents to and from the `#PROP_text` ritobin form. `rs_bin::to_text`/
`from_text` already guarantee `bin -> text -> bin` is byte-identical, so this module only
moves bytes across the FFI boundary and maps errors — it must not reimplement any of that
logic. `from_text` accepts a `HashMapper` parameter upstream but its parser ignores it and
always re-derives names via FNV1a instead, so a dictionary entry that does not re-hash to
its own key would otherwise be silently swapped in on parse, corrupting the rebuilt `.bin`
with no error raised. `load_mapper` guards against this: every FNV1a-keyed entry (field,
class, entry, hash, and link names — anything at or below `u32::MAX`) is checked against
`rs_hash::fnv1a` before being kept, so only names that survive re-hashing ever reach
`to_text`. Entries above `u32::MAX` are XXH64 `File` values, which `to_text` also looks up
through the same mapper; those cannot be checked against FNV1a and are passed through as
loaded. `text_to_bin_*` takes no `hashes` argument: exposing one would claim a capability
that does not exist.
*/

use std::io::{BufRead, BufReader};

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::bin::{Bin, from_text, to_text};
use ritoshark::hash::{HashMapper, fnv1a};
use ritoshark::prelude::{Parse, Serialize};

use crate::error::{parse_err, write_err};

fn load_verified_mapper(path: &str) -> PyResult<HashMapper> {
    let file = std::fs::File::open(path).map_err(parse_err)?;
    let mut mapper = HashMapper::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(parse_err)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.trim().is_empty() {
            continue;
        }
        let (hex, name) = trimmed
            .split_once(' ')
            .ok_or_else(|| parse_err(format!("invalid hash dictionary line: {trimmed:?}")))?;
        let hash = u64::from_str_radix(hex, 16)
            .map_err(|_| parse_err(format!("invalid hash value: {hex:?}")))?;
        if hash > u32::MAX as u64 || fnv1a(name) as u64 == hash {
            mapper.insert(hash, name);
        }
    }
    Ok(mapper)
}

fn load_mapper(hashes: Option<&str>) -> PyResult<Option<HashMapper>> {
    match hashes {
        Some(path) => load_verified_mapper(path).map(Some),
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
