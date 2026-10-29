//! Rulers along the top and left of the stage, in stage pixels, and the
//! guides dragged out of them (see `ruler_guides.rs` in the core).
//!
//! Drag down from the top ruler for a guide across, or right from the left
//! one for a guide up and down. Drag a guide to move it, or back onto a
//! ruler to take it away. With the view turned, the rulers can't line up
//! with the stage's axes, so they hide; the guides turn with the stage.

use egui::{Align2, Color32, CursorIcon, FontId, Pos2, Rect, Stroke, pos2};
use inkframe_core::{Point, RulerGuide};

use super::{StageView, View};
use crate::editor::Editor;

/// How deep a ruler is, in points.
pub(super) const RULER: f32 = 18.0;
/// How near the pointer must be to grab a guide, in points.
const GRAB: f32 = 4.0;
const GUIDE: Color32 = Color32::from_rgb(0, 196, 210);

/// A guide being dragged: one already there (by its place in the list), or
/// a new one coming out of a ruler.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GuideDrag {
    index: Option<usize>,
    vertical: bool,
}

/// The ruler strips: across the top, and down the left.
fn strips(area: Rect) -> (Rect, Rect) {
    let top = Rect::from_min_max(area.min, pos2(area.max.x, area.min.y + RULER));
    let left = Rect::from_min_max(area.min, pos2(area.min.x + RULER, area.max.y));
    (top, left)
}

/// The rulers show only when they mean something: asked for, and the view
/// neither turned nor mirrored, since they're read along the stage's edges.
fn rulers_shown(editor: &Editor, view: View) -> bool {
    editor.show_rulers && view.rotation.abs() < 1e-4 && !view.flipped
}

/// A guide's line on screen, from one side of the area to the other.
fn guide_line(guide: RulerGuide, view: View) -> (Pos2, Pos2) {
    let far = 1.0e5;
    let (a, b) = if guide.vertical {
        (Point::new(guide.at, -far), Point::new(guide.at, far))
    } else {
        (Point::new(-far, guide.at), Point::new(far, guide.at))
    };
    (view.to_screen(a), view.to_screen(b))
}

impl StageView {
    /// Starts, continues or ends dragging a guide. Returns whether the
    /// pointer is busy with one, so the tools leave it alone. `view` is the
    /// stage's, not a symbol's: guides are in stage pixels.
    pub(super) fn handle_rulers(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        let area = response.rect;
        let (top, left) = strips(area);
        let rulers = rulers_shown(editor, view);
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(press) = ui.input(|i| i.pointer.press_origin())
        {
            self.guide_drag = if rulers && top.contains(press) {
                Some(GuideDrag { index: None, vertical: false })
            } else if rulers && left.contains(press) {
                Some(GuideDrag { index: None, vertical: true })
            } else if editor.show_guides && !editor.lock_guides {
                guide_at(editor, view, press)
                    .map(|i| GuideDrag { index: Some(i), vertical: editor.doc.ruler_guides[i].vertical })
            } else {
                None
            };
        }
        let Some(drag) = self.guide_drag else {
            // A press on a ruler that isn't a drag does nothing else either.
            return rulers
                && response.hovered()
                && response.hover_pos().is_some_and(|p| top.contains(p) || left.contains(p));
        };

        if let Some(pointer) = response.interact_pointer_pos() {
            let at = view.to_local(pointer);
            let value = if drag.vertical { at.x } else { at.y };
            // Back on a ruler (or off the stage area), it goes.
            let gone =
                !area.contains(pointer) || (rulers && (top.contains(pointer) || left.contains(pointer)));
            let guides = &mut editor.doc.ruler_guides;
            match drag.index {
                Some(i) if gone => {
                    if i < guides.len() {
                        guides.remove(i);
                    }
                    self.guide_drag = Some(GuideDrag { index: None, ..drag });
                }
                Some(i) => {
                    if let Some(guide) = guides.get_mut(i) {
                        guide.at = value.round();
                    }
                }
                None if !gone => {
                    guides.push(RulerGuide { vertical: drag.vertical, at: value.round() });
                    self.guide_drag = Some(GuideDrag { index: Some(guides.len() - 1), ..drag });
                }
                None => {}
            }
            ui.ctx().set_cursor_icon(if drag.vertical {
                CursorIcon::ResizeHorizontal
            } else {
                CursorIcon::ResizeVertical
            });
        }
        if response.drag_stopped() {
            self.guide_drag = None;
        }
        true
    }

