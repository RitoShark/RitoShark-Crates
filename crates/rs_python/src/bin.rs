/*!
Reads `.bin` documents into plain Python values. This view is deliberately lossy: it drops the
hash-versus-resolved-name duality, the LIST/LIST2 distinction, and duplicate map keys, none of
which a reader needs. Writing bins requires an editable tree that preserves every round-trip
invariant, which is a separate design.

`rs_bin` places no depth cap on `List`/`Map`/`Pointer`/`Embed`/`Option` nesting, so a malformed
file can parse into a value tree deep enough to blow the native stack once walked recursively — a
Rust stack overflow aborts the process outright, which Python cannot catch. `value_to_py` tracks
its own recursion depth and fails with a `ParseError` past `MAX_DEPTH` instead, well before any
real `.bin` file (observed real-world nesting tops out around 10) or the host stack is at risk.
*/

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use ritoshark::bin::{Bin, BinValue};
use ritoshark::prelude::Parse;

use crate::error::parse_err;

const MAX_DEPTH: usize = 128;

fn value_to_py<'py>(py: Python<'py>, v: &BinValue, depth: usize) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(parse_err(format!(
            "bin value nesting exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    Ok(match v {
        BinValue::None => py.None().into_bound(py),
        BinValue::Bool(b) | BinValue::Flag(b) => b.into_pyobject(py)?.to_owned().into_any(),
        BinValue::I8(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U8(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I16(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U16(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::I64(n) => n.into_pyobject(py)?.into_any(),
        BinValue::U64(n) => n.into_pyobject(py)?.into_any(),
        BinValue::F32(n) => n.into_pyobject(py)?.into_any(),
        BinValue::Vec2(a) => (a[0], a[1]).into_pyobject(py)?.into_any(),
        BinValue::Vec3(a) => (a[0], a[1], a[2]).into_pyobject(py)?.into_any(),
        BinValue::Vec4(a) => (a[0], a[1], a[2], a[3]).into_pyobject(py)?.into_any(),
        BinValue::Mtx44(a) => a.to_vec().into_pyobject(py)?.into_any(),
        BinValue::Rgba(a) => (a[0], a[1], a[2], a[3]).into_pyobject(py)?.into_any(),
        BinValue::String(s) => s.into_pyobject(py)?.into_any(),
        BinValue::Hash(h) | BinValue::Link(h) => h.into_pyobject(py)?.into_any(),
        BinValue::File(h) => h.into_pyobject(py)?.into_any(),
        BinValue::List { items, .. } => {
            let list = PyList::empty(py);
            for item in items {
                list.append(value_to_py(py, item, depth + 1)?)?;
            }
            list.into_any()
        }
        BinValue::Map { entries, .. } => {
            let dict = PyDict::new(py);
            for (k, val) in entries {
                dict.set_item(
                    value_to_py(py, k, depth + 1)?,
                    value_to_py(py, val, depth + 1)?,
                )?;
            }
            dict.into_any()
        }
        BinValue::Pointer { class, fields } | BinValue::Embed { class, fields } => {
            let dict = PyDict::new(py);
            dict.set_item("__class__", class)?;
            for (name_hash, val) in fields {
                dict.set_item(name_hash, value_to_py(py, val, depth + 1)?)?;
            }
            dict.into_any()
        }
        BinValue::Option { value, .. } => match value {
            Some(inner) => value_to_py(py, inner, depth + 1)?,
            None => py.None().into_bound(py),
        },
    })
}

fn doc_to_py<'py>(py: Python<'py>, bin: &Bin) -> PyResult<Bound<'py, PyDict>> {
    let doc = PyDict::new(py);
    doc.set_item("version", bin.version)?;
    doc.set_item("is_patch", bin.is_patch)?;
    doc.set_item("linked", bin.linked.clone())?;

    let entries = PyDict::new(py);
    for entry in &bin.entries {
        let fields = PyDict::new(py);
        fields.set_item("__class__", entry.class_hash)?;
        for (name_hash, val) in &entry.fields {
            fields.set_item(name_hash, value_to_py(py, val, 0)?)?;
        }
        entries.set_item(entry.path_hash, fields)?;
    }
    doc.set_item("entries", entries)?;
    Ok(doc)
}

#[pyfunction]
fn read_bin(py: Python<'_>, path: std::path::PathBuf) -> PyResult<Bound<'_, PyDict>> {
    let bin = Bin::from_path(path).map_err(parse_err)?;
    doc_to_py(py, &bin)
}

#[pyfunction]
fn read_bin_bytes<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let bin = Bin::from_bytes(data).map_err(parse_err)?;
    doc_to_py(py, &bin)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(read_bin, m)?)?;
    m.add_function(wrap_pyfunction!(read_bin_bytes, m)?)?;
    Ok(())
}
