//! The Pen tool: placing anchors and pulling out their handles, and the
//! preview of the path under way.

use egui::{Color32, Pos2, Rect, StrokeKind, Vec2};
use inkframe_core::{Anchor, Path, Shape, Style, TimelineId, path_from_anchors};

use super::anchors::{AnchorRef, OutlineHit};
use super::{ANCHOR_HIT, MIN_HANDLE_DRAG, SELECTION, StageView, View, add_path_layer, insert_point};
use crate::editor::{Editor, Tool};
use crate::i18n::tr;
use crate::pen::{Continued, DragKeys, PenPath, Released};
use crate::subselect::AnchorPart;

impl StageView {
    /// Turns the pen path in progress into a new layer. Paths with fewer
    /// than two anchors are discarded.
    pub fn finish_path(&mut self, editor: &mut Editor, closed: bool) {
        let Some(pen) = self.pen.take() else { return };
        // Carrying on from a shape: the shape grows, in its layer's coordinates.
        if let Some(continued) = pen.continues {
            let local: Vec<Anchor> = pen
                .anchors()
                .iter()
                .map(|a| Anchor {
                    point: continued.to_local.apply(a.point),
                    handle_in: continued.to_local.apply(a.handle_in),
                    handle_out: continued.to_local.apply(a.handle_out),
                })
                .collect();
            if let (Some(element), Some(path)) =
                (editor.element_mut(continued.layer, continued.element), path_from_anchors(&local, closed))
            {
                element.shape = Shape::Path(path);
            }
            return;
        }
        let Some(path) = pen.finish(closed) else {
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

    /// What a press at `pointer` would do with the Pen or one of its
    /// slot-mates, the keys in `modifiers` held. The press, the cursor and
    /// the badge beside it all ask this, so they always agree.
    pub(super) fn pen_action(
        &self,
        editor: &Editor,
        view: View,
        pointer: Pos2,
        modifiers: egui::Modifiers,
    ) -> PenAction {
        // A path under way: its own points, or the next one.
        if let Some(pen) = &self.pen {
            let (p, tolerance) = (view.to_local(pointer), ANCHOR_HIT / view.zoom);
            return if modifiers.command
                && let Some((index, part)) = pen.part_at(p, tolerance)
            {
                PenAction::Adjust(index, part)
            } else if modifiers.command {
                PenAction::End
            } else if pen.can_close_at(p, tolerance) {
                PenAction::Close
            } else if pen.is_on_last(p, tolerance) {
                PenAction::Reach
            } else {
                PenAction::Place
            };
        }
        let tool = editor.tool;
        // Illustrator's held keys: ⌘ is Direct Selection while held, and Alt
        // is Convert Anchor Point.
        if matches!(tool, Tool::Pen | Tool::Curvature) && modifiers.command {
            return PenAction::Subselect;
        }
        if tool == Tool::ConvertAnchor || (tool == Tool::Pen && modifiers.alt) {
            return PenAction::Convert;
        }
        let point = self
            .anchor_targets(editor, view)
            .into_iter()
            .find(|(_, part, at)| *part == AnchorPart::Point && at.distance(pointer) <= ANCHOR_HIT)
            .map(|(target, ..)| target);
        match (tool, point) {
            // The Curvature tool drags a shape's points, the curve following.
            (Tool::Curvature, Some(_)) => PenAction::Subselect,
            (Tool::Pen, Some(target)) if open_end(editor, target).is_some() => PenAction::Continue(target),
            (Tool::Pen | Tool::DeleteAnchor, Some(target)) => PenAction::Delete(target),
            _ => match (tool, self.outline_hit(editor, view, pointer)) {
                (Tool::Pen | Tool::Curvature | Tool::AddAnchor, Some(hit)) => PenAction::Add(hit),
                (Tool::Pen | Tool::Curvature, None) => PenAction::Place,
                _ => PenAction::Nothing,
            },
        }
    }

    /// Input for the Pen and its slot-mates: presses that place points,
    /// add, delete and carry on from them, and the drag that follows a
    /// press placing one. (Dragging points and handles, with Alt or ⌘ held
    /// or the Convert Anchor Point tool, is a gesture: see `begin_gesture`.)
    pub(super) fn handle_pen(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) {
        let (pressed, button_down, space_held, modifiers) = ui.input(|i| {
            (i.pointer.primary_pressed(), i.pointer.primary_down(), i.key_down(egui::Key::Space), i.modifiers)
        });
        let pointer = response.interact_pointer_pos().map(|p| view.to_local(p));

        // Space before the press pans, as everywhere; once a point is being
        // placed, Space moves it instead (below).
        if pressed
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(press) = response.interact_pointer_pos()
        {
            let p = view.to_local(press);
            match self.pen_action(editor, view, press, modifiers) {
                PenAction::Subselect | PenAction::Convert | PenAction::Nothing => {}
                PenAction::End => return self.finish_path(editor, false),
                PenAction::Close => self.pen.as_mut().expect("drawing").begin_closing(),
                PenAction::Reach => self.pen.as_mut().expect("drawing").reach_from_last(),
                PenAction::Adjust(index, part) => {
                    self.pen.as_mut().expect("drawing").begin_adjusting(index, part, p);
                }
                // The path keeps the way it was started, whichever of the
                // two tools places the next point.
                PenAction::Place
                    if self.pen.as_ref().map_or(editor.tool == Tool::Curvature, PenPath::is_curving) =>
                {
                    // Alt-click: a corner, the curve meeting it sharply.
                    match &mut self.pen {
                        Some(pen) => pen.add_curve_point(p, modifiers.alt),
                        None => {
                            self.clear_picked();
                            self.pen = Some(PenPath::curving(p, modifiers.alt));
                        }
                    }
                }
                PenAction::Place => match &mut self.pen {
                    // With a perspective guide, Shift snaps to its directions
                    // rather than to 45° steps.
                    Some(pen) => match editor
                        .doc
                        .perspective
                        .as_ref()
                        .filter(|_| modifiers.shift && editor.timeline() == TimelineId::Main)
                    {
                        Some(guide) => pen.add_anchor(guide.snap(pen.last_point(), p), false),
                        None => pen.add_anchor(p, modifiers.shift),
                    },
                    None => {
                        self.clear_picked();
                        self.pen = Some(PenPath::new(p));
                    }
                },
                PenAction::Add(hit) => insert_point(editor, hit),
                PenAction::Delete(target) => {
                    self.pick_only(target);
                    self.delete_selected_anchor(editor);
                }
                PenAction::Continue(target) => self.continue_path(editor, target),
            }
        }

        if let Some(pen) = &mut self.pen
            && pen.is_dragging()
        {
            if let Some(p) = pointer {
                let keys = DragKeys { broken: modifiers.alt, constrain: modifiers.shift, moving: space_held };
                pen.drag_handle(p, MIN_HANDLE_DRAG / view.zoom, keys);
            }
            if !button_down {
                match pen.release() {
                    Released::Nothing => {}
                    Released::EndOpen => self.finish_path(editor, false),
                    Released::Close => self.finish_path(editor, true),
                }
            }
        }
    }

    /// Takes up drawing from the open end of a shape at `target`.
    fn continue_path(&mut self, editor: &Editor, target: AnchorRef) {
        let Some(from_start) = open_end(editor, target) else { return };
        let Some(layer) = editor.doc.layer(target.layer) else { return };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let Some(to_local) = to_stage.inverse() else { return };
        let Some(element) = layer.elements_at(editor.frame).get(target.element) else { return };
        // The pen works on the stage; the shape's points are the layer's.
        let anchors = element
            .shape
            .to_anchors()
            .0
            .into_iter()
            .map(|a| Anchor {
                point: to_stage.apply(a.point),
                handle_in: to_stage.apply(a.handle_in),
                handle_out: to_stage.apply(a.handle_out),
            })
            .collect();
        let continued = Continued { layer: target.layer, element: target.element, to_local, own: 0 };
        self.clear_picked();
        self.pen = Some(PenPath::continuing(anchors, from_start, continued));
    }

    /// Draws what a press would do beside the pointer, as Adobe's Pen does
    /// with its cursor: `+` adds a point, `−` deletes one, `/` carries on
    /// from an open end, `o` closes the path, `^` turns a point between a
    /// corner and a curve, and `*` starts a new path.
    pub(super) fn draw_pen_badge(&self, painter: &egui::Painter, editor: &Editor, view: View, hover: Pos2) {
        let modifiers = painter.ctx().input(|i| i.modifiers);
        let badge = match self.pen_action(editor, view, hover, modifiers) {
            PenAction::Add(_) => "+",
            PenAction::Delete(_) => "−",
            PenAction::Continue(_) => "/",
            PenAction::Close => "o",
            PenAction::Convert => "^",
            PenAction::Place if self.pen.is_none() => "*",
            _ => return,
        };
        let at = hover + Vec2::new(9.0, 9.0);
        let font = egui::FontId::proportional(12.0);
        // Dark under light, so it reads on any drawing.
        painter.text(at + Vec2::splat(1.0), egui::Align2::LEFT_TOP, badge, font.clone(), Color32::BLACK);
        painter.text(at, egui::Align2::LEFT_TOP, badge, font, Color32::WHITE);
    }

    /// Draws the pen path in progress: the curve so far, a rubber-band line
    /// to the pointer, the anchors, and the handles of the newest anchor.
    pub(super) fn draw_pen_preview(&self, painter: &egui::Painter, view: View, hover: Option<Pos2>) {
        let Some(pen) = &self.pen else { return };
        let to_screen =
            |path: &Path| path.flatten().into_iter().map(|p| view.to_screen(p)).collect::<Vec<_>>();

        let hover_stage = hover.map(|p| view.to_local(p));
        // A Curvature path's preview is the whole path, as the next point
        // would bend it, so the path as it stands isn't drawn under it.
        let previewing = pen.is_curving() && hover_stage.is_some() && !pen.is_dragging();
        if !previewing && let Some(path) = pen.finish(false) {
            painter.line(to_screen(&path), (1.5, SELECTION));
        }
        let closing = hover_stage.is_some_and(|p| pen.can_close_at(p, ANCHOR_HIT / view.zoom));
        if !pen.is_dragging()
            && let Some(preview) = hover_stage.and_then(|p| pen.preview_to(p))
        {
            painter.line(to_screen(&preview), (1.0, SELECTION.gamma_multiply(0.6)));
        }

        // A Curvature path's handles look after themselves, so aren't shown.
        let newest = &pen.anchors()[pen.shown()];
        for handle in [newest.handle_in, newest.handle_out] {
            if handle != newest.point && !pen.is_curving() {
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

/// What a press with the Pen or one of its slot-mates does (see
/// [`StageView::pen_action`]).
#[derive(Debug, Clone, Copy)]
pub(super) enum PenAction {
    /// ⌘ held with the Pen: the Subselection tool's, until it's let go.
    Subselect,
    /// Alt held with the Pen, or the Convert Anchor Point tool.
    Convert,
    /// On an open shape's end: draw on from it.
    Continue(AnchorRef),
    /// On a point of the selected shape: delete it.
    Delete(AnchorRef),
    /// On the selected shape's outline: add a point there.
    Add(OutlineHit),
    /// On the first point of the path being drawn: close it.
    Close,
    /// On the newest point of the path being drawn: take back its way out,
    /// or pull a new one.
    Reach,
    /// Anywhere else: start a path, or place its next point.
    Place,
    /// ⌘-press on a point of the path being drawn, or a handle showing:
    /// drag it, and carry on drawing after.
    Adjust(usize, AnchorPart),
    /// ⌘-click while drawing, away from the path: it's done, left open.
    End,
    /// Nothing here for this tool.
    Nothing,
}

/// Whether `target` is an end of an open drawn path, and if so, whether
/// it's the first (`true`) or the last.
fn open_end(editor: &Editor, target: AnchorRef) -> Option<bool> {
    let element = editor.element(target.layer, target.element)?;
    let Shape::Path(path) = &element.shape else { return None };
    if path.closed {
        return None;
    }
    let count = element.shape.to_anchors().0.len();
    match target.index {
        0 => Some(true),
        i if i + 1 == count => Some(false),
        _ => None,
    }
}
