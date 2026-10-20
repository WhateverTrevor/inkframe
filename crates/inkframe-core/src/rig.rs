//! Parenting and pivots: what makes a character rig.
//!
//! A layer can have a **parent**: another layer in the same timeline whose
//! motion (position, rotation, scale and skew, but not opacity) it
//! inherits, the way a forearm follows the upper arm, which follows the
//! torso (Harmony's pegs, OpenToonz's pegbars, Animate's layer
//! parenting). The layer's own transform is then relative to its parent,
//! so animating the torso carries everything attached to it along, and the
//! forearm only needs keys for how it bends at the elbow.
//!
//! Each layer also has a **pivot**: the point, in its own coordinates,
//! that it rotates and scales around (Animate's transformation point). For
//! a limb that's the joint, so rotating the forearm swings it from the elbow
//! instead of spinning it around its middle. The layer's position is where
//! its pivot sits.
//!
//! Putting it together, a point `p` on a layer lands in its timeline at
//!
//! ```text
//! parent's matrix · translate(position) · rotate · skew · scale · translate(−pivot) · p
//! ```
//!
//! [`Document::layer_matrix`] works that out; everything that draws or
//! edits layers goes through it.

use crate::document::{Document, Layer, LayerId};
use crate::geometry::{Affine, Point};

/// How many parents deep a chain may go. Setting a parent refuses loops, so
/// this only guards against a hand-edited file.
pub(crate) const MAX_DEPTH: usize = 64;

impl Document {
    /// Maps a layer's own coordinates (where its shapes are drawn) into its
    /// timeline's at `frame`, through its pivot, its pose and its parents.
    pub fn layer_matrix(&self, layer: &Layer, frame: u32) -> Affine {
        self.layer_matrix_at(layer, frame as f32)
    }

    /// [`Document::layer_matrix`] at any moment, between frames too: what
    /// motion blur samples.
    pub fn layer_matrix_at(&self, layer: &Layer, frame: f32) -> Affine {
        self.parent_matrix_at(layer, frame).pre_concat(self.own_matrix(layer, frame))
    }

    /// The part of [`Document::layer_matrix`] that comes from the layer's
    /// parents and the camera: it maps the space the layer's pose is in
    /// (its parent's coordinates) into the timeline's as shown. The
    /// identity for a layer with no parent and no camera.
    pub fn parent_matrix(&self, layer: &Layer, frame: u32) -> Affine {
        self.parent_matrix_at(layer, frame as f32)
    }

    fn parent_matrix_at(&self, layer: &Layer, frame: f32) -> Affine {
        let mut matrix = Affine::IDENTITY;
        let mut current = layer;
        for _ in 0..MAX_DEPTH {
            let Some(parent) = self.parent_of(current) else { break };
            matrix = self.own_matrix(parent, frame).pre_concat(matrix);
            current = parent;
        }
        // Above the top of the chain, the camera's view (see `camera.rs`).
        self.camera_for(current, frame).pre_concat(matrix)
    }

    /// Where a layer is placed through its parents, but not the camera: the
    /// space things in the scene stay put in while the camera moves.
    pub fn placement_without_camera(&self, layer: &Layer, frame: f32) -> Affine {
        let mut matrix = self.own_matrix(layer, frame);
        let mut current = layer;
        for _ in 0..MAX_DEPTH {
            let Some(parent) = self.parent_of(current) else { break };
            matrix = self.own_matrix(parent, frame).pre_concat(matrix);
            current = parent;
        }
        matrix
    }

    /// A layer's pose and pivot, without its parents.
    fn own_matrix(&self, layer: &Layer, frame: f32) -> Affine {
        self.pose_at(layer, frame).to_affine().pre_concat(Affine::translate(Point::ZERO - layer.pivot))
    }

    /// The layer's parent, if it has one that exists. A parent that has
    /// since moved to another timeline (converted into a symbol, say) no
    /// longer counts: the two aren't drawn in the same space any more.
    pub fn parent_of(&self, layer: &Layer) -> Option<&Layer> {
        let parent = self.layer(layer.parent?)?;
        let together = parent.id != layer.id && self.timeline_of(parent.id) == self.timeline_of(layer.id);
        together.then_some(parent)
    }

