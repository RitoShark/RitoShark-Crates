/*!
Decoding and encoding of individual vertex attributes out of and into the interleaved buffers a
[`VertexDescription`] describes. The reader and writer treat those buffers as opaque bytes because
round-tripping them requires nothing else; editing does, since a caller supplying new geometry has
plain `f32` attributes and no way to know the stride, element order or packing a given description
demands.

The packed formats are fixed-point: a `1616`/`161616` component is a `u16` holding the value scaled
into a per-buffer range, and a `8888` component is a normalised byte. Riot's packed ranges are not
stored in the file, so a decoded packed attribute is returned in its raw normalised `0..=1` form and
an encoded one is expected in the same space. Positions are always `XyzFloat32` in every observed
file, so the lossy path never applies to them.
*/

use crate::error::{Error, Result};
use crate::mapgeo::{ElementFormat, ElementName, VertexDescription};

impl ElementFormat {
    /// Number of scalar components one attribute of this format carries.
    pub fn component_count(self) -> usize {
        match self {
            Self::XFloat32 => 1,
            Self::XyFloat32 | Self::XyPacked1616 => 2,
            Self::XyzFloat32 | Self::XyzPacked161616 => 3,
            Self::XyzwFloat32
            | Self::XyzwPacked16161616
            | Self::BgraPacked8888
            | Self::ZyxwPacked8888
            | Self::RgbaPacked8888 => 4,
        }
    }

    /// Whether this format stores its components as normalised fixed-point rather than `f32`.
    pub fn is_packed(self) -> bool {
        !matches!(
            self,
            Self::XFloat32 | Self::XyFloat32 | Self::XyzFloat32 | Self::XyzwFloat32
        )
    }

    fn decode(self, bytes: &[u8], out: &mut [f32]) {
        match self {
            Self::XFloat32 | Self::XyFloat32 | Self::XyzFloat32 | Self::XyzwFloat32 => {
                for (i, slot) in out.iter_mut().enumerate() {
                    let base = i * 4;
                    *slot = f32::from_le_bytes([
                        bytes[base],
                        bytes[base + 1],
                        bytes[base + 2],
                        bytes[base + 3],
                    ]);
                }
            }
            Self::XyPacked1616 | Self::XyzPacked161616 | Self::XyzwPacked16161616 => {
                for (i, slot) in out.iter_mut().enumerate() {
                    let base = i * 2;
                    let raw = u16::from_le_bytes([bytes[base], bytes[base + 1]]);
                    *slot = f32::from(raw) / f32::from(u16::MAX);
                }
            }
            Self::BgraPacked8888 => {
                out[0] = f32::from(bytes[2]) / 255.0;
                out[1] = f32::from(bytes[1]) / 255.0;
                out[2] = f32::from(bytes[0]) / 255.0;
                out[3] = f32::from(bytes[3]) / 255.0;
            }
            Self::ZyxwPacked8888 => {
                out[0] = f32::from(bytes[2]) / 255.0;
                out[1] = f32::from(bytes[1]) / 255.0;
                out[2] = f32::from(bytes[0]) / 255.0;
                out[3] = f32::from(bytes[3]) / 255.0;
            }
            Self::RgbaPacked8888 => {
                for (i, slot) in out.iter_mut().enumerate() {
                    *slot = f32::from(bytes[i]) / 255.0;
                }
            }
        }
    }

