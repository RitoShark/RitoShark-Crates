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
        let mut inner = Skeleton::new();
        inner.name = name;
        inner.asset = asset;
        inner.joints = joints.into_iter().map(|j| j.inner).collect();
        inner.influences = influences;
        Self { inner }
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

    /// Explicit selection with CANM -> v5 -> v4 fallback and decoded-byte quality checks.
    /// Returns bytes, actual format, fallback reasons and measured errors. Does not alter the clip.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(format, *, skeleton=None, rotation_error=0.034906585, translation_error=0.5, scale_error=0.005, model_space_error=1.0))]
    fn export(
        &self,
        py: Python<'_>,
        format: &str,
        skeleton: Option<PyRef<'_, Skl>>,
        rotation_error: f32,
        translation_error: f32,
        scale_error: f32,
        model_space_error: f32,
    ) -> PyResult<PyAnmExport> {
        use ritoshark::anim::{AnimationExportOptions, AnimationFormat};
        let format = match format {
            "canm" => AnimationFormat::Canm,
            "v5" => AnimationFormat::V5,
            "v4" => AnimationFormat::V4,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "format must be 'canm', 'v5' or 'v4'",
                ));
            }
        };
        let options = AnimationExportOptions {
            format,
            rotation_error,
            translation_error,
            scale_error,
            model_space_error,
        };
        let skeleton = skeleton.as_ref().map(|s| &s.inner);
        let inner = py
            .allow_threads(|| self.inner.export_bytes(&options, skeleton))
            .map_err(write_err)?;
        Ok(PyAnmExport { inner })
    }

    /// Dense ANMD v5, retaining every frame and float32 translation/scale.
    fn to_v5_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = py
            .allow_threads(|| self.inner.to_v5_bytes())
            .map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    fn to_v5_path(&self, py: Python<'_>, path: std::path::PathBuf) -> PyResult<()> {
        py.allow_threads(|| self.inner.to_v5_path(path))
            .map_err(write_err)
    }

    /// CANM v1, checking subframes and optionally the composed skeleton hierarchy.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (*, rotation_error=0.001, translation_error=0.05, scale_error=0.001, sample_subdivisions=4, skeleton=None, model_space_error=0.1))]
    fn to_compressed_bytes<'py>(
        &self,
        py: Python<'py>,
        rotation_error: f32,
        translation_error: f32,
        scale_error: f32,
        sample_subdivisions: u32,
        skeleton: Option<PyRef<'_, Skl>>,
        model_space_error: f32,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let options = ritoshark::anim::CompressionOptions {
            rotation_error,
            translation_error,
            scale_error,
            sample_subdivisions,
            model_space_error,
        };
        let skeleton = skeleton.as_ref().map(|s| &s.inner);
        let data = py
            .allow_threads(|| match skeleton {
                Some(s) => self.inner.to_compressed_bytes_with_skeleton(&options, s),
                None => self.inner.to_compressed_bytes(&options),
            })
            .map_err(write_err)?;
        Ok(PyBytes::new(py, &data))
    }

    /// Validate and encode before opening the destination file.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, *, rotation_error=0.001, translation_error=0.05, scale_error=0.001, sample_subdivisions=4, skeleton=None, model_space_error=0.1))]
    fn to_compressed_path(
        &self,
        py: Python<'_>,
        path: std::path::PathBuf,
        rotation_error: f32,
        translation_error: f32,
        scale_error: f32,
        sample_subdivisions: u32,
        skeleton: Option<PyRef<'_, Skl>>,
        model_space_error: f32,
    ) -> PyResult<()> {
        let options = ritoshark::anim::CompressionOptions {
            rotation_error,
            translation_error,
            scale_error,
            sample_subdivisions,
            model_space_error,
        };
        let skeleton = skeleton.as_ref().map(|s| &s.inner);
        py.allow_threads(|| match skeleton {
            Some(s) => self
                .inner
                .to_compressed_path_with_skeleton(path, &options, s),
            None => self.inner.to_compressed_path(path, &options),
        })
        .map_err(write_err)
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

fn format_name(format: ritoshark::anim::AnimationFormat) -> &'static str {
    use ritoshark::anim::AnimationFormat::*;
    match format {
        Canm => "canm",
        V5 => "v5",
        V4 => "v4",
    }
}

/// Validated animation bytes and the actual format selected by fallback.
#[pyclass(name = "AnmExport")]
pub struct PyAnmExport {
    inner: ritoshark::anim::AnimationExport,
}
#[pymethods]
impl PyAnmExport {
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }
    #[getter]
    fn format(&self) -> &'static str {
        format_name(self.inner.format)
    }
    #[getter]
    fn requested_format(&self) -> &'static str {
        format_name(self.inner.requested_format)
    }
    #[getter]
    fn fallbacks(&self) -> Vec<(&'static str, String)> {
        self.inner
            .fallbacks
            .iter()
            .map(|f| (format_name(f.format), f.reason.clone()))
            .collect()
    }
    #[getter]
    fn metrics(&self) -> std::collections::BTreeMap<&'static str, Option<f32>> {
        let m = &self.inner.metrics;
        [
            ("rotation", Some(m.rotation)),
            ("translation", Some(m.translation)),
            ("scale", Some(m.scale)),
            ("model_space", m.model_space),
        ]
        .into_iter()
        .collect()
    }
    fn to_path(&self, path: std::path::PathBuf) -> PyResult<()> {
        std::fs::write(path, &self.inner.bytes)
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))
    }
    fn __repr__(&self) -> String {
        format!(
            "AnmExport(requested={:?}, format={:?}, bytes={}, fallbacks={})",
            self.requested_format(),
            self.format(),
            self.inner.bytes.len(),
            self.inner.fallbacks.len()
        )
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Skl>()?;
    m.add_class::<PyJoint>()?;
    m.add_class::<Anm>()?;
    m.add_class::<PyAnmExport>()?;
    m.add_class::<PyAnimTrack>()?;
    m.add_class::<PyAnimFrame>()?;
    Ok(())
}