    /// Whether `ancestor` is somewhere up `layer`'s chain of parents.
    pub fn hangs_from(&self, layer: &Layer, ancestor: LayerId) -> bool {
        std::iter::successors(self.parent_of(layer), |l| self.parent_of(l))
            .take(MAX_DEPTH)
            .any(|l| l.id == ancestor)
    }

    /// Whether `parent` could be `child`'s parent: in the same timeline, and
    /// not the child itself or one of its descendants (which would make a
    /// loop).
    pub fn can_parent(&self, child: LayerId, parent: LayerId) -> bool {
        if child == parent || self.timeline_of(child) != self.timeline_of(parent) {
            return false;
        }
        // Folders only group layers in the timeline; they have no place on
        // the stage to hang from or to hang.
        if [child, parent].iter().any(|&id| self.layer(id).is_some_and(|l| l.folder || l.camera)) {
            return false;
        }
        let mut current = self.layer(parent);
        for _ in 0..MAX_DEPTH {
            match current {
                Some(layer) if layer.id == child => return false,
                Some(layer) => current = self.parent_of(layer),
                None => return true,
            }
        }
        false
    }

    /// Attaches a layer to `parent` (or detaches it, with `None`) without it
    /// jumping on `frame`.
    ///
    /// The layer's keys are in its parent's coordinates, so on their own
    /// they'd suddenly mean something else. Instead every key is changed by
    /// the same amount: undoing the old parent's placement on `frame` and
    /// applying the new one's inverse. On `frame` the layer stays exactly
    /// where it was; on other frames it moves with its new parent, which is
    /// the point. Rotation and uniform scale carry over exactly; a parent
    /// stretched unevenly or skewed is approximated, since each key keeps
    /// its own scale and skew.
    pub fn set_parent(&mut self, id: LayerId, parent: Option<LayerId>, frame: u32) -> bool {
        if let Some(parent) = parent
            && !self.can_parent(id, parent)
        {
            return false;
        }
        let Some(layer) = self.layer(id) else { return false };
        let old = self.parent_matrix(layer, frame);
        let mut probe = layer.clone();
        probe.parent = parent;
        let new = self.parent_matrix(&probe, frame);
        // Old parent space → timeline → new parent space.
        let Some(change) = new.inverse().map(|into_new| into_new.pre_concat(old)) else { return false };
        let (turn, stretch) = rotation_and_scale(change);

        let Some(layer) = self.layer_mut(id) else { return false };
        layer.parent = parent;
        let track = &mut layer.track;
        track.position.map_values(|p| change.apply(p));
        track.rotation.map_values(|r| r + turn);
        track.scale.map_values(|s| Point::new(s.x * stretch, s.y * stretch));
        true
    }

    /// Moves a layer's pivot to `pivot` (in its own coordinates) without
    /// moving the layer.
    ///
    /// The position is where the pivot sits, so each position key shifts
    /// by however far the pivot moved, as transformed by the pose at that
    /// key. Each key's own frame is used, so a rotated or scaled layer
    /// stays put on every key, not just the current frame.
    pub fn set_pivot(&mut self, id: LayerId, pivot: Point) {
        let Some(layer) = self.layer(id) else { return };
        let moved = pivot - layer.pivot;
        let shifts: Vec<(u32, Point)> = layer
            .track
            .position
            .keys()
            .iter()
            .map(|key| {
                let pose = layer.track.sample(key.frame as f32);
                let linear = Affine { e: 0.0, f: 0.0, ..pose.to_affine() };
                (key.frame, linear.apply(moved))
            })
            .collect();
        let Some(layer) = self.layer_mut(id) else { return };
        layer.pivot = pivot;
        for (frame, shift) in shifts {
            if let Some(key) = layer.track.position.key_at(frame) {
                let value = key.value + shift;
                layer.track.position.set(frame, value);
            }
        }
    }

    /// Before a layer goes away: its children move up to its own parent,
    /// staying where they are on `frame`.
    pub fn release_children(&mut self, id: LayerId, frame: u32) {
        let Some(layer) = self.layer(id) else { return };
        let grandparent = layer.parent;
        let children: Vec<LayerId> =
            self.all_layers().filter(|l| l.parent == Some(id)).map(|l| l.id).collect();
        for child in children {
            self.set_parent(child, grandparent, frame);
        }
    }
}

