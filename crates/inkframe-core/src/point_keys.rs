//! Point animation: keyframing the points of a shape one by one, as Moho
//! and After Effects' path keyframes do.
//!
//! A drawing holds its shapes as drawn. Each point that's been animated
//! gets a [`PointTrack`]: how far it (and its two handles) sits from where
//! it's drawn, keyed over time, with its own keys and easing like any other
//! property. So an eyebrow can raise while the rest of the face holds, and
//! a shape's outline can wobble point by point, all in one drawing.
//!
//! Offsets, rather than places, are what's kept, so redrawing the shape
//! moves its animation with it: a point keyed 10 to the right stays 10 to
//! the right of wherever it's drawn now.
//!
//! A track names its point by the shape's place in the drawing's line art
//! and the point's place in the shape's outline. The drawing keeps the
//! shape's place up to date as shapes come and go (see
//! [`Drawing::shape_removed`]), as it does for shapes bound to bones;
//! adding or deleting points in an animated shape renumbers the points
//! after them, and their tracks with them.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, path_from_anchors};
use crate::channel::{Animatable, Channel};
use crate::document::{Element, Layer};
use crate::drawing::Drawing;
use crate::geometry::Point;
use crate::shape::Shape;

/// How far a point and its handles are from where they're drawn.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PointOffset {
    pub point: Point,
    #[serde(default)]
    pub handle_in: Point,
    #[serde(default)]
    pub handle_out: Point,
}

impl PointOffset {
    /// The offset that takes `drawn` to `posed`.
    pub fn between(drawn: &Anchor, posed: &Anchor) -> Self {
        Self {
            point: posed.point - drawn.point,
            handle_in: posed.handle_in - drawn.handle_in,
            handle_out: posed.handle_out - drawn.handle_out,
        }
    }

    /// `anchor` moved by this offset.
    pub fn apply(self, anchor: &Anchor) -> Anchor {
        Anchor {
            point: anchor.point + self.point,
            handle_in: anchor.handle_in + self.handle_in,
            handle_out: anchor.handle_out + self.handle_out,
        }
    }

    /// Whether this is so close to `other` that nobody could see the
    /// difference: a fraction of a pixel.
    fn same_as(self, other: PointOffset) -> bool {
        let near = |a: Point, b: Point| a.distance(b) < 1e-3;
        near(self.point, other.point)
            && near(self.handle_in, other.handle_in)
            && near(self.handle_out, other.handle_out)
    }
}

impl Animatable for PointOffset {
    fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            point: self.point.lerp(other.point, t),
            handle_in: self.handle_in.lerp(other.handle_in, t),
            handle_out: self.handle_out.lerp(other.handle_out, t),
        }
    }
}

/// One animated point: shape `element` of the drawing's line art, point
/// `anchor` of its outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointTrack {
    pub element: usize,
    pub anchor: usize,
    pub offset: Channel<PointOffset>,
}

