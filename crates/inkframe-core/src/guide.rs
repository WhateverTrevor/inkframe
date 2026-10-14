//! Motion guides: layers that travel along a drawn path.
//!
//! This is Animate's classic motion guide. Any layer can be marked as a
//! guide; its outline (a pencil stroke, a pen path, even an oval) becomes a
//! track that other layers can follow. A guide is shown while editing but
//! never exported.
//!
//! A following layer keeps its own position keys, which say *when* it
//! travels:
//!
//! * its first position key is the start of the path, and its last key the
//!   end, so it can run a whole loop even though a loop starts and ends in
//!   the same place;
//! * any keys in between are pinned to the nearest point on the path, so
//!   dragging the layer on a middle key slides it along the path;
//! * between keys it moves along the path by distance, with the tween's
//!   easing deciding how fast, exactly as an ordinary tween would.
//!
//! Its other properties (rotation, scale, opacity) animate as usual, and
//! "Orient to path" follows the guide's curves.

use crate::document::{Document, Layer, LayerId};
use crate::geometry::Point;
use crate::shape::Shape;
use crate::track::heading_along;
use crate::transform::Transform;
use crate::{Easing, path_from_anchors};

/// A guide's path as a chain of short straight pieces, measured so points
/// can be found by distance along it.
pub struct GuidePath {
    points: Vec<Point>,
    /// The distance along the path to each point; the last is its length.
    distances: Vec<f32>,
}

impl GuidePath {
    fn new(points: Vec<Point>) -> Option<Self> {
        let mut distances = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, point) in points.iter().enumerate() {
            if i > 0 {
                total += points[i - 1].distance(*point);
            }
            distances.push(total);
        }
        (points.len() >= 2 && total > 0.0).then_some(Self { points, distances })
    }

    fn length(&self) -> f32 {
        *self.distances.last().expect("at least two points")
    }

    /// The point a fraction `t` of the way along (0 = start, 1 = end).
    pub fn at(&self, t: f32) -> Point {
        let target = t.clamp(0.0, 1.0) * self.length();
        let i = self.distances.partition_point(|d| *d < target).clamp(1, self.points.len() - 1);
        let (from, to) = (self.distances[i - 1], self.distances[i]);
        let within = if to > from { (target - from) / (to - from) } else { 0.0 };
        self.points[i - 1].lerp(self.points[i], within)
    }

    /// How far along (0–1) the point on the path nearest `point` is.
    pub fn nearest(&self, point: Point) -> f32 {
        let mut best = (f32::MAX, 0.0);
        for i in 1..self.points.len() {
            let (a, b) = (self.points[i - 1], self.points[i]);
            let span = b - a;
            let length_sq = span.dot(span);
            let along =
                if length_sq > 0.0 { ((point - a).dot(span) / length_sq).clamp(0.0, 1.0) } else { 0.0 };
            let distance = point.distance(a.lerp(b, along));
            if distance < best.0 {
                let travelled = self.distances[i - 1] + (self.distances[i] - self.distances[i - 1]) * along;
                best = (distance, travelled / self.length());
            }
        }
        best.1
    }
}

impl Document {
    /// A layer's transform at `frame`, following its motion guide if it has
    /// one. Use this rather than [`Layer::transform_at`] wherever the layer
    /// is drawn or edited.
    pub fn pose(&self, layer: &Layer, frame: u32) -> Transform {
        self.pose_at(layer, frame as f32)
    }

    fn pose_at(&self, layer: &Layer, frame: f32) -> Transform {
        let own = layer.track.sample(frame);
        let Some(path) = layer.follow.and_then(|guide| self.guide_path(guide, frame)) else {
            return own;
        };
        let keys = layer.track.position.keys();
        let position = |frame: f32| Some(path.at(guide_progress(layer, &path, frame)));
        let mut rotation = layer.track.rotation.sample(frame).unwrap_or_default();
        if layer.track.orient_to_path {
            let (first, last) = (keys[0].frame, keys[keys.len() - 1].frame);
            rotation += heading_along(first, last, frame, position).unwrap_or(0.0);
        }
        Transform { position: position(frame).unwrap_or(own.position), rotation, ..own }
    }

    /// The path of the guide layer `guide` at `frame`, in its timeline's
    /// coordinates: its first shape with an outline, through the guide
    /// layer's own transform. `None` if it isn't a guide or has no outline.
    pub fn guide_path(&self, guide: LayerId, frame: f32) -> Option<GuidePath> {
        let layer = self.layer(guide).filter(|l| l.guide)?;
        let to_timeline = layer.track.sample(frame).to_affine();
        let outline = layer.elements.iter().find_map(|element| match &element.shape {
            Shape::Path(path) => Some(path.flatten()),
            shape @ (Shape::Rect { .. } | Shape::Ellipse { .. }) => {
                let (anchors, closed) = shape.to_anchors();
                path_from_anchors(&anchors, closed).map(|path| path.flatten())
            }
            Shape::Image { .. } | Shape::Text(_) => None,
        })?;
        GuidePath::new(outline.into_iter().map(|p| to_timeline.apply(p)).collect())
    }

    /// Makes a layer have `pose` at `frame`, as [`Track::set_pose`] does,
    /// taking its motion guide into account.
    ///
    /// A following layer is shown on its guide, not at its own position
    /// keys. So a pose whose position is just where it's shown (because
    /// only its rotation, say, was edited) leaves the position keys alone,
    /// and one where it was moved keys the new spot, which then pins to the
    /// nearest point on the guide. Its rotation is shown with the guide's
    /// heading added, which is taken back off before keying.
    ///
    /// [`Track::set_pose`]: crate::Track::set_pose
    pub fn set_pose(&mut self, id: LayerId, frame: u32, mut pose: Transform) {
        let Some(layer) = self.layer(id) else { return };
        if layer.follow.is_some() {
            let shown = self.pose(layer, frame);
            let own = layer.track.sample(frame as f32);
            if pose.position == shown.position {
                pose.position = own.position;
            }
            // Track::set_pose removes the layer's own heading; swap in the
            // guide's instead.
            pose.rotation += own.rotation - shown.rotation;
        }
        if let Some(layer) = self.layer_mut(id) {
            layer.track.set_pose(frame, pose);
        }
    }

