//! Combining shapes as Animate's Modify › ShapeOp Objects does: Union,
//! Intersect, Punch and Crop, and Break Apart for a shape of several
//! pieces.
//!
//! Each shape's filled area is its outline, flattened into straight edges.
//! Combining two areas works edge by edge:
//!
//! 1. Every edge is split wherever it crosses another, so no two edges
//!    cross except at their ends.
//! 2. An edge is kept if the result is inside on one side of it and
//!    outside on the other. That's checked a hair to each side of its
//!    middle, asking of each original area whether the point is inside it
//!    (by its winding number, as it's filled) and combining the answers.
//!    It's turned so the result is on its left.
//! 3. The kept edges join end to end into closed loops: outlines, and
//!    (running the other way) holes.
//!
//! The loops are then simplified and fitted with curves again (see
//! `vectorise::fit_points`), so a circle comes out as a few curves, not
//! hundreds of segments, and corners stay sharp. A hole is joined into the
//! outline round it, since a shape here is one path.

use std::collections::HashMap;

use crate::anchor::path_from_anchors;
use crate::document::Element;
use crate::geometry::Point;
use crate::shape::{Path, Shape, Style};
use crate::vectorise::{fit_points, join_hole};

/// How shapes combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeOp {
    /// One shape covering all of them, styled as the top one.
    Union,
    /// Only where they all overlap, styled as the top one.
    Intersect,
    /// The top shape cut out of each one beneath it, which keep their own
    /// styles; the top one goes.
    Punch,
    /// Each shape beneath trimmed to the top one; the top one goes.
    Crop,
}

/// Why shapes couldn't be combined.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CombineError {
    #[error("pick at least two shapes to combine")]
    TooFew,
    #[error("pictures, paintings and text can't be combined; Break Apart turns text into shapes first")]
    NotAShape,
}

/// A filled area: closed loops of points, filled by winding number.
pub type Area = Vec<Vec<Point>>;

/// How finely curves are flattened, and how closely the result is fitted
/// back, in the shapes' units.
const TOLERANCE: f32 = 0.25;

/// Combines `shapes` (bottom first, all in the same coordinates) as `how`
/// says, returning what replaces them, bottom first. Shapes that come to
/// nothing (punched right through, say) are left out.
pub fn combine(how: ShapeOp, shapes: &[Element]) -> Result<Vec<Element>, CombineError> {
    if shapes.len() < 2 {
        return Err(CombineError::TooFew);
    }
    let areas: Vec<Area> =
        shapes.iter().map(area_of).collect::<Option<_>>().ok_or(CombineError::NotAShape)?;
    let (top_area, below) = areas.split_last().expect("at least two");
    let top_style = &shapes[shapes.len() - 1].style;
    Ok(match how {
        ShapeOp::Union | ShapeOp::Intersect => {
            let op = if how == ShapeOp::Union { Op::Union } else { Op::Intersect };
            let merged = areas[1..].iter().fold(areas[0].clone(), |acc, next| boolean(op, &acc, next));
            elements_from(&merged, top_style)
        }
        ShapeOp::Punch | ShapeOp::Crop => {
            let op = if how == ShapeOp::Punch { Op::Difference } else { Op::Intersect };
            below
                .iter()
                .zip(shapes)
                .flat_map(|(area, shape)| elements_from(&boolean(op, area, top_area), &shape.style))
                .collect()
        }
    })
}

/// A shape split into its separate pieces, each (with the holes in it) a
/// shape of its own, in the same style: Animate's Break Apart. `outline`
/// is the shape's area, which for text the caller works out from its
/// glyphs. The pieces come in reading order, left to right, so a word's
/// letters stay in order.
pub fn break_apart(outline: &Area, style: &Style) -> Vec<Element> {
    let mut pieces = elements_from(outline, style);
    pieces.sort_by(|a, b| {
        let (a, b) = (a.bounds(), b.bounds());
        a.min.x.total_cmp(&b.min.x).then(a.min.y.total_cmp(&b.min.y))
    });
    pieces
}

