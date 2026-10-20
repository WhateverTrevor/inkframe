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
use crate::shape::{Path, Segment, Shape, cubic_point};

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
    /// change how it looks. Images and text have no anchors: their outlines
    /// come from the picture or the font, which can't be bent point by point.
    pub fn to_anchors(&self) -> (Vec<Anchor>, bool) {
        match self {
            Shape::Path(path) => (anchors_from_path(path), path.closed),
            // The outline it sweeps out: what bends, morphs and bounds a fill.
            Shape::Line { path, widths } => {
                (anchors_from_path(&crate::line::line_outline(path, widths)), true)
            }
            Shape::Rect { width, height, corner_radius } => {
                let bounds = Rect::from_center_size(Point::ZERO, *width, *height);
                let radius = corner_radius.clamp(0.0, width.min(*height) / 2.0);
                (rect_anchors(bounds, radius), true)
            }
            Shape::Ellipse { radius_x, radius_y } => (ellipse_anchors(*radius_x, *radius_y), true),
            Shape::Image { .. } | Shape::Text(_) | Shape::Paint { .. } => (Vec::new(), false),
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

/// The closest point on a path to `target`, for adding anchors where the
/// user clicks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathHit {
    /// Which segment: segment `i` runs from anchor `i` to anchor `i + 1`
    /// (and, in a closed path, the last one runs back to anchor 0).
    pub segment: usize,
    /// How far along the segment, from 0 (its start) to 1 (its end).
    pub t: f32,
    pub point: Point,
    pub distance: f32,
}

/// Finds the point on the path through `anchors` nearest to `target`.
///
/// There's no closed-form answer for a cubic Bézier, so this works in two
/// passes: sample each segment coarsely to find roughly where the closest
/// point is, then home in on it by repeatedly narrowing the interval around
/// the best sample. Accurate to far below a pixel, and cheap enough to run
/// on every mouse move.
pub fn nearest_on_anchors(anchors: &[Anchor], closed: bool, target: Point) -> Option<PathHit> {
    const SAMPLES: usize = 32;
    const REFINE_STEPS: usize = 12;

    let mut best: Option<PathHit> = None;
    for (segment, (a, b)) in segment_pairs(anchors, closed).enumerate() {
        let at = |t: f32| point_on_segment(a, b, t);
        let distance_at = |t: f32| at(t).distance(target);

        let mut t = (0..=SAMPLES)
            .map(|i| i as f32 / SAMPLES as f32)
            .min_by(|x, y| distance_at(*x).total_cmp(&distance_at(*y)))
            .expect("samples are never empty");

        // Each step tries a nudge either side of the current best and keeps
        // whichever is closer, halving the nudge every time.
        let mut step = 1.0 / SAMPLES as f32;
        for _ in 0..REFINE_STEPS {
            step /= 2.0;
            let candidates = [t, (t - step).max(0.0), (t + step).min(1.0)];
            t = candidates.into_iter().min_by(|x, y| distance_at(*x).total_cmp(&distance_at(*y))).unwrap();
        }

        let hit = PathHit { segment, t, point: at(t), distance: distance_at(t) };
        if best.is_none_or(|b| hit.distance < b.distance) {
            best = Some(hit);
        }
    }
    best
}

/// Inserts a new anchor `t` of the way along `segment`, without changing the
/// shape of the path.
///
/// Curves are split with de Casteljau's construction. A cubic's four control
/// points are blended pairwise at `t`, then the results blended again, then
/// once more; the final point lies on the curve, and the in-between points
/// are exactly the handles that make the two halves trace the original
/// curve:
///
/// ```text
///  p0 ──── p01 ──── p1 ──── p12 ──── p2 ──── p23 ──── p3
///            ╲              │               ╱
///             p012 ──────── new ──────── p123
/// ```
///
/// Straight segments get a corner anchor instead, so lines stay lines.
pub fn insert_anchor(anchors: &[Anchor], closed: bool, segment: usize, t: f32) -> Vec<Anchor> {
    let mut anchors = anchors.to_vec();
    let start = segment;
    let end = (segment + 1) % anchors.len();
    assert!(closed || segment + 1 < anchors.len(), "segment {segment} is past the end of an open path");

    let (a, b) = (anchors[start], anchors[end]);
    let new_anchor = if is_straight(&a, &b) {
        Anchor::corner(point_on_segment(&a, &b, t))
    } else {
        let (p0, p1, p2, p3) = (a.point, a.handle_out, b.handle_in, b.point);
        let (p01, p12, p23) = (p0.lerp(p1, t), p1.lerp(p2, t), p2.lerp(p3, t));
        let (p012, p123) = (p01.lerp(p12, t), p12.lerp(p23, t));
        anchors[start].handle_out = p01;
        anchors[end].handle_in = p23;
        Anchor { point: p012.lerp(p123, t), handle_in: p012, handle_out: p123 }
    };
    anchors.insert(segment + 1, new_anchor);
    anchors
}

/// A segment is straight when neither end has a handle sticking out.
fn is_straight(from: &Anchor, to: &Anchor) -> bool {
    from.handle_out == from.point && to.handle_in == to.point
}

/// The point `t` of the way along the segment from `from` to `to`.
///
/// Straight segments use plain linear interpolation. Evaluating them as a
/// cubic would give the right line but the wrong *speed*: with both handles
/// sitting on the endpoints the curve eases in and out, so `t = 0.3` would
/// land only about 22% of the way along. Using one formula for both the
/// nearest-point search and insertion keeps a new point exactly where the
/// user clicked.
fn point_on_segment(from: &Anchor, to: &Anchor, t: f32) -> Point {
    if is_straight(from, to) {
        from.point.lerp(to.point, t)
    } else {
        cubic_point(from.point, from.handle_out, to.handle_in, to.point, t)
    }
}

/// Each segment's start and end anchor, in order, including the closing
/// segment of a closed path.
fn segment_pairs(anchors: &[Anchor], closed: bool) -> impl Iterator<Item = (&Anchor, &Anchor)> {
    let closing = (closed && anchors.len() > 1).then(|| (&anchors[anchors.len() - 1], &anchors[0]));
    anchors.windows(2).map(|w| (&w[0], &w[1])).chain(closing)
}

fn segment_between(from: &Anchor, to: &Anchor) -> Segment {
    if is_straight(from, to) {
        Segment::Line { to: to.point }
    } else {
        Segment::Cubic { ctrl1: from.handle_out, ctrl2: to.handle_in, to: to.point }
    }
}

/// Moving, turning or scaling one shape on its own, rather than the layer
/// it's on. A layer's shapes have no positions of their own (each sits in
/// the layer's coordinates), so a shape is moved by moving its points:
/// rectangles and ovals become paths first.
impl crate::Element {
    /// Whether the shape can be moved by its points. Pictures, paintings
    /// and text are placed by their layer, so they move with it.
    pub fn is_reshapable(&self) -> bool {
        !matches!(self.shape, Shape::Image { .. } | Shape::Paint { .. } | Shape::Text(_))
    }

    /// The shape with every point mapped by `m`, or `None` for one that
    /// isn't [reshapable](Self::is_reshapable). A pencil line keeps its
    /// widths, scaled by how much `m` scales.
    pub fn transformed(&self, m: crate::Affine) -> Option<crate::Element> {
        if !self.is_reshapable() {
            return None;
        }
        let map = |anchors: Vec<Anchor>| -> Vec<Anchor> {
            anchors
                .into_iter()
                .map(|a| Anchor {
                    point: m.apply(a.point),
                    handle_in: m.apply(a.handle_in),
                    handle_out: m.apply(a.handle_out),
                })
                .collect()
        };
        let shape = if let Shape::Line { path, widths } = &self.shape {
            let anchors = anchors_from_path(path);
            let path = path_from_anchors(&map(anchors), path.closed)?;
            let scale = m.average_scale();
            Shape::Line { path, widths: widths.iter().map(|w| w * scale).collect() }
        } else {
            let (anchors, closed) = self.shape.to_anchors();
            Shape::Path(path_from_anchors(&map(anchors), closed)?)
        };
        let mut style = self.style.clone();
        if let Some(stroke) = &mut style.stroke {
            stroke.width *= m.average_scale();
            if let Some(dash) = &mut stroke.dash {
                dash.length *= m.average_scale();
                dash.gap *= m.average_scale();
            }
        }
        Some(crate::Element { shape, style })
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
    fn nearest_point_on_a_line() {
        let anchors = [p(0.0, 0.0), p(100.0, 0.0)].map(Anchor::corner);
        let hit = nearest_on_anchors(&anchors, false, p(30.0, 8.0)).unwrap();
        assert_eq!(hit.segment, 0);
        assert!((hit.t - 0.3).abs() < 1e-3);
        assert!((hit.distance - 8.0).abs() < 1e-3);
    }

    #[test]
    fn nearest_point_finds_the_closing_segment() {
        let square = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)].map(Anchor::corner);
        // Only a closed square has a left edge.
        assert_eq!(nearest_on_anchors(&square, true, p(-1.0, 5.0)).unwrap().segment, 3);
    }

    #[test]
    fn inserting_on_a_line_keeps_it_straight() {
        let anchors = [p(0.0, 0.0), p(100.0, 0.0)].map(Anchor::corner);
        let inserted = insert_anchor(&anchors, false, 0, 0.25);
        assert_eq!(inserted.len(), 3);
        assert_eq!(inserted[1], Anchor::corner(p(25.0, 0.0)));
        let path = path_from_anchors(&inserted, false).unwrap();
        assert!(path.segments.iter().all(|s| matches!(s, Segment::Line { .. })));
    }

    #[test]
    fn inserting_on_a_curve_preserves_its_shape() {
        let anchors =
            [Anchor::smooth(p(0.0, 0.0), p(30.0, -60.0)), Anchor::smooth(p(100.0, 0.0), p(130.0, 60.0))];
        let before = path_from_anchors(&anchors, false).unwrap().flatten();
        let inserted = insert_anchor(&anchors, false, 0, 0.4);
        let after = path_from_anchors(&inserted, false).unwrap();

        // Every point of the original curve should still lie on the new one.
        let after_anchors = anchors_from_path(&after);
        for point in before {
            let hit = nearest_on_anchors(&after_anchors, false, point).unwrap();
            assert!(hit.distance < 0.05, "drifted {} at {point:?}", hit.distance);
        }
    }

    #[test]
    fn inserting_on_the_closing_segment_appends() {
        let triangle = [p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0)].map(Anchor::corner);
        let inserted = insert_anchor(&triangle, true, 2, 0.5);
        assert_eq!(inserted.len(), 4);
        assert_eq!(inserted[3].point, p(0.0, 5.0));
    }

    #[test]
    fn one_anchor_is_not_a_path() {
        assert!(path_from_anchors(&[Anchor::corner(p(1.0, 1.0))], true).is_none());
    }

    #[test]
    fn a_shape_moves_and_scales_by_its_points() {
        use crate::{Affine, Color, Element, Style};
        let square = Element {
            shape: Shape::Rect { width: 20.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let moved = square.transformed(Affine::translate(Point::new(100.0, 50.0))).unwrap();
        assert!(matches!(moved.shape, Shape::Path(_)), "a rectangle becomes a path");
        let bounds = moved.shape.bounds();
        assert!(bounds.center().distance(Point::new(100.0, 50.0)) < 1e-3);
        assert!((bounds.width() - 20.0).abs() < 1e-3);

        let doubled = square.transformed(Affine::scale(2.0, 2.0)).unwrap();
        assert!((doubled.shape.bounds().width() - 40.0).abs() < 1e-3);

        let picture = Element {
            shape: Shape::Image { image: crate::ImageId(1), width: 10.0, height: 10.0 },
            style: square.style,
        };
        assert!(!picture.is_reshapable(), "a picture moves with its layer");
        assert!(picture.transformed(Affine::translate(Point::new(1.0, 0.0))).is_none());
    }
}
