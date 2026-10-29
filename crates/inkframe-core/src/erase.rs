//! Erasing drawn shapes, as Animate's Eraser does on its vector drawings:
//! rubbing across a line cuts it, leaving the pieces either side, and
//! rubbing across a fill takes a bite out of it.
//!
//! Lines are cut along their centreline. Each stretch the eraser missed is
//! kept, split off its curves exactly where the eraser's edge crosses them
//! (de Casteljau), so the pieces are the same smooth curves as before,
//! and a pencil line keeps its width at every point. Fills have the area
//! the eraser swept punched out of them (see `boolean.rs`). A shape with
//! both a fill and an outline is erased as the two it looks like: its
//! fill, and a line round it.
//!
//! As in Animate, the eraser can be kept to lines or to fills, or made to
//! take whole shapes it touches (Animate's faucet).

use serde::{Deserialize, Serialize};

use crate::anchor::path_from_anchors;
use crate::boolean::{ShapeOp, area_of, combine};
use crate::deform::split_cubic;
use crate::document::Element;
use crate::geometry::{Point, Rect};
use crate::i18n::tr_later;
use crate::shape::{Path, Segment, Shape, Style, cubic_point};

/// What the eraser takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EraseMode {
    /// Lines and fills, wherever it rubs.
    #[default]
    Normal,
    Lines,
    Fills,
    /// Every shape it touches, whole.
    Whole,
}

impl EraseMode {
    pub const ALL: [EraseMode; 4] = [EraseMode::Normal, EraseMode::Lines, EraseMode::Fills, EraseMode::Whole];

    pub fn label(self) -> &'static str {
        match self {
            EraseMode::Normal => tr_later("Lines and fills"),
            EraseMode::Lines => tr_later("Lines only"),
            EraseMode::Fills => tr_later("Fills only"),
            EraseMode::Whole => tr_later("Whole shapes"),
        }
    }
}

/// One movement of the eraser, `radius` round, from `from` to `to`, in the
/// drawing's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rub {
    pub from: Point,
    pub to: Point,
    pub radius: f32,
}

impl Rub {
    fn covers(&self, p: Point) -> bool {
        p.distance_to_segment(self.from, self.to) <= self.radius
    }

    fn bounds(&self) -> Rect {
        Rect::from_points(self.from, self.to).expand(self.radius)
    }

    /// The area the eraser swept: a circle drawn out along its movement.
    fn swept(&self) -> Element {
        const STEPS: usize = 12;
        let along = self.to - self.from;
        let heading = if along.length() > 1e-4 { along.y.atan2(along.x) } else { 0.0 };
        let half_turn = std::f32::consts::PI;
        // Round the front end, then back round the rear one.
        let arc = |centre: Point, from_angle: f32| {
            (0..=STEPS).map(move |i| {
                let angle = from_angle + half_turn * i as f32 / STEPS as f32;
                centre + Point::new(angle.cos(), angle.sin()) * self.radius
            })
        };
        let mut points =
            arc(self.to, heading - half_turn / 2.0).chain(arc(self.from, heading + half_turn / 2.0));
        let start = points.next().expect("arcs have points");
        let segments = points.map(|to| Segment::Line { to }).collect();
        Element {
            shape: Shape::Path(Path { start, segments, closed: true }),
            style: Style { fill: Some(crate::Color::BLACK), stroke: None, gradient: None },
        }
    }
}

/// `elements` rubbed by the eraser, or `None` if it touched none of them.
/// Shapes it cuts are replaced, in place, by what's left of them; the
/// returned counts say how many shapes replaced each one (1 for those left
/// as they were), so anything keeping track of shapes by their place can
/// follow.
pub fn erase(elements: &[Element], rub: Rub, mode: EraseMode) -> Option<(Vec<Element>, Vec<usize>)> {
    let mut touched = false;
    let mut kept = Vec::with_capacity(elements.len());
    let mut counts = Vec::with_capacity(elements.len());
    for element in elements {
        match erase_one(element, rub, mode) {
            Some(pieces) => {
                touched = true;
                counts.push(pieces.len());
                kept.extend(pieces);
            }
            None => {
                counts.push(1);
                kept.push(element.clone());
            }
        }
    }
    touched.then_some((kept, counts))
}

