/*!
Sampling and skeleton posing. An [`Animation`] holds explicit per-joint keyframes; this module
turns those into a skeleton pose at an arbitrary time and composes the joint hierarchy into the
world-space and skinning matrices a renderer or exporter needs.

Sampling a track locates the two keyframes bracketing the requested time and interpolates between
them, so it is independent of the frame rate the clip was authored at and of whether the source
container was uncompressed or compressed. Joints the animation does not drive keep their bind pose
rather than collapsing to identity, which is what makes a partial clip (a clip that animates only
an arm, say) pose the whole rig correctly.
*/

use rs_math::{Mat4, Quat, Vec3};

use crate::animation::{AnimTrack, Animation};
use crate::skeleton::Skeleton;

/// A local translation/rotation/scale triple, the form both keyframes and bind poses take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// Composed 4x4 matrix, scale applied first and translation last.
    pub fn to_matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /** Interpolates towards `other` by `amount`, clamped to `0..=1`. Rotation takes the shortest
    arc: a quaternion and its negation describe the same orientation, so an unadjusted pair can
    otherwise spin the long way around. */
    pub fn lerp(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let end = if self.rotation.dot(other.rotation) < 0.0 {
            -other.rotation
        } else {
            other.rotation
        };
        Self {
            translation: self.translation.lerp(other.translation, amount),
            rotation: self.rotation.slerp(end, amount).normalize(),
            scale: self.scale.lerp(other.scale, amount),
        }
    }
}

impl AnimTrack {
    /// Time of the last keyframe, or `0.0` when the track is empty.
    pub fn duration(&self) -> f32 {
        self.frames.last().map(|frame| frame.time).unwrap_or(0.0)
    }

    /** Samples this track at `time` seconds, interpolating between the two keyframes that bracket
    it. Times outside the track clamp to its first or last keyframe rather than extrapolating, and a
    NaN time resolves to the first keyframe. Returns `None` only for an empty track. */
    pub fn sample(&self, time: f32) -> Option<Transform> {
        let first = self.frames.first()?;
        let as_transform = |frame: &crate::animation::AnimFrame| {
            Transform::new(frame.translation, frame.rotation, frame.scale)
        };

        if time.is_nan() || time <= first.time {
            return Some(as_transform(first));
        }

        let next = self.frames.partition_point(|frame| frame.time <= time);
        let Some(after) = self.frames.get(next) else {
            return self.frames.last().map(as_transform);
        };
        let before = &self.frames[next - 1];

        let span = after.time - before.time;
        let amount = if span > 0.0 {
            (time - before.time) / span
        } else {
            0.0
        };
        Some(as_transform(before).lerp(as_transform(after), amount))
    }
}

impl Animation {
    /// Time of the last keyframe across every track, in seconds.
    pub fn duration(&self) -> f32 {
        self.tracks
            .iter()
            .map(AnimTrack::duration)
            .fold(0.0, f32::max)
    }

    /// The track driving `joint_hash`, if this animation has one.
    pub fn track(&self, joint_hash: u32) -> Option<&AnimTrack> {
        self.tracks
            .iter()
            .find(|track| track.joint_hash == joint_hash)
    }

    /// Samples the track driving `joint_hash` at `time`, or `None` when the animation does not
    /// drive that joint.
    pub fn sample_joint(&self, joint_hash: u32, time: f32) -> Option<Transform> {
        self.track(joint_hash)?.sample(time)
    }
}

/** A skeleton's joints posed at one instant, one local [`Transform`] per joint in skeleton order.

Build one with [`Pose::sample`] to evaluate an animation, or [`Pose::rest`] for the skeleton's own
bind pose. The transforms are local to each joint's parent; [`Pose::global_transforms`] composes
the hierarchy and [`Pose::skinning_matrices`] produces what a skinned mesh binds. */
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    locals: Vec<Transform>,
}

impl Pose {
    /// The skeleton's own bind pose, taken from each joint's local transform.
    pub fn rest(skeleton: &Skeleton) -> Self {
        Self {
            locals: skeleton
                .joints
                .iter()
                .map(|joint| {
                    Transform::new(
                        joint.local_translation,
                        joint.local_rotation,
                        joint.local_scale,
                    )
                })
                .collect(),
        }
    }

    /** Poses `skeleton` with `animation` at `time` seconds. Joints the animation drives take their
    sampled transform; the rest keep their bind pose, so a clip that animates part of a rig leaves
    the remainder standing correctly rather than collapsing it to identity. */
    pub fn sample(skeleton: &Skeleton, animation: &Animation, time: f32) -> Self {
        Self {
            locals: skeleton
                .joints
                .iter()
                .map(|joint| {
                    animation.sample_joint(joint.hash, time).unwrap_or_else(|| {
                        Transform::new(
                            joint.local_translation,
                            joint.local_rotation,
                            joint.local_scale,
                        )
                    })
                })
                .collect(),
        }
    }

