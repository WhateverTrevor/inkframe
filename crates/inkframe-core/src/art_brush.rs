//! Art and pattern brushes, as Animate's Paint Brush has: a stroke drawn
//! as a picture laid along it, rather than as a plain line.
//!
//! The picture is a symbol from the library, its first frame. An **art**
//! brush stretches it along the whole stroke, a brushstroke or a ribbon
//! drawn once, end to end. A **pattern** brush repeats it, each copy as
//! long as it is wide at the stroke's width (rounded so a whole number
//! fit), for chains, stitches, fences and vines. Either way it bends with
//! the stroke: a point of the picture so far across it and so far up goes
//! that far along the stroke and that far to one side of it, the
//! picture's height spanning the stroke's width.
//!
//! The stroke's shape is kept as drawn, so it can be edited as any other;
//! the brush only changes how it's drawn (and exported).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, anchors_from_path, path_from_anchors};
use crate::deform::subdivide;
use crate::document::{Document, Element};
use crate::geometry::{Point, Rect};
use crate::shape::{Path, Shape};
use crate::symbol::SymbolId;

/// How a brush lays its picture along a stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushKind {
    /// Stretched once along the whole stroke.
    Art,
    /// Repeated along it.
    Pattern,
}

/// A stroke drawn with a symbol's picture (see the module's notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrokeBrush {
    pub kind: BrushKind,
    pub symbol: SymbolId,
}

/// A line to lay pictures along: its points, how far along it each is,
/// and which way is "up" at each (the left of the way it goes).
struct Track {
    points: Vec<Point>,
    along: Vec<f32>,
    normals: Vec<Point>,
}

impl Track {
    fn new(points: Vec<Point>) -> Option<Self> {
        let mut points = points;
        points.dedup_by(|a, b| a.distance(*b) < 1e-4);
        if points.len() < 2 {
            return None;
        }
        let mut along = vec![0.0];
        for pair in points.windows(2) {
            along.push(along.last().copied().unwrap_or(0.0) + pair[0].distance(pair[1]));
        }
        // Each point's normal halfway between its two segments', so the picture
        // bends round corners rather than tearing at them.
        let side = |a: Point, b: Point| {
            let d = b - a;
            let length = d.length().max(1e-6);
            Point::new(d.y / length, -d.x / length)
        };
        let last = points.len() - 1;
        let normals = (0..points.len())
            .map(|i| {
                let before = (i > 0).then(|| side(points[i - 1], points[i]));
                let after = (i < last).then(|| side(points[i], points[i + 1]));
                match (before, after) {
                    (Some(a), Some(b)) => {
                        let mid = a + b;
                        if mid.length() < 1e-4 { a } else { mid * (1.0 / mid.length()) }
                    }
                    (Some(n), None) | (None, Some(n)) => n,
                    (None, None) => Point::new(0.0, -1.0),
                }
            })
            .collect();
        Some(Self { points, along, normals })
    }

    fn length(&self) -> f32 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// The point `s` along the line, moved `up` to its left.
    fn at(&self, s: f32, up: f32) -> Point {
        let s = s.clamp(0.0, self.length());
        let i = self.along.partition_point(|&a| a <= s).clamp(1, self.points.len() - 1);
        let (a, b) = (self.along[i - 1], self.along[i]);
        let t = if b - a < 1e-6 { 0.0 } else { (s - a) / (b - a) };
        let point = self.points[i - 1].lerp(self.points[i], t);
        let normal = self.normals[i - 1].lerp(self.normals[i], t);
        point + normal * up
    }
}

