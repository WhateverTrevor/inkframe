//! Pressure-sensitive lines, as Harmony's pencil lines: a centreline with
//! its own width at each of its points.
//!
//! A [`Shape::Line`] keeps the centreline the pencil drew and how wide the
//! line is at each anchor (from how hard the pen pressed, or set with the
//! Width tool). Between anchors the width eases from one to the next. It's
//! drawn as the outline that sweeps out, filled with the element's fill,
//! and with round ends.
//!
//! Everything that works on outlines (bending, warping, morphing, filling
//! between lines, clicking) uses that outline, through
//! [`Shape::to_anchors`]; only the Width tool works on the centreline.

use crate::anchor::anchors_from_path;
use crate::geometry::Point;
use crate::shape::{Path, Segment, cubic_point};

/// How far apart the outline's points are along the line, at most.
const STEP: f32 = 2.0;
/// Points round each round end.
const CAP_POINTS: usize = 8;

/// The width at each anchor of `path`, given widths per anchor (missing
/// ones taking the last given).
fn width_at(widths: &[f32], anchor: usize) -> f32 {
    widths.get(anchor).or(widths.last()).copied().unwrap_or(1.0).max(0.0)
}

/// Points along the centreline with the line's width there, closely
/// enough spaced to follow its curves.
pub fn centreline_samples(path: &Path, widths: &[f32]) -> Vec<(Point, f32)> {
    let mut samples = vec![(path.start, width_at(widths, 0))];
    let mut from = path.start;
    for (i, segment) in path.segments.iter().enumerate() {
        let (w0, w1) = (width_at(widths, i), width_at(widths, i + 1));
        let to = segment.end();
        let length = match *segment {
            Segment::Line { to } => from.distance(to),
            Segment::Cubic { ctrl1, ctrl2, to } => {
                from.distance(ctrl1) + ctrl1.distance(ctrl2) + ctrl2.distance(to)
            }
        };
        let steps = ((length / STEP).ceil() as usize).clamp(1, 400);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let point = match *segment {
                Segment::Line { to } => from.lerp(to, t),
                Segment::Cubic { ctrl1, ctrl2, to } => cubic_point(from, ctrl1, ctrl2, to, t),
            };
            // Eased, so a width change doesn't show as a kink.
            let eased = t * t * (3.0 - 2.0 * t);
            samples.push((point, w0 + (w1 - w0) * eased));
        }
        from = to;
    }
    samples.dedup_by(|a, b| a.0.distance(b.0) < 1e-4);
    samples
}

/// The outline a line of these widths sweeps out along `path`, with round
/// ends, as a closed path.
pub fn line_outline(path: &Path, widths: &[f32]) -> Path {
    let samples = centreline_samples(path, widths);
    if samples.len() < 2 {
        // A dot: a circle of its width.
        let (centre, width) = samples.first().copied().unwrap_or((path.start, 1.0));
        let points = cap(centre, Point::new(1.0, 0.0), width / 2.0, std::f32::consts::TAU);
        return polygon(points);
    }
    // The direction along the line at each sample.
    let n = samples.len();
    let direction = |i: usize| {
        let (a, b) = (samples[i.saturating_sub(1)].0, samples[(i + 1).min(n - 1)].0);
        let d = b - a;
        let length = d.length().max(1e-6);
        Point::new(d.x / length, d.y / length)
    };
    let side = |i: usize, sign: f32| {
        let d = direction(i);
        let normal = Point::new(-d.y, d.x);
        samples[i].0 + normal * (sign * samples[i].1 / 2.0)
    };
    let mut points: Vec<Point> = (0..n).map(|i| side(i, 1.0)).collect();
    // Round the far end, from the left side over to the right.
    let end = direction(n - 1);
    let left_normal = Point::new(-end.y, end.x);
    points.extend(cap(samples[n - 1].0, left_normal, samples[n - 1].1 / 2.0, -std::f32::consts::PI));
    points.extend((0..n).rev().map(|i| side(i, -1.0)));
    let start = direction(0);
    let right_normal = Point::new(start.y, -start.x);
    points.extend(cap(samples[0].0, right_normal, samples[0].1 / 2.0, -std::f32::consts::PI));
    polygon(points)
}