    /// Local transforms, one per skeleton joint, in skeleton order.
    pub fn locals(&self) -> &[Transform] {
        &self.locals
    }

    /// Local transform of joint `index`.
    pub fn local(&self, index: usize) -> Option<Transform> {
        self.locals.get(index).copied()
    }

    /// Overwrites the local transform of joint `index`, ignoring an out-of-range index.
    pub fn set_local(&mut self, index: usize, transform: Transform) {
        if let Some(slot) = self.locals.get_mut(index) {
            *slot = transform;
        }
    }

    /** Composes each joint with its parent chain, producing model-space matrices in skeleton order.

    Joints are resolved parent-before-child without assuming the file stores them in that order.
    A joint whose parent index is out of range is treated as a root, and a parent cycle resolves to
    the joint's own local transform, so malformed input degrades instead of looping forever. */
    pub fn global_transforms(&self, skeleton: &Skeleton) -> Vec<Mat4> {
        let count = skeleton.joints.len().min(self.locals.len());
        let locals: Vec<Mat4> = self.locals[..count]
            .iter()
            .map(|transform| transform.to_matrix())
            .collect();

        let mut globals = locals.clone();
        let mut resolved = vec![false; count];
        let mut pending = count;

        while pending > 0 {
            let mut progressed = false;
            for index in 0..count {
                if resolved[index] {
                    continue;
                }
                let parent = skeleton.joints[index].parent_id;
                let parent = usize::try_from(parent).ok().filter(|&p| p < count);
                match parent {
                    None => {
                        resolved[index] = true;
                    }
                    Some(parent) if resolved[parent] => {
                        globals[index] = globals[parent] * locals[index];
                        resolved[index] = true;
                    }
                    Some(_) => continue,
                }
                pending -= 1;
                progressed = true;
            }
            if !progressed {
                break;
            }
        }

        globals
    }