impl Document {
    /// A brush's picture: the shapes on the first frame of its symbol, in
    /// the symbol's coordinates, and the box round them. Brushes in the
    /// picture's own strokes are left out, so a brush can't draw itself.
    fn brush_picture(&self, symbol: SymbolId) -> Option<(Vec<Element>, Rect)> {
        let symbol = self.symbol(symbol)?;
        let mut shapes = Vec::new();
        for layer in symbol.layers.iter().filter(|l| l.is_shown_at(0) && !l.guide && !l.mask) {
            let m = self.layer_matrix(layer, 0);
            for element in layer.shown_elements(0).iter().filter_map(|e| e.transformed(m)) {
                let mut element = element;
                if let Some(stroke) = &mut element.style.stroke {
                    stroke.brush = None;
                }
                shapes.push(element);
            }
        }
        let bounds = shapes.iter().map(Element::bounds).reduce(Rect::union)?;
        (bounds.width() > 1e-3 && bounds.height() > 1e-3).then_some((shapes, bounds))
    }

    /// What `element` is drawn as: itself, or with a brushed stroke, its
    /// fill (the stroke left off) and then the brush's picture laid along
    /// its outline. A pencil line is laid along its centre at its average
    /// width, and drawn as the brush alone.
    pub fn drawn_as<'a>(&self, element: &'a Element) -> Cow<'a, [Element]> {
        let Some(stroke) = element.style.stroke else { return Cow::Borrowed(std::slice::from_ref(element)) };
        let Some(brush) = stroke.brush else { return Cow::Borrowed(std::slice::from_ref(element)) };
        let Some((picture, bounds)) = self.brush_picture(brush.symbol) else {
            return Cow::Borrowed(std::slice::from_ref(element));
        };
        let (centre, width) = match &element.shape {
            Shape::Line { path, widths } => {
                let mean = widths.iter().sum::<f32>() / widths.len().max(1) as f32;
                (path.clone(), mean.max(1.0))
            }
            shape => {
                let (anchors, closed) = shape.to_anchors();
                let Some(path) = path_from_anchors(&anchors, closed) else {
                    return Cow::Borrowed(std::slice::from_ref(element));
                };
                (path, stroke.width.max(1.0))
            }
        };
        let mut out = Vec::new();
        if !matches!(element.shape, Shape::Line { .. }) && element.style.fill.is_some() {
            let mut fill = element.clone();
            fill.style.stroke = None;
            out.push(fill);
        }
        out.extend(lay_along(&picture, bounds, &centre, width, brush.kind));
        Cow::Owned(out)
    }
}

/// `picture` (in a box `bounds`) laid along `path` at `width`, once or
/// over and over.
fn lay_along(picture: &[Element], bounds: Rect, path: &Path, width: f32, kind: BrushKind) -> Vec<Element> {
    let Some(track) = Track::new(path.flatten()) else { return Vec::new() };
    let total = track.length();
    let height_scale = width / bounds.height();
    let (tiles, tile) = match kind {
        BrushKind::Art => (1, total),
        BrushKind::Pattern => {
            // Each copy as long as it is wide at this width, as many as fit.
            let natural = (bounds.width() * height_scale).max(1.0);
            let count = ((total / natural).round() as usize).clamp(1, 2000);
            (count, total / count as f32)
        }
    };
    // Short enough pieces that each bends smoothly with the line.
    let piece = (bounds.width() / 24.0).max(0.5);
    let middle = (bounds.min.y + bounds.max.y) / 2.0;
    let mut out = Vec::new();
    for t in 0..tiles {
        let place = |p: Point| {
            let across = (p.x - bounds.min.x) / bounds.width();
            // Up the picture is to the line's left, as drawn left to right.
            track.at((t as f32 + across) * tile, (middle - p.y) * height_scale)
        };
        for element in picture {
            if let Some(laid) = lay_element(element, piece, &place, height_scale) {
                out.push(laid);
            }
        }
    }
    out
}