/// What's left of one shape after the rub, or `None` if it wasn't touched.
fn erase_one(element: &Element, rub: Rub, mode: EraseMode) -> Option<Vec<Element>> {
    let reach = element.style.stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
    if !overlaps(element.shape.bounds().expand(reach), rub.bounds()) {
        return None;
    }
    if mode == EraseMode::Whole {
        return touches(element, rub).then(Vec::new);
    }
    match &element.shape {
        Shape::Image { .. } | Shape::Paint { .. } | Shape::Text(_) => None,
        Shape::Line { path, widths } => {
            if mode == EraseMode::Fills {
                return None;
            }
            let pieces = cut(path, Some(widths), rub)?;
            let style = &element.style;
            Some(
                pieces
                    .into_iter()
                    .map(|(path, widths)| Element {
                        shape: Shape::Line { path, widths },
                        style: style.clone(),
                    })
                    .collect(),
            )
        }
        shape => {
            let filled = element.style.fill.is_some() || element.style.gradient.is_some();
            let outlined = element.style.stroke.is_some();
            let fill =
                Element { shape: shape.clone(), style: Style { stroke: None, ..element.style.clone() } };
            let outline = Element {
                shape: shape.clone(),
                style: Style { fill: None, gradient: None, ..element.style.clone() },
            };
            let fill_left = (filled && mode != EraseMode::Lines).then(|| punch(&fill, rub)).flatten();
            let outline_left = (outlined && mode != EraseMode::Fills)
                .then(|| {
                    let (anchors, closed) = shape.to_anchors();
                    let path = path_from_anchors(&anchors, closed)?;
                    cut(&path, None, rub)
                })
                .flatten();
            if fill_left.is_none() && outline_left.is_none() {
                return None;
            }
            // Fill beneath, outline on top, as the shape drew them.
            let mut left = Vec::new();
            if filled {
                left.extend(fill_left.unwrap_or_else(|| vec![fill]));
            }
            if outlined {
                match outline_left {
                    Some(pieces) => {
                        left.extend(pieces.into_iter().map(|(path, _)| Element {
                            shape: Shape::Path(path),
                            style: outline.style.clone(),
                        }))
                    }
                    None => left.push(outline),
                }
            }
            Some(left)
        }
    }
}

/// A fill with the rub's area punched out, or `None` if the rub missed it.
fn punch(fill: &Element, rub: Rub) -> Option<Vec<Element>> {
    let area = area_of(fill)?;
    let inside = |p: Point| area.iter().filter(|ring| encloses(ring, p)).count() % 2 == 1;
    let near_edge = area.iter().any(|ring| {
        (0..ring.len())
            .any(|i| segments_apart((ring[i], ring[(i + 1) % ring.len()]), (rub.from, rub.to)) <= rub.radius)
    });
    if !(near_edge || inside(rub.from) || inside(rub.to)) {
        return None;
    }
    combine(ShapeOp::Punch, &[fill.clone(), rub.swept()]).ok()
}

