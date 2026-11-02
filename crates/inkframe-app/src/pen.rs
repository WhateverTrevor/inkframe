//! State for a path being drawn with the Pen tool.
//!
//! Unlike the other tools, a pen path spans many separate clicks, so it
//! lives here between mouse gestures rather than in a single drag. All
//! positions are in stage pixels. The stage handles input and drawing.

use inkframe_core::{Affine, Anchor, LayerId, Path, Point, path_from_anchors};

use crate::subselect::{AnchorPart, curvature_anchors, move_part};

#[derive(Debug, Clone)]
pub struct PenPath {
    anchors: Vec<Anchor>,
    /// What the button held down is doing, between pressing and releasing.
    drag: Option<Drag>,
    /// Where the pointer was last, so Space can move the point being placed
    /// by however far it has gone since.
    pointer: Option<Point>,
    /// The point whose handles show: the newest, or one ⌘-dragged since.
    shown: Option<usize>,
    /// Drawn with the Curvature tool: which points are corners. The curve
    /// flows smoothly through the rest by itself, so the anchors' handles
    /// are worked out from the points after every change rather than drawn.
    corners: Option<Vec<bool>>,
    /// The shape this path carries on from, when it was started on the end
    /// of one: finishing changes that shape rather than adding a layer.
    pub continues: Option<Continued>,
}

/// A shape a pen path carries on from (see [`PenPath::continuing`]).
#[derive(Debug, Clone, Copy)]
pub struct Continued {
    pub layer: LayerId,
    pub element: usize,
    /// From the stage into the layer's own coordinates, where its shapes are.
    pub to_local: Affine,
    /// How many of the anchors were the shape's own: Backspace stops there.
    pub own: usize,
}

/// What a press with the Pen is doing until the button comes up.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    /// Placing a new anchor: dragging pulls its handles out.
    Placing,
    /// Pressed on the newest anchor: dragging pulls a new way out of it, the
    /// way in left as it is; a click takes the way out back. `had_out`
    /// remembers whether there was one, since a click on a point without
    /// one ends the path (so a double-click on the last point does).
    Reaching { had_out: bool },
    /// Pressed on the first anchor to close the path: dragging shapes the
    /// curve through it.
    Closing,
    /// ⌘ held: dragging a point already placed, or a handle showing, as
    /// the Subselection tool would; the path carries on after. `offset` is
    /// from the pointer to what's grabbed, so it doesn't jump.
    Adjusting { index: usize, part: AnchorPart, offset: Point },
}

/// How the newest point's handle follows the pointer while dragging: the
/// keys Animate and Illustrator use.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DragKeys {
    /// Alt/Option: only the handle being dragged moves, breaking the point
    /// into a corner with a curve on each side.
    pub broken: bool,
    /// Shift: the handle keeps to 45° steps.
    pub constrain: bool,
    /// Space: the point itself moves, handles and all, to put it elsewhere.
    pub moving: bool,
}

/// What letting go of the button did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Released {
    Nothing,
    /// The path is done, open: a click on its last point.
    EndOpen,
    /// The path is done, closed: a press on its first point.
    Close,
}

impl PenPath {
    /// Starts a path at `first`, with the button still held.
    pub fn new(first: Point) -> Self {
        Self {
            anchors: vec![Anchor::corner(first)],
            drag: Some(Drag::Placing),
            pointer: None,
            shown: None,
            corners: None,
            continues: None,
        }
    }

    /// Carries on drawing from one end of an open shape: `anchors` in stage
    /// coordinates, in order, `from_start` if the pen took up its first end
    /// (so the anchors are turned round to end there).
    pub fn continuing(mut anchors: Vec<Anchor>, from_start: bool, continued: Continued) -> Self {
        if from_start {
            anchors.reverse();
            for anchor in &mut anchors {
                std::mem::swap(&mut anchor.handle_in, &mut anchor.handle_out);
            }
        }
        let own = anchors.len();
        Self {
            anchors,
            drag: None,
            pointer: None,
            shown: None,
            corners: None,
            continues: Some(Continued { own, ..continued }),
        }
    }

    /// Starts a path with the Curvature tool at `first`, a `corner` or a
    /// point the curve flows through.
    pub fn curving(first: Point, corner: bool) -> Self {
        Self { corners: Some(vec![corner]), ..Self::new(first) }
    }

