//! Conversions between Inkframe's core types and egui's.

use inkframe_core::{Color, Point};

pub fn to_point(p: egui::Pos2) -> Point {
    Point::new(p.x, p.y)
}

pub fn to_color32(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// A colour button that edits `color` in place through the colour picker
/// (see `color_picker.rs`), returning true when it changed.
pub fn color_button(ui: &mut egui::Ui, color: &mut Color) -> bool {
    crate::color_picker::color_button(ui, color)
}
