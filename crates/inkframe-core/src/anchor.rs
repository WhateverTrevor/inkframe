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

use crate::geometry::Point;
use crate::shape::{Path, Segment};

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
    fn one_anchor_is_not_a_path() {
        assert!(path_from_anchors(&[Anchor::corner(p(1.0, 1.0))], true).is_none());
    }
}
