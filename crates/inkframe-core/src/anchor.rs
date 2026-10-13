//! Anchor points: the pen tool's view of a Bézier path.
//!
//! A stored [`Path`] is a list of segments, which suits rendering. People
//! drawing with a pen tool think in *anchors* instead: points the curve
//! passes through, each with up to two handles controlling the curve's
//! direction on the way in and the way out.
//!
//! ```text
//!        handle_in ●───────◆───────● handle_out
//!                        point
//! ```
//!
//! Converting anchors to segments is direct: the curve from anchor `a` to
//! anchor `b` is a cubic Bézier with control points `a.handle_out` and
//! `b.handle_in`. If both of those sit on their anchors, the segment is a
//! straight line, and we store it as one.
//!
//! The conversion also runs the other way ([`anchors_from_path`] and
//! [`Shape::to_anchors`]), which is how the Subselection tool edits shapes.

use crate::geometry::{Point, Rect};
use crate::shape::{Path, Segment, Shape};

/// How far along each side a quarter-circle's Bézier handles reach, as a
/// fraction of the radius. 4/3·(√2 − 1) ≈ 0.5523 is the standard constant
/// that makes a cubic Bézier hug a circular arc to within 0.03%.
pub const KAPPA: f32 = 0.552_284_8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub point: Point,
    /// Controls the curve arriving at this point. Equal to `point` for a corner.
    pub handle_in: Point,
    /// Controls the curve leaving this point. Equal to `point` for a corner.
    pub handle_out: Point,
}

impl Anchor {
    /// A sharp point with no handles. Segments touching it start or end straight.
    pub fn corner(point: Point) -> Self {
        Self { point, handle_in: point, handle_out: point }
    }

    /// A smooth point: the incoming handle mirrors `handle_out` through the
    /// point, so the curve passes through without a kink.
    pub fn smooth(point: Point, handle_out: Point) -> Self {
        Self { point, handle_in: point * 2.0 - handle_out, handle_out }
    }
}

/// Splits a path back into anchors, the inverse of [`path_from_anchors`].
///
/// Each segment contributes its end point as an anchor. A cubic segment's
/// first control point becomes the outgoing handle of the anchor it leaves,
/// and its second becomes the incoming handle of the anchor it arrives at.
///
/// Closed paths need care at the seam. Paths made by [`path_from_anchors`]
/// end with an explicit segment back to the start, and that segment's end
/// point is the first anchor again, not a new one. We fold its handle into
/// the first anchor instead of duplicating it.
pub fn anchors_from_path(path: &Path) -> Vec<Anchor> {
    let mut anchors = vec![Anchor::corner(path.start)];
    for segment in &path.segments {
        let last = anchors.last_mut().expect("starts with one anchor");
        let next = match *segment {
            Segment::Line { to } => Anchor::corner(to),
            Segment::Cubic { ctrl1, ctrl2, to } => {
                last.handle_out = ctrl1;
                Anchor { point: to, handle_in: ctrl2, handle_out: to }
            }
        };
        anchors.push(next);
    }

    let returns_to_start = anchors.len() > 2 && anchors.last().is_some_and(|a| a.point == path.start);
    if path.closed && returns_to_start {
        let seam = anchors.pop().expect("checked above");
        anchors[0].handle_in = seam.handle_in;
    }
    anchors
}

impl Shape {
    /// This shape as anchors in local coordinates, plus whether it's closed.
    ///
    /// Rectangles and ellipses come out as the same Bézier outline the
    /// renderer draws, so converting one to a path for editing doesn't
    /// change how it looks.
    pub fn to_anchors(&self) -> (Vec<Anchor>, bool) {
        match self {
            Shape::Path(path) => (anchors_from_path(path), path.closed),
            Shape::Rect { width, height, corner_radius } => {
                let bounds = Rect::from_center_size(Point::ZERO, *width, *height);
                let radius = corner_radius.clamp(0.0, width.min(*height) / 2.0);
                (rect_anchors(bounds, radius), true)
            }
            Shape::Ellipse { radius_x, radius_y } => (ellipse_anchors(*radius_x, *radius_y), true),
        }
    }
}

/// A rectangle's outline, clockwise from the top-left. Rounded corners
/// become two anchors each, joined by a quarter-circle curve.
fn rect_anchors(b: Rect, r: f32) -> Vec<Anchor> {
    let (l, t, rt, bm) = (b.min.x, b.min.y, b.max.x, b.max.y);
    if r <= 0.0 {
        return b.corners().map(Anchor::corner).to_vec();
    }
    let k = r * KAPPA;
    let p = Point::new;
    // Each tuple is (point, handle_in, handle_out). Straight sides have no
    // handles; the curve at each corner pulls toward the corner by `k`.
    [
        (p(l + r, t), p(l + r - k, t), p(l + r, t)),
        (p(rt - r, t), p(rt - r, t), p(rt - r + k, t)),
        (p(rt, t + r), p(rt, t + r - k), p(rt, t + r)),
        (p(rt, bm - r), p(rt, bm - r), p(rt, bm - r + k)),
        (p(rt - r, bm), p(rt - r + k, bm), p(rt - r, bm)),
        (p(l + r, bm), p(l + r, bm), p(l + r - k, bm)),
        (p(l, bm - r), p(l, bm - r + k), p(l, bm - r)),
        (p(l, t + r), p(l, t + r), p(l, t + r - k)),
    ]
    .map(|(point, handle_in, handle_out)| Anchor { point, handle_in, handle_out })
    .to_vec()
}

