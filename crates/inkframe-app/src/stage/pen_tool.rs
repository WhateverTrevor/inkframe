//! The Pen tool: placing anchors and pulling out their handles, and the
//! preview of the path under way.

use egui::{Color32, Pos2, Rect, StrokeKind, Vec2};
use inkframe_core::{Path, Style, TimelineId};

use super::{ANCHOR_HIT, MIN_HANDLE_DRAG, SELECTION, StageView, View, add_path_layer, insert_point};
use crate::editor::Editor;
use crate::i18n::tr;
use crate::pen::PenPath;

impl StageView {
    /// Turns the pen path in progress into a new layer. Paths with fewer
    /// than two anchors are discarded.
    pub fn finish_path(&mut self, editor: &mut Editor, closed: bool) {
        let Some(path) = self.pen.take().and_then(|pen| pen.finish(closed)) else {
            return;
        };
        // A closed shape gets the full fill-and-stroke style. An open line
        // can't be filled, so it gets just a stroke, falling back to the
        // pencil's stroke if the shape style has none.
        let style = if closed {
            editor.draw_style.clone()
        } else {
            Style {
                fill: None,
                stroke: editor.draw_style.stroke.or(Some(editor.pencil_stroke)),
                gradient: None,
            }
        };
        add_path_layer(editor, tr("Path"), path, style);
    }

    /// Backspace while drawing: take back the most recent anchor.
    pub fn remove_last_path_point(&mut self) {
        if let Some(pen) = &mut self.pen
            && !pen.remove_last()
        {
            self.pen = None;
        }
    }

    /// Pen tool input. Each press places an anchor; dragging before release
    /// pulls out its curve handles. Pressing on the first anchor closes the
    /// path, and pressing on the last one again ends it open.
    pub(super) fn handle_pen(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) {
        let (pressed, button_down, space_held, shift) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.key_down(egui::Key::Space),
                i.modifiers.shift,
            )
        });
        let pointer = response.interact_pointer_pos().map(|p| view.to_local(p));

        // Between paths, a press on one of the selected shape's points picks
        // it, to drag it or its handles (see `begin_anchor_edit`), rather
        // than starting a new path there.
        if pressed
            && self.pen.is_none()
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(press) = response.interact_pointer_pos()
            && let Some((target, _)) = self.anchor_hit(editor, view, press)
        {
            self.anchor_selection = Some(target);
            return;
        }

        // Before starting a new path, a click on the selected shape's outline
        // adds a point to it instead, as in Animate and Illustrator.
        if pressed
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(hit) = response.interact_pointer_pos().and_then(|p| self.outline_hit(editor, view, p))
        {
            insert_point(editor, hit);
            return;
        }

        if pressed
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(p) = pointer
        {
            let tolerance = ANCHOR_HIT / view.zoom;
            match &mut self.pen {
                Some(pen) if pen.can_close_at(p, tolerance) => return self.finish_path(editor, true),
                Some(pen) if pen.is_on_last(p, tolerance) => return self.finish_path(editor, false),
                // With a perspective guide, Shift snaps to its directions
                // rather than to 45° steps.
                Some(pen) => match editor
                    .doc
                    .perspective
                    .as_ref()
                    .filter(|_| shift && editor.timeline() == TimelineId::Main)
                {
                    Some(guide) => pen.add_anchor(guide.snap(pen.last_point(), p), false),
                    None => pen.add_anchor(p, shift),
                },
                None => {
                    self.anchor_selection = None;
                    self.pen = Some(PenPath::new(p));
                }
            }
        }

        if let Some(pen) = &mut self.pen
            && pen.is_dragging()
        {
            if let Some(p) = pointer {
                pen.drag_handle(p, MIN_HANDLE_DRAG / view.zoom);
            }
            if !button_down {
                pen.release();
            }
        }
    }

    /// Draws the pen path in progress: the curve so far, a rubber-band line
    /// to the pointer, the anchors, and the handles of the newest anchor.
    pub(super) fn draw_pen_preview(&self, painter: &egui::Painter, view: View, hover: Option<Pos2>) {
        let Some(pen) = &self.pen else { return };
        let to_screen =
            |path: &Path| path.flatten().into_iter().map(|p| view.to_screen(p)).collect::<Vec<_>>();

        if let Some(path) = pen.finish(false) {
            painter.line(to_screen(&path), (1.5, SELECTION));
        }
        let hover_stage = hover.map(|p| view.to_local(p));
        let closing = hover_stage.is_some_and(|p| pen.can_close_at(p, ANCHOR_HIT / view.zoom));
        if !pen.is_dragging()
            && let Some(preview) = hover_stage.and_then(|p| pen.preview_to(p))
        {
            painter.line(to_screen(&preview), (1.0, SELECTION.gamma_multiply(0.6)));
        }

        let newest = pen.anchors().last().expect("never empty");
        for handle in [newest.handle_in, newest.handle_out] {
            if handle != newest.point {
                let (a, h) = (view.to_screen(newest.point), view.to_screen(handle));
                painter.line_segment([a, h], (1.0, SELECTION));
                painter.circle_filled(h, 3.0, SELECTION);
            }
        }

        for (i, anchor) in pen.anchors().iter().enumerate() {
            // The first anchor fills in when hovering it would close the path.
            let fill = if i == 0 && closing { SELECTION } else { Color32::WHITE };
            let square = Rect::from_center_size(view.to_screen(anchor.point), Vec2::splat(7.0));
            painter.rect_filled(square, 0.0, fill);
            painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }
    }
}
