//! The math behind showing and bending a layer's motion path on the stage.
//!
//! A layer's position keys are joined by tweens, each a straight line or a
//! cubic Bézier curve (see `inkframe_core::Bend`). The stage draws them as
//! the path the layer travels, and lets the curves be reshaped in two ways:
//!
//! * dragging the path itself, which bends that tween through the pointer;
//! * dragging a curve handle, which moves just that handle.
//!
//! Positions here are in the coordinates of the layer's timeline.

use inkframe_core::{Bend, Channel, Key, MotionCurve, Point};

/// One tween of a motion path, as a cubic Bézier. Straight tweens get
/// control points a third and two thirds of the way along, the cubic that
/// traces a straight line at an even pace, so they can be bent like any other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// The frames of the keys at each end.
    pub from: u32,
    pub to: u32,
    pub points: [Point; 4],
    /// Whether it actually curves (so its handles are worth showing).
    pub bent: bool,
}

/// The tweens of a position channel, in order.
pub fn segments(position: &Channel<Point>) -> Vec<Segment> {
    position.keys().windows(2).map(|pair| segment(&pair[0], &pair[1])).collect()
}

fn segment(from: &Key<Point>, to: &Key<Point>) -> Segment {
    let (points, bent) = match MotionCurve::between(from, to) {
        Some(curve) => (curve.points, true),
        None => {
            let (a, b) = (from.value, to.value);
            ([a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b], false)
        }
    };
    Segment { from: from.frame, to: to.frame, points, bent }
}

/// The handles that bend `segment` so the point at parameter `s` moves to
/// `target`, as (the start key's `out`, the end key's `into`).
///
/// Both control points move by the same amount `d`. The curve's point at
/// `s` then moves by `3s(1-s)·d` (the sum of the middle two Bernstein
/// weights), so `d` is the distance to cover divided by that. Grabbing near
/// an end, where the weight is tiny, would fling the handles far away, so
/// `s` is kept away from the ends.
pub fn bend_through(segment: &Segment, s: f32, target: Point) -> (Point, Point) {
    let s = s.clamp(0.15, 0.85);
    let [p0, p1, p2, p3] = segment.points;
    let current = inkframe_core::MotionCurve { points: segment.points }.at(s);
    let d = (target - current) * (1.0 / (3.0 * s * (1.0 - s)));
    (p1 + d - p0, p2 + d - p3)
}

/// The bend for the key at `frame` with one handle replaced.
pub fn with_handle(position: &Channel<Point>, frame: u32, out: bool, handle: Point) -> Option<Bend> {
    let key = position.key_at(frame)?;
    let mut bend = key.bend.unwrap_or_default();
    if out {
        bend.out = handle
    } else {
        bend.into = handle
    }
    Some(bend)
}

/// The parameter of the point on `segment` nearest `point`, and how far it
/// is, found by sampling (plenty for picking with a mouse).
pub fn nearest(segment: &Segment, point: Point) -> (f32, f32) {
    let curve = inkframe_core::MotionCurve { points: segment.points };
    (0..=64)
        .map(|i| {
            let s = i as f32 / 64.0;
            (s, curve.at(s).distance(point))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("sampled at least once")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> Channel<Point> {
        let mut channel = Channel::new(0, Point::ZERO);
        channel.set(10, Point::new(90.0, 0.0));
        channel
    }

    #[test]
    fn straight_tweens_are_evenly_spaced_cubics() {
        let segments = segments(&straight());
        assert_eq!(segments.len(), 1);
        assert!(!segments[0].bent);
        assert_eq!(segments[0].points[1], Point::new(30.0, 0.0));
    }

    #[test]
    fn bending_moves_the_grabbed_point_to_the_pointer() {
        let mut channel = straight();
        let segment = segments(&channel)[0];
        let target = Point::new(45.0, -40.0);
        let (out, into) = bend_through(&segment, 0.5, target);
        channel.set_bend(0, Some(Bend { into: Point::ZERO, out }));
        channel.set_bend(10, Some(Bend { into, out: Point::ZERO }));

        let bent = segments(&channel)[0];
        assert!(bent.bent);
        let curve = MotionCurve { points: bent.points };
        assert!(curve.at(0.5).distance(target) < 1e-3);
    }

    #[test]
    fn nearest_finds_where_on_the_path_was_clicked() {
        let segment = segments(&straight())[0];
        let (s, distance) = nearest(&segment, Point::new(30.0, 4.0));
        assert!((s - 1.0 / 3.0).abs() < 0.02 && (distance - 4.0).abs() < 0.5);
    }
}
