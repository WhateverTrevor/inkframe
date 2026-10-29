//! Picking colours as painting apps do: a hue ring round a square of
//! saturation and brightness, sliders for hue, saturation, brightness and
//! opacity, the colour as hex, and the colours used lately.
//!
//! Every colour button in Inkframe opens this picker ([`color_button`]),
//! and the Colour panel ([`panel`]) keeps it open for the colour the
//! current tool uses. The recent colours are shared by all of them and
//! kept for next time in the settings (see `prefs.rs`).

use std::f32::consts::TAU;
use std::sync::Mutex;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, vec2};
use inkframe_core::{Color, hsv_to_rgb, rgb_to_hsv};

use crate::convert::to_color32;
use crate::editor::{Editor, Tool};
use crate::i18n::tr;

/// How many recent colours are kept.
const RECENT_COUNT: usize = 16;
/// The wheel's size across at most, in points.
const WHEEL: f32 = 240.0;
/// How thick the hue ring is.
const RING: f32 = 18.0;

/// The colours used lately, newest first.
static RECENT: Mutex<Vec<Color>> = Mutex::new(Vec::new());

pub fn recent() -> Vec<Color> {
    RECENT.lock().expect("not poisoned").clone()
}

/// Sets the recent colours, as loaded from the settings.
pub fn set_recent(colors: Vec<Color>) {
    let mut recent = RECENT.lock().expect("not poisoned");
    *recent = colors;
    recent.truncate(RECENT_COUNT);
}

/// Puts `color` first among the recent colours (moving it there if it was
/// already among them).
fn remember(color: Color) {
    let mut recent = RECENT.lock().expect("not poisoned");
    recent.retain(|c| *c != color);
    recent.insert(0, color);
    recent.truncate(RECENT_COUNT);
}

/// A swatch of `color` that opens the picker under it when clicked.
/// Returns true when the colour changed.
pub fn color_button(ui: &mut egui::Ui, color: &mut Color) -> bool {
    let size = ui.spacing().interact_size;
    let (rect, response) = ui.allocate_exact_size(vec2(size.y * 2.0, size.y), Sense::click());
    paint_swatch(ui.painter(), rect, *color);
    if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, ui.visuals().strong_text_color()),
            egui::StrokeKind::Outside,
        );
    }
    let mut changed = false;
    egui::Popup::menu(&response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        changed = picker(ui, color);
    });
    changed
}

/// A colour over a checkerboard, so how see-through it is shows.
fn paint_swatch(painter: &egui::Painter, rect: Rect, color: Color) {
    if color.a < 255 {
        let cell = rect.height() / 2.0;
        painter.rect_filled(rect, 2.0, Color32::from_gray(230));
        let columns = (rect.width() / cell).ceil() as usize;
        for row in 0..2 {
            for column in (row % 2..columns).step_by(2) {
                let min = rect.min + vec2(column as f32 * cell, row as f32 * cell);
                let square = Rect::from_min_size(min, Vec2::splat(cell)).intersect(rect);
                painter.rect_filled(square, 0.0, Color32::from_gray(170));
            }
        }
    }
    painter.rect_filled(rect, 2.0, to_color32(color));
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, Color32::from_black_alpha(110)),
        egui::StrokeKind::Inside,
    );
}

/// Hue, saturation and value, each 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

impl Hsv {
    fn of(color: Color) -> Self {
        let (h, s, v) = rgb_to_hsv(color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0);
        Self { h, s, v }
    }

    fn color(self, alpha: u8) -> Color {
        let (r, g, b) = hsv_to_rgb(self.h, self.s, self.v);
        let byte = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
        Color::rgba(byte(r), byte(g), byte(b), alpha)
    }
}

/// What a drag on the wheel is moving.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Dragging {
    Hue,
    Square,
}

