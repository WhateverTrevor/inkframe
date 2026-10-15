//! Inverse kinematics: posing a chain of bones by where its end should go.
//!
//! The bones are the parent chain from `rig.rs`: each layer is a bone that
//! turns at its pivot, and its children hang from it. Posing that by
//! turning each joint (forward kinematics) is precise but slow for things
//! like "put the hand on the table". With IK the animator drags the hand
//! and the elbow and shoulder turn to follow, as with Animate's Bone tool or
//! Harmony's IK tool.
//!
//! The solver is cyclic coordinate descent: going down the chain from the
//! joint nearest the root, turn each joint so the end points at the
//! target, then repeat a few times. It only ever changes rotations, which
//! is what a joint is, and settles on the nearest pose that reaches,
//! rather than flipping the elbow the other way. A chain that can't reach
//! stretches straight toward the target.
//!
//! Its one blind spot is a chain that is dead straight with the target on
//! the same line but closer: every joint already points at the target, so
//! none turns. When a pass makes no headway there, the joints past the
//! first are bent a little (toward the middle of their limits, so the right
//! way for an elbow) and the solver carries on from there.
//!
//! The chain is the dragged layer and every parent above it, except the
//! topmost (the root, say a torso), which holds the rest in place. Joints
//! can have rotation limits, so an elbow doesn't bend backward.

use serde::{Deserialize, Serialize};

use crate::document::{Document, LayerId};
use crate::geometry::Point;

/// How far a joint may turn, as the lowest and highest values its
/// rotation may take, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RotationLimits {
    pub min: f32,
    pub max: f32,
}

impl RotationLimits {
    pub fn clamp(&self, rotation: f32) -> f32 {
        rotation.clamp(self.min.min(self.max), self.max.max(self.min))
    }
}

/// Passes over the chain before giving up on reaching exactly. Each pass
/// usually halves the gap or better, so this leaves room for a few bends
/// out of a stall.
const PASSES: usize = 24;
/// How much a stalled joint is bent to get going again, in degrees.
const NUDGE: f32 = 10.0;
/// Close enough, in timeline units.
const REACHED: f32 = 0.05;

impl Document {
    /// The joints IK turns when `layer` is dragged: the layer itself and its
    /// parents up to, but not including, the root of its rig. Empty for a
    /// layer without a parent, which has nothing to pull against.
    pub fn ik_chain(&self, layer: LayerId) -> Vec<LayerId> {
        let mut chain = Vec::new();
        let mut current = self.layer(layer);
        while let Some(joint) = current {
            let Some(parent) = self.parent_of(joint) else { break };
            chain.push(joint.id);
            if chain.len() > 64 {
                break;
            }
            current = Some(parent);
        }
        chain
    }

    /// Turns the joints above `effector` so the point `local` on it (in its
    /// own coordinates) moves as near `target` (in its timeline's
    /// coordinates) as the chain allows, keying the rotations on `frame`.
    /// Returns how far off it ended up.
    pub fn reach(&mut self, effector: LayerId, local: Point, target: Point, frame: u32) -> f32 {
        // From the joint nearest the root outward: the shoulder swings the
        // arm toward the target, then the elbow bends to close the gap.
        let mut chain = self.ik_chain(effector);
        chain.reverse();
        let mut gap = self.ik_gap(effector, local, target, frame);
        for pass in 0..PASSES {
            if gap <= REACHED {
                break;
            }
            for &joint in &chain {
                self.aim_joint(joint, effector, local, target, frame);
            }
            let before = gap;
            gap = self.ik_gap(effector, local, target, frame);
            let stalled = gap > before * 0.99 && pass + 1 < PASSES;
            if stalled && self.overreaches(chain[0], effector, local, target, frame) {
                for &joint in &chain[1..] {
                    self.nudge_joint(joint, frame);
                }
            }
        }
        gap
    }

    /// Whether the chain's end is further from its base joint than the
    /// target is: stuck straight past it, rather than unable to reach.
    fn overreaches(&self, base: LayerId, effector: LayerId, local: Point, target: Point, frame: u32) -> bool {
        let Some(layer) = self.layer(base) else { return false };
        let Some(end) = self.ik_end(effector, local, frame) else { return false };
        let pivot = self.layer_matrix(layer, frame).apply(layer.pivot);
        target.distance(pivot) < end.distance(pivot)
    }

    /// Bends a joint a little toward the middle of its limits (or just a
    /// little, if it has none), to get the solver out of a stall.
    fn nudge_joint(&mut self, joint: LayerId, frame: u32) {
        let Some(layer) = self.layer(joint) else { return };
        let mut pose = self.pose(layer, frame);
        let direction =
            layer.rotation_limits.map_or(1.0, |l| ((l.min + l.max) / 2.0 - pose.rotation).signum());
        pose.rotation += NUDGE * direction;
        if let Some(limits) = layer.rotation_limits {
            pose.rotation = limits.clamp(pose.rotation);
        }
        self.set_pose(joint, frame, pose);
    }

    fn ik_gap(&self, effector: LayerId, local: Point, target: Point, frame: u32) -> f32 {
        self.ik_end(effector, local, frame).map_or(0.0, |end| end.distance(target))
    }

    fn ik_end(&self, effector: LayerId, local: Point, frame: u32) -> Option<Point> {
        let layer = self.layer(effector)?;
        Some(self.layer_matrix(layer, frame).apply(local))
    }

