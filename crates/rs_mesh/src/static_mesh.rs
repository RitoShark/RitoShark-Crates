use rs_math::{Aabb, Vec2, Vec3};

pub(crate) const SCB_MAGIC: &[u8; 8] = b"r3d2Mesh";
pub(crate) const SCO_MAGIC: &str = "[ObjectBegin]";

/// A single triangle of a [`StaticMesh`], carrying its own material and per-corner UVs.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticMeshFace {
    pub material: String,
    pub indices: [u32; 3],
    pub uvs: [Vec2; 3],
}

impl StaticMeshFace {
    pub fn new(material: impl Into<String>, indices: [u32; 3], uvs: [Vec2; 3]) -> Self {
        Self {
            material: material.into(),
            indices,
            uvs,
        }
    }
}

/// A static (non-skinned) mesh shared by the binary `.scb` (`"r3d2Mesh"`) and text `.sco`
/// (`[ObjectBegin]`) formats: a position list plus per-face triangles with materials and UVs,
/// and optional per-vertex colors carried by `.scb` color layouts.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticMesh {
    pub name: String,
    /// `(major, minor)` version of the binary container; `(0, 0)` for the text `.sco` form.
    pub version: (u16, u16),
    /// Raw `r3d2Mesh` flag bits (`bit0` = `HasVcp`, `bit1` = `HasLocalOriginLocatorAndPivot`).
    /// Zero for the text `.sco` form, which has no flag word.
    pub flags: u32,
    /// On-disk axis-aligned bounds (`.scb` only); `min == max == 0` for `.sco`.
    pub bounding_box: Aabb,
    /// Raw `vertexType` word for `.scb` 3.2 files; `None` for older `.scb` and for `.sco`.
    pub vertex_type: Option<u32>,
    pub central: Vec3,
    pub positions: Vec<Vec3>,
    pub colors: Option<Vec<[u8; 4]>>,
    pub faces: Vec<StaticMeshFace>,
    /// Opaque bytes that follow the face list in `.scb` files (the per-face VCP RGB block and the
    /// local-origin/pivot vectors carried when the corresponding flag bits are set). Captured raw
    /// so that `from_scb_reader` -> `to_scb_writer` is byte-exact even though the exact layout of
    /// this tail is not modelled. Always empty for `.sco`.
    pub trailing: Vec<u8>,
}

impl StaticMesh {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn positions(&self) -> &[Vec3] {
        &self.positions
    }

    pub fn faces(&self) -> &[StaticMeshFace] {
        &self.faces
    }

    pub fn colors(&self) -> Option<&[[u8; 4]]> {
        self.colors.as_deref()
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Per-face-CORNER vertex colors from the `HasVcp` tail block (`flags` bit 0), as RGB triples
    /// in face order: `3 * faces().len()` entries, corner `k` of face `i` at `i * 3 + k`.
    ///
    /// Distinct from [`colors`](Self::colors), which is the per-VERTEX RGBA block selected by the
    /// 3.2 `vertexType` word. A file may carry either, both, or neither. Returns `None` when the
    /// flag is clear or the tail is too short to hold the block.
    pub fn vcp_colors(&self) -> Option<Vec<[u8; 3]>> {
        if self.flags & 0x1 == 0 {
            return None;
        }
        let need = self.faces.len() * 9;
        if self.trailing.len() < need {
            return None;
        }
        Some(
            self.trailing[..need]
                .chunks_exact(3)
                .map(|c| [c[0], c[1], c[2]])
                .collect(),
        )
    }

    /// The mesh's LOCAL ORIGIN and PIVOT from the tail, when present: `(local_origin, pivot)`.
    ///
    /// These are the two `Vec3`s written after the optional VCP block. The local origin repeats
    /// [`central`](Self::central) - a consumer that wants the mesh drawn about its own origin
    /// should translate the positions by `-central` (equivalently `-local_origin`).
    ///
    /// GATED ON BIT 2, not bit 1. The struct field doc calls bit 1
    /// `HasLocalOriginLocatorAndPivot`, but a real 3.2 file disproves that: Draven Skin68's
    /// `draven_skin68_weapontrail.scb` has `flags = 5` (bits 0 and 2, bit 1 CLEAR) and still
    /// carries the pair. Its tail is 7224 bytes for 800 faces, and only the per-corner layout
    /// divides it exactly - `800 * 9 = 7200`, leaving 24 = two `Vec3`s - so the pair is present
    /// while bit 1 is clear. Accepting either bit keeps older files working if bit 1 is in fact
    /// used by some writer.
    pub fn local_origin_and_pivot(&self) -> Option<(Vec3, Vec3)> {
        if self.flags & 0x6 == 0 {
            return None;
        }
        let skip = if self.flags & 0x1 != 0 {
            self.faces.len() * 9
        } else {
            0
        };
        let tail = self.trailing.get(skip..)?;
        if tail.len() < 24 {
            return None;
        }
        let f = |o: usize| f32::from_le_bytes([tail[o], tail[o + 1], tail[o + 2], tail[o + 3]]);
        Some((
            Vec3::new(f(0), f(4), f(8)),
            Vec3::new(f(12), f(16), f(20)),
        ))
    }

    /// Vertex positions translated so the mesh is centred on its own local origin
    /// ([`central`](Self::central)).
    ///
    /// A `.scb` stores its vertices in the authoring scene's space and carries that origin
    /// separately, so a consumer that places the mesh by an emitter/attachment position wants
    /// these rather than the raw [`positions`](Self::positions) - otherwise the geometry is drawn
    /// displaced by exactly `central`. Meshes authored about their own origin have `central == 0`
    /// and are unaffected.
    ///
    /// Draven Skin68's weapon trail is the case that shows it: its vertex AABB centre is
    /// `(-73.948, 119.003, 4.447)` and `central` is that same value, so the raw positions render
    /// ~119 units up and ~74 out from where the effect belongs.
    pub fn centred_positions(&self) -> Vec<Vec3> {
        self.positions.iter().map(|p| *p - self.central).collect()
    }

    /// The mesh's true axis-aligned bounds, falling back to the vertex extents when the on-disk
    /// [`bounding_box`](Self::bounding_box) is degenerate.
    ///
    /// Newer `.scb` writers leave the header AABB all-zero and the real extents implicit. Trusting
    /// it hands consumers a zero-size box - Maya's `lol_maya` importer builds its extents from that
    /// field and fails with `kInvalidParameter` on such a file, and any size-derived logic
    /// (bounding-sphere fits, scale heuristics) silently collapses. Draven Skin68's weapon trail
    /// writes `(0,0,0)-(0,0,0)` while its vertices span
    /// `(-124.47, 24.16, -78.24)` to `(-23.43, 213.84, 87.13)`.
    pub fn effective_bounds(&self) -> Aabb {
        let bb = self.bounding_box;
        let degenerate = bb.min == bb.max;
        if !degenerate || self.positions.is_empty() {
            return bb;
        }
        let mut min = self.positions[0];
        let mut max = self.positions[0];
        for p in &self.positions[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        Aabb::new(min, max)
    }
}