/// One shape of a brush's picture, cut into short pieces and each point
/// placed along the line. Its stroke thins or thickens with the picture.
fn lay_element(element: &Element, piece: f32, place: &dyn Fn(Point) -> Point, scale: f32) -> Option<Element> {
    let place_anchor = |a: Anchor| Anchor {
        point: place(a.point),
        handle_in: place(a.handle_in),
        handle_out: place(a.handle_out),
    };
    let shape = match &element.shape {
        Shape::Line { path, widths } => {
            let anchors: Vec<Anchor> = anchors_from_path(path).into_iter().map(place_anchor).collect();
            Shape::Line {
                path: path_from_anchors(&anchors, path.closed)?,
                widths: widths.iter().map(|w| w * scale).collect(),
            }
        }
        shape => {
            let (anchors, closed) = shape.to_anchors();
            let anchors: Vec<Anchor> =
                subdivide(&anchors, closed, piece).into_iter().map(place_anchor).collect();
            Shape::Path(path_from_anchors(&anchors, closed)?)
        }
    };
    let mut style = element.style.clone();
    if let Some(stroke) = &mut style.stroke {
        stroke.width *= scale;
    }
    Some(Element { shape, style })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Segment, Stroke, Style, Transform};

    /// A document with a symbol holding a 40×10 bar, and a straight
    /// 200-long stroke along the x axis drawn with it, 20 wide.
    fn brushed(kind: BrushKind) -> (Document, Element) {
        let mut doc = Document::default();
        let bar = Element {
            shape: Shape::Rect { width: 40.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::rgb(200, 0, 0)), stroke: None, gradient: None },
        };
        let layer = doc.add_layer("Bead", vec![bar], 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(layer, 0).unwrap();
        let stroke = Stroke { brush: Some(StrokeBrush { kind, symbol }), ..Stroke::new(Color::BLACK, 20.0) };
        let line = Element {
            shape: Shape::Path(Path {
                start: Point::new(0.0, 0.0),
                segments: vec![Segment::Line { to: Point::new(200.0, 0.0) }],
                closed: false,
            }),
            style: Style { fill: None, stroke: Some(stroke), gradient: None },
        };
        (doc, line)
    }

    #[test]
    fn an_art_brush_stretches_its_picture_along_the_whole_stroke() {
        let (doc, line) = brushed(BrushKind::Art);
        let drawn = doc.drawn_as(&line);
        assert_eq!(drawn.len(), 1);
        let bounds = drawn[0].shape.bounds();
        assert!((bounds.width() - 200.0).abs() < 0.5 && (bounds.height() - 20.0).abs() < 0.5, "{bounds:?}");
        assert_eq!(drawn[0].style.fill, Some(Color::rgb(200, 0, 0)));
    }

    #[test]
    fn a_pattern_brush_repeats_its_picture_as_many_times_as_fit() {
        let (doc, line) = brushed(BrushKind::Pattern);
        // At 20 wide, the 40×10 bar is 80 long: 200 / 80 rounds to 3 copies,
        // each stretched a little to fill the stroke exactly.
        let drawn = doc.drawn_as(&line);
        assert_eq!(drawn.len(), 2 + 1);
        let first = drawn[0].shape.bounds();
        assert!((first.width() - 200.0 / 3.0).abs() < 0.5, "{first:?}");
    }

    #[test]
    fn a_brush_bends_round_a_corner() {
        let (doc, mut line) = brushed(BrushKind::Art);
        line.shape = Shape::Path(Path {
            start: Point::new(0.0, 0.0),
            segments: vec![
                Segment::Line { to: Point::new(100.0, 0.0) },
                Segment::Line { to: Point::new(100.0, 100.0) },
            ],
            closed: false,
        });
        let drawn = doc.drawn_as(&line);
        let bounds = drawn[0].shape.bounds();
        // Along the top and down the side: the bar covers both legs.
        assert!(bounds.max.y > 95.0 && bounds.max.x > 105.0, "{bounds:?}");
    }

    #[test]
    fn a_plain_stroke_is_drawn_as_it_is() {
        let (doc, mut line) = brushed(BrushKind::Art);
        line.style.stroke = line.style.stroke.map(|s| Stroke { brush: None, ..s });
        assert!(matches!(doc.drawn_as(&line), Cow::Borrowed(_)));
    }
}