/// The picker itself: the wheel, the sliders, hex and the recent colours.
/// Returns true when the colour changed.
pub fn picker(ui: &mut egui::Ui, color: &mut Color) -> bool {
    let id = ui.id().with("colour picker");
    // The hue (and saturation) are kept while they can't be told from the
    // colour: at no saturation every hue is the same grey, and at no
    // brightness, the same black. Without this the ring would jump to red.
    let kept: Option<(Color, Hsv)> = ui.data(|d| d.get_temp(id));
    let mut hsv = match kept {
        Some((was, hsv)) if was == *color => hsv,
        _ => Hsv::of(*color),
    };
    let before = *color;
    let mut changed = wheel(ui, id, &mut hsv);

    egui::Grid::new(id.with("sliders")).num_columns(2).show(ui, |ui| {
        let mut row = |ui: &mut egui::Ui, label: &str, value: &mut f32, range: f32, unit: &str| {
            ui.label(label);
            let mut shown = *value * range;
            let slider = egui::Slider::new(&mut shown, 0.0..=range).fixed_decimals(0).suffix(unit);
            if ui.add(slider).changed() {
                *value = shown / range;
                changed = true;
            }
            ui.end_row();
        };
        row(ui, tr("Hue"), &mut hsv.h, 360.0, "°");
        row(ui, tr("Saturation"), &mut hsv.s, 100.0, "%");
        row(ui, tr("Brightness"), &mut hsv.v, 100.0, "%");
        let mut alpha = color.a as f32 / 255.0;
        row(ui, tr("Opacity"), &mut alpha, 100.0, "%");
        color.a = (alpha * 255.0).round() as u8;
    });
    if changed {
        *color = hsv.color(color.a);
    }

    // As hex, to copy, paste or type; it takes effect once it parses.
    ui.horizontal(|ui| {
        ui.label(tr("Hex"));
        let text_id = id.with("hex");
        let mut text: String = ui
            .data(|d| d.get_temp(text_id))
            .filter(|_| ui.memory(|m| m.has_focus(text_id)))
            .unwrap_or_else(|| color.to_string());
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id(text_id)
                .desired_width(90.0)
                .font(egui::TextStyle::Monospace),
        );
        if response.changed() {
            let typed = if text.starts_with('#') { text.clone() } else { format!("#{text}") };
            if let Ok(parsed) = typed.parse::<Color>() {
                *color = parsed;
                hsv = Hsv::of(parsed);
            }
        }
        ui.data_mut(|d| d.insert_temp(text_id, text));
    });

    // The recent colours: clicking one picks it.
    let recent = recent();
    if !recent.is_empty() {
        ui.label(tr("Recent"));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            for used in recent {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                paint_swatch(ui.painter(), rect, used);
                if response.on_hover_text(used.to_string()).clicked() {
                    *color = used;
                    hsv = Hsv::of(used);
                }
            }
        });
    }

    let changed = *color != before;
    ui.data_mut(|d| d.insert_temp(id, (*color, hsv)));
    // A colour settled on goes among the recent ones: at once when typed
    // or clicked, and at the end of a drag rather than at each step of it.
    let pending_id = id.with("unsettled");
    let held = ui.input(|i| i.pointer.any_down());
    if changed && held {
        ui.data_mut(|d| d.insert_temp(pending_id, true));
    } else if changed || (!held && ui.data(|d| d.get_temp::<bool>(pending_id)).unwrap_or(false)) {
        remember(*color);
        ui.data_mut(|d| d.remove::<bool>(pending_id));
    }
    changed
}

