//! Explicit format selection and validated CANM -> v5 -> v4 fallback.
use crate::{Animation, Error, Result, Skeleton};
use std::io::Write;
use std::path::Path;

/// Requested or actually written animation container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationFormat {
    Canm,
    V5,
    V4,
}
impl std::fmt::Display for AnimationFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Canm => "CANM",
            Self::V5 => "v5",
            Self::V4 => "v4",
        })
    }
}

/// Frame-input export policy. Errors include packing and interpolation, not just key reduction.
/// Translation and model-space errors use the input's units; rotation uses radians.
/// The model-space check requires a skeleton and covers origins/unit-axis probes, not mesh vertices.
/// All values must be finite and nonnegative. Zero requests exact agreement on the check samples.
#[derive(Clone, Copy, Debug)]
pub struct AnimationExportOptions {
    pub format: AnimationFormat,
    pub rotation_error: f32,
    pub translation_error: f32,
    pub scale_error: f32,
    pub model_space_error: f32,
}
impl AnimationExportOptions {
    /// The reviewed frame-input quality: 2 degrees, 0.5 translation, 0.005 scale, 1 model unit.
    pub fn new(format: AnimationFormat) -> Self {
        Self {
            format,
            rotation_error: 2.0_f32.to_radians(),
            translation_error: 0.5,
            scale_error: 0.005,
            model_space_error: 1.0,
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if [
            self.rotation_error,
            self.translation_error,
            self.scale_error,
            self.model_space_error,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err(Error::InvalidData(
                "animation export tolerances must be finite and nonnegative",
            ));
        }
        Ok(())
    }
}

/// Maximum errors found by checking the finished bytes against frame interpolation.
/// Validation uses 96 and 97 intervals/frame; CANM additionally checks clock-cell edges.
/// These are sampled measurements, not a continuous or whole-mesh error guarantee.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimationExportMetrics {
    pub rotation: f32,
    pub translation: f32,
    pub scale: f32,
    /// `None` means no skeleton was supplied; unmatched animation tracks receive local checks only.
    pub model_space: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct AnimationFallback {
    pub format: AnimationFormat,
    /// Why this format could not be emitted at the requested accuracy or within its format limits.
    pub reason: String,
}

/// Successful export, including the real container and every fallback reason.
#[derive(Debug)]
pub struct AnimationExport {
    pub bytes: Vec<u8>,
    pub requested_format: AnimationFormat,
    pub format: AnimationFormat,
    pub fallbacks: Vec<AnimationFallback>,
    pub metrics: AnimationExportMetrics,
}

impl Animation {
    /// Encode current frame samples, ignoring retained raw bytes without modifying the animation.
    ///
    /// Selection is explicit: CANM tries CANM, v5, then v4; v5 tries v5, then v4; v4 tries only v4.
    /// Every successful candidate is decoded and checked against the same caller tolerances.
    /// Only representational/precision failures trigger fallback. Invalid input returns immediately.
    /// Both dense formats have 16-bit palettes; v4 is not a way around every v5 palette overflow.
    /// Ordinary byte-exact saves and the older strict format-specific writers are unchanged.
    pub fn export_bytes(
        &self,
        options: &AnimationExportOptions,
        skeleton: Option<&Skeleton>,
    ) -> Result<AnimationExport> {
        options.validate()?;
        crate::animation_v5_write::validate_samples(self)?;
        let duration = (self.frame_count() - 1) as f32 * (1. / self.fps);
        if !duration.is_finite()
            || !(self.fps * 97.).is_finite()
            || !(1. / (self.fps * 97.)).is_finite()
        {
            return Err(Error::InvalidData(
                "animation export timing is not representable",
            ));
        }
        if let Some(s) = skeleton {
            crate::compressed_quality::Hierarchy::new(s, self)?;
        }
        let formats: &[AnimationFormat] = match options.format {
            AnimationFormat::Canm => &[
                AnimationFormat::Canm,
                AnimationFormat::V5,
                AnimationFormat::V4,
            ],
            AnimationFormat::V5 => &[AnimationFormat::V5, AnimationFormat::V4],
            AnimationFormat::V4 => &[AnimationFormat::V4],
        };
        let mut fallbacks = Vec::new();
        for &format in formats {
            let attempt = match format {
                AnimationFormat::Canm => {
                    crate::compressed_write::adaptive::encode(self, options, skeleton)
                }
                AnimationFormat::V5 => self.to_v5_bytes().and_then(|bytes| {
                    let metrics = crate::compressed_write::adaptive::validate_dense(
                        self, &bytes, options, skeleton,
                    )?;
                    Ok((bytes, metrics))
                }),
                AnimationFormat::V4 => self.to_v4_export_bytes().and_then(|bytes| {
                    let metrics = crate::compressed_write::adaptive::validate_dense(
                        self, &bytes, options, skeleton,
                    )?;
                    Ok((bytes, metrics))
                }),
            };
            match attempt {
                Ok((bytes, metrics)) => {
                    return Ok(AnimationExport {
                        bytes,
                        requested_format: options.format,
                        format,
                        fallbacks,
                        metrics,
                    });
                }
                Err(
                    error @ (Error::Unsupported(_)
                    | Error::CompressionTolerance { .. }
                    | Error::CompressionTimePrecision { .. }
                    | Error::CompressionFit { .. }),
                ) => {
                    fallbacks.push(AnimationFallback {
                        format,
                        reason: error.to_string(),
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Err(Error::AnimationExportFailed(
            fallbacks
                .iter()
                .map(|f| format!("{}: {}", f.format, f.reason))
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }

    /// Complete encoding and validation before writing. Returns the same format/error report.
    pub fn export_to_writer<W: Write>(
        &self,
        writer: &mut W,
        options: &AnimationExportOptions,
        skeleton: Option<&Skeleton>,
    ) -> Result<AnimationExport> {
        let result = self.export_bytes(options, skeleton)?;
        writer
            .write_all(&result.bytes)
            .map_err(rs_io::Error::from)?;
        Ok(result)
    }

    /// Encode and validate before opening the destination; encoding failure preserves an existing file.
    pub fn export_to_path(
        &self,
        path: impl AsRef<Path>,
        options: &AnimationExportOptions,
        skeleton: Option<&Skeleton>,
    ) -> Result<AnimationExport> {
        let result = self.export_bytes(options, skeleton)?;
        rs_io::write_atomic(path, |writer| {
            writer.write_all(&result.bytes).map_err(rs_io::Error::from)
        })?;
        Ok(result)
    }
}