/// The stretches of `path` the rub missed, as paths of their own (with
/// their widths, for a pencil line's `widths`), or `None` if it missed the
/// whole path. Pieces too short to see are left out.
fn cut(path: &Path, widths: Option<&[f32]>, rub: Rub) -> Option<Vec<(Path, Vec<f32>)>> {
    // Each segment with where it starts; a closed path's closing edge too.
    let mut segments: Vec<(Point, Segment)> = Vec::new();
    let mut from = path.start;
    for segment in &path.segments {
        segments.push((from, *segment));
        from = segment.end();
    }
    if path.closed && from.distance(path.start) > 1e-4 {
        segments.push((from, Segment::Line { to: path.start }));
    }
    let at = |i: usize, t: f32| point_on(segments[i], t);

    // The kept stretches, each a run of (segment, from t, to t).
    let mut pieces: Vec<Vec<(usize, f32, f32)>> = Vec::new();
    let mut run: Vec<(usize, f32, f32)> = Vec::new();
    let mut touched = false;
    for (i, &segment) in segments.iter().enumerate() {
        let mut inside = rub.covers(at(i, 0.0));
        let mut kept_from = (!inside).then_some(0.0);
        touched |= inside;
        // Steps of a quarter of the eraser's width find every crossing.
        let length = rough_length(segment);
        let steps = ((length / (rub.radius * 0.5).max(0.25)).ceil() as usize).clamp(4, 1024);
        let mut before = 0.0;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let now_inside = rub.covers(at(i, t));
            if now_inside != inside {
                // Where it crosses the eraser's edge, by halving.
                let (mut lo, mut hi) = (before, t);
                for _ in 0..24 {
                    let middle = (lo + hi) / 2.0;
                    if rub.covers(at(i, middle)) == inside {
                        lo = middle;
                    } else {
                        hi = middle;
                    }
                }
                let crossing = (lo + hi) / 2.0;
                if now_inside {
                    touched = true;
                    run.push((i, kept_from.take().unwrap_or(0.0), crossing));
                    pieces.push(std::mem::take(&mut run));
                } else {
                    kept_from = Some(crossing);
                }
                inside = now_inside;
            }
            before = t;
        }
        if let Some(from) = kept_from {
            run.push((i, from, 1.0));
        }
    }
    if !run.is_empty() {
        pieces.push(run);
    }
    if !touched {
        return None;
    }
    // A closed path cut open: its last stretch runs on into its first.
    if path.closed && pieces.len() > 1 {
        let starts_at_start = pieces[0].first().is_some_and(|&(i, t, _)| i == 0 && t == 0.0);
        let last = segments.len() - 1;
        let ends_at_end =
            pieces.last().and_then(|p| p.last()).is_some_and(|&(i, _, t)| i == last && t == 1.0);
        if starts_at_start && ends_at_end {
            let first = pieces.remove(0);
            pieces.last_mut().expect("more than one").extend(first);
        }
    }

    // A pencil line's width at an anchor; the closing edge ends at the first.
    let anchor_width = |anchor: usize| {
        let widths = widths.unwrap_or(&[]);
        let anchor = if anchor > path.segments.len() { 0 } else { anchor };
        widths.get(anchor).or(widths.last()).copied().unwrap_or(1.0)
    };
    // Between anchors it eases, as the line is drawn (see `line.rs`).
    let width_at = |i: usize, t: f32| {
        let (w0, w1) = (anchor_width(i), anchor_width(i + 1));
        w0 + (w1 - w0) * t * t * (3.0 - 2.0 * t)
    };
    let kept = pieces
        .into_iter()
        .filter_map(|stretches| {
            let &(first, t0, _) = stretches.first()?;
            let start = at(first, t0);
            let mut out = Path { start, segments: Vec::new(), closed: false };
            let mut out_widths = vec![width_at(first, t0)];
            let mut length = 0.0;
            for (i, t0, t1) in stretches {
                if t1 - t0 < 1e-6 {
                    continue;
                }
                let part = part_of(segments[i], t0, t1);
                length += rough_length((at(i, t0), part));
                out.segments.push(part);
                out_widths.push(width_at(i, t1));
            }
            (length > 0.5 && !out.segments.is_empty()).then(|| {
                let widths = if widths.is_some() { out_widths } else { Vec::new() };
                (out, widths)
            })
        })
        .collect();
    Some(kept)
}

