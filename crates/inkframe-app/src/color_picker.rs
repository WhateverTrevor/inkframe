//! Picking colours as painting apps do, in the style you like (see
//! [`Style`]): a hue ring round a square, or round a triangle; sliders for
//! hue, saturation and lightness; or swatches. Each has opacity, the
//! colour as hex, and the colours used lately.
//!
//! Every colour button in Inkframe opens this picker ([`color_button`]),
//! and the Colour tab ([`panel`]) keeps it open for the colour active in
//! the toolbar's well (see `colour_well.rs`). The style and the recent
//! colours are shared by all of them and kept for next time in the
//! settings (see `prefs.rs`).

use std::f32::consts::TAU;
use std::sync::Mutex;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use inkframe_core::{Color, hsv_to_rgb, rgb_to_hsv};

use crate::convert::to_color32;
use crate::editor::Editor;
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
pub(crate) fn paint_swatch(painter: &egui::Painter, rect: Rect, color: Color) {
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

    /// Hue, saturation and lightness, as CSS's `hsl()` has them.
    fn hsl(self) -> (f32, f32, f32) {
        let l = self.v * (1.0 - self.s / 2.0);
        let s = if l <= 0.0 || l >= 1.0 { 0.0 } else { (self.v - l) / l.min(1.0 - l) };
        (self.h, s, l)
    }

    fn from_hsl(h: f32, s: f32, l: f32) -> Self {
        let v = l + s * l.min(1.0 - l);
        let s = if v <= 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) };
        Self { h, s, v }
    }

    fn color(self, alpha: u8) -> Color {
        let (r, g, b) = hsv_to_rgb(self.h, self.s, self.v);
        let byte = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
        Color::rgba(byte(r), byte(g), byte(b), alpha)
    }
}

/// The ways of picking a colour, as the picker's tabs offer them. The one
/// last used is used everywhere, and kept for next time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// A hue ring round a square of saturation and brightness.
    Wheel,
    /// A hue ring round a triangle from the pure hue to white and black, as
    /// Painter's and Krita's (and, people remember, Adobe's) pickers have.
    Triangle,
    /// Hue, saturation and lightness sliders, each showing the colours it
    /// runs through, with red, green and blue as numbers.
    Sliders,
    /// Ready colours: the project's swatches, a standard set, and the
    /// recent ones.
    Swatches,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Wheel, Style::Triangle, Style::Sliders, Style::Swatches];

    fn label(self) -> &'static str {
        match self {
            Style::Wheel => tr("Wheel"),
            Style::Triangle => tr("Triangle"),
            Style::Sliders => tr("HSL"),
            Style::Swatches => tr("Swatches"),
        }
    }

    /// Its name in the settings file, which stays the same in every language.
    pub fn key(self) -> &'static str {
        match self {
            Style::Wheel => "wheel",
            Style::Triangle => "triangle",
            Style::Sliders => "hsl",
            Style::Swatches => "swatches",
        }
    }

    pub fn from_key(key: &str) -> Option<Style> {
        Style::ALL.into_iter().find(|s| s.key() == key)
    }
}

static STYLE: Mutex<Style> = Mutex::new(Style::Wheel);
/// The open project's swatches, for the Swatches style, set as the project
/// changes (see `set_project_swatches`): the picker is opened from many
/// places that don't otherwise see the project.
static PROJECT: Mutex<Vec<(String, Color)>> = Mutex::new(Vec::new());

pub fn style() -> Style {
    *STYLE.lock().expect("not poisoned")
}

pub fn set_style(style: Style) {
    *STYLE.lock().expect("not poisoned") = style;
}

pub fn set_project_swatches(swatches: Vec<(String, Color)>) {
    *PROJECT.lock().expect("not poisoned") = swatches;
}

/// What a drag on the ring and what's inside it is moving.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Dragging {
    Hue,
    Inside,
}

/// What's inside the hue ring.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Inside {
    Square,
    Triangle,
}