    /// The guide layers a layer in the same timeline could follow.
    pub fn guides_beside(&self, layer: LayerId) -> Vec<&Layer> {
        let Some(timeline) = self.timeline_of(layer) else { return Vec::new() };
        self.layers_of(timeline).iter().filter(|l| l.guide && l.id != layer).collect()
    }
}

/// How far along the guide (0–1) a following layer is at `frame`.
fn guide_progress(layer: &Layer, path: &GuidePath, frame: f32) -> f32 {
    let keys = layer.track.position.keys();
    let last = keys.len() - 1;
    // Where along the path each key puts the layer.
    let pinned = |i: usize| match i {
        0 if last > 0 => 0.0,
        i if i == last && last > 0 => 1.0,
        i => path.nearest(keys[i].value),
    };
    if frame <= keys[0].frame as f32 {
        return pinned(0);
    }
    if frame >= keys[last].frame as f32 {
        return pinned(last);
    }
    let next = keys.partition_point(|k| k.frame as f32 <= frame);
    let (a, b) = (&keys[next - 1], &keys[next]);
    let t = (frame - a.frame as f32) / (b.frame - a.frame) as f32;
    let eased = if a.easing == Easing::Hold { 0.0 } else { a.easing.apply(t) };
    let (from, to) = (pinned(next - 1), pinned(next));
    from + (to - from) * eased
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Segment, Style};

    /// A guide running right 300 px then down 100 px, and a layer
    /// following it from frame 0 to frame 40.
    fn guided() -> (Document, LayerId) {
        let mut doc = Document::default();
        let corner = crate::Path {
            start: Point::ZERO,
            segments: vec![
                Segment::Line { to: Point::new(300.0, 0.0) },
                Segment::Line { to: Point::new(300.0, 100.0) },
            ],
            closed: false,
        };
        let stroke = Style { fill: None, stroke: Some(crate::Stroke { color: Color::BLACK, width: 2.0 }) };
        let guide = doc.add_layer(
            "Guide",
            vec![Element { shape: Shape::Path(corner), style: stroke }],
            0,
            Transform::default(),
        );
        doc.layer_mut(guide).unwrap().guide = true;
        // The follower's own keys are nowhere near the path; only their
        // timing matters.
        let car = doc.add_layer("Car", vec![], 0, Transform::at(Point::new(-50.0, 500.0)));
        let layer = doc.layer_mut(car).unwrap();
        layer.track.position.set(40, Point::new(900.0, 900.0));
        layer.follow = Some(guide);
        (doc, car)
    }

    fn position(doc: &Document, layer: LayerId, frame: u32) -> Point {
        doc.pose(doc.layer(layer).unwrap(), frame).position
    }

    #[test]
    fn followers_run_the_whole_guide_by_distance() {
        let (doc, car) = guided();
        assert_eq!(position(&doc, car, 0), Point::ZERO);
        assert_eq!(position(&doc, car, 40), Point::new(300.0, 100.0));
        // 400 px of path over 40 frames: 10 px a frame, round the corner.
        assert!(position(&doc, car, 15).distance(Point::new(150.0, 0.0)) < 1e-3);
        assert!(position(&doc, car, 35).distance(Point::new(300.0, 50.0)) < 1e-3);
    }

    #[test]
    fn middle_keys_pin_to_the_nearest_point_on_the_guide() {
        let (mut doc, car) = guided();
        // A key on frame 10 placed near the corner: a quarter of the time
        // gets three quarters of the way along.
        doc.layer_mut(car).unwrap().track.position.set(10, Point::new(310.0, -5.0));
        assert!(position(&doc, car, 10).distance(Point::new(300.0, 0.0)) < 1e-3);
    }

    #[test]
    fn oriented_followers_turn_with_the_guide() {
        let (mut doc, car) = guided();
        doc.layer_mut(car).unwrap().track.orient_to_path = true;
        let rotation = |frame| doc.pose(doc.layer(car).unwrap(), frame).rotation;
        assert!(rotation(10).abs() < 1e-3, "heading right");
        assert!((rotation(38) - 90.0).abs() < 1e-3, "heading down");
    }

    #[test]
    fn editing_a_followers_rotation_leaves_its_path_alone() {
        let (mut doc, car) = guided();
        doc.layer_mut(car).unwrap().track.orient_to_path = true;
        let shown = doc.pose(doc.layer(car).unwrap(), 10);
        doc.set_pose(car, 10, Transform { rotation: shown.rotation + 15.0, ..shown });

        let track = &doc.layer(car).unwrap().track;
        assert!(track.position.key_at(10).is_none(), "no position key from just turning it");
        assert_eq!(track.rotation.key_at(10).unwrap().value, 15.0);
        assert!((doc.pose(doc.layer(car).unwrap(), 10).rotation - 15.0).abs() < 1e-3);
    }

    #[test]
    fn layers_without_a_usable_guide_move_as_usual() {
        let (mut doc, car) = guided();
        let guide = doc.layer(car).unwrap().follow.unwrap();
        doc.layer_mut(guide).unwrap().guide = false;
        assert_eq!(position(&doc, car, 0), Point::new(-50.0, 500.0));
    }
}