/// The hue ring and, inside it, the square of saturation (across) and
/// brightness (up). Returns true when `hsv` changed.
fn wheel(ui: &mut egui::Ui, id: egui::Id, hsv: &mut Hsv) -> bool {
    // As wide as there's room for, within reason.
    let size = ui.available_width().clamp(140.0, WHEEL);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let centre = rect.center();
    let outer = size / 2.0;
    let inner = outer - RING;
    // The square fits inside the ring with a little room.
    let half = inner * std::f32::consts::FRAC_1_SQRT_2 - 4.0;
    let square = Rect::from_center_size(centre, Vec2::splat(half * 2.0));
    let painter = ui.painter_at(rect);

    // The ring: red at the top, round clockwise.
    let angle_of = |h: f32| h * TAU - TAU / 4.0;
    let mut ring = Mesh::default();
    const STEPS: usize = 96;
    for i in 0..=STEPS {
        let h = i as f32 / STEPS as f32;
        let (sin, cos) = angle_of(h).sin_cos();
        let shade = to_color32(Hsv { h, s: 1.0, v: 1.0 }.color(255));
        ring.vertices.push(Vertex { pos: centre + vec2(cos, sin) * outer, uv: WHITE_UV, color: shade });
        ring.vertices.push(Vertex { pos: centre + vec2(cos, sin) * inner, uv: WHITE_UV, color: shade });
        if i > 0 {
            let at = (i as u32) * 2;
            ring.add_triangle(at - 2, at - 1, at);
            ring.add_triangle(at - 1, at + 1, at);
        }
    }
    painter.add(ring);

    // The square, as a grid so every point is its exact colour.
    let mut grid = Mesh::default();
    const CELLS: u32 = 16;
    for row in 0..=CELLS {
        for column in 0..=CELLS {
            let (s, v) = (column as f32 / CELLS as f32, 1.0 - row as f32 / CELLS as f32);
            let pos = square.min + vec2(s, 1.0 - v) * square.size();
            let shade = to_color32(Hsv { h: hsv.h, s, v }.color(255));
            grid.vertices.push(Vertex { pos, uv: WHITE_UV, color: shade });
            if row > 0 && column > 0 {
                let at = |r: u32, c: u32| r * (CELLS + 1) + c;
                grid.add_triangle(at(row - 1, column - 1), at(row - 1, column), at(row, column));
                grid.add_triangle(at(row - 1, column - 1), at(row, column), at(row, column - 1));
            }
        }
    }
    painter.add(grid);

    // Markers: rings in black and white, so they show on any colour.
    let marker = |at: Pos2, radius: f32| {
        painter.circle_stroke(at, radius, Stroke::new(2.5, Color32::BLACK));
        painter.circle_stroke(at, radius, Stroke::new(1.2, Color32::WHITE));
    };
    let (sin, cos) = angle_of(hsv.h).sin_cos();
    marker(centre + vec2(cos, sin) * (inner + RING / 2.0), RING / 2.0 - 2.0);
    marker(square.min + vec2(hsv.s, 1.0 - hsv.v) * square.size(), 5.0);

    // A press on the ring turns the hue; in the square, it picks the rest.
    // The drag keeps to what it started on, however far it strays.
    let dragging_id = id.with("wheel drag");
    if response.drag_started() || response.clicked() {
        let at = response.interact_pointer_pos().unwrap_or(centre);
        let on_ring = (at - centre).length() >= inner - 2.0;
        ui.data_mut(|d| d.insert_temp(dragging_id, if on_ring { Dragging::Hue } else { Dragging::Square }));
    }
    let dragging: Option<Dragging> = ui.data(|d| d.get_temp(dragging_id));
    let mut changed = false;
    if let (Some(dragging), Some(at)) = (dragging, response.interact_pointer_pos())
        && (response.dragged() || response.clicked() || response.drag_started())
    {
        match dragging {
            Dragging::Hue => {
                let offset = at - centre;
                hsv.h = ((offset.y.atan2(offset.x) + TAU / 4.0) / TAU).rem_euclid(1.0);
            }
            Dragging::Square => {
                let t = (at - square.min) / square.size();
                hsv.s = t.x.clamp(0.0, 1.0);
                hsv.v = (1.0 - t.y).clamp(0.0, 1.0);
            }
        }
        changed = true;
    }
    if response.drag_stopped() || response.clicked() {
        ui.data_mut(|d| d.remove::<Dragging>(dragging_id));
    }
    changed
}

/// The Colour panel: the picker kept open for the colour the current tool
/// uses, and, for painting, the second colour beside it to swap with.
pub fn panel(ui: &mut egui::Ui, editor: &mut Editor) {
    let (label, color) = current_color(editor);
    ui.heading(label);
    picker(ui, color);
    if editor.tool.paints() {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(tr("Second colour"));
            color_button(ui, &mut editor.second_color);
            if ui
                .button(tr("Swap"))
                .on_hover_text(tr("Swap the paint colour and the second colour"))
                .clicked()
            {
                std::mem::swap(&mut editor.paint_color, &mut editor.second_color);
            }
        });
    }
}

/// The colour the current tool uses, and what to call it.
fn current_color(editor: &mut Editor) -> (&'static str, &mut Color) {
    if editor.tool == Tool::Pencil {
        return (tr("Line colour"), &mut editor.pencil_stroke.color);
    }
    if editor.tool.paints() {
        return (tr("Paint colour"), &mut editor.paint_color);
    }
    // New shapes' fill, brought back if it was turned off.
    let fill = editor.draw_style.fill.get_or_insert(Color::BLACK);
    (tr("Fill colour"), fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip_through_hue_saturation_and_brightness() {
        for color in
            [Color::rgb(255, 0, 0), Color::rgb(12, 200, 99), Color::rgba(250, 240, 180, 128), Color::BLACK]
        {
            assert_eq!(Hsv::of(color).color(color.a), color);
        }
    }

    #[test]
    fn recent_colours_put_the_newest_first_without_repeats() {
        set_recent(Vec::new());
        let (red, blue) = (Color::rgb(255, 0, 0), Color::rgb(0, 0, 255));
        remember(red);
        remember(blue);
        remember(red);
        assert_eq!(recent(), [red, blue]);
        for i in 0..40 {
            remember(Color::rgb(i, i, i));
        }
        assert_eq!(recent().len(), RECENT_COUNT);
    }
}
