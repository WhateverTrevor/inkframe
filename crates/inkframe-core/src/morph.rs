//! Shape tweens: one drawing turning smoothly into the next, as Animate's
//! shape tweens do.
//!
//! A layer's drawings swap at exposure keys (see `drawing.rs`). A
//! [`ShapeTween`] on a swap makes the frames before it morph, from the
//! drawing showing into the one the swap brings in, instead of holding
//! and then jumping.
//!
//! The morph works on anchors (see `anchor.rs`), so rectangles, ellipses
//! and paths all take part, and the shapes in between keep real curves:
//!
//! 1. Each pair of shapes (the first of one drawing with the first of the
//!    other, and so on) is turned into anchors.
//! 2. The one with fewer anchors has its longest segments split until the
//!    counts match. Splitting a segment doesn't change the shape, so the
//!    tween still starts and ends exactly on the two drawings.
//! 3. Closed shapes are turned round (and, if need be, reversed) so each
//!    anchor pairs with the one nearest it, relative to each shape's
//!    centre. Shape hints, if there are any, decide this instead: hint `a`
//!    on one drawing goes to hint `a` on the other.
//! 4. Points, handles, colours and stroke widths are blended.
//!
//! Shapes with nothing to pair with shrink away to (or grow from) their
//! centre. Pictures, text and paint can't be morphed; they hold until the
//! swap.

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, insert_anchor, nearest_on_anchors, path_from_anchors};
use crate::color::Color;
use crate::document::{Element, Layer};
use crate::easing::Easing;
use crate::geometry::Point;
use crate::shape::{Shape, Stroke, Style};

/// Morphs into the drawing swapped in at frame `end`, from the one showing
/// before it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShapeTween {
    pub end: u32,
    #[serde(default)]
    pub easing: Easing,
}

impl Layer {
    /// The shape tween running on `frame`, if any: where its span starts,
    /// where it ends (the swap), and the tween.
    pub fn shape_tween_at(&self, frame: u32) -> Option<(u32, u32, &ShapeTween)> {
        let next = self.exposure.iter().find(|key| key.frame > frame)?;
        let tween = self.shape_tweens.iter().find(|t| t.end == next.frame)?;
        let start =
            self.exposure.iter().take_while(|key| key.frame <= frame).last().map_or(self.start, |k| k.frame);
        Some((start.min(frame), next.frame, tween))
    }

    /// Starts or stops morphing through the span `frame` is in, into the
    /// next swap. Returns false if there's no swap after `frame` to morph
    /// into.
    pub fn set_shape_tween(&mut self, frame: u32, on: bool) -> bool {
        let Some(end) = self.exposure.iter().find(|key| key.frame > frame).map(|k| k.frame) else {
            return false;
        };
        self.shape_tweens.retain(|t| t.end != end);
        if on {
            self.shape_tweens.push(ShapeTween { end, easing: Easing::Linear });
            self.shape_tweens.sort_by_key(|t| t.end);
        }
        true
    }

    /// The shape tween's settings for the span `frame` is in, to edit.
    pub fn shape_tween_mut(&mut self, frame: u32) -> Option<&mut ShapeTween> {
        let end = self.shape_tween_at(frame)?.1;
        self.shape_tweens.iter_mut().find(|t| t.end == end)
    }

    /// The morphed shapes on `frame`, if a shape tween is part way through
    /// there. On the span's first frame the drawing shows as it is.
    pub fn morph_at(&self, frame: u32) -> Option<Vec<Element>> {
        let (start, end, tween) = self.shape_tween_at(frame)?;
        if frame <= start {
            return None;
        }
        let from = &self.drawings[self.drawing_index_at(start)];
        let to = &self.drawings[self.drawing_index_at(end)];
        let t = tween.easing.apply((frame - start) as f32 / (end - start) as f32);
        Some(morph(&from.elements, &to.elements, (&from.hints, &to.hints), t))
    }
}

