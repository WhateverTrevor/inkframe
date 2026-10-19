//! Bone deformers: bending one drawing smoothly, like a rubber-hose arm.
//!
//! Parenting (`rig.rs`) moves whole layers, so an arm made of an upper arm
//! and a forearm bends with a hard corner at the elbow. A deformer instead
//! runs a short chain of bones through a single drawing, and when a bone
//! turns the drawing bends along with it, smoothly through each joint. It's
//! Toon Boom Harmony's bone deformer, and the same idea as OpenToonz's
//! Plastic tool.
//!
//! Each point of the drawing follows the bones near it. Close to one bone
//! (well away from any joint) it moves rigidly with that bone; near a joint
//! it blends between the two bones meeting there, which is what rounds the
//! bend. How wide that blend is, is the deformer's `smoothness`. (This is
//! linear blend skinning, as 3D characters use.)
//!
//! Only outlines bend: rectangles, ovals and paths, whose points can each be
//! moved. Their segments are cut into short pieces first, or a long straight
//! edge across a joint would stay straight. Images and text are drawn as
//! they are.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, path_from_anchors};
use crate::channel::{AnyChannel, Channel};
use crate::document::{Element, Layer};
use crate::geometry::{Point, Rect};
use crate::shape::Shape;

/// A chain of bones through a layer's drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deformer {
    /// The joints at rest, in the layer's own coordinates: the chain's
    /// start, each joint between bones, and its tip. Bone `i` runs from
    /// joint `i` to joint `i + 1`.
    pub joints: Vec<Point>,
    /// How far each bone is turned from rest relative to the bone before it,
    /// in degrees, animated. Turning one bone carries those after it along,
    /// like bending an elbow carries the hand.
    pub bends: Vec<Channel<f32>>,
    /// How wide the soft blend around each joint is, as a fraction of the
    /// average bone's length: 0 bends like a hinge, 1 like a hose.
    #[serde(default = "default_smoothness")]
    pub smoothness: f32,
}

fn default_smoothness() -> f32 {
    0.35
}

/// One bone's move from rest: rotate by `turn` (radians) about its rest
/// start, then put that start at `start`.
#[derive(Debug, Clone, Copy)]
struct BonePose {
    rest_start: Point,
    rest_end: Point,
    start: Point,
    turn: f32,
}

impl BonePose {
    fn apply(&self, p: Point) -> Point {
        let (sin, cos) = self.turn.sin_cos();
        let d = p - self.rest_start;
        self.start + Point::new(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
    }
}

impl Deformer {
    /// A straight chain of `bones` equal bones along the long side of
    /// `bounds`, unbent, keyed at `frame`. The usual starting point: an arm
    /// or tail drawn lying along its length.
    pub fn along(bounds: Rect, bones: usize, frame: u32) -> Self {
        let bones = bones.max(1);
        let center = bounds.center();
        let (start, end) = if bounds.width() >= bounds.height() {
            (Point::new(bounds.min.x, center.y), Point::new(bounds.max.x, center.y))
        } else {
            (Point::new(center.x, bounds.min.y), Point::new(center.x, bounds.max.y))
        };
        let joints = (0..=bones).map(|i| start.lerp(end, i as f32 / bones as f32)).collect();
        let bends = (0..bones).map(|_| Channel::new(frame, 0.0)).collect();
        Self { joints, bends, smoothness: default_smoothness() }
    }

    pub fn bone_count(&self) -> usize {
        self.bends.len().min(self.joints.len().saturating_sub(1))
    }

    /// Each bone's bend on `frame`.
    pub fn bends_at(&self, frame: u32) -> Vec<f32> {
        self.bends.iter().take(self.bone_count()).map(|c| c.sample(frame as f32).unwrap_or(0.0)).collect()
    }

    /// Whether nothing is bent on `frame`, so the drawing is as drawn.
    pub fn is_at_rest(&self, frame: u32) -> bool {
        self.bends_at(frame).iter().all(|b| b.abs() < 1e-4)
    }

    /// The joints where they are on `frame`, in the layer's coordinates.
    pub fn posed_joints(&self, frame: u32) -> Vec<Point> {
        let poses = self.poses(frame);
        let mut joints: Vec<Point> = poses.iter().map(|p| p.start).collect();
        if let Some(last) = poses.last() {
            joints.push(last.apply(last.rest_end));
        } else if let Some(&only) = self.joints.first() {
            joints.push(only);
        }
        joints
    }

