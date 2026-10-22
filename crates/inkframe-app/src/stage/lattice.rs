//! The handles of a Distort or Envelope on the stage (see `envelope.rs`):
//! drag one to bend the shapes under it. An Envelope's points have
//! smaller handles on stalks, as the pen's do; Alt-dragging one of those
//! moves it on its own, kinking the side. Clicking or dragging anywhere
//! else leaves the shapes bent and carries on as a click there would.

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, StrokeKind, Vec2, vec2};
use inkframe_core::{Affine, Point};

use super::{HANDLE_SIZE, SELECTION, StageView, View};
use crate::editor::Editor;

/// From the lattice's layer's coordinates to the timeline's.
fn layer_matrix(editor: &Editor) -> Option<Affine> {
    let edit = editor.lattice.as_ref()?;
    Some(editor.doc.layer_matrix(editor.doc.layer(edit.layer())?, editor.frame))
}

/// Every handle on screen, in the lattice's order.
fn handles(editor: &Editor, view: View) -> Vec<Pos2> {
    let (Some(edit), Some(m)) = (&editor.lattice, layer_matrix(editor)) else { return Vec::new() };
    edit.lattice.points.iter().map(|p| view.to_screen(m.apply(*p))).collect()
}

/// The handle under `pointer`, the nearest if several are close.
fn handle_at(editor: &Editor, view: View, pointer: Pos2) -> Option<usize> {
    handles(editor, view)
        .into_iter()
        .enumerate()
        .map(|(i, at)| (i, at.distance(pointer)))
        .filter(|(_, d)| *d <= HANDLE_SIZE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

impl StageView {
    /// Drags the lattice's handles. Returns whether the pointer is busy
    /// with one; pressing anywhere else ends the edit, and the press goes
    /// on to the tools.
    pub(super) fn handle_lattice(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        editor.check_lattice();
        if editor.lattice.is_none() {
            self.lattice_drag = None;
            return false;
        }
        let pressed = response.drag_started_by(egui::PointerButton::Primary)
            || response.clicked_by(egui::PointerButton::Primary);
        if pressed
            && self.lattice_drag.is_none()
            && let Some(press) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
        {
            match handle_at(editor, view, press) {
                Some(index) if response.drag_started() => self.lattice_drag = Some(index),
                Some(_) => return true,
                None => {
                    editor.finish_lattice();
                    return false;
                }
            }
        }
        let Some(index) = self.lattice_drag else { return false };
        if let Some(pointer) = response.interact_pointer_pos()
            && let Some(to_local) = layer_matrix(editor).and_then(|m| m.inverse())
        {
            let smooth = !ui.input(|i| i.modifiers.alt);
            editor.move_lattice_point(index, to_local.apply(view.to_local(pointer)), smooth);
        }
        if response.drag_stopped() {
            self.lattice_drag = None;
        }
        true
    }

    /// The lattice: the bent box's outline, the lines joining the handles,
    /// the handles (square at the corners), a note saying how to finish,
    /// and a cursor that says the handles can be moved.
    pub(super) fn draw_lattice(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        editor: &Editor,
        view: View,
    ) {
        let (Some(edit), Some(m)) = (&editor.lattice, layer_matrix(editor)) else { return };
        let lattice = &edit.lattice;
        let on_screen = |p: Point| view.to_screen(m.apply(p));

        // The box's edges as the handles have bent them.
        let mut outline = Vec::new();
        for [a, b, c, d] in lattice.outline() {
            outline.extend((0..16).map(|k| {
                let t = k as f32 / 16.0;
                let s = 1.0 - t;
                on_screen(a * (s * s * s) + b * (3.0 * s * s * t) + c * (3.0 * s * t * t) + d * (t * t * t))
            }));
        }
        painter.add(egui::Shape::closed_line(outline, (1.0, SELECTION)));

        // Each Envelope handle on a stalk from its point, under the points.
        let points = handles(editor, view);
        for (i, at) in points.iter().enumerate() {
            if let Some(point) = lattice.handle_of(i) {
                painter.line_segment([points[point], *at], (1.0, SELECTION.gamma_multiply(0.6)));
            }
        }
        for (i, at) in points.iter().enumerate() {
            let fill = if self.lattice_drag == Some(i) { SELECTION } else { Color32::WHITE };
            if lattice.handle_of(i).is_some() {
                painter.circle(*at, HANDLE_SIZE / 2.0 - 1.5, fill, (1.0, SELECTION));
            } else if lattice.is_corner(i) {
                let square = Rect::from_center_size(*at, Vec2::splat(HANDLE_SIZE));
                painter.rect_filled(square, 1.0, fill);
                painter.rect_stroke(square, 1.0, (1.0, SELECTION), StrokeKind::Inside);
            } else {
                painter.circle(*at, HANDLE_SIZE / 2.0, fill, (1.0, SELECTION));
            }
        }

        let note = format!(
            "{} · drag the handles (Alt: one side only) · Enter or click away to finish",
            edit.kind.name()
        );
        let area = painter.clip_rect();
        let galley = painter.layout_no_wrap(note, FontId::proportional(12.0), Color32::WHITE);
        let pill = Rect::from_min_size(area.left_top() + vec2(10.0, 10.0), galley.size() + vec2(20.0, 10.0));
        painter.rect_filled(pill, pill.height() / 2.0, SELECTION.gamma_multiply(0.85));
        painter.galley(pill.min + vec2(10.0, 5.0), galley, Color32::WHITE);

        if self.lattice_drag.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hover_pos().is_some_and(|p| handle_at(editor, view, p).is_some()) {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
}
