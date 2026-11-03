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
//!
//! That's the Linear blend, Animate's. A tween can instead use the Natural
//! blend, which pairs shapes by likeness, matches outlines corner to
//! corner, and bends sides rather than sliding points (see `natural.rs`).

mod natural;

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, insert_anchor, nearest_on_anchors, path_from_anchors};
use crate::color::Color;
use crate::document::{Element, Layer};
use crate::drawing::Drawing;
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
    #[serde(default, skip_serializing_if = "ShapeBlend::is_linear")]
    pub blend: ShapeBlend,
}

/// How a shape tween works out the shapes in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeBlend {
    /// As Animate does: shapes pair in drawing order and points move in
    /// straight lines.
    #[default]
    Linear,
    /// Shapes pair by likeness, outlines match corner to corner, and sides
    /// turn rather than slide (see `natural.rs`).
    Natural,
}

impl ShapeBlend {
    fn is_linear(&self) -> bool {
        *self == ShapeBlend::Linear
    }
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
            self.shape_tweens.push(ShapeTween { end, easing: Easing::Linear, blend: ShapeBlend::Linear });
            self.shape_tweens.sort_by_key(|t| t.end);
        }
        true
    }

    /// The shape tween's settings for the span `frame` is in, to edit.
    pub fn shape_tween_mut(&mut self, frame: u32) -> Option<&mut ShapeTween> {
        let end = self.shape_tween_at(frame)?.1;
        self.shape_tweens.iter_mut().find(|t| t.end == end)
    }

    /// Draws the inbetweens for the span `frame` is in, as an assistant
    /// would: a new drawing on every frame from the one after the span's
    /// key drawing up to the next swap, each the key drawing morphed that
    /// far toward the next (with the shape hints, and spaced by the span's
    /// shape tween easing if it has one). Being drawings, each can then be
    /// touched up by hand; the shape tween, if any, is done with. Returns
    /// how many it drew: none if there's no next swap, or no frame between.
    pub fn make_inbetweens(&mut self, frame: u32) -> usize {
        let Some(end) = self.exposure.iter().find(|key| key.frame > frame).map(|k| k.frame) else { return 0 };
        let start =
            self.exposure.iter().take_while(|key| key.frame <= frame).last().map_or(self.start, |k| k.frame);
        if end <= start + 1 {
            return 0;
        }
        let tween = self.shape_tweens.iter().find(|t| t.end == end);
        let easing = tween.map_or(Easing::Linear, |t| t.easing);
        let blend = tween.map_or(ShapeBlend::Linear, |t| t.blend);
        let from = self.drawings[self.drawing_index_at(start)].clone();
        let to = self.drawings[self.drawing_index_at(end)].clone();
        let hints = (from.hints.as_slice(), to.hints.as_slice());
        for f in start + 1..end {
            let t = easing.apply((f - start) as f32 / (end - start) as f32);
            let mut drawing = Drawing::new(
                self.next_drawing_name(),
                morph_with(&from.elements, &to.elements, hints, t, blend),
            );
            drawing.colour_art = morph_with(&from.colour_art, &to.colour_art, (&[], &[]), t, blend);
            let index = self.add_drawing(drawing);
            self.expose(f, index);
        }
        self.shape_tweens.retain(|t| t.end != end);
        (end - start - 1) as usize
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
        // Each drawing as posed (see `point_keys.rs`): the one morphing from
        // as it is now, the one morphing into as it'll be on arriving.
        Some(morph_with(&from.shown_at(frame), &to.shown_at(end), (&from.hints, &to.hints), t, tween.blend))
    }
}

/// The shapes `t` of the way from `from` to `to`, blended linearly.
/// `hints` are the two drawings' shape hints, paired by position in their
/// lists.
pub fn morph(from: &[Element], to: &[Element], hints: (&[Point], &[Point]), t: f32) -> Vec<Element> {
    morph_with(from, to, hints, t, ShapeBlend::Linear)
}

