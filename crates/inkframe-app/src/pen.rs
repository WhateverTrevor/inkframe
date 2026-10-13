//! State for a path being drawn with the Pen tool.
//!
//! Unlike the other tools, a pen path spans many separate clicks, so it
//! lives here between mouse gestures rather than in a single drag. All
//! positions are in stage pixels. The stage handles input and drawing.

use inkframe_core::{Anchor, Path, Point, path_from_anchors};

#[derive(Debug, Clone)]
pub struct PenPath {
    anchors: Vec<Anchor>,
    /// True between pressing to place an anchor and releasing the button.
    /// Dragging in that time pulls out the anchor's curve handles.
    dragging: bool,
}

impl PenPath {
    /// Starts a path at `first`, with the button still held.
    pub fn new(first: Point) -> Self {
        Self { anchors: vec![Anchor::corner(first)], dragging: true }
    }

    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn last(&self) -> &Anchor {
        self.anchors.last().expect("a pen path always has at least one anchor")
    }

    /// Whether clicking at `p` should close the path: it must land on the
    /// first anchor, and there must be something to close.
    pub fn can_close_at(&self, p: Point, tolerance: f32) -> bool {
        self.anchors.len() >= 2 && p.distance(self.anchors[0].point) <= tolerance
    }

    /// Whether `p` lands on the most recent anchor. Clicking it again ends
    /// an open path, which is also what a double-click turns into.
    pub fn is_on_last(&self, p: Point, tolerance: f32) -> bool {
        p.distance(self.last().point) <= tolerance
    }

    /// Places a new corner anchor. With `constrain`, the new point snaps to
    /// the nearest 45° direction from the previous one.
    pub fn add_anchor(&mut self, p: Point, constrain: bool) {
        let p = if constrain { snap_to_45(self.last().point, p) } else { p };
        self.anchors.push(Anchor::corner(p));
        self.dragging = true;
    }

    /// Updates the newest anchor's handles while the button is held.
    ///
    /// Dragging less than `min_drag` leaves a corner, so a slightly shaky
    /// click doesn't produce a tiny curve.
    pub fn drag_handle(&mut self, pointer: Point, min_drag: f32) {
        let anchor = self.anchors.last_mut().expect("never empty");
        *anchor = if pointer.distance(anchor.point) > min_drag {
            Anchor::smooth(anchor.point, pointer)
        } else {
            Anchor::corner(anchor.point)
        };
    }

    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// Removes the newest anchor. Returns `false` if nothing is left, in
    /// which case the caller should abandon the path.
    pub fn remove_last(&mut self) -> bool {
        self.anchors.pop();
        self.dragging = false;
        !self.anchors.is_empty()
    }

    /// The segment that would be added by clicking at `pointer`, so the user
    /// can see where the next click will go.
    pub fn preview_to(&self, pointer: Point) -> Option<Path> {
        path_from_anchors(&[*self.last(), Anchor::corner(pointer)], false)
    }

    /// The finished path, or `None` if it has fewer than two anchors.
    pub fn finish(&self, closed: bool) -> Option<Path> {
        path_from_anchors(&self.anchors, closed)
    }
}

/// Snaps `p` onto the nearest horizontal, vertical or diagonal line through
/// `from`, keeping its distance along that line.
fn snap_to_45(from: Point, p: Point) -> Point {
    let d = p - from;
    let step = std::f32::consts::FRAC_PI_4;
    let angle = (d.y.atan2(d.x) / step).round() * step;
    // Project onto the snapped direction rather than keeping the raw length,
    // so the point stays close to where the user is pointing.
    let direction = Point::new(angle.cos(), angle.sin());
    from + direction * d.dot(direction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::Segment;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn clicks_make_a_polyline() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.release();
        pen.add_anchor(p(100.0, 0.0), false);
        pen.release();
        pen.add_anchor(p(100.0, 100.0), false);
        pen.release();

        let path = pen.finish(false).unwrap();
        assert!(path.segments.iter().all(|s| matches!(s, Segment::Line { .. })));
        assert_eq!(path.segments.len(), 2);
    }

    #[test]
    fn dragging_makes_a_smooth_anchor() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.release();
        pen.add_anchor(p(100.0, 0.0), false);
        pen.drag_handle(p(130.0, 40.0), 1.0);
        pen.release();

        let anchor = pen.anchors()[1];
        assert_eq!(anchor.handle_out, p(130.0, 40.0));
        assert_eq!(anchor.handle_in, p(70.0, -40.0));
    }

    #[test]
    fn tiny_drags_stay_corners() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.drag_handle(p(0.5, 0.0), 2.0);
        assert_eq!(pen.anchors()[0], Anchor::corner(p(0.0, 0.0)));
    }

    #[test]
    fn closing_needs_at_least_two_anchors() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        assert!(!pen.can_close_at(p(0.0, 0.0), 5.0));
        pen.add_anchor(p(50.0, 0.0), false);
        assert!(pen.can_close_at(p(2.0, 2.0), 5.0));
    }

    #[test]
    fn shift_snaps_to_45_degrees() {
        let snapped = snap_to_45(p(0.0, 0.0), p(100.0, 10.0));
        assert!((snapped.y).abs() < 1e-4);
        let diagonal = snap_to_45(p(0.0, 0.0), p(100.0, 90.0));
        assert!((diagonal.x - diagonal.y).abs() < 1e-3);
    }

    #[test]
    fn removing_the_only_anchor_abandons_the_path() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        assert!(!pen.remove_last());
    }
}