    fn poses(&self, frame: u32) -> Vec<BonePose> {
        let mut poses = Vec::with_capacity(self.bone_count());
        let mut turn = 0.0f32;
        let mut start = self.joints.first().copied().unwrap_or(Point::ZERO);
        for (i, bend) in self.bends_at(frame).into_iter().enumerate() {
            turn += bend.to_radians();
            let pose = BonePose { rest_start: self.joints[i], rest_end: self.joints[i + 1], start, turn };
            start = pose.apply(pose.rest_end);
            poses.push(pose);
        }
        poses
    }

    /// The blend radius around joints, in the layer's units.
    fn blend_radius(&self) -> f32 {
        let count = self.bone_count().max(1);
        let length: f32 = self.joints.windows(2).take(count).map(|w| w[0].distance(w[1])).sum();
        (self.smoothness.max(0.0) * length / count as f32).max(1e-3)
    }

    /// Where the drawing's point `p` goes, given the bones' poses.
    fn deform_point(&self, poses: &[BonePose], radius: f32, p: Point) -> Point {
        let distances: Vec<f32> =
            poses.iter().map(|b| p.distance_to_segment(b.rest_start, b.rest_end)).collect();
        let nearest = distances.iter().copied().fold(f32::INFINITY, f32::min);
        // Measured from the nearest bone, so a point always follows that
        // bone fully however far from the chain it is. Others join in as
        // they get within `radius` of being as close, easing in (smoothstep)
        // so the bend has no kinks, and not at all beyond that, so the
        // middle of a long bone stays perfectly rigid.
        let mut total = 0.0;
        let mut moved = Point::ZERO;
        for (bone, distance) in poses.iter().zip(distances) {
            let t = (1.0 - (distance - nearest) / radius).clamp(0.0, 1.0);
            let weight = t * t * (3.0 - 2.0 * t);
            let to = bone.apply(p);
            moved = Point::new(moved.x + to.x * weight, moved.y + to.y * weight);
            total += weight;
        }
        Point::new(moved.x / total, moved.y / total)
    }

    /// The drawing's shapes bent as on `frame`. Shapes that can't bend
    /// (images, text) come back as they were.
    pub fn deform(&self, elements: &[Element], frame: u32) -> Vec<Element> {
        let poses = self.poses(frame);
        if poses.is_empty() || self.is_at_rest(frame) {
            return elements.to_vec();
        }
        let radius = self.blend_radius();
        // Pieces short enough that each bends as one.
        let piece = radius / 2.0;
        let bend = |p: Point| self.deform_point(&poses, radius, p);
        elements
            .iter()
            .map(|element| {
                let (anchors, closed) = element.shape.to_anchors();
                let bent: Vec<Anchor> = subdivide(&anchors, closed, piece)
                    .into_iter()
                    .map(|a| Anchor {
                        point: bend(a.point),
                        handle_in: bend(a.handle_in),
                        handle_out: bend(a.handle_out),
                    })
                    .collect();
                match path_from_anchors(&bent, closed) {
                    Some(path) => Element { shape: Shape::Path(path), style: element.style },
                    None => element.clone(),
                }
            })
            .collect()
    }

    /// Turns bone `bone` on `frame` so it points at `target` (in the
    /// layer's coordinates), keying its bend. The bones after it keep their
    /// own bends, so they swing along.
    pub fn point_bone_at(&mut self, bone: usize, target: Point, frame: u32) {
        let poses = self.poses(frame);
        let Some(pose) = poses.get(bone) else { return };
        let rest = pose.rest_end - pose.rest_start;
        let now = rest.y.atan2(rest.x) + pose.turn;
        let to = target - pose.start;
        if to.length() < 1e-4 {
            return;
        }
        let change = (to.y.atan2(to.x) - now).to_degrees();
        let change = (change + 180.0).rem_euclid(360.0) - 180.0;
        let bends = &mut self.bends[bone];
        let value = bends.sample(frame as f32).unwrap_or(0.0) + change;
        bends.set(frame, value);
    }