/// The picker: a tab for each style, that style's controls, opacity, hex,
/// and (but for Swatches, which shows them itself) the recent colours.
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

    let mut style = style();
    ui.horizontal(|ui| {
        for option in Style::ALL {
            ui.selectable_value(&mut style, option, option.label());
        }
    });
    set_style(style);
    ui.add_space(4.0);

    let mut changed = false;
    match style {
        Style::Wheel => {
            changed |= hue_ring(ui, id, &mut hsv, Inside::Square);
            changed |= gradient_slider(ui, tr("Hue"), &mut hsv.h, 360.0, "°", |t| {
                Hsv { h: t, s: 1.0, v: 1.0 }.color(255)
            });
            changed |= gradient_slider(ui, tr("Saturation"), &mut hsv.s, 100.0, "%", |t| {
                Hsv { s: t, ..hsv }.color(255)
            });
            changed |= gradient_slider(ui, tr("Brightness"), &mut hsv.v, 100.0, "%", |t| {
                Hsv { v: t, ..hsv }.color(255)
            });
        }
        Style::Triangle => changed |= hue_ring(ui, id, &mut hsv, Inside::Triangle),
        Style::Sliders => changed |= hsl_sliders(ui, &mut hsv, color),
        Style::Swatches => {
            if let Some(picked) = swatches(ui) {
                *color = Color { a: color.a, ..picked };
                hsv = Hsv::of(picked);
            }
        }
    }
    if changed {
        *color = hsv.color(color.a);
    }
    let mut alpha = color.a as f32 / 255.0;
    let shown = *color;
    if gradient_slider(ui, tr("Opacity"), &mut alpha, 100.0, "%", |t| Color {
        a: (t * 255.0).round() as u8,
        ..shown
    }) {
        color.a = (alpha * 255.0).round() as u8;
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

    if style != Style::Swatches
        && let Some(picked) = swatch_row(ui, tr("Recent"), &recent())
    {
        *color = picked;
        hsv = Hsv::of(picked);
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

/// The hue ring and, inside it, a square of saturation (across) and
/// brightness (up) or a triangle from the pure hue to white and black.
/// Returns true when `hsv` changed.
fn hue_ring(ui: &mut egui::Ui, id: egui::Id, hsv: &mut Hsv, inside: Inside) -> bool {
    // As wide as there's room for, within reason.
    let size = ui.available_width().clamp(140.0, WHEEL);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let centre = rect.center();
    let outer = size / 2.0;
    let inner = outer - RING;
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

    // The square fits inside the ring with a little room; the triangle's
    // corners touch a circle just inside it, the pure hue's pointing at
    // its place on the ring, then white and black round clockwise.
    let half = inner * std::f32::consts::FRAC_1_SQRT_2 - 4.0;
    let square = Rect::from_center_size(centre, Vec2::splat(half * 2.0));
    let corner = |turn: f32| {
        let (sin, cos) = (angle_of(hsv.h) + turn).sin_cos();
        centre + vec2(cos, sin) * (inner - 4.0)
    };
    let (hue_at, white_at, black_at) = (corner(0.0), corner(TAU / 3.0), corner(2.0 * TAU / 3.0));
    let marker_at = match inside {
        Inside::Square => {
            // As a grid, so every point is its exact colour.
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
            square.min + vec2(hsv.s, 1.0 - hsv.v) * square.size()
        }
        Inside::Triangle => {
            // One triangle shaded between its corners is exact: a point a
            // part of the way to each corner is that much of each colour,
            // which is the hue at saturation a/(a+b), brightness a+b.
            let mut triangle = Mesh::default();
            let full = to_color32(Hsv { h: hsv.h, s: 1.0, v: 1.0 }.color(255));
            for (pos, color) in [(hue_at, full), (white_at, Color32::WHITE), (black_at, Color32::BLACK)] {
                triangle.vertices.push(Vertex { pos, uv: WHITE_UV, color });
            }
            triangle.add_triangle(0, 1, 2);
            painter.add(triangle);
            let (a, b) = (hsv.s * hsv.v, hsv.v - hsv.s * hsv.v);
            let c = 1.0 - a - b;
            pos2(0.0, 0.0) + hue_at.to_vec2() * a + white_at.to_vec2() * b + black_at.to_vec2() * c
        }
    };

    // Markers: rings in black and white, so they show on any colour.
    let marker = |at: Pos2, radius: f32| {
        painter.circle_stroke(at, radius, Stroke::new(2.5, Color32::BLACK));
        painter.circle_stroke(at, radius, Stroke::new(1.2, Color32::WHITE));
    };
    let (sin, cos) = angle_of(hsv.h).sin_cos();
    marker(centre + vec2(cos, sin) * (inner + RING / 2.0), RING / 2.0 - 2.0);
    marker(marker_at, 5.0);

    // A press on the ring turns the hue; inside, it picks the rest. The
    // drag keeps to what it started on, however far it strays.
    let dragging_id = id.with("wheel drag");
    if response.drag_started() || response.clicked() {
        let at = response.interact_pointer_pos().unwrap_or(centre);
        let on_ring = (at - centre).length() >= inner - 2.0;
        ui.data_mut(|d| d.insert_temp(dragging_id, if on_ring { Dragging::Hue } else { Dragging::Inside }));
    }
    let dragging: Option<Dragging> = ui.data(|d| d.get_temp(dragging_id));
    let mut changed = false;
    if let (Some(dragging), Some(at)) = (dragging, response.interact_pointer_pos())
        && (response.dragged() || response.clicked() || response.drag_started())
    {
        match (dragging, inside) {
            (Dragging::Hue, _) => {
                let offset = at - centre;
                hsv.h = ((offset.y.atan2(offset.x) + TAU / 4.0) / TAU).rem_euclid(1.0);
            }
            (Dragging::Inside, Inside::Square) => {
                let t = (at - square.min) / square.size();
                hsv.s = t.x.clamp(0.0, 1.0);
                hsv.v = (1.0 - t.y).clamp(0.0, 1.0);
            }
            (Dragging::Inside, Inside::Triangle) => {
                let (a, b, _) = barycentric(at, hue_at, white_at, black_at);
                hsv.v = (a + b).clamp(0.0, 1.0);
                // At black every saturation is the same; keep the last.
                if hsv.v > 1e-4 {
                    hsv.s = (a / (a + b)).clamp(0.0, 1.0);
                }
            }
        }
        changed = true;
    }
    if response.drag_stopped() || response.clicked() {
        ui.data_mut(|d| d.remove::<Dragging>(dragging_id));
    }
    changed
}

/// How much of each corner of the triangle `a`, `b`, `c` the point `p`
/// is, kept inside it: three parts, none below nothing, adding up to one.
fn barycentric(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> (f32, f32, f32) {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let d = v0.x * v1.y - v1.x * v0.y;
    if d.abs() < 1e-6 {
        return (1.0, 0.0, 0.0);
    }
    let wb = (v2.x * v1.y - v1.x * v2.y) / d;
    let wc = (v0.x * v2.y - v2.x * v0.y) / d;
    let wa = 1.0 - wb - wc;
    // Outside, the nearest it can be: the parts below nothing let go, the
    // rest shared out again.
    let (wa, wb, wc) = (wa.max(0.0), wb.max(0.0), wc.max(0.0));
    let sum = wa + wb + wc;
    (wa / sum, wb / sum, wc / sum)
}

/// A slider drawn over the colours it runs through (`shade` gives the one
/// at each place along it, 0 to 1), as Photoshop's Color panel's are, with
/// the value as a number beside it, shown `0..range` with `unit`.
fn gradient_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: f32,
    unit: &str,
    shade: impl Fn(f32) -> Color,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (label_width, number_width) = (72.0, 56.0);
        ui.add_sized([label_width, 18.0], egui::Label::new(label).halign(egui::Align::LEFT));
        let width = (ui.available_width() - number_width - 8.0).max(60.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
        // A checkerboard first, so a see-through end shows as one.
        paint_swatch(ui.painter(), rect, Color::rgba(0, 0, 0, 0));
        let mut bar = Mesh::default();
        const STEPS: u32 = 24;
        for i in 0..=STEPS {
            let t = i as f32 / STEPS as f32;
            let x = rect.left() + t * rect.width();
            let color = to_color32(shade(t));
            bar.vertices.push(Vertex { pos: pos2(x, rect.top()), uv: WHITE_UV, color });
            bar.vertices.push(Vertex { pos: pos2(x, rect.bottom()), uv: WHITE_UV, color });
            if i > 0 {
                let at = i * 2;
                bar.add_triangle(at - 2, at - 1, at);
                bar.add_triangle(at - 1, at + 1, at);
            }
        }
        ui.painter().add(bar);
        ui.painter().rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, Color32::from_black_alpha(110)),
            egui::StrokeKind::Inside,
        );
        let x = rect.left() + value.clamp(0.0, 1.0) * rect.width();
        let knob = Rect::from_center_size(pos2(x, rect.center().y), vec2(6.0, rect.height() + 6.0));
        ui.painter().rect_filled(knob, 2.0, Color32::WHITE);
        ui.painter().rect_stroke(knob, 2.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
        if let Some(at) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            *value = ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            changed = true;
        }
        let mut shown = *value * range;
        if ui
            .add_sized(
                [number_width, 18.0],
                egui::DragValue::new(&mut shown).range(0.0..=range).max_decimals(0).suffix(unit),
            )
            .changed()
        {
            *value = shown / range;
            changed = true;
        }
    });
    changed
}

