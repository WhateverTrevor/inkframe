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
use crate::document::{Element, LayerId};
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
    /// What turning it does besides bending: a smart bone, as Moho's,
    /// whose turn reshapes drawings or picks what a switch shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SmartAction>,
}

/// Something a smart bone does as it turns. Each is reached at `turn`
/// degrees (either way round, by its sign), from nothing at rest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SmartAction {
    /// Reshapes `layer`'s drawing toward its drawing `drawing` (by its
    /// place), all the way by `turn`: a corrected elbow, a smile.
    Morph { layer: LayerId, drawing: usize, turn: f32 },
    /// Has switch folder `folder` show `layer` once turned past `turn`.
    Switch { folder: LayerId, layer: LayerId, turn: f32 },
}

impl SmartAction {
    /// How far into the action a bone turned `now` degrees is: 0 at rest,
    /// 1 at the action's turn and beyond.
    pub fn amount(&self, now: f32) -> f32 {
        let turn = match *self {
            SmartAction::Morph { turn, .. } | SmartAction::Switch { turn, .. } => turn,
        };
        if turn.abs() < 1e-3 { 0.0 } else { (now / turn).clamp(0.0, 1.0) }
    }
}

/// Shapes, or points of a shape, bound to one bone, overriding how the
/// rest of the layer's drawing follows the bones. `element` is the shape's
/// place in the line art, or in the colour art; `anchors` are its points'
/// places in its outline, or `None` for all of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BonePoints {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub colour_art: bool,
    pub element: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchors: Option<Vec<usize>>,
    pub bone: usize,
}

/// Which bone one shape's points are bound to, if any.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ShapeBones {
    /// The layer's binding.
    #[default]
    Layer,
    /// The whole shape follows one bone.
    Whole(usize),
    /// Each point (by its place in the outline) to a bone, or the layer's.
    Points(Vec<Option<usize>>),
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
            actions: Vec::new(),
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

    /// Bends `elements` (whose points `to_rest` takes into the bones'
    /// coordinates at rest, and `from_rest` back) as they're bound, in
    /// `pose`: by `binding`, except for any shapes or points `pins` binds
    /// to a bone of their own (one entry per element; missing ones follow
    /// the layer). Pictures, paintings and text come back as they were:
    /// the renderer bends those through a mesh (see [`LayerBend`]).
    pub fn bend_elements(
        &self,
        pose: &Pose,
        binding: BoneBinding,
        elements: &[Element],
        pins: &[ShapeBones],
        to_rest: Affine,
        from_rest: Affine,
    ) -> Vec<Element> {
        if self.bones.is_empty() {
            return elements.to_vec();
        }
        let shortest = self.bones.iter().map(Bone::length).fold(f32::MAX, f32::min);
        // Short enough pieces to bend round a joint: a quarter of the
        // shortest bone, measured where the drawing is (the layer may be
        // scaled).
        let piece = (shortest / 4.0 / to_rest.average_scale().max(1e-3)).max(1.0);
        let follow = |bone: usize| pose.moves.get(bone).map(|m| from_rest.pre_concat(*m).pre_concat(to_rest));
        // Where a point goes: with its own bone, or else as the layer does.
        let bend = |bone: Option<usize>, p: Point| match bone.or(match binding {
            BoneBinding::Bone(b) => Some(b),
            _ => None,
        }) {
            Some(b) => follow(b).map_or(p, |m| m.apply(p)),
            None if binding == BoneBinding::Region => from_rest.apply(self.carry(pose, to_rest.apply(p))),
            None => p,
        };
        elements
            .iter()
            .enumerate()
            .map(|(i, element)| match pins.get(i).unwrap_or(&ShapeBones::Layer) {
                ShapeBones::Whole(bone) => match follow(*bone) {
                    Some(m) => element.transformed(m).unwrap_or_else(|| element.clone()),
                    None => element.clone(),
                },
                ShapeBones::Layer => match binding {
                    BoneBinding::Unbound => element.clone(),
                    BoneBinding::Bone(_) => {
                        let m = follow(match binding {
                            BoneBinding::Bone(b) => b,
                            _ => 0,
                        });
                        m.and_then(|m| element.transformed(m)).unwrap_or_else(|| element.clone())
                    }
                    BoneBinding::Region => bend_element(element, piece, &[], &bend),
                },
                ShapeBones::Points(bones) => bend_element(element, piece, bones, &bend),
            })
            .collect()
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
    /// Shapes or points bound to bones of their own, one entry per shape
    /// as shown (the colour art, then the line art).
    pub pins: Vec<ShapeBones>,
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
        self.skeleton.bend_elements(
            &self.pose,
            self.binding,
            elements,
            &self.pins,
            self.to_bones,
            self.from_bones,
        )
    }
}