    /** Matrices that transform a bound vertex from bind pose into this pose: each joint's model-space
    transform times its inverse bind transform, in skeleton order. This is what a skinned mesh
    multiplies its weighted joint influences by. */
    pub fn skinning_matrices(&self, skeleton: &Skeleton) -> Vec<Mat4> {
        self.global_transforms(skeleton)
            .into_iter()
            .zip(skeleton.joints.iter())
            .map(|(global, joint)| global * joint.inverse_bind_transform())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{AnimFrame, AnimTrack};
    use crate::skeleton::Joint;

    fn joint(hash: u32, id: i16, parent_id: i16, translation: Vec3) -> Joint {
        Joint {
            name: format!("joint{id}"),
            flags: 0,
            id,
            parent_id,
            radius: 0.0,
            hash,
            local_translation: translation,
            local_scale: Vec3::ONE,
            local_rotation: Quat::IDENTITY,
            inverse_bind_translation: -translation,
            inverse_bind_scale: Vec3::ONE,
            inverse_bind_rotation: Quat::IDENTITY,
        }
    }

    fn track(hash: u32, frames: &[(f32, Vec3)]) -> AnimTrack {
        AnimTrack {
            joint_hash: hash,
            frames: frames
                .iter()
                .map(|&(time, translation)| {
                    AnimFrame::new(time, Quat::IDENTITY, translation, Vec3::ONE)
                })
                .collect(),
        }
    }

    #[test]
    fn sampling_interpolates_between_keyframes() {
        let track = track(1, &[(0.0, Vec3::ZERO), (1.0, Vec3::new(10.0, 0.0, 0.0))]);
        let mid = track.sample(0.5).unwrap();
        assert!((mid.translation.x - 5.0).abs() < 1e-5, "{mid:?}");
    }

    #[test]
    fn sampling_clamps_outside_the_track() {
        let track = track(1, &[(1.0, Vec3::ZERO), (2.0, Vec3::new(4.0, 0.0, 0.0))]);
        assert_eq!(track.sample(-5.0).unwrap().translation, Vec3::ZERO);
        assert_eq!(
            track.sample(99.0).unwrap().translation,
            Vec3::new(4.0, 0.0, 0.0)
        );
    }

    #[test]
    fn sampling_lands_exactly_on_keyframes() {
        let track = track(
            1,
            &[
                (0.0, Vec3::ZERO),
                (1.0, Vec3::new(3.0, 0.0, 0.0)),
                (2.0, Vec3::new(9.0, 0.0, 0.0)),
            ],
        );
        assert_eq!(
            track.sample(1.0).unwrap().translation,
            Vec3::new(3.0, 0.0, 0.0)
        );
        assert_eq!(
            track.sample(2.0).unwrap().translation,
            Vec3::new(9.0, 0.0, 0.0)
        );
    }

    #[test]
    fn empty_track_samples_to_nothing() {
        assert!(AnimTrack::new(1).sample(0.0).is_none());
    }

    #[test]
    fn rotation_interpolation_takes_the_shortest_arc() {
        let start = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
        let flipped = Transform::new(Vec3::ZERO, -Quat::IDENTITY, Vec3::ONE);
        let mid = start.lerp(flipped, 0.5);
        assert!(
            (mid.rotation.dot(Quat::IDENTITY).abs() - 1.0).abs() < 1e-5,
            "halfway between a rotation and its negation must stay put: {:?}",
            mid.rotation
        );
    }

    #[test]
    fn undriven_joints_keep_their_bind_pose() {
        let mut skeleton = Skeleton::new();
        skeleton.joints = vec![
            joint(1, 0, -1, Vec3::ZERO),
            joint(2, 1, 0, Vec3::new(0.0, 7.0, 0.0)),
        ];
        let mut animation = Animation::new(30.0);
        animation.tracks = vec![track(1, &[(0.0, Vec3::ZERO)])];

        let pose = Pose::sample(&skeleton, &animation, 0.0);
        assert_eq!(pose.local(1).unwrap().translation, Vec3::new(0.0, 7.0, 0.0));
    }

    #[test]
    fn globals_compose_the_parent_chain() {
        let mut skeleton = Skeleton::new();
        skeleton.joints = vec![
            joint(1, 0, -1, Vec3::new(1.0, 0.0, 0.0)),
            joint(2, 1, 0, Vec3::new(2.0, 0.0, 0.0)),
            joint(3, 2, 1, Vec3::new(4.0, 0.0, 0.0)),
        ];
        let globals = Pose::rest(&skeleton).global_transforms(&skeleton);
        assert_eq!(globals[2].w_axis.truncate(), Vec3::new(7.0, 0.0, 0.0));
    }

    #[test]
    fn globals_resolve_children_stored_before_parents() {
        let mut skeleton = Skeleton::new();
        skeleton.joints = vec![
            joint(3, 0, 1, Vec3::new(4.0, 0.0, 0.0)),
            joint(2, 1, 2, Vec3::new(2.0, 0.0, 0.0)),
            joint(1, 2, -1, Vec3::new(1.0, 0.0, 0.0)),
        ];
        let globals = Pose::rest(&skeleton).global_transforms(&skeleton);
        assert_eq!(globals[0].w_axis.truncate(), Vec3::new(7.0, 0.0, 0.0));
    }

    #[test]
    fn parent_cycles_do_not_hang() {
        let mut skeleton = Skeleton::new();
        skeleton.joints = vec![
            joint(1, 0, 1, Vec3::new(1.0, 0.0, 0.0)),
            joint(2, 1, 0, Vec3::new(2.0, 0.0, 0.0)),
        ];
        let globals = Pose::rest(&skeleton).global_transforms(&skeleton);
        assert_eq!(globals.len(), 2);
        assert_eq!(globals[0].w_axis.truncate(), Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn out_of_range_parent_is_treated_as_a_root() {
        let mut skeleton = Skeleton::new();
        skeleton.joints = vec![joint(1, 0, 40, Vec3::new(5.0, 0.0, 0.0))];
        let globals = Pose::rest(&skeleton).global_transforms(&skeleton);
        assert_eq!(globals[0].w_axis.truncate(), Vec3::new(5.0, 0.0, 0.0));
    }

    #[test]
    fn skinning_matrix_of_the_bind_pose_is_identity() {
        let mut skeleton = Skeleton::new();
        skeleton.joints = vec![joint(1, 0, -1, Vec3::new(3.0, 2.0, 1.0))];
        let skinning = Pose::rest(&skeleton).skinning_matrices(&skeleton);
        assert!(
            (skinning[0] - Mat4::IDENTITY)
                .to_cols_array()
                .iter()
                .all(|v| v.abs() < 1e-5),
            "{:?}",
            skinning[0]
        );
    }

    #[test]
    fn animation_duration_spans_every_track() {
        let mut animation = Animation::new(30.0);
        animation.tracks = vec![
            track(1, &[(0.0, Vec3::ZERO), (0.5, Vec3::ZERO)]),
            track(2, &[(0.0, Vec3::ZERO), (2.5, Vec3::ZERO)]),
        ];
        assert_eq!(animation.duration(), 2.5);
        assert_eq!(Animation::new(30.0).duration(), 0.0);
    }
}
