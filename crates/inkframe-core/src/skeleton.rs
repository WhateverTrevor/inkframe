//! Skeletons, as in Moho: a tree of bones over a whole character, which
//! the character's drawings bind to and bend with.
//!
//! A skeleton belongs to a folder layer (see `folder.rs`), and the layers
//! in the folder are the character: a head, a body, arms painted with the
//! brush or drawn as shapes. Each of them binds to the bones in one of
//! three ways (see [`BoneBinding`]):
//!
//! * **By region**, the default: every point of the drawing follows the
//!   bones near it. Close to one bone it moves rigidly with that bone;
//!   between two (round an elbow) it blends them, which rounds the bend.
//!   How far a bone reaches is its `strength`.
//! * **To one bone**: the whole layer moves with that bone, unbent, as a
//!   hand or a hat should.
//! * **Not at all**: it stays where it's drawn.
//!
//! Unlike the deformer in `deform.rs`, which is one chain of bones inside
//! one drawing, a skeleton is a tree (a body with two arms and two legs
//! hanging from it) shared by every layer in the folder, so the arm drawn
//! on one layer and the sleeve painted on another bend together.
//!
//! Bones live in the coordinates of the timeline they're in, as the stage
//! shows it, so they line up with the art however its layers are placed.
//! Each has a rest pose (where it was drawn) and, animated, how far it's
//! turned from there and how far moved. A drawing is bound in the rest
//! pose: a point's weights come from where it is among the bones at rest,
//! and then it follows those bones as they're posed. That's linear blend
//! skinning, as 3D characters use.

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, anchors_from_path, path_from_anchors};
use crate::channel::{AnyChannel, Channel};
use crate::deform::subdivide;
use crate::document::Element;
use crate::geometry::{Affine, Point};
use crate::shape::Shape;

/// The deepest a bone may hang below the root. Deeper bones (only in a
/// broken file) are treated as roots.
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Skeleton {
    /// The bones, each after the bone it hangs from.
    pub bones: Vec<Bone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    /// The bone it hangs from (by its place in the list, always earlier),
    /// or `None` for a root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// Where it starts and ends at rest, in the timeline's coordinates.
    pub start: Point,
    pub end: Point,
    /// How far it reaches into a drawing bound by region: points about this
    /// far from it follow it about as much as the next bone along.
    pub strength: f32,
    /// How far it's turned from rest, in degrees, relative to the bone it
    /// hangs from (turning a shoulder carries the arm). Animated.
    pub turn: Channel<f32>,
    /// How far it's moved from rest, animated. Moving the root moves the
    /// whole character.
    pub moved: Channel<Point>,
}

/// How a layer in a skeleton's folder follows the bones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoneBinding {
    /// Each point follows the bones near it.
    #[default]
    Region,
    /// The whole layer moves with one bone (by its place in the skeleton).
    Bone(usize),
    /// The layer stays put.
    Unbound,
}

impl BoneBinding {
    pub fn is_region(&self) -> bool {
        *self == BoneBinding::Region
    }
}

/// Every bone's place on a frame: the matrix taking a point from where it
/// is at rest to where the bone has carried it.
#[derive(Debug, Clone, PartialEq)]
pub struct Pose {
    pub moves: Vec<Affine>,
}

impl Bone {
    /// A bone from `start` to `end` at rest, hanging from `parent`, reaching
    /// as far as half its length, keyed at rest on `frame`.
    pub fn new(name: impl Into<String>, parent: Option<usize>, start: Point, end: Point, frame: u32) -> Self {
        Self {
            name: name.into(),
            parent,
            start,
            end,
            strength: (start.distance(end) * 0.5).max(10.0),
            turn: Channel::new(frame, 0.0),
            moved: Channel::new(frame, Point::ZERO),
        }
    }

    pub fn length(&self) -> f32 {
        self.start.distance(self.end)
    }

    /// The bone's direction at rest, in radians.
    fn rest_angle(&self) -> f32 {
        let along = self.end - self.start;
        along.y.atan2(along.x)
    }

    /// From the bone's own frame (its start at the origin, pointing along
    /// x) to the timeline at rest.
    fn rest_frame(&self) -> Affine {
        Affine::translate(self.start).pre_concat(Affine::rotate(self.rest_angle()))
    }
}

impl Skeleton {
    /// The bone `index` hangs from, if it's a real one earlier in the list.
    fn parent_of(&self, index: usize) -> Option<usize> {
        self.bones.get(index)?.parent.filter(|&p| p < index)
    }

