//! Distort and Envelope, as in Animate's Free Transform tool: a lattice of
//! handles over some shapes that bends them for good when it's dragged.
//!
//! Distort has a handle at each corner of the shapes' box. Dragging one
//! pulls that corner and stretches everything else to match, so a square
//! can lean into a slanted card or narrow into a road running off into the
//! distance. Envelope has a 4×4 lattice, the corners and the points between
//! them, for bulging and bending: drag the middle of a side out to make
//! that side bow.
//!
//! Both are one Bézier patch over the box: a point of a shape goes to a
//! blend of every handle, weighted by Bernstein polynomials across and down
//! (as the free-form deformer in `warp.rs` does). With two handles each way
//! the blend is the plain bilinear one Distort wants; with four it's a
//! smooth bicubic surface, and the handles between the corners pull it
//! like a curve's control points. At rest the lattice is evenly spaced, and
//! an even lattice maps every point to itself.
//!
//! Unlike the deformer, nothing is animated or kept: the lattice changes
//! the shapes' points, and is gone once the edit is over. Shapes are cut
//! into short pieces first, so straight edges can curve.

use crate::anchor::{Anchor, anchors_from_path, path_from_anchors};
use crate::deform::subdivide;
use crate::document::Element;
use crate::geometry::{Point, Rect};
use crate::shape::Shape;
use crate::warp::bernstein;

/// Handles of a Distort or an Envelope, over the box they bend.
#[derive(Debug, Clone, PartialEq)]
pub struct Lattice {
    /// The box the lattice covers at rest.
    pub bounds: Rect,
    /// Handles across and down (at least 2 each).
    pub columns: usize,
    pub rows: usize,
    /// Where each handle is now, row by row from the top left.
    pub points: Vec<Point>,
}

impl Lattice {
    /// An even lattice of `columns`×`rows` handles over `bounds`, bending
    /// nothing yet.
    pub fn new(bounds: Rect, columns: usize, rows: usize) -> Self {
        let (columns, rows) = (columns.clamp(2, 8), rows.clamp(2, 8));
        let mut lattice = Self { bounds, columns, rows, points: Vec::new() };
        lattice.points = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| lattice.rest_point(column, row))
            .collect();
        lattice
    }

    /// Distort's four corners.
    pub fn distort(bounds: Rect) -> Self {
        Self::new(bounds, 2, 2)
    }

    /// Envelope's sixteen handles.
    pub fn envelope(bounds: Rect) -> Self {
        Self::new(bounds, 4, 4)
    }

    /// Handle `(column, row)` at rest.
    pub fn rest_point(&self, column: usize, row: usize) -> Point {
        let u = column as f32 / (self.columns - 1) as f32;
        let v = row as f32 / (self.rows - 1) as f32;
        Point::new(self.bounds.min.x + u * self.bounds.width(), self.bounds.min.y + v * self.bounds.height())
    }

    /// Whether every handle is where it started.
    pub fn is_at_rest(&self) -> bool {
        self.points.iter().enumerate().all(|(i, p)| {
            let rest = self.rest_point(i % self.columns, i / self.columns);
            p.distance(rest) < 1e-4
        })
    }

    /// Whether the handle at `index` is a corner of the lattice, which
    /// Envelope draws square and the others round.
    pub fn is_corner(&self, index: usize) -> bool {
        let (column, row) = (index % self.columns, index / self.columns);
        (column == 0 || column + 1 == self.columns) && (row == 0 || row + 1 == self.rows)
    }

    /// Where the point `p` of a shape goes. Points outside the box carry
    /// on along the same surface, so a stroke's edge isn't flattened.
    pub fn map(&self, p: Point) -> Point {
        let u = (p.x - self.bounds.min.x) / self.bounds.width().max(1e-3);
        let v = (p.y - self.bounds.min.y) / self.bounds.height().max(1e-3);
        let across = bernstein(self.columns - 1, u);
        let down = bernstein(self.rows - 1, v);
        let mut to = Point::ZERO;
        for (row, wv) in down.iter().enumerate() {
            for (column, wu) in across.iter().enumerate() {
                to = to + self.points[row * self.columns + column] * (wu * wv);
            }
        }
        to
    }

    /// The shape bent by the lattice, or `None` for one that can't be
    /// (a picture, a painting, text). A pencil line keeps its widths along
    /// it; anything else becomes a path.
    pub fn bend(&self, element: &Element) -> Option<Element> {
        if !element.is_reshapable() || self.points.len() != self.columns * self.rows {
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
    fn envelope_bows_a_side_smoothly() {
        let mut lattice = Lattice::envelope(bounds());
        // Push the two handles in the middle of the top edge up by 30.
        for i in [1, 2] {
            lattice.points[i].y -= 30.0;
        }
        let bent = lattice.bend(&square()).unwrap();
        let points = outline(&bent);
        let top_middle = points.iter().filter(|p| p.x.abs() < 3.0).map(|p| p.y).fold(f32::MAX, f32::min);
        // A cubic with its middle handles 30 up peaks 3/4 of the way there.
        assert!((top_middle - (-50.0 - 22.5)).abs() < 0.5, "{top_middle}");
        // The corners stay put, and the bottom doesn't move.
        assert!(points.iter().any(|p| p.distance(Point::new(-50.0, -50.0)) < 1e-2));
        assert!((bent.shape.bounds().max.y - 50.0).abs() < 1e-2);
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
        lattice.points[5].y -= 20.0;
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
