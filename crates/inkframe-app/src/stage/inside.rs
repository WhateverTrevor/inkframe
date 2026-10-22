//! The stage while working inside a layer (see `crate::inside`): clicks
//! pick shapes rather than layers, dragging moves the picked shapes, the
//! corner handles scale them, the knob above turns them, and dragging over
//! empty stage picks every shape the box touches.

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, StrokeKind, Vec2, vec2};
use inkframe_core::{Affine, Element, Point};

use super::{HANDLE_SIZE, HIT_TOLERANCE, SELECTION, StageView, View};
use crate::editor::{Editor, Tool};
use crate::handles::RotateDrag;
use crate::inside::ShapeRef;

/// How far the rotate knob sits beyond the top of the picked shapes' box,
/// on screen.
const KNOB_DISTANCE: f32 = 22.0;

/// A drag in progress inside a layer. Every update works from the shapes
/// as they were when it began, so nothing drifts.
pub(super) enum ShapeDrag {
    /// Moving the picked shapes. `grab` is where the drag began, in the
    /// timeline's coordinates; `to_local` maps those to the layer's.
    Move { originals: Vec<(ShapeRef, Element)>, grab: Point, to_local: Affine },
    /// Scaling the picked shapes by a corner, about the opposite corner
    /// (`anchor`), both in the layer's coordinates.
    Scale { originals: Vec<(ShapeRef, Element)>, anchor: Point, corner: Point, to_local: Affine },
    /// Turning the picked shapes about their box's middle (`center`, in
    /// the layer's coordinates), with Shift snapping to 15°.
    Rotate { originals: Vec<(ShapeRef, Element)>, drag: RotateDrag, center: Point, to_local: Affine },
    /// Dragging a box over empty stage. `adding` keeps what's picked
    /// (Shift held).
    Box { start: Point, current: Point, adding: bool },
}

