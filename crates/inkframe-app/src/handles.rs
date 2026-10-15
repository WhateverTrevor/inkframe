//! The math behind the free-transform handles on a selected layer.
//!
//! This module only deals in stage coordinates and `Transform`s. Drawing the
//! handles and hit-testing them on screen lives in `stage.rs`.
//!
//! All edits are computed from the pose at the *start* of the drag rather
//! than applied incrementally. Small per-frame rounding errors therefore
//! never accumulate, and dragging back to where you started restores the
//! original pose exactly.

use inkframe_core::{Affine, Point, Rect, Transform};

/// One of the eight scale handles around a layer's bounding box.
///
/// A handle is described by which side of the box it sits on along each
/// axis: -1 (left/top), 0 (middle) or +1 (right/bottom). Corners move both
/// axes; edge handles move only one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaleHandle {
    pub x: i8,
    pub y: i8,
}

impl ScaleHandle {
    pub const ALL: [ScaleHandle; 8] = [
        ScaleHandle { x: -1, y: -1 },
        ScaleHandle { x: 0, y: -1 },
        ScaleHandle { x: 1, y: -1 },
        ScaleHandle { x: 1, y: 0 },
        ScaleHandle { x: 1, y: 1 },
        ScaleHandle { x: 0, y: 1 },
        ScaleHandle { x: -1, y: 1 },
        ScaleHandle { x: -1, y: 0 },
    ];

    pub const TOP: ScaleHandle = ScaleHandle { x: 0, y: -1 };

    pub fn is_corner(self) -> bool {
        self.x != 0 && self.y != 0
    }

    pub fn opposite(self) -> ScaleHandle {
        ScaleHandle { x: -self.x, y: -self.y }
    }

    /// Where this handle sits on `bounds`, in the layer's local coordinates.
    pub fn local_position(self, bounds: Rect) -> Point {
        let c = bounds.center();
        Point::new(c.x + self.x as f32 * bounds.width() / 2.0, c.y + self.y as f32 * bounds.height() / 2.0)
    }
}

/// Modifier-key options for scaling.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScaleOptions {
    /// Keep the aspect ratio (Shift). Only applies to corner handles.
    pub proportional: bool,
    /// Scale around the layer's pivot instead of the opposite handle (Alt/Option).
    pub from_pivot: bool,
}

/// The pose that puts `handle` under `pointer` (stage pixels), starting
/// from `start`.
///
/// By default the opposite handle stays pinned in place, which is how
/// Animate, Illustrator and Figma behave. That takes two steps:
///
/// 1. Work in the layer's *rotated* frame (rotation undone, scale kept), where
///    a local point `x` sits at `scale ⊙ x` relative to the pivot (`⊙` is
///    per-axis multiplication). In that frame, solving "handle lands on the
///    pointer while the anchor stays put" gives each axis's new scale
///    directly: `s' = (pointer − s·anchor) / (handle − anchor)`.
/// 2. Changing the scale moves every point that isn't on the pivot,
///    including the anchor. Shift the position by however far the anchor
///    moved, so it ends up exactly where it was.
pub fn scale_pose(
    start: &Transform,
    bounds: Rect,
    handle: ScaleHandle,
    pointer: Point,
    options: ScaleOptions,
) -> Transform {
    // Below this, a box side is too thin to scale meaningfully (for example
    // the height of a perfectly straight horizontal pencil line).
    const MIN_SPAN: f32 = 1e-3;

    // The layer's frame with scale left in: rotation and skew undone. (With
    // no skew this is just the rotation.)
    let frame = turn_and_slant(start);
    let Some(into_frame) = frame.inverse() else { return *start };
    let q = into_frame.apply(pointer - start.position);

    let h = handle.local_position(bounds);
    let a = if options.from_pivot { Point::ZERO } else { handle.opposite().local_position(bounds) };
    let s = start.scale;
    let anchor = Point::new(s.x * a.x, s.y * a.y);
    let span = h - a;

    let mut scale = s;
    if options.proportional && handle.is_corner() {
        // One shared factor for both axes: project the pointer onto the
        // anchor→handle diagonal and see how far along it is. Projecting
        // (instead of using only x or only y) means the box follows the
        // pointer smoothly whichever way the user drags.
        let diagonal = Point::new(s.x * span.x, s.y * span.y);
        let length_sq = diagonal.dot(diagonal);
        if length_sq > MIN_SPAN {
            let factor = (q - anchor).dot(diagonal) / length_sq;
            scale = s * factor;
        }
    } else {
        if handle.x != 0 && span.x.abs() > MIN_SPAN {
            scale.x = (q.x - anchor.x) / span.x;
        }
        if handle.y != 0 && span.y.abs() > MIN_SPAN {
            scale.y = (q.y - anchor.y) / span.y;
        }
    }

    let anchor_drift = Point::new((s.x - scale.x) * a.x, (s.y - scale.y) * a.y);
    let position = start.position + frame.apply(anchor_drift);
    Transform { position, scale, ..*start }
}

