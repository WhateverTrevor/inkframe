//! Typing into a text element on the stage: the caret, selecting with the
//! pointer and keys, and drawing the caret and selection over the text.

use egui::{Pos2, Rect};
use inkframe_core::{LayerId, Point, Shape, Text};

use super::{HIT_TOLERANCE, SELECTION, StageView, View, layer_to_local};
use crate::editor::{Editor, Tool};
use crate::text_edit::{TextCursor, TypingHistory, caret_position, position_at, selection_boxes, word_at};

impl StageView {
    /// Starts typing into the named text layer with all its text selected.
    /// For the screenshot hook; people start typing by clicking.
    pub fn type_into(&mut self, editor: &mut Editor, name: &str) {
        let layer = editor.layers().iter().find(|l| l.name == name).map(|l| l.id);
        let Some((layer, element)) = layer.and_then(|id| text_element(editor, id).map(|e| (id, e))) else {
            eprintln!("No text layer named {name:?} for INKFRAME_SCREENSHOT_TYPE");
            return;
        };
        let length = text_of(editor, layer, element).map_or(0, |t| t.content.len());
        editor.tool = Tool::Text;
        editor.selection = Some(layer);
        let cursor = TextCursor::select(0, length);
        self.text_editing = Some(TextEditing { layer, element, cursor, selecting: false });
    }

    /// Starts typing into a text layer's `element`, with the cursor where
    /// `pointer` is, or at the end of the text.
    pub(super) fn begin_text_editing(
        &mut self,
        editor: &mut Editor,
        view: View,
        layer: LayerId,
        element: usize,
        pointer: Option<Pos2>,
    ) {
        editor.selection = Some(layer);
        let Some(text) = text_of(editor, layer, element) else { return };
        let layout = inkframe_render::layout_text(text);
        let position = match (pointer, layer_to_local(editor, layer)) {
            (Some(pointer), Some(to_local)) => position_at(&layout, to_local.apply(view.to_local(pointer))),
            _ => text.content.len(),
        };
        self.text_editing =
            Some(TextEditing { layer, element, cursor: TextCursor::at(position), selecting: false });
        // Each time typing starts, its undo starts afresh: what came before
        // is in the project's own undo.
        self.typing = TypingHistory::default();
    }

    /// Stops typing. Text left empty is removed, as in Animate: clicking
    /// with the Text tool and then clicking away shouldn't leave an
    /// invisible layer behind.
    pub(super) fn finish_text_editing(&mut self, ui: &egui::Ui, id: egui::Id, editor: &mut Editor) {
        let Some(editing) = self.text_editing.take() else { return };
        ui.memory_mut(|m| m.surrender_focus(id));
        let empty =
            text_of(editor, editing.layer, editing.element).is_some_and(|t| t.content.trim().is_empty());
        editor.select(Some(editing.layer));
        if empty {
            editor.delete_selected_layer();
        }
    }