    /// `index` and the bones it hangs from, up to its root.
    pub fn chain(&self, index: usize) -> Vec<usize> {
        let mut chain = vec![index];
        while let Some(parent) = chain.last().and_then(|&b| self.parent_of(b)) {
            if chain.len() > MAX_DEPTH {
                break;
            }
            chain.push(parent);
        }
        chain
    }

    /// Where every bone is on `frame`.
    pub fn pose(&self, frame: u32) -> Pose {
        self.pose_with(|_, bone| {
            (
                bone.turn.sample(frame as f32).unwrap_or(0.0),
                bone.moved.sample(frame as f32).unwrap_or(Point::ZERO),
            )
        })
    }

    /// Where every bone is with the turn and move `posed` gives each (by
    /// its place).
    fn pose_with(&self, posed: impl Fn(usize, &Bone) -> (f32, Point)) -> Pose {
        let mut moves: Vec<Affine> = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let carried = self.parent_of(index).map_or(Affine::IDENTITY, |p| moves[p]);
            let (turn, moved) = posed(index, bone);
            // Posed: carried by the bone it hangs from, then moved and
            // turned about its own start.
            let frame = carried.pre_concat(
                Affine::translate(bone.start + moved)
                    .pre_concat(Affine::rotate(bone.rest_angle() + turn.to_radians())),
            );
            let undo_rest = bone.rest_frame().inverse().unwrap_or(Affine::IDENTITY);
            moves.push(frame.pre_concat(undo_rest));
        }
        Pose { moves }
    }

    /// Where bone `index` starts and ends on `frame`.
    pub fn posed_ends(&self, index: usize, frame: u32) -> Option<(Point, Point)> {
        let bone = self.bones.get(index)?;
        let m = *self.pose(frame).moves.get(index)?;
        Some((m.apply(bone.start), m.apply(bone.end)))
    }

    /// Whether no bone is turned or moved on `frame`.
    pub fn is_at_rest(&self, frame: u32) -> bool {
        self.bones.iter().all(|b| {
            b.turn.sample(frame as f32).unwrap_or(0.0).abs() < 1e-4
                && b.moved.sample(frame as f32).unwrap_or(Point::ZERO).length() < 1e-4
        })
    }

    /// How much each bone pulls on the point `p` (where it is at rest), as
    /// fractions that add up to 1. Near a bone it's nearly all that bone;
    /// halfway between two, half each.
    pub fn weights(&self, p: Point) -> Vec<f32> {
        let raw: Vec<f32> = self
            .bones
            .iter()
            .map(|b| {
                let reach = (p.distance_to_segment(b.start, b.end) / b.strength.max(1e-3)).powi(4);
                1.0 / (1e-6 + reach)
            })
            .collect();
        let sum: f32 = raw.iter().sum();
        if sum <= 0.0 || !sum.is_finite() {
            return vec![1.0 / self.bones.len().max(1) as f32; self.bones.len()];
        }
        raw.into_iter().map(|w| w / sum).collect()
    }

    /// Where the point `p` (at rest, in the timeline's coordinates) goes
    /// in `pose`, bound by region.
    pub fn carry(&self, pose: &Pose, p: Point) -> Point {
        let mut to = Point::ZERO;
        for (w, m) in self.weights(p).into_iter().zip(&pose.moves) {
            if w > 1e-5 {
                to = to + m.apply(p) * w;
            }
        }
        to
    }

    /// Bends `elements` (whose points `to_rest` takes into the timeline at
    /// rest, and `from_rest` back) as they're bound, in `pose`. Pictures,
    /// paintings and text come back as they were: the renderer bends those
    /// through a mesh (see [`Skeleton::carry`]).
    pub fn bend_elements(
        &self,
        pose: &Pose,
        binding: BoneBinding,
        elements: &[Element],
        to_rest: Affine,
        from_rest: Affine,
    ) -> Vec<Element> {
        match binding {
            BoneBinding::Unbound => elements.to_vec(),
            BoneBinding::Bone(bone) => {
                let Some(m) = pose.moves.get(bone) else { return elements.to_vec() };
                let m = from_rest.pre_concat(*m).pre_concat(to_rest);
                elements.iter().map(|e| e.transformed(m).unwrap_or_else(|| e.clone())).collect()
            }
            BoneBinding::Region => {
                if self.bones.is_empty() {
                    return elements.to_vec();
                }
                let shortest = self.bones.iter().map(Bone::length).fold(f32::MAX, f32::min);
                // Short enough pieces to bend round a joint: a quarter of the
                // shortest bone, measured where the drawing is (the layer may
                // be scaled).
                let piece = (shortest / 4.0 / to_rest.average_scale().max(1e-3)).max(1.0);
                let bend = |p: Point| from_rest.apply(self.carry(pose, to_rest.apply(p)));
                elements.iter().map(|e| bend_element(e, piece, &bend)).collect()
            }
        }
    }

    /// Turns bone `index` and those it hangs from so its end reaches for
    /// `target` on `frame`, keying the turns: inverse kinematics, as
    /// dragging a bone does in Moho. With `alone`, only that bone turns.
    /// Roots turn too; to move the character, move a root (see
    /// [`Skeleton::move_bone`]).
    pub fn reach(&mut self, index: usize, target: Point, frame: u32, alone: bool) {
        if index >= self.bones.len() {
            return;
        }
        let chain = if alone { vec![index] } else { self.chain(index) };
        let mut turns: Vec<f32> =
            self.bones.iter().map(|b| b.turn.sample(frame as f32).unwrap_or(0.0)).collect();
        let moved: Vec<Point> =
            self.bones.iter().map(|b| b.moved.sample(frame as f32).unwrap_or(Point::ZERO)).collect();
        // Cyclic coordinate descent: from the bone itself up to the root,
        // turn each joint so the end points at the target; a few rounds
        // settle it (see `ik.rs`).
        for _ in 0..12 {
            for &joint in &chain {
                let pose = self.pose_with(|i, _| (turns[i], moved[i]));
                let m = pose.moves[joint];
                let pivot = m.apply(self.bones[joint].start);
                let tip = pose.moves[index].apply(self.bones[index].end);
                let (now, want) = (tip - pivot, target - pivot);
                if now.length() < 1e-4 || want.length() < 1e-4 {
                    continue;
                }
                let change = (want.y.atan2(want.x) - now.y.atan2(now.x)).to_degrees();
                turns[joint] += (change + 180.0).rem_euclid(360.0) - 180.0;
            }
        }
        for &joint in &chain {
            self.bones[joint].turn.set(frame, turns[joint]);
        }
    }

    /// Moves bone `index` so it starts at `to` on `frame`, keying it. It
    /// carries whatever hangs from it.
    pub fn move_bone(&mut self, index: usize, to: Point, frame: u32) {
        let pose = self.pose(frame);
        let Some(bone) = self.bones.get(index) else { return };
        let now = pose.moves[index].apply(bone.start);
        let moved = bone.moved.sample(frame as f32).unwrap_or(Point::ZERO);
        // The move is made before the bone it hangs from carries it, so
        // undo that carrying to find how far to move at rest.
        let carried = self.parent_of(index).map_or(Affine::IDENTITY, |p| pose.moves[p]);
        let delta = match carried.inverse() {
            Some(back) => back.apply(to) - back.apply(now),
            None => to - now,
        };
        self.bones[index].moved.set(frame, moved + delta);
    }

    /// Adds a bone, hanging from `parent`, and returns its place.
    pub fn add_bone(&mut self, parent: Option<usize>, start: Point, end: Point, frame: u32) -> usize {
        let parent = parent.filter(|&p| p < self.bones.len());
        let name = format!("Bone {}", self.bones.len() + 1);
        self.bones.push(Bone::new(name, parent, start, end, frame));
        self.bones.len() - 1
    }

    /// Takes bone `index` out, with everything hanging from it. Returns the
    /// old places' new ones (`None` for those removed), so bindings to
    /// bones can follow.
    pub fn remove_bone(&mut self, index: usize) -> Vec<Option<usize>> {
        let gone: Vec<bool> = (0..self.bones.len()).map(|b| self.chain(b).contains(&index)).collect();
        let mut map = Vec::with_capacity(self.bones.len());
        let mut next = 0;
        for &g in &gone {
            map.push((!g).then(|| {
                next += 1;
                next - 1
            }));
        }
        let bones = std::mem::take(&mut self.bones);
        self.bones = bones
            .into_iter()
            .zip(&gone)
            .filter(|(_, g)| !**g)
            .map(|(mut bone, _)| {
                bone.parent = bone.parent.and_then(|p| map.get(p).copied().flatten());
                bone
            })
            .collect();
        map
    }

    /// Moves bone `index`'s rest pose by `delta`, as rigging rather than
    /// animating does: the whole bone, with everything hanging from it, or
    /// just its end, with the bones that start there (and theirs).
    pub fn shift_rest(&mut self, index: usize, delta: Point, part: RestPart) {
        let Some(old_end) = self.bones.get(index).map(|b| b.end) else { return };
        // Each bone below this one, and whether it moves: all of them for
        // the whole bone; for the end, those under a child joined there.
        let moving: Vec<usize> = (0..self.bones.len())
            .filter(|&b| {
                let chain = self.chain(b);
                let Some(at) = chain.iter().position(|&c| c == index).filter(|&at| at > 0) else {
                    return false;
                };
                part == RestPart::Whole || self.bones[chain[at - 1]].start.distance(old_end) < 1e-3
            })
            .collect();
        let bone = &mut self.bones[index];
        if part == RestPart::Whole {
            bone.start = bone.start + delta;
        }
        bone.end = bone.end + delta;
        for b in moving {
            let child = &mut self.bones[b];
            child.start = child.start + delta;
            child.end = child.end + delta;
        }
    }

    // ------------------------------------------------------------- keys

    pub fn channels_mut(&mut self) -> impl Iterator<Item = &mut dyn AnyChannel> {
        self.bones
            .iter_mut()
            .flat_map(|b| [&mut b.turn as &mut dyn AnyChannel, &mut b.moved as &mut dyn AnyChannel])
    }

    pub fn key_frames(&self) -> Vec<u32> {
        let mut frames: Vec<u32> = self
            .bones
            .iter()
            .flat_map(|b| b.turn.key_frames().into_iter().chain(b.moved.key_frames()))
            .collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    pub fn insert_key(&mut self, frame: u32) -> bool {
        self.channels_mut().fold(false, |added, c| c.insert_key(frame) | added)
    }

    pub fn remove_key(&mut self, frame: u32) -> bool {
        self.channels_mut().fold(false, |removed, c| c.remove_key(frame) | removed)
    }

    pub fn move_key(&mut self, from: u32, to: u32) {
        for channel in self.channels_mut() {
            channel.move_key(from, to);
        }
    }
}

