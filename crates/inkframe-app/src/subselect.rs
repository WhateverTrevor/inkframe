//! The math behind the Subselection tool: moving anchors and their handles.
//!
//! Everything here works on a layer's anchors in its *local* coordinates.
//! The stage converts the pointer into those coordinates first, so edits
//! behave the same however the layer is moved, rotated or scaled.
//!
//! As with the transform handles, each drag update starts from a snapshot of
//! the anchors taken when the drag began, so rounding errors can't build up.

use inkframe_core::{Anchor, Point};

/// Which part of an anchor is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorPart {
    Point,
    HandleIn,
    HandleOut,
}

impl AnchorPart {
    /// Where this part of `anchor` is.
    pub fn position(self, anchor: &Anchor) -> Point {
        match self {
            AnchorPart::Point => anchor.point,
            AnchorPart::HandleIn => anchor.handle_in,
            AnchorPart::HandleOut => anchor.handle_out,
        }
    }
}

/// Returns `anchors` with `part` of anchor `index` moved to `target`.
///
/// - Moving the **point** carries both handles along, so the curve's shape
///   around the point is preserved.
/// - Moving a **handle** of a smooth anchor swings the opposite handle to
///   stay in line (keeping its own length), so the curve stays smooth.
///   With `independent`, only the dragged handle moves, which turns the
///   anchor into a corner. That's Alt/Option-drag in most vector editors.
pub fn move_part(
    anchors: &[Anchor],
    index: usize,
    part: AnchorPart,
    target: Point,
    independent: bool,
) -> Vec<Anchor> {
    let mut anchors = anchors.to_vec();
    let a = &mut anchors[index];
    match part {
        AnchorPart::Point => {
            let delta = target - a.point;
            a.point = target;
            a.handle_in = a.handle_in + delta;
            a.handle_out = a.handle_out + delta;
        }
        AnchorPart::HandleIn => {
            let smooth = !independent && is_smooth(a);
            a.handle_in = target;
            if smooth {
                a.handle_out = mirror_direction(a.point, target, a.handle_out);
            }
        }
        AnchorPart::HandleOut => {
            let smooth = !independent && is_smooth(a);
            a.handle_out = target;
            if smooth {
                a.handle_in = mirror_direction(a.point, target, a.handle_in);
            }
        }
    }
    anchors
}

/// Switches an anchor between corner and smooth.
///
/// - An anchor with any handle sticking out becomes a **corner**: both handles
///   collapse onto the point, so the curve meets it at a sharp angle.
/// - An anchor without handles becomes **smooth**. The new handles point
///   along the line from the previous anchor to the next one, the same
///   tangent a Catmull-Rom spline would use, so the curve flows through
///   naturally. Each handle reaches a third of the way to its neighbour,
///   which gives gentle curves that don't overshoot the neighbours.
///
/// The first and last anchors of an open path have only one neighbour, so
/// they aim straight at it.
pub fn toggle_smooth(anchors: &[Anchor], index: usize, closed: bool) -> Vec<Anchor> {
    let mut anchors = anchors.to_vec();
    let a = anchors[index];
    if a.handle_in != a.point || a.handle_out != a.point {
        anchors[index] = Anchor::corner(a.point);
        return anchors;
    }

    let neighbour = |offset: isize| -> Option<Point> {
        let i = index as isize + offset;
        let n = anchors.len() as isize;
        if closed {
            Some(anchors[i.rem_euclid(n) as usize].point)
        } else {
            (0..n).contains(&i).then(|| anchors[i as usize].point)
        }
    };
    let (prev, next) = (neighbour(-1), neighbour(1));
    let direction = match (prev, next) {
        (Some(prev), Some(next)) => next - prev,
        (None, Some(next)) => next - a.point,
        (Some(prev), None) => a.point - prev,
        (None, None) => return anchors, // A lone point has no curve to smooth.
    };
    let length = direction.length();
    if length < 1e-6 {
        return anchors;
    }
    let unit = direction * (1.0 / length);
    // A missing neighbour (an open path's end) gets no handle on that side.
    let reach = |other: Option<Point>| other.map_or(0.0, |o| o.distance(a.point) / 3.0);
    anchors[index] = Anchor {
        point: a.point,
        handle_in: a.point - unit * reach(prev),
        handle_out: a.point + unit * reach(next),
    };
    anchors
}

/// Removes an anchor, unless that would leave too little to be a shape:
/// open paths need two anchors and closed paths need three.
pub fn remove_anchor(anchors: &[Anchor], index: usize, closed: bool) -> Option<Vec<Anchor>> {
    let minimum = if closed { 3 } else { 2 };
    if anchors.len() <= minimum || index >= anchors.len() {
        return None;
    }
    let mut anchors = anchors.to_vec();
    anchors.remove(index);
    Some(anchors)
}

/// An anchor is smooth if both handles stick out and point in opposite
/// directions. A small angular tolerance absorbs float noise from earlier edits.
fn is_smooth(a: &Anchor) -> bool {
    const MIN_LENGTH: f32 = 1e-3;
    // cos(178°): the handles may be up to 2° off a straight line.
    const OPPOSITE: f32 = -0.9994;

    let (into, out) = (a.handle_in - a.point, a.handle_out - a.point);
    let (len_in, len_out) = (into.length(), out.length());
    len_in > MIN_LENGTH && len_out > MIN_LENGTH && into.dot(out) / (len_in * len_out) < OPPOSITE
}