/// A shape's filled area, or `None` for what has no outline here.
pub fn area_of(element: &Element) -> Option<Area> {
    let path = match &element.shape {
        Shape::Image { .. } | Shape::Paint { .. } | Shape::Text(_) => return None,
        Shape::Line { path, widths } => crate::line::line_outline(path, widths),
        shape => {
            let (anchors, closed) = shape.to_anchors();
            path_from_anchors(&anchors, closed)?
        }
    };
    // An open path fills as if closed.
    let mut points = path.flatten();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    Some(if points.len() >= 3 { vec![points] } else { Vec::new() })
}

// ------------------------------------------------------------ the boolean

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Union,
    Intersect,
    Difference,
}

/// `a` combined with `b`.
fn boolean(op: Op, a: &Area, b: &Area) -> Area {
    let mut edges: Vec<(Point, Point)> = Vec::new();
    for ring in a.iter().chain(b) {
        for i in 0..ring.len() {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            if p.distance(q) > 1e-6 {
                edges.push((p, q));
            }
        }
    }
    let pieces = split_at_crossings(&edges);

    // A hair either side of an edge, in proportion to the drawing's size.
    let size = edges.iter().flat_map(|&(p, q)| [p, q]).fold(0.0f32, |m, p| m.max(p.x.abs()).max(p.y.abs()));
    let hair = (size * 1e-5).max(1e-4);
    let result = |p: Point| {
        let (in_a, in_b) = (winding(a, p) != 0, winding(b, p) != 0);
        match op {
            Op::Union => in_a || in_b,
            Op::Intersect => in_a && in_b,
            Op::Difference => in_a && !in_b,
        }
    };

    let mut kept: Vec<(Point, Point)> = Vec::new();
    let mut seen: std::collections::HashSet<(Key, Key)> = std::collections::HashSet::new();
    for (p, q) in pieces {
        // The same stretch of edge from both shapes counts once.
        let (kp, kq) = (key(p), key(q));
        if kp == kq || !seen.insert(if kp < kq { (kp, kq) } else { (kq, kp) }) {
            continue;
        }
        let middle = Point::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0);
        let along = q - p;
        let left = Point::new(along.y, -along.x) * (hair / along.length());
        let (on_left, on_right) = (result(middle + left), result(middle - left));
        if on_left && !on_right {
            kept.push((p, q));
        } else if on_right && !on_left {
            kept.push((q, p));
        }
    }
    join_into_loops(kept)
}

/// The number of times the area's loops wind round `p` (0 is outside).
fn winding(area: &Area, p: Point) -> i32 {
    let mut winding = 0;
    for ring in area {
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let cross = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
            if a.y <= p.y {
                if b.y > p.y && cross > 0.0 {
                    winding += 1;
                }
            } else if b.y <= p.y && cross < 0.0 {
                winding -= 1;
            }
        }
    }
    winding
}

/// Every edge cut at every point where another crosses or touches it, so
/// the pieces only meet at their ends.
fn split_at_crossings(edges: &[(Point, Point)]) -> Vec<(Point, Point)> {
    let mut cuts: Vec<Vec<f32>> = vec![Vec::new(); edges.len()];
    let bounds = |&(p, q): &(Point, Point)| (p.x.min(q.x), p.y.min(q.y), p.x.max(q.x), p.y.max(q.y));
    let boxes: Vec<_> = edges.iter().map(bounds).collect();
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            let (a, b) = (boxes[i], boxes[j]);
            if a.2 < b.0 - 1e-6 || b.2 < a.0 - 1e-6 || a.3 < b.1 - 1e-6 || b.3 < a.1 - 1e-6 {
                continue;
            }
            for (s, t) in crossings(edges[i], edges[j]) {
                cuts[i].push(s);
                cuts[j].push(t);
            }
        }
    }
    let mut pieces = Vec::new();
    for (&(p, q), mut at) in edges.iter().zip(cuts) {
        at.retain(|&t| t > 1e-6 && t < 1.0 - 1e-6);
        at.sort_by(f32::total_cmp);
        let mut from = p;
        for t in at {
            let to = p + (q - p) * t;
            if from.distance(to) > 1e-6 {
                pieces.push((from, to));
            }
            from = to;
        }
        if from.distance(q) > 1e-6 {
            pieces.push((from, q));
        }
    }
    pieces
}