/// How one layer is bent by the skeleton over it on one frame: the bones'
/// pose, how the layer is bound, and the way between the layer's own
/// coordinates and the bones'.
pub struct LayerBend<'a> {
    pub skeleton: &'a Skeleton,
    pub pose: Pose,
    pub binding: BoneBinding,
    /// From the layer's coordinates to the bones', and back.
    pub to_bones: Affine,
    pub from_bones: Affine,
}

impl LayerBend<'_> {
    /// Where the layer's point `p` goes, in the layer's coordinates.
    pub fn bend(&self, p: Point) -> Point {
        match self.binding {
            BoneBinding::Unbound => p,
            BoneBinding::Bone(_) => self.rigid().map_or(p, |m| m.apply(p)),
            BoneBinding::Region => {
                self.from_bones.apply(self.skeleton.carry(&self.pose, self.to_bones.apply(p)))
            }
        }
    }

    /// For a layer bound to one bone, the one matrix that moves all of it
    /// (in its own coordinates).
    pub fn rigid(&self) -> Option<Affine> {
        let BoneBinding::Bone(bone) = self.binding else { return None };
        let m = self.pose.moves.get(bone)?;
        Some(self.from_bones.pre_concat(*m).pre_concat(self.to_bones))
    }

    /// The layer's shapes, bent.
    pub fn elements(&self, elements: &[Element]) -> Vec<Element> {
        self.skeleton.bend_elements(&self.pose, self.binding, elements, self.to_bones, self.from_bones)
    }
}