impl Drawing {
    /// The line art as it's posed on `frame`, each animated point moved by
    /// its offset then. Shapes with no animated points are as drawn.
    pub fn posed(&self, frame: u32) -> Cow<'_, [Element]> {
        if self.point_tracks.is_empty() {
            return Cow::Borrowed(&self.elements);
        }
        let mut elements = self.elements.clone();
        for (index, element) in elements.iter_mut().enumerate() {
            let mut tracks = self.point_tracks.iter().filter(|t| t.element == index).peekable();
            if tracks.peek().is_none() {
                continue;
            }
            let (mut anchors, closed) = element.shape.to_anchors();
            let mut moved = false;
            for track in tracks {
                if let (Some(anchor), Some(offset)) =
                    (anchors.get_mut(track.anchor), track.offset.sample(frame as f32))
                {
                    *anchor = offset.apply(anchor);
                    moved = true;
                }
            }
            if moved && let Some(path) = path_from_anchors(&anchors, closed) {
                element.shape = Shape::Path(path);
            }
        }
        Cow::Owned(elements)
    }

    /// Everything the drawing shows on `frame`, bottom first: the colour
    /// art, then the line art over it, posed.
    pub fn shown_at(&self, frame: u32) -> Cow<'_, [Element]> {
        if self.point_tracks.is_empty() {
            return self.shown();
        }
        let posed = self.posed(frame);
        Cow::Owned(self.colour_art.iter().chain(posed.iter()).cloned().collect())
    }

    /// How far point `anchor` of shape `element` is moved on `frame`.
    pub fn point_offset(&self, element: usize, anchor: usize, frame: u32) -> PointOffset {
        self.point_tracks
            .iter()
            .find(|t| t.element == element && t.anchor == anchor)
            .and_then(|t| t.offset.sample(frame as f32))
            .unwrap_or_default()
    }

    /// Keys shape `element`'s points on `frame` so it's posed as `posed`
    /// (its whole outline, point for point). Only the points that move get
    /// a key; a point keyed for the first time also gets one at `rest` (the
    /// layer's first frame), unmoved, so it animates from where it's drawn
    /// rather than jumping there for every frame. Returns false, keying
    /// nothing, if `posed` doesn't have the shape's number of points.
    pub fn key_points(&mut self, frame: u32, rest: u32, element: usize, posed: &[Anchor]) -> bool {
        let Some(shape) = self.elements.get(element) else { return false };
        let (drawn, _) = shape.shape.to_anchors();
        if drawn.len() != posed.len() {
            return false;
        }
        for (index, (drawn, posed)) in drawn.iter().zip(posed).enumerate() {
            let wanted = PointOffset::between(drawn, posed);
            if wanted.same_as(self.point_offset(element, index, frame)) {
                continue;
            }
            match self.point_tracks.iter_mut().find(|t| t.element == element && t.anchor == index) {
                Some(track) => track.offset.set(frame, wanted),
                None => {
                    let mut offset = Channel::new(rest, PointOffset::default());
                    offset.set(frame, wanted);
                    self.point_tracks.push(PointTrack { element, anchor: index, offset });
                }
            }
        }
        true
    }

    /// Redraws shape `element` so that, posed on `frame`, it's `posed`:
    /// each point drawn where it has to be for its offset then to carry it
    /// there. That's editing the drawing itself (every frame changes),
    /// keeping the points' animation as it is. With no animated points
    /// it's simply drawing the shape as `posed`.
    pub fn redraw_posed(&mut self, frame: u32, element: usize, posed: &[Anchor], closed: bool) {
        let drawn: Vec<Anchor> = posed
            .iter()
            .enumerate()
            .map(|(index, anchor)| {
                let offset = self.point_offset(element, index, frame);
                Anchor {
                    point: anchor.point - offset.point,
                    handle_in: anchor.handle_in - offset.handle_in,
                    handle_out: anchor.handle_out - offset.handle_out,
                }
            })
            .collect();
        if let (Some(shape), Some(path)) = (self.elements.get_mut(element), path_from_anchors(&drawn, closed))
        {
            shape.shape = Shape::Path(path);
        }
    }

    /// Keeps shape `element`'s animated points on the same points after a
    /// new one goes in at place `at` of its outline.
    pub fn point_inserted(&mut self, element: usize, at: usize) {
        for track in self.point_tracks.iter_mut().filter(|t| t.element == element && t.anchor >= at) {
            track.anchor += 1;
        }
    }

    /// Likewise after point `index` is deleted: its animation goes too.
    pub fn point_removed(&mut self, element: usize, index: usize) {
        self.point_tracks.retain(|t| t.element != element || t.anchor != index);
        for track in self.point_tracks.iter_mut().filter(|t| t.element == element && t.anchor > index) {
            track.anchor -= 1;
        }
    }

    /// The frames point `anchor` of shape `element` is keyed on: none if
    /// it isn't animated.
    pub fn point_key_frames(&self, element: usize, anchor: usize) -> Vec<u32> {
        self.point_tracks
            .iter()
            .find(|t| t.element == element && t.anchor == anchor)
            .map_or_else(Vec::new, |t| t.offset.keys().iter().map(|k| k.frame).collect())
    }

    /// Stops animating point `anchor` of shape `element`: it's shown where
    /// it's drawn on every frame.
    pub fn clear_point_keys(&mut self, element: usize, anchor: usize) {
        self.point_tracks.retain(|t| t.element != element || t.anchor != anchor);
    }
}

