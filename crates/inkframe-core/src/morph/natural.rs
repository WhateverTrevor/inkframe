//! The Natural blend: a shape tween that works out for itself what goes
//! where, and bends shapes rather than sliding their points.
//!
//! A Linear tween (the rest of `morph.rs`) pairs shapes by their order in
//! the drawing, lines anchors up by distance, and moves each point in a
//! straight line. That's Animate's shape tween, and it goes wrong in the
//! ways Animate's does: an eye drawn before the head in one drawing and
//! after it in the other morphs into the head; outlines slide round each
//! other instead of corner meeting corner; and anything that turns, like a
//! swinging arm, collapses through its middle on the way.
//!
//! Natural fixes each of those in turn:
//!
//! 1. [`pair_shapes`] pairs shapes by likeness (where they are, how big,
//!    what colour), choosing the pairing that's best overall, so the order
//!    they were drawn in doesn't matter.
//! 2. [`feature_hints`] matches the two outlines feature to feature,
//!    corners with corners, and places shape hints at the matches. The
//!    hints then line the anchors up exactly as hints placed by hand do.
//! 3. [`blend_intrinsic`] blends each side's length and direction rather
//!    than its ends' positions, as in Sederberg et al., "2-D Shape
//!    Blending: An Intrinsic Solution to the Vertex Path Problem" (1993).
//!    A side that turns keeps its length on the way round.
//!
//! None of it learns or guesses beyond the two drawings: a head turning
//! from the front to profile still has parts on one side that don't exist
//! on the other, and those still need hints.

use std::f32::consts::{PI, TAU};

use crate::anchor::Anchor;
use crate::color::Color;
use crate::document::Element;
use crate::geometry::{Point, Rect};

/// Which shape of the first drawing becomes which of the second. Each
/// entry is a pair, or a shape on one side only (which grows or shrinks
/// away), listed in the order they should be drawn.
pub(super) fn pair_shapes(from: &[Element], to: &[Element], t: f32) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (from.len(), to.len());
    if n == 0 || m == 0 {
        return (0..n).map(|i| (Some(i), None)).chain((0..m).map(|j| (None, Some(j)))).collect();
    }
    let (a, b): (Vec<Likeness>, Vec<Likeness>) =
        (from.iter().map(Likeness::of).collect(), to.iter().map(Likeness::of).collect());
    // Distances are measured against the size of both drawings together,
    // so the same layout scores the same however big it's drawn.
    let extent = a
        .iter()
        .chain(&b)
        .map(|l| l.bounds)
        .reduce(Rect::union)
        .map_or(1.0, |r| r.width().hypot(r.height()).max(1.0));
    let cost = |i: usize, j: usize| -> f32 {
        let (p, q) = (&a[i], &b[j]);
        if p.outlined != q.outlined {
            // A picture or text can't become a shape, or the other way.
            return f32::INFINITY;
        }
        let place = p.bounds.center().distance(q.bounds.center()) / extent;
        let size = (p.size / q.size).ln().abs();
        let colour = p.colour.zip(q.colour).map_or(0.5, |(x, y)| colour_distance(x, y));
        // Order breaks ties, so identical shapes keep their pairing.
        let order = (i as f32 / n as f32 - j as f32 / m as f32).abs();
        2.0 * place + 0.5 * size + colour + 0.1 * order
    };
    // A shape unlike anything on the other side does better growing or
    // shrinking away than turning into something it isn't.
    const ALONE: f32 = 1.2;
    let size = n + m;
    let mut matrix = vec![vec![0.0f32; size]; size];
    for (i, row) in matrix.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = match (i < n, j < m) {
                (true, true) => cost(i, j).min(1e6),
                (true, false) | (false, true) => ALONE,
                (false, false) => 0.0,
            };
        }
    }
    let assignment = assign(&matrix);

    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut to_paired = vec![false; m];
    for (i, &j) in assignment.iter().enumerate().take(n) {
        if j < m && matrix[i][j] < 1e6 {
            pairs.push((Some(i), Some(j)));
            to_paired[j] = true;
        } else {
            pairs.push((Some(i), None));
        }
    }
    pairs.extend((0..m).filter(|&j| !to_paired[j]).map(|j| (None, Some(j))));

    // Stacked as the first drawing has them until halfway, then as the
    // second does. A shape on one side only takes the place of its nearest
    // neighbour in the list it's missing from.
    let rank = |pair: &(Option<usize>, Option<usize>)| -> f32 {
        let (mine, theirs, len, other_len) =
            if t < 0.5 { (pair.0, pair.1, n, m) } else { (pair.1, pair.0, m, n) };
        match (mine, theirs) {
            (Some(index), _) => index as f32,
            (None, Some(other)) => (other as f32 + 0.5) * len as f32 / other_len as f32 - 0.5,
            (None, None) => 0.0,
        }
    };
    pairs.sort_by(|p, q| rank(p).total_cmp(&rank(q)));
    pairs
}

