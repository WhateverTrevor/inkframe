//! The timeline panel: transport controls, plus a grid of layers (rows) by
//! frames (columns) showing keyframes and tweens.
//!
//! Each layer has a summary row showing every frame where any of its
//! properties has a key. Expanding it (the ▸ toggle) adds a row per
//! property, where keys and easing can be edited one property at a time.
//!
//! The grid is painted by hand rather than built from widgets. A long
//! animation has thousands of cells, and painting only the visible ones keeps
//! it fast. It also lets the ruler and the layer names stay pinned while the
//! frames scroll underneath.

use std::collections::HashSet;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use inkframe_core::{AnyChannel, AudioId, Easing, Layer, LayerId, Property, TimelineId};

use crate::audio_engine::AudioEngine;
use crate::editor::Editor;
use crate::value_graph::{self, FrameAxis, ValueGraph};

const NAME_WIDTH: f32 = 200.0;
const RULER_HEIGHT: f32 = 22.0;
const ROW_HEIGHT: f32 = 24.0;
const FRAME_WIDTH: f32 = 12.0;
/// How far property names are indented under their layer.
const PROPERTY_INDENT: f32 = 44.0;
/// Columns drawn past the end of the animation, so the end is visible.
const TRAILING_FRAMES: u32 = 20;
/// How close to the end of a layer's bar a press must land to grab it, in points.
const EDGE_GRAB: f32 = 3.0;

const PLAYHEAD: Color32 = Color32::from_rgb(0xe8, 0x3a, 0x3a);
const CURRENT_KEY: Color32 = Color32::from_rgb(0x2f, 0x9b, 0xff);
/// Animate shows tweens in a violet-blue; we follow suit so they read the same.
const TWEEN: Color32 = Color32::from_rgb(0x8a, 0x7c, 0xd8);
const HOLD: Color32 = Color32::from_gray(150);
const AUDIO: Color32 = Color32::from_rgb(0x4c, 0xc2, 0x8a);

/// One line of the grid: a layer's summary row, or one of its properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    layer: LayerId,
    /// `None` for the layer's summary row.
    property: Option<Property>,
}

enum Drag {
    /// Dragging an audio track's waveform slides where the sound starts.
    Audio { id: AudioId, grab: i64, original_start: u32 },
    /// Dragging along the ruler or empty cells moves the playhead.
    Scrub,
    /// Dragging a key slides it to another frame. On a summary row this
    /// moves every property's key on that frame together.
    Key { row: Row, frame: u32 },
    /// Dragging the left or right end of a layer's bar changes the frames
    /// the layer appears on.
    SpanEdge { layer: LayerId, edge: Edge },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Start,
    End,
}

/// Screen geometry of the grid for one frame of UI.
struct Layout {
    ruler: Rect,
    names: Rect,
    frames: Rect,
    scroll: Vec2,
}

impl Layout {
    fn new(rect: Rect, scroll: Vec2) -> Self {
        let body_top = rect.top() + RULER_HEIGHT;
        let split_x = rect.left() + NAME_WIDTH;
        Self {
            ruler: Rect::from_min_max(pos2(split_x, rect.top()), pos2(rect.right(), body_top)),
            names: Rect::from_min_max(pos2(rect.left(), body_top), pos2(split_x, rect.bottom())),
            frames: Rect::from_min_max(pos2(split_x, body_top), rect.max),
            scroll,
        }
    }

    /// Left edge of a frame's column.
    fn frame_x(&self, frame: u32) -> f32 {
        self.frames.left() + frame as f32 * FRAME_WIDTH - self.scroll.x
    }

    fn frame_center_x(&self, frame: u32) -> f32 {
        self.frame_x(frame) + FRAME_WIDTH / 2.0
    }

    fn frame_at(&self, x: f32) -> i64 {
        ((x - self.frames.left() + self.scroll.x) / FRAME_WIDTH).floor() as i64
    }

    /// The column boundary nearest to `x`: boundary `n` is the left edge of
    /// frame `n`. Used when dragging the ends of a layer's bar, which sit
    /// between frames rather than on them.
    fn boundary_at(&self, x: f32) -> i64 {
        ((x - self.frames.left() + self.scroll.x) / FRAME_WIDTH).round() as i64
    }

    /// Which end of `layer`'s bar is under `x`, if either.
    fn span_edge_at(&self, layer: &Layer, doc_last_frame: u32, x: f32) -> Option<Edge> {
        let start = self.frame_x(layer.start);
        let end = self.frame_x(layer.last_frame(doc_last_frame) + 1);
        if (x - start).abs() <= EDGE_GRAB {
            Some(Edge::Start)
        } else if (x - end).abs() <= EDGE_GRAB {
            Some(Edge::End)
        } else {
            None
        }
    }

    fn row_top(&self, row: usize) -> f32 {
        self.frames.top() + row as f32 * ROW_HEIGHT - self.scroll.y
    }

    fn row_at(&self, y: f32) -> Option<usize> {
        let offset = y - self.frames.top() + self.scroll.y;
        (offset >= 0.0).then(|| (offset / ROW_HEIGHT) as usize)
    }

    fn row_rect(&self, row: usize, columns: Rect) -> Rect {
        let top = self.row_top(row);
        Rect::from_x_y_ranges(columns.x_range(), top..=top + ROW_HEIGHT)
    }

    fn expander_rect(&self, row: usize) -> Rect {
        let r = self.row_rect(row, self.names);
        Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), Vec2::splat(ROW_HEIGHT * 0.8))
    }

    fn eye_rect(&self, row: usize) -> Rect {
        let r = self.row_rect(row, self.names);
        Rect::from_center_size(pos2(r.left() + 32.0, r.center().y), Vec2::splat(ROW_HEIGHT))
    }

    fn lock_rect(&self, row: usize) -> Rect {
        self.eye_rect(row).translate(vec2(ROW_HEIGHT, 0.0))
    }
}