impl Layer {
    /// The line art showing on `frame`, posed: what the Subselection tool
    /// shows and edits. Unlike [`Layer::shown_elements`], it's not morphed,
    /// bent by bones or warped.
    pub fn posed_elements(&self, frame: u32) -> Cow<'_, [Element]> {
        self.drawings.get(self.drawing_index_at(frame)).map_or(Cow::Borrowed(&[]), |d| d.posed(frame))
    }

    /// Keys shape `element`'s points in the drawing showing on `frame` so
    /// it's posed as `posed` then (see [`Drawing::key_points`]).
    pub fn key_points(&mut self, frame: u32, element: usize, posed: &[Anchor]) -> bool {
        let (index, rest) = (self.drawing_index_at(frame), self.start.min(frame));
        self.drawings.get_mut(index).is_some_and(|d| d.key_points(frame, rest, element, posed))
    }

    /// Keys every animated point on `frame` where it is then, as F6 does.
    pub(crate) fn insert_point_key(&mut self, frame: u32) -> bool {
        use crate::channel::AnyChannel;
        self.point_channels_mut().fold(false, |added, c| c.insert_key(frame) | added)
    }

    pub(crate) fn remove_point_key(&mut self, frame: u32) -> bool {
        use crate::channel::AnyChannel;
        self.point_channels_mut().fold(false, |removed, c| c.remove_key(frame) | removed)
    }

    /// Every animated point's keyframes, in every drawing.
    pub(crate) fn point_channels_mut(&mut self) -> impl Iterator<Item = &mut Channel<PointOffset>> {
        self.drawings.iter_mut().flat_map(|d| d.point_tracks.iter_mut().map(|t| &mut t.offset))
    }

    pub(crate) fn point_key_frames(&self) -> impl Iterator<Item = u32> + '_ {
        self.drawings
            .iter()
            .flat_map(|d| d.point_tracks.iter())
            .flat_map(|t| t.offset.keys().iter().map(|k| k.frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Document, Style, Transform};

    fn square() -> Element {
        Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }
    }

    fn layer() -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        doc.layer(id).unwrap().clone()
    }

    fn points(layer: &Layer, frame: u32) -> Vec<Point> {
        layer.shown_elements(frame)[0].shape.to_anchors().0.iter().map(|a| a.point).collect()
    }

    /// The square's points as drawn, with point `index` moved by `by`.
    fn moved(index: usize, by: Point) -> Vec<Anchor> {
        let (mut anchors, _) = square().shape.to_anchors();
        anchors[index] = PointOffset { point: by, handle_in: by, handle_out: by }.apply(&anchors[index]);
        anchors
    }

    #[test]
    fn a_keyed_point_animates_from_where_its_drawn() {
        let mut layer = layer();
        // The top-right corner pulled out 40 to the right on frame 10.
        assert!(layer.key_points(10, 0, &moved(1, Point::new(40.0, 0.0))));
        assert_eq!(points(&layer, 0)[1], Point::new(50.0, -50.0), "as drawn at the start");
        assert_eq!(points(&layer, 5)[1], Point::new(70.0, -50.0), "halfway");
        assert_eq!(points(&layer, 10)[1], Point::new(90.0, -50.0));
        assert_eq!(points(&layer, 20)[1], Point::new(90.0, -50.0), "and holds");
        // The other corners never move.
        for frame in [0, 5, 10] {
            assert_eq!(points(&layer, frame)[0], Point::new(-50.0, -50.0));
        }
        // The drawing itself is untouched: still a rectangle.
        assert!(matches!(layer.drawings[0].elements[0].shape, Shape::Rect { .. }));
        assert_eq!(layer.drawings[0].point_tracks.len(), 1, "only the moved point is animated");
    }

    #[test]
    fn each_point_has_its_own_keys() {
        let mut layer = layer();
        layer.key_points(10, 0, &moved(1, Point::new(40.0, 0.0)));
        // Another point keyed later, from frame 10's pose.
        let mut pose: Vec<Anchor> = layer.posed_elements(20)[0].shape.to_anchors().0;
        pose[3] = PointOffset { point: Point::new(0.0, 30.0), ..Default::default() }.apply(&pose[3]);
        layer.key_points(20, 0, &pose);
        assert_eq!(layer.key_frames(), [0, 10, 20]);
        // The first point's still done moving by frame 10; the second only
        // starts from its rest key at 0, so it's partway at 10.
        assert_eq!(points(&layer, 10)[1], Point::new(90.0, -50.0));
        assert_eq!(points(&layer, 10)[3], Point::new(-50.0, 65.0));
    }

    #[test]
    fn redrawing_keeps_the_animation_on_top() {
        let mut layer = layer();
        layer.key_points(10, 0, &moved(1, Point::new(40.0, 0.0)));
        // Redraw the whole square 10 lower, on frame 10, where it's posed.
        let mut pose: Vec<Anchor> = layer.posed_elements(10)[0].shape.to_anchors().0;
        let down = PointOffset {
            point: Point::new(0.0, 10.0),
            handle_in: Point::new(0.0, 10.0),
            handle_out: Point::new(0.0, 10.0),
        };
        pose.iter_mut().for_each(|a| *a = down.apply(a));
        layer.drawings[0].redraw_posed(10, 0, &pose, true);
        assert_eq!(points(&layer, 10)[1], Point::new(90.0, -40.0), "posed where it was put");
        assert_eq!(points(&layer, 0)[1], Point::new(50.0, -40.0), "and drawn lower on every frame");
    }

    #[test]
    fn point_keys_move_with_the_layers_other_keys() {
        let mut layer = layer();
        layer.key_points(10, 0, &moved(1, Point::new(40.0, 0.0)));
        assert!(layer.move_key(10, 14));
        assert_eq!(points(&layer, 14)[1], Point::new(90.0, -50.0));
        layer.shift_time(2);
        assert_eq!(layer.key_frames(), [2, 16]);
        assert!(layer.remove_key(16));
        assert_eq!(points(&layer, 16)[1], Point::new(50.0, -50.0), "back as drawn");
    }

    #[test]
    fn removing_a_shape_keeps_the_others_points_animated() {
        let mut layer = layer();
        layer.drawings[0].elements.insert(0, square());
        layer.key_points(10, 1, &moved(1, Point::new(40.0, 0.0)));
        layer.drawings[0].elements.remove(0);
        layer.drawings[0].shape_removed(false, 0);
        assert_eq!(points(&layer, 10)[1], Point::new(90.0, -50.0));
    }

    #[test]
    fn point_tracks_are_saved_only_when_there_are_some() {
        let layer = layer();
        let json = serde_json::to_string(&layer.drawings[0]).unwrap();
        assert!(!json.contains("point_tracks"));
        let mut animated = layer.clone();
        animated.key_points(10, 0, &moved(1, Point::new(40.0, 0.0)));
        let json = serde_json::to_string(&animated.drawings[0]).unwrap();
        let back: Drawing = serde_json::from_str(&json).unwrap();
        assert_eq!(back, animated.drawings[0]);
    }
}