/// Points round an arc of `radius` about `centre`, starting from
/// `from_direction` and turning `sweep` radians (clockwise when negative,
/// in screen terms), leaving out both ends.
fn cap(centre: Point, from_direction: Point, radius: f32, sweep: f32) -> Vec<Point> {
    let start = from_direction.y.atan2(from_direction.x);
    (1..CAP_POINTS)
        .map(|i| {
            let a = start + sweep * i as f32 / CAP_POINTS as f32;
            centre + Point::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

fn polygon(points: Vec<Point>) -> Path {
    let mut points = points.into_iter();
    let start = points.next().unwrap_or(Point::ZERO);
    Path { start, segments: points.map(|to| Segment::Line { to }).collect(), closed: true }
}

/// `path` with anchors added so none are more than about `spacing` apart,
/// keeping its exact shape: room for a line's width to change along it.
pub fn with_anchors_every(path: &Path, spacing: f32) -> Path {
    let anchors = crate::deform::subdivide(&anchors_from_path(path), path.closed, spacing.max(0.5));
    crate::anchor::path_from_anchors(&anchors, path.closed).unwrap_or_else(|| path.clone())
}

/// The centreline's anchors, for the Width tool.
pub fn centreline_anchors(path: &Path) -> Vec<Point> {
    anchors_from_path(path).into_iter().map(|a| a.point).collect()
}

/// The grain a layer's lines are drawn with, as Harmony's pencil line
/// textures: rather than flat ink, the lines show the tooth of the paper
/// through them. It's the layer's, so everything inked on it matches; the
/// grain is fixed to the layer, so it moves with the drawing rather than
/// crawling over it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LineTexture {
    pub kind: TextureKind,
    /// How strongly the grain shows, from 0 (flat) to 1.
    pub amount: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TextureKind {
    /// A fine, even grain: graphite on smooth paper.
    Pencil,
    /// Coarse and streaky, dragged along: charcoal on rough paper.
    Charcoal,
    /// Speckled with gaps where the paper shows: chalk.
    Chalk,
}

impl TextureKind {
    pub const ALL: [TextureKind; 3] = [TextureKind::Pencil, TextureKind::Charcoal, TextureKind::Chalk];

    pub fn label(self) -> &'static str {
        match self {
            TextureKind::Pencil => crate::i18n::tr_later("Pencil"),
            TextureKind::Charcoal => crate::i18n::tr_later("Charcoal"),
            TextureKind::Chalk => crate::i18n::tr_later("Chalk"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Shape, Style};

    fn straight(widths: Vec<f32>) -> Shape {
        let path = Path {
            start: Point::new(0.0, 0.0),
            segments: vec![Segment::Line { to: Point::new(100.0, 0.0) }],
            closed: false,
        };
        Shape::Line { path, widths }
    }

    #[test]
    fn a_line_is_as_wide_as_its_anchors_say_with_round_ends() {
        let line = straight(vec![10.0, 10.0]);
        let bounds = line.bounds();
        // 10 wide, and the round ends reach 5 past each end.
        assert!((bounds.height() - 10.0).abs() < 0.2, "{bounds:?}");
        assert!((bounds.min.x + 5.0).abs() < 0.2 && (bounds.max.x - 105.0).abs() < 0.2, "{bounds:?}");
    }

    #[test]
    fn widths_ease_from_one_anchor_to_the_next() {
        let samples = centreline_samples(
            &Path {
                start: Point::ZERO,
                segments: vec![Segment::Line { to: Point::new(100.0, 0.0) }],
                closed: false,
            },
            &[2.0, 20.0],
        );
        let middle =
            samples.iter().min_by(|a, b| (a.0.x - 50.0).abs().total_cmp(&(b.0.x - 50.0).abs())).unwrap();
        assert!((middle.1 - 11.0).abs() < 0.5, "{middle:?}");
        assert_eq!(samples.last().unwrap().1, 20.0);
    }

    #[test]
    fn a_line_is_hit_on_its_body_and_not_beside_it() {
        let line = straight(vec![10.0, 10.0]);
        let style = Style { fill: Some(Color::BLACK), stroke: None, gradient: None };
        assert!(line.hit_test(Point::new(50.0, 3.0), &style, 0.0));
        assert!(!line.hit_test(Point::new(50.0, 8.0), &style, 0.0));
    }

    #[test]
    fn its_anchors_are_the_outline_so_other_tools_see_its_shape() {
        let (anchors, closed) = straight(vec![10.0, 10.0]).to_anchors();
        assert!(closed && anchors.len() > 10);
    }
}
