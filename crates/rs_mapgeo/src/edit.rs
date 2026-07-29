/*!
Editing operations over a parsed [`MapGeometry`]: replacing a model's geometry, appending a new
model, and removing one.

A `.mapgeo` carries far more than the geometry a DCC can author — bucketed scene graphs, planar
reflectors, per-version baked lighting and the shader/sampler tables. None of it is derivable from a
mesh, so editing is expressed as a change against a file that already has it rather than as
construction from nothing. The scene graphs are independent of the render models (they carry their
own vertices and indices, and models reference them only by name hash), so moving, reshaping or
deleting a model leaves them valid.

Every edit writes into freshly appended vertex and index buffers instead of the ones the model used.
Buffers are shared between models in real files, so writing in place would silently corrupt an
unrelated model that happened to index the same bytes. The abandoned buffers are left in the file:
the format reaches them only through model references, and dropping them would mean renumbering
every remaining reference for no gain in validity.
*/

use rs_math::{Aabb, Mat4, Vec3};

use crate::error::{Error, Result};
use crate::mapgeo::{
    AssetChannel, ElementFormat, ElementName, IndexBuffer, MapGeometry, MapModel, Submesh,
    VertexBuffer, VertexDescription, VertexElement, VertexUsage,
};

/** The geometry of one model in the plain, unpacked form a DCC works in. `positions` holds three
floats per vertex; `indices` holds one `u16` per triangle corner. `normals` takes three floats per
vertex and `uvs` two, and either may be left empty when the target layout does not carry it.

`submeshes` names the draw ranges into `indices` and must cover it without gaps if the model is to
render in full. A single unnamed submesh spanning everything is valid and is what [`Self::single`]
builds. */
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u16>,
    pub submeshes: Vec<SubmeshRange>,
}

/// A named range of [`Geometry::indices`] that draws with one material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmeshRange {
    pub name: String,
    pub index_start: u32,
    pub index_count: u32,
}

impl Geometry {
    /// Geometry drawn as one submesh covering every index.
    pub fn single(
        name: impl Into<String>,
        positions: Vec<f32>,
        normals: Vec<f32>,
        uvs: Vec<f32>,
        indices: Vec<u16>,
    ) -> Self {
        let index_count = indices.len() as u32;
        Self {
            positions,
            normals,
            uvs,
            indices,
            submeshes: vec![SubmeshRange {
                name: name.into(),
                index_start: 0,
                index_count,
            }],
        }
    }

    /// Vertices implied by the position list.
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    fn validate(&self) -> Result<()> {
        if self.positions.is_empty() {
            return Err(Error::InvalidData("geometry has no positions".to_string()));
        }
        if self.positions.len() % 3 != 0 {
            return Err(Error::InvalidData(format!(
                "positions must be 3 floats per vertex, got {}",
                self.positions.len()
            )));
        }
        let vertex_count = self.vertex_count();
        if vertex_count > u32::MAX as usize {
            return Err(Error::InvalidData(format!(
                "{vertex_count} vertices exceeds the u32 vertex count the format stores"
            )));
        }
        if !self.normals.is_empty() && self.normals.len() != vertex_count * 3 {
            return Err(Error::InvalidData(format!(
                "expected {} normal floats for {} vertices, got {}",
                vertex_count * 3,
                vertex_count,
                self.normals.len()
            )));
        }
        if !self.uvs.is_empty() && self.uvs.len() != vertex_count * 2 {
            return Err(Error::InvalidData(format!(
                "expected {} uv floats for {} vertices, got {}",
                vertex_count * 2,
                vertex_count,
                self.uvs.len()
            )));
        }
        if self.indices.is_empty() {
            return Err(Error::InvalidData("geometry has no indices".to_string()));
        }
        if vertex_count > usize::from(u16::MAX) + 1 {
            return Err(Error::InvalidData(format!(
                "{vertex_count} vertices exceeds the {} a u16 index buffer can address; split the \
                 mesh",
                usize::from(u16::MAX) + 1
            )));
        }
        if let Some(&worst) = self.indices.iter().max() {
            if usize::from(worst) >= vertex_count {
                return Err(Error::InvalidData(format!(
                    "index {worst} addresses past the {vertex_count} vertices supplied"
                )));
            }
        }
        if self.submeshes.is_empty() {
            return Err(Error::InvalidData("geometry has no submeshes".to_string()));
        }
        for submesh in &self.submeshes {
            let end = submesh.index_start as usize + submesh.index_count as usize;
            if end > self.indices.len() {
                return Err(Error::InvalidData(format!(
                    "submesh {:?} spans indices {}..{} but only {} were supplied",
                    submesh.name,
                    submesh.index_start,
                    end,
                    self.indices.len()
                )));
            }
        }
        Ok(())
    }