/// Like [`morph`], blended as `blend` says.
pub fn morph_with(
    from: &[Element],
    to: &[Element],
    hints: (&[Point], &[Point]),
    t: f32,
    blend: ShapeBlend,
) -> Vec<Element> {
    let pairs: Vec<(Option<usize>, Option<usize>)> = match blend {
        ShapeBlend::Linear => (0..from.len().max(to.len()))
            .map(|i| ((i < from.len()).then_some(i), (i < to.len()).then_some(i)))
            .collect(),
        ShapeBlend::Natural => natural::pair_shapes(from, to, t),
    };
    let hinted = hints.0.len().min(hints.1.len());
    pairs
        .into_iter()
        .filter_map(|pair| match pair {
            (Some(i), Some(j)) => {
                // The hints that belong to this pair: the ones nearest its
                // outline on the first drawing.
                let mine: Vec<(Point, Point)> = (0..hinted)
                    .filter(|&h| nearest_element(from, hints.0[h]) == Some(i))
                    .map(|h| (hints.0[h], hints.1[h]))
                    .collect();
                Some(morph_element(&from[i], &to[j], &mine, t, blend))
            }
            (Some(i), None) => vanish(&from[i], t),
            (None, Some(j)) => vanish(&to[j], 1.0 - t),
            (None, None) => None,
        })
        .collect()
}

/// One shape `t` of the way to another.
fn morph_element(a: &Element, b: &Element, hints: &[(Point, Point)], t: f32, blend: ShapeBlend) -> Element {
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
    let (from, to) = (open(from, from_closed), open(to, to_closed));
    let (from, to) = match blend {
        // Hints placed by hand always decide; otherwise Natural places its
        // own where the outlines' features match.
        ShapeBlend::Natural if hints.is_empty() => {
            let (found, reversed) = natural::feature_hints(&from, &to, closed);
            let to = if reversed { reverse(&to) } else { to };
            pin_hints(&from, &to, closed, &found).unwrap_or_else(|| correspond(from, to, closed, &[]))
        }
        _ => correspond(from, to, closed, hints),
    };
    let linear = || -> Vec<Anchor> {
        from.iter()
            .zip(&to)
            .map(|(p, q)| Anchor {
                point: p.point.lerp(q.point, t),
                handle_in: p.handle_in.lerp(q.handle_in, t),
                handle_out: p.handle_out.lerp(q.handle_out, t),
            })
            .collect()
    };
    let mixed = match blend {
        ShapeBlend::Linear => linear(),
        ShapeBlend::Natural => natural::blend_intrinsic(&from, &to, closed, t).unwrap_or_else(linear),
    };
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
        gradient: element.style.gradient.clone().map(|mut g| {
            g.stops.iter_mut().for_each(|stop| stop.color = fade(stop.color));
            g
        }),
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
    let reversed = reverse(&b);
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

/// The same outline, run the other way.
fn reverse(anchors: &[Anchor]) -> Vec<Anchor> {
    anchors
        .iter()
        .rev()
        .map(|a| Anchor { point: a.point, handle_in: a.handle_out, handle_out: a.handle_in })
        .collect()
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
            // Ends, corners and dashes can't blend; they switch halfway.
            let look = if t < 0.5 { x.or(y) } else { y.or(x) };
            blend(x.map(|s| s.color), y.map(|s| s.color)).map(|color| Stroke {
                color,
                width,
                ..look.unwrap_or(Stroke::new(color, width))
            })
        }
    };
    Style { fill: blend(a.fill, b.fill), stroke, gradient: blend_gradient(a, b, t) }
}

