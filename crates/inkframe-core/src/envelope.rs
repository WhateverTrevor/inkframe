//! Distort and Envelope, as in Animate's Free Transform tool: handles
//! around some shapes that bend them for good when they're dragged.
//!
//! Distort has a handle at each corner of the shapes' box. Dragging one
//! pulls that corner and stretches everything else to match, so a square
//! can lean into a slanted card or narrow into a road running off into the
//! distance. Envelope has a point at each corner and in the middle of each
//! side, and each point has handles that shape the curve either side of
//! it, as the pen tool's do: drag the middle of a side out to make it bow,
//! or a corner's handle to swing the side leaving it.
//!
//! Either way the box's four sides become curves (straight ones for
//! Distort), and the inside is filled in between them with a Coons patch:
//! a point of a shape goes to the blend of the sides above and below it
//! and the sides left and right of it, less the blend of the corners that
//! was counted twice. With straight sides that's the plain bilinear map
//! Distort wants. At rest the sides are the box's own edges, evenly
//! parametrised, so every point stays where it is.
//!
//! Unlike the deformers, nothing is animated or kept: the handles change
//! the shapes' points, and are gone once the edit is over. Shapes are cut
//! into short pieces first, so straight edges can curve.

use crate::anchor::{Anchor, anchors_from_path, path_from_anchors};
use crate::deform::subdivide;
use crate::document::Element;
use crate::geometry::{Point, Rect};
use crate::shape::Shape;

/// The handles of a Distort or an Envelope, round the box they bend.
#[derive(Debug, Clone, PartialEq)]
pub struct Lattice {
    /// The box the handles go round at rest.
    pub bounds: Rect,
    /// Round the box clockwise from its top left. Distort: the four
    /// corners. Envelope: each of its eight points (corners and the middles
    /// of the sides), followed by the two handles of the curve on to the
    /// next one: the point's way out, then the next point's way in.
    pub points: Vec<Point>,
    /// Envelope, with points between the corners and handles on them.
    curved: bool,
}

impl Lattice {
    /// Distort's four corners.
    pub fn distort(bounds: Rect) -> Self {
        Self { bounds, points: bounds.corners().to_vec(), curved: false }
    }