impl crate::Document {
    /// The folder layer whose skeleton `layer` is bound to: the nearest
    /// folder it's in that has one.
    pub fn skeleton_layer_of(&self, layer: &crate::Layer) -> Option<&crate::Layer> {
        let mut current = layer;
        for _ in 0..MAX_DEPTH {
            let folder = self.layer(current.in_folder?)?;
            if folder.skeleton.is_some() {
                return Some(folder);
            }
            current = folder;
        }
        None
    }

    /// From the bones' coordinates to the timeline's as shown: the
    /// skeleton folder's parents and the camera, but not the folder's own
    /// transform (move a root bone to move the character).
    pub fn bone_space(&self, skeleton_layer: &crate::Layer, frame: u32) -> Affine {
        self.parent_matrix(skeleton_layer, frame)
    }

    /// How the skeleton over `layer` bends it on `frame`, if it's bound to
    /// one that's posed away from rest.
    pub fn layer_bend(&self, layer: &crate::Layer, frame: u32) -> Option<LayerBend<'_>> {
        if layer.bone_binding == BoneBinding::Unbound {
            return None;
        }
        let owner = self.skeleton_layer_of(layer)?;
        let skeleton = owner.skeleton.as_ref().filter(|s| !s.bones.is_empty() && !s.is_at_rest(frame))?;
        let from_space = self.bone_space(owner, frame).inverse()?;
        let to_bones = from_space.pre_concat(self.layer_matrix(layer, frame));
        let from_bones = to_bones.inverse()?;
        Some(LayerBend {
            skeleton,
            pose: skeleton.pose(frame),
            binding: layer.bone_binding,
            to_bones,
            from_bones,
        })
    }

    /// What `layer` draws on `frame`, bent by its own deformers (see
    /// [`crate::Layer::shown_elements`]) and then by the skeleton it's
    /// bound to. Pictures and paintings come back unbent: the renderer
    /// bends those through a mesh with [`Document::layer_bend`].
    pub fn posed_elements<'a>(&self, layer: &'a crate::Layer, frame: u32) -> std::borrow::Cow<'a, [Element]> {
        let shown = layer.shown_elements(frame);
        match self.layer_bend(layer, frame) {
            Some(bend) => std::borrow::Cow::Owned(bend.elements(&shown)),
            None => shown,
        }
    }
}

