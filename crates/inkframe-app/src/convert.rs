//! Conversions between Inkframe's core types and egui's.

use inkframe_core::{Color, Point};

pub fn to_point(p: egui::Pos2) -> Point {
    Point::new(p.x, p.y)
}

pub fn to_color32(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Shows a color button that edits `color` in place, returning true when it
/// changed. Works with straight alpha so low-opacity colors don't lose
/// precision the way a round trip through egui's premultiplied `Color32` would.
pub fn color_button(ui: &mut egui::Ui, color: &mut Color) -> bool {
    let mut rgba = [color.r, color.g, color.b, color.a];
    if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
        let [r, g, b, a] = rgba;
        *color = Color::rgba(r, g, b, a);
        true
    } else {
        false
    }
}