    fn encode(self, values: &[f32], bytes: &mut [u8]) {
        let clamp_unit = |v: f32| v.clamp(0.0, 1.0);
        match self {
            Self::XFloat32 | Self::XyFloat32 | Self::XyzFloat32 | Self::XyzwFloat32 => {
                for (i, &value) in values.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
            Self::XyPacked1616 | Self::XyzPacked161616 | Self::XyzwPacked16161616 => {
                for (i, &value) in values.iter().enumerate() {
                    let raw = (clamp_unit(value) * f32::from(u16::MAX)).round() as u16;
                    bytes[i * 2..i * 2 + 2].copy_from_slice(&raw.to_le_bytes());
                }
            }
            Self::BgraPacked8888 | Self::ZyxwPacked8888 => {
                bytes[0] = (clamp_unit(values[2]) * 255.0).round() as u8;
                bytes[1] = (clamp_unit(values[1]) * 255.0).round() as u8;
                bytes[2] = (clamp_unit(values[0]) * 255.0).round() as u8;
                bytes[3] = (clamp_unit(values[3]) * 255.0).round() as u8;
            }
            Self::RgbaPacked8888 => {
                for (i, &value) in values.iter().enumerate() {
                    bytes[i] = (clamp_unit(value) * 255.0).round() as u8;
                }
            }
        }
    }
}

impl VertexDescription {
    /// Byte offset of an attribute within one vertex, and its format, if the layout carries it.
    pub fn element(&self, name: ElementName) -> Option<(usize, ElementFormat)> {
        let mut offset = 0usize;
        for element in &self.elements {
            if element.name == name {
                return Some((offset, element.format));
            }
            offset += element.format.byte_size();
        }
        None
    }

    /** Reads one attribute for every vertex, returning `vertex_count * component_count` floats in
    vertex order. Packed formats are normalised to `0..=1`. Returns `Ok(None)` when the layout does
    not carry the attribute at all, which is a normal state rather than an error: a `.mapgeo` splits
    its attributes across several buffers and any one of them holds only some of them. */
    pub fn read_attribute(
        &self,
        data: &[u8],
        name: ElementName,
        vertex_count: usize,
    ) -> Result<Option<Vec<f32>>> {
        let Some((offset, format)) = self.element(name) else {
            return Ok(None);
        };
        let stride = self.vertex_size();
        if stride == 0 {
            return Ok(None);
        }
        let size = format.byte_size();
        let components = format.component_count();
        let needed = vertex_count.saturating_sub(1) * stride + offset + size;
        if vertex_count > 0 && needed > data.len() {
            return Err(Error::InvalidData(format!(
                "vertex buffer holds {} bytes but {:?} at offset {} needs {} for {} vertices",
                data.len(),
                name,
                offset,
                needed,
                vertex_count
            )));
        }

        let mut out = vec![0.0f32; vertex_count * components];
        for i in 0..vertex_count {
            let base = i * stride + offset;
            format.decode(
                &data[base..base + size],
                &mut out[i * components..(i + 1) * components],
            );
        }
        Ok(Some(out))
    }

    /** Writes one attribute for every vertex into an existing interleaved buffer, leaving the other
    attributes untouched. `values` must hold `vertex_count * component_count` floats. */
    pub fn write_attribute(
        &self,
        data: &mut [u8],
        name: ElementName,
        values: &[f32],
    ) -> Result<()> {
        let Some((offset, format)) = self.element(name) else {
            return Err(Error::InvalidData(format!(
                "vertex description has no {name:?} element"
            )));
        };
        let stride = self.vertex_size();
        if stride == 0 {
            return Err(Error::InvalidData(
                "vertex description has a zero stride".to_string(),
            ));
        }
        let components = format.component_count();
        if values.len() % components != 0 {
            return Err(Error::InvalidData(format!(
                "{:?} takes {} components per vertex but got {} values",
                name,
                components,
                values.len()
            )));
        }
        let size = format.byte_size();
        let vertex_count = values.len() / components;
        let needed = vertex_count.saturating_sub(1) * stride + offset + size;
        if vertex_count > 0 && needed > data.len() {
            return Err(Error::InvalidData(format!(
                "vertex buffer holds {} bytes but {} vertices of {:?} need {}",
                data.len(),
                vertex_count,
                name,
                needed
            )));
        }

        for i in 0..vertex_count {
            let base = i * stride + offset;
            format.encode(
                &values[i * components..(i + 1) * components],
                &mut data[base..base + size],
            );
        }
        Ok(())
    }
}