impl StageView {
    /// Handles the pointer while working inside a layer with the Selection
    /// tool. Returns whether it did, in which case the usual tools leave the
    /// pointer alone.
    pub(super) fn handle_inside(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        editor.check_inside();
        if editor.inside_layer().is_none() || editor.tool != Tool::Select {
            self.shape_drag = None;
            return false;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let tolerance = HIT_TOLERANCE / view.zoom;

        // Double-clicking empty stage steps out; double-clicking another
        // layer steps into that one instead.
        if response.double_clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let p = view.to_local(pointer);
            if editor.shape_at(p, tolerance).is_none() {
                let other = editor.hit_test(p, tolerance).filter(|&id| Some(id) != editor.inside_layer());
                editor.exit_layer();
                if let Some(other) = other {
                    editor.enter_layer(other);
                }
            }
            return true;
        }

        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(press) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
        {
            let p = view.to_local(press);
            self.shape_drag = self.begin_shape_drag(editor, view, press, p, shift, tolerance);
        }

        if let Some(pointer) = response.interact_pointer_pos() {
            let p = view.to_local(pointer);
            match &mut self.shape_drag {
                Some(ShapeDrag::Move { originals, grab, to_local }) => {
                    let mut delta = p - *grab;
                    if shift {
                        // Along the main direction only, as when moving layers.
                        if delta.x.abs() > delta.y.abs() { delta.y = 0.0 } else { delta.x = 0.0 }
                    }
                    let local = to_local.apply(*grab + delta) - to_local.apply(*grab);
                    let originals = originals.clone();
                    editor.transform_shapes(&originals, Affine::translate(local));
                }
                Some(ShapeDrag::Scale { originals, anchor, corner, to_local }) => {
                    let q = to_local.apply(p);
                    let factor = |now: f32, from: f32, about: f32| {
                        let span = from - about;
                        if span.abs() < 1e-3 { 1.0 } else { (now - about) / span }
                    };
                    let (mut sx, mut sy) = (factor(q.x, corner.x, anchor.x), factor(q.y, corner.y, anchor.y));
                    if shift {
                        // Keep the proportions: follow whichever way moved more.
                        let s = if sx.abs() > sy.abs() { sx } else { sy };
                        (sx, sy) = (s, s);
                    }
                    let m = Affine::translate(*anchor)
                        .pre_concat(Affine::scale(sx, sy))
                        .pre_concat(Affine::translate(-*anchor));
                    let originals = originals.clone();
                    editor.transform_shapes(&originals, m);
                }
                Some(ShapeDrag::Rotate { originals, drag, center, to_local }) => {
                    let turned = drag.update(to_local.apply(p), shift).rotation.to_radians();
                    let m = Affine::translate(*center)
                        .pre_concat(Affine::rotate(turned))
                        .pre_concat(Affine::translate(-*center));
                    let originals = originals.clone();
                    editor.transform_shapes(&originals, m);
                }
                Some(ShapeDrag::Box { current, .. }) => *current = p,
                None => {}
            }
        }

        if response.drag_stopped() {
            if let Some(ShapeDrag::Box { start, current, adding }) = self.shape_drag.take() {
                let mut picked = if adding { editor.picked_shapes().to_vec() } else { Vec::new() };
                picked.extend(editor.shapes_in(start, current));
                editor.pick_shapes(picked);
            }
            self.shape_drag = None;
        }

        // A click picks the shape under it (Shift adds or takes it away);
        // a click on nothing picks nothing.
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            match editor.shape_at(view.to_local(pointer), tolerance) {
                Some(shape) if shift => editor.toggle_shape(shape),
                Some(shape) => editor.pick_shapes(vec![shape]),
                None if shift => {}
                None => editor.pick_shapes(Vec::new()),
            }
        }
        true
    }

    fn begin_shape_drag(
        &self,
        editor: &mut Editor,
        view: View,
        press: Pos2,
        p: Point,
        shift: bool,
        tolerance: f32,
    ) -> Option<ShapeDrag> {
        let layer = editor.doc.layer(editor.inside_layer()?)?;
        let to_local = editor.doc.layer_matrix(layer, editor.frame).inverse()?;

        // The knob above the picked shapes turns them.
        if let Some((knob, center)) = rotate_knob(editor, view)
            && knob.distance(press) <= HANDLE_SIZE
        {
            let start = inkframe_core::Transform::at(center);
            let drag = RotateDrag::new(start, to_local.apply(p));
            return Some(ShapeDrag::Rotate { originals: editor.picked_originals(), drag, center, to_local });
        }
        // A corner handle of the picked shapes' box scales them.
        if let Some((corner, anchor)) = picked_corners(editor)
            .into_iter()
            .find(|(corner, _)| view.to_screen(corner.1).distance(press) <= HANDLE_SIZE)
            .map(|(corner, anchor)| (corner.0, anchor))
        {
            return Some(ShapeDrag::Scale { originals: editor.picked_originals(), anchor, corner, to_local });
        }
        match editor.shape_at(p, tolerance) {
            Some(shape) => {
                // Dragging a shape that isn't picked picks it (or adds it,
                // with Shift) and moves it.
                if !editor.picked_shapes().contains(&shape) {
                    if shift {
                        editor.toggle_shape(shape);
                    } else {
                        editor.pick_shapes(vec![shape]);
                    }
                }
                Some(ShapeDrag::Move { originals: editor.picked_originals(), grab: p, to_local })
            }
            None => Some(ShapeDrag::Box { start: p, current: p, adding: shift }),
        }
    }

    /// Outlines the picked shapes and their box with its corner handles,
    /// the box being dragged, and a note saying how to step back out.
    pub(super) fn draw_inside(&self, painter: &egui::Painter, editor: &Editor, view: View) {
        let Some(layer) = editor.inside_layer().and_then(|id| editor.doc.layer(id)) else { return };
        // A Distort or Envelope draws its own handles and note.
        if editor.lattice.is_some() {
            return;
        }
        let to_timeline = editor.doc.layer_matrix(layer, editor.frame);
        let on_screen = |p: Point| view.to_screen(to_timeline.apply(p));

        // Each picked shape's own box, faintly; then the box around them all.
        for (_, shape) in editor.picked_originals() {
            let corners = shape.bounds().corners().map(on_screen);
            painter.add(egui::Shape::closed_line(corners.to_vec(), (1.0, SELECTION.gamma_multiply(0.6))));
        }
        if let Some(bounds) = editor.picked_bounds() {
            let corners = bounds.corners().map(on_screen);
            painter.add(egui::Shape::closed_line(corners.to_vec(), (1.0, SELECTION)));
            if let Some((knob, _)) = rotate_knob(editor, view) {
                let top = corners[0].lerp(corners[1], 0.5);
                painter.line_segment([top, knob], (1.0, SELECTION));
                painter.circle(knob, HANDLE_SIZE / 2.0 + 1.0, Color32::WHITE, (1.0, SELECTION));
            }
            for ((_, at), _) in picked_corners(editor) {
                let handle = Rect::from_center_size(view.to_screen(at), Vec2::splat(HANDLE_SIZE));
                painter.rect_filled(handle, 1.0, Color32::WHITE);
                painter.rect_stroke(handle, 1.0, (1.0, SELECTION), StrokeKind::Inside);
            }
        }
        if let Some(ShapeDrag::Box { start, current, .. }) = &self.shape_drag {
            let area = Rect::from_two_pos(view.to_screen(*start), view.to_screen(*current));
            painter.rect_filled(area, 0.0, SELECTION.gamma_multiply(0.12));
            painter.rect_stroke(area, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }

        // Where you are, along the top of the stage.
        let picked = editor.picked_shapes().len();
        let what = match picked {
            0 => "click shapes to pick them".to_owned(),
            1 => "1 shape picked".to_owned(),
            n => format!("{n} shapes picked"),
        };
        let note = format!("Inside \u{201c}{}\u{201d} · {what} · Esc to step out", layer.name);
        let area = painter.clip_rect();
        let galley = painter.layout_no_wrap(note, FontId::proportional(12.0), Color32::WHITE);
        let pill = Rect::from_min_size(area.left_top() + vec2(10.0, 10.0), galley.size() + vec2(20.0, 10.0));
        painter.rect_filled(pill, pill.height() / 2.0, SELECTION.gamma_multiply(0.85));
        painter.galley(pill.min + vec2(10.0, 5.0), galley, Color32::WHITE);
    }

    /// The cursor while inside a layer: a hand over shapes, arrows over the
    /// scale handles.
    pub(super) fn inside_cursor(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &Editor,
        view: View,
    ) {
        if editor.inside_layer().is_none() || editor.tool != Tool::Select || editor.lattice.is_some() {
            return;
        }
        let cursor = match &self.shape_drag {
            Some(ShapeDrag::Move { .. }) => CursorIcon::Grabbing,
            Some(ShapeDrag::Scale { .. }) => CursorIcon::ResizeNwSe,
            Some(ShapeDrag::Rotate { .. }) => CursorIcon::Grabbing,
            Some(ShapeDrag::Box { .. }) => CursorIcon::Crosshair,
            None => {
                let Some(pointer) = response.hover_pos() else { return };
                let on_handle = picked_corners(editor)
                    .iter()
                    .any(|((_, at), _)| view.to_screen(*at).distance(pointer) <= HANDLE_SIZE);
                let on_knob =
                    rotate_knob(editor, view).is_some_and(|(knob, _)| knob.distance(pointer) <= HANDLE_SIZE);
                if on_knob {
                    CursorIcon::Grab
                } else if on_handle {
                    CursorIcon::ResizeNwSe
                } else if editor.shape_at(view.to_local(pointer), HIT_TOLERANCE / view.zoom).is_some() {
                    CursorIcon::Grab
                } else {
                    CursorIcon::Default
                }
            }
        };
        ui.ctx().set_cursor_icon(cursor);
    }
}