/// Which part of a bone's rest pose is being moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestPart {
    Whole,
    End,
}

/// One shape bent point by point. A pencil line keeps its widths (cut
/// along with it); anything else becomes a path.
fn bend_element(element: &Element, piece: f32, bend: &dyn Fn(Point) -> Point) -> Element {
    if !element.is_reshapable() {
        return element.clone();
    }
    let bend_anchor = |a: Anchor| Anchor {
        point: bend(a.point),
        handle_in: bend(a.handle_in),
        handle_out: bend(a.handle_out),
    };
    let shape = match &element.shape {
        // A pencil line is bent as it is, without cutting: its widths go
        // one per anchor, and its anchors are close together already.
        Shape::Line { path, widths } => {
            let anchors: Vec<Anchor> = anchors_from_path(path).into_iter().map(bend_anchor).collect();
            match path_from_anchors(&anchors, path.closed) {
                Some(path) => Shape::Line { path, widths: widths.clone() },
                None => return element.clone(),
            }
        }
        shape => {
            let (anchors, closed) = shape.to_anchors();
            let anchors: Vec<Anchor> =
                subdivide(&anchors, closed, piece).into_iter().map(bend_anchor).collect();
            match path_from_anchors(&anchors, closed) {
                Some(path) => Shape::Path(path),
                None => return element.clone(),
            }
        }
    };
    Element { shape, style: element.style.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Style};

    /// An arm along the x axis: an upper arm from 0 to 100 and a forearm
    /// from 100 to 200 hanging from it.
    fn arm() -> Skeleton {
        let mut skeleton = Skeleton::default();
        let upper = skeleton.add_bone(None, Point::new(0.0, 0.0), Point::new(100.0, 0.0), 0);
        skeleton.add_bone(Some(upper), Point::new(100.0, 0.0), Point::new(200.0, 0.0), 0);
        skeleton
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-2
    }

    #[test]
    fn at_rest_nothing_moves() {
        let skeleton = arm();
        let pose = skeleton.pose(0);
        assert!(skeleton.is_at_rest(0));
        for p in [Point::new(30.0, 10.0), Point::new(150.0, -20.0), Point::new(500.0, 500.0)] {
            assert!(close(skeleton.carry(&pose, p), p), "{p:?}");
        }
    }

    #[test]
    fn turning_the_upper_arm_carries_the_forearm() {
        let mut skeleton = arm();
        skeleton.bones[0].turn.set(0, 90.0);
        let (start, end) = skeleton.posed_ends(1, 0).unwrap();
        // Down the screen: y grows downwards, and positive turns go clockwise.
        assert!(close(start, Point::new(0.0, 100.0)), "{start:?}");
        assert!(close(end, Point::new(0.0, 200.0)), "{end:?}");
    }

    #[test]
    fn points_follow_the_bone_theyre_nearest() {
        let mut skeleton = arm();
        skeleton.bones[1].turn.set(0, 90.0);
        let pose = skeleton.pose(0);
        // On the forearm, near its end: swung down with it.
        let hand = skeleton.carry(&pose, Point::new(190.0, 0.0));
        assert!(close(hand, Point::new(100.0, 90.0)), "{hand:?}");
        // On the upper arm, near the shoulder: where it was.
        let shoulder = skeleton.carry(&pose, Point::new(10.0, 0.0));
        assert!(close(shoulder, Point::new(10.0, 0.0)), "{shoulder:?}");
        // Right at the elbow, a blend: the bend is rounded.
        let elbow = skeleton.carry(&pose, Point::new(100.0, -20.0));
        assert!(elbow.x > 100.0 && elbow.y > -20.0, "{elbow:?}");
    }

    #[test]
    fn reaching_bends_the_chain_toward_the_target() {
        let mut skeleton = arm();
        let target = Point::new(100.0, 100.0);
        skeleton.reach(1, target, 0, false);
        let (_, end) = skeleton.posed_ends(1, 0).unwrap();
        assert!(end.distance(target) < 0.5, "{end:?}");
        // Only the forearm, when asked: the upper arm stays.
        let mut alone = arm();
        alone.reach(1, Point::new(100.0, 100.0), 0, true);
        assert_eq!(alone.bones[0].turn.sample(0.0), Some(0.0));
        let (_, end) = alone.posed_ends(1, 0).unwrap();
        assert!(close(end, Point::new(100.0, 100.0)), "{end:?}");
    }

    #[test]
    fn moving_the_root_moves_everything() {
        let mut skeleton = arm();
        skeleton.bones[0].turn.set(0, 90.0);
        skeleton.move_bone(0, Point::new(50.0, 20.0), 0);
        let (start, _) = skeleton.posed_ends(0, 0).unwrap();
        assert!(close(start, Point::new(50.0, 20.0)), "{start:?}");
        let (_, end) = skeleton.posed_ends(1, 0).unwrap();
        assert!(close(end, Point::new(50.0, 220.0)), "{end:?}");
        // And a forearm moved on its own pulls away from the elbow.
        skeleton.move_bone(1, Point::new(60.0, 120.0), 0);
        let (start, _) = skeleton.posed_ends(1, 0).unwrap();
        assert!(close(start, Point::new(60.0, 120.0)), "{start:?}");
    }

    #[test]
    fn a_layer_bound_to_one_bone_moves_rigidly_with_it() {
        let mut skeleton = arm();
        skeleton.bones[1].turn.set(0, 90.0);
        let pose = skeleton.pose(0);
        let hand = Element {
            shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        // The layer's own coordinates are 200 to the right of the timeline's.
        let to_rest = Affine::translate(Point::new(200.0, 0.0));
        let from_rest = to_rest.inverse().unwrap();
        let bent = skeleton.bend_elements(&pose, BoneBinding::Bone(1), &[hand], to_rest, from_rest);
        let bounds = bent[0].shape.bounds();
        // The hand at the forearm's tip (200, 0) swings down to (100, 100).
        assert!(close(bounds.center(), Point::new(-100.0, 100.0)), "{bounds:?}");
        assert!((bounds.width() - 20.0).abs() < 1e-2);
    }

    #[test]
    fn removing_a_bone_takes_what_hangs_from_it() {
        let mut skeleton = arm();
        skeleton.add_bone(None, Point::new(0.0, 50.0), Point::new(0.0, 150.0), 0);
        let map = skeleton.remove_bone(0);
        assert_eq!(map, [None, None, Some(0)]);
        assert_eq!(skeleton.bones.len(), 1);
        assert_eq!(skeleton.bones[0].parent, None);
    }

    #[test]
    fn moving_a_bones_end_at_rest_takes_its_children_along() {
        let mut skeleton = arm();
        skeleton.shift_rest(0, Point::new(0.0, 30.0), RestPart::End);
        assert!(close(skeleton.bones[0].end, Point::new(100.0, 30.0)));
        assert!(close(skeleton.bones[1].start, Point::new(100.0, 30.0)));
        assert!(close(skeleton.bones[1].end, Point::new(200.0, 30.0)));
    }
}
