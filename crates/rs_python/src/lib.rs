#![forbid(unsafe_code)]
#![allow(dead_code)]
/*!
Python bindings for the RitoShark format crates. Each format module wraps its `rs_*` type in a
`#[pyclass]` and converts bulk geometry to tightly packed little-endian buffers, so callers move
vertex data into Blender or Maya with a single memcpy instead of a per-element Python loop.
*/

mod convert;
mod error;

use pyo3::prelude::*;

#[pymodule]
fn ritoshark(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    error::register(py, m)?;
    convert::register(m)?;
    Ok(())
}
