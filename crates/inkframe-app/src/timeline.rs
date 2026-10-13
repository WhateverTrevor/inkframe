//! The timeline panel: transport controls, plus a grid of layers (rows) by
//! frames (columns) showing keyframes and tweens.
//!
//! The grid is painted by hand rather than built from widgets. A long
//! animation has thousands of cells, and painting only the visible ones keeps
//! it fast. It also lets the ruler and the layer names stay pinned while the
//! frames scroll underneath.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use inkframe_core::{Easing, Layer, LayerId};

use crate::editor::Editor;

const NAME_WIDTH: f32 = 200.0;
const RULER_HEIGHT: f32 = 22.0;
const ROW_HEIGHT: f32 = 24.0;
const FRAME_WIDTH: f32 = 12.0;
/// Columns drawn past the end of the animation, so the end is visible.
const TRAILING_FRAMES: u32 = 20;

const PLAYHEAD: Color32 = Color32::from_rgb(0xe8, 0x3a, 0x3a);
/// Animate shows tweens in a violet-blue; we follow suit so they read the same.
const TWEEN: Color32 = Color32::from_rgb(0x8a, 0x7c, 0xd8);
const HOLD: Color32 = Color32::from_gray(150);

enum Drag {
    /// Dragging along the ruler or empty cells moves the playhead.
    Scrub,
    /// Dragging a keyframe slides it to another frame.
    Key { layer: LayerId, frame: u32 },
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

    fn eye_rect(&self, row: usize) -> Rect {
        let r = self.row_rect(row, self.names);
        Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), Vec2::splat(ROW_HEIGHT))
    }

    fn lock_rect(&self, row: usize) -> Rect {
        self.eye_rect(row).translate(vec2(ROW_HEIGHT, 0.0))
    }
}

#[derive(Default)]
pub struct TimelineView {
    scroll: Vec2,
    drag: Option<Drag>,
}

impl TimelineView {
    pub fn is_busy(&self) -> bool {
        self.drag.is_some()
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        transport_bar(ui, editor);
        ui.add_space(2.0);
        self.grid(ui, editor);
    }