/// A gradient partway between two shapes' fills: blended stop by stop when
/// both have one of the same kind with as many stops, otherwise whichever
/// is nearer.
fn blend_gradient(a: &Style, b: &Style, t: f32) -> Option<crate::ShapeGradient> {
    match (&a.gradient, &b.gradient) {
        (Some(x), Some(y)) if x.shape == y.shape && x.stops.len() == y.stops.len() => {
            let mix = |p: Point, q: Point| p + (q - p) * t;
            let stops = x
                .stops
                .iter()
                .zip(&y.stops)
                .map(|(p, q)| crate::GradientStop {
                    at: p.at + (q.at - p.at) * t,
                    color: lerp_color(p.color, q.color, t),
                })
                .collect();
            Some(crate::ShapeGradient {
                shape: x.shape,
                stops,
                from: mix(x.from, y.from),
                to: mix(x.to, y.to),
            })
        }
        (x, y) => {
            if t < 0.5 {
                x.clone()
            } else {
                y.clone()
            }
        }
    }
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color { r: mix(a.r, b.r), g: mix(a.g, b.g), b: mix(a.b, b.b), a: mix(a.a, b.a) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Drawing, Rect, path_from_anchors};

    fn element(shape: Shape, fill: Color) -> Element {
        Element { shape, style: Style { fill: Some(fill), stroke: None, gradient: None } }
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
    fn inbetweens_are_drawn_on_every_frame_between_two_keys() {
        let mut doc = crate::Document::default();
        let id = doc.add_layer("Ball", vec![square(20.0)], 0, crate::Transform::default());
        let mut layer = doc.layer(id).unwrap().clone();
        let big = layer.add_drawing(Drawing::new("2", vec![square(60.0)]));
        layer.expose(4, big);
        layer.set_shape_tween(0, true);
        assert_eq!(layer.make_inbetweens(0), 3);
        // A drawing of its own on each frame between, growing evenly.
        let widths: Vec<f32> = (0..=4).map(|f| bounds(layer.elements_at(f)).width()).collect();
        for (w, want) in widths.iter().zip([20.0, 30.0, 40.0, 50.0, 60.0]) {
            assert!((w - want).abs() < 0.5, "{widths:?}");
        }
        assert_eq!(layer.drawings.len(), 5);
        assert!(layer.shape_tweens.is_empty(), "the tween's done with");
        // And none to draw where the keys are next to each other.
        assert_eq!(layer.make_inbetweens(3), 0);
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

    /// A closed polygon through `points`, in `fill`.
    fn polygon(points: &[Point], fill: Color) -> Element {
        let anchors: Vec<Anchor> = points.iter().copied().map(Anchor::corner).collect();
        element(Shape::Path(path_from_anchors(&anchors, true).unwrap()), fill)
    }

    /// A thin stick 100 long hanging from the origin, turned `angle` from
    /// straight down.
    fn stick(angle: f32) -> Element {
        let (s, c) = angle.sin_cos();
        let turn = |x: f32, y: f32| Point::new(x * c - y * s, x * s + y * c);
        polygon(&[turn(-4.0, 0.0), turn(4.0, 0.0), turn(4.0, 100.0), turn(-4.0, 100.0)], Color::BLACK)
    }

    fn outline(element: &Element) -> Vec<Point> {
        let Shape::Path(path) = &element.shape else { panic!("a path") };
        path.flatten()
    }

    /// How far apart two closed outlines are at most: each point of one
    /// from the nearest side of the other.
    fn apart(a: &Element, b: &Element) -> f32 {
        let (pa, pb) = (outline(a), outline(b));
        let nearest = |p: &Point, other: &[Point]| {
            (0..other.len())
                .map(|i| p.distance_to_segment(other[i], other[(i + 1) % other.len()]))
                .fold(f32::MAX, f32::min)
        };
        pa.iter().map(|p| nearest(p, &pb)).chain(pb.iter().map(|p| nearest(p, &pa))).fold(0.0, f32::max)
    }

    #[test]
    fn natural_tweens_start_and_end_on_the_drawings() {
        let cases = [
            (square(100.0), circle(60.0)),
            (stick(0.3), stick(-1.4)),
            (
                polygon(
                    &[Point::new(0.0, -50.0), Point::new(40.0, 30.0), Point::new(-40.0, 30.0)],
                    Color::BLACK,
                ),
                square(80.0),
            ),
        ];
        for (from, to) in cases {
            let (a, b) = (std::slice::from_ref(&from), std::slice::from_ref(&to));
            let start = morph_with(a, b, (&[], &[]), 0.0, ShapeBlend::Natural);
            let end = morph_with(a, b, (&[], &[]), 1.0, ShapeBlend::Natural);
            let as_path = |e: &Element| {
                let (anchors, closed) = e.shape.to_anchors();
                element(Shape::Path(path_from_anchors(&anchors, closed).unwrap()), Color::BLACK)
            };
            assert!(apart(&start[0], &as_path(&from)) < 0.5, "starts on the first drawing");
            assert!(apart(&end[0], &as_path(&to)) < 0.5, "ends on the second");
        }
    }

    #[test]
    fn a_turning_part_keeps_its_length_in_a_natural_tween() {
        let (from, to) = ([stick(0.3)], [stick(-1.4)]);
        // The farthest apart two points of it are: end corner to corner.
        let length = |shapes: &[Element]| {
            let points = outline(&shapes[0]);
            points.iter().flat_map(|p| points.iter().map(move |q| p.distance(*q))).fold(0.0, f32::max)
        };
        let natural = morph_with(&from, &to, (&[], &[]), 0.5, ShapeBlend::Natural);
        let linear = morph_with(&from, &to, (&[], &[]), 0.5, ShapeBlend::Linear);
        // About 100 long (and 8 wide) all the way round...
        assert!((length(&natural) - 100.3).abs() < 3.0, "{}", length(&natural));
        // ...where moving its points in straight lines shrinks it.
        assert!(length(&linear) < 80.0, "{}", length(&linear));
    }

    #[test]
    fn natural_tweens_pair_shapes_by_likeness_not_order() {
        let head = |x: f32| {
            let (anchors, _) = Shape::Ellipse { radius_x: 15.0, radius_y: 15.0 }.to_anchors();
            let moved: Vec<Anchor> = anchors
                .iter()
                .map(|a| {
                    let by = Point::new(x, -60.0);
                    Anchor { point: a.point + by, handle_in: a.handle_in + by, handle_out: a.handle_out + by }
                })
                .collect();
            element(Shape::Path(path_from_anchors(&moved, true).unwrap()), Color::rgb(255, 160, 0))
        };
        let body = |x: f32| {
            let corners = Rect::from_center_size(Point::new(x, 20.0), 60.0, 80.0).corners();
            polygon(&corners, Color::rgb(0, 0, 255))
        };
        // The head's listed first in one drawing and last in the other.
        let from = [head(0.0), body(0.0)];
        let to = [body(10.0), head(10.0)];
        let middle = morph_with(&from, &to, (&[], &[]), 0.5, ShapeBlend::Natural);
        assert_eq!(middle.len(), 2);
        let small = middle.iter().find(|e| e.bounds().width() < 40.0).expect("the head stays a head");
        assert_eq!(small.style.fill, Some(Color::rgb(255, 160, 0)), "and stays orange");
        // Linear, by contrast, turns the head into the body.
        let linear = morph_with(&from, &to, (&[], &[]), 0.5, ShapeBlend::Linear);
        assert!(linear.iter().all(|e| e.style.fill != Some(Color::rgb(255, 160, 0))));
    }

    #[test]
    fn natural_tweens_let_unlike_shapes_shrink_away_rather_than_pair() {
        // A small red dot far to one side and a big blue square: nothing
        // alike, so the dot goes and the square grows in.
        let dot = polygon(
            &Rect::from_center_size(Point::new(-300.0, 0.0), 6.0, 6.0).corners(),
            Color::rgb(255, 0, 0),
        );
        let big = polygon(
            &Rect::from_center_size(Point::new(300.0, 0.0), 200.0, 200.0).corners(),
            Color::rgb(0, 0, 255),
        );
        let middle = morph_with(&[dot], &[big], (&[], &[]), 0.5, ShapeBlend::Natural);
        assert_eq!(middle.len(), 2, "one fading out, one in");
    }

    #[test]
    fn hand_placed_hints_still_decide_a_natural_tween() {
        let from = [square(100.0)];
        let to = [square(100.0)];
        let (top_left, top_right) = (Point::new(-50.0, -50.0), Point::new(50.0, -50.0));
        let middle = morph_with(&from, &to, (&[top_left], &[top_right]), 0.5, ShapeBlend::Natural);
        // Turned an eighth of the way round, the top left corner has moved
        // up and right, rather than staying put as it would unhinted.
        let path = outline(&middle[0]);
        assert!(path.iter().all(|p| p.distance(top_left) > 5.0), "nothing left at the old corner");
    }

    #[test]
    fn the_blend_is_saved_only_when_its_natural() {
        let linear = ShapeTween { end: 10, easing: Easing::Linear, blend: ShapeBlend::Linear };
        let natural = ShapeTween { blend: ShapeBlend::Natural, ..linear };
        assert!(!serde_json::to_string(&linear).unwrap().contains("blend"));
        let json = serde_json::to_string(&natural).unwrap();
        assert!(json.contains(r#""blend":"natural""#));
        assert_eq!(serde_json::from_str::<ShapeTween>(&json).unwrap(), natural);
        // Tweens saved before there was a choice are Linear.
        let old: ShapeTween = serde_json::from_str(r#"{"end":10}"#).unwrap();
        assert_eq!(old.blend, ShapeBlend::Linear);
    }

    #[test]
    fn removing_the_swap_removes_its_tween() {
        let mut layer = mouth();
        layer.set_shape_tween(0, true);
        layer.clear_exposure(10);
        assert!(layer.shape_tweens.is_empty());
    }
}