/// What a shape is like, for pairing.
struct Likeness {
    bounds: Rect,
    /// The side of a square of the same area as its bounds.
    size: f32,
    colour: Option<Color>,
    outlined: bool,
}

impl Likeness {
    fn of(element: &Element) -> Self {
        let bounds = element.bounds();
        let (anchors, _) = element.shape.to_anchors();
        Self {
            bounds,
            size: (bounds.width() * bounds.height()).sqrt().max(1.0),
            colour: element.style.fill.or(element.style.stroke.map(|s| s.color)),
            outlined: anchors.len() >= 2,
        }
    }
}

/// How different two colours look, 0 for the same to about 1 for black
/// and white.
fn colour_distance(x: Color, y: Color) -> f32 {
    let d = |p: u8, q: u8| (p as f32 - q as f32) / 255.0;
    // Weighted as the eye weighs them: green counts most, blue least.
    (0.3 * d(x.r, y.r).powi(2) + 0.59 * d(x.g, y.g).powi(2) + 0.11 * d(x.b, y.b).powi(2)).sqrt()
}

/// The cheapest way to give each row of a square cost matrix its own
/// column: the Hungarian algorithm (Kuhn and Munkres), in the compact form
/// that keeps a potential for each row and column. `result[row]` is the
/// row's column.
fn assign(cost: &[Vec<f32>]) -> Vec<usize> {
    let n = cost.len();
    // 1-based, with row and column 0 as the algorithm's starting point.
    let (mut u, mut v) = (vec![0.0f64; n + 1], vec![0.0f64; n + 1]);
    let mut owner = vec![0usize; n + 1]; // the row each column is given to
    let mut way = vec![0usize; n + 1];
    for row in 1..=n {
        owner[0] = row;
        let mut column = 0;
        let mut least = vec![f64::INFINITY; n + 1];
        let mut used = vec![false; n + 1];
        loop {
            used[column] = true;
            let current = owner[column];
            let (mut delta, mut next) = (f64::INFINITY, 0);
            for j in 1..=n {
                if used[j] {
                    continue;
                }
                let reduced = cost[current - 1][j - 1] as f64 - u[current] - v[j];
                if reduced < least[j] {
                    least[j] = reduced;
                    way[j] = column;
                }
                if least[j] < delta {
                    delta = least[j];
                    next = j;
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[owner[j]] += delta;
                    v[j] -= delta;
                } else {
                    least[j] -= delta;
                }
            }
            column = next;
            if owner[column] == 0 {
                break;
            }
        }
        // Hand each column on along the path that was found.
        while column != 0 {
            let previous = way[column];
            owner[column] = owner[previous];
            column = previous;
        }
    }
    let mut result = vec![0; n];
    for j in 1..=n {
        if owner[j] > 0 {
            result[owner[j] - 1] = j - 1;
        }
    }
    result
}