/// The shapes `t` of the way from `from` to `to`. `hints` are the two
/// drawings' shape hints, paired by position in their lists.
pub fn morph(from: &[Element], to: &[Element], hints: (&[Point], &[Point]), t: f32) -> Vec<Element> {
    let pairs = hints.0.len().min(hints.1.len());
    (0..from.len().max(to.len()))
        .filter_map(|i| match (from.get(i), to.get(i)) {
            (Some(a), Some(b)) => {
                // The hints that belong to this pair: the ones nearest its
                // outline on the first drawing.
                let mine: Vec<(Point, Point)> = (0..pairs)
                    .filter(|&h| nearest_element(from, hints.0[h]) == Some(i))
                    .map(|h| (hints.0[h], hints.1[h]))
                    .collect();
                Some(morph_element(a, b, &mine, t))
            }
            (Some(a), None) => vanish(a, t),
            (None, Some(b)) => vanish(b, 1.0 - t),
            (None, None) => None,
        })
        .collect()
}

/// One shape `t` of the way to another.
fn morph_element(a: &Element, b: &Element, hints: &[(Point, Point)], t: f32) -> Element {
    let (from, from_closed) = a.shape.to_anchors();
    let (to, to_closed) = b.shape.to_anchors();
    if from.len() < 2 || to.len() < 2 {
        // A picture, text or paint: no outline to bend.
        return if t < 1.0 { a.clone() } else { b.clone() };
    }
    // A closed shape morphing with an open one is traced as an open line
    // that ends back where it started.
    let closed = from_closed && to_closed;
    let open = |mut anchors: Vec<Anchor>, was_closed: bool| {
        if was_closed && !closed {
            anchors.push(anchors[0]);
        }
        anchors
    };
    let (from, to) = correspond(open(from, from_closed), open(to, to_closed), closed, hints);
    let mixed: Vec<Anchor> = from
        .iter()
        .zip(&to)
        .map(|(p, q)| Anchor {
            point: p.point.lerp(q.point, t),
            handle_in: p.handle_in.lerp(q.handle_in, t),
            handle_out: p.handle_out.lerp(q.handle_out, t),
        })
        .collect();
    let shape = path_from_anchors(&mixed, closed).map_or_else(|| a.shape.clone(), Shape::Path);
    Element { shape, style: blend_style(&a.style, &b.style, t) }
}

/// A shape with no partner, `amount` of the way to gone: shrunk towards
/// its centre and faded.
fn vanish(element: &Element, amount: f32) -> Option<Element> {
    if amount >= 1.0 {
        return None;
    }
    let (anchors, closed) = element.shape.to_anchors();
    if anchors.len() < 2 {
        return (amount < 0.5).then(|| element.clone());
    }
    let centre = centroid(&anchors);
    let keep = 1.0 - amount;
    let toward = |p: Point| centre.lerp(p, keep);
    let shrunk: Vec<Anchor> = anchors
        .iter()
        .map(|a| Anchor {
            point: toward(a.point),
            handle_in: toward(a.handle_in),
            handle_out: toward(a.handle_out),
        })
        .collect();
    let fade = |c: Color| Color { a: (c.a as f32 * keep).round() as u8, ..c };
    let style = Style {
        fill: element.style.fill.map(fade),
        stroke: element.style.stroke.map(|s| Stroke { color: fade(s.color), ..s }),
    };
    Some(Element { shape: Shape::Path(path_from_anchors(&shrunk, closed)?), style })
}

/// The fewest anchors a closed shape (or an open line) is morphed with.
const MIN_CLOSED_ANCHORS: usize = 16;
const MIN_OPEN_ANCHORS: usize = 8;

/// Makes two anchor lists the same length and lines them up, so anchor
/// `i` of one morphs into anchor `i` of the other.
fn correspond(
    mut a: Vec<Anchor>,
    mut b: Vec<Anchor>,
    closed: bool,
    hints: &[(Point, Point)],
) -> (Vec<Anchor>, Vec<Anchor>) {
    if !hints.is_empty()
        && let Some(pinned) = pin_hints(&a, &b, closed, hints)
    {
        return pinned;
    }
    // Enough anchors that each can find a partner in about the same
    // direction from the centre: with only a square's four corners and an
    // ellipse's four sides, the shape would twist a quarter turn on the way.
    let count = a.len().max(b.len()).max(if closed { MIN_CLOSED_ANCHORS } else { MIN_OPEN_ANCHORS });
    while a.len() < count {
        a = split_longest(&a, closed, 0..segments(&a, closed));
    }
    while b.len() < count {
        b = split_longest(&b, closed, 0..segments(&b, closed));
    }
    let b = best_match(&a, b, closed);
    (with_handles(a, closed), with_handles(b, closed))
}