/// The rotation (degrees) and average scale in a matrix, assuming it's
/// close to a rotation plus an even scale.
fn rotation_and_scale(m: Affine) -> (f32, f32) {
    let turn = m.b.atan2(m.a).to_degrees();
    (turn, m.average_scale())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Shape, Style, Transform};

    fn square() -> Element {
        Element {
            shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }
    }

    /// An arm at (100, 100) and a hand at (200, 100), both unparented.
    fn arm_and_hand() -> (Document, LayerId, LayerId) {
        let mut doc = Document::default();
        let arm = doc.add_layer("Arm", vec![square()], 0, Transform::at(Point::new(100.0, 100.0)));
        let hand = doc.add_layer("Hand", vec![square()], 0, Transform::at(Point::new(200.0, 100.0)));
        (doc, arm, hand)
    }

    fn on_stage(doc: &Document, id: LayerId, frame: u32, local: Point) -> Point {
        doc.layer_matrix(doc.layer(id).unwrap(), frame).apply(local)
    }

    #[test]
    fn children_follow_their_parent() {
        let (mut doc, arm, hand) = arm_and_hand();
        assert!(doc.set_parent(hand, Some(arm), 0));
        // Attaching didn't move it.
        assert!(on_stage(&doc, hand, 0, Point::ZERO).distance(Point::new(200.0, 100.0)) < 1e-3);

        // Turn the arm a quarter turn clockwise on frame 10: the hand swings
        // round with it, from the right of the arm to below it.
        doc.layer_mut(arm).unwrap().track.rotation.set(10, 90.0);
        assert!(on_stage(&doc, hand, 10, Point::ZERO).distance(Point::new(100.0, 200.0)) < 1e-3);
    }

    #[test]
    fn parenting_refuses_loops() {
        let (mut doc, arm, hand) = arm_and_hand();
        doc.set_parent(hand, Some(arm), 0);
        assert!(!doc.can_parent(arm, hand), "the arm can't hang off its own hand");
        assert!(!doc.set_parent(arm, Some(arm), 0));
    }

    #[test]
    fn detaching_keeps_the_layer_where_it_is() {
        let (mut doc, arm, hand) = arm_and_hand();
        doc.set_parent(hand, Some(arm), 0);
        doc.layer_mut(arm).unwrap().track.rotation.set(0, 30.0);
        let before = on_stage(&doc, hand, 0, Point::new(5.0, 5.0));
        doc.set_parent(hand, None, 0);
        assert!(on_stage(&doc, hand, 0, Point::new(5.0, 5.0)).distance(before) < 1e-3);
    }

    #[test]
    fn moving_the_pivot_doesnt_move_the_layer_but_changes_what_it_turns_around() {
        let (mut doc, arm, _) = arm_and_hand();
        doc.layer_mut(arm).unwrap().track.rotation.set(0, 0.0);
        let corner = Point::new(10.0, 10.0);
        let before = on_stage(&doc, arm, 0, corner);
        // Pivot on the arm's left edge, like a shoulder.
        doc.set_pivot(arm, Point::new(-10.0, 0.0));
        assert!(on_stage(&doc, arm, 0, corner).distance(before) < 1e-3);
        // Rotating now swings it from the shoulder, which stays put.
        doc.layer_mut(arm).unwrap().track.rotation.set(10, 90.0);
        let shoulder = on_stage(&doc, arm, 10, Point::new(-10.0, 0.0));
        assert!(shoulder.distance(Point::new(90.0, 100.0)) < 1e-3, "{shoulder:?}");
    }

    #[test]
    fn deleting_a_parent_hands_its_children_to_their_grandparent() {
        let (mut doc, arm, hand) = arm_and_hand();
        let finger = doc.add_layer("Finger", vec![square()], 0, Transform::at(Point::new(230.0, 100.0)));
        doc.set_parent(hand, Some(arm), 0);
        doc.set_parent(finger, Some(hand), 0);
        let before = on_stage(&doc, finger, 0, Point::ZERO);
        doc.release_children(hand, 0);
        doc.remove_layer(hand);
        assert_eq!(doc.layer(finger).unwrap().parent, Some(arm));
        assert!(on_stage(&doc, finger, 0, Point::ZERO).distance(before) < 1e-3);
    }
}