/// The HSL style: hue, saturation and lightness, as CSS and Photoshop's
/// HSL sliders have them, then red, green and blue as numbers. Returns
/// true when `hsv` changed (`color` itself, for red, green and blue).
fn hsl_sliders(ui: &mut egui::Ui, hsv: &mut Hsv, color: &mut Color) -> bool {
    let (mut h, mut s, mut l) = hsv.hsl();
    let mut changed =
        gradient_slider(ui, tr("Hue"), &mut h, 360.0, "°", |t| Hsv::from_hsl(t, 1.0, 0.5).color(255));
    changed |=
        gradient_slider(ui, tr("Saturation"), &mut s, 100.0, "%", |t| Hsv::from_hsl(h, t, l).color(255));
    changed |=
        gradient_slider(ui, tr("Lightness"), &mut l, 100.0, "%", |t| Hsv::from_hsl(h, s, t).color(255));
    if changed {
        *hsv = Hsv::from_hsl(h, s, l);
    }
    ui.horizontal(|ui| {
        for (label, channel) in [("R", 0), ("G", 1), ("B", 2)] {
            ui.label(label);
            let value = match channel {
                0 => &mut color.r,
                1 => &mut color.g,
                _ => &mut color.b,
            };
            if ui.add(egui::DragValue::new(value).range(0..=255)).changed() {
                // Red, green and blue set the colour itself; the rest follows.
                *hsv = Hsv::of(*color);
            }
        }
    });
    changed
}