/// Gives straight segments handles a third of the way along them: the
/// same line, written as a curve. Blended with a curve's handles, a
/// straight side then bends evenly, rather than staying flat near its ends
/// and dimpling.
fn with_handles(mut anchors: Vec<Anchor>, closed: bool) -> Vec<Anchor> {
    let n = anchors.len();
    for i in 0..segments(&anchors, closed) {
        let j = (i + 1) % n;
        let (p, q) = (anchors[i].point, anchors[j].point);
        if anchors[i].handle_out == p && anchors[j].handle_in == q {
            anchors[i].handle_out = p.lerp(q, 1.0 / 3.0);
            anchors[j].handle_in = q.lerp(p, 1.0 / 3.0);
        }
    }
    anchors
}

/// Lines the shapes up by their hints: each hint becomes an anchor on both
/// shapes, closed shapes start at the first hint, and each stretch between
/// two hints gets the same number of anchors on both. `None` if the hints
/// don't run round the shapes in the same order, which can't be matched.
fn pin_hints(
    a: &[Anchor],
    b: &[Anchor],
    closed: bool,
    hints: &[(Point, Point)],
) -> Option<(Vec<Anchor>, Vec<Anchor>)> {
    let (a, a_at) = anchor_at_each(a, closed, hints.iter().map(|h| h.0))?;
    let (b, b_at) = anchor_at_each(b, closed, hints.iter().map(|h| h.1))?;
    // Closed shapes start at the first hint, so the stretches don't wrap.
    let (a, a_at) = if closed { rotate_to(&a, &a_at, a_at[0]) } else { (a, a_at) };
    let (b, b_at) = if closed { rotate_to(&b, &b_at, b_at[0]) } else { (b, b_at) };
    let ordered = |at: &[usize]| at.windows(2).all(|w| w[0] < w[1]);
    if !ordered(&a_at) || !ordered(&b_at) {
        return None;
    }

    // The stretches: before the first hint (open shapes only), between
    // hints, and after the last. Matched from the end, so splitting a
    // stretch doesn't move the ones still to do.
    let bounds = |at: &[usize], len: usize| {
        let mut cuts = vec![0];
        cuts.extend(at.iter().copied().filter(|&i| i > 0));
        cuts.push(if closed { len } else { len - 1 });
        cuts.dedup();
        cuts
    };
    let (mut a, mut b) = (a, b);
    let (a_cuts, b_cuts) = (bounds(&a_at, a.len()), bounds(&b_at, b.len()));
    if a_cuts.len() != b_cuts.len() {
        return None;
    }
    for stretch in (0..a_cuts.len() - 1).rev() {
        let (a_range, b_range) = (a_cuts[stretch]..a_cuts[stretch + 1], b_cuts[stretch]..b_cuts[stretch + 1]);
        let (mut na, mut nb) = (a_range.len(), b_range.len());
        while na < nb {
            a = split_longest(&a, closed, a_range.start..a_range.start + na);
            na += 1;
        }
        while nb < na {
            b = split_longest(&b, closed, b_range.start..b_range.start + nb);
            nb += 1;
        }
    }
    Some((with_handles(a, closed), with_handles(b, closed)))
}

/// Adds an anchor on the outline nearest each point (or uses an anchor
/// already there), returning the anchors and where each point's anchor is.
fn anchor_at_each(
    anchors: &[Anchor],
    closed: bool,
    points: impl Iterator<Item = Point>,
) -> Option<(Vec<Anchor>, Vec<usize>)> {
    let mut anchors = anchors.to_vec();
    let mut pins: Vec<Point> = Vec::new();
    for point in points {
        let hit = nearest_on_anchors(&anchors, closed, point)?;
        let near = |p: Point| p.distance(hit.point) < 0.5;
        if !anchors.iter().any(|a| near(a.point)) {
            anchors = insert_anchor(&anchors, closed, hit.segment, hit.t);
        }
        pins.push(hit.point);
    }
    let at = pins
        .iter()
        .map(|&p| {
            (0..anchors.len())
                .min_by(|&i, &j| anchors[i].point.distance(p).total_cmp(&anchors[j].point.distance(p)))
        })
        .collect::<Option<Vec<usize>>>()?;
    Some((anchors, at))
}

/// A closed shape's anchors starting from `first`, and the pinned indices
/// renumbered to match.
fn rotate_to(anchors: &[Anchor], at: &[usize], first: usize) -> (Vec<Anchor>, Vec<usize>) {
    let n = anchors.len();
    let rotated = (0..n).map(|i| anchors[(i + first) % n]).collect();
    (rotated, at.iter().map(|&i| (i + n - first) % n).collect())
}