    /// Every frame any bone has a key on.
    pub fn key_frames(&self) -> Vec<u32> {
        let mut frames: Vec<u32> = self.bends.iter().flat_map(|c| c.key_frames()).collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    /// Keys every bone's current bend on `frame`.
    pub fn insert_key(&mut self, frame: u32) -> bool {
        self.bends.iter_mut().fold(false, |added, c| c.insert_key(frame) | added)
    }

    pub fn remove_key(&mut self, frame: u32) -> bool {
        self.bends.iter_mut().fold(false, |removed, c| c.remove_key(frame) | removed)
    }

    pub fn move_key(&mut self, from: u32, to: u32) {
        for channel in &mut self.bends {
            channel.move_key(from, to);
        }
    }
}

impl Layer {
    /// What the layer draws on `frame`: the drawing showing then (or a
    /// shape tween's morph), bent by its bone deformer and then its warp
    /// grid, if it has them. Use this for anything that should match
    /// what's seen (drawing, hit testing, bounds), and `elements_at` for
    /// editing the drawing itself.
    pub fn shown_elements(&self, frame: u32) -> Cow<'_, [Element]> {
        // Part way through a shape tween, the morph; otherwise the drawing.
        let elements: Cow<'_, [Element]> = match self.morph_at(frame) {
            Some(morphed) => Cow::Owned(morphed),
            None => self.drawings.get(self.drawing_index_at(frame)).map_or(Cow::Borrowed(&[]), |d| d.shown()),
        };
        let bent = match &self.deformer {
            Some(deformer) if !deformer.is_at_rest(frame) => Cow::Owned(deformer.deform(&elements, frame)),
            _ => elements,
        };
        match &self.warp {
            Some(warp) if !warp.is_at_rest(frame) => Cow::Owned(warp.deform(&bent, frame)),
            _ => bent,
        }
    }

    /// Whether the transform or the deformer has a key on `frame`.
    pub fn has_key(&self, frame: u32) -> bool {
        self.key_frames().contains(&frame)
    }

    /// Keys everything that animates (the transform, any deformer and any
    /// warp) on `frame`, as F6 does.
    pub fn insert_key(&mut self, frame: u32) -> bool {
        let mut added = self.track.insert_key(frame);
        added |= self.deformer.as_mut().is_some_and(|d| d.insert_key(frame));
        added | self.warp.as_mut().is_some_and(|w| w.insert_key(frame))
    }

    pub fn remove_key(&mut self, frame: u32) -> bool {
        let mut removed = self.track.remove_key(frame);
        removed |= self.deformer.as_mut().is_some_and(|d| d.remove_key(frame));
        removed | self.warp.as_mut().is_some_and(|w| w.remove_key(frame))
    }

    /// Moves the keys on `from` to `to`: the transform's, the deformer's
    /// and the warp's together. Returns whether anything moved.
    pub fn move_key(&mut self, from: u32, to: u32) -> bool {
        let had_deformer_key = self.deformer.as_ref().is_some_and(|d| d.key_frames().contains(&from));
        let had_warp_key = self.warp.as_ref().is_some_and(|w| w.key_frames().contains(&from));
        let moved = self.track.move_key(from, to);
        if let Some(deformer) = &mut self.deformer
            && had_deformer_key
        {
            deformer.move_key(from, to);
        }
        if let Some(warp) = &mut self.warp
            && had_warp_key
        {
            warp.move_key(from, to);
        }
        moved || had_deformer_key || had_warp_key
    }

    /// Every frame with a key: the transform's, the deformer's and the
    /// warp's.
    pub fn key_frames(&self) -> Vec<u32> {
        let mut frames = self.track.key_frames();
        frames.extend(self.deformer.iter().flat_map(|d| d.key_frames()));
        frames.extend(self.warp.iter().flat_map(|w| w.key_frames()));
        frames.sort_unstable();
        frames.dedup();
        frames
    }
}

/// Cuts each segment of an outline into pieces no longer than about
/// `piece`, keeping its exact shape (de Casteljau), so the pieces can bend
/// separately.
pub(crate) fn subdivide(anchors: &[Anchor], closed: bool, piece: f32) -> Vec<Anchor> {
    if anchors.len() < 2 {
        return anchors.to_vec();
    }
    let count = if closed { anchors.len() } else { anchors.len() - 1 };
    let mut out: Vec<Anchor> = vec![anchors[0]];
    for i in 0..count {
        let (a, b) = (anchors[i], anchors[(i + 1) % anchors.len()]);
        let mut curve = [a.point, a.handle_out, b.handle_in, b.point];
        let length = curve.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
        let pieces = ((length / piece).ceil() as usize).clamp(1, 64);
        for k in 0..pieces {
            // Split off the first 1/(pieces - k) of what's left.
            let t = 1.0 / (pieces - k) as f32;
            let (head, tail) = split_cubic(curve, t);
            out.last_mut().expect("starts with an anchor").handle_out = head[1];
            let end = if k + 1 == pieces { b } else { Anchor::corner(head[3]) };
            out.push(Anchor { point: head[3], handle_in: head[2], handle_out: end.handle_out });
            curve = tail;
        }
    }
    if closed {
        // The last piece ended back on the first anchor; merge the two.
        let last = out.pop().expect("at least one piece");
        out[0].handle_in = last.handle_in;
    }
    out
}