    /// Whether it's drawn with the Curvature tool (see `corners`).
    pub fn is_curving(&self) -> bool {
        self.corners.is_some()
    }

    /// Places the next point of a Curvature path: a `corner`, or one the
    /// curve flows smoothly through, which bends the curve before it too.
    pub fn add_curve_point(&mut self, p: Point, corner: bool) {
        self.add_anchor(p, false);
        if let Some(corners) = &mut self.corners {
            corners.push(corner);
        }
        self.reflow();
    }

    /// A Curvature path's handles, worked out afresh from its points.
    fn reflow(&mut self) {
        if let Some(corners) = &self.corners {
            let points: Vec<Point> = self.anchors.iter().map(|a| a.point).collect();
            self.anchors = curvature_anchors(&points, corners, false);
        }
    }

    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Where the newest anchor is.
    pub fn last_point(&self) -> Point {
        self.last().point
    }

    fn last(&self) -> &Anchor {
        self.anchors.last().expect("a pen path always has at least one anchor")
    }

    /// Whether clicking at `p` should close the path: it must land on the
    /// first anchor, and there must be something to close.
    pub fn can_close_at(&self, p: Point, tolerance: f32) -> bool {
        self.anchors.len() >= 2 && p.distance(self.anchors[0].point) <= tolerance
    }

    /// Whether `p` lands on the most recent anchor.
    pub fn is_on_last(&self, p: Point, tolerance: f32) -> bool {
        p.distance(self.last().point) <= tolerance
    }

    /// The point whose handles show: the one ⌘-dragged last, or the newest.
    pub fn shown(&self) -> usize {
        self.shown.filter(|&i| i < self.anchors.len()).unwrap_or(self.anchors.len() - 1)
    }

    /// The point or handle of the path at `p`, for ⌘-dragging: the shown
    /// point's handles first (they may sit over a point), then any point.
    pub fn part_at(&self, p: Point, tolerance: f32) -> Option<(usize, AnchorPart)> {
        // A Curvature path's handles aren't shown; only its points move.
        if self.is_curving() {
            return self
                .anchors
                .iter()
                .position(|a| a.point.distance(p) <= tolerance)
                .map(|i| (i, AnchorPart::Point));
        }
        let shown = self.shown();
        let anchor = &self.anchors[shown];
        [AnchorPart::HandleIn, AnchorPart::HandleOut]
            .into_iter()
            .filter(|part| part.position(anchor) != anchor.point)
            .find(|part| part.position(anchor).distance(p) <= tolerance)
            .map(|part| (shown, part))
            .or_else(|| {
                self.anchors
                    .iter()
                    .position(|a| a.point.distance(p) <= tolerance)
                    .map(|i| (i, AnchorPart::Point))
            })
    }

    /// ⌘-press on a point or handle of the path: dragging moves it.
    pub fn begin_adjusting(&mut self, index: usize, part: AnchorPart, pointer: Point) {
        let offset = part.position(&self.anchors[index]) - pointer;
        self.drag = Some(Drag::Adjusting { index, part, offset });
        self.shown = Some(index);
        self.pointer = None;
    }

    /// Places a new corner anchor. With `constrain`, the new point snaps to
    /// the nearest 45° direction from the previous one.
    pub fn add_anchor(&mut self, p: Point, constrain: bool) {
        let p = if constrain { snap_to_45(self.last().point, p) } else { p };
        self.anchors.push(Anchor::corner(p));
        self.shown = None;
        self.drag = Some(Drag::Placing);
        self.pointer = None;
    }

    /// A press on the newest anchor: its way out is taken back, and a drag
    /// pulls a new one, as Illustrator's Pen does over the last point.
    pub fn reach_from_last(&mut self) {
        let last = self.anchors.last_mut().expect("never empty");
        let had_out = last.handle_out != last.point;
        last.handle_out = last.point;
        self.drag = Some(Drag::Reaching { had_out });
        self.pointer = None;
    }

    /// A press on the first anchor: the path will close when the button
    /// comes up, and dragging till then shapes the curve through it.
    pub fn begin_closing(&mut self) {
        self.drag = Some(Drag::Closing);
        self.pointer = None;
    }