/// The Swatches style: the project's swatches, a standard set (greys, then
/// twelve hues from dark to light), and the recent colours. Returns the
/// colour clicked, if one was.
fn swatches(ui: &mut egui::Ui) -> Option<Color> {
    let project: Vec<Color> = PROJECT.lock().expect("not poisoned").iter().map(|(_, c)| *c).collect();
    let mut picked = None;
    if !project.is_empty() {
        picked = picked.or(swatch_row(ui, tr("Project"), &project));
    }
    picked = picked.or(swatch_row(ui, tr("Basic"), &basic_colours()));
    picked.or(swatch_row(ui, tr("Recent"), &recent()))
}

/// The standard swatches: twelve greys from black to white, then twelve
/// hues round the wheel at five lightnesses each, a row a lightness.
fn basic_colours() -> Vec<Color> {
    let greys = (0..12).map(|i| {
        let v = (i as f32 / 11.0 * 255.0).round() as u8;
        Color::rgb(v, v, v)
    });
    let hues = [0.2, 0.35, 0.5, 0.65, 0.8]
        .into_iter()
        .flat_map(|l| (0..12).map(move |i| Hsv::from_hsl(i as f32 / 12.0, 0.85, l).color(255)));
    greys.chain(hues).collect()
}

/// How many swatches a row: the basic set's twelve hues line up in
/// columns, a row a lightness.
const SWATCH_COLUMNS: usize = 12;

