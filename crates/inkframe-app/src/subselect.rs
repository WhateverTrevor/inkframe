//! The math behind the Subselection tool (and the Pen's point tools):
//! moving anchors and their handles.
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

/// What sort of point an anchor is, as Illustrator names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointKind {
    /// No handles: the outline turns sharply here.
    Corner,
    /// Handles in line, so the curve flows through; each its own length,
    /// so it can be tighter on one side.
    Smooth,
    /// Handles in line and the same length.
    Symmetric,
    /// Handles that aren't in line (or only one): a corner between curves,
    /// as Alt-dragging a handle makes.
    Broken,
}

impl PointKind {
    /// How alike two handle lengths must be to count as the same.
    const SAME_LENGTH: f32 = 0.005;

    pub fn of(a: &Anchor) -> Self {
        let (into, out) = ((a.handle_in - a.point).length(), (a.handle_out - a.point).length());
        if a.handle_in == a.point && a.handle_out == a.point {
            PointKind::Corner
        } else if !is_smooth(a) {
            PointKind::Broken
        } else if (into - out).abs() <= Self::SAME_LENGTH * into.max(out) {
            PointKind::Symmetric
        } else {
            PointKind::Smooth
        }
    }
}

/// Returns `anchors` with anchor `index` made a `kind` of point, changing
/// its handles as little as that allows:
///
/// - **Corner**: the handles are taken in.
/// - **Smooth**: a corner gets handles along its neighbours, as
///   [`toggle_smooth`] gives it; handles out of line are brought into line
///   along the way between them, each keeping its length.
/// - **Symmetric**: smooth, and both handles the length of their average.
///
/// A broken point is one only Alt-dragging a handle makes, so asking for
/// one leaves the anchor as it is.
pub fn set_point_kind(anchors: &[Anchor], index: usize, closed: bool, kind: PointKind) -> Vec<Anchor> {
    let a = anchors[index];
    let mut anchors = anchors.to_vec();
    match kind {
        PointKind::Broken => {}
        PointKind::Corner => anchors[index] = Anchor::corner(a.point),
        PointKind::Smooth | PointKind::Symmetric => {
            if PointKind::of(&a) == PointKind::Corner {
                anchors = toggle_smooth(&anchors, index, closed);
            } else if !is_smooth(&a) {
                let (into, out) = (a.handle_in - a.point, a.handle_out - a.point);
                let (len_in, len_out) = (into.length(), out.length());
                // The way through: from the way in's handle towards the way
                // out's, or along the only handle there is.
                let unit = |v: Point| if v.length() > 1e-6 { v * (1.0 / v.length()) } else { Point::ZERO };
                let along = unit(unit(out) - unit(into));
                let along = if along == Point::ZERO { unit(out) } else { along };
                // A handle that wasn't there takes the other's length, so
                // both sides curve.
                let (len_in, len_out) = match (len_in > 1e-6, len_out > 1e-6) {
                    (true, true) => (len_in, len_out),
                    (false, _) => (len_out, len_out),
                    (_, false) => (len_in, len_in),
                };
                anchors[index] = Anchor {
                    point: a.point,
                    handle_in: a.point - along * len_in,
                    handle_out: a.point + along * len_out,
                };
            }
            if kind == PointKind::Symmetric {
                let b = anchors[index];
                let (into, out) = (b.handle_in - b.point, b.handle_out - b.point);
                let length = (into.length() + out.length()) / 2.0;
                if out.length() > 1e-6 {
                    let unit = out * (1.0 / out.length());
                    anchors[index] = Anchor {
                        point: b.point,
                        handle_in: b.point - unit * length,
                        handle_out: b.point + unit * length,
                    };
                }
            }
        }
    }
    anchors
}