    /// Follows the pointer while the button is held, as `keys` say.
    ///
    /// Dragging less than `min_drag` leaves a corner, so a slightly shaky
    /// click doesn't produce a tiny curve.
    pub fn drag_handle(&mut self, pointer: Point, min_drag: f32, keys: DragKeys) {
        let Some(drag) = self.drag else { return };
        // A point or handle already placed: as the Subselection tool drags
        // it, Alt moving a handle alone.
        if let Drag::Adjusting { index, part, offset } = drag {
            self.anchors = move_part(&self.anchors, index, part, pointer + offset, keys.broken);
            self.reflow();
            return;
        }
        // With the Curvature tool, dragging the point just placed moves it,
        // so it can be put just so, the curve following.
        if self.is_curving() {
            if drag == Drag::Placing {
                let last = self.anchors.len() - 1;
                self.anchors[last] = Anchor::corner(pointer);
                self.reflow();
            }
            return;
        }
        let moved = self.pointer.map_or(Point::ZERO, |last| pointer - last);
        self.pointer = Some(pointer);
        let index = if drag == Drag::Closing { 0 } else { self.anchors.len() - 1 };
        let anchor = &mut self.anchors[index];
        if keys.moving {
            // Space: the point goes with the pointer, keeping its handles.
            anchor.point = anchor.point + moved;
            anchor.handle_in = anchor.handle_in + moved;
            anchor.handle_out = anchor.handle_out + moved;
            return;
        }
        let pointer = if keys.constrain { snap_to_45(anchor.point, pointer) } else { pointer };
        let pulled = pointer.distance(anchor.point) > min_drag;
        let reach = if pulled { pointer } else { anchor.point };
        match drag {
            // Broken, only the way out follows; the way in stays as it was.
            Drag::Placing if keys.broken => anchor.handle_out = reach,
            Drag::Placing => *anchor = Anchor::smooth(anchor.point, reach),
            Drag::Reaching { .. } => anchor.handle_out = reach,
            // Closing, the pointer pulls the first point's way out, and the
            // closing curve arrives along its mirror; broken, only the way
            // in (the closing curve's) moves, opposite the pointer.
            Drag::Closing if keys.broken => anchor.handle_in = anchor.point * 2.0 - reach,
            Drag::Closing => *anchor = Anchor::smooth(anchor.point, reach),
            Drag::Adjusting { .. } => unreachable!("handled above"),
        }
    }

    /// The button came up.
    pub fn release(&mut self) -> Released {
        match self.drag.take() {
            Some(Drag::Closing) => Released::Close,
            // A click on a last point that had nothing to take back ends the
            // path, as double-clicking it does in Animate.
            Some(Drag::Reaching { had_out: false }) if self.last().handle_out == self.last().point => {
                Released::EndOpen
            }
            _ => Released::Nothing,
        }
    }

    /// Removes the newest anchor. Returns `false` if nothing is left (or
    /// only the shape being carried on from), in which case the caller
    /// should abandon the path.
    pub fn remove_last(&mut self) -> bool {
        // The shape's own points aren't the pen's to take back.
        let own = self.continues.map_or(0, |c| c.own);
        if self.anchors.len() > own {
            self.anchors.pop();
            if let Some(corners) = &mut self.corners {
                corners.pop();
            }
            self.reflow();
        }
        self.drag = None;
        // A new path needs a point left; one carrying on, a point of its own.
        !self.anchors.is_empty() && (self.continues.is_none() || self.anchors.len() > own)
    }

    /// The segment that would be added by clicking at `pointer`, so the user
    /// can see where the next click will go.
    ///
    /// With the Curvature tool, a point there would bend the curve before
    /// it too, so the preview is the whole path as it would be.
    pub fn preview_to(&self, pointer: Point) -> Option<Path> {
        match &self.corners {
            Some(corners) => {
                let points: Vec<Point> = self.anchors.iter().map(|a| a.point).chain([pointer]).collect();
                let corners: Vec<bool> = corners.iter().copied().chain([false]).collect();
                path_from_anchors(&curvature_anchors(&points, &corners, false), false)
            }
            None => path_from_anchors(&[*self.last(), Anchor::corner(pointer)], false),
        }
    }