    /// Envelope's eight points with their handles, the sides straight.
    pub fn envelope(bounds: Rect) -> Self {
        let [tl, tr, br, bl] = bounds.corners();
        let ring = [tl, tl.lerp(tr, 0.5), tr, tr.lerp(br, 0.5), br, br.lerp(bl, 0.5), bl, bl.lerp(tl, 0.5)];
        let points = (0..ring.len())
            .flat_map(|k| {
                let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                [a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0)]
            })
            .collect();
        Self { bounds, points, curved: true }
    }

    /// Whether this is an Envelope rather than a Distort.
    pub fn is_envelope(&self) -> bool {
        self.curved
    }

    /// The same kind of lattice, untouched.
    fn at_rest(&self) -> Self {
        if self.curved { Self::envelope(self.bounds) } else { Self::distort(self.bounds) }
    }

    /// Whether every handle is where it started.
    pub fn is_at_rest(&self) -> bool {
        self.points.iter().zip(&self.at_rest().points).all(|(p, rest)| p.distance(*rest) < 1e-4)
    }

    /// For one of Envelope's handles, the point it belongs to (both by
    /// their place in [`points`](Self::points)); `None` for a point.
    pub fn handle_of(&self, index: usize) -> Option<usize> {
        if !self.curved {
            return None;
        }
        match index % 3 {
            0 => None,
            1 => Some(index - 1),
            _ => Some((index + 1) % self.points.len()),
        }
    }

    /// Whether `index` is a corner, which is drawn square; the middles of
    /// the sides are round, and handles smaller.
    pub fn is_corner(&self, index: usize) -> bool {
        !self.curved || index % 6 == 0
    }

    /// Moves handle or point `index` to `to`. A point takes its handles
    /// with it. The handles of a point in the middle of a side stay in line,
    /// so the side goes through it smoothly, unless `smooth` is off (Alt,
    /// as with the pen): then each moves on its own and the side can kink.
    pub fn move_point(&mut self, index: usize, to: Point, smooth: bool) {
        let count = self.points.len();
        if index >= count {
            return;
        }
        if !self.curved {
            self.points[index] = to;
            return;
        }
        match self.handle_of(index) {
            None => {
                let delta = to - self.points[index];
                for i in [index, index + 1, (index + count - 1) % count] {
                    self.points[i] = self.points[i] + delta;
                }
            }
            Some(point) => {
                self.points[index] = to;
                let middle = point % 6 == 3;
                if smooth && middle {
                    // The other handle turns to point the opposite way,
                    // keeping its own length.
                    let other = if index == point + 1 { (point + count - 1) % count } else { point + 1 };
                    let at = self.points[point];
                    let length = self.points[other].distance(at);
                    let away = at - to;
                    if away.length() > 1e-4 {
                        self.points[other] = at + away * (length / away.length());
                    }
                }
            }
        }
    }

    /// The curve along each side, as Bézier segments clockwise from the
    /// top left: one a side for Distort, two for Envelope.
    fn segments(&self) -> Vec<[Point; 4]> {
        let count = self.points.len();
        if self.curved {
            (0..count / 3)
                .map(|k| {
                    let i = k * 3;
                    [self.points[i], self.points[i + 1], self.points[i + 2], self.points[(i + 3) % count]]
                })
                .collect()
        } else {
            (0..4)
                .map(|k| {
                    let (a, b) = (self.points[k], self.points[(k + 1) % 4]);
                    [a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b]
                })
                .collect()
        }
    }

    /// The box's outline as the handles have bent it, as Bézier segments.
    pub fn outline(&self) -> Vec<[Point; 4]> {
        self.segments()
    }

    /// Where the point `p` of a shape goes. Points outside the box carry
    /// on along the same surface, so a stroke's edge isn't flattened.
    pub fn map(&self, p: Point) -> Point {
        let u = (p.x - self.bounds.min.x) / self.bounds.width().max(1e-3);
        let v = (p.y - self.bounds.min.y) / self.bounds.height().max(1e-3);
        let segments = self.segments();
        let per_side = segments.len() / 4;
        // Side `n` (top, right, bottom, left) at `t` from where it starts.
        let side = |n: usize, t: f32| {
            let s = t * per_side as f32;
            let piece = (s.floor().max(0.0) as usize).min(per_side - 1);
            let [a, b, c, d] = segments[n * per_side + piece];
            cubic(a, b, c, d, s - piece as f32)
        };
        // The bottom runs right to left and the left bottom to top.
        let (top, bottom) = (side(0, u), side(2, 1.0 - u));
        let (left, right) = (side(3, 1.0 - v), side(1, v));
        let corner = |n: usize| segments[n * per_side][0];
        let (tl, tr, br, bl) = (corner(0), corner(1), corner(2), corner(3));
        let rows = top * (1.0 - v) + bottom * v;
        let columns = left * (1.0 - u) + right * u;
        let corners =
            tl * ((1.0 - u) * (1.0 - v)) + tr * (u * (1.0 - v)) + bl * ((1.0 - u) * v) + br * (u * v);
        rows + columns - corners
    }

    /// The shape bent by the lattice, or `None` for one that can't be
    /// (a picture, a painting, text). A pencil line keeps its widths along
    /// it; anything else becomes a path.
    pub fn bend(&self, element: &Element) -> Option<Element> {
        if !element.is_reshapable() || self.points.len() != self.at_rest().points.len() {
            return None;
        }
        // Short enough pieces that a straight edge pulled sideways comes out
        // as a smooth curve: about a twentieth of the box's smaller side.
        let piece = (self.bounds.width().min(self.bounds.height()) / 20.0).max(1.0);
        let bend = |a: Anchor| Anchor {
            point: self.map(a.point),
            handle_in: self.map(a.handle_in),
            handle_out: self.map(a.handle_out),
        };
        let shape = match &element.shape {
            Shape::Line { path, widths } => {
                let anchors = anchors_from_path(path);
                let (anchors, widths) = cut(&anchors, Some(widths), path.closed, piece);
                let anchors: Vec<Anchor> = anchors.into_iter().map(bend).collect();
                Shape::Line { path: path_from_anchors(&anchors, path.closed)?, widths }
            }
            shape => {
                let (anchors, closed) = shape.to_anchors();
                let (anchors, _) = cut(&anchors, None, closed, piece);
                let anchors: Vec<Anchor> = anchors.into_iter().map(bend).collect();
                Shape::Path(path_from_anchors(&anchors, closed)?)
            }
        };
        Some(Element { shape, style: element.style.clone() })
    }
}