/// How many segments a path through `anchors` has.
fn segments(anchors: &[Anchor], closed: bool) -> usize {
    if closed { anchors.len() } else { anchors.len() - 1 }
}

/// Splits the longest of the segments in `range` in half.
fn split_longest(anchors: &[Anchor], closed: bool, range: std::ops::Range<usize>) -> Vec<Anchor> {
    let n = anchors.len();
    let length = |i: usize| {
        let (p, q) = (&anchors[i], &anchors[(i + 1) % n]);
        // The control polygon: never shorter than the curve, and close
        // enough to rank segments by.
        p.point.distance(p.handle_out) + p.handle_out.distance(q.handle_in) + q.handle_in.distance(q.point)
    };
    let range = range.start..range.end.min(segments(anchors, closed));
    match range.clone().max_by(|&i, &j| length(i).total_cmp(&length(j))) {
        Some(longest) => insert_anchor(anchors, closed, longest, 0.5),
        None => anchors.to_vec(),
    }
}

/// `b` turned round (closed shapes) and maybe reversed so its anchors sit
/// as close as they can to `a`'s, each measured from its shape's centre.
fn best_match(a: &[Anchor], b: Vec<Anchor>, closed: bool) -> Vec<Anchor> {
    let (ca, cb) = (centroid(a), centroid(&b));
    let cost = |candidate: &[Anchor]| -> f32 {
        a.iter().zip(candidate).map(|(p, q)| (p.point - ca).distance(q.point - cb).powi(2)).sum()
    };
    let reversed: Vec<Anchor> = b
        .iter()
        .rev()
        .map(|a| Anchor { point: a.point, handle_in: a.handle_out, handle_out: a.handle_in })
        .collect();
    let n = b.len();
    let shifts = if closed { n } else { 1 };
    let mut best = (f32::INFINITY, b.clone());
    for order in [&b, &reversed] {
        for shift in 0..shifts {
            let candidate: Vec<Anchor> = (0..n).map(|i| order[(i + shift) % n]).collect();
            let c = cost(&candidate);
            if c < best.0 {
                best = (c, candidate);
            }
        }
    }
    best.1
}

fn centroid(anchors: &[Anchor]) -> Point {
    let sum = anchors.iter().fold(Point::ZERO, |sum, a| sum + a.point);
    sum * (1.0 / anchors.len().max(1) as f32)
}

/// Which element's outline is nearest `point`.
fn nearest_element(elements: &[Element], point: Point) -> Option<usize> {
    let distance = |e: &Element| {
        let (anchors, closed) = e.shape.to_anchors();
        nearest_on_anchors(&anchors, closed, point).map_or(f32::INFINITY, |hit| hit.distance)
    };
    (0..elements.len()).min_by(|&i, &j| distance(&elements[i]).total_cmp(&distance(&elements[j])))
}

