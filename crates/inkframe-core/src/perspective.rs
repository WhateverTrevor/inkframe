//! Perspective guides, as Harmony's: vanishing points on a horizon over
//! the stage, with lines radiating from them, to draw buildings, roads and
//! rooms that recede properly.
//!
//! One-point perspective has a single vanishing point; edges run toward it,
//! or straight across and up and down. Two-point has one at each side of
//! the horizon; edges run toward either, or straight up and down. Three-
//! point adds one far above or below for the verticals, so nothing is
//! straight. With the guide showing, Shift with the Pen snaps each new
//! segment to whichever of those directions is nearest.

use serde::{Deserialize, Serialize};

use crate::geometry::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerspectiveKind {
    OnePoint,
    TwoPoint,
    ThreePoint,
}

impl PerspectiveKind {
    pub const ALL: [PerspectiveKind; 3] =
        [PerspectiveKind::OnePoint, PerspectiveKind::TwoPoint, PerspectiveKind::ThreePoint];

    pub fn label(self) -> &'static str {
        match self {
            PerspectiveKind::OnePoint => crate::i18n::tr_later("One-Point"),
            PerspectiveKind::TwoPoint => crate::i18n::tr_later("Two-Point"),
            PerspectiveKind::ThreePoint => crate::i18n::tr_later("Three-Point"),
        }
    }

    fn points(self) -> usize {
        match self {
            PerspectiveKind::OnePoint => 1,
            PerspectiveKind::TwoPoint => 2,
            PerspectiveKind::ThreePoint => 3,
        }
    }
}

/// A perspective guide over the stage, in stage pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Perspective {
    pub kind: PerspectiveKind,
    /// The vanishing points: the first one or two on the horizon, the third
    /// (three-point) above or below.
    pub points: Vec<Point>,
}

impl Perspective {
    /// A guide of `kind` laid out for a stage `width`×`height`: the horizon
    /// a third of the way down, its vanishing points in the middle or near
    /// either side (on the stage, where they can be grabbed and dragged
    /// further out), and a third just below.
    pub fn new(kind: PerspectiveKind, width: f32, height: f32) -> Self {
        let horizon = height / 3.0;
        let points = match kind {
            PerspectiveKind::OnePoint => vec![Point::new(width / 2.0, horizon)],
            PerspectiveKind::TwoPoint => {
                vec![Point::new(width * 0.04, horizon), Point::new(width * 0.96, horizon)]
            }
            PerspectiveKind::ThreePoint => vec![
                Point::new(width * 0.04, horizon),
                Point::new(width * 0.96, horizon),
                Point::new(width / 2.0, height * 0.97),
            ],
        };
        Self { kind, points }
    }

    /// Makes it of `kind`, keeping the points it has where they'll do.
    pub fn set_kind(&mut self, kind: PerspectiveKind, width: f32, height: f32) {
        let fresh = Self::new(kind, width, height);
        let mut points: Vec<Point> = self.points.iter().copied().take(kind.points()).collect();
        points.extend(fresh.points.into_iter().skip(points.len()));
        *self = Self { kind, points };
    }

    /// The horizon's height: the first vanishing point's. Moving one
    /// vanishing point on the horizon moves the other up or down with it.
    pub fn horizon(&self) -> f32 {
        self.points.first().map_or(0.0, |p| p.y)
    }

    /// Moves vanishing point `index` to `to`. The two on the horizon stay
    /// level with each other.
    pub fn move_point(&mut self, index: usize, to: Point) {
        if index >= self.points.len() {
            return;
        }
        self.points[index] = to;
        if index < 2 && self.kind != PerspectiveKind::OnePoint {
            let other = 1 - index;
            if let Some(p) = self.points.get_mut(other) {
                p.y = to.y;
            }
        }
    }

    /// The directions a line drawn from `from` may run in, as unit vectors.
    pub fn directions(&self, from: Point) -> Vec<Point> {
        let mut directions: Vec<Point> = self
            .points
            .iter()
            .filter_map(|&vp| {
                let d = vp - from;
                (d.length() > 1e-3).then(|| d * (1.0 / d.length()))
            })
            .collect();
        match self.kind {
            PerspectiveKind::OnePoint => directions.extend([Point::new(1.0, 0.0), Point::new(0.0, 1.0)]),
            PerspectiveKind::TwoPoint => directions.push(Point::new(0.0, 1.0)),
            PerspectiveKind::ThreePoint => {}
        }
        directions
    }

    /// Where a line from `from` toward `p` ends, snapped onto the nearest
    /// of the guide's directions (either way along it).
    pub fn snap(&self, from: Point, p: Point) -> Point {
        let want = p - from;
        if want.length() < 1e-3 {
            return p;
        }
        let best =
            self.directions(from).into_iter().max_by(|a, b| a.dot(want).abs().total_cmp(&b.dot(want).abs()));
        match best {
            Some(d) => from + d * d.dot(want),
            None => p,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_point_snaps_toward_the_vanishing_point_or_square_on() {
        let guide = Perspective::new(PerspectiveKind::OnePoint, 1920.0, 1080.0);
        let vp = guide.points[0];
        let from = Point::new(400.0, 900.0);
        // Roughly toward the vanishing point: exactly toward it.
        let toward = from + (vp - from) * 0.5 + Point::new(15.0, 0.0);
        let snapped = guide.snap(from, toward);
        let (a, b) = (snapped - from, vp - from);
        let sine = (a.x * b.y - a.y * b.x) / (a.length() * b.length());
        assert!(sine.abs() < 1e-4, "in line with the vanishing point: {snapped:?}");
        // Nearly level: level.
        assert!((guide.snap(from, Point::new(700.0, 910.0)).y - 900.0).abs() < 1e-3);
        // Nearly upright: upright.
        assert!((guide.snap(from, Point::new(405.0, 600.0)).x - 400.0).abs() < 1e-3);
    }

    #[test]
    fn two_points_on_the_horizon_stay_level() {
        let mut guide = Perspective::new(PerspectiveKind::TwoPoint, 1920.0, 1080.0);
        guide.move_point(0, Point::new(-300.0, 500.0));
        assert_eq!(guide.points[1].y, 500.0);
        assert_eq!(guide.horizon(), 500.0);
        // Two-point has no level lines: a nearly level one goes to a point.
        let from = Point::new(960.0, 800.0);
        let snapped = guide.snap(from, Point::new(1400.0, 790.0));
        assert!((snapped.y - 800.0).abs() > 1.0);
    }

    #[test]
    fn changing_kind_keeps_the_points_it_can() {
        let mut guide = Perspective::new(PerspectiveKind::TwoPoint, 1920.0, 1080.0);
        guide.move_point(1, Point::new(2500.0, 400.0));
        guide.set_kind(PerspectiveKind::ThreePoint, 1920.0, 1080.0);
        assert_eq!(guide.points.len(), 3);
        assert_eq!(guide.points[1], Point::new(2500.0, 400.0));
        guide.set_kind(PerspectiveKind::OnePoint, 1920.0, 1080.0);
        assert_eq!(guide.points.len(), 1);
    }
}
