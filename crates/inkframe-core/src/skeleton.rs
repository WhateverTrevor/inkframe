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
    /// How far it may turn, in degrees from rest. `None` is free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<crate::RotationLimits>,
    /// How long it is against its rest length (1 is as drawn), animated:
    /// squash and stretch. `None` never stretches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stretch: Option<Channel<f32>>,
    /// Stretching makes it thinner as it gets longer, and fatter as it gets
    /// shorter, so what it bends keeps its size, as a squashed ball does.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub squash: bool,
    /// The bone swings after the bones it hangs from, overshoots and settles,
    /// as hair, a tail or an antenna does. `None` follows exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamics: Option<Dynamics>,
    /// A bone this one reaches for, as Moho's target bones: its end is
    /// kept on the target's start, the bones above it (up to two) bending
    /// to get it there. A foot stays planted on a target bone while the
    /// body moves; move the target to step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<usize>,
    /// A turn to show it at instead of its animation's, while its smart
    /// actions are scrubbed through on their own timeline, as in Moho.
    /// Never saved, and never counted as a change.
    #[serde(skip)]
    pub preview: TurnPreview,
}

/// A turn a bone is shown at for now, in place of its animation's (see
/// [`Bone::preview`]). Two bones are the same whatever this says, as with
/// [`RestPoseView`], so scrubbing a smart bone's actions never counts as a
/// change to undo or save.
///
/// A copy of a document doesn't keep it: only the one being worked on shows
/// a preview, so undo steps, saves and exports (all made from copies)
/// never do.
#[derive(Debug, Default)]
pub struct TurnPreview(pub Option<f32>);

impl Clone for TurnPreview {
    fn clone(&self) -> Self {
        Self(None)
    }
}

impl PartialEq for TurnPreview {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// How a bone with dynamics swings after the bones it hangs from (see
/// [`Skeleton::pose`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dynamics {
    /// How hard it's pulled back to where it's posed, each frame, from 0
    /// (not at all: it trails behind forever) to 1 (straight away).
    pub spring: f32,
    /// How much of its swing it loses each frame, from 0 (it swings on and
    /// on) to 1 (it doesn't swing at all).
    pub damping: f32,
    /// The furthest it may swing from where it's posed, in degrees.
    pub limit: f32,
}

impl Default for Dynamics {
    fn default() -> Self {
        Self { spring: 0.2, damping: 0.15, limit: 60.0 }
    }
}

/// Something a smart bone does as it turns. A morph or a switch is reached
/// at `turn` degrees (either way round, by its sign), from nothing at rest;
/// a correction has keys at as many turns as it needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SmartAction {
    /// Reshapes `layer`'s drawing toward its drawing `drawing` (by its
    /// place), all the way by `turn`: a corrected elbow, a smile.
    Morph { layer: LayerId, drawing: usize, turn: f32 },
    /// Has switch folder `folder` show `layer` once turned past `turn`.
    Switch { folder: LayerId, layer: LayerId, turn: f32 },
    /// Moho's action: `layer`'s drawing `drawing` reshaped by hand at
    /// several turns of the bone (a key each), and morphed between them,
    /// and from the drawing as drawn at rest. While the drawing's showing,
    /// the bone's turn picks how it's reshaped.
    Correct { layer: LayerId, drawing: usize, keys: Vec<CorrectionKey> },
}

/// A drawing as it's reshaped at one turn of a smart bone: its colour art
/// and its line art, shape for shape as at rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectionKey {
    pub turn: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colour_art: Vec<Element>,
    pub line_art: Vec<Element>,
}

impl SmartAction {
    /// How far into a morph or switch a bone turned `now` degrees is: 0 at
    /// rest, 1 at the action's turn and beyond. A correction is keyed
    /// instead (see [`corrected`]).
    pub fn amount(&self, now: f32) -> f32 {
        let turn = match self {
            SmartAction::Morph { turn, .. } | SmartAction::Switch { turn, .. } => *turn,
            SmartAction::Correct { .. } => return 0.0,
        };
        if turn.abs() < 1e-3 { 0.0 } else { (now / turn).clamp(0.0, 1.0) }
    }
}