fn blend_style(a: &Style, b: &Style, t: f32) -> Style {
    // A colour on one side only fades in or out in that colour.
    let blend = |x: Option<Color>, y: Option<Color>| match (x, y) {
        (Some(x), Some(y)) => Some(lerp_color(x, y, t)),
        (Some(x), None) => Some(lerp_color(x, Color { a: 0, ..x }, t)),
        (None, Some(y)) => Some(lerp_color(Color { a: 0, ..y }, y, t)),
        (None, None) => None,
    };
    let stroke = match (a.stroke, b.stroke) {
        (None, None) => None,
        (x, y) => {
            // A stroke on one side only keeps its width while it fades.
            let width = match (x, y) {
                (Some(x), Some(y)) => x.width + (y.width - x.width) * t,
                (Some(s), None) | (None, Some(s)) => s.width,
                (None, None) => 0.0,
            };
            blend(x.map(|s| s.color), y.map(|s| s.color)).map(|color| Stroke { color, width })
        }
    };
    Style { fill: blend(a.fill, b.fill), stroke }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color { r: mix(a.r, b.r), g: mix(a.g, b.g), b: mix(a.b, b.b), a: mix(a.a, b.a) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Drawing, Rect};

    fn element(shape: Shape, fill: Color) -> Element {
        Element { shape, style: Style { fill: Some(fill), stroke: None } }
    }

    fn square(side: f32) -> Element {
        element(Shape::Rect { width: side, height: side, corner_radius: 0.0 }, Color::rgb(255, 0, 0))
    }

    fn circle(radius: f32) -> Element {
        element(Shape::Ellipse { radius_x: radius, radius_y: radius }, Color::rgb(0, 0, 255))
    }

    fn bounds(elements: &[Element]) -> Rect {
        elements.iter().map(Element::bounds).reduce(Rect::union).unwrap()
    }

    #[test]
    fn a_square_becomes_a_circle_through_shapes_in_between() {
        let (from, to) = ([square(100.0)], [circle(100.0)]);
        let start = morph(&from, &to, (&[], &[]), 0.0);
        let middle = morph(&from, &to, (&[], &[]), 0.5);
        let end = morph(&from, &to, (&[], &[]), 1.0);
        // Exactly the two shapes at the ends, as far as anyone can see.
        assert!((bounds(&start).width() - 100.0).abs() < 1e-3);
        assert!((bounds(&end).width() - 200.0).abs() < 1e-3);
        assert!((bounds(&middle).width() - 150.0).abs() < 1.0);
        // Colours blend too.
        assert_eq!(middle[0].style.fill, Some(Color::rgb(128, 0, 128)));
    }

    #[test]
    fn shapes_with_no_partner_shrink_away() {
        let from = [square(100.0), square(20.0)];
        let to = [square(100.0)];
        let middle = morph(&from, &to, (&[], &[]), 0.5);
        assert_eq!(middle.len(), 2);
        assert!((middle[1].bounds().width() - 10.0).abs() < 1e-3);
        assert_eq!(middle[1].style.fill.unwrap().a, 128);
        assert_eq!(morph(&from, &to, (&[], &[]), 1.0).len(), 1);
    }

    #[test]
    fn anchors_pair_with_the_nearest_after_lining_the_shapes_up() {
        // The same square, its anchors listed from a different corner:
        // nothing should move.
        let a: Vec<Anchor> =
            Rect::from_center_size(Point::ZERO, 10.0, 10.0).corners().map(Anchor::corner).to_vec();
        let mut b = a.clone();
        b.rotate_left(2);
        let (a2, b2) = correspond(a.clone(), b, true, &[]);
        assert_eq!(a2, b2);
    }

    #[test]
    fn hints_decide_what_goes_where() {
        // Without hints, a square turned a quarter round would just stay
        // put. With the top-left corner hinted to the top-right one, every
        // corner moves one place clockwise.
        let from = [square(100.0)];
        let to = [square(100.0)];
        let top_left = Point::new(-50.0, -50.0);
        let top_right = Point::new(50.0, -50.0);
        let middle = morph(&from, &to, (&[top_left], &[top_right]), 0.5);
        let Shape::Path(path) = &middle[0].shape else { panic!("a path") };
        // The corner that started at the top left is half way across.
        assert!(path.flatten().iter().any(|p| p.distance(Point::new(0.0, -50.0)) < 1e-3));
    }

    fn mouth() -> Layer {
        let mut doc = crate::Document::default();
        let id = doc.add_layer("Mouth", vec![square(100.0)], 0, crate::Transform::default());
        let mut layer = doc.layer(id).unwrap().clone();
        let open = layer.add_drawing(Drawing::new("open", vec![circle(100.0)]));
        layer.expose(10, open);
        layer
    }

    #[test]
    fn a_shape_tween_morphs_the_frames_before_its_swap() {
        let mut layer = mouth();
        assert!(layer.morph_at(5).is_none(), "no tween yet: hold");
        assert!(layer.set_shape_tween(3, true));
        assert_eq!(layer.shape_tween_at(5).map(|(s, e, _)| (s, e)), Some((0, 10)));
        assert!(layer.morph_at(0).is_none(), "the first frame shows the drawing itself");
        let middle = layer.morph_at(5).unwrap();
        assert!((bounds(&middle).width() - 150.0).abs() < 1.0);
        assert!(layer.morph_at(10).is_none(), "the swap shows the new drawing");
        // And it's what the layer shows.
        assert_eq!(layer.shown_elements(5).as_ref(), middle.as_slice());
        assert!(!layer.set_shape_tween(12, true), "nothing to morph into after the last swap");
    }

    #[test]
    fn removing_the_swap_removes_its_tween() {
        let mut layer = mouth();
        layer.set_shape_tween(0, true);
        layer.clear_exposure(10);
        assert!(layer.shape_tweens.is_empty());
    }
}