/// About how many points each outline is sampled at to compare them. Every
/// anchor is one of them, so a corner is always sampled exactly.
const SAMPLES: usize = 48;

/// Shape hints that line `b` up with `a` feature by feature, as pairs of
/// points on the two outlines, in order round `a`. The second value says
/// whether `b` runs the other way round and has to be reversed first.
/// Empty if the outlines are too small to compare.
pub(super) fn feature_hints(a: &[Anchor], b: &[Anchor], closed: bool) -> (Vec<(Point, Point)>, bool) {
    let (Some(sa), Some(sb)) = (Outline::sample(a, closed), Outline::sample(b, closed)) else {
        return (Vec::new(), false);
    };
    let reversed_b = sb.reversed();
    // Each way round, from each likely start: the match that costs least.
    let mut best: Option<(f32, Match)> = None;
    for (candidate, reversed) in [(&sb, false), (&reversed_b, true)] {
        let starts: Vec<usize> = if closed { likely_starts(&sa, candidate) } else { vec![0] };
        for start in starts {
            let (cost, path) = warp(&sa, candidate, start, closed);
            if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                best = Some((cost, Match { path, start, reversed }));
            }
        }
    }
    let Some((_, Match { path, start, reversed })) = best else { return (Vec::new(), false) };
    let sb = if reversed { reversed_b } else { sb };
    // The path counts `b` from where matching started.
    let m = sb.points.len();
    let at_b = |j: usize| (j + start) % m;

    // Hints at the corners of either outline, and every so often between
    // them so long smooth stretches stay matched too.
    let corner = 0.4; // radians of turning, about 23°
    let every = (sa.points.len() / 8).max(1);
    let mut hints: Vec<(usize, usize)> = Vec::new();
    for (k, &(i, j)) in path.iter().enumerate() {
        let even = i % every == 0 && (k == 0 || path[k - 1].0 != i);
        let wanted = even || sa.is_corner(i, corner) || sb.is_corner(at_b(j), corner);
        // One hint per sample of either outline, so they stay in order.
        if wanted && hints.last().is_none_or(|&(pi, pj)| i > pi && j > pj) {
            hints.push((i, j));
        }
    }
    (hints.into_iter().map(|(i, j)| (sa.points[i], sb.points[at_b(j)])).collect(), reversed)
}

/// One way of matching two outlines: the path through their samples (see
/// [`warp`]), where on the second it starts, and whether it runs the other
/// way round.
struct Match {
    path: Vec<(usize, usize)>,
    start: usize,
    reversed: bool,
}

/// An outline sampled at points about evenly spaced along it, with what's
/// needed to compare it with another.
struct Outline {
    points: Vec<Point>,
    /// Each point measured from the centre, in units of the outline's
    /// average distance from it, so a small square compares with a big one.
    shape: Vec<Point>,
    /// How sharply the outline turns at each point, in radians (positive
    /// turning left).
    turning: Vec<f32>,
    /// Whether it runs round back to its start.
    closed: bool,
}

impl Outline {
    fn sample(anchors: &[Anchor], closed: bool) -> Option<Self> {
        let points = sample_with_anchors(anchors, closed)?;
        let n = points.len();
        let centre = points.iter().fold(Point::ZERO, |s, &p| s + p) * (1.0 / n as f32);
        let spread = points.iter().map(|p| p.distance(centre)).sum::<f32>() / n as f32;
        if spread < 1e-3 || n < 3 {
            return None;
        }
        let shape = points.iter().map(|&p| (p - centre) * (1.0 / spread)).collect();
        let at = |k: isize| {
            let k = if closed { k.rem_euclid(n as isize) } else { k.clamp(0, n as isize - 1) };
            points[k as usize]
        };
        let turning = (0..n as isize)
            .map(|i| {
                // From two samples back to two on, so a corner shows as one
                // sharp turn however the samples either side fall.
                let (before, after) = (at(i) - at(i - 2), at(i + 2) - at(i));
                if before.length() < 1e-6 || after.length() < 1e-6 {
                    0.0
                } else {
                    angle_between(before, after)
                }
            })
            .collect();
        Some(Self { points, shape, turning, closed })
    }

