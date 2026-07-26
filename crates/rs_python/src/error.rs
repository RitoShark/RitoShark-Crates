use std::fmt::Display;

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

create_exception!(ritoshark, FormatError, PyException);
create_exception!(ritoshark, ParseError, FormatError);
create_exception!(ritoshark, UnsupportedVersion, FormatError);
create_exception!(ritoshark, WriteError, FormatError);

pub fn parse_err<E: Display>(e: E) -> PyErr {
    ParseError::new_err(e.to_string())
}

pub fn write_err<E: Display>(e: E) -> PyErr {
    WriteError::new_err(e.to_string())
}

pub fn register(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FormatError", py.get_type::<FormatError>())?;
    m.add("ParseError", py.get_type::<ParseError>())?;
    m.add("UnsupportedVersion", py.get_type::<UnsupportedVersion>())?;
    m.add("WriteError", py.get_type::<WriteError>())?;
    Ok(())
}