/// Where two edges meet, as how far along each (0 to 1). Edges lying along
/// each other meet at the ends of their overlap.
fn crossings((p, q): (Point, Point), (r, s): (Point, Point)) -> Vec<(f32, f32)> {
    let (d1, d2) = (q - p, s - r);
    let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
    let denominator = cross(d1, d2);
    let scale = d1.length() * d2.length();
    if denominator.abs() > 1e-9 * scale.max(1e-9) {
        let t = cross(r - p, d2) / denominator;
        let u = cross(r - p, d1) / denominator;
        let slack = 1e-6;
        if (-slack..=1.0 + slack).contains(&t) && (-slack..=1.0 + slack).contains(&u) {
            return vec![(t.clamp(0.0, 1.0), u.clamp(0.0, 1.0))];
        }
        return Vec::new();
    }
    // Parallel: overlapping only if on the same line.
    if cross(r - p, d1).abs() > 1e-6 * d1.length().max(1e-6) {
        return Vec::new();
    }
    let along1 = |x: Point| ((x - p).x * d1.x + (x - p).y * d1.y) / (d1.x * d1.x + d1.y * d1.y);
    let along2 = |x: Point| ((x - r).x * d2.x + (x - r).y * d2.y) / (d2.x * d2.x + d2.y * d2.y);
    let mut found = Vec::new();
    for end in [r, s] {
        let t = along1(end);
        if (0.0..=1.0).contains(&t) {
            found.push((t, along2(end)));
        }
    }
    for end in [p, q] {
        let u = along2(end);
        if (0.0..=1.0).contains(&u) {
            found.push((along1(end), u));
        }
    }
    found
}

/// A point rounded, to find edges meeting at it.
type Key = (i64, i64);

fn key(p: Point) -> Key {
    ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64)
}

/// Edges joined end to start into closed loops.
fn join_into_loops(edges: Vec<(Point, Point)>) -> Area {
    let mut from: HashMap<Key, Vec<usize>> = HashMap::new();
    for (i, &(p, _)) in edges.iter().enumerate() {
        from.entry(key(p)).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut ring = Vec::new();
        let mut at = start;
        loop {
            used[at] = true;
            ring.push(edges[at].0);
            let end = key(edges[at].1);
            if end == key(edges[start].0) {
                break;
            }
            let Some(&next) = from.get(&end).and_then(|c| c.iter().find(|&&i| !used[i])) else {
                break; // Shouldn't happen: every kept edge's end starts another.
            };
            at = next;
        }
        if ring.len() >= 3 {
            loops.push(ring);
        }
    }
    loops
}

/// Signed area: positive for a loop running clockwise on screen (y down),
/// which is how the kept edges run round an outline.
fn signed_area(ring: &[Point]) -> f32 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>()
        / 2.0
}