    fn reversed(&self) -> Self {
        let flip = |v: &[Point]| v.iter().rev().copied().collect::<Vec<_>>();
        Self {
            points: flip(&self.points),
            shape: flip(&self.shape),
            // Running the other way, left turns become right ones.
            turning: self.turning.iter().rev().map(|t| -t).collect(),
            closed: self.closed,
        }
    }

    /// Whether the outline turns at least `threshold` at `i`, and more than
    /// at the samples beside it (so a corner gives one hint, not three).
    fn is_corner(&self, i: usize, threshold: f32) -> bool {
        let n = self.turning.len();
        let here = self.turning[i].abs();
        // An open line's ends have a neighbour on one side only.
        let near = |k: isize| match (self.closed, k) {
            (true, _) => self.turning[k.rem_euclid(n as isize) as usize].abs(),
            (false, k) if k < 0 || k >= n as isize => 0.0,
            (false, k) => self.turning[k as usize].abs(),
        };
        here >= threshold && here >= near(i as isize - 1) && here >= near(i as isize + 1)
    }

    /// How unalike sample `i` of this outline is to sample `j` of another.
    fn unlike(&self, i: usize, other: &Outline, j: usize) -> f32 {
        let place = self.shape[i].distance(other.shape[j]).powi(2);
        let bend = (self.turning[i] - other.turning[j]).powi(2);
        place + 2.0 * bend
    }
}

/// The best few places on `b` to start matching a closed outline from:
/// where `b`, turned round to start there, sits closest to `a` point for
/// point. Matching then only has to be tried from these.
fn likely_starts(a: &Outline, b: &Outline) -> Vec<usize> {
    let (n, m) = (a.points.len(), b.points.len());
    let mut scored: Vec<(f32, usize)> =
        (0..m).map(|s| ((0..n).map(|i| a.unlike(i, b, (i * m / n + s) % m)).sum::<f32>(), s)).collect();
    scored.sort_by(|x, y| x.0.total_cmp(&y.0));
    scored.into_iter().take(6).map(|(_, s)| s).collect()
}

/// Dynamic time warping: the cheapest way to walk both outlines from start
/// to end together, one or both stepping on each move, so features on one
/// meet the most alike features on the other in order. `b` is read from
/// `start` (closed outlines). Returns the cost and the path, as pairs of
/// sample indices into `a` and into `b`, the latter counted from `start`
/// so they only ever increase.
fn warp(a: &Outline, b: &Outline, start: usize, closed: bool) -> (f32, Vec<(usize, usize)>) {
    let (n, m) = (a.points.len(), b.points.len());
    let bj = |j: usize| if closed { (j + start) % m } else { j };
    // Stepping along one outline while the other waits stretches a part of
    // it; a little cost on that keeps the match from bunching up.
    const STRETCH: f32 = 0.05;
    let mut total = vec![vec![f32::INFINITY; m]; n];
    for i in 0..n {
        for j in 0..m {
            let here = a.unlike(i, b, bj(j));
            let before = if i == 0 && j == 0 {
                0.0
            } else {
                let diagonal = if i > 0 && j > 0 { total[i - 1][j - 1] } else { f32::INFINITY };
                let down = if i > 0 { total[i - 1][j] + STRETCH } else { f32::INFINITY };
                let across = if j > 0 { total[i][j - 1] + STRETCH } else { f32::INFINITY };
                diagonal.min(down).min(across)
            };
            total[i][j] = here + before;
        }
    }
    // Walk back from the end along the cheapest steps.
    let (mut i, mut j) = (n - 1, m - 1);
    let mut path = vec![(i, j)];
    while i > 0 || j > 0 {
        let options = [
            (i > 0 && j > 0).then(|| (total[i - 1][j - 1], i - 1, j - 1)),
            (i > 0).then(|| (total[i - 1][j] + STRETCH, i - 1, j)),
            (j > 0).then(|| (total[i][j - 1] + STRETCH, i, j - 1)),
        ];
        let (_, pi, pj) =
            options.into_iter().flatten().min_by(|x, y| x.0.total_cmp(&y.0)).expect("a step back");
        (i, j) = (pi, pj);
        path.push((i, j));
    }
    path.reverse();
    // Longer outlines have more samples; compare costs per step.
    (total[n - 1][m - 1] / path.len() as f32, path)
}