/// Which bone each shape showing in `drawing` (the colour art, then the
/// line art) is bound to by its `bone_points`.
fn pins_of(drawing: &crate::Drawing) -> Vec<ShapeBones> {
    let colour = drawing.colour_art.len();
    let mut pins = vec![ShapeBones::Layer; colour + drawing.elements.len()];
    for bound in &drawing.bone_points {
        let (index, element) = if bound.colour_art {
            (bound.element, drawing.colour_art.get(bound.element))
        } else {
            (colour + bound.element, drawing.elements.get(bound.element))
        };
        let Some(element) = element else { continue };
        let pin = &mut pins[index];
        match &bound.anchors {
            None => *pin = ShapeBones::Whole(bound.bone),
            Some(anchors) => {
                let count = element.shape.to_anchors().0.len();
                if let ShapeBones::Whole(_) = pin {
                    continue;
                }
                if !matches!(pin, ShapeBones::Points(_)) {
                    *pin = ShapeBones::Points(vec![None; count]);
                }
                if let ShapeBones::Points(points) = pin {
                    for &a in anchors.iter().filter(|&&a| a < count) {
                        points[a] = Some(bound.bone);
                    }
                }
            }
        }
    }
    pins
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
        let owner = self.skeleton_layer_of(layer)?;
        let skeleton = owner.skeleton.as_ref().filter(|s| !s.bones.is_empty() && !s.is_at_rest(frame))?;
        let pins = layer.drawings.get(layer.drawing_index_at(frame)).map(pins_of).unwrap_or_default();
        let pinned = pins.iter().any(|p| *p != ShapeBones::Layer);
        if layer.bone_binding == BoneBinding::Unbound && !pinned {
            return None;
        }
        let from_space = self.bone_space(owner, frame).inverse()?;
        let to_bones = from_space.pre_concat(self.layer_matrix(layer, frame));
        let from_bones = to_bones.inverse()?;
        Some(LayerBend {
            skeleton,
            pose: skeleton.pose(frame),
            binding: layer.bone_binding,
            pins,
            to_bones,
            from_bones,
        })
    }

    /// What `layer` draws on `frame`: as its own deformers bend it (see
    /// [`crate::Layer::shown_elements`]), reshaped by any smart bone that
    /// morphs it, and then bent by the skeleton it's bound to. Pictures and
    /// paintings come back unbent: the renderer bends those through a mesh
    /// with [`Document::layer_bend`].
    pub fn posed_elements<'a>(&self, layer: &'a crate::Layer, frame: u32) -> std::borrow::Cow<'a, [Element]> {
        let mut shown = layer.shown_elements(frame);
        if let Some(skeleton) = self.skeleton_layer_of(layer).and_then(|o| o.skeleton.as_ref()) {
            let from = layer.drawings.get(layer.drawing_index_at(frame));
            for bone in &skeleton.bones {
                let now = bone.turn.sample(frame as f32).unwrap_or(0.0);
                for action in &bone.actions {
                    let SmartAction::Morph { layer: target, drawing, .. } = *action else { continue };
                    let amount = action.amount(now);
                    let (Some(from), Some(to)) = (from, layer.drawings.get(drawing)) else { continue };
                    if target == layer.id && amount > 0.0 {
                        let morphed =
                            crate::morph::morph(&shown, &to.shown(), (&from.hints, &to.hints), amount);
                        shown = std::borrow::Cow::Owned(morphed);
                    }
                }
            }
        }
        match self.layer_bend(layer, frame) {
            Some(bend) => std::borrow::Cow::Owned(bend.elements(&shown)),
            None => shown,
        }
    }

    /// Which layer a smart bone has switch `folder` show on `frame`, if
    /// one's turned far enough: the furthest-reaching that is.
    pub fn smart_switch(&self, folder: &crate::Layer, frame: u32) -> Option<LayerId> {
        let skeleton = self.skeleton_layer_of(folder).and_then(|o| o.skeleton.as_ref())?;
        let mut best: Option<(f32, LayerId)> = None;
        for bone in &skeleton.bones {
            let now = bone.turn.sample(frame as f32).unwrap_or(0.0);
            for action in &bone.actions {
                let SmartAction::Switch { folder: target, layer, turn } = *action else { continue };
                if target == folder.id
                    && action.amount(now) >= 1.0
                    && best.is_none_or(|(t, _)| turn.abs() > t)
                {
                    best = Some((turn.abs(), layer));
                }
            }
        }
        best.map(|(_, layer)| layer)
    }
}

