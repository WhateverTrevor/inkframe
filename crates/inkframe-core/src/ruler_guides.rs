//! Guides: lines dragged out of the rulers onto the stage, as in Animate,
//! for lining things up. They're kept with the project, in stage pixels,
//! and things being moved or drawn can snap to them.

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect};

/// One guide line across the stage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RulerGuide {
    /// Up and down (dragged from the left ruler), at `at` across; otherwise
    /// across (from the top ruler), at `at` down.
    pub vertical: bool,
    pub at: f32,
}

/// Where `value` should move to, along one axis, to land on the nearest of
/// `lines` within `reach`; `None` if none is that close.
pub fn snap_value(value: f32, lines: impl Iterator<Item = f32>, reach: f32) -> Option<f32> {
    lines
        .map(|line| line - value)
        .filter(|d| d.abs() <= reach)
        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        .map(|d| value + d)
}

/// A point moved onto any guides within `reach` of it, on each axis.
pub fn snap_point(guides: &[RulerGuide], p: Point, reach: f32) -> Point {
    let lines = |vertical: bool| guides.iter().filter(move |g| g.vertical == vertical).map(|g| g.at);
    Point::new(
        snap_value(p.x, lines(true), reach).unwrap_or(p.x),
        snap_value(p.y, lines(false), reach).unwrap_or(p.y),
    )
}

/// How far to nudge a box so one of its edges or its middle lands on a
/// guide within `reach`, on each axis (0 where nothing's close).
pub fn snap_box(guides: &[RulerGuide], area: Rect, reach: f32) -> Point {
    let nudge = |vertical: bool, edges: [f32; 3]| {
        guides
            .iter()
            .filter(|g| g.vertical == vertical)
            .flat_map(|g| edges.map(|e| g.at - e))
            .filter(|d| d.abs() <= reach)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.0)
    };
    let centre = area.center();
    Point::new(
        nudge(true, [area.min.x, centre.x, area.max.x]),
        nudge(false, [area.min.y, centre.y, area.max.y]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guides() -> Vec<RulerGuide> {
        vec![RulerGuide { vertical: true, at: 100.0 }, RulerGuide { vertical: false, at: 50.0 }]
    }

    #[test]
    fn points_snap_to_nearby_guides_only() {
        assert_eq!(snap_point(&guides(), Point::new(97.0, 54.0), 5.0), Point::new(100.0, 50.0));
        assert_eq!(snap_point(&guides(), Point::new(90.0, 54.0), 5.0), Point::new(90.0, 50.0), "x too far");
    }

    #[test]
    fn a_box_snaps_by_its_nearest_edge_or_middle() {
        // 20 wide, right edge at 98: its right edge goes to the guide.
        let area = Rect::from_points(Point::new(78.0, 0.0), Point::new(98.0, 20.0));
        assert_eq!(snap_box(&guides(), area, 5.0), Point::new(2.0, 0.0));
        // Its middle at 48 (top 38, bottom 58): the middle snaps.
        let area = Rect::from_points(Point::new(0.0, 38.0), Point::new(10.0, 58.0));
        assert_eq!(snap_box(&guides(), area, 5.0).y, 2.0);
    }
}