/// Returns `anchor` with one handle at `length` and `degrees` from its
/// point (0° is to the right, turning clockwise as the stage's y runs
/// down). The other handle keeps in line with it if the point is smooth,
/// and the same length too if it's symmetric, as dragging it would.
pub fn set_handle(anchor: &Anchor, part: AnchorPart, length: f32, degrees: f32) -> Anchor {
    let kind = PointKind::of(anchor);
    let radians = degrees.to_radians();
    let target = anchor.point + Point::new(radians.cos(), radians.sin()) * length.max(0.0);
    let mut moved = move_part(&[*anchor], 0, part, target, false)[0];
    if kind == PointKind::Symmetric && length > 1e-6 {
        let away = Point::new(-radians.cos(), -radians.sin()) * length;
        match part {
            AnchorPart::HandleIn => moved.handle_out = anchor.point + away,
            AnchorPart::HandleOut => moved.handle_in = anchor.point + away,
            AnchorPart::Point => {}
        }
    }
    moved
}

/// A handle's length and direction from its point, in degrees as
/// [`set_handle`] takes them.
pub fn handle_polar(anchor: &Anchor, part: AnchorPart) -> (f32, f32) {
    let d = part.position(anchor) - anchor.point;
    (d.length(), d.y.atan2(d.x).to_degrees())
}

/// Anchors for the Curvature tool: the curve flows smoothly through each
/// point but the `corners`, its handles along the line between the point's
/// neighbours as [`toggle_smooth`] gives them, so moving one point bends the
/// curve either side of it, as Illustrator's Curvature tool does.
pub fn curvature_anchors(points: &[Point], corners: &[bool], closed: bool) -> Vec<Anchor> {
    let corner_anchors: Vec<Anchor> = points.iter().map(|&p| Anchor::corner(p)).collect();
    // Each point's handles come from its neighbours' places alone, so
    // working them out one at a time gives the same as all at once.
    (0..points.len())
        .map(|i| {
            if corners.get(i).copied().unwrap_or(false) {
                corner_anchors[i]
            } else {
                toggle_smooth(&corner_anchors, i, closed)[i]
            }
        })
        .collect()
}