fn point_on((from, segment): (Point, Segment), t: f32) -> Point {
    match segment {
        Segment::Line { to } => from.lerp(to, t),
        Segment::Cubic { ctrl1, ctrl2, to } => cubic_point(from, ctrl1, ctrl2, to, t),
    }
}

/// The part of a segment from `t0` to `t1`, the same curve exactly.
fn part_of((from, segment): (Point, Segment), t0: f32, t1: f32) -> Segment {
    match segment {
        Segment::Line { to } => Segment::Line { to: from.lerp(to, t1) },
        Segment::Cubic { ctrl1, ctrl2, to } => {
            let (head, _) = split_cubic([from, ctrl1, ctrl2, to], t1);
            // `t0` along the whole is `t0 / t1` along the head.
            let (_, [_, ctrl1, ctrl2, to]) = split_cubic(head, if t1 > 0.0 { t0 / t1 } else { 0.0 });
            Segment::Cubic { ctrl1, ctrl2, to }
        }
    }
}

/// About how long a segment is: its control polygon's length, which is
/// never shorter than the curve.
fn rough_length((from, segment): (Point, Segment)) -> f32 {
    match segment {
        Segment::Line { to } => from.distance(to),
        Segment::Cubic { ctrl1, ctrl2, to } => {
            from.distance(ctrl1) + ctrl1.distance(ctrl2) + ctrl2.distance(to)
        }
    }
}

/// Whether a shape is under the rub anywhere: its lines or its fill.
fn touches(element: &Element, rub: Rub) -> bool {
    let along = rub.from.distance(rub.to);
    let steps = (along / (rub.radius * 0.5).max(0.25)).ceil().max(1.0) as usize;
    (0..=steps).any(|i| {
        let p = rub.from.lerp(rub.to, i as f32 / steps as f32);
        element.shape.hit_test(p, &element.style, rub.radius)
    })
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x <= b.max.x && a.max.x >= b.min.x && a.min.y <= b.max.y && a.max.y >= b.min.y
}