fn split_cubic(c: [Point; 4], t: f32) -> ([Point; 4], [Point; 4]) {
    let ab = c[0].lerp(c[1], t);
    let bc = c[1].lerp(c[2], t);
    let cd = c[2].lerp(c[3], t);
    let abc = ab.lerp(bc, t);
    let bcd = bc.lerp(cd, t);
    let mid = abc.lerp(bcd, t);
    ([c[0], ab, abc, mid], [mid, bcd, cd, c[3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Style};

    /// A 200×40 bar along the x axis, centred on the origin, with a
    /// two-bone deformer from (−100, 0) to (100, 0).
    fn bar() -> (Vec<Element>, Deformer) {
        let element = Element {
            shape: Shape::Rect { width: 200.0, height: 40.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        let bounds = Rect::from_center_size(Point::ZERO, 200.0, 40.0);
        (vec![element], Deformer::along(bounds, 2, 0))
    }

    fn points(elements: &[Element]) -> Vec<Point> {
        elements[0].shape.to_anchors().0.iter().map(|a| a.point).collect()
    }

    #[test]
    fn a_new_deformer_runs_along_the_long_side() {
        let (_, deformer) = bar();
        assert_eq!(deformer.joints, [Point::new(-100.0, 0.0), Point::ZERO, Point::new(100.0, 0.0)]);
        let tall = Deformer::along(Rect::from_center_size(Point::ZERO, 10.0, 90.0), 3, 0);
        assert_eq!(tall.joints[0], Point::new(0.0, -45.0));
        assert_eq!(tall.joints.len(), 4);
    }

    #[test]
    fn at_rest_the_drawing_is_unchanged() {
        let (elements, deformer) = bar();
        assert_eq!(deformer.deform(&elements, 0), elements);
    }

    #[test]
    fn bending_the_second_bone_swings_the_far_end() {
        let (elements, mut deformer) = bar();
        deformer.bends[1].set(0, 90.0);
        assert!(deformer.posed_joints(0)[2].distance(Point::new(0.0, 100.0)) < 1e-3);

        let bent = deformer.deform(&elements, 0);
        let points = points(&bent);
        // The far end's corners went from x 100 to around y 100.
        assert!(points.iter().any(|p| p.distance(Point::new(-20.0, 100.0)) < 1.0), "{points:?}");
        assert!(points.iter().any(|p| p.distance(Point::new(20.0, 100.0)) < 1.0));
        // The near end stayed where it was.
        assert!(points.iter().any(|p| p.distance(Point::new(-100.0, -20.0)) < 1e-3));
    }

    #[test]
    fn the_bend_is_smooth_through_the_joint() {
        let (elements, mut deformer) = bar();
        deformer.bends[1].set(0, 90.0);
        let bent = deformer.deform(&elements, 0);
        // The bar's top edge, which ran straight through the joint, now
        // has points spread around the corner rather than one sharp turn.
        let corner_points = points(&bent)
            .into_iter()
            .filter(|p| p.x > -10.0 && p.x < 40.0 && p.y > -40.0 && p.y < 10.0)
            .count();
        assert!(corner_points >= 3, "{corner_points} points round the bend");
    }

    #[test]
    fn pointing_a_bone_keys_its_bend() {
        let (_, mut deformer) = bar();
        deformer.point_bone_at(1, Point::new(0.0, -50.0), 10);
        let bends = deformer.bends_at(10);
        assert!((bends[1] - -90.0).abs() < 1e-3, "{bends:?}");
        assert_eq!(deformer.bends_at(0), [0.0, 0.0], "frame 0 keeps its key");
        assert_eq!(deformer.key_frames(), [0, 10]);
    }

    #[test]
    fn turning_the_first_bone_carries_the_second() {
        let (_, mut deformer) = bar();
        deformer.bends[0].set(0, 90.0);
        let joints = deformer.posed_joints(0);
        assert!(joints[1].distance(Point::new(-100.0, 100.0)) < 1e-3, "{joints:?}");
        assert!(joints[2].distance(Point::new(-100.0, 200.0)) < 1e-3);
    }

    #[test]
    fn subdividing_keeps_the_shape() {
        let (elements, _) = bar();
        let (anchors, closed) = elements[0].shape.to_anchors();
        let pieces = subdivide(&anchors, closed, 25.0);
        assert!(pieces.len() > anchors.len() * 3);
        // Every new point lies on the rectangle's outline.
        for a in &pieces {
            let on_edge = (a.point.x.abs() - 100.0).abs() < 1e-3 || (a.point.y.abs() - 20.0).abs() < 1e-3;
            assert!(on_edge, "{:?} is off the outline", a.point);
        }
    }
}
