/*!
Reads `.tex` textures and decodes them to RGBA. Two decoded views are offered: `rgba` is the
top-down 8-bit buffer every general consumer wants, while `rgba_f32` is normalised to 0..1 and
row-flipped to match Blender's bottom-up `image.pixels` layout, so an addon can hand it straight to
`foreach_set` without a per-pixel Python loop over millions of values.
*/

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::prelude::Parse;
use ritoshark::tex::Texture;

use crate::error::parse_err;

#[pyclass]
pub struct Tex {
    inner: Texture,
}

#[pymethods]
impl Tex {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        Texture::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        Texture::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[getter]
    fn width(&self) -> u32 {
        self.inner.width
    }

    #[getter]
    fn height(&self) -> u32 {
        self.inner.height
    }

    #[getter]
    fn format(&self) -> String {
        format!("{:?}", self.inner.format)
    }

    #[getter]
    fn mip_count(&self) -> u32 {
        self.inner.mip_count()
    }

    #[getter]
    fn rgba<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        if self.inner.width == 0 || self.inner.height == 0 {
            return Ok(PyBytes::new(py, &[]));
        }
        let image = self.inner.decode_rgba().map_err(parse_err)?;
        Ok(PyBytes::new(py, image.as_raw()))
    }

    #[getter]
    fn rgba_f32<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        if self.inner.width == 0 || self.inner.height == 0 {
            return Ok(PyBytes::new(py, &[]));
        }
        let image = self.inner.decode_rgba().map_err(parse_err)?;
        let raw = image.as_raw();
        let row = self.inner.width as usize * 4;
        let mut out = Vec::with_capacity(raw.len() * 4);
        for y in (0..self.inner.height as usize).rev() {
            for &b in &raw[y * row..y * row + row] {
                out.extend_from_slice(&(b as f32 / 255.0).to_le_bytes());
            }
        }
        Ok(PyBytes::new(py, &out))
    }

    fn __repr__(&self) -> String {
        format!(
            "Tex({}x{}, format={:?}, mips={})",
            self.inner.width,
            self.inner.height,
            self.inner.format,
            self.inner.mip_count()
        )
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Tex>()?;
    Ok(())
}
