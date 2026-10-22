//! The perspective guide on the stage (see `perspective.rs` in the core):
//! lines radiating from each vanishing point, the horizon, and the points
//! themselves, which can be dragged. Only on the main timeline, as the
//! guide is laid over the stage.

use egui::{Color32, CursorIcon, Pos2, Stroke};
use inkframe_core::{Point, TimelineId};

use super::{StageView, View};
use crate::editor::Editor;

/// How near the pointer must be to grab a vanishing point, on screen.
const GRAB: f32 = 9.0;
const GUIDE: Color32 = Color32::from_rgb(0x9b, 0x6b, 0xff);
/// How many lines radiate from each vanishing point.
const RAYS: usize = 36;

/// The vanishing point under `pointer`, if any.
fn point_at(editor: &Editor, view: View, pointer: Pos2) -> Option<usize> {
    let guide = editor.doc.perspective.as_ref()?;
    guide
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| (i, view.to_screen(*p).distance(pointer)))
        .filter(|(_, d)| *d <= GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

impl StageView {
    /// Drags the guide's vanishing points. Returns whether the pointer is
    /// busy with one. `view` is the stage's: the guide is in stage pixels.
    pub(super) fn handle_perspective(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        if editor.doc.perspective.is_none() || editor.timeline() != TimelineId::Main {
            self.perspective_drag = None;
            return false;
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(press) = ui.input(|i| i.pointer.press_origin())
        {
            self.perspective_drag = point_at(editor, view, press);
        }
        let Some(index) = self.perspective_drag else { return false };
        if let Some(pointer) = response.interact_pointer_pos()
            && let Some(guide) = &mut editor.doc.perspective
        {
            guide.move_point(index, view.to_local(pointer));
        }
        if response.drag_stopped() {
            self.perspective_drag = None;
        }
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        true
    }

    /// The horizon, the rays from each vanishing point, and the points.
    pub(super) fn draw_perspective(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        editor: &Editor,
        view: View,
    ) {
        let Some(guide) = &editor.doc.perspective else { return };
        if editor.timeline() != TimelineId::Main {
            return;
        }
        let area = painter.clip_rect();
        let far = (area.width() + area.height()) * 4.0;
        let faint = Stroke::new(1.0, GUIDE.gamma_multiply(0.35));
        // The horizon, right across.
        let horizon = view.to_screen(Point::new(0.0, guide.horizon()));
        let level = view.to_screen(Point::new(1.0, guide.horizon())) - horizon;
        let level = if level.length() > 1e-4 { level.normalized() } else { egui::vec2(1.0, 0.0) };
        painter.line_segment(
            [horizon - level * far, horizon + level * far],
            Stroke::new(1.5, GUIDE.gamma_multiply(0.7)),
        );
        for vp in &guide.points {
            let at = view.to_screen(*vp);
            for k in 0..RAYS {
                let angle = std::f32::consts::TAU * k as f32 / RAYS as f32;
                let end = at + egui::vec2(angle.cos(), angle.sin()) * far;
                painter.line_segment([at, end], faint);
            }
        }
        for (i, vp) in guide.points.iter().enumerate() {
            let at = view.to_screen(*vp);
            let dragged = self.perspective_drag == Some(i);
            painter.circle(at, 6.0, if dragged { GUIDE } else { Color32::WHITE }, Stroke::new(2.0, GUIDE));
        }
        if self.perspective_drag.is_none()
            && response.hover_pos().is_some_and(|p| point_at(editor, view, p).is_some())
        {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
}