/// The turn from direction `u` to direction `v`, in radians, between -π
/// and π.
fn angle_between(u: Point, v: Point) -> f32 {
    let cross = u.x * v.y - u.y * v.x;
    cross.atan2(u.dot(v))
}

/// Points along the outline: each anchor, and between each two about as
/// many as that stretch's share of the whole length, evenly spaced along
/// the curve.
fn sample_with_anchors(anchors: &[Anchor], closed: bool) -> Option<Vec<Point>> {
    let n = anchors.len();
    let segments = if closed { n } else { n.checked_sub(1)? };
    // Each segment as a fine polyline, to measure and walk along.
    let polylines: Vec<Vec<Point>> = (0..segments)
        .map(|i| {
            let (p, q) = (&anchors[i], &anchors[(i + 1) % n]);
            (0..=16).map(|k| cubic(p.point, p.handle_out, q.handle_in, q.point, k as f32 / 16.0)).collect()
        })
        .collect();
    let length = |line: &[Point]| line.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
    let total: f32 = polylines.iter().map(|l| length(l)).sum();
    if total < 1e-3 {
        return None;
    }
    let mut points = Vec::new();
    for line in &polylines {
        let count = ((length(line) / total * SAMPLES as f32).round() as usize).max(1);
        points.extend(walk(line, count));
    }
    if !closed {
        points.push(anchors[n - 1].point);
    }
    Some(points)
}

/// `count` points along a polyline, evenly spaced, from its start up to
/// (but not including) its end.
fn walk(line: &[Point], count: usize) -> Vec<Point> {
    let lengths: Vec<f32> = line.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f32 = lengths.iter().sum();
    let mut out = Vec::with_capacity(count);
    let (mut segment, mut walked) = (0, 0.0);
    for k in 0..count {
        let target = total * k as f32 / count as f32;
        while segment < lengths.len() - 1 && walked + lengths[segment] < target {
            walked += lengths[segment];
            segment += 1;
        }
        let along = if lengths[segment] > 0.0 { (target - walked) / lengths[segment] } else { 0.0 };
        out.push(line[segment].lerp(line[segment + 1], along.clamp(0.0, 1.0)));
    }
    out
}

fn cubic(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let s = 1.0 - t;
    p0 * (s * s * s) + p1 * (3.0 * s * s * t) + p2 * (3.0 * s * t * t) + p3 * (t * t * t)
}