    fn bounds(&self) -> Aabb {
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for point in self.positions.chunks_exact(3) {
            min.x = min.x.min(point[0]);
            min.y = min.y.min(point[1]);
            min.z = min.z.min(point[2]);
            max.x = max.x.max(point[0]);
            max.y = max.y.max(point[1]);
            max.z = max.z.max(point[2]);
        }
        Aabb { min, max }
    }

    fn submeshes_for(&self, hash_of: impl Fn(&str) -> u32) -> Vec<Submesh> {
        let vertex_count = self.vertex_count() as u32;
        self.submeshes
            .iter()
            .map(|range| {
                let end = (range.index_start + range.index_count) as usize;
                let used = &self.indices[range.index_start as usize..end];
                let (min_vertex, max_vertex) = match (used.iter().min(), used.iter().max()) {
                    (Some(&lo), Some(&hi)) => (u32::from(lo), u32::from(hi)),
                    _ => (0, vertex_count.saturating_sub(1)),
                };
                Submesh {
                    hash: hash_of(&range.name),
                    name: range.name.clone(),
                    index_start: range.index_start,
                    index_count: range.index_count,
                    min_vertex,
                    max_vertex,
                }
            })
            .collect()
    }
}

/** The vertex layout a newly added model is given: position, normal and one UV channel as plain
`f32`, matching the most common description in real files. Reused when an existing description
already matches so the file does not accumulate duplicates. */
fn default_description() -> VertexDescription {
    VertexDescription {
        usage: VertexUsage::Static,
        elements: vec![
            VertexElement {
                name: ElementName::Position,
                format: ElementFormat::XyzFloat32,
            },
            VertexElement {
                name: ElementName::Normal,
                format: ElementFormat::XyzFloat32,
            },
            VertexElement {
                name: ElementName::Texcoord0,
                format: ElementFormat::XyFloat32,
            },
        ],
    }
}

impl MapGeometry {
    /** The description that decodes each of a model's vertex buffers, paired with that buffer's id.

    A model stores one `vertex_description_id` but may reference several buffers. The descriptions
    are consumed consecutively from that id: the first buffer uses it, the second the one after it,
    and so on. Riot splits a vertex that way — positions and normals in one stream, texture
    coordinates in the next — so treating the single stored id as describing every buffer silently
    loses every attribute past the first stream. */
    fn buffer_layouts(&self, index: usize) -> Result<Vec<(i32, VertexDescription)>> {
        let model = self
            .models
            .get(index)
            .ok_or_else(|| Error::InvalidData(format!("no model at index {index}")))?;

        let first = model.vertex_description_id as usize;
        model
            .vertex_buffer_ids
            .iter()
            .enumerate()
            .map(|(offset, &id)| {
                let description = self
                    .vertex_descriptions
                    .get(first + offset)
                    .ok_or_else(|| {
                        Error::InvalidData(format!(
                            "model {index} needs vertex description {} for its buffer {offset} but \
                             the file only has {}",
                            first + offset,
                            self.vertex_descriptions.len()
                        ))
                    })?
                    .clone();
                Ok((id, description))
            })
            .collect()
    }