/// An area as shapes: each outline (with the holes inside it joined in)
/// fitted with curves and given `style`.
fn elements_from(area: &Area, style: &Style) -> Vec<Element> {
    // Which way round an outline runs depends on the caller's area; take
    // the biggest loop's direction as "outline".
    let outward = area.iter().max_by(|a, b| signed_area(a).abs().total_cmp(&signed_area(b).abs()));
    let Some(outward) = outward.map(|r| signed_area(r).signum()) else { return Vec::new() };
    let (outlines, holes): (Vec<&Vec<Point>>, Vec<&Vec<Point>>) =
        area.iter().filter(|r| signed_area(r).abs() > 1e-3).partition(|r| signed_area(r).signum() == outward);

    let mut placed: Vec<(f32, Path, Vec<Path>)> = outlines
        .iter()
        .filter_map(|ring| Some((signed_area(ring).abs(), fit_points(ring.to_vec(), TOLERANCE)?, Vec::new())))
        .collect();
    // Each hole goes in the smallest outline it's inside.
    for hole in holes {
        let inside = |outline: &Vec<Point>| winding(&vec![outline.clone()], hole[0]) != 0;
        let host = outlines
            .iter()
            .enumerate()
            .filter(|(_, o)| inside(o))
            .min_by(|a, b| signed_area(a.1).abs().total_cmp(&signed_area(b.1).abs()))
            .map(|(i, _)| i);
        if let (Some(i), Some(path)) = (host, fit_points(hole.clone(), TOLERANCE)) {
            if let Some(entry) = placed.get_mut(i) {
                entry.2.push(path);
            }
        }
    }
    // Biggest first, so smaller pieces lie on top.
    placed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut elements = Vec::new();
    for (_, outline, holes) in placed {
        if holes.is_empty() || style.stroke.is_none() {
            let path = holes.into_iter().fold(outline, join_hole);
            elements.push(Element { shape: Shape::Path(path), style: style.clone() });
            continue;
        }
        // A hole is joined into its outline through a slit, there and
        // back, since a shape here is one path. Filled, the slit doesn't
        // show; stroked, it would. So, as Animate keeps them, the fill is a
        // shape of its own, and the outline's and each hole's stroke are
        // shapes on top.
        let fill_only = Style { stroke: None, ..style.clone() };
        let stroke_only = Style { fill: None, gradient: None, ..style.clone() };
        let edges: Vec<Path> = std::iter::once(outline.clone()).chain(holes.iter().cloned()).collect();
        let filled = holes.into_iter().fold(outline, join_hole);
        elements.push(Element { shape: Shape::Path(filled), style: fill_only });
        elements.extend(
            edges.into_iter().map(|path| Element { shape: Shape::Path(path), style: stroke_only.clone() }),
        );
    }
    elements
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Affine, Color};

    fn square(x: f32, y: f32, size: f32, color: Color) -> Element {
        Element {
            shape: Shape::Rect { width: size, height: size, corner_radius: 0.0 },
            style: Style { fill: Some(color), stroke: None, gradient: None },
        }
        .transformed(Affine::translate(Point::new(x, y)))
        .unwrap()
    }

    /// The area a shape covers, counted on a grid of sample points.
    fn covered(elements: &[Element], from: Point, to: Point) -> usize {
        let areas: Vec<Area> = elements.iter().filter_map(area_of).collect();
        let mut count = 0;
        let mut y = from.y + 0.5;
        while y < to.y {
            let mut x = from.x + 0.5;
            while x < to.x {
                if areas.iter().any(|a| winding(a, Point::new(x, y)) != 0) {
                    count += 1;
                }
                x += 1.0;
            }
            y += 1.0;
        }
        count
    }

    const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    const BLUE: Color = Color { r: 0, g: 0, b: 255, a: 255 };

    /// Two 20×20 squares overlapping by a 10×10 corner.
    fn pair() -> [Element; 2] {
        [square(0.0, 0.0, 20.0, RED), square(10.0, 10.0, 20.0, BLUE)]
    }

    fn whole() -> (Point, Point) {
        (Point::new(-20.0, -20.0), Point::new(30.0, 30.0))
    }

    #[test]
    fn union_covers_both_once() {
        let result = combine(ShapeOp::Union, &pair()).unwrap();
        assert_eq!(result.len(), 1);
        let (a, b) = whole();
        assert_eq!(covered(&result, a, b), 400 + 400 - 100);
        assert_eq!(result[0].style.fill, Some(BLUE), "styled as the top shape");
        let Shape::Path(path) = &result[0].shape else { panic!() };
        assert_eq!(path.segments.len(), 8, "eight sharp corners: {path:?}");
    }

    #[test]
    fn intersect_keeps_the_overlap() {
        let result = combine(ShapeOp::Intersect, &pair()).unwrap();
        let (a, b) = whole();
        assert_eq!(covered(&result, a, b), 100);
    }

    #[test]
    fn punch_cuts_the_top_out_of_what_is_below() {
        let result = combine(ShapeOp::Punch, &pair()).unwrap();
        assert_eq!(result.len(), 1, "the top shape goes");
        assert_eq!(result[0].style.fill, Some(RED), "what's left keeps its own style");
        let (a, b) = whole();
        assert_eq!(covered(&result, a, b), 300);
    }

    #[test]
    fn crop_trims_to_the_top() {
        let result = combine(ShapeOp::Crop, &pair()).unwrap();
        assert_eq!(result[0].style.fill, Some(RED));
        let (a, b) = whole();
        assert_eq!(covered(&result, a, b), 100);
    }

    #[test]
    fn punching_a_hole_keeps_the_hole() {
        // A small square right inside a big one.
        let shapes = [square(0.0, 0.0, 40.0, RED), square(0.0, 0.0, 10.0, BLUE)];
        let result = combine(ShapeOp::Punch, &shapes).unwrap();
        assert_eq!(result.len(), 1, "one shape with a hole in it");
        let (a, b) = (Point::new(-25.0, -25.0), Point::new(25.0, 25.0));
        assert_eq!(covered(&result, a, b), 1600 - 100);

        // Stroked, the stroke goes round the outline and the hole, not
        // along the slit joining them: a fill and two strokes.
        let mut stroked = shapes.clone();
        stroked[0].style.stroke = Some(crate::Stroke::new(Color::BLACK, 2.0));
        let result = combine(ShapeOp::Punch, &stroked).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].style.stroke.is_none() && result[0].style.fill.is_some());
        assert!(result[1..].iter().all(|e| e.style.fill.is_none() && e.style.stroke.is_some()));
    }

    #[test]
    fn shapes_sharing_an_edge_union_cleanly() {
        let shapes = [square(0.0, 0.0, 20.0, RED), square(20.0, 0.0, 20.0, RED)];
        let result = combine(ShapeOp::Union, &shapes).unwrap();
        assert_eq!(result.len(), 1);
        let (a, b) = (Point::new(-20.0, -20.0), Point::new(40.0, 20.0));
        assert_eq!(covered(&result, a, b), 800);
    }

    #[test]
    fn circles_come_back_as_curves() {
        let circle = |x: f32| {
            Element {
                shape: Shape::Ellipse { radius_x: 20.0, radius_y: 20.0 },
                style: Style { fill: Some(RED), stroke: None, gradient: None },
            }
            .transformed(Affine::translate(Point::new(x, 0.0)))
            .unwrap()
        };
        let result = combine(ShapeOp::Union, &[circle(0.0), circle(25.0)]).unwrap();
        let Shape::Path(path) = &result[0].shape else { panic!() };
        // Flattened, two circles would be over 120 straight pieces.
        assert!(path.segments.len() < 60, "fitted, not flattened: {} segments", path.segments.len());
        let area = covered(&result, Point::new(-30.0, -30.0), Point::new(60.0, 30.0));
        // Two circles of area 1257 overlapping by about 337.
        assert!((2100..2250).contains(&area), "{area}");
    }

    #[test]
    fn breaking_apart_separates_pieces() {
        let two = vec![
            vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)],
            vec![
                Point::new(20.0, 0.0),
                Point::new(30.0, 0.0),
                Point::new(30.0, 10.0),
                Point::new(20.0, 10.0),
            ],
        ];
        let style = Style { fill: Some(RED), stroke: None, gradient: None };
        assert_eq!(break_apart(&two, &style).len(), 2);
    }

    #[test]
    fn too_few_or_unshaped_is_refused() {
        assert_eq!(combine(ShapeOp::Union, &pair()[..1]), Err(CombineError::TooFew));
    }
}