/// Returns `anchors` with the curve re-flowed through point `index` and
/// its neighbours, as the Curvature tool does when a point moves: each of
/// them that isn't a corner gets its handles afresh from its neighbours.
pub fn reflow_around(anchors: &[Anchor], index: usize, closed: bool) -> Vec<Anchor> {
    let n = anchors.len();
    let mut around = vec![index];
    if index > 0 || closed {
        around.push((index + n - 1) % n);
    }
    if index + 1 < n || closed {
        around.push((index + 1) % n);
    }
    let mut anchors = anchors.to_vec();
    for i in around {
        let a = anchors[i];
        if PointKind::of(&a) != PointKind::Corner {
            // Made a corner first, so `toggle_smooth` makes it smooth anew.
            anchors[i] = Anchor::corner(a.point);
            anchors[i] = toggle_smooth(&anchors, i, closed)[i];
        }
    }
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

/// Returns `anchors` with new handles pulled out of anchor `index`: its
/// way out reaching to `target` and its way in mirroring it, so the curve
/// passes smoothly through the point. That's Alt-dragging a point in
/// Animate, with the Subselection tool or the Pen, and it turns a corner
/// into a curve (or re-draws a curve's handles from scratch).
pub fn pull_handles(anchors: &[Anchor], index: usize, target: Point) -> Vec<Anchor> {
    let mut anchors = anchors.to_vec();
    let a = &mut anchors[index];
    a.handle_out = target;
    a.handle_in = a.point - (target - a.point);
    anchors
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
    fn moving_a_point_with_the_curvature_tool_reflows_its_neighbours_not_corners() {
        let points = [p(0.0, 0.0), p(100.0, 0.0), p(200.0, 0.0), p(300.0, 0.0)];
        let anchors = curvature_anchors(&points, &[true, false, false, false], false);
        let mut moved = anchors.clone();
        moved[1] = Anchor { point: p(100.0, -80.0), ..anchors[1] };
        let flowed = reflow_around(&moved, 1, false);
        assert_eq!(flowed[0], anchors[0], "a corner stays a corner");
        assert!(flowed[2].handle_in.y < -1.0, "the next point's curve leans up to meet it: {:?}", flowed[2]);
        assert_eq!(flowed[3], anchors[3], "two along, nothing changes");
    }

    #[test]
    fn points_are_told_apart_by_their_handles() {
        assert_eq!(PointKind::of(&Anchor::corner(p(0.0, 0.0))), PointKind::Corner);
        assert_eq!(PointKind::of(&Anchor::smooth(p(0.0, 0.0), p(10.0, 0.0))), PointKind::Symmetric);
        let uneven = Anchor { point: p(0.0, 0.0), handle_in: p(-20.0, 0.0), handle_out: p(10.0, 0.0) };
        assert_eq!(PointKind::of(&uneven), PointKind::Smooth);
        let bent = Anchor { point: p(0.0, 0.0), handle_in: p(0.0, -20.0), handle_out: p(10.0, 0.0) };
        assert_eq!(PointKind::of(&bent), PointKind::Broken);
    }

    #[test]
    fn a_broken_point_made_smooth_comes_into_line_keeping_its_lengths() {
        let bent = Anchor { point: p(0.0, 0.0), handle_in: p(0.0, 20.0), handle_out: p(10.0, 0.0) };
        let smooth = set_point_kind(&[bent], 0, false, PointKind::Smooth)[0];
        assert_eq!(PointKind::of(&smooth), PointKind::Smooth);
        assert!(((smooth.handle_in - smooth.point).length() - 20.0).abs() < 1e-3);
        assert!(((smooth.handle_out - smooth.point).length() - 10.0).abs() < 1e-3);

        let even = set_point_kind(&[bent], 0, false, PointKind::Symmetric)[0];
        assert_eq!(PointKind::of(&even), PointKind::Symmetric);
        assert!(((even.handle_out - even.point).length() - 15.0).abs() < 1e-3);

        let corner = set_point_kind(&[bent], 0, false, PointKind::Corner)[0];
        assert_eq!(corner, Anchor::corner(p(0.0, 0.0)));
    }

    #[test]
    fn a_corner_made_smooth_gets_handles_along_its_neighbours() {
        let line =
            [Anchor::corner(p(0.0, 0.0)), Anchor::corner(p(50.0, 50.0)), Anchor::corner(p(100.0, 0.0))];
        let smooth = set_point_kind(&line, 1, false, PointKind::Symmetric)[1];
        assert_eq!(PointKind::of(&smooth), PointKind::Symmetric);
        assert!((smooth.handle_out.y - 50.0).abs() < 1e-3, "level, along the neighbours: {smooth:?}");
    }

    #[test]
    fn setting_a_handle_keeps_a_smooth_point_smooth() {
        let uneven = Anchor { point: p(0.0, 0.0), handle_in: p(-20.0, 0.0), handle_out: p(10.0, 0.0) };
        let turned = set_handle(&uneven, AnchorPart::HandleOut, 10.0, 90.0);
        assert!(close(turned.handle_out, p(0.0, 10.0)));
        assert!(close(turned.handle_in, p(0.0, -20.0)), "in line, its own length: {turned:?}");

        let even = Anchor::smooth(p(0.0, 0.0), p(10.0, 0.0));
        let longer = set_handle(&even, AnchorPart::HandleIn, 30.0, 180.0);
        assert!(close(longer.handle_out, p(30.0, 0.0)), "symmetric stays so: {longer:?}");
        let (length, degrees) = handle_polar(&longer, AnchorPart::HandleIn);
        assert!((length - 30.0).abs() < 1e-3 && (degrees.abs() - 180.0).abs() < 1e-3);
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
    fn pulling_handles_out_of_a_corner_makes_it_smooth() {
        let anchors =
            [Anchor::corner(p(0.0, 0.0)), Anchor::corner(p(100.0, 0.0)), Anchor::corner(p(100.0, 100.0))];
        let pulled = pull_handles(&anchors, 1, p(130.0, 20.0));
        let a = pulled[1];
        assert!(close(a.handle_out, p(130.0, 20.0)));
        assert!(close(a.handle_in, p(70.0, -20.0)));
        assert!(is_smooth(&a));
        assert_eq!(pulled[0], anchors[0]);
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