#[derive(Default)]
pub struct TimelineView {
    scroll: Vec2,
    drag: Option<Drag>,
    /// Layers whose property rows are showing.
    expanded: HashSet<LayerId>,
    /// The row that was right-clicked, which the context menu acts on.
    context_row: Option<Row>,
    /// Or the audio row that was right-clicked.
    context_audio: Option<AudioId>,
    /// Showing the value graph instead of the layer grid.
    graph_mode: bool,
    graph: ValueGraph,
    /// Dragging along the ruler in graph mode, which scrubs.
    scrubbing: bool,
    /// Where the grid was drawn last frame. Tests use it to find cells.
    #[cfg(test)]
    grid_rect: Option<Rect>,
}

impl TimelineView {
    pub fn is_busy(&self) -> bool {
        self.drag.is_some() || self.graph.is_busy() || self.scrubbing
    }

    /// Shows the value graph instead of the layer rows, or not.
    pub fn set_graph_mode(&mut self, on: bool) {
        self.graph_mode = on;
    }

    /// Shows or hides a layer's property rows.
    pub fn set_expanded(&mut self, layer: LayerId, expanded: bool) {
        if expanded {
            self.expanded.insert(layer);
        } else {
            self.expanded.remove(&layer);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) {
        transport_bar(ui, editor, &mut self.graph_mode);
        ui.add_space(2.0);
        if self.graph_mode {
            self.graph_grid(ui, editor);
        } else {
            self.grid(ui, editor, audio);
        }
    }

    /// The value graph in place of the layer rows: the same ruler and
    /// frame spacing, with the selected layer's properties to choose from
    /// on the left.
    fn graph_grid(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        self.update_scroll(ui, &response, editor, rect, 0);
        let layout = Layout::new(rect, self.scroll);

        // Pressing on the ruler scrubs, as in the timeline.
        let press = ui.input(|i| i.pointer.press_origin());
        if response.drag_started() || response.clicked() {
            self.scrubbing = press.is_some_and(|p| layout.ruler.contains(p));
        }
        if self.scrubbing
            && let Some(p) = response.interact_pointer_pos()
        {
            editor.set_frame(layout.frame_at(p.x).clamp(0, editor.last_frame() as i64) as u32);
        }
        if !response.is_pointer_button_down_on() {
            self.scrubbing = false;
        }
        // Clicking a property name on the left shows its graph.
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
            && layout.names.contains(p)
            && let Some(row) = layout.row_at(p.y)
            && let Some(&property) = Property::ALL.get(row)
        {
            editor.focused_property = property;
        }

        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        paint_ruler(&painter.with_clip_rect(layout.ruler), ui, editor, &layout);
        let (left, scroll) = (layout.frames.left(), self.scroll.x);
        let x_of = move |frame: f32| left + frame * FRAME_WIDTH - scroll + FRAME_WIDTH / 2.0;
        let frame_of = move |x: f32| (x - left + scroll - FRAME_WIDTH / 2.0) / FRAME_WIDTH;
        // The graph only takes presses inside its own area, so scrubbing on
        // the ruler above never grabs a key.
        let axis = FrameAxis { x_of: &x_of, frame_of: &frame_of };
        self.graph.show(ui, &response, layout.frames, editor, axis);

        // The property list, with the selected one highlighted, and a key to
        // the curve colors under it.
        let names = painter.with_clip_rect(layout.names);
        names.rect_filled(layout.names, 0.0, ui.visuals().panel_fill);
        let font = FontId::proportional(12.0);
        for (row, property) in Property::ALL.into_iter().enumerate() {
            let cell = layout.row_rect(row, layout.names);
            if property == editor.focused_property {
                names.rect_filled(cell, 0.0, ui.visuals().selection.bg_fill);
            }
            names.text(
                pos2(cell.left() + 10.0, cell.center().y),
                Align2::LEFT_CENTER,
                property.name(),
                font.clone(),
                ui.visuals().text_color(),
            );
        }
        let legend_top = layout.row_rect(Property::ALL.len(), layout.names).top() + 6.0;
        for (i, name) in value_graph::curve_names(editor.focused_property).iter().enumerate() {
            let y = legend_top + i as f32 * 16.0 + 8.0;
            names.circle_filled(pos2(layout.names.left() + 14.0, y), 4.0, value_graph::curve_color(i));
            names.text(
                pos2(layout.names.left() + 24.0, y),
                Align2::LEFT_CENTER,
                *name,
                FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
        }

        let corner = Rect::from_min_max(rect.min, pos2(layout.names.right(), layout.ruler.bottom()));
        painter.rect_filled(corner, 0.0, ui.visuals().panel_fill);
        let title = editor.selected_layer().map_or_else(|| "Graph".to_owned(), |l| l.name.clone());
        painter.text(
            pos2(corner.left() + 8.0, corner.center().y),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(11.0),
            ui.visuals().weak_text_color(),
        );
        let line = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
        painter.vline(layout.frames.left(), rect.y_range(), line);
        painter.hline(rect.x_range(), layout.frames.top(), line);
    }

    /// The rows to show, top to bottom. Layers run from the front-most down,
    /// the reverse of the document's back-to-front storage order.
    fn rows(&self, editor: &Editor) -> Vec<Row> {
        let mut rows = Vec::new();
        for layer in editor.layers().iter().rev() {
            rows.push(Row { layer: layer.id, property: None });
            if self.expanded.contains(&layer.id) {
                rows.extend(Property::ALL.map(|p| Row { layer: layer.id, property: Some(p) }));
            }
        }
        rows
    }

    fn grid(&mut self, ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        #[cfg(test)]
        {
            self.grid_rect = Some(rect);
        }

        // Forget expanded layers that have since been deleted.
        self.expanded.retain(|id| editor.doc.layer(*id).is_some());
        let rows = self.rows(editor);
        // Audio rows sit below the layers. Sound belongs to the main
        // timeline, so they're hidden while editing inside a symbol.
        let audio_rows: Vec<AudioId> = if editor.timeline() == TimelineId::Main {
            editor.doc.audio.iter().map(|t| t.id).collect()
        } else {
            Vec::new()
        };

        self.update_scroll(ui, &response, editor, rect, rows.len() + audio_rows.len());
        let layout = Layout::new(rect, self.scroll);

        self.handle_input(ui, &response, editor, &layout, &rows, &audio_rows);
        self.update_cursor(ui, &response, editor, &layout, &rows);
        context_menu(&response, editor, self.context_row, self.context_audio);

        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        paint_frames(&painter.with_clip_rect(layout.frames), editor, &layout, &rows);
        paint_ruler(&painter.with_clip_rect(layout.ruler), ui, editor, &layout);
        self.paint_names(&painter.with_clip_rect(layout.names), ui, editor, &layout, &rows);
        paint_audio_rows(
            &painter.with_clip_rect(layout.frames),
            &painter.with_clip_rect(layout.names),
            ui,
            editor,
            audio,
            &layout,
            rows.len(),
            &audio_rows,
        );

        // Header for the names column, in the corner left of the ruler.
        let corner = Rect::from_min_max(rect.min, pos2(layout.names.right(), layout.ruler.bottom()));
        painter.rect_filled(corner, 0.0, ui.visuals().panel_fill);
        painter.text(
            pos2(corner.left() + 8.0, corner.center().y),
            Align2::LEFT_CENTER,
            "Layers",
            FontId::proportional(11.0),
            ui.visuals().weak_text_color(),
        );

        let line = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
        painter.vline(layout.frames.left(), rect.y_range(), line);
        painter.hline(rect.x_range(), layout.frames.top(), line);
    }

    fn update_scroll(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &Editor,
        rect: Rect,
        rows: usize,
    ) {
        let layout = Layout::new(rect, self.scroll);
        if response.hovered() {
            self.scroll -= ui.input(|i| i.smooth_scroll_delta);
        }
        // Keep the playhead in view while playing.
        if editor.playback.playing {
            let x = layout.frame_x(editor.frame);
            if x < layout.frames.left() || x + FRAME_WIDTH > layout.frames.right() {
                self.scroll.x = editor.frame as f32 * FRAME_WIDTH;
            }
        }
        let content =
            vec2((editor.duration() + TRAILING_FRAMES) as f32 * FRAME_WIDTH, rows as f32 * ROW_HEIGHT);
        let max = (content - layout.frames.size()).max(Vec2::ZERO);
        self.scroll = self.scroll.clamp(Vec2::ZERO, max);
    }

    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        layout: &Layout,
        rows: &[Row],
        audio_rows: &[AudioId],
    ) {
        let last_frame = editor.last_frame();
        let clamp_frame = |f: i64| f.clamp(0, last_frame as i64) as u32;
        let row_under = |y: f32| layout.row_at(y).and_then(|row| rows.get(row).copied());
        let audio_under = |y: f32| {
            layout
                .row_at(y)
                .and_then(|row| row.checked_sub(rows.len()))
                .and_then(|i| audio_rows.get(i).copied())
        };

        // Right-click moves the playhead and selection to the clicked cell
        // first, so the context menu acts on what was clicked.
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
            && layout.frames.contains(p)
        {
            editor.set_frame(clamp_frame(layout.frame_at(p.x)));
            self.context_row = row_under(p.y);
            self.context_audio = audio_under(p.y);
            if let Some(row) = self.context_row {
                select_row(editor, row);
            }
        }

        let pressed = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed());
        if pressed && let Some(p) = response.interact_pointer_pos() {
            // Shift- or ⌘-clicking a layer's name adds it to the selection.
            let adding = ui.input(|i| i.modifiers.shift || i.modifiers.command);
            self.drag = match audio_under(p.y).filter(|_| !layout.ruler.contains(p)) {
                Some(id) => press_audio(editor, layout, id, p),
                None => self.press(editor, layout, rows, p, adding),
            };
        }

        if let (Some(drag), Some(p)) = (&mut self.drag, response.interact_pointer_pos()) {
            let frame = clamp_frame(layout.frame_at(p.x));
            match drag {
                Drag::Scrub => editor.set_frame(frame),
                Drag::Audio { id, grab, original_start } => {
                    let moved = layout.frame_at(p.x) - *grab;
                    if let Some(track) = editor.doc.audio_track_mut(*id) {
                        track.start = (*original_start as i64 + moved).max(0) as u32;
                    }
                }
                Drag::SpanEdge { layer, edge } => {
                    let boundary = layout.boundary_at(p.x).clamp(0, last_frame as i64 + 1) as u32;
                    if let Some(layer) = editor.doc.layer_mut(*layer) {
                        match edge {
                            Edge::Start => layer.set_start(boundary.min(last_frame)),
                            // The end boundary sits after the last frame shown.
                            // Dragging it all the way out makes the layer
                            // open-ended again.
                            Edge::End => {
                                let end = boundary.saturating_sub(1);
                                layer.set_end((end < last_frame).then_some(end));
                            }
                        }
                    }
                }
                Drag::Key { row, frame: from } => {
                    let moved = editor.doc.layer_mut(row.layer).is_some_and(|l| match row.property {
                        None => l.track.move_key(*from, frame),
                        Some(property) => l.track.channel_mut(property).move_key(*from, frame),
                    });
                    if moved {
                        *from = frame;
                        editor.set_frame(frame);
                    }
                }
            }
        }

        if !ui.input(|i| i.pointer.primary_down()) {
            self.drag = None;
        }
    }