    /** Replaces the geometry of the model at `index`, keeping its transform, layer, lighting,
    texture overrides, render flags and scene-graph association. Submesh names come from the supplied
    geometry, so a caller that renames or re-splits materials is reflected in the file.

    The model keeps the vertex layout it already had, across every stream it was split over.
    Attributes the layout does not carry are dropped, and attributes it carries that the geometry
    does not supply are written as zero.

    `bounds` is recomputed from the new positions. */
    pub fn replace_geometry(&mut self, index: usize, geometry: &Geometry) -> Result<()> {
        geometry.validate()?;
        let layouts = self.buffer_layouts(index)?;
        let model = &self.models[index];

        let buffer_layers: Vec<u8> = layouts
            .iter()
            .map(|(id, _)| {
                usize::try_from(*id)
                    .ok()
                    .and_then(|id| self.vertex_buffers.get(id))
                    .map_or(0, |buffer| buffer.layer)
            })
            .collect();
        let index_layer = usize::try_from(model.index_buffer_id)
            .ok()
            .and_then(|id| self.index_buffers.get(id))
            .map_or(0, |buffer| buffer.layer);

        let descriptions: Vec<VertexDescription> = layouts.into_iter().map(|(_, d)| d).collect();
        let new_vertex_ids = self.write_vertex_buffers(&descriptions, geometry, &buffer_layers)?;
        self.index_buffers.push(IndexBuffer {
            layer: index_layer,
            indices: geometry.indices.clone(),
        });
        let new_index_id = (self.index_buffers.len() - 1) as i32;

        let model = &mut self.models[index];
        model.vertex_count = geometry.vertex_count() as u32;
        model.vertex_buffer_ids = new_vertex_ids;
        model.index_buffer_id = new_index_id;
        model.index_count = geometry.indices.len() as u32;
        model.submeshes = geometry.submeshes_for(rs_hash::fnv1a);
        model.bounds = geometry.bounds();
        Ok(())
    }

    /** Appends a model built from `geometry`, placed by `transform` and drawn on `layer`.

    The new model gets a position/normal/uv0 `f32` layout, no baked lighting and no scene-graph
    association, which is what a model authored outside Riot's tools can honestly claim. It inherits
    nothing from the rest of the file, so a map it is added to keeps rendering exactly as before plus
    the new mesh. Returns the index of the appended model. */
    pub fn add_model(
        &mut self,
        name: impl Into<String>,
        geometry: &Geometry,
        transform: Mat4,
        layer: u8,
    ) -> Result<usize> {
        geometry.validate()?;

        let description = default_description();
        let description_id = match self
            .vertex_descriptions
            .iter()
            .position(|existing| *existing == description)
        {
            Some(id) => id,
            None => {
                self.vertex_descriptions.push(description.clone());
                self.vertex_descriptions.len() - 1
            }
        } as u32;

        let vertex_buffer_ids =
            self.write_vertex_buffers(std::slice::from_ref(&description), geometry, &[0])?;
        self.index_buffers.push(IndexBuffer {
            layer: 0,
            indices: geometry.indices.clone(),
        });

        self.models.push(MapModel {
            name: name.into(),
            vertex_count: geometry.vertex_count() as u32,
            vertex_description_id: description_id,
            vertex_buffer_ids,
            index_count: geometry.indices.len() as u32,
            index_buffer_id: (self.index_buffers.len() - 1) as i32,
            layer,
            unknown_v18: 0,
            bucket_grid_hash: 0,
            submeshes: geometry.submeshes_for(rs_hash::fnv1a),
            disable_backface_culling: false,
            bounds: geometry.bounds(),
            transform,
            quality: ALL_QUALITIES,
            layer_transition: 0,
            render_flags: 0,
            point_light: None,
            spherical_harmonics: if self.version < 9 {
                Some([Vec3::ZERO; 9])
            } else {
                None
            },
            baked_light: AssetChannel::empty(),
            stationary_light: AssetChannel::empty(),
            texture_overrides: Vec::new(),
            baked_paint_scale_offset: [0.0; 4],
            baked_paint: if (12..=16).contains(&self.version) {
                Some(AssetChannel::empty())
            } else {
                None
            },
        });
        Ok(self.models.len() - 1)
    }

