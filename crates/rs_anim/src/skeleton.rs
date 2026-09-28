use rs_math::{Mat4, Quat, Vec3};

/// One joint (bone) of a [`Skeleton`].
#[derive(Clone, Debug, PartialEq)]
pub struct Joint {
    pub name: String,
    pub flags: u16,
    pub id: i16,
    pub parent_id: i16,
    pub radius: f32,
    pub hash: u32,
    pub local_translation: Vec3,
    pub local_scale: Vec3,
    pub local_rotation: Quat,
    pub inverse_bind_translation: Vec3,
    pub inverse_bind_scale: Vec3,
    pub inverse_bind_rotation: Quat,
}

impl Joint {
    /// Local transform composed from the stored translation, scale, and rotation.
    pub fn local_transform(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.local_scale,
            self.local_rotation,
            self.local_translation,
        )
    }

    /// Inverse bind transform composed from the stored translation, scale, and rotation.
    pub fn inverse_bind_transform(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.inverse_bind_scale,
            self.inverse_bind_rotation,
            self.inverse_bind_translation,
        )
    }
}

/// A League skeleton / rig (`.skl`).
///
/// Reads modern v0 and legacy v1/v2 rigs. Unchanged rigs retain their source layout;
/// edited rigs are written as modern v0.
#[derive(Clone, Debug, Default)]
pub struct Skeleton {
    pub flags: u16,
    pub name: String,
    pub asset: String,
    pub joints: Vec<Joint>,
    pub influences: Vec<u16>,
    pub(crate) raw: Option<Box<RawSkeleton>>,
}

#[derive(Clone, Debug)]
pub(crate) struct RawSkeleton {
    pub bytes: Vec<u8>,
    pub decoded: Skeleton,
}

impl PartialEq for Skeleton {
    fn eq(&self, other: &Self) -> bool {
        self.flags == other.flags
            && self.name == other.name
            && self.asset == other.asset
            && self.joints == other.joints
            && self.influences == other.influences
    }
}

impl Skeleton {
    /// Modern skeleton magic, found at byte offset 4 (bytes 0..4 hold the file size).
    pub const MAGIC: u32 = 0x22FD_4FC3;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }

    pub fn influences(&self) -> &[u16] {
        &self.influences
    }

    /// The joint with this name hash, if the skeleton has one.
    pub fn joint_by_hash(&self, hash: u32) -> Option<&Joint> {
        self.joints.iter().find(|joint| joint.hash == hash)
    }

    /// Index of the joint with this name hash, if the skeleton has one.
    pub fn joint_index_by_hash(&self, hash: u32) -> Option<usize> {
        self.joints.iter().position(|joint| joint.hash == hash)
    }
}