    /// Shows a resize cursor over (or while dragging) the ends of a layer's bar.
    fn update_cursor(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &Editor,
        layout: &Layout,
        rows: &[Row],
    ) {
        let over_edge = response.hover_pos().is_some_and(|p| {
            layout.frames.contains(p)
                && layout
                    .row_at(p.y)
                    .and_then(|i| rows.get(i))
                    .filter(|row| row.property.is_none())
                    .and_then(|row| editor.doc.layer(row.layer))
                    .is_some_and(|layer| {
                        !layer.locked && layout.span_edge_at(layer, editor.last_frame(), p.x).is_some()
                    })
        });
        if over_edge || matches!(self.drag, Some(Drag::SpanEdge { .. })) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }

    /// Decides what a primary-button press starts, based on where it landed.
    fn press(
        &mut self,
        editor: &mut Editor,
        layout: &Layout,
        rows: &[Row],
        p: Pos2,
        adding: bool,
    ) -> Option<Drag> {
        if layout.ruler.contains(p) {
            return Some(Drag::Scrub);
        }

        let index = layout.row_at(p.y)?;
        let row = rows.get(index).copied();

        if layout.names.contains(p) {
            let row = row?;
            if row.property.is_none() {
                let layer = editor.doc.layer_mut(row.layer)?;
                if layout.expander_rect(index).contains(p) {
                    if !self.expanded.remove(&row.layer) {
                        self.expanded.insert(row.layer);
                    }
                    return None;
                } else if layout.eye_rect(index).contains(p) {
                    layer.visible = !layer.visible;
                    return None;
                } else if layout.lock_rect(index).contains(p) {
                    layer.locked = !layer.locked;
                    return None;
                }
            }
            if adding && row.property.is_none() {
                editor.toggle_selected(row.layer);
            } else {
                select_row(editor, row);
            }
            return None;
        }

        if layout.frames.contains(p) {
            let Some(row) = row else {
                editor.select(None);
                return Some(Drag::Scrub);
            };
            select_row(editor, row);
            let doc_last_frame = editor.last_frame();
            let layer = editor.doc.layer(row.layer)?;
            if row.property.is_none()
                && !layer.locked
                && let Some(edge) = layout.span_edge_at(layer, doc_last_frame, p.x)
            {
                return Some(Drag::SpanEdge { layer: row.layer, edge });
            }
            if let Ok(frame) = u32::try_from(layout.frame_at(p.x)) {
                let has_key = match row.property {
                    None => layer.track.has_key(frame),
                    Some(property) => layer.track.channel(property).has_key(frame),
                };
                if !layer.locked && has_key {
                    editor.set_frame(frame);
                    return Some(Drag::Key { row, frame });
                }
            }
            return Some(Drag::Scrub);
        }
        None
    }