/// A labelled grid of swatches, twelve across, sized to the room there is;
/// returns the one clicked.
fn swatch_row(ui: &mut egui::Ui, label: &str, colors: &[Color]) -> Option<Color> {
    if colors.is_empty() {
        return None;
    }
    let mut picked = None;
    ui.label(label);
    let gap = 3.0;
    let columns = SWATCH_COLUMNS as f32;
    let cell = ((ui.available_width() - gap * (columns - 1.0)) / columns).clamp(10.0, 22.0);
    for row in colors.chunks(SWATCH_COLUMNS) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, gap);
            for &color in row {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                paint_swatch(ui.painter(), rect, color);
                if response.on_hover_text(color.to_string()).clicked() {
                    picked = Some(color);
                }
            }
        });
    }
    picked
}

/// The Colour tab: the current tool's colours to choose between (a shape's
/// fill and stroke, or the paint colour and the second one), then the
/// picker for the one chosen, the same one the toolbar's well shows in
/// front (see `colour_well.rs`).
pub fn panel(ui: &mut egui::Ui, editor: &mut Editor) {
    let choices = editor.colour_choices();
    let active = editor.active_colour();
    ui.horizontal(|ui| {
        for &choice in choices {
            let (rect, response) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
            match editor.colour(choice) {
                Some(color) => paint_swatch(ui.painter(), rect, color),
                None => {
                    ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
                    ui.painter().line_segment(
                        [rect.left_bottom(), rect.right_top()],
                        Stroke::new(2.0, Color32::from_rgb(0xe0, 0x30, 0x30)),
                    );
                }
            }
            let label = ui.selectable_label(active == choice, choice.label());
            if label.clicked() || response.clicked() {
                editor.choose_colour(choice);
            }
            ui.add_space(6.0);
        }
        if choices.len() > 1 && crate::colour_well::swap_button(ui).clicked() {
            editor.swap_colours();
        }
    });
    let active = editor.active_colour();
    let current = editor.colour(active);
    if active.can_be_none() {
        let mut none = current.is_none();
        if ui.checkbox(&mut none, tr("None")).on_hover_text(tr("Draw new shapes without it")).changed() {
            // Coming back, it comes back as black, the picker below then
            // changing it.
            editor.set_colour(active, if none { None } else { Some(Color::BLACK) });
        }
    }
    ui.add_space(4.0);
    // A shape without the colour shows the picker at black: picking in it
    // turns the colour on. Only a change is written, so looking at an
    // unused fill doesn't give new shapes one.
    let mut color = current.unwrap_or(Color::BLACK);
    if picker(ui, &mut color) {
        editor.set_colour(active, Some(color));
    }
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
    fn hsl_round_trips_through_hsv() {
        for (h, s, l) in [(0.1, 0.8, 0.3), (0.6, 0.4, 0.7), (0.9, 1.0, 0.5), (0.0, 0.0, 0.25)] {
            let (h2, s2, l2) = Hsv::from_hsl(h, s, l).hsl();
            assert!((h - h2).abs() < 1e-4 && (s - s2).abs() < 1e-4 && (l - l2).abs() < 1e-4, "{h} {s} {l}");
        }
        // Lightness half way at full saturation is the pure hue.
        assert_eq!(Hsv::from_hsl(0.0, 1.0, 0.5).color(255), Color::rgb(255, 0, 0));
    }

    #[test]
    fn the_triangle_reads_saturation_and_brightness_from_where_its_pressed() {
        let (hue, white, black) = (pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(50.0, 80.0));
        assert_eq!(barycentric(hue, hue, white, black), (1.0, 0.0, 0.0));
        let (a, b, c) = barycentric(pos2(50.0, 0.0), hue, white, black);
        assert!((a - 0.5).abs() < 1e-4 && (b - 0.5).abs() < 1e-4 && c.abs() < 1e-4);
        // Outside, it keeps to the triangle.
        let (a, b, c) = barycentric(pos2(50.0, -40.0), hue, white, black);
        assert!(a >= 0.0 && b >= 0.0 && c >= 0.0 && (a + b + c - 1.0).abs() < 1e-4);
    }

    #[test]
    fn the_basic_swatches_run_from_black_to_white_then_round_the_hues() {
        let basic = basic_colours();
        assert_eq!(basic.len(), 72);
        assert_eq!((basic[0], basic[11]), (Color::BLACK, Color::WHITE));
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
