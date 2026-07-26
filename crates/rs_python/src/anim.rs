/*!
Wraps the `.skl` skeleton and `.anm` animation. Joints and keyframes are exposed as objects rather
than packed buffers: a rig has hundreds of joints, not hundreds of thousands of vertices, so the
per-object cost is irrelevant and the resulting API is far easier to drive a DCC rig from.
*/

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::anim::{Joint, Skeleton};
use ritoshark::hash::elf_lower;
use ritoshark::math::{Quat, Vec3};
use ritoshark::prelude::{Parse, Serialize};

use crate::error::{parse_err, write_err};

fn to_vec3(t: (f32, f32, f32)) -> Vec3 {
    Vec3::new(t.0, t.1, t.2)
}

fn from_vec3(v: Vec3) -> (f32, f32, f32) {
    (v.x, v.y, v.z)
}

fn to_quat(t: (f32, f32, f32, f32)) -> Quat {
    Quat::from_xyzw(t.0, t.1, t.2, t.3)
}

fn from_quat(q: Quat) -> (f32, f32, f32, f32) {
    (q.x, q.y, q.z, q.w)
}

#[pyclass(name = "Joint")]
#[derive(Clone)]
pub struct PyJoint {
    inner: Joint,
}

#[pymethods]
impl PyJoint {
    #[new]
    #[pyo3(signature = (name, id, parent_id, radius, local_translation, local_scale,
                        local_rotation, inverse_bind_translation, inverse_bind_scale,
                        inverse_bind_rotation, flags = 0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: String,
        id: i16,
        parent_id: i16,
        radius: f32,
        local_translation: (f32, f32, f32),
        local_scale: (f32, f32, f32),
        local_rotation: (f32, f32, f32, f32),
        inverse_bind_translation: (f32, f32, f32),
        inverse_bind_scale: (f32, f32, f32),
        inverse_bind_rotation: (f32, f32, f32, f32),
        flags: u16,
    ) -> Self {
        let hash = elf_lower(&name);
        Self {
            inner: Joint {
                name,
                flags,
                id,
                parent_id,
                radius,
                hash,
                local_translation: to_vec3(local_translation),
                local_scale: to_vec3(local_scale),
                local_rotation: to_quat(local_rotation),
                inverse_bind_translation: to_vec3(inverse_bind_translation),
                inverse_bind_scale: to_vec3(inverse_bind_scale),
                inverse_bind_rotation: to_quat(inverse_bind_rotation),
            },
        }
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }
    #[getter]
    fn id(&self) -> i16 {
        self.inner.id
    }
    #[getter]
    fn parent_id(&self) -> i16 {
        self.inner.parent_id
    }
    #[getter]
    fn radius(&self) -> f32 {
        self.inner.radius
    }
    #[getter]
    fn hash(&self) -> u32 {
        self.inner.hash
    }
    #[getter]
    fn flags(&self) -> u16 {
        self.inner.flags
    }
    #[getter]
    fn local_translation(&self) -> (f32, f32, f32) {
        from_vec3(self.inner.local_translation)
    }
    #[getter]
    fn local_scale(&self) -> (f32, f32, f32) {
        from_vec3(self.inner.local_scale)
    }
    #[getter]
    fn local_rotation(&self) -> (f32, f32, f32, f32) {
        from_quat(self.inner.local_rotation)
    }
    #[getter]
    fn inverse_bind_translation(&self) -> (f32, f32, f32) {
        from_vec3(self.inner.inverse_bind_translation)
    }
    #[getter]
    fn inverse_bind_scale(&self) -> (f32, f32, f32) {
        from_vec3(self.inner.inverse_bind_scale)
    }
    #[getter]
    fn inverse_bind_rotation(&self) -> (f32, f32, f32, f32) {
        from_quat(self.inner.inverse_bind_rotation)
    }

    fn __repr__(&self) -> String {
        format!(
            "Joint(name={:?}, id={}, parent_id={})",
            self.inner.name, self.inner.id, self.inner.parent_id
        )
    }
}

#[pyclass]
pub struct Skl {
    inner: Skeleton,
}

#[pymethods]
impl Skl {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        Skeleton::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        Skeleton::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    #[pyo3(signature = (joints, influences, name = String::new(), asset = String::new()))]
    fn new(joints: Vec<PyJoint>, influences: Vec<u16>, name: String, asset: String) -> Self {
        Self {
            inner: Skeleton {
                flags: 0,
                name,
                asset,
                joints: joints.into_iter().map(|j| j.inner).collect(),
                influences,
            },
        }
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }
    #[getter]
    fn asset(&self) -> &str {
        &self.inner.asset
    }
    #[getter]
    fn joints(&self) -> Vec<PyJoint> {
        self.inner
            .joints
            .iter()
            .map(|j| PyJoint { inner: j.clone() })
            .collect()
    }
    #[getter]
    fn influences(&self) -> Vec<u16> {
        self.inner.influences.clone()
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self.inner.to_bytes().map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    fn to_path(&self, path: std::path::PathBuf) -> PyResult<()> {
        self.inner.to_path(path).map_err(write_err)
    }

    fn __repr__(&self) -> String {
        format!("Skl(joints={})", self.inner.joints.len())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Skl>()?;
    m.add_class::<PyJoint>()?;
    Ok(())
}