/// The picked shapes' box's corners: for each, the corner in the layer's
/// coordinates and in the timeline's, and the opposite corner (in the
/// layer's), which stays put when that corner is dragged.
#[allow(clippy::type_complexity)]
fn picked_corners(editor: &Editor) -> Vec<((Point, Point), Point)> {
    let Some(bounds) = editor.picked_bounds() else { return Vec::new() };
    let Some(layer) = editor.inside_layer().and_then(|id| editor.doc.layer(id)) else { return Vec::new() };
    let to_timeline = editor.doc.layer_matrix(layer, editor.frame);
    let corners = bounds.corners();
    (0..4).map(|i| ((corners[i], to_timeline.apply(corners[i])), corners[(i + 2) % 4])).collect()
}

/// Where the rotate knob is on screen, beyond the middle of the top of the
/// picked shapes' box (following the layer if it's turned), and the box's
/// middle in the layer's coordinates, which they turn about.
pub(super) fn rotate_knob(editor: &Editor, view: View) -> Option<(Pos2, Point)> {
    let bounds = editor.picked_bounds()?;
    let layer = editor.doc.layer(editor.inside_layer()?)?;
    let to_timeline = editor.doc.layer_matrix(layer, editor.frame);
    let on_screen = |p: Point| view.to_screen(to_timeline.apply(p));
    let corners = bounds.corners();
    let top = on_screen(corners[0].lerp(corners[1], 0.5));
    let middle = on_screen(bounds.center());
    let up = (top - middle).normalized();
    // A shape with no height (a straight line) has no "up"; go up the screen.
    let up = if up.is_finite() && up.length() > 0.5 { up } else { vec2(0.0, -1.0) };
    Some((top + up * KNOB_DISTANCE, bounds.center()))
}