    /// Keyboard and mouse input while typing into text. Returns whether the
    /// mouse was used to place the cursor or select, in which case the tools
    /// shouldn't also act on it.
    pub(super) fn handle_text_editing(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        let Some(mut editing) = self.text_editing else { return false };
        // Stop if the text went away (undo, say), another tool was picked,
        // or the keyboard went to another widget, like the properties panel.
        let focus_elsewhere = ui.memory(|m| m.focused().is_some_and(|focused| focused != response.id));
        let to_local = layer_to_local(editor, editing.layer);
        if text_of(editor, editing.layer, editing.element).is_none()
            || !matches!(editor.tool, Tool::Select | Tool::Text)
            || focus_elsewhere
            || to_local.is_none()
        {
            self.finish_text_editing(ui, response.id, editor);
            return false;
        }
        let to_local = to_local.expect("checked above");
        let local = |screen: Pos2| to_local.apply(view.to_local(screen));

        // Keep the keyboard, including the keys egui would otherwise use to
        // move between widgets (Tab, arrows, Escape).
        response.request_focus();
        let filter =
            egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
        ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));

        // Mouse: a press on the text places the cursor (Shift extends the
        // selection) and dragging selects. A press anywhere else stops
        // editing, and the tools handle it as usual.
        let (pressed, down, shift) =
            ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers.shift));
        let layout = text_of(editor, editing.layer, editing.element).map(inkframe_render::layout_text);
        let layout = layout.expect("checked above");
        let mut pointer_used = false;
        if pressed && let Some(pointer) = response.hover_pos() {
            let slack = HIT_TOLERANCE / view.zoom;
            if layout.bounds.expand(slack.max(layout.ascent * 0.25)).contains(local(pointer)) {
                editing.cursor.move_to(position_at(&layout, local(pointer)), shift);
                editing.selecting = true;
                pointer_used = true;
            } else {
                self.finish_text_editing(ui, response.id, editor);
                return false;
            }
        }
        if editing.selecting {
            pointer_used = true;
            if let Some(pointer) = response.hover_pos().or(response.interact_pointer_pos()) {
                editing.cursor.cursor = position_at(&layout, local(pointer));
            }
            editing.selecting = down;
        }
        if response.double_clicked() {
            let text = text_of(editor, editing.layer, editing.element).expect("checked above");
            let word = word_at(&text.content, editing.cursor.cursor);
            editing.cursor = TextCursor::select(word.start, word.end);
            pointer_used = true;
        }

        // Keyboard. The layout is redone for each event, since arrow keys
        // need to know where the text is after the previous keystroke.
        if response.has_focus() {
            let events = ui.input(|i| i.events.clone());
            for event in &events {
                let Some(text) = text_of_mut(editor, editing.layer, editing.element) else { break };
                // ⌘Z and ⌘⇧Z step through what's been typed (see
                // `TypingHistory`) rather than the project's undo.
                if let Some(redo) = TypingHistory::undo_key(event) {
                    if redo {
                        self.typing.redo(&mut text.content, &mut editing.cursor);
                    } else {
                        self.typing.undo(&mut text.content, &mut editing.cursor);
                    }
                    continue;
                }
                let layout = inkframe_render::layout_text(text);
                let before = (text.content.clone(), editing.cursor);
                let outcome = editing.cursor.handle(&mut text.content, &layout, event);
                let changed = text.content != before.0;
                self.typing.record(before, event, changed);
                if let Some(copied) = outcome.copy {
                    ui.ctx().copy_text(copied);
                }
                if outcome.finish {
                    self.text_editing = Some(editing);
                    self.finish_text_editing(ui, response.id, editor);
                    return pointer_used;
                }
            }
        }
        // Tell the system where the cursor is. That's what lets input
        // methods work here (egui only enables them for a widget that asks),
        // and their candidate window then opens beside the cursor.
        if let Some(text) = text_of(editor, editing.layer, editing.element)
            && let Some(layer) = editor.doc.layer(editing.layer)
        {
            let layout = inkframe_render::layout_text(text);
            let to_stage = editor.doc.layer_matrix(layer, editor.frame);
            let (x, baseline) = caret_position(&layout, editing.cursor.cursor);
            let top = view.to_screen(to_stage.apply(Point::new(x, baseline - layout.ascent)));
            let bottom = view.to_screen(to_stage.apply(Point::new(x, baseline + layout.descent)));
            let caret = Rect::from_two_pos(top, bottom).expand(1.0);
            ui.ctx().output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: caret,
                    cursor_rect: caret,
                    should_interrupt_composition: false,
                });
            });
        }
        self.text_editing = Some(editing);
        pointer_used
    }

    /// The cursor and selection of text being typed into.
    pub(super) fn draw_text_editing(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        editor: &Editor,
        view: View,
    ) {
        let Some(editing) = &self.text_editing else { return };
        let (Some(text), Some(layer)) =
            (text_of(editor, editing.layer, editing.element), editor.doc.layer(editing.layer))
        else {
            return;
        };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(to_stage.apply(p));
        let layout = inkframe_render::layout_text(text);

        // Each box is drawn as four transformed corners, so the highlight
        // follows the text when it's rotated or scaled.
        for selected in selection_boxes(&layout, editing.cursor.selection()) {
            let corners = selected.corners().map(screen).to_vec();
            painter.add(egui::Shape::convex_polygon(
                corners,
                SELECTION.gamma_multiply(0.35),
                egui::Stroke::NONE,
            ));
        }
        // Text being composed with an input method is underlined until
        // it's committed, as in every text field.
        if let Some((start, end)) = editing.cursor.composing {
            for composing in selection_boxes(&layout, start..end) {
                let y = composing.max.y - layout.descent * 0.5;
                let line = [screen(Point::new(composing.min.x, y)), screen(Point::new(composing.max.x, y))];
                painter.line_segment(line, (1.5, SELECTION));
            }
        }
        // The cursor blinks, as text cursors do, but stays solid while
        // selecting so it's easy to follow.
        let time = ui.input(|i| i.time);
        if editing.selecting || (time * 2.0) as i64 % 2 == 0 {
            let (x, baseline) = caret_position(&layout, editing.cursor.cursor);
            let top = screen(Point::new(x, baseline - layout.ascent));
            let bottom = screen(Point::new(x, baseline + layout.descent));
            painter.line_segment([top, bottom], (1.5, SELECTION));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// Text being typed into on the stage.
#[derive(Clone, Copy)]
pub(super) struct TextEditing {
    pub(super) layer: LayerId,
    /// Which of the layer's elements is the text.
    pub(super) element: usize,
    cursor: TextCursor,
    /// While the mouse button that placed the cursor is held, dragging selects.
    selecting: bool,
}

/// The index of the first text element on a layer.
pub(super) fn text_element(editor: &Editor, layer: LayerId) -> Option<usize> {
    editor.doc.layer(layer)?.elements_at(editor.frame).iter().position(|e| matches!(e.shape, Shape::Text(_)))
}

pub(super) fn text_of(editor: &Editor, layer: LayerId, element: usize) -> Option<&Text> {
    match &editor.element(layer, element)?.shape {
        Shape::Text(text) => Some(text),
        _ => None,
    }
}

pub(super) fn text_of_mut(editor: &mut Editor, layer: LayerId, element: usize) -> Option<&mut Text> {
    match &mut editor.element_mut(layer, element)?.shape {
        Shape::Text(text) => Some(text),
        _ => None,
    }
}