    fn paint_names(
        &self,
        painter: &egui::Painter,
        ui: &egui::Ui,
        editor: &Editor,
        layout: &Layout,
        rows: &[Row],
    ) {
        painter.rect_filled(layout.names, 0.0, ui.visuals().panel_fill);
        let text = ui.visuals().text_color();
        let faint = text.gamma_multiply(0.3);
        let font = FontId::proportional(13.0);

        for (index, row) in rows.iter().enumerate() {
            let Some(layer) = editor.doc.layer(row.layer) else { continue };
            let rect = layout.row_rect(index, layout.names);
            let selected = editor.is_selected(row.layer);

            match row.property {
                None => {
                    if selected {
                        painter.rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
                    }
                    let expanded = self.expanded.contains(&row.layer);
                    disclosure_triangle(painter, layout.expander_rect(index).center(), expanded, text);

                    let eye = if layer.visible { text } else { faint };
                    painter.text(
                        layout.eye_rect(index).center(),
                        Align2::CENTER_CENTER,
                        "👁",
                        font.clone(),
                        eye,
                    );
                    let (lock, lock_color) = if layer.locked { ("🔒", text) } else { ("🔓", faint) };
                    painter.text(
                        layout.lock_rect(index).center(),
                        Align2::CENTER_CENTER,
                        lock,
                        font.clone(),
                        lock_color,
                    );

                    let name_pos = pos2(layout.lock_rect(index).right() + 6.0, rect.center().y);
                    let color = if layer.visible { text } else { faint };
                    let name = painter.text(name_pos, Align2::LEFT_CENTER, &layer.name, font.clone(), color);
                    // Guides and masks are labelled, since neither is drawn in
                    // an export, and masked layers say so, since what shows of
                    // them depends on another layer.
                    let tag = if layer.guide {
                        Some("guide")
                    } else if layer.mask {
                        Some("mask")
                    } else if editor.doc.mask_for(layer, editor.frame).is_some() {
                        Some("masked")
                    } else {
                        None
                    };
                    if let Some(tag) = tag {
                        let at = pos2(name.right() + 6.0, rect.center().y);
                        painter.text(at, Align2::LEFT_CENTER, tag, font.clone(), faint);
                    }
                }
                Some(property) => {
                    // The property the inspector's easing graph is showing is
                    // highlighted, tying the two panels together.
                    let focused = selected && editor.focused_property == property;
                    if focused {
                        painter.rect_filled(rect, 0.0, ui.visuals().selection.bg_fill.gamma_multiply(0.5));
                    }
                    let name_pos = pos2(rect.left() + PROPERTY_INDENT, rect.center().y);
                    painter.text(
                        name_pos,
                        Align2::LEFT_CENTER,
                        property.name(),
                        FontId::proportional(12.0),
                        text,
                    );
                }
            }
            painter.hline(rect.x_range(), rect.bottom(), (1.0, Color32::from_black_alpha(60)));
        }

        if rows.is_empty() {
            let hint = "Draw on the stage to create a layer";
            painter.text(
                layout.names.left_top() + vec2(10.0, 8.0),
                Align2::LEFT_TOP,
                hint,
                FontId::proportional(12.0),
                faint,
            );
        }
    }
}