/// A layer's rotation and skew together, without its scale or position.
fn turn_and_slant(pose: &Transform) -> Affine {
    let skew = Point::new(pose.skew.x.to_radians(), pose.skew.y.to_radians());
    Affine::rotate(pose.rotation.to_radians()).pre_concat(Affine::skew(skew))
}

/// The pose that slants the layer by dragging the edge `edge` (an edge
/// handle: top, bottom, left or right) from `grab` to `pointer`, both in
/// stage coordinates, starting from `start`. The opposite edge stays put,
/// as with Animate's Free Transform.
///
/// Working in the layer's rotated frame, a point `p` on the layer sits at
/// `(sx·px − tan(kx)·sy·py, tan(ky)·sx·px + sy·py)`. So sliding the top edge
/// sideways by `d` relative to the bottom one changes `tan(kx)` by
/// `−d / (sy·(top − bottom))`, and likewise for the side edges and `ky`.
/// Then, as for scaling, the position shifts so the opposite edge's middle
/// lands back where it was.
pub fn skew_pose(
    start: &Transform,
    bounds: Rect,
    edge: ScaleHandle,
    grab: Point,
    pointer: Point,
) -> Transform {
    // Beyond this a layer would slant into a line.
    const LIMIT: f32 = 89.0;
    let into_rotated = Affine::rotate(-start.rotation.to_radians());
    let moved = into_rotated.apply(pointer - grab);
    let (e, o) = (edge.local_position(bounds), edge.opposite().local_position(bounds));
    let s = start.scale;

    let mut skew = start.skew;
    if edge.y != 0 && (s.y * (e.y - o.y)).abs() > 1e-3 {
        let tan = start.skew.x.to_radians().tan() - moved.x / (s.y * (e.y - o.y));
        skew.x = tan.atan().to_degrees().clamp(-LIMIT, LIMIT);
    } else if edge.x != 0 && (s.x * (e.x - o.x)).abs() > 1e-3 {
        let tan = start.skew.y.to_radians().tan() + moved.y / (s.x * (e.x - o.x));
        skew.y = tan.atan().to_degrees().clamp(-LIMIT, LIMIT);
    }

    let slanted = Transform { skew, ..*start };
    let pinned = Point::new(s.x * o.x, s.y * o.y);
    let drift = turn_and_slant(start).apply(pinned) - turn_and_slant(&slanted).apply(pinned);
    Transform { position: start.position + drift, ..slanted }
}

/// Tracks a rotation drag around a layer's pivot.
///
/// Angles from `atan2` jump by a full turn when the pointer crosses the
/// line to the left of the pivot. Accumulating small, wrapped steps instead
/// of using the raw angle lets the user keep circling to rotate past 360°,
/// which matters in animation: tweening 0° → 720° spins twice.
#[derive(Debug, Clone, Copy)]
pub struct RotateDrag {
    start: Transform,
    last_angle: f32,
    turned: f32,
}

impl RotateDrag {
    pub fn new(start: Transform, pointer: Point) -> Self {
        Self { start, last_angle: angle_from(start.position, pointer), turned: 0.0 }
    }

    /// The pose when the drag began.
    pub fn start(&self) -> Transform {
        self.start
    }