    /// The finished path, or `None` if it has fewer than two anchors.
    pub fn finish(&self, closed: bool) -> Option<Path> {
        match &self.corners {
            // Closed, the curve flows round through the first point too.
            Some(corners) => {
                let points: Vec<Point> = self.anchors.iter().map(|a| a.point).collect();
                path_from_anchors(&curvature_anchors(&points, corners, closed), closed)
            }
            None => path_from_anchors(&self.anchors, closed),
        }
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
    use inkframe_core::{Segment, Shape};

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
        pen.drag_handle(p(130.0, 40.0), 1.0, DragKeys::default());
        pen.release();

        let anchor = pen.anchors()[1];
        assert_eq!(anchor.handle_out, p(130.0, 40.0));
        assert_eq!(anchor.handle_in, p(70.0, -40.0));
    }

    #[test]
    fn tiny_drags_stay_corners() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.drag_handle(p(0.5, 0.0), 2.0, DragKeys::default());
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
    fn alt_breaks_the_handle_leaving_the_way_in() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.release();
        pen.add_anchor(p(100.0, 0.0), false);
        pen.drag_handle(p(140.0, 0.0), 1.0, DragKeys::default());
        // Alt pressed mid-drag: the way out swings up, the way in stays put.
        let broken = DragKeys { broken: true, ..DragKeys::default() };
        pen.drag_handle(p(100.0, -40.0), 1.0, broken);
        let anchor = pen.anchors()[1];
        assert_eq!(anchor.handle_out, p(100.0, -40.0));
        assert_eq!(anchor.handle_in, p(60.0, 0.0));
    }

    #[test]
    fn shift_keeps_the_handle_to_45_degrees() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        let constrain = DragKeys { constrain: true, ..DragKeys::default() };
        pen.drag_handle(p(50.0, 4.0), 1.0, constrain);
        assert!(pen.anchors()[0].handle_out.y.abs() < 1e-4);
    }

    #[test]
    fn space_moves_the_point_being_placed() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.drag_handle(p(30.0, 0.0), 1.0, DragKeys::default());
        let moving = DragKeys { moving: true, ..DragKeys::default() };
        pen.drag_handle(p(50.0, 20.0), 1.0, moving);
        let anchor = pen.anchors()[0];
        assert_eq!(anchor.point, p(20.0, 20.0));
        assert_eq!(anchor.handle_out, p(50.0, 20.0), "the handle comes along");
    }

    #[test]
    fn a_click_on_the_last_point_takes_back_its_way_out_then_ends_the_path() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.drag_handle(p(30.0, 30.0), 1.0, DragKeys::default());
        pen.release();
        pen.reach_from_last();
        assert_eq!(pen.release(), Released::Nothing);
        let anchor = pen.anchors()[0];
        assert_eq!(anchor.handle_out, anchor.point, "the way out is gone");
        assert_ne!(anchor.handle_in, anchor.point, "the way in stays");

        pen.add_anchor(p(100.0, 0.0), false);
        pen.release();
        pen.reach_from_last();
        assert_eq!(pen.release(), Released::EndOpen, "nothing to take back: done");
    }

    #[test]
    fn dragging_from_the_last_point_pulls_a_new_way_out() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.release();
        pen.add_anchor(p(100.0, 0.0), false);
        pen.release();
        pen.reach_from_last();
        pen.drag_handle(p(100.0, 50.0), 1.0, DragKeys::default());
        assert_eq!(pen.release(), Released::Nothing);
        let anchor = pen.anchors()[1];
        assert_eq!(anchor.handle_out, p(100.0, 50.0));
        assert_eq!(anchor.handle_in, anchor.point);
    }

    #[test]
    fn dragging_on_the_first_point_closes_with_a_curve() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.release();
        pen.add_anchor(p(100.0, 0.0), false);
        pen.release();
        pen.add_anchor(p(50.0, 80.0), false);
        pen.release();
        pen.begin_closing();
        pen.drag_handle(p(-30.0, 0.0), 1.0, DragKeys::default());
        assert_eq!(pen.release(), Released::Close);
        let first = pen.anchors()[0];
        assert_eq!(first.handle_out, p(-30.0, 0.0));
        assert_eq!(first.handle_in, p(30.0, 0.0));
    }

    #[test]
    fn carrying_on_from_a_shapes_start_turns_it_round_and_keeps_its_points() {
        let shape = [Anchor::corner(p(0.0, 0.0)), Anchor::smooth(p(100.0, 0.0), p(130.0, 0.0))];
        let layer = inkframe_core::Document::default().add_layer("Line", Vec::new(), 0, Default::default());
        let continued = Continued { layer, element: 0, to_local: Affine::IDENTITY, own: 0 };
        let mut pen = PenPath::continuing(shape.to_vec(), true, continued);
        assert_eq!(pen.last_point(), p(0.0, 0.0), "it carries on from the end picked");
        assert_eq!(pen.anchors()[0].handle_in, p(130.0, 0.0), "handles swap round with it");
        pen.add_anchor(p(0.0, 100.0), false);
        pen.add_anchor(p(50.0, 100.0), false);
        assert!(pen.remove_last());
        assert!(!pen.remove_last(), "nothing new left: the carrying on is over");
        assert!(!pen.remove_last());
        assert_eq!(pen.anchors().len(), 2, "the shape's own points stay");
    }

    #[test]
    fn command_dragging_an_earlier_point_moves_it_and_drawing_carries_on() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        pen.release();
        pen.add_anchor(p(100.0, 0.0), false);
        pen.drag_handle(p(130.0, 0.0), 1.0, DragKeys::default());
        pen.release();
        pen.add_anchor(p(200.0, 0.0), false);
        pen.release();

        let (index, part) = pen.part_at(p(101.0, 1.0), 4.0).expect("the middle point");
        assert_eq!((index, part), (1, AnchorPart::Point));
        pen.begin_adjusting(index, part, p(101.0, 1.0));
        pen.drag_handle(p(101.0, 51.0), 1.0, DragKeys::default());
        assert_eq!(pen.release(), Released::Nothing);
        let moved = pen.anchors()[1];
        assert_eq!(moved.point, p(100.0, 50.0), "no jump from where it was grabbed");
        assert_eq!(moved.handle_out, p(130.0, 50.0), "its handles come along");
        assert_eq!(pen.shown(), 1, "its handles show now");

        // Its handle, now showing, can be grabbed too; Alt moves it alone.
        let (index, part) = pen.part_at(p(130.0, 50.0), 4.0).unwrap();
        assert_eq!(part, AnchorPart::HandleOut);
        pen.begin_adjusting(index, part, p(130.0, 50.0));
        pen.drag_handle(p(130.0, 80.0), 1.0, DragKeys { broken: true, ..DragKeys::default() });
        pen.release();
        assert_eq!(pen.anchors()[1].handle_in, p(70.0, 50.0), "broken: the way in stays");

        pen.add_anchor(p(300.0, 0.0), false);
        assert_eq!(pen.anchors().len(), 4);
        assert_eq!(pen.shown(), 3, "the newest shows again");
    }

    #[test]
    fn the_curvature_tool_flows_through_its_points_but_keeps_corners() {
        let mut pen = PenPath::curving(p(0.0, 0.0), false);
        pen.release();
        pen.add_curve_point(p(100.0, -50.0), false);
        pen.release();
        pen.add_curve_point(p(200.0, 0.0), false);
        pen.release();
        // The middle point is smooth, level between its neighbours.
        let middle = pen.anchors()[1];
        assert!((middle.handle_out.y - middle.point.y).abs() < 1e-3, "{middle:?}");
        assert!(middle.handle_out.x > middle.point.x);

        pen.add_curve_point(p(300.0, -50.0), true);
        pen.release();
        assert_eq!(pen.anchors()[3], Anchor::corner(p(300.0, -50.0)), "Alt-click: a corner");
        // Closing flows round through the first point.
        let closed = pen.finish(true).unwrap();
        let (anchors, _) = Shape::Path(closed).to_anchors();
        assert_ne!(anchors[0].handle_out, anchors[0].point, "the first point curves, closed");

        assert!(pen.remove_last());
        assert_eq!(pen.anchors().len(), 3);
        assert_eq!(pen.part_at(p(100.0, -50.0), 4.0), Some((1, AnchorPart::Point)));
    }

    #[test]
    fn removing_the_only_anchor_abandons_the_path() {
        let mut pen = PenPath::new(p(0.0, 0.0));
        assert!(!pen.remove_last());
    }
}