    /// One step of the solver: turns `joint` about its pivot so the chain's
    /// end points at the target.
    fn aim_joint(&mut self, joint: LayerId, effector: LayerId, local: Point, target: Point, frame: u32) {
        let Some(layer) = self.layer(joint) else { return };
        let Some(end) = self.ik_end(effector, local, frame) else { return };
        let pivot = self.layer_matrix(layer, frame).apply(layer.pivot);
        let (to_end, to_target) = (end - pivot, target - pivot);
        // A joint sitting on the end or the target can't aim either way.
        if to_end.length() < 1e-4 || to_target.length() < 1e-4 {
            return;
        }
        let turn = (to_target.y.atan2(to_target.x) - to_end.y.atan2(to_end.x)).to_degrees();
        let turn = (turn + 180.0).rem_euclid(360.0) - 180.0;
        // Under a mirrored parent, turning the joint one way turns it the
        // other way on the stage.
        let parent = self.parent_matrix(layer, frame);
        let mirrored = parent.a * parent.d - parent.b * parent.c < 0.0;
        let turn = if mirrored { -turn } else { turn };

        let mut pose = self.pose(layer, frame);
        pose.rotation += turn;
        if let Some(limits) = layer.rotation_limits {
            pose.rotation = limits.clamp(pose.rotation);
        }
        self.set_pose(joint, frame, pose);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, Transform};

    /// A torso at (0, 0) with an upper arm and a forearm, each 100 long,
    /// reaching right along the x axis from a shoulder at (100, 0): the hand
    /// end is at (300, 0). Returns the document and the three layers.
    fn arm() -> (Document, LayerId, LayerId, LayerId) {
        let mut doc = Document::default();
        let torso = doc.add_layer("Torso", Vec::<Element>::new(), 0, Transform::at(Point::ZERO));
        let upper = doc.add_layer("Upper arm", Vec::new(), 0, Transform::at(Point::new(100.0, 0.0)));
        let fore = doc.add_layer("Forearm", Vec::new(), 0, Transform::at(Point::new(200.0, 0.0)));
        doc.set_parent(upper, Some(torso), 0);
        doc.set_parent(fore, Some(upper), 0);
        (doc, torso, upper, fore)
    }

    /// The end of the forearm, 100 along it from its pivot.
    const HAND: Point = Point { x: 100.0, y: 0.0 };

    fn hand_at(doc: &Document, fore: LayerId) -> Point {
        doc.layer_matrix(doc.layer(fore).unwrap(), 0).apply(HAND)
    }

    #[test]
    fn the_chain_stops_below_the_root() {
        let (doc, torso, upper, fore) = arm();
        assert_eq!(doc.ik_chain(fore), [fore, upper]);
        assert!(doc.ik_chain(torso).is_empty());
    }

    #[test]
    fn a_reachable_target_is_reached() {
        let (mut doc, torso, _, fore) = arm();
        let target = Point::new(200.0, 120.0);
        let gap = doc.reach(fore, HAND, target, 0);
        assert!(gap < 0.1, "gap {gap}");
        assert!(hand_at(&doc, fore).distance(target) < 0.1);
        // The shoulder stayed where it was, and the root didn't turn.
        assert_eq!(doc.pose(doc.layer(torso).unwrap(), 0).rotation, 0.0);
    }

    #[test]
    fn an_unreachable_target_straightens_the_arm_toward_it() {
        let (mut doc, _, upper, fore) = arm();
        // Straight down from the shoulder, but 500 away: the arm is 200 long.
        let gap = doc.reach(fore, HAND, Point::new(100.0, 500.0), 0);
        assert!((gap - 300.0).abs() < 0.5, "gap {gap}");
        assert!(hand_at(&doc, fore).distance(Point::new(100.0, 200.0)) < 0.5);
        let upper = doc.pose(doc.layer(upper).unwrap(), 0).rotation;
        assert!((upper - 90.0).abs() < 0.5, "the arm points down: {upper}");
    }

    #[test]
    fn limits_keep_a_joint_in_range() {
        let (mut doc, _, upper, fore) = arm();
        doc.layer_mut(fore).unwrap().rotation_limits = Some(RotationLimits { min: 0.0, max: 150.0 });
        // A target that would need the elbow to bend the other way.
        doc.reach(fore, HAND, Point::new(200.0, -120.0), 0);
        let elbow = doc.pose(doc.layer(fore).unwrap(), 0).rotation;
        assert!((0.0..=150.0).contains(&elbow), "elbow {elbow}");
        // It still got there, by bending the allowed way.
        assert!(hand_at(&doc, fore).distance(Point::new(200.0, -120.0)) < 0.5);
        let upper = doc.pose(doc.layer(upper).unwrap(), 0).rotation;
        assert!(upper < -50.0, "the upper arm went up past the target: {upper}");
    }

    #[test]
    fn a_straight_arm_bends_to_reach_a_nearer_point_in_line() {
        let (mut doc, _, _, fore) = arm();
        let gap = doc.reach(fore, HAND, Point::new(250.0, 0.0), 0);
        assert!(gap < 0.1, "gap {gap}");
    }

    #[test]
    fn reaching_keys_the_rotations_on_the_frame() {
        let (mut doc, _, upper, fore) = arm();
        doc.reach(fore, HAND, Point::new(200.0, 120.0), 12);
        for joint in [upper, fore] {
            assert!(doc.layer(joint).unwrap().track.rotation.key_at(12).is_some());
        }
        // Frame 0 is untouched.
        assert!(hand_at(&doc, fore).distance(Point::new(300.0, 0.0)) < 1e-3);
    }
}