/// A press on an audio row. In the names column, the speaker icon mutes the
/// track; on the waveform, it moves the playhead there and starts dragging
/// the sound to a new start frame.
fn press_audio(editor: &mut Editor, layout: &Layout, id: AudioId, p: Pos2) -> Option<Drag> {
    let row = layout.row_at(p.y)?;
    if layout.names.contains(p) {
        if layout.eye_rect(row).contains(p)
            && let Some(track) = editor.doc.audio_track_mut(id)
        {
            track.muted = !track.muted;
        }
        return None;
    }
    let frame = layout.frame_at(p.x);
    editor.set_frame(frame.clamp(0, editor.last_frame() as i64) as u32);
    let original_start = editor.doc.audio_track(id)?.start;
    Some(Drag::Audio { id, grab: frame, original_start })
}

/// Selects a row's layer, and for a property row, points the inspector's
/// easing graph at that property.
fn select_row(editor: &mut Editor, row: Row) {
    editor.select(Some(row.layer));
    if let Some(property) = row.property {
        editor.focused_property = property;
    }
}

fn transport_bar(ui: &mut egui::Ui, editor: &mut Editor, graph_mode: &mut bool) {
    ui.horizontal(|ui| {
        let last = editor.last_frame();
        if ui.button("⏮").on_hover_text("First frame (Home)").clicked() {
            editor.set_frame(0);
        }
        if ui.button("⏴").on_hover_text("Previous frame (,)").clicked() {
            editor.step(-1);
        }
        let play = if editor.playback.playing { "⏸" } else { "▶" };
        if ui.button(play).on_hover_text("Play / pause (Enter)").clicked() {
            editor.toggle_playback();
        }
        if ui.button("⏵").on_hover_text("Next frame (.)").clicked() {
            editor.step(1);
        }
        if ui.button("⏭").on_hover_text("Last frame (End)").clicked() {
            editor.set_frame(last);
        }
        ui.toggle_value(&mut editor.playback.looping, "🔁").on_hover_text("Loop playback");

        ui.separator();

        // Frames are 1-based in the interface and 0-based in the document.
        let mut shown = editor.frame + 1;
        ui.label("Frame");
        if ui.add(egui::DragValue::new(&mut shown).range(1..=last + 1)).changed() {
            editor.set_frame(shown - 1);
        }
        let seconds = editor.frame as f64 / editor.doc.frame_rate.fps();
        ui.label(format!("of {}   {seconds:.2}s", editor.duration()));

        ui.separator();

        ui.toggle_value(graph_mode, "Graph")
            .on_hover_text("Show the selected layer's animation as a graph of values over time");
        ui.separator();

        ui.toggle_value(&mut editor.onion.enabled, "Onion skin")
            .on_hover_text("Show faint copies of nearby frames");
        if editor.onion.enabled {
            ui.add(egui::DragValue::new(&mut editor.onion.before).range(0..=10).prefix("◀ "))
                .on_hover_text("Frames shown before the current one");
            ui.add(egui::DragValue::new(&mut editor.onion.after).range(0..=10).suffix(" ▶"))
                .on_hover_text("Frames shown after the current one");
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let has_selection = editor.selection.is_some();
            if ui.add_enabled(has_selection, egui::Button::new("🗑")).on_hover_text("Delete layer").clicked()
            {
                editor.delete_selected_layer();
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("⬇"))
                .on_hover_text("Move layer down")
                .clicked()
            {
                editor.move_selected_layer(-1);
            }
            if ui.add_enabled(has_selection, egui::Button::new("⬆")).on_hover_text("Move layer up").clicked()
            {
                editor.move_selected_layer(1);
            }
            if ui.add_enabled(has_selection, egui::Button::new("Duplicate")).clicked() {
                editor.duplicate_selected_layer();
            }
        });
    });
}

