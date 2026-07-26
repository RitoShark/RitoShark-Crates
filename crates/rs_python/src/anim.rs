/*!
Wraps the `.skl` skeleton and `.anm` animation. Joints and keyframes are exposed as objects rather
than packed buffers: a rig has hundreds of joints, not hundreds of thousands of vertices, so the
per-object cost is irrelevant and the resulting API is far easier to drive a DCC rig from. An
`Animation` read from a file preserves its source bytes, so an unedited read-write round-trips
byte-exact for both uncompressed and compressed containers; `is_byte_exact` and `make_editable`
are exposed directly rather than hidden behind an automatic transition, so a caller who mutates
tracks always knows whether a write will emit those edits or silently reproduce the original file.
*/

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use ritoshark::anim::{
    AnimFrame as RsAnimFrame, AnimTrack as RsAnimTrack, Animation, Joint, Skeleton,
};
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

#[pyclass(name = "AnimFrame")]
#[derive(Clone)]
pub struct PyAnimFrame {
    inner: RsAnimFrame,
}

#[pymethods]
impl PyAnimFrame {
    #[new]
    fn new(
        time: f32,
        rotation: (f32, f32, f32, f32),
        translation: (f32, f32, f32),
        scale: (f32, f32, f32),
    ) -> Self {
        Self {
            inner: RsAnimFrame::new(
                time,
                to_quat(rotation),
                to_vec3(translation),
                to_vec3(scale),
            ),
        }
    }

    #[getter]
    fn time(&self) -> f32 {
        self.inner.time
    }
    #[getter]
    fn rotation(&self) -> (f32, f32, f32, f32) {
        from_quat(self.inner.rotation)
    }
    #[getter]
    fn translation(&self) -> (f32, f32, f32) {
        from_vec3(self.inner.translation)
    }
    #[getter]
    fn scale(&self) -> (f32, f32, f32) {
        from_vec3(self.inner.scale)
    }

    fn __repr__(&self) -> String {
        format!("AnimFrame(time={})", self.inner.time)
    }
}

#[pyclass(name = "AnimTrack")]
#[derive(Clone)]
pub struct PyAnimTrack {
    inner: RsAnimTrack,
}

#[pymethods]
impl PyAnimTrack {
    #[new]
    fn new(joint_hash: u32, frames: Vec<PyAnimFrame>) -> Self {
        Self {
            inner: RsAnimTrack {
                joint_hash,
                frames: frames.into_iter().map(|f| f.inner).collect(),
            },
        }
    }

    #[getter]
    fn joint_hash(&self) -> u32 {
        self.inner.joint_hash
    }

    #[getter]
    fn frames(&self) -> Vec<PyAnimFrame> {
        self.inner
            .frames
            .iter()
            .map(|f| PyAnimFrame { inner: *f })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "AnimTrack(joint_hash={}, frames={})",
            self.inner.joint_hash,
            self.inner.frames.len()
        )
    }
}

#[pyclass]
pub struct Anm {
    inner: Animation,
}

#[pymethods]
impl Anm {
    #[staticmethod]
    fn from_path(path: std::path::PathBuf) -> PyResult<Self> {
        Animation::from_path(path)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        Animation::from_bytes(data)
            .map(|inner| Self { inner })
            .map_err(parse_err)
    }

    #[staticmethod]
    fn new(fps: f32, tracks: Vec<PyAnimTrack>) -> Self {
        let mut inner = Animation::new(fps);
        inner.tracks = tracks.into_iter().map(|t| t.inner).collect();
        Self { inner }
    }

    #[getter]
    fn fps(&self) -> f32 {
        self.inner.fps
    }

    #[getter]
    fn frame_count(&self) -> usize {
        self.inner.frame_count()
    }

    #[getter]
    fn is_byte_exact(&self) -> bool {
        self.inner.is_byte_exact()
    }

    fn make_editable(&mut self) {
        self.inner.make_editable();
    }

    #[getter]
    fn tracks(&self) -> Vec<PyAnimTrack> {
        self.inner
            .tracks
            .iter()
            .map(|t| PyAnimTrack { inner: t.clone() })
            .collect()
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self.inner.to_bytes().map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    fn to_path(&self, path: std::path::PathBuf) -> PyResult<()> {
        self.inner.to_path(path).map_err(write_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "Anm(fps={}, tracks={}, frames={})",
            self.inner.fps,
            self.inner.tracks.len(),
            self.inner.frame_count()
        )
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Skl>()?;
    m.add_class::<PyJoint>()?;
    m.add_class::<Anm>()?;
    m.add_class::<PyAnimTrack>()?;
    m.add_class::<PyAnimFrame>()?;
    Ok(())
}