    fn grid(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;

        // Rows run top to bottom from the front-most layer, the reverse of
        // the document's back-to-front storage order.
        let rows: Vec<LayerId> = editor.doc.layers.iter().rev().map(|l| l.id).collect();

        self.update_scroll(ui, &response, editor, rect, rows.len());
        let layout = Layout::new(rect, self.scroll);

        self.handle_input(ui, &response, editor, &layout, &rows);
        context_menu(&response, editor);

        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        paint_frames(&painter.with_clip_rect(layout.frames), editor, &layout, &rows);
        paint_ruler(&painter.with_clip_rect(layout.ruler), ui, editor, &layout);
        paint_names(&painter.with_clip_rect(layout.names), ui, editor, &layout, &rows);

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
        rows: &[LayerId],
    ) {
        let last_frame = editor.doc.last_frame();
        let clamp_frame = |f: i64| f.clamp(0, last_frame as i64) as u32;
        let row_layer = |y: f32| layout.row_at(y).and_then(|row| rows.get(row).copied());

        // Right-click moves the playhead and selection to the clicked cell
        // first, so the context menu acts on what was clicked.
        if response.secondary_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                if layout.frames.contains(p) {
                    editor.set_frame(clamp_frame(layout.frame_at(p.x)));
                    editor.selection = row_layer(p.y);
                }
            }
        }

        let pressed = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed());
        if pressed {
            if let Some(p) = response.interact_pointer_pos() {
                self.drag = self.press(editor, layout, rows, p);
            }
        }

        if let (Some(drag), Some(p)) = (&mut self.drag, response.interact_pointer_pos()) {
            let frame = clamp_frame(layout.frame_at(p.x));
            match drag {
                Drag::Scrub => editor.set_frame(frame),
                Drag::Key { layer, frame: from } => {
                    let moved = editor.doc.layer_mut(*layer).is_some_and(|l| l.track.move_key(*from, frame));
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
    fn press(&self, editor: &mut Editor, layout: &Layout, rows: &[LayerId], p: Pos2) -> Option<Drag> {
        if layout.ruler.contains(p) {
            return Some(Drag::Scrub);
        }

        let row = layout.row_at(p.y)?;
        let id = rows.get(row).copied();

        if layout.names.contains(p) {
            let layer = editor.doc.layer_mut(id?)?;
            if layout.eye_rect(row).contains(p) {
                layer.visible = !layer.visible;
            } else if layout.lock_rect(row).contains(p) {
                layer.locked = !layer.locked;
            } else {
                editor.selection = id;
            }
            return None;
        }

        if layout.frames.contains(p) {
            editor.selection = id;
            let frame = layout.frame_at(p.x);
            if let (Some(id), Ok(frame)) = (id, u32::try_from(frame)) {
                let layer = editor.doc.layer(id)?;
                if !layer.locked && layer.track.key_at(frame).is_some() {
                    editor.set_frame(frame);
                    return Some(Drag::Key { layer: id, frame });
                }
            }
            return Some(Drag::Scrub);
        }
        None
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

fn context_menu(response: &egui::Response, editor: &mut Editor) {
    response.context_menu(|ui| {
        let frame = editor.frame;
        let Some(layer) = editor.selected_layer() else {
            ui.label("Click a layer row to edit its keyframes");
            return;
        };
        let has_key = layer.track.key_at(frame).is_some();
        let can_remove = has_key && layer.track.keys().len() > 1;
        let tween_easing = layer.track.tween_start(frame).map(|k| k.easing);

        if ui.add_enabled(!has_key, egui::Button::new("Insert Keyframe").shortcut_text("F6")).clicked() {
            editor.insert_keyframe();
            ui.close();
        }
        if ui.add_enabled(can_remove, egui::Button::new("Remove Keyframe").shortcut_text("⇧F6")).clicked() {
            editor.remove_keyframe();
            ui.close();
        }
        ui.add_enabled_ui(tween_easing.is_some(), |ui| {
            ui.menu_button("Easing", |ui| {
                for preset in Easing::PRESETS {
                    if ui.selectable_label(tween_easing == Some(preset), preset.label()).clicked() {
                        if let Some(key) =
                            editor.selected_layer_mut().and_then(|l| l.track.tween_start_mut(frame))
                        {
                            key.easing = preset;
                        }
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

fn paint_frames(painter: &egui::Painter, editor: &Editor, layout: &Layout, rows: &[LayerId]) {
    let doc = &editor.doc;
    let visible_frames = visible_frame_range(layout, doc.duration + TRAILING_FRAMES);

    // Darken the columns past the end of the animation.
    let end_x = layout.frame_x(doc.duration);
    if end_x < layout.frames.right() {
        let past_end = Rect::from_min_max(pos2(end_x, layout.frames.top()), layout.frames.max);
        painter.rect_filled(past_end, 0.0, Color32::from_black_alpha(60));
    }

    for (row, id) in rows.iter().enumerate() {
        let row_rect = layout.row_rect(row, layout.frames);
        if !row_rect.intersects(layout.frames) {
            continue;
        }
        let Some(layer) = doc.layer(*id) else { continue };
        if editor.selection == Some(*id) {
            painter.rect_filled(row_rect, 0.0, Color32::from_white_alpha(12));
        }
        paint_layer_track(painter, editor, layout, layer, row_rect);
        painter.hline(row_rect.x_range(), row_rect.bottom(), (1.0, Color32::from_black_alpha(60)));
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

fn paint_layer_track(painter: &egui::Painter, editor: &Editor, layout: &Layout, layer: &Layer, row: Rect) {
    let keys = layer.track.keys();
    let span = row.shrink2(vec2(0.0, 4.0));
    let dim = |c: Color32| if layer.visible { c } else { c.gamma_multiply(0.4) };

    // The layer's content exists on every frame; show that as a faint bar.
    let all = Rect::from_x_y_ranges(layout.frame_x(0)..=layout.frame_x(editor.doc.duration), span.y_range());
    painter.rect_filled(all, 2.0, dim(Color32::from_white_alpha(10)));

    // Each pair of keyframes is a tween; Hold tweens are drawn grey.
    for pair in keys.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let color = if a.easing == Easing::Hold { HOLD } else { TWEEN };
        let x_range = layout.frame_center_x(a.frame)..=layout.frame_center_x(b.frame);
        painter.rect_filled(
            Rect::from_x_y_ranges(x_range.clone(), span.y_range()),
            0.0,
            dim(color.gamma_multiply(0.45)),
        );
        painter.hline(x_range, span.center().y, (1.0, dim(color)));
    }

    let is_selected = editor.selection == Some(layer.id);
    for key in keys {
        let center = pos2(layout.frame_center_x(key.frame), row.center().y);
        let current = is_selected && key.frame == editor.frame;
        let fill = if current { Color32::from_rgb(0x2f, 0x9b, 0xff) } else { Color32::from_gray(230) };
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

fn paint_names(painter: &egui::Painter, ui: &egui::Ui, editor: &Editor, layout: &Layout, rows: &[LayerId]) {
    painter.rect_filled(layout.names, 0.0, ui.visuals().panel_fill);
    let text = ui.visuals().text_color();
    let faint = text.gamma_multiply(0.3);
    let font = FontId::proportional(13.0);

    for (row, id) in rows.iter().enumerate() {
        let Some(layer) = editor.doc.layer(*id) else { continue };
        let rect = layout.row_rect(row, layout.names);
        if editor.selection == Some(*id) {
            painter.rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
        }

        let eye = if layer.visible { text } else { faint };
        painter.text(layout.eye_rect(row).center(), Align2::CENTER_CENTER, "👁", font.clone(), eye);
        let (lock, lock_color) = if layer.locked { ("🔒", text) } else { ("🔓", faint) };
        painter.text(layout.lock_rect(row).center(), Align2::CENTER_CENTER, lock, font.clone(), lock_color);

        let name_pos = pos2(layout.lock_rect(row).right() + 6.0, rect.center().y);
        painter.text(
            name_pos,
            Align2::LEFT_CENTER,
            &layer.name,
            font.clone(),
            if layer.visible { text } else { faint },
        );
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