/// Which part of a bone's rest pose is being moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestPart {
    Whole,
    End,
}

/// One shape bent point by point, each point by the bone `bones` binds it
/// to (by its place in the outline), or as the layer is bound where it
/// has none. A pencil line keeps its widths; anything else becomes a path,
/// cut into pieces first so it can bend round joints. A piece between two
/// points bound to the same bone follows that bone too.
fn bend_element(
    element: &Element,
    piece: f32,
    bones: &[Option<usize>],
    bend: &dyn Fn(Option<usize>, Point) -> Point,
) -> Element {
    if !element.is_reshapable() {
        return element.clone();
    }
    let bone_of = |i: usize| bones.get(i).copied().flatten();
    let bend_anchor = |a: Anchor, bone: Option<usize>| Anchor {
        point: bend(bone, a.point),
        handle_in: bend(bone, a.handle_in),
        handle_out: bend(bone, a.handle_out),
    };
    let shape = match &element.shape {
        // A pencil line is bent as it is, without cutting: its widths go
        // one per anchor, and its anchors are close together already. Its
        // points are its outline's, so it can only be bound whole.
        Shape::Line { path, widths } => {
            let anchors: Vec<Anchor> =
                anchors_from_path(path).into_iter().map(|a| bend_anchor(a, None)).collect();
            match path_from_anchors(&anchors, path.closed) {
                Some(path) => Shape::Line { path, widths: widths.clone() },
                None => return element.clone(),
            }
        }
        shape => {
            let (anchors, closed) = shape.to_anchors();
            let count = if closed { anchors.len() } else { anchors.len().saturating_sub(1) };
            let mut out: Vec<Anchor> = Vec::new();
            for i in 0..count {
                let next = (i + 1) % anchors.len();
                let pieces = subdivide(&[anchors[i], anchors[next]], false, piece);
                let last = pieces.len() - 1;
                let shared = bone_of(i).filter(|b| bone_of(next) == Some(*b));
                for (k, a) in pieces.into_iter().enumerate() {
                    let bone = match k {
                        0 => bone_of(i),
                        k if k == last => bone_of(next),
                        _ => shared,
                    };
                    let a = bend_anchor(a, bone);
                    match out.last_mut() {
                        // Where this segment starts is where the last ended.
                        Some(previous) if k == 0 => previous.handle_out = a.handle_out,
                        _ => out.push(a),
                    }
                }
            }
            if count == 0 {
                out = anchors.iter().enumerate().map(|(i, a)| bend_anchor(*a, bone_of(i))).collect();
            }
            if closed && out.len() > 1 {
                // Round the last segment and back to the first anchor.
                let seam = out.pop().expect("checked above");
                out[0].handle_in = seam.handle_in;
            }
            match path_from_anchors(&out, closed) {
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
        let bent = skeleton.bend_elements(&pose, BoneBinding::Bone(1), &[hand], &[], to_rest, from_rest);
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

    /// A document with the arm as a skeleton on a folder, and a 200×10
    /// bar along it (0 to 200, centred on y 0) in the folder, by region.
    fn rigged_bar() -> (crate::Document, LayerId, LayerId) {
        use crate::{Color, Document, Style, TimelineId, Transform};
        let mut doc = Document::default();
        let bar = Element {
            shape: Shape::Rect { width: 200.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let layer = doc.add_layer("Bar", vec![bar], 0, Transform::at(Point::new(100.0, 0.0)));
        let folder = doc.add_folder(TimelineId::Main, &[layer]).unwrap();
        doc.layer_mut(folder).unwrap().skeleton = Some(arm());
        (doc, folder, layer)
    }

    fn outline_of(doc: &crate::Document, layer: LayerId, frame: u32) -> Vec<Point> {
        let layer = doc.layer(layer).unwrap();
        match &doc.posed_elements(layer, frame)[0].shape {
            Shape::Path(path) => path.flatten(),
            other => panic!("bent into a path, not {other:?}"),
        }
    }

    #[test]
    fn points_pinned_to_a_bone_follow_only_it() {
        let (mut doc, folder, layer) = rigged_bar();
        doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap().bones[1].turn.set(0, 90.0);
        // The bar's right end (its outline's points 1 and 2, at x 100 in
        // its own coordinates) pinned to the upper arm: it stays put.
        doc.layer_mut(layer).unwrap().drawings[0].bone_points.push(BonePoints {
            colour_art: false,
            element: 0,
            anchors: Some(vec![1, 2]),
            bone: 0,
        });
        let points = outline_of(&doc, layer, 0);
        assert!(points.iter().any(|p| p.distance(Point::new(100.0, -5.0)) < 1e-2), "the pinned end stays");
    }

    #[test]
    fn a_shape_pinned_whole_moves_rigidly() {
        let (mut doc, folder, layer) = rigged_bar();
        doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap().bones[0].turn.set(0, 90.0);
        doc.layer_mut(layer).unwrap().drawings[0].bone_points.push(BonePoints {
            colour_art: false,
            element: 0,
            anchors: None,
            bone: 0,
        });
        // Turned about the shoulder at (0, 0): the bar hangs straight down,
        // unbent (in its own coordinates, 100 to the right of the bones').
        let layer = doc.layer(layer).unwrap();
        let bounds = doc.posed_elements(layer, 0)[0].shape.bounds();
        assert!((bounds.width() - 10.0).abs() < 1e-2 && (bounds.height() - 200.0).abs() < 1e-2, "{bounds:?}");
    }

    #[test]
    fn a_smart_bone_morphs_a_drawing_as_it_turns() {
        let (mut doc, folder, layer) = rigged_bar();
        // A second drawing: the bar twice as tall.
        let mut tall = doc.layer(layer).unwrap().drawings[0].clone();
        tall.elements[0].shape = Shape::Rect { width: 200.0, height: 30.0, corner_radius: 0.0 };
        let drawing = doc.layer_mut(layer).unwrap().add_drawing(tall);
        let skeleton = doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap();
        // Bind the bar to nothing, so only the morph shows.
        skeleton.bones[1].actions.push(SmartAction::Morph { layer, drawing, turn: 40.0 });
        skeleton.bones[1].turn.set(10, 20.0);
        doc.layer_mut(layer).unwrap().bone_binding = BoneBinding::Unbound;
        let height = |doc: &crate::Document, frame| {
            doc.posed_elements(doc.layer(layer).unwrap(), frame)[0].shape.bounds().height()
        };
        assert!((height(&doc, 0) - 10.0).abs() < 1e-2);
        // Turned 20° of the action's 40°: halfway to the tall bar.
        assert!((height(&doc, 10) - 20.0).abs() < 0.5, "{}", height(&doc, 10));
    }

    #[test]
    fn a_smart_bone_switches_once_turned_far_enough() {
        use crate::{TimelineId, Transform};
        let (mut doc, folder, _) = rigged_bar();
        let open = doc.add_layer("Open", Vec::new(), 0, Transform::default());
        let fist = doc.add_layer("Fist", Vec::new(), 0, Transform::default());
        let hand = doc.add_folder(TimelineId::Main, &[open, fist]).unwrap();
        doc.set_folder(hand, Some(folder));
        doc.layer_mut(hand).unwrap().switch = true;
        let skeleton = doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap();
        skeleton.bones[1].actions.push(SmartAction::Switch { folder: hand, layer: open, turn: -30.0 });
        skeleton.bones[1].turn.set(0, 0.0);
        skeleton.bones[1].turn.set(10, -45.0);
        let showing = |doc: &crate::Document, frame| doc.switch_choice(doc.layer(hand).unwrap(), frame);
        // At rest the switch's own choice (its top layer, the fist); turned
        // the action's way far enough, the open hand.
        assert_eq!(showing(&doc, 0), Some(fist));
        assert_eq!(showing(&doc, 10), Some(open));
        assert!(!doc.shows(doc.layer(fist).unwrap(), 10));
    }
}