fn cubic(a: Point, b: Point, c: Point, d: Point, t: f32) -> Point {
    let s = 1.0 - t;
    a * (s * s * s) + b * (3.0 * s * s * t) + c * (3.0 * s * t * t) + d * (t * t * t)
}

/// Cuts an outline into pieces about `piece` long, one segment at a time so
/// a pencil line's widths (one per anchor) can be shared out along each
/// segment's new anchors. Without widths, the second half is empty.
fn cut(anchors: &[Anchor], widths: Option<&[f32]>, closed: bool, piece: f32) -> (Vec<Anchor>, Vec<f32>) {
    let width = |i: usize| widths.and_then(|w| w.get(i).or(w.last())).copied().unwrap_or(1.0);
    if anchors.len() < 2 {
        return (anchors.to_vec(), anchors.iter().enumerate().map(|(i, _)| width(i)).collect());
    }
    let count = if closed { anchors.len() } else { anchors.len() - 1 };
    let mut out: Vec<Anchor> = Vec::new();
    let mut out_widths: Vec<f32> = Vec::new();
    for i in 0..count {
        let next = (i + 1) % anchors.len();
        let pieces = subdivide(&[anchors[i], anchors[next]], false, piece);
        let steps = (pieces.len() - 1).max(1) as f32;
        for (k, anchor) in pieces.into_iter().enumerate() {
            if k == 0
                && let Some(last) = out.last_mut()
            {
                // Where this segment starts is where the last one ended:
                // one anchor, with this segment's way out.
                last.handle_out = anchor.handle_out;
                continue;
            }
            out.push(anchor);
            let t = k as f32 / steps;
            out_widths.push(width(i) + (width(next) - width(i)) * t);
        }
    }
    if closed && out.len() > 1 {
        // The last piece came back round to the first anchor.
        let seam = out.pop().expect("checked above");
        out_widths.pop();
        out[0].handle_in = seam.handle_in;
    }
    if widths.is_none() {
        out_widths.clear();
    }
    (out, out_widths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::anchors_from_path;
    use crate::shape::{Path, Segment};
    use crate::{Color, Style};

    fn square() -> Element {
        Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }
    }

    fn bounds() -> Rect {
        Rect::from_center_size(Point::ZERO, 100.0, 100.0)
    }

    fn outline(element: &Element) -> Vec<Point> {
        let Shape::Path(path) = &element.shape else { panic!("a path") };
        path.flatten()
    }

    #[test]
    fn a_lattice_at_rest_moves_nothing() {
        for lattice in [Lattice::distort(bounds()), Lattice::envelope(bounds())] {
            assert!(lattice.is_at_rest());
            for p in [Point::new(-50.0, -50.0), Point::new(12.0, -30.0), Point::new(80.0, 70.0)] {
                assert!(lattice.map(p).distance(p) < 1e-3, "{p:?}");
            }
            let bent = lattice.bend(&square()).expect("a square bends");
            let original = square().shape.bounds();
            let after = bent.shape.bounds();
            assert!(after.min.distance(original.min) < 1e-3 && after.max.distance(original.max) < 1e-3);
        }
    }

    #[test]
    fn distort_pulls_one_corner_and_keeps_the_others() {
        let mut lattice = Lattice::distort(bounds());
        // Narrow the top into a trapezoid: the top right corner comes in.
        lattice.points[1] = Point::new(20.0, -50.0);
        let points = outline(&lattice.bend(&square()).unwrap());
        let near = |q: Point| points.iter().any(|p| p.distance(q) < 1e-2);
        assert!(near(Point::new(20.0, -50.0)), "the corner follows the handle");
        assert!(
            near(Point::new(-50.0, -50.0)) && near(Point::new(-50.0, 50.0)) && near(Point::new(50.0, 50.0))
        );
        // Halfway down the right side is halfway between the two corners.
        assert!(near(Point::new(35.0, 0.0)));
    }

    #[test]
    fn envelope_bows_a_side_through_its_middle_point() {
        let mut lattice = Lattice::envelope(bounds());
        // The point in the middle of the top, 30 up, its handles with it.
        lattice.move_point(3, Point::new(0.0, -80.0), true);
        let bent = lattice.bend(&square()).unwrap();
        let points = outline(&bent);
        let top_middle = points.iter().filter(|p| p.x.abs() < 3.0).map(|p| p.y).fold(f32::MAX, f32::min);
        assert!((top_middle + 80.0).abs() < 0.5, "{top_middle}");
        // Halfway down, the bulge is halfway gone; the corners and the
        // bottom don't move.
        assert!((lattice.map(Point::new(0.0, 0.0)).y + 15.0).abs() < 1e-3);
        assert!(points.iter().any(|p| p.distance(Point::new(-50.0, -50.0)) < 1e-2));
        assert!((bent.shape.bounds().max.y - 50.0).abs() < 1e-2);
    }

    #[test]
    fn a_middle_points_handles_stay_in_line_unless_told_not_to() {
        let mut lattice = Lattice::envelope(bounds());
        // The top middle point's way out (toward the top right corner).
        assert_eq!(lattice.handle_of(4), Some(3));
        lattice.move_point(4, Point::new(16.0, -60.0), true);
        let (at, way_in) = (lattice.points[3], lattice.points[2]);
        let (out, inn) = (lattice.points[4] - at, way_in - at);
        assert!((out.x * inn.y - out.y * inn.x).abs() < 1e-2, "opposite ways: {out:?} {inn:?}");
        assert!((inn.length() - 50.0 / 3.0).abs() < 1e-3, "keeps its own length");
        // With Alt, the other handle stays where it was.
        lattice.move_point(4, Point::new(16.0, -40.0), false);
        assert_eq!(lattice.points[2], way_in);
    }

    #[test]
    fn a_corners_handle_swings_the_side_leaving_it() {
        let mut lattice = Lattice::envelope(bounds());
        // The top left corner's way out, pulled up: the top bulges near
        // that corner, and the corner itself stays.
        lattice.move_point(1, Point::new(-33.0, -80.0), true);
        assert!(lattice.map(Point::new(-40.0, -50.0)).y < -55.0);
        assert!(lattice.map(Point::new(-50.0, -50.0)).distance(Point::new(-50.0, -50.0)) < 1e-3);
        assert!(
            lattice.map(Point::new(40.0, -50.0)).distance(Point::new(40.0, -50.0)) < 1e-3,
            "the far half stays"
        );
    }

    #[test]
    fn a_pencil_line_keeps_its_widths_along_it() {
        let path = Path {
            start: Point::new(-50.0, 0.0),
            segments: vec![Segment::Line { to: Point::new(50.0, 0.0) }],
            closed: false,
        };
        let line = Element {
            shape: Shape::Line { path, widths: vec![2.0, 10.0] },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let mut lattice = Lattice::envelope(Rect::from_center_size(Point::ZERO, 100.0, 10.0));
        lattice.move_point(3, Point::new(0.0, -25.0), true);
        let Some(Element { shape: Shape::Line { path, widths }, .. }) = lattice.bend(&line) else {
            panic!("still a pencil line");
        };
        let anchors = anchors_from_path(&path);
        assert_eq!(anchors.len(), widths.len());
        assert!(anchors.len() > 2, "cut into pieces to bend");
        assert_eq!((widths[0], *widths.last().unwrap()), (2.0, 10.0));
        assert!(widths.windows(2).all(|w| w[1] >= w[0]), "widening all the way: {widths:?}");
    }

    #[test]
    fn closed_outlines_stay_closed_without_a_doubled_anchor() {
        let (anchors, closed) = square().shape.to_anchors();
        let (cut_up, _) = cut(&anchors, None, closed, 10.0);
        // Each 100-long side in ten pieces: forty anchors, none repeated.
        assert_eq!(cut_up.len(), 40);
        assert!(cut_up.windows(2).all(|w| w[0].point.distance(w[1].point) > 1.0));
        assert!(cut_up[0].point.distance(cut_up[39].point) > 1.0);
    }

    #[test]
    fn pictures_and_text_cant_be_bent() {
        let text = Element {
            shape: Shape::Text(crate::Text::new("hi", 12.0)),
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        assert!(Lattice::distort(bounds()).bend(&text).is_none());
    }
}