/// Points `other` directly away from `dragged` through `pivot`, keeping
/// `other`'s distance from the pivot. This keeps uneven handle lengths
/// uneven, so a curve that's tighter on one side stays that way.
fn mirror_direction(pivot: Point, dragged: Point, other: Point) -> Point {
    let direction = pivot - dragged;
    let length = direction.length();
    if length < 1e-6 {
        return other;
    }
    pivot + direction * ((other - pivot).length() / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn moving_a_point_carries_its_handles() {
        let anchors = [Anchor::smooth(p(0.0, 0.0), p(10.0, 0.0))];
        let moved = move_part(&anchors, 0, AnchorPart::Point, p(5.0, 5.0), false);
        assert_eq!(moved[0].handle_out, p(15.0, 5.0));
        assert_eq!(moved[0].handle_in, p(-5.0, 5.0));
    }

    #[test]
    fn smooth_handles_stay_in_line_and_keep_their_length() {
        // Uneven handles: 10 out, 20 in.
        let anchor = Anchor { point: p(0.0, 0.0), handle_in: p(-20.0, 0.0), handle_out: p(10.0, 0.0) };
        let moved = move_part(&[anchor], 0, AnchorPart::HandleOut, p(0.0, 30.0), false);
        assert_eq!(moved[0].handle_out, p(0.0, 30.0));
        assert!(close(moved[0].handle_in, p(0.0, -20.0)), "{:?}", moved[0].handle_in);
    }

    #[test]
    fn independent_drag_breaks_the_tangent() {
        let anchor = Anchor::smooth(p(0.0, 0.0), p(10.0, 0.0));
        let moved = move_part(&[anchor], 0, AnchorPart::HandleOut, p(0.0, 10.0), true);
        assert_eq!(moved[0].handle_in, p(-10.0, 0.0));
    }

    #[test]
    fn corners_stay_corners() {
        // A corner whose handles point different ways isn't smooth, so
        // dragging one handle leaves the other alone.
        let anchor = Anchor { point: p(0.0, 0.0), handle_in: p(0.0, -10.0), handle_out: p(10.0, 0.0) };
        let moved = move_part(&[anchor], 0, AnchorPart::HandleOut, p(20.0, 5.0), false);
        assert_eq!(moved[0].handle_in, p(0.0, -10.0));
    }

    #[test]
    fn corner_becomes_smooth_along_its_neighbours() {
        let anchors = [p(0.0, 0.0), p(50.0, 50.0), p(100.0, 0.0)].map(Anchor::corner);
        let a = toggle_smooth(&anchors, 1, false)[1];
        // The tangent runs parallel to the line from the first to the last point.
        assert!((a.handle_out.y - a.point.y).abs() < 1e-4 && a.handle_out.x > a.point.x);
        assert!((a.handle_in.y - a.point.y).abs() < 1e-4 && a.handle_in.x < a.point.x);
        // Each handle reaches a third of the way to its neighbour (√5000 / 3).
        let third = 5000f32.sqrt() / 3.0;
        assert!((a.handle_out.distance(a.point) - third).abs() < 1e-3);
    }

    #[test]
    fn smooth_becomes_corner() {
        let anchors = [Anchor::smooth(p(0.0, 0.0), p(10.0, 5.0))];
        assert_eq!(toggle_smooth(&anchors, 0, false)[0], Anchor::corner(p(0.0, 0.0)));
    }

    #[test]
    fn toggling_twice_from_corner_round_trips() {
        let anchors = [p(0.0, 0.0), p(50.0, 50.0), p(100.0, 0.0)].map(Anchor::corner);
        let back = toggle_smooth(&toggle_smooth(&anchors, 1, false), 1, false);
        assert_eq!(back, anchors);
    }

    #[test]
    fn open_endpoints_aim_at_their_only_neighbour() {
        let anchors = [p(0.0, 0.0), p(90.0, 0.0)].map(Anchor::corner);
        let a = toggle_smooth(&anchors, 0, false)[0];
        assert_eq!(a.handle_out, p(30.0, 0.0));
        assert_eq!(a.handle_in, a.point, "no neighbour before the first point");
    }

    #[test]
    fn closed_paths_wrap_around_for_neighbours() {
        let square = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)].map(Anchor::corner);
        // The first anchor's neighbours are the last and second anchors.
        let a = toggle_smooth(&square, 0, true)[0];
        assert!(a.handle_in != a.point && a.handle_out != a.point);
        let (into, out) = (a.handle_in - a.point, a.handle_out - a.point);
        assert!(into.dot(out) < 0.0, "handles should point opposite ways");
    }

    #[test]
    fn closed_paths_keep_at_least_three_anchors() {
        let triangle = [p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0)].map(Anchor::corner);
        assert!(remove_anchor(&triangle, 0, true).is_none());
        assert_eq!(remove_anchor(&triangle, 0, false).unwrap().len(), 2);
    }
}