/// Anchors `t` of the way from `a` to `b` (the same length, anchor `i`
/// pairing with anchor `i`), found by blending each side's length and
/// direction. `None` if the two can't be blended that way without the
/// outline looping on itself; the caller then blends positions instead.
pub(super) fn blend_intrinsic(a: &[Anchor], b: &[Anchor], closed: bool, t: f32) -> Option<Vec<Anchor>> {
    let n = a.len();
    if n != b.len() || n < 2 {
        return None;
    }
    let edges = if closed { n } else { n - 1 };
    let (ea, eb) = (Sides::of(a, closed), Sides::of(b, closed));

    // How far each side turns on the way: the first side by the shorter
    // way round, each after it by as much more as the corner between them
    // changes. Building it up corner by corner keeps neighbouring sides
    // turning together rather than one going left and the next right.
    let mut turn = vec![0.0f32; edges];
    turn[0] = wrap(eb.angle[0] - ea.angle[0]);
    for i in 1..edges {
        let bend_a = wrap(ea.angle[i] - ea.angle[i - 1]);
        let bend_b = wrap(eb.angle[i] - eb.angle[i - 1]);
        turn[i] = turn[i - 1] + wrap(bend_b - bend_a);
    }
    if closed {
        // Round the last corner too, back to the first side: it has to
        // come back to the same turn, or the blended outline would loop.
        let bend_a = wrap(ea.angle[0] - ea.angle[edges - 1]);
        let bend_b = wrap(eb.angle[0] - eb.angle[edges - 1]);
        if (turn[edges - 1] + wrap(bend_b - bend_a) - turn[0]).abs() > 1e-2 {
            return None;
        }
    }
    // The whole shape turns the shorter way round on average.
    let weight: f32 = (0..edges).map(|i| ea.length[i] + eb.length[i]).sum::<f32>().max(1e-6);
    let mean = (0..edges).map(|i| turn[i] * (ea.length[i] + eb.length[i])).sum::<f32>() / weight;
    let whole_turns = (mean / TAU).round() * TAU;
    turn.iter_mut().for_each(|r| *r -= whole_turns);

    let angle: Vec<f32> = (0..edges).map(|i| ea.angle[i] + turn[i] * t).collect();
    let mut length: Vec<f32> = (0..edges).map(|i| lerp(ea.length[i], eb.length[i], t)).collect();
    let direction: Vec<Point> = angle.iter().map(|&r| Point::new(r.cos(), r.sin())).collect();

    // A closed outline has to meet itself again. An open one's far end has
    // to come out where the two drawings' ends say, or a line whose bends
    // change a lot (a C becoming an S) would fling its end out on the way.
    let reach = if closed {
        Point::ZERO
    } else {
        let chord = |anchors: &[Anchor]| anchors[n - 1].point - anchors[0].point;
        let (ca, cb) = (chord(a), chord(b));
        if ca.length() < 1e-3 || cb.length() < 1e-3 {
            // A line that ends where it starts (a closed shape morphing
            // with an open one) has no direction to turn.
            ca.lerp(cb, t)
        } else {
            let angle = |v: Point| v.y.atan2(v.x);
            // The end turns round the start the way the line turns on average.
            let on_average = mean - whole_turns;
            let swing = wrap(angle(cb) - angle(ca) - on_average) + on_average;
            rotate(ca, swing * t) * (lerp(ca.length(), cb.length(), t) / ca.length())
        }
    };
    fit_to(&mut length, &direction, &ea.length, &eb.length, reach);

    // Lay the sides end to end, then move the outline so its centre is
    // where the two centres blend to.
    let mut points = vec![Point::ZERO; n];
    for i in 1..n {
        points[i] = points[i - 1] + direction[i - 1] * length[i - 1];
    }
    let centre = |anchors: &[Anchor]| anchors.iter().fold(Point::ZERO, |s, x| s + x.point) * (1.0 / n as f32);
    let here = points.iter().fold(Point::ZERO, |s, &p| s + p) * (1.0 / n as f32);
    let shift = centre(a).lerp(centre(b), t) - here;
    points.iter_mut().for_each(|p| *p = *p + shift);

    // Handles turn with the point they belong to, by the average of its
    // two sides' turns, so a smooth point's two handles stay in line and
    // don't kink it; each stretches with the side it reaches along.
    let before = |i: usize| if closed { (i + n - 1) % n } else { i.saturating_sub(1) };
    let after = |i: usize| if closed || i < edges { i } else { edges - 1 };
    let mixed = (0..n)
        .map(|i| {
            let (s0, s1) = (before(i), after(i));
            // The direction at the point: halfway between its two sides.
            let facing = |sides: &Sides| sides.angle[s0] + wrap(sides.angle[s1] - sides.angle[s0]) / 2.0;
            let start = facing(&ea);
            let end = start + (turn[s0] + turn[s1]) / 2.0;
            let turned = lerp(start, end, t);
            let handle = |side: usize, of: fn(&Anchor) -> Point| {
                let local = |anchors: &[Anchor], sides: &Sides, frame: f32| {
                    let offset = of(&anchors[i]) - anchors[i].point;
                    rotate(offset, -frame) * (1.0 / sides.length[side].max(1e-3))
                };
                // `b`'s handles are measured in the frame the point turns
                // to, so they come out exactly as drawn at the end.
                let blended = local(a, &ea, start).lerp(local(b, &eb, end), t);
                points[i] + rotate(blended * length[side].max(1e-3), turned)
            };
            Anchor {
                point: points[i],
                handle_in: handle(s0, |x| x.handle_in),
                handle_out: handle(s1, |x| x.handle_out),
            }
        })
        .collect();
    Some(mixed)
}