/// An ellipse as four smooth anchors (top, right, bottom, left), clockwise.
fn ellipse_anchors(rx: f32, ry: f32) -> Vec<Anchor> {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let p = Point::new;
    vec![
        Anchor::smooth(p(0.0, -ry), p(kx, -ry)),
        Anchor::smooth(p(rx, 0.0), p(rx, ky)),
        Anchor::smooth(p(0.0, ry), p(-kx, ry)),
        Anchor::smooth(p(-rx, 0.0), p(-rx, -ky)),
    ]
}

/// Builds a path through `anchors`. A closed path gets an extra segment from
/// the last anchor back to the first.
///
/// Returns `None` with fewer than two anchors, since that isn't a line yet.
pub fn path_from_anchors(anchors: &[Anchor], closed: bool) -> Option<Path> {
    if anchors.len() < 2 {
        return None;
    }
    let mut segments: Vec<Segment> = anchors.windows(2).map(|w| segment_between(&w[0], &w[1])).collect();
    if closed {
        segments.push(segment_between(&anchors[anchors.len() - 1], &anchors[0]));
    }
    Some(Path { start: anchors[0].point, segments, closed })
}

fn segment_between(from: &Anchor, to: &Anchor) -> Segment {
    if from.handle_out == from.point && to.handle_in == to.point {
        Segment::Line { to: to.point }
    } else {
        Segment::Cubic { ctrl1: from.handle_out, ctrl2: to.handle_in, to: to.point }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn corners_make_straight_lines() {
        let path =
            path_from_anchors(&[Anchor::corner(p(0.0, 0.0)), Anchor::corner(p(10.0, 0.0))], false).unwrap();
        assert_eq!(path.segments, vec![Segment::Line { to: p(10.0, 0.0) }]);
    }

    #[test]
    fn smooth_anchors_mirror_their_handles() {
        let anchor = Anchor::smooth(p(10.0, 10.0), p(15.0, 10.0));
        assert_eq!(anchor.handle_in, p(5.0, 10.0));
    }

    #[test]
    fn handles_become_control_points() {
        let a = Anchor::smooth(p(0.0, 0.0), p(5.0, -5.0));
        let b = Anchor::corner(p(20.0, 0.0));
        let path = path_from_anchors(&[a, b], false).unwrap();
        assert_eq!(
            path.segments,
            vec![Segment::Cubic { ctrl1: p(5.0, -5.0), ctrl2: p(20.0, 0.0), to: p(20.0, 0.0) }]
        );
    }

    #[test]
    fn closing_adds_a_segment_back_to_the_start() {
        let anchors = [p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0)].map(Anchor::corner);
        let path = path_from_anchors(&anchors, true).unwrap();
        assert!(path.closed);
        assert_eq!(path.segments.len(), 3);
        assert_eq!(path.segments[2].end(), p(0.0, 0.0));
    }

    #[test]
    fn open_paths_round_trip() {
        let anchors = vec![
            Anchor::corner(p(0.0, 0.0)),
            Anchor::smooth(p(50.0, 20.0), p(70.0, 0.0)),
            Anchor::corner(p(100.0, 50.0)),
        ];
        let path = path_from_anchors(&anchors, false).unwrap();
        assert_eq!(anchors_from_path(&path), anchors);
    }

    #[test]
    fn closed_paths_round_trip_without_a_duplicate_seam() {
        let anchors = vec![
            Anchor::smooth(p(0.0, 0.0), p(10.0, -10.0)),
            Anchor::corner(p(100.0, 0.0)),
            Anchor::corner(p(50.0, 80.0)),
        ];
        let path = path_from_anchors(&anchors, true).unwrap();
        assert_eq!(anchors_from_path(&path), anchors);
    }

    #[test]
    fn ellipse_anchors_sit_on_the_ellipse() {
        let (anchors, closed) = Shape::Ellipse { radius_x: 40.0, radius_y: 20.0 }.to_anchors();
        assert!(closed);
        let points: Vec<Point> = anchors.iter().map(|a| a.point).collect();
        assert_eq!(points, [p(0.0, -20.0), p(40.0, 0.0), p(0.0, 20.0), p(-40.0, 0.0)]);
    }

    #[test]
    fn rect_converts_to_its_corners() {
        let (anchors, _) = Shape::Rect { width: 20.0, height: 10.0, corner_radius: 0.0 }.to_anchors();
        assert_eq!(anchors.len(), 4);
        assert_eq!(anchors[2].point, p(10.0, 5.0));
    }

    #[test]
    fn rounded_rect_keeps_its_outline() {
        // The converted path should cover the same area as the original:
        // compare bounds of the flattened outline.
        let shape = Shape::Rect { width: 100.0, height: 60.0, corner_radius: 10.0 };
        let (anchors, closed) = shape.to_anchors();
        assert_eq!(anchors.len(), 8);
        let path = path_from_anchors(&anchors, closed).unwrap();
        let bounds = Rect::bounding(path.flatten()).unwrap();
        assert!((bounds.width() - 100.0).abs() < 1e-3 && (bounds.height() - 60.0).abs() < 1e-3);
    }

    #[test]
    fn one_anchor_is_not_a_path() {
        assert!(path_from_anchors(&[Anchor::corner(p(1.0, 1.0))], true).is_none());
    }
}
