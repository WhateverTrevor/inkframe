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
use inkframe_core::{AnyChannel, Easing, Layer, LayerId, Property};

use crate::editor::Editor;

const NAME_WIDTH: f32 = 200.0;
const RULER_HEIGHT: f32 = 22.0;
const ROW_HEIGHT: f32 = 24.0;
const FRAME_WIDTH: f32 = 12.0;
/// How far property names are indented under their layer.
const PROPERTY_INDENT: f32 = 44.0;
/// Columns drawn past the end of the animation, so the end is visible.
const TRAILING_FRAMES: u32 = 20;

const PLAYHEAD: Color32 = Color32::from_rgb(0xe8, 0x3a, 0x3a);
const CURRENT_KEY: Color32 = Color32::from_rgb(0x2f, 0x9b, 0xff);
/// Animate shows tweens in a violet-blue; we follow suit so they read the same.
const TWEEN: Color32 = Color32::from_rgb(0x8a, 0x7c, 0xd8);
const HOLD: Color32 = Color32::from_gray(150);

/// One line of the grid: a layer's summary row, or one of its properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    layer: LayerId,
    /// `None` for the layer's summary row.
    property: Option<Property>,
}

enum Drag {
    /// Dragging along the ruler or empty cells moves the playhead.
    Scrub,
    /// Dragging a key slides it to another frame. On a summary row this
    /// moves every property's key on that frame together.
    Key { row: Row, frame: u32 },
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
    /// Where the grid was drawn last frame. Tests use it to find cells.
    #[cfg(test)]
    grid_rect: Option<Rect>,
}

impl TimelineView {
    pub fn is_busy(&self) -> bool {
        self.drag.is_some()
    }

    /// Shows or hides a layer's property rows.
    pub fn set_expanded(&mut self, layer: LayerId, expanded: bool) {
        if expanded {
            self.expanded.insert(layer);
        } else {
            self.expanded.remove(&layer);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        transport_bar(ui, editor);
        ui.add_space(2.0);
        self.grid(ui, editor);
    }

    /// The rows to show, top to bottom. Layers run from the front-most down,
    /// the reverse of the document's back-to-front storage order.
    fn rows(&self, editor: &Editor) -> Vec<Row> {
        let mut rows = Vec::new();
        for layer in editor.doc.layers.iter().rev() {
            rows.push(Row { layer: layer.id, property: None });
            if self.expanded.contains(&layer.id) {
                rows.extend(Property::ALL.map(|p| Row { layer: layer.id, property: Some(p) }));
            }
        }
        rows
    }

    fn grid(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        #[cfg(test)]
        {
            self.grid_rect = Some(rect);
        }

        // Forget expanded layers that have since been deleted.
        self.expanded.retain(|id| editor.doc.layer(*id).is_some());
        let rows = self.rows(editor);

        self.update_scroll(ui, &response, editor, rect, rows.len());
        let layout = Layout::new(rect, self.scroll);

        self.handle_input(ui, &response, editor, &layout, &rows);
        context_menu(&response, editor, self.context_row);

        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        paint_frames(&painter.with_clip_rect(layout.frames), editor, &layout, &rows);
        paint_ruler(&painter.with_clip_rect(layout.ruler), ui, editor, &layout);
        self.paint_names(&painter.with_clip_rect(layout.names), ui, editor, &layout, &rows);

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
            vec2((editor.doc.duration + TRAILING_FRAMES) as f32 * FRAME_WIDTH, rows as f32 * ROW_HEIGHT);
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
    ) {
        let last_frame = editor.doc.last_frame();
        let clamp_frame = |f: i64| f.clamp(0, last_frame as i64) as u32;
        let row_under = |y: f32| layout.row_at(y).and_then(|row| rows.get(row).copied());

        // Right-click moves the playhead and selection to the clicked cell
        // first, so the context menu acts on what was clicked.
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
            && layout.frames.contains(p)
        {
            editor.set_frame(clamp_frame(layout.frame_at(p.x)));
            self.context_row = row_under(p.y);
            if let Some(row) = self.context_row {
                select_row(editor, row);
            }
        }

        let pressed = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed());
        if pressed && let Some(p) = response.interact_pointer_pos() {
            self.drag = self.press(editor, layout, rows, p);
        }

        if let (Some(drag), Some(p)) = (&mut self.drag, response.interact_pointer_pos()) {
            let frame = clamp_frame(layout.frame_at(p.x));
            match drag {
                Drag::Scrub => editor.set_frame(frame),
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

    /// Decides what a primary-button press starts, based on where it landed.
    fn press(&mut self, editor: &mut Editor, layout: &Layout, rows: &[Row], p: Pos2) -> Option<Drag> {
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
            select_row(editor, row);
            return None;
        }

        if layout.frames.contains(p) {
            let Some(row) = row else {
                editor.selection = None;
                return Some(Drag::Scrub);
            };
            select_row(editor, row);
            if let Ok(frame) = u32::try_from(layout.frame_at(p.x)) {
                let layer = editor.doc.layer(row.layer)?;
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
            let selected = editor.selection == Some(row.layer);

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
                    painter.text(name_pos, Align2::LEFT_CENTER, &layer.name, font.clone(), color);
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

/// Selects a row's layer, and for a property row, points the inspector's
/// easing graph at that property.
fn select_row(editor: &mut Editor, row: Row) {
    editor.selection = Some(row.layer);
    if let Some(property) = row.property {
        editor.focused_property = property;
    }
}

fn transport_bar(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.horizontal(|ui| {
        let last = editor.doc.last_frame();
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
        ui.label(format!("of {}   {seconds:.2}s", editor.doc.duration));

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

fn context_menu(response: &egui::Response, editor: &mut Editor, row: Option<Row>) {
    response.context_menu(|ui| {
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
    let visible_frames = visible_frame_range(layout, doc.duration + TRAILING_FRAMES);

    // Darken the columns past the end of the animation.
    let end_x = layout.frame_x(doc.duration);
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

    // The layer's content exists on every frame; show that as a faint bar.
    let all = Rect::from_x_y_ranges(layout.frame_x(0)..=layout.frame_x(editor.doc.duration), span.y_range());
    painter.rect_filled(all, 2.0, dim(Color32::from_white_alpha(10)));

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

    for frame in visible_frame_range(layout, editor.doc.duration + TRAILING_FRAMES) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use inkframe_core::{Document, Point, Transform};

    type State = (TimelineView, Editor);

    /// A document with one layer keyed on frame 0, plus an opacity key on
    /// frame 10 and a position key on frame 20.
    fn harness() -> (Harness<'static, State>, LayerId) {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("A", vec![], 0, Transform::default());
        let track = &mut editor.doc.layer_mut(id).unwrap().track;
        track.opacity.set(10, 0.0);
        track.position.set(20, Point::new(100.0, 0.0));

        let mut h = Harness::new_ui_state(
            |ui, (timeline, editor): &mut State| timeline.show(ui, editor),
            (TimelineView::default(), editor),
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

    #[test]
    fn expanding_a_layer_shows_its_properties() {
        let (mut h, _) = harness();
        expand(&mut h);
        let (timeline, editor) = h.state();
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