    /// The guides, then the rulers over the edges of the area.
    pub(super) fn draw_rulers(&self, ui: &egui::Ui, painter: &egui::Painter, editor: &Editor, view: View) {
        let area = painter.clip_rect();
        if editor.show_guides {
            for (i, guide) in editor.doc.ruler_guides.iter().enumerate() {
                let dragged = self.guide_drag.is_some_and(|d| d.index == Some(i));
                let (a, b) = guide_line(*guide, view);
                painter.line_segment([a, b], Stroke::new(if dragged { 1.5 } else { 1.0 }, GUIDE));
                if dragged && let Some(pointer) = ui.ctx().pointer_latest_pos() {
                    let text = format!("{}", guide.at as i64);
                    painter.text(
                        pointer + egui::vec2(12.0, -12.0),
                        Align2::LEFT_BOTTOM,
                        text,
                        FontId::proportional(11.0),
                        GUIDE,
                    );
                }
            }
        }
        if !rulers_shown(editor, view) {
            return;
        }
        let (top, left) = strips(area);
        let visuals = ui.visuals();
        let fill = visuals.panel_fill;
        let line = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
        let text = visuals.weak_text_color();
        painter.rect_filled(top, 0.0, fill);
        painter.rect_filled(left, 0.0, fill);
        painter.hline(top.x_range(), top.bottom(), line);
        painter.vline(left.right(), left.y_range(), line);

        // Numbered ticks at a round step that leaves room for the numbers,
        // with smaller ones between.
        let zoom = view.stage_zoom;
        let step = [1.0f32, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0]
            .into_iter()
            .find(|s| s * zoom >= 60.0)
            .unwrap_or(10_000.0);
        let minor = step / 5.0;
        let origin = view.to_screen(Point::ZERO);
        let font = FontId::proportional(9.0);
        // Across the top.
        let first = ((top.left() + RULER - origin.x) / zoom / minor).floor() as i64;
        let last = ((top.right() - origin.x) / zoom / minor).ceil() as i64;
        for n in first..=last {
            let value = n as f32 * minor;
            let x = origin.x + value * zoom;
            if x < top.left() + RULER {
                continue;
            }
            let major = (value / step).round() * step == value;
            let length = if major { RULER * 0.6 } else { RULER * 0.25 };
            painter.vline(x, (top.bottom() - length)..=top.bottom(), line);
            if major {
                painter.text(
                    pos2(x + 2.0, top.top() + 1.0),
                    Align2::LEFT_TOP,
                    format!("{}", value as i64),
                    font.clone(),
                    text,
                );
            }
        }
        // Down the left.
        let first = ((left.top() + RULER - origin.y) / zoom / minor).floor() as i64;
        let last = ((left.bottom() - origin.y) / zoom / minor).ceil() as i64;
        for n in first..=last {
            let value = n as f32 * minor;
            let y = origin.y + value * zoom;
            if y < left.top() + RULER {
                continue;
            }
            let major = (value / step).round() * step == value;
            let length = if major { RULER * 0.6 } else { RULER * 0.25 };
            painter.hline((left.right() - length)..=left.right(), y, line);
            if major {
                painter.text(
                    pos2(left.left() + 1.0, y + 2.0),
                    Align2::LEFT_TOP,
                    format!("{}", value as i64),
                    font.clone(),
                    text,
                );
            }
        }
        // Where the pointer is, marked on both.
        if let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|p| area.contains(*p)) {
            let accent = Stroke::new(1.0, visuals.selection.stroke.color);
            painter.vline(pointer.x, top.y_range(), accent);
            painter.hline(left.x_range(), pointer.y, accent);
        }
        painter.rect_filled(Rect::from_min_size(area.min, egui::Vec2::splat(RULER)), 0.0, fill);
    }

    /// The cursor over a guide that can be moved.
    pub(super) fn guide_cursor(&self, ui: &egui::Ui, response: &egui::Response, editor: &Editor, view: View) {
        if self.guide_drag.is_some() || !editor.show_guides || editor.lock_guides {
            return;
        }
        if let Some(i) = response.hover_pos().and_then(|p| guide_at(editor, view, p)) {
            let vertical = editor.doc.ruler_guides[i].vertical;
            ui.ctx().set_cursor_icon(if vertical {
                CursorIcon::ResizeHorizontal
            } else {
                CursorIcon::ResizeVertical
            });
        }
    }
}

/// The guide under `pointer`, if one's within grabbing distance.
fn guide_at(editor: &Editor, view: View, pointer: Pos2) -> Option<usize> {
    editor
        .doc
        .ruler_guides
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let (a, b) = guide_line(*g, view);
            (
                i,
                Point::new(pointer.x, pointer.y)
                    .distance_to_segment(Point::new(a.x, a.y), Point::new(b.x, b.y)),
            )
        })
        .filter(|(_, d)| *d <= GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}