/// `rest` (a drawing's colour art and line art as drawn) reshaped by
/// correction `keys` for a bone turned `now` degrees: between the two keys
/// either side of `now` (the drawing at rest counting as a key at 0),
/// morphed that far from one to the other; past the last key, as it is.
pub fn corrected(
    rest: (&[Element], &[Element]),
    keys: &[CorrectionKey],
    now: f32,
) -> (Vec<Element>, Vec<Element>) {
    let mut stops: Vec<(f32, &[Element], &[Element])> = vec![(0.0, rest.0, rest.1)];
    stops.extend(keys.iter().map(|k| (k.turn, k.colour_art.as_slice(), k.line_art.as_slice())));
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let last = stops.len() - 1;
    let after = stops.partition_point(|s| s.0 <= now);
    let (from, to) = match after {
        0 => (stops[0], stops[0]),
        i if i > last => (stops[last], stops[last]),
        i => (stops[i - 1], stops[i]),
    };
    let t = if (to.0 - from.0).abs() < 1e-4 { 0.0 } else { (now - from.0) / (to.0 - from.0) };
    let between = |a: &[Element], b: &[Element]| crate::morph::morph(a, b, (&[], &[]), t);
    (between(from.1, to.1), between(from.2, to.2))
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
    /// Each point follows the bones near it, but only this bone and the
    /// bones hanging from it: a limb. Two legs drawn over each other (a
    /// walk from the side) each bend with their own bones alone.
    Limb(usize),
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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pose {
    pub moves: Vec<Affine>,
    /// The same without the stretching: how each bone carries the bones
    /// that hang from it, which follow its end but aren't stretched.
    rigid: Vec<Affine>,
}

/// How many frames a bone with dynamics is simulated for before the one
/// asked for, from rest. Long enough for any swing to have settled, so the
/// pose doesn't depend on how far into the scene the frame is.
const SETTLE: u32 = 240;

/// What `pose_with` needs to place one bone: its turn, move and stretch.
type Placing = (f32, Point, f32);

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
            limits: None,
            stretch: None,
            squash: false,
            dynamics: None,
            target: None,
            preview: TurnPreview::default(),
        }
    }

    /// Its turn as shown on `frame`: as animated, or as previewed.
    pub fn shown_turn(&self, frame: f32) -> f32 {
        self.preview.0.unwrap_or_else(|| self.turn.sample(frame).unwrap_or(0.0))
    }

    /// Its turn on `frame`, kept within its limits.
    fn moved_at(&self, frame: f32) -> Point {
        self.moved.sample(frame).unwrap_or(Point::ZERO)
    }

    fn turn_at(&self, frame: f32) -> f32 {
        let turn = self.shown_turn(frame);
        self.limits.map_or(turn, |l| l.clamp(turn))
    }

    /// How stretched it is on `frame`.
    fn stretch_at(&self, frame: f32) -> f32 {
        self.stretch.as_ref().and_then(|s| s.sample(frame)).unwrap_or(1.0).max(0.01)
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

    /// Where every bone is on `frame`. Bones with dynamics are simulated
    /// up to it, swinging after the bones they hang from.
    ///
    /// While a frame is drawn (see `draw_cache.rs`), each skeleton's pose on
    /// a frame is worked out once and shared: every layer a skeleton bends
    /// asks for it, and target bones and dynamics make it costly.
    pub fn pose(&self, frame: u32) -> Pose {
        crate::draw_cache::pose(self as *const Skeleton as usize, frame, || self.work_out_pose(frame))
    }

    fn work_out_pose(&self, frame: u32) -> Pose {
        if self.bones.iter().all(|b| b.dynamics.is_none()) {
            let placing = self.placing(frame);
            return self.pose_with(|i, _| placing[i]);
        }
        // Each swinging bone's tip, where it's got to and how fast it's going.
        let mut tips: Vec<Option<(Point, Point)>> = vec![None; self.bones.len()];
        let mut pose = Pose { moves: Vec::new(), rigid: Vec::new() };
        for f in frame.saturating_sub(SETTLE)..=frame {
            let placing = self.placing(f);
            pose = Pose {
                moves: Vec::with_capacity(self.bones.len()),
                rigid: Vec::with_capacity(self.bones.len()),
            };
            for (index, bone) in self.bones.iter().enumerate() {
                let (turn, moved, stretch) = placing[index];
                let (skin, rigid) = self.place(index, &pose, turn, moved, stretch);
                let Some(dynamics) = bone.dynamics else {
                    pose.moves.push(skin);
                    pose.rigid.push(rigid);
                    continue;
                };
                // Where it's posed to be, and where its tip has got to: pulled
                // toward the posed tip like a weight on a spring, but kept the
                // bone's length from its start.
                let (start, target) = (skin.apply(bone.start), skin.apply(bone.end));
                let (was, speed) = tips[index].unwrap_or((target, Point::ZERO));
                let speed = speed * (1.0 - dynamics.damping.clamp(0.0, 1.0))
                    + (target - was) * dynamics.spring.clamp(0.0, 1.0);
                let mut tip = was + speed;
                let (reach, along) = (target.distance(start), tip - start);
                if along.length() > 1e-4 {
                    tip = start + along * (reach / along.length());
                }
                let mut swing = angle_between(target - start, tip - start);
                let limit = dynamics.limit.abs();
                if swing.abs() > limit {
                    swing = swing.clamp(-limit, limit);
                    tip = start + rotate_by(target - start, swing);
                }
                tips[index] = Some((tip, tip - was));
                let (skin, rigid) = self.place(index, &pose, turn + swing, moved, stretch);
                pose.moves.push(skin);
                pose.rigid.push(rigid);
            }
        }
        pose
    }

    /// Every bone's turn, move and stretch on `frame`, as keyed, then with
    /// the bones that reach for a target turned to reach it.
    fn placing(&self, frame: u32) -> Vec<Placing> {
        let at = frame as f32;
        let mut placing: Vec<Placing> =
            self.bones.iter().map(|b| (b.turn_at(at), b.moved_at(at), b.stretch_at(at))).collect();
        for (index, bone) in self.bones.iter().enumerate() {
            let Some(target) = bone.target else { continue };
            let chain = self.target_chain(index, target);
            if chain.is_empty() {
                continue;
            }
            // Cyclic coordinate descent, as `reach` does, toward where the
            // target starts with everything else as it's posed, until it's
            // there (or as near as the bones reach).
            for _ in 0..24 {
                let pose = self.pose_with(|i, _| placing[i]);
                let goal = pose.moves[target].apply(self.bones[target].start);
                if pose.moves[index].apply(self.bones[index].end).distance(goal) < 0.05 {
                    break;
                }
                for &joint in &chain {
                    let pose = self.pose_with(|i, _| placing[i]);
                    let pivot = pose.moves[joint].apply(self.bones[joint].start);
                    let tip = pose.moves[index].apply(self.bones[index].end);
                    let (now, want) = (tip - pivot, goal - pivot);
                    if now.length() < 1e-4 || want.length() < 1e-4 {
                        continue;
                    }
                    let mut turn = placing[joint].0 + angle_between(now, want);
                    if let Some(limits) = self.bones[joint].limits {
                        turn = limits.clamp(turn);
                    }
                    placing[joint].0 = turn;
                }
            }
        }
        placing
    }

    /// The bones turned to bring bone `index`'s end to `target`: it and up
    /// to two above it, short of the root (which holds the character where
    /// it is). Empty if the target can't be reached that way: itself, or
    /// carried by the bones that would turn, which would chase it round.
    pub fn target_chain(&self, index: usize, target: usize) -> Vec<usize> {
        if target == index || target >= self.bones.len() {
            return Vec::new();
        }
        let mut chain = self.chain(index);
        if chain.len() > 1 {
            chain.pop();
        }
        chain.truncate(3);
        let carried = self.chain(target).iter().any(|b| chain.contains(b));
        if carried { Vec::new() } else { chain }
    }

    /// Where every bone is with the turn, move and stretch `posed` gives
    /// each (by its place).
    fn pose_with(&self, posed: impl Fn(usize, &Bone) -> Placing) -> Pose {
        let mut pose =
            Pose { moves: Vec::with_capacity(self.bones.len()), rigid: Vec::with_capacity(self.bones.len()) };
        for (index, bone) in self.bones.iter().enumerate() {
            let (turn, moved, stretch) = posed(index, bone);
            let (skin, rigid) = self.place(index, &pose, turn, moved, stretch);
            pose.moves.push(skin);
            pose.rigid.push(rigid);
        }
        pose
    }

    /// Places bone `index`, the bones before it placed in `pose` already:
    /// carried by the bone it hangs from, then moved, turned about its own
    /// start and stretched along itself. Returns the matrix that takes a
    /// point from rest to where it goes, and the same without the stretch.
    fn place(&self, index: usize, pose: &Pose, turn: f32, moved: Point, stretch: f32) -> (Affine, Affine) {
        let bone = &self.bones[index];
        // A bone hangs from where its parent has taken its start, stretched
        // or not, but turns with the parent unstretched, so stretching an
        // upper arm doesn't stretch the forearm too.
        let carried = match self.parent_of(index) {
            Some(p) => {
                let follow = pose.moves[p].apply(bone.start) - pose.rigid[p].apply(bone.start);
                Affine::translate(follow).pre_concat(pose.rigid[p])
            }
            None => Affine::IDENTITY,
        };
        let frame = carried.pre_concat(
            Affine::translate(bone.start + moved)
                .pre_concat(Affine::rotate(bone.rest_angle() + turn.to_radians())),
        );
        let undo_rest = bone.rest_frame().inverse().unwrap_or(Affine::IDENTITY);
        // Keeping its area, as a flat drawing has no volume to keep.
        let across = if bone.squash { 1.0 / stretch } else { 1.0 };
        let skin = frame.pre_concat(Affine::scale(stretch, across)).pre_concat(undo_rest);
        (skin, frame.pre_concat(undo_rest))
    }

    /// Where bone `index` starts and ends on `frame`.
    pub fn posed_ends(&self, index: usize, frame: u32) -> Option<(Point, Point)> {
        let bone = self.bones.get(index)?;
        let m = *self.pose(frame).moves.get(index)?;
        Some((m.apply(bone.start), m.apply(bone.end)))
    }

    /// Whether no bone is turned, moved or stretched on `frame`. A
    /// skeleton with dynamics may still be swinging from earlier, so it's
    /// only at rest if it never moves at all.
    pub fn is_at_rest(&self, frame: u32) -> bool {
        let still = |b: &Bone, f: f32| {
            b.turn_at(f).abs() < 1e-4 && b.moved_at(f).length() < 1e-4 && (b.stretch_at(f) - 1.0).abs() < 1e-4
        };
        if self.bones.iter().any(|b| b.dynamics.is_some()) {
            let keys = self.key_frames();
            return self
                .bones
                .iter()
                .all(|b| keys.iter().all(|&k| still(b, k as f32)) && still(b, frame as f32));
        }
        self.bones.iter().all(|b| still(b, frame as f32))
    }

    /// How much each bone pulls on the point `p` (where it is at rest), as
    /// fractions that add up to 1. Near a bone it's nearly all that bone;
    /// halfway between two, half each.
    pub fn weights(&self, p: Point) -> Vec<f32> {
        self.weights_among(p, None)
    }

    /// As [`Skeleton::weights`], counting only the bones `among` marks (all
    /// of them with `None`); the rest pull on nothing.
    fn weights_among(&self, p: Point, among: Option<&[bool]>) -> Vec<f32> {
        let counts = |i: usize| among.is_none_or(|a| a.get(i).copied().unwrap_or(false));
        let raw: Vec<f32> = self
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if !counts(i) {
                    return 0.0;
                }
                let reach = (p.distance_to_segment(b.start, b.end) / b.strength.max(1e-3)).powi(4);
                1.0 / (1e-6 + reach)
            })
            .collect();
        let sum: f32 = raw.iter().sum();
        if sum <= 0.0 || !sum.is_finite() {
            let shared = (0..self.bones.len()).filter(|&i| counts(i)).count().max(1) as f32;
            return (0..self.bones.len()).map(|i| if counts(i) { 1.0 / shared } else { 0.0 }).collect();
        }
        raw.into_iter().map(|w| w / sum).collect()
    }

    /// The bones a [`BoneBinding::Limb`] at `bone` follows: it and every
    /// bone hanging from it, marked by place.
    pub fn limb(&self, bone: usize) -> Vec<bool> {
        (0..self.bones.len()).map(|i| self.chain(i).contains(&bone)).collect()
    }

    /// Where the point `p` (at rest, in the timeline's coordinates) goes
    /// in `pose`, bound by region.
    pub fn carry(&self, pose: &Pose, p: Point) -> Point {
        self.carry_among(pose, p, None)
    }

    /// As [`Skeleton::carry`], following only the bones `among` marks.
    pub fn carry_among(&self, pose: &Pose, p: Point, among: Option<&[bool]>) -> Point {
        let mut to = Point::ZERO;
        for (w, m) in self.weights_among(p, among).into_iter().zip(&pose.moves) {
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
        let limb = match binding {
            BoneBinding::Limb(bone) => Some(self.limb(bone)),
            _ => None,
        };
        // Where a point goes: with its own bone, or else as the layer does.
        let bend = |bone: Option<usize>, p: Point| match bone.or(match binding {
            BoneBinding::Bone(b) => Some(b),
            _ => None,
        }) {
            Some(b) => follow(b).map_or(p, |m| m.apply(p)),
            None if matches!(binding, BoneBinding::Region | BoneBinding::Limb(_)) => {
                from_rest.apply(self.carry_among(pose, to_rest.apply(p), limb.as_deref()))
            }
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
                    BoneBinding::Region | BoneBinding::Limb(_) => bend_element(element, piece, &[], &bend),
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
        let at = frame as f32;
        let mut turns: Vec<f32> = self.bones.iter().map(|b| b.turn_at(at)).collect();
        let moved: Vec<Point> = self.bones.iter().map(|b| b.moved_at(at)).collect();
        let stretch: Vec<f32> = self.bones.iter().map(|b| b.stretch_at(at)).collect();
        // Cyclic coordinate descent: from the bone itself up to the root,
        // turn each joint so the end points at the target; a few rounds
        // settle it (see `ik.rs`).
        for _ in 0..12 {
            for &joint in &chain {
                let pose = self.pose_with(|i, _| (turns[i], moved[i], stretch[i]));
                let m = pose.moves[joint];
                let pivot = m.apply(self.bones[joint].start);
                let tip = pose.moves[index].apply(self.bones[index].end);
                let (now, want) = (tip - pivot, target - pivot);
                if now.length() < 1e-4 || want.length() < 1e-4 {
                    continue;
                }
                let change = (want.y.atan2(want.x) - now.y.atan2(now.x)).to_degrees();
                turns[joint] += (change + 180.0).rem_euclid(360.0) - 180.0;
                // A joint at its limit stops there, and the ones above it
                // make up the rest.
                if let Some(limits) = self.bones[joint].limits {
                    turns[joint] = limits.clamp(turns[joint]);
                }
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
        let carried = self.parent_of(index).map_or(Affine::IDENTITY, |p| pose.rigid[p]);
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
                bone.target = bone.target.and_then(|t| map.get(t).copied().flatten());
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
        self.bones.iter_mut().flat_map(|b| {
            let stretch = b.stretch.as_mut().map(|s| s as &mut dyn AnyChannel);
            [Some(&mut b.turn as &mut dyn AnyChannel), Some(&mut b.moved as &mut dyn AnyChannel), stretch]
                .into_iter()
                .flatten()
        })
    }

    pub fn key_frames(&self) -> Vec<u32> {
        let mut frames: Vec<u32> = self
            .bones
            .iter()
            .flat_map(|b| {
                let stretch = b.stretch.iter().flat_map(|s| s.key_frames());
                b.turn.key_frames().into_iter().chain(b.moved.key_frames()).chain(stretch)
            })
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

/// Whether skeletons are shown at rest (see [`crate::Document::rest_pose`]).
/// Two documents are the same whatever this says, so switching it never
/// counts as a change to undo.
#[derive(Debug, Clone, Copy, Default)]
pub struct RestPoseView(pub bool);

impl PartialEq for RestPoseView {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// How big a piece of the triangle mesh a bent picture or painting is drawn
/// through is, in the layer's units (see the renderer's `draw_bent`), and
/// what painting on it is mapped back through (see [`LayerBend::unbend`]).
pub const MESH_CELL: f32 = 16.0;

/// Where the mesh's lines run across `min` to `max`: on every multiple of
/// [`MESH_CELL`] between, and at both ends. Lined up on the multiples, the
/// mesh a picture is drawn through and the one painting is mapped back
/// through cut the same pieces wherever they overlap, so a brush lands
/// exactly where it's seen, wherever each mesh happens to end.
pub fn mesh_lines(min: f32, max: f32) -> Vec<f32> {
    let mut lines = vec![min];
    let mut at = (min / MESH_CELL).floor() * MESH_CELL + MESH_CELL;
    // At most a few thousand pieces each way, for an absurdly large layer.
    while at < max - 1e-3 && lines.len() < 4096 {
        if at > min + 1e-3 {
            lines.push(at);
        }
        at += MESH_CELL;
    }
    lines.push(max);
    lines
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
            BoneBinding::Limb(bone) => {
                let limb = self.skeleton.limb(bone);
                self.from_bones.apply(self.skeleton.carry_among(
                    &self.pose,
                    self.to_bones.apply(p),
                    Some(&limb),
                ))
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

    /// The point at rest that bends to `p` (both in the layer's
    /// coordinates), so a brush can paint on a bent layer where it's seen.
    /// Bending can't be undone directly, so this goes back through the same
    /// mesh the renderer draws with: over `area` (where the layer may be
    /// painted), the bent triangle `p` falls in, and the matching point of
    /// that triangle at rest. Where bent pieces overlap (a folded elbow),
    /// the one drawn last, on top, wins. `None` outside the bent mesh.
    pub fn unbend(&self, p: Point, area: crate::Rect) -> Option<Point> {
        if let Some(m) = self.rigid() {
            return m.inverse().map(|back| back.apply(p));
        }
        let (xs, ys) = (mesh_lines(area.min.x, area.max.x), mesh_lines(area.min.y, area.max.y));
        let (columns, rows) = (xs.len() - 1, ys.len() - 1);
        let rest = |c: usize, r: usize| Point::new(xs[c], ys[r]);
        let bent: Vec<Point> = (0..=rows)
            .flat_map(|r| (0..=columns).map(move |c| (c, r)))
            .map(|(c, r)| self.bend(rest(c, r)))
            .collect();
        let at = |c: usize, r: usize| bent[r * (columns + 1) + c];
        let mut found = None;
        for r in 0..rows {
            for c in 0..columns {
                let corners = [(c, r), (c + 1, r), (c + 1, r + 1), (c, r + 1)];
                for [a, b, d] in [[0, 1, 2], [0, 2, 3]] {
                    let to = [corners[a], corners[b], corners[d]].map(|(c, r)| at(c, r));
                    let Some((u, v)) = barycentric(p, to) else { continue };
                    let from = [corners[a], corners[b], corners[d]].map(|(c, r)| rest(c, r));
                    found = Some(from[0] + (from[1] - from[0]) * u + (from[2] - from[0]) * v);
                }
            }
        }
        found
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

/// Where `p` is in triangle `t`, as how far along its first and second
/// edges, if it's inside.
fn barycentric(p: Point, t: [Point; 3]) -> Option<(f32, f32)> {
    let (e1, e2, q) = (t[1] - t[0], t[2] - t[0], p - t[0]);
    let det = e1.x * e2.y - e1.y * e2.x;
    if det.abs() < 1e-9 {
        return None;
    }
    let u = (q.x * e2.y - q.y * e2.x) / det;
    let v = (e1.x * q.y - e1.y * q.x) / det;
    const SLACK: f32 = 1e-4;
    (u >= -SLACK && v >= -SLACK && u + v <= 1.0 + SLACK).then_some((u, v))
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
    weld_pins(&drawing.shown(), &mut pins);
    pins
}

/// Pins carry across welds (see `weld.rs`): a point welded to a pinned
/// point is pinned to the same bone, unless it's pinned already, so two
/// shapes joined there stay joined when the bones pull them apart.
fn weld_pins(shown: &[Element], pins: &mut [ShapeBones]) {
    let mut carried: Vec<(usize, usize, usize)> = Vec::new();
    for (i, pin) in pins.iter().enumerate() {
        let Some(element) = shown.get(i) else { continue };
        let anchors = element.shape.to_anchors().0;
        let bone_of = |a: usize| match pin {
            ShapeBones::Whole(bone) => Some(*bone),
            ShapeBones::Points(points) => points.get(a).copied().flatten(),
            ShapeBones::Layer => None,
        };
        for (a, anchor) in anchors.iter().enumerate() {
            let Some(bone) = bone_of(a) else { continue };
            for (other, point) in crate::welded_at(shown, anchor.point, Some(i)) {
                carried.push((other, point, bone));
            }
        }
    }
    for (other, point, bone) in carried {
        let count = shown[other].shape.to_anchors().0.len();
        let pin = &mut pins[other];
        if matches!(pin, ShapeBones::Whole(_)) {
            continue;
        }
        if *pin == ShapeBones::Layer {
            *pin = ShapeBones::Points(vec![None; count]);
        }
        if let ShapeBones::Points(points) = pin
            && let Some(slot) = points.get_mut(point)
        {
            slot.get_or_insert(bone);
        }
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
        if self.rest_pose.0 {
            return None;
        }
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

    /// [`Document::posed_elements`], worked out once per layer and frame
    /// while a frame is drawn (see `draw_cache.rs`) and shared after, rather
    /// than bent again for every band of the picture that draws it.
    pub fn posed_shapes<'a>(&self, layer: &'a crate::Layer, frame: u32) -> crate::Posed<'a> {
        let shared = crate::draw_cache::posed(layer as *const crate::Layer as usize, frame, || {
            match self.posed_elements(layer, frame) {
                std::borrow::Cow::Owned(posed) => Some(std::sync::Arc::new(posed)),
                // Unchanged by posing, so borrowed as it is: nothing to share.
                std::borrow::Cow::Borrowed(_) => None,
            }
        });
        match shared {
            Some(posed) => crate::Posed::Shared(posed),
            None => crate::Posed::Own(self.posed_elements(layer, frame)),
        }
    }

    /// What `layer` draws on `frame`: as its own deformers bend it (see
    /// [`crate::Layer::shown_elements`]), reshaped by any smart bone that
    /// morphs it, and then bent by the skeleton it's bound to. Pictures and
    /// paintings come back unbent: the renderer bends those through a mesh
    /// with [`Document::layer_bend`].
    pub fn posed_elements<'a>(&self, layer: &'a crate::Layer, frame: u32) -> std::borrow::Cow<'a, [Element]> {
        let mut shown = layer.shown_elements(frame);
        if self.rest_pose.0 {
            return shown;
        }
        if let Some(skeleton) = self.skeleton_layer_of(layer).and_then(|o| o.skeleton.as_ref()) {
            let from = layer.drawings.get(layer.drawing_index_at(frame));
            for bone in &skeleton.bones {
                let now = bone.shown_turn(frame as f32);
                for action in &bone.actions {
                    match action {
                        SmartAction::Morph { layer: target, drawing, .. } => {
                            let amount = action.amount(now);
                            let (Some(from), Some(to)) = (from, layer.drawings.get(*drawing)) else {
                                continue;
                            };
                            if *target == layer.id && amount > 0.0 {
                                let morphed = crate::morph::morph(
                                    &shown,
                                    &to.shown(),
                                    (&from.hints, &to.hints),
                                    amount,
                                );
                                shown = std::borrow::Cow::Owned(morphed);
                            }
                        }
                        // Corrections key the drawing showing, as drawn (not
                        // part way through a shape tween).
                        SmartAction::Correct { layer: target, drawing, keys } => {
                            let shows =
                                layer.drawing_index_at(frame) == *drawing && layer.morph_at(frame).is_none();
                            if *target != layer.id || !shows || keys.is_empty() || now.abs() < 1e-4 {
                                continue;
                            }
                            let Some(rest) = layer.drawings.get(*drawing) else { continue };
                            let (colour, line) = corrected((&rest.colour_art, &rest.elements), keys, now);
                            shown = std::borrow::Cow::Owned(colour.into_iter().chain(line).collect());
                        }
                        SmartAction::Switch { .. } => {}
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
        if self.rest_pose.0 {
            return None;
        }
        let skeleton = self.skeleton_layer_of(folder).and_then(|o| o.skeleton.as_ref())?;
        let mut best: Option<(f32, LayerId)> = None;
        for bone in &skeleton.bones {
            let now = bone.shown_turn(frame as f32);
            for action in &bone.actions {
                let &SmartAction::Switch { folder: target, layer, turn } = action else { continue };
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

/// The turn from `from` to `to`, in degrees, the short way round.
fn angle_between(from: Point, to: Point) -> f32 {
    let change = (to.y.atan2(to.x) - from.y.atan2(from.x)).to_degrees();
    (change + 180.0).rem_euclid(360.0) - 180.0
}

/// `p` turned by `degrees` about the origin.
fn rotate_by(p: Point, degrees: f32) -> Point {
    Affine::rotate(degrees.to_radians()).apply(p)
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
    fn cached_poses_last_only_as_long_as_their_scope() {
        let mut skeleton = Skeleton::default();
        let bone = skeleton.add_bone(None, Point::new(0.0, 0.0), Point::new(100.0, 0.0), 0);
        let tip = |s: &Skeleton| s.pose(0).moves[bone].apply(Point::new(100.0, 0.0));
        let still = crate::with_pose_cache(|| {
            let first = tip(&skeleton);
            // Asked again in the scope (as every layer it bends asks), the same.
            assert_eq!(crate::with_pose_cache(|| tip(&skeleton)), first);
            first
        });
        // Turned afterwards, it's posed afresh: nothing cached survives.
        skeleton.bones[bone].turn.set(0, 90.0);
        let turned = tip(&skeleton);
        assert!(still.distance(turned) > 100.0, "{still:?} {turned:?}");
        // And a scope that panics still clears it.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::with_pose_cache(|| {
                tip(&skeleton);
                panic!("drawing failed");
            })
        }));
        skeleton.bones[bone].turn.set(0, 0.0);
        assert!(tip(&skeleton).distance(still) < 1e-3);
    }

    #[test]
    fn a_layer_bound_to_a_limb_ignores_the_other_bones() {
        // Two legs over the same spot, from a hip: each a thigh and a shin.
        let mut skeleton = Skeleton::default();
        let hip = skeleton.add_bone(None, Point::new(0.0, -10.0), Point::new(0.0, 0.0), 0);
        let mut thighs = Vec::new();
        for _ in 0..2 {
            let thigh = skeleton.add_bone(Some(hip), Point::new(0.0, 0.0), Point::new(0.0, 100.0), 0);
            skeleton.add_bone(Some(thigh), Point::new(0.0, 100.0), Point::new(0.0, 200.0), 0);
            thighs.push(thigh);
        }
        // Swing the second leg forward from the hip; the first hangs still.
        skeleton.bones[thighs[1]].turn.set(0, -40.0);
        let pose = skeleton.pose(0);
        let ankle = Point::new(0.0, 190.0);
        let first = skeleton.limb(thighs[0]);
        assert_eq!(first.iter().filter(|&&b| b).count(), 2, "the thigh and its shin");
        let still = skeleton.carry_among(&pose, ankle, Some(&first));
        assert!(still.distance(ankle) < 1e-3, "{still:?}");
        let swung = skeleton.carry_among(&pose, ankle, Some(&skeleton.limb(thighs[1])));
        assert!(swung.x > 100.0, "{swung:?}");
        // Bound by region, the same point is pulled halfway between.
        let both = skeleton.carry(&pose, ankle);
        assert!((both.x - swung.x / 2.0).abs() < 5.0, "{both:?}");
    }

    #[test]
    fn a_previewed_turn_poses_the_bone_without_changing_the_document() {
        let (mut doc, folder, layer) = rigged_bar();
        let mut tall = doc.layer(layer).unwrap().drawings[0].clone();
        tall.elements[0].shape = Shape::Rect { width: 200.0, height: 30.0, corner_radius: 0.0 };
        let drawing = doc.layer_mut(layer).unwrap().add_drawing(tall);
        doc.layer_mut(layer).unwrap().bone_binding = BoneBinding::Unbound;
        let skeleton = doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap();
        skeleton.bones[1].actions.push(SmartAction::Morph { layer, drawing, turn: 40.0 });
        let before = doc.clone();

        // Shown turned 40°, though its animation never turns: the morph
        // runs all the way, and the bone is posed there.
        let bone = &mut doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap().bones[1];
        bone.preview.0 = Some(40.0);
        let height = doc.posed_elements(doc.layer(layer).unwrap(), 0)[0].shape.bounds().height();
        assert!((height - 30.0).abs() < 0.5, "{height}");
        let skeleton = doc.layer(folder).unwrap().skeleton.as_ref().unwrap();
        assert!(!skeleton.is_at_rest(0));
        assert_eq!(skeleton.bones[1].turn.sample(0.0), Some(0.0));
        // And nothing to undo or save, and copies (undo steps, exports)
        // show the animation.
        assert_eq!(doc, before);
        let copy = doc.clone();
        assert_eq!(copy.layer(folder).unwrap().skeleton.as_ref().unwrap().bones[1].preview.0, None);
        assert!(!serde_json::to_string(&doc).unwrap().contains("preview"));
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

    #[test]
    fn limits_stop_a_joint_and_the_ones_above_make_up_the_rest() {
        let mut skeleton = arm();
        skeleton.bones[1].limits = Some(crate::RotationLimits { min: 0.0, max: 45.0 });
        // Straight back over the shoulder: the elbow can't fold that far.
        skeleton.reach(1, Point::new(50.0, 50.0), 0, false);
        let elbow = skeleton.bones[1].turn.sample(0.0).unwrap();
        assert!((0.0..=45.0 + 1e-3).contains(&elbow), "{elbow}");
        // Typed past its limit, it's held at it.
        skeleton.bones[1].turn.set(5, 120.0);
        let (start, end) = skeleton.posed_ends(1, 5).unwrap();
        let angle =
            (end - start).y.atan2((end - start).x).to_degrees() - skeleton.bones[0].turn.sample(5.0).unwrap();
        assert!((angle - 45.0).abs() < 1e-2, "{angle}");
    }

    #[test]
    fn stretching_a_bone_carries_the_next_without_stretching_it() {
        let mut skeleton = arm();
        skeleton.bones[0].stretch = Some(Channel::new(0, 1.5));
        let (_, end) = skeleton.posed_ends(0, 0).unwrap();
        assert!(close(end, Point::new(150.0, 0.0)), "{end:?}");
        let (start, end) = skeleton.posed_ends(1, 0).unwrap();
        assert!(
            close(start, Point::new(150.0, 0.0)) && close(end, Point::new(250.0, 0.0)),
            "{start:?} {end:?}"
        );
    }

    #[test]
    fn squashing_keeps_a_drawing_about_the_same_size() {
        let mut skeleton = arm();
        skeleton.bones[0].stretch = Some(Channel::new(0, 0.5));
        skeleton.bones[0].squash = true;
        let pose = skeleton.pose(0);
        // Half as long, so twice as wide: the area it covers stays the same.
        let side = pose.moves[0].apply(Point::new(50.0, 10.0));
        assert!(close(side, Point::new(25.0, 20.0)), "{side:?}");
    }

    #[test]
    fn a_bone_with_dynamics_swings_after_its_parent_and_settles() {
        let mut skeleton = arm();
        skeleton.bones[1].dynamics = Some(Dynamics::default());
        // The upper arm whips round 90° between frames 10 and 12, then holds.
        skeleton.bones[0].turn.set(10, 0.0);
        skeleton.bones[0].turn.set(12, 90.0);
        let swing = |frame| {
            let (start, end) = skeleton.posed_ends(1, frame).unwrap();
            let posed = skeleton.bones[0].turn_at(frame as f32);
            (end - start).y.atan2((end - start).x).to_degrees() - posed
        };
        assert!(swing(5).abs() < 1e-3, "still before the whip");
        assert!(swing(12) < -5.0, "lags behind as the arm swings: {}", swing(12));
        assert!(swing(200).abs() < 0.5, "settled long after: {}", swing(200));
        assert!(!skeleton.is_at_rest(200), "a skeleton that ever moves may still be swinging");
    }

    #[test]
    fn pins_carry_to_welded_points_so_shapes_stay_joined() {
        let (mut doc, folder, layer) = rigged_bar();
        doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap().bones[1].turn.set(0, 90.0);
        // A second box sharing the bar's right edge (at x 100 in the bar's
        // coordinates), pinned whole to the upper arm; the bar's right end
        // is welded to it, so it follows the upper arm too.
        let style = crate::Style { fill: Some(crate::Color::BLACK), stroke: None, gradient: None };
        let cap = Element { shape: Shape::Rect { width: 20.0, height: 10.0, corner_radius: 0.0 }, style }
            .transformed(Affine::translate(Point::new(110.0, 0.0)))
            .unwrap();
        let drawing = &mut doc.layer_mut(layer).unwrap().drawings[0];
        drawing.elements.push(cap);
        drawing.bone_points.push(BonePoints { colour_art: false, element: 1, anchors: None, bone: 0 });
        let points = outline_of(&doc, layer, 0);
        assert!(
            points.iter().any(|p| p.distance(Point::new(100.0, -5.0)) < 1e-2),
            "the welded end stays with the cap"
        );
    }

    #[test]
    fn unbending_finds_the_point_at_rest_that_bends_to_where_its_seen() {
        let (mut doc, folder, layer) = rigged_bar();
        doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap().bones[1].turn.set(0, 90.0);
        let layer = doc.layer(layer).unwrap();
        let bend = doc.layer_bend(layer, 0).unwrap();
        let area = crate::Rect::from_center_size(Point::ZERO, 400.0, 400.0);
        // A point near the forearm's tip (x 90 in the bar's coordinates,
        // which sit 100 to the right of the bones'), and back.
        // Points where the mesh's lines cross are bent exactly as drawn, so
        // they come straight back, even round the elbow.
        for at in
            [Point::new(80.0, 0.0), Point::new(-64.0, 16.0), Point::new(16.0, -16.0), Point::new(0.0, 0.0)]
        {
            let seen = bend.bend(at);
            let back = bend.unbend(seen, area).expect("inside the mesh");
            assert!(back.distance(at) < 1e-2, "{at:?} -> {seen:?} -> {back:?}");
        }
        assert!(bend.unbend(Point::new(5000.0, 0.0), area).is_none());
    }

    #[test]
    fn mesh_lines_sit_on_the_cells_multiples() {
        assert_eq!(mesh_lines(-20.0, 40.0), [-20.0, -16.0, 0.0, 16.0, 32.0, 40.0]);
        assert_eq!(mesh_lines(0.0, 32.0), [0.0, 16.0, 32.0]);
    }

    #[test]
    fn showing_skeletons_at_rest_turns_bending_off() {
        let (mut doc, folder, layer) = rigged_bar();
        doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap().bones[1].turn.set(0, 90.0);
        assert!(doc.layer_bend(doc.layer(layer).unwrap(), 0).is_some());
        let before = doc.clone();
        doc.rest_pose.0 = true;
        assert!(doc.layer_bend(doc.layer(layer).unwrap(), 0).is_none());
        assert!(doc == before, "only a view: the document's the same");
    }

    #[test]
    fn a_leg_reaches_for_its_target_and_stays_planted_as_the_body_moves() {
        // A hip (the root), a thigh and a shin hanging straight down, and a
        // foot target on the ground ahead.
        let mut skeleton = Skeleton::default();
        let hip = skeleton.add_bone(None, Point::new(0.0, 0.0), Point::new(0.0, 10.0), 0);
        let thigh = skeleton.add_bone(Some(hip), Point::new(0.0, 10.0), Point::new(0.0, 110.0), 0);
        let shin = skeleton.add_bone(Some(thigh), Point::new(0.0, 110.0), Point::new(0.0, 210.0), 0);
        let foot = skeleton.add_bone(None, Point::new(60.0, 180.0), Point::new(90.0, 180.0), 0);
        skeleton.bones[shin].target = Some(foot);
        assert_eq!(skeleton.target_chain(shin, foot), [shin, thigh]);
        let (_, heel) = skeleton.posed_ends(shin, 0).unwrap();
        assert!(heel.distance(Point::new(60.0, 180.0)) < 0.5, "{heel:?}");
        // The body moves along and down; the foot stays where it is.
        skeleton.move_bone(hip, Point::new(30.0, 20.0), 5);
        let (_, heel) = skeleton.posed_ends(shin, 5).unwrap();
        assert!(heel.distance(Point::new(60.0, 180.0)) < 0.5, "{heel:?}");
    }

    #[test]
    fn a_bone_cant_reach_for_what_it_carries() {
        let mut skeleton = arm();
        assert!(skeleton.target_chain(0, 1).is_empty(), "the forearm hangs from the upper arm");
        assert!(skeleton.target_chain(1, 1).is_empty());
        skeleton.bones[0].target = Some(1);
        // Ignored: the arm poses as keyed.
        let (_, end) = skeleton.posed_ends(1, 0).unwrap();
        assert!(close(end, Point::new(200.0, 0.0)));
    }

    #[test]
    fn corrections_are_keyed_by_turn_and_morphed_between() {
        let (mut doc, folder, layer) = rigged_bar();
        let wide = |height: f32| Element {
            shape: Shape::Rect { width: 200.0, height, corner_radius: 0.0 },
            style: crate::Style { fill: Some(crate::Color::BLACK), stroke: None, gradient: None },
        };
        // The bar 10 tall at rest, 30 tall keyed at 40°, 50 tall at 80°.
        let keys = vec![
            CorrectionKey { turn: 40.0, colour_art: Vec::new(), line_art: vec![wide(30.0)] },
            CorrectionKey { turn: 80.0, colour_art: Vec::new(), line_art: vec![wide(50.0)] },
        ];
        let skeleton = doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap();
        skeleton.bones[1].actions.push(SmartAction::Correct { layer, drawing: 0, keys });
        doc.layer_mut(layer).unwrap().bone_binding = BoneBinding::Unbound;
        let height = |doc: &mut crate::Document, turn: f32| {
            let skeleton = doc.layer_mut(folder).unwrap().skeleton.as_mut().unwrap();
            skeleton.bones[1].turn.set(0, turn);
            doc.posed_elements(doc.layer(layer).unwrap(), 0)[0].shape.bounds().height()
        };
        for (turn, want) in
            [(0.0, 10.0), (20.0, 20.0), (40.0, 30.0), (60.0, 40.0), (120.0, 50.0), (-30.0, 10.0)]
        {
            let got = height(&mut doc, turn);
            assert!((got - want).abs() < 0.5, "at {turn}°: {got}");
        }
    }
}