    /// Returns the pose for the pointer's new position. With `snap`, the
    /// rotation lands on multiples of 15°.
    pub fn update(&mut self, pointer: Point, snap: bool) -> Transform {
        const SNAP_DEGREES: f32 = 15.0;

        let angle = angle_from(self.start.position, pointer);
        self.turned += wrap_angle(angle - self.last_angle);
        self.last_angle = angle;

        let mut rotation = self.start.rotation + self.turned.to_degrees();
        if snap {
            rotation = (rotation / SNAP_DEGREES).round() * SNAP_DEGREES;
        }
        Transform { rotation, ..self.start }
    }
}

/// Angle of `point` around `pivot`, in radians. Because stage y points down,
/// increasing angles turn clockwise, matching `Transform::rotation`.
fn angle_from(pivot: Point, point: Point) -> f32 {
    let d = point - pivot;
    d.y.atan2(d.x)
}

/// Wraps an angle difference into (−π, π], i.e. the shortest way around.
fn wrap_angle(radians: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let wrapped = (radians + PI).rem_euclid(TAU) - PI;
    if wrapped == -PI { PI } else { wrapped }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100×50 box centered on the pivot.
    fn bounds() -> Rect {
        Rect::from_center_size(Point::ZERO, 100.0, 50.0)
    }

    fn stage_point(pose: &Transform, local: Point) -> Point {
        pose.to_affine().apply(local)
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-3
    }

    const BOTTOM_RIGHT: ScaleHandle = ScaleHandle { x: 1, y: 1 };
    const RIGHT: ScaleHandle = ScaleHandle { x: 1, y: 0 };

    #[test]
    fn dragging_the_top_edge_slants_with_the_bottom_pinned() {
        // The 100×50 box around its pivot; drag the top edge 50 px right,
        // as far as the box is tall.
        let start = Transform::at(Point::new(200.0, 200.0));
        let top = ScaleHandle { x: 0, y: -1 };
        let grab = Point::new(200.0, 175.0);
        let pose = skew_pose(&start, bounds(), top, grab, grab + Point::new(50.0, 0.0));

        let to_stage = pose.to_affine();
        let bottom_middle = to_stage.apply(Point::new(0.0, 25.0));
        let top_middle = to_stage.apply(Point::new(0.0, -25.0));
        assert!(close(bottom_middle, Point::new(200.0, 225.0)), "bottom stays: {bottom_middle:?}");
        assert!(close(top_middle, Point::new(250.0, 175.0)), "top follows: {top_middle:?}");
        assert!((pose.skew.x - 45.0).abs() < 0.01, "a 50 px lean over 50 px of height: {}", pose.skew.x);
    }

    #[test]
    fn scaling_a_skewed_layer_keeps_the_opposite_corner_pinned() {
        let start = Transform {
            skew: Point::new(20.0, 0.0),
            rotation: 15.0,
            ..Transform::at(Point::new(100.0, 100.0))
        };
        let pinned = stage_point(&start, ScaleHandle { x: -1, y: -1 }.local_position(bounds()));
        let pointer = stage_point(&start, BOTTOM_RIGHT.local_position(bounds())) + Point::new(30.0, 20.0);
        let pose = scale_pose(&start, bounds(), BOTTOM_RIGHT, pointer, ScaleOptions::default());
        assert!(close(stage_point(&pose, ScaleHandle { x: -1, y: -1 }.local_position(bounds())), pinned));
        assert!(close(stage_point(&pose, BOTTOM_RIGHT.local_position(bounds())), pointer));
    }

    #[test]
    fn corner_drag_pins_the_opposite_corner() {
        let start = Transform::at(Point::new(500.0, 300.0));
        let pinned = stage_point(&start, ScaleHandle { x: -1, y: -1 }.local_position(bounds()));

        // Drag the bottom-right corner 100 px right and 50 px down: the box
        // should double in size while the top-left corner stays put.
        let pointer = Point::new(650.0, 375.0);
        let pose = scale_pose(&start, bounds(), BOTTOM_RIGHT, pointer, ScaleOptions::default());

        assert!(close(pose.scale, Point::new(2.0, 2.0)), "{:?}", pose.scale);
        assert!(close(stage_point(&pose, ScaleHandle { x: -1, y: -1 }.local_position(bounds())), pinned));
        assert!(close(stage_point(&pose, BOTTOM_RIGHT.local_position(bounds())), pointer));
    }

    #[test]
    fn edge_handles_change_one_axis() {
        let start = Transform::at(Point::new(500.0, 300.0));
        let pose = scale_pose(&start, bounds(), RIGHT, Point::new(600.0, 999.0), ScaleOptions::default());
        assert!((pose.scale.x - 1.5).abs() < 1e-4);
        assert_eq!(pose.scale.y, 1.0);
    }

    #[test]
    fn works_on_rotated_layers() {
        let start = Transform { rotation: 90.0, ..Transform::at(Point::new(500.0, 300.0)) };
        let pinned_local = ScaleHandle { x: -1, y: -1 }.local_position(bounds());
        let pinned = stage_point(&start, pinned_local);

        // Rotated 90° clockwise, the box's local +x points down the screen.
        // Dragging the right-edge handle down by 50 px should widen it 1.5×.
        let pointer = stage_point(&start, RIGHT.local_position(bounds())) + Point::new(0.0, 50.0);
        let pose = scale_pose(&start, bounds(), RIGHT, pointer, ScaleOptions::default());

        assert!((pose.scale.x - 1.5).abs() < 1e-3, "{:?}", pose.scale);
        assert!(close(stage_point(&pose, pinned_local), pinned));
    }

    #[test]
    fn proportional_keeps_the_aspect_ratio() {
        let start = Transform::at(Point::ZERO);
        let options = ScaleOptions { proportional: true, ..ScaleOptions::default() };
        // Mostly horizontal drag; both axes should still scale equally.
        let pose = scale_pose(&start, bounds(), BOTTOM_RIGHT, Point::new(150.0, 30.0), options);
        assert!((pose.scale.x - pose.scale.y).abs() < 1e-5);
        assert!(pose.scale.x > 1.0);
    }

    #[test]
    fn from_pivot_scales_symmetrically() {
        let start = Transform::at(Point::new(200.0, 200.0));
        let options = ScaleOptions { from_pivot: true, ..ScaleOptions::default() };
        let pose = scale_pose(&start, bounds(), RIGHT, Point::new(300.0, 200.0), options);
        assert!((pose.scale.x - 2.0).abs() < 1e-4);
        assert_eq!(pose.position, start.position);
    }

    #[test]
    fn dragging_past_the_anchor_flips() {
        let start = Transform::at(Point::ZERO);
        let pose = scale_pose(&start, bounds(), RIGHT, Point::new(-150.0, 0.0), ScaleOptions::default());
        assert!(pose.scale.x < 0.0);
    }

    #[test]
    fn rotation_accumulates_past_a_full_turn() {
        let start = Transform::at(Point::ZERO);
        let mut drag = RotateDrag::new(start, Point::new(1.0, 0.0));
        // Walk the pointer around the pivot one and a half times, clockwise.
        let mut pose = start;
        for step in 1..=24 {
            let a = step as f32 * std::f32::consts::FRAC_PI_8;
            pose = drag.update(Point::new(a.cos(), a.sin()), false);
        }
        assert!((pose.rotation - 540.0).abs() < 1e-2, "{}", pose.rotation);
    }

    #[test]
    fn rotation_snaps_to_15_degrees() {
        let start = Transform::at(Point::ZERO);
        let mut drag = RotateDrag::new(start, Point::new(1.0, 0.0));
        let a = 50f32.to_radians();
        let pose = drag.update(Point::new(a.cos(), a.sin()), true);
        assert_eq!(pose.rotation, 45.0);
    }

    #[test]
    fn wrap_angle_takes_the_short_way() {
        use std::f32::consts::PI;
        assert!((wrap_angle(1.5 * PI) + 0.5 * PI).abs() < 1e-5);
        assert!((wrap_angle(-1.5 * PI) - 0.5 * PI).abs() < 1e-5);
    }
}
