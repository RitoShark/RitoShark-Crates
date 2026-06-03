use rs_math::{Vec2, Vec3};

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
    pub central: Vec3,
    pub positions: Vec<Vec3>,
    pub colors: Option<Vec<[u8; 4]>>,
    pub faces: Vec<StaticMeshFace>,
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
}