/// Whether a closed ring of points goes round `p` (counting crossings of a
/// line out to the right).
fn encloses(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// The closest two straight segments come to each other.
fn segments_apart((a, b): (Point, Point), (c, d): (Point, Point)) -> f32 {
    let cross = |o: Point, p: Point, q: Point| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let crossing =
        (cross(a, b, c) > 0.0) != (cross(a, b, d) > 0.0) && (cross(c, d, a) > 0.0) != (cross(c, d, b) > 0.0);
    if crossing {
        return 0.0;
    }
    [
        a.distance_to_segment(c, d),
        b.distance_to_segment(c, d),
        c.distance_to_segment(a, b),
        d.distance_to_segment(a, b),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Stroke};

    fn line_across(width: f32) -> Element {
        Element {
            shape: Shape::Path(Path {
                start: Point::new(0.0, 0.0),
                segments: vec![Segment::Line { to: Point::new(100.0, 0.0) }],
                closed: false,
            }),
            style: Style { fill: None, stroke: Some(Stroke::new(Color::BLACK, width)), gradient: None },
        }
    }

    fn rub_down_at(x: f32, radius: f32) -> Rub {
        Rub { from: Point::new(x, -20.0), to: Point::new(x, 20.0), radius }
    }

    fn ends(element: &Element) -> (Point, Point) {
        let Shape::Path(path) = &element.shape else { panic!("a path") };
        (path.start, path.segments.last().unwrap().end())
    }

    #[test]
    fn rubbing_across_a_line_cuts_it_in_two() {
        let (left, counts) = erase(&[line_across(2.0)], rub_down_at(50.0, 5.0), EraseMode::Normal).unwrap();
        assert_eq!(counts, [2]);
        let (a, b) = (ends(&left[0]), ends(&left[1]));
        assert!(a.0.distance(Point::new(0.0, 0.0)) < 1e-3 && (a.1.x - 45.0).abs() < 0.01, "{a:?}");
        assert!((b.0.x - 55.0).abs() < 0.01 && b.1.distance(Point::new(100.0, 0.0)) < 1e-3, "{b:?}");
        // Missing it changes nothing.
        assert!(erase(&[line_across(2.0)], rub_down_at(150.0, 5.0), EraseMode::Normal).is_none());
        // Fills only leaves lines alone.
        assert!(erase(&[line_across(2.0)], rub_down_at(50.0, 5.0), EraseMode::Fills).is_none());
    }

    #[test]
    fn a_cut_curve_is_the_same_curve() {
        let curve = Segment::Cubic {
            ctrl1: Point::new(30.0, 80.0),
            ctrl2: Point::new(70.0, -80.0),
            to: Point::new(100.0, 0.0),
        };
        let whole = (Point::new(0.0, 0.0), curve);
        let part = part_of(whole, 0.25, 0.75);
        for t in [0.0, 0.3, 0.5, 1.0] {
            let on_part = point_on((point_on(whole, 0.25), part), t);
            let on_whole = point_on(whole, 0.25 + 0.5 * t);
            assert!(on_part.distance(on_whole) < 1e-3, "{t}: {on_part:?} vs {on_whole:?}");
        }
    }

    #[test]
    fn a_pencil_line_keeps_its_widths() {
        let line = Element {
            shape: Shape::Line {
                path: Path {
                    start: Point::new(0.0, 0.0),
                    segments: vec![Segment::Line { to: Point::new(100.0, 0.0) }],
                    closed: false,
                },
                widths: vec![2.0, 10.0],
            },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let (left, _) = erase(&[line], rub_down_at(50.0, 5.0), EraseMode::Lines).unwrap();
        let Shape::Line { widths, .. } = &left[0].shape else { panic!("still a pencil line") };
        // The left piece ends at x 45, 45% of the way: eased, a bit under halfway.
        assert_eq!(widths.len(), 2);
        assert_eq!(widths[0], 2.0);
        assert!(widths[1] > 2.0 && widths[1] < 6.0, "{widths:?}");
    }

    #[test]
    fn a_closed_outline_opens_where_its_cut() {
        let square = Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: None, stroke: Some(Stroke::new(Color::BLACK, 2.0)), gradient: None },
        };
        // Through the middle of the top side only.
        let rub = Rub { from: Point::new(0.0, -60.0), to: Point::new(0.0, -40.0), radius: 5.0 };
        let (left, counts) = erase(&[square], rub, EraseMode::Normal).unwrap();
        assert_eq!(counts, [1], "one piece, running the long way round");
        let (a, b) = ends(&left[0]);
        assert!(a.distance(b) > 9.0 && a.distance(b) < 11.0, "the gap is the eraser's width: {a:?} {b:?}");
    }

    #[test]
    fn rubbing_a_fill_bites_it_and_keeps_its_outline_whole() {
        let square = Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style {
                fill: Some(Color::rgb(200, 0, 0)),
                stroke: Some(Stroke::new(Color::BLACK, 2.0)),
                gradient: None,
            },
        };
        // In the middle, away from the outline.
        let rub = Rub { from: Point::new(-10.0, 0.0), to: Point::new(10.0, 0.0), radius: 10.0 };
        let (left, counts) = erase(&[square], rub, EraseMode::Normal).unwrap();
        assert_eq!(counts, [2], "the fill with a hole, and the outline");
        assert!(left[0].style.stroke.is_none() && left[0].style.fill.is_some());
        assert!(!left[0].shape.hit_test(Point::ZERO, &left[0].style, 0.0), "a hole where it rubbed");
        assert!(left[0].shape.hit_test(Point::new(30.0, 30.0), &left[0].style, 0.0));
        assert!(matches!(left[1].shape, Shape::Rect { .. }), "the outline, untouched");
        // Whole shapes: it's gone.
        let (gone, _) = erase(&left, rub, EraseMode::Whole).unwrap();
        assert!(gone.iter().all(|e| e.style.fill.is_none()));
    }
}