/// The length and direction of each side of an outline: anchor to anchor.
struct Sides {
    length: Vec<f32>,
    angle: Vec<f32>,
}

impl Sides {
    fn of(anchors: &[Anchor], closed: bool) -> Self {
        let n = anchors.len();
        let edges = if closed { n } else { n - 1 };
        let vectors: Vec<Point> = (0..edges).map(|i| anchors[(i + 1) % n].point - anchors[i].point).collect();
        let length: Vec<f32> = vectors.iter().map(|v| v.length()).collect();
        // A side with no length has no direction of its own; it takes the
        // one before it, so it doesn't add a turn.
        let mut angle = Vec::with_capacity(edges);
        for (i, v) in vectors.iter().enumerate() {
            let own = (length[i] > 1e-4).then(|| v.y.atan2(v.x));
            let before = angle.last().copied();
            angle.push(own.or(before).unwrap_or(0.0));
        }
        if let Some(first) = vectors.iter().position(|v| v.length() > 1e-4) {
            // Sides before the first real one take its direction instead.
            let a = angle[first];
            angle[..first].iter_mut().for_each(|r| *r = a);
        }
        Self { length, angle }
    }
}

/// Stretches the sides of a blended outline just enough that, laid end to
/// end, they reach `reach` from the start (nothing, for a closed outline,
/// so it meets itself again). Blending directions leaves them a little
/// short or past it; the sides whose lengths change most between the
/// drawings take most of the difference (a weighted least squares fit, as
/// Sederberg's paper does), so sides the two drawings agree on stay as
/// they are.
fn fit_to(length: &mut [f32], direction: &[Point], from: &[f32], to: &[f32], reach: Point) {
    let mean = length.iter().sum::<f32>() / length.len() as f32;
    let weight: Vec<f32> = from.iter().zip(to).map(|(x, y)| (x - y).abs() + 0.05 * mean).collect();
    let gap = length.iter().zip(direction).fold(Point::ZERO, |s, (&l, &d)| s + d * l) - reach;
    // Solve (Σ w d dᵀ) λ = gap; each side then changes by -w (d · λ).
    let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
    for (w, d) in weight.iter().zip(direction) {
        xx += w * d.x * d.x;
        xy += w * d.x * d.y;
        yy += w * d.y * d.y;
    }
    let det = xx * yy - xy * xy;
    if det.abs() < 1e-9 {
        return;
    }
    let lambda = Point::new((yy * gap.x - xy * gap.y) / det, (xx * gap.y - xy * gap.x) / det);
    for ((l, w), d) in length.iter_mut().zip(&weight).zip(direction) {
        *l = (*l - w * d.dot(lambda)).max(0.0);
    }
}

fn rotate(v: Point, radians: f32) -> Point {
    let (s, c) = radians.sin_cos();
    Point::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// An angle brought into -π..π.
fn wrap(radians: f32) -> f32 {
    (radians + PI).rem_euclid(TAU) - PI
}

fn lerp(x: f32, y: f32, t: f32) -> f32 {
    x + (y - x) * t
}