fn context_menu(response: &egui::Response, editor: &mut Editor, row: Option<Row>, audio: Option<AudioId>) {
    response.context_menu(|ui| {
        if let Some(id) = audio {
            let Some(track) = editor.doc.audio_track_mut(id) else { return };
            ui.weak(&track.name);
            let mute = if track.muted { "Unmute" } else { "Mute" };
            if ui.button(mute).clicked() {
                track.muted = !track.muted;
                ui.close();
            }
            if ui.button("Remove Audio").clicked() {
                editor.doc.remove_audio(id);
                ui.close();
            }
            return;
        }
        let frame = editor.frame;
        let Some((row, layer)) = row.and_then(|r| Some((r, editor.doc.layer(r.layer)?))) else {
            ui.label("Right-click a layer row to edit its keyframes");
            return;
        };

        // The menu acts on one property for a property row, or on every
        // property for a layer's summary row.
        let scope: Vec<Property> = row.property.map_or(Property::ALL.to_vec(), |p| vec![p]);
        let channel = |p: Property| layer.track.channel(p);
        let can_insert = scope.iter().any(|&p| !channel(p).has_key(frame));
        let can_remove =
            scope.iter().any(|&p| channel(p).has_key(frame) && channel(p).key_frames().len() > 1);
        let easings: Vec<Easing> =
            scope.iter().filter_map(|&p| channel(p).tween_at(frame)).map(|t| t.easing).collect();
        // Show a check mark only when every tween in scope agrees.
        let shared_easing = easings.first().copied().filter(|e| easings.iter().all(|x| x == e));

        if let Some(property) = row.property {
            ui.weak(property.name());
        }
        let insert_label = if row.property.is_some() { "Insert Key" } else { "Insert Keyframe" };
        if ui.add_enabled(can_insert, egui::Button::new(insert_label).shortcut_text("F6")).clicked() {
            for_each_channel(editor, row, |c| {
                c.insert_key(frame);
            });
            ui.close();
        }
        let remove_label = if row.property.is_some() { "Remove Key" } else { "Remove Keyframe" };
        if ui.add_enabled(can_remove, egui::Button::new(remove_label).shortcut_text("⇧F6")).clicked() {
            for_each_channel(editor, row, |c| {
                c.remove_key(frame);
            });
            ui.close();
        }
        ui.add_enabled_ui(!easings.is_empty(), |ui| {
            ui.menu_button("Easing", |ui| {
                for preset in Easing::PRESETS {
                    if ui.selectable_label(shared_easing == Some(preset), preset.label()).clicked() {
                        for_each_channel(editor, row, |c| {
                            if let Some(e) = c.tween_easing_mut(frame) {
                                *e = preset;
                            }
                        });
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        if ui.add(egui::Button::new("Start Layer Here").shortcut_text("⌥[")).clicked() {
            editor.set_layer_start_here();
            ui.close();
        }
        if ui.add(egui::Button::new("End Layer Here").shortcut_text("⌥]")).clicked() {
            editor.set_layer_end_here();
            ui.close();
        }
        ui.separator();
        if ui.button("Duplicate Layer").clicked() {
            editor.duplicate_selected_layer();
            ui.close();
        }
        if ui.button("Delete Layer").clicked() {
            editor.delete_selected_layer();
            ui.close();
        }
    });
}

/// Runs `f` on the channels a row stands for: one property, or all of them.
fn for_each_channel(editor: &mut Editor, row: Row, mut f: impl FnMut(&mut dyn AnyChannel)) {
    let Some(layer) = editor.doc.layer_mut(row.layer) else { return };
    for property in row.property.map_or(Property::ALL.to_vec(), |p| vec![p]) {
        f(layer.track.channel_mut(property));
    }
}

fn paint_frames(painter: &egui::Painter, editor: &Editor, layout: &Layout, rows: &[Row]) {
    let doc = &editor.doc;
    let duration = editor.duration();
    let visible_frames = visible_frame_range(layout, duration + TRAILING_FRAMES);

    // Darken the columns past the end of the animation.
    let end_x = layout.frame_x(duration);
    if end_x < layout.frames.right() {
        let past_end = Rect::from_min_max(pos2(end_x, layout.frames.top()), layout.frames.max);
        painter.rect_filled(past_end, 0.0, Color32::from_black_alpha(60));
    }

    for (index, row) in rows.iter().enumerate() {
        let rect = layout.row_rect(index, layout.frames);
        if !rect.intersects(layout.frames) {
            continue;
        }
        let Some(layer) = doc.layer(row.layer) else { continue };
        if editor.selection == Some(row.layer) {
            painter.rect_filled(rect, 0.0, Color32::from_white_alpha(12));
        }
        match row.property {
            None => paint_summary_row(painter, editor, layout, layer, rect),
            Some(property) => paint_property_row(painter, editor, layout, layer, property, rect),
        }
        painter.hline(rect.x_range(), rect.bottom(), (1.0, Color32::from_black_alpha(60)));
    }

    // Faint column lines, stronger every 5 frames like a ruler.
    for frame in visible_frames {
        let x = layout.frame_x(frame);
        let alpha = if frame % 5 == 0 { 20 } else { 8 };
        painter.vline(x, layout.frames.y_range(), (1.0, Color32::from_white_alpha(alpha)));
    }

    let x = layout.frame_center_x(editor.frame);
    painter.vline(x, layout.frames.y_range(), (1.0, PLAYHEAD));
}

/// A layer's summary: one diamond wherever any property has a key, over a
/// band covering the frames where something is animating.
fn paint_summary_row(painter: &egui::Painter, editor: &Editor, layout: &Layout, layer: &Layer, row: Rect) {
    let span = row.shrink2(vec2(0.0, 4.0));
    let dim = |c: Color32| if layer.visible { c } else { c.gamma_multiply(0.4) };

    // A bar across the frames the layer appears on, with brighter ends to
    // show they can be dragged.
    let (start, end) = (layer.start, layer.last_frame(editor.last_frame()));
    let bar = Rect::from_x_y_ranges(layout.frame_x(start)..=layout.frame_x(end + 1), span.y_range());
    painter.rect_filled(bar, 2.0, dim(Color32::from_white_alpha(18)));
    for x in [bar.left(), bar.right()] {
        painter.vline(x, bar.y_range(), (2.0, dim(Color32::from_white_alpha(90))));
    }

    let frames = layer.track.key_frames();
    if let (Some(&first), Some(&last)) = (frames.first(), frames.last())
        && first != last
    {
        let x_range = layout.frame_center_x(first)..=layout.frame_center_x(last);
        painter.rect_filled(
            Rect::from_x_y_ranges(x_range, span.y_range()),
            0.0,
            dim(TWEEN.gamma_multiply(0.3)),
        );
    }
    paint_keys(painter, editor, layout, layer, &frames, row, dim);
}

/// One property's own tweens, colored by easing, and its keys.
fn paint_property_row(
    painter: &egui::Painter,
    editor: &Editor,
    layout: &Layout,
    layer: &Layer,
    property: Property,
    row: Rect,
) {
    let span = row.shrink2(vec2(0.0, 6.0));
    let dim = |c: Color32| if layer.visible { c } else { c.gamma_multiply(0.4) };
    let channel = layer.track.channel(property);

    for tween in channel.tweens() {
        let color = if tween.easing == Easing::Hold { HOLD } else { TWEEN };
        let x_range = layout.frame_center_x(tween.start)..=layout.frame_center_x(tween.end);
        painter.rect_filled(
            Rect::from_x_y_ranges(x_range.clone(), span.y_range()),
            0.0,
            dim(color.gamma_multiply(0.45)),
        );
        painter.hline(x_range, span.center().y, (1.0, dim(color)));
    }
    paint_keys(painter, editor, layout, layer, &channel.key_frames(), row, dim);
}

fn paint_keys(
    painter: &egui::Painter,
    editor: &Editor,
    layout: &Layout,
    layer: &Layer,
    frames: &[u32],
    row: Rect,
    dim: impl Fn(Color32) -> Color32,
) {
    let is_selected = editor.selection == Some(layer.id);
    for &frame in frames {
        let center = pos2(layout.frame_center_x(frame), row.center().y);
        let current = is_selected && frame == editor.frame;
        let fill = if current { CURRENT_KEY } else { Color32::from_gray(230) };
        diamond(painter, center, 5.0, dim(fill));
    }
}

fn paint_ruler(painter: &egui::Painter, ui: &egui::Ui, editor: &Editor, layout: &Layout) {
    let ruler = layout.ruler;
    painter.rect_filled(ruler, 0.0, ui.visuals().panel_fill);
    let text = ui.visuals().text_color();

    for frame in visible_frame_range(layout, editor.duration() + TRAILING_FRAMES) {
        let x = layout.frame_x(frame);
        // Frame numbers are shown 1-based at 1, 5, 10, 15… like Animate.
        let number = frame + 1;
        let labelled = number == 1 || number % 5 == 0;
        let tick = if labelled { 8.0 } else { 4.0 };
        painter.vline(x, ruler.bottom() - tick..=ruler.bottom(), (1.0, text.gamma_multiply(0.5)));
        if labelled {
            painter.text(
                pos2(x + 2.0, ruler.top() + 2.0),
                Align2::LEFT_TOP,
                number,
                FontId::proportional(10.0),
                text,
            );
        }
    }

    // The playhead handle shows the current frame number.
    let x = layout.frame_center_x(editor.frame);
    let handle = Rect::from_center_size(pos2(x, ruler.center().y), vec2(26.0, ruler.height() - 4.0));
    painter.rect_filled(handle, 3.0, PLAYHEAD);
    painter.text(
        handle.center(),
        Align2::CENTER_CENTER,
        editor.frame + 1,
        FontId::proportional(10.0),
        Color32::WHITE,
    );
}

/// Only the frames whose columns are on screen, so long animations stay fast.
fn visible_frame_range(layout: &Layout, total: u32) -> std::ops::Range<u32> {
    let first = layout.frame_at(layout.frames.left()).max(0) as u32;
    let last = (layout.frame_at(layout.frames.right()) + 1).max(0) as u32;
    first..last.min(total)
}

fn diamond(painter: &egui::Painter, center: Pos2, radius: f32, fill: Color32) {
    let points = vec![
        center + vec2(0.0, -radius),
        center + vec2(radius, 0.0),
        center + vec2(0.0, radius),
        center + vec2(-radius, 0.0),
    ];
    painter.add(egui::Shape::convex_polygon(points, fill, Stroke::new(1.0, Color32::from_black_alpha(160))));
}

/// The ▸ / ▾ toggle that expands a layer into its property rows.
fn disclosure_triangle(painter: &egui::Painter, center: Pos2, expanded: bool, color: Color32) {
    let r = 4.0;
    let points = if expanded {
        vec![center + vec2(-r, -r * 0.6), center + vec2(r, -r * 0.6), center + vec2(0.0, r * 0.7)]
    } else {
        vec![center + vec2(-r * 0.6, -r), center + vec2(r * 0.7, 0.0), center + vec2(-r * 0.6, r)]
    };
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Paints the audio rows: name and mute toggle on the left, the waveform on
/// the right, starting at each track's start frame.
#[allow(clippy::too_many_arguments)]
fn paint_audio_rows(
    frames: &egui::Painter,
    names: &egui::Painter,
    ui: &egui::Ui,
    editor: &Editor,
    audio: &mut AudioEngine,
    layout: &Layout,
    first_row: usize,
    tracks: &[AudioId],
) {
    let project_dir = editor.project_dir();
    let fps = editor.doc.frame_rate.fps();
    let text = ui.visuals().text_color();
    let faint = text.gamma_multiply(0.3);

    for (i, id) in tracks.iter().enumerate() {
        let Some(track) = editor.doc.audio_track(*id) else { continue };
        let index = first_row + i;
        let row = layout.row_rect(index, layout.frames);
        let name_row = layout.row_rect(index, layout.names);
        let color = if track.muted { AUDIO.gamma_multiply(0.35) } else { AUDIO };

        // Names column: speaker (click to mute) and the track's name.
        let speaker = if track.muted { "🔇" } else { "🔊" };
        names.text(
            layout.eye_rect(index).center(),
            Align2::CENTER_CENTER,
            speaker,
            FontId::proportional(13.0),
            if track.muted { faint } else { text },
        );
        let name_pos = pos2(layout.lock_rect(index).right() + 6.0, name_row.center().y);
        names.text(
            name_pos,
            Align2::LEFT_CENTER,
            &track.name,
            FontId::proportional(13.0),
            if track.muted { faint } else { text },
        );
        names.hline(name_row.x_range(), name_row.bottom(), (1.0, Color32::from_black_alpha(60)));

        // Frames: one bar per animation frame, as tall as that frame is loud.
        frames.rect_filled(row.shrink2(vec2(0.0, 2.0)), 0.0, color.gamma_multiply(0.12));
        if let Some(problem) = audio.problem(track, project_dir.as_deref()) {
            let at = pos2(layout.frame_x(track.start) + 4.0, row.center().y);
            frames.text(
                at,
                Align2::LEFT_CENTER,
                format!("Can't play: {problem}"),
                FontId::proportional(11.0),
                ui.visuals().error_fg_color,
            );
        } else if let Some(peaks) = audio.peaks(track, project_dir.as_deref(), fps) {
            // Scale each waveform to its own loudest moment. For timing, the
            // shape of the sound is what matters, and a quietly recorded
            // line of dialogue would otherwise be a flat, unreadable strip.
            let loudest = peaks.iter().copied().fold(0.0f32, f32::max).max(1e-6);
            let visible = visible_frame_range(layout, editor.duration() + TRAILING_FRAMES);
            for frame in visible.filter(|f| *f >= track.start) {
                let Some(&peak) = peaks.get((frame - track.start) as usize) else { break };
                let height = (peak / loudest * (ROW_HEIGHT - 6.0)).max(1.0);
                let x = layout.frame_center_x(frame);
                frames.vline(
                    x,
                    row.center().y - height / 2.0..=row.center().y + height / 2.0,
                    (FRAME_WIDTH * 0.6, color),
                );
            }
        } else if audio.is_loading(track, project_dir.as_deref()) {
            let at = pos2(layout.frame_x(track.start) + 4.0, row.center().y);
            frames.text(at, Align2::LEFT_CENTER, "Loading…", FontId::proportional(11.0), faint);
        }
        frames.hline(row.x_range(), row.bottom(), (1.0, Color32::from_black_alpha(60)));
    }

    // The waveforms are painted after the grid, so put the playhead back on top.
    if !tracks.is_empty() {
        frames.vline(layout.frame_center_x(editor.frame), layout.frames.y_range(), (1.0, PLAYHEAD));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use inkframe_core::{Document, Point, Transform};

    type State = (TimelineView, Editor, AudioEngine);

    /// A document with one layer keyed on frame 0, plus an opacity key on
    /// frame 10 and a position key on frame 20.
    fn harness() -> (Harness<'static, State>, LayerId) {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("A", vec![], 0, Transform::default());
        let track = &mut editor.doc.layer_mut(id).unwrap().track;
        track.opacity.set(10, 0.0);
        track.position.set(20, Point::new(100.0, 0.0));

        let mut h = Harness::new_ui_state(
            |ui, (timeline, editor, audio): &mut State| timeline.show(ui, editor, audio),
            (TimelineView::default(), editor, AudioEngine::without_output()),
        );
        h.set_size(vec2(1000.0, 300.0));
        h.run();
        (h, id)
    }

    /// Screen position of a frame's cell in a given row.
    fn cell(h: &Harness<'_, State>, row: usize, frame: u32) -> Pos2 {
        let timeline = &h.state().0;
        let layout = Layout::new(timeline.grid_rect.unwrap(), timeline.scroll);
        pos2(layout.frame_center_x(frame), layout.row_rect(row, layout.frames).center().y)
    }

    fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
        h.drag_at(from);
        h.step();
        h.hover_at(to);
        h.step();
        h.drop_at(to);
        h.run();
    }

    fn expand(h: &mut Harness<'_, State>) {
        let expander = {
            let timeline = &h.state().0;
            Layout::new(timeline.grid_rect.unwrap(), timeline.scroll).expander_rect(0).center()
        };
        h.drag_at(expander);
        h.step();
        h.drop_at(expander);
        h.run();
    }

    /// Screen position of a column boundary (the left edge of `frame`) in a row.
    fn boundary(h: &Harness<'_, State>, row: usize, frame: u32) -> Pos2 {
        let timeline = &h.state().0;
        let layout = Layout::new(timeline.grid_rect.unwrap(), timeline.scroll);
        pos2(layout.frame_x(frame), layout.row_rect(row, layout.frames).center().y)
    }

    #[test]
    fn dragging_the_bar_ends_sets_the_layer_span() {
        let (mut h, id) = harness();
        // Grab just inside each end, as a user would.
        let (from, to) = (boundary(&h, 0, 0) + vec2(2.0, 0.0), boundary(&h, 0, 5));
        drag(&mut h, from, to);
        // The animation is 48 frames, so the open end sits after frame 47.
        let (from, to) = (boundary(&h, 0, 48) - vec2(2.0, 0.0), boundary(&h, 0, 30));
        drag(&mut h, from, to);

        let layer = h.state().1.doc.layer(id).unwrap();
        assert_eq!((layer.start, layer.end), (5, Some(29)));
    }

    #[test]
    fn dragging_the_end_back_out_makes_the_layer_open_ended() {
        let (mut h, id) = harness();
        h.state_mut().1.doc.layer_mut(id).unwrap().set_end(Some(20));
        h.run();
        let (from, to) = (boundary(&h, 0, 21) - vec2(2.0, 0.0), boundary(&h, 0, 60));
        drag(&mut h, from, to);

        assert_eq!(h.state().1.doc.layer(id).unwrap().end, None);
    }

    #[test]
    fn dragging_audio_moves_where_it_starts() {
        let (mut h, _) = harness();
        let id = h.state_mut().1.doc.add_audio(std::path::Path::new("/nowhere/voice.wav"), None, 4);
        h.run();
        // The audio row sits below the single layer row.
        let (from, to) = (cell(&h, 1, 6), cell(&h, 1, 16));
        drag(&mut h, from, to);

        assert_eq!(h.state().1.doc.audio_track(id).unwrap().start, 14);
    }

    #[test]
    fn expanding_a_layer_shows_its_properties() {
        let (mut h, _) = harness();
        expand(&mut h);
        let (timeline, editor, _) = h.state();
        let rows = timeline.rows(editor);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4].property, Some(Property::Opacity));
    }

    #[test]
    fn dragging_a_property_key_moves_only_that_property() {
        let (mut h, id) = harness();
        expand(&mut h);
        // Rows: layer, Position, Rotation, Scale, Opacity.
        let (from, to) = (cell(&h, 4, 10), cell(&h, 4, 15));
        drag(&mut h, from, to);

        let track = &h.state().1.doc.layer(id).unwrap().track;
        assert_eq!(track.opacity.keys().iter().map(|k| k.frame).collect::<Vec<_>>(), [0, 15]);
        assert_eq!(track.key_frames(), [0, 15, 20]);
    }

    #[test]
    fn dragging_a_summary_key_moves_every_property_on_that_frame() {
        let (mut h, id) = harness();
        let (from, to) = (cell(&h, 0, 0), cell(&h, 0, 3));
        drag(&mut h, from, to);

        let track = &h.state().1.doc.layer(id).unwrap().track;
        assert!(Property::ALL.iter().all(|&p| track.channel(p).has_key(3) && !track.channel(p).has_key(0)));
    }
}