    /** Removes the model at `index`. The buffers it used are left in place because other models may
    index the same bytes and every remaining model's buffer references are positional. */
    pub fn remove_model(&mut self, index: usize) -> Result<MapModel> {
        if index >= self.models.len() {
            return Err(Error::InvalidData(format!("no model at index {index}")));
        }
        Ok(self.models.remove(index))
    }

    /** Writes `geometry` into one newly appended vertex buffer per entry of `descriptions`, and
    returns their ids. Each buffer receives only the attributes its own description declares, which
    is how a model split across several streams is laid out: positions and normals in one, texture
    coordinates in the next. An attribute a description declares but the geometry does not supply is
    left zeroed rather than refused, so replacing a mesh that has no UVs into a layout that expects
    them still produces a file the game can load. */
    fn write_vertex_buffers(
        &mut self,
        descriptions: &[VertexDescription],
        geometry: &Geometry,
        layers: &[u8],
    ) -> Result<Vec<i32>> {
        let vertex_count = geometry.vertex_count();
        let mut ids = Vec::with_capacity(descriptions.len());
        for (offset, description) in descriptions.iter().enumerate() {
            let stride = description.vertex_size();
            if stride == 0 {
                return Err(Error::InvalidData(format!(
                    "vertex description for buffer {offset} has a zero stride"
                )));
            }
            let mut data = vec![0u8; vertex_count * stride];
            for (name, values) in [
                (ElementName::Position, &geometry.positions),
                (ElementName::Normal, &geometry.normals),
                (ElementName::Texcoord0, &geometry.uvs),
            ] {
                if !values.is_empty() && description.element(name).is_some() {
                    description.write_attribute(&mut data, name, values)?;
                }
            }
            self.vertex_buffers.push(VertexBuffer {
                layer: layers.get(offset).copied().unwrap_or(0),
                data,
            });
            ids.push((self.vertex_buffers.len() - 1) as i32);
        }
        Ok(ids)
    }

    /** Reads a model's geometry back in the same plain form [`Self::replace_geometry`] takes, so a
    caller can round-trip a model through an editor without knowing the layout. Attributes the
    layout does not carry come back empty. */
    pub fn geometry(&self, index: usize) -> Result<Geometry> {
        let layouts = self.buffer_layouts(index)?;
        let model = &self.models[index];
        let vertex_count = model.vertex_count as usize;

        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        for (id, description) in &layouts {
            let Some(buffer) = usize::try_from(*id)
                .ok()
                .and_then(|id| self.vertex_buffers.get(id))
            else {
                continue;
            };
            for (name, slot) in [
                (ElementName::Position, &mut positions),
                (ElementName::Normal, &mut normals),
                (ElementName::Texcoord0, &mut uvs),
            ] {
                if slot.is_empty() {
                    if let Some(values) =
                        description.read_attribute(&buffer.data, name, vertex_count)?
                    {
                        *slot = values;
                    }
                }
            }
        }

        let indices = usize::try_from(model.index_buffer_id)
            .ok()
            .and_then(|id| self.index_buffers.get(id))
            .map(|buffer| buffer.indices.clone())
            .unwrap_or_default();

        Ok(Geometry {
            positions,
            normals,
            uvs,
            indices,
            submeshes: model
                .submeshes
                .iter()
                .map(|submesh| SubmeshRange {
                    name: submesh.name.clone(),
                    index_start: submesh.index_start,
                    index_count: submesh.index_count,
                })
                .collect(),
        })
    }
}

/// `quality` is a bitmask of the detail levels a model draws at; every real model sets all five.
const ALL_QUALITIES: u8 = 0b0001_1111;
