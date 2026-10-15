//! The vertical tool strip on the left, plus the colors new shapes get.
//!
//! Tool icons are drawn with vector shapes instead of an icon font. That
//! keeps them crisp at any display scale and avoids depending on which
//! glyphs a font happens to include.

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::convert::color_button;
use crate::editor::{Editor, Tool};
use crate::shortcuts;

const BUTTON_SIZE: f32 = 36.0;

pub fn show(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        for tool in Tool::ALL {
            let key = shortcuts::tool_key(tool).name();
            let response =
                tool_button(ui, tool, editor.tool == tool).on_hover_text(format!("{} ({key})", tool.name()));
            if response.clicked() {
                editor.tool = tool;
            }
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(4.0);

        // Colors for new shapes. Pencil strokes have their own color, since a
        // filled pencil line rarely makes sense.
        if editor.tool == Tool::Pencil {
            ui.weak("Line");
            color_button(ui, &mut editor.pencil_stroke.color);
            ui.add(egui::DragValue::new(&mut editor.pencil_stroke.width).range(0.5..=100.0).speed(0.1))
                .on_hover_text("Line width");
        } else {
            ui.weak("Fill");
            swatch(ui, &mut editor.draw_style.fill);
            ui.weak("Stroke");
            let mut stroke_color = editor.draw_style.stroke.map(|s| s.color);
            if swatch(ui, &mut stroke_color) {
                editor.draw_style.stroke = stroke_color.map(|color| inkframe_core::Stroke {
                    color,
                    width: editor.draw_style.stroke.map_or(2.0, |s| s.width),
                });
            }
        }
    });
}

/// A color button with a small "none" toggle underneath. Returns true on change.
fn swatch(ui: &mut egui::Ui, color: &mut Option<inkframe_core::Color>) -> bool {
    let mut changed = false;
    if let Some(c) = color {
        changed |= color_button(ui, c);
    }
    let mut none = color.is_none();
    if ui.toggle_value(&mut none, "none").on_hover_text("No color").changed() {
        *color = if none { None } else { Some(inkframe_core::Color::BLACK) };
        changed = true;
    }
    changed
}

fn tool_button(ui: &mut egui::Ui, tool: Tool, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_SIZE), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    draw_icon(ui.painter(), tool, icon_rect, Stroke::new(1.5, visuals.fg_stroke.color));
    response
}

/// The pointer-arrow shape shared by the two selection tools.
fn arrow_outline(at: impl Fn(f32, f32) -> Pos2) -> Vec<Pos2> {
    [(0.2, 0.0), (0.2, 0.85), (0.42, 0.65), (0.58, 1.0), (0.72, 0.93), (0.56, 0.6), (0.85, 0.6)]
        .iter()
        .map(|&(x, y)| at(x, y))
        .collect()
}

fn draw_icon(painter: &Painter, tool: Tool, r: Rect, stroke: Stroke) {
    // Map from a 0–1 unit square into the icon's rectangle.
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    match tool {
        Tool::Select => {
            // A solid arrow, like Animate's black Selection tool. egui only
            // fills convex polygons, so the arrow is filled as two convex
            // pieces (head and tail) and then outlined as one.
            let head = [(0.2, 0.0), (0.2, 0.85), (0.85, 0.6)];
            let tail = [(0.42, 0.65), (0.58, 1.0), (0.72, 0.93), (0.56, 0.6)];
            for piece in [&head[..], &tail[..]] {
                let points = piece.iter().map(|&(x, y)| at(x, y)).collect();
                painter.add(egui::Shape::convex_polygon(points, stroke.color, Stroke::NONE));
            }
            painter.add(egui::Shape::closed_line(arrow_outline(at), stroke));
        }
        Tool::Subselect => {
            // A hollow arrow with an anchor square: Animate's white arrow.
            painter.add(egui::Shape::closed_line(arrow_outline(at), stroke));
            let anchor = Rect::from_center_size(at(0.9, 0.9), Vec2::splat(5.0));
            painter.rect_filled(anchor, 0.0, stroke.color);
        }
        Tool::Pen => {
            // A fountain-pen nib, point down, with its slit and breather hole.
            let nib = [(0.5, 1.0), (0.18, 0.5), (0.32, 0.12), (0.68, 0.12), (0.82, 0.5)];
            let points: Vec<Pos2> = nib.iter().map(|&(x, y)| at(x, y)).collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            painter.line_segment([at(0.5, 1.0), at(0.5, 0.55)], stroke);
            painter.circle_stroke(at(0.5, 0.48), 1.5, stroke);
            painter.line_segment([at(0.25, 0.0), at(0.75, 0.0)], stroke);
        }
        Tool::Rectangle => {
            painter.rect_stroke(r.shrink2(vec2(0.0, 3.0)), 1.0, stroke, StrokeKind::Middle);
        }
        Tool::Ellipse => {
            painter.add(egui::Shape::ellipse_stroke(
                r.center(),
                vec2(r.width() / 2.0, r.height() / 2.0 - 2.0),
                stroke,
            ));
        }
        Tool::Pencil => {
            // A pencil drawn diagonally: body outline plus the sharpened tip.
            let body = [(0.75, 0.0), (1.0, 0.25), (0.3, 0.95), (0.05, 0.7)];
            let points: Vec<Pos2> = body.iter().map(|&(x, y)| at(x, y)).collect();
            painter.add(egui::Shape::closed_line(points, stroke));
            painter.line_segment([at(0.05, 0.7), at(0.0, 1.0)], stroke);
            painter.line_segment([at(0.0, 1.0), at(0.3, 0.95)], stroke);
            painter.line_segment([at(0.6, 0.15), at(0.85, 0.4)], Stroke::new(1.0, Color32::from_gray(140)));
        }
        Tool::Bone => {
            // Two bones meeting at a joint, like an arm bent at the elbow.
            let (shoulder, elbow, hand) = (at(0.15, 0.2), at(0.45, 0.7), at(0.9, 0.35));
            painter.line_segment([shoulder, elbow], stroke);
            painter.line_segment([elbow, hand], stroke);
            for joint in [shoulder, elbow] {
                painter.circle_stroke(joint, 2.5, stroke);
            }
            painter.circle_filled(hand, 2.5, stroke.color);
        }
        Tool::Brush => {
            // A round brush, handle up and to the right, its bristles
            // coming to a point at the bottom left.
            painter.line_segment(
                [at(0.95, 0.05), at(0.55, 0.45)],
                Stroke::new(stroke.width + 1.0, stroke.color),
            );
            let ferrule = [(0.6, 0.35), (0.65, 0.4), (0.45, 0.6), (0.4, 0.55)];
            painter.add(egui::Shape::closed_line(ferrule.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
            let bristles = [(0.4, 0.55), (0.45, 0.6), (0.3, 0.85), (0.05, 0.95), (0.15, 0.7)];
            painter.add(egui::Shape::convex_polygon(
                bristles.iter().map(|&(x, y)| at(x, y)).collect(),
                stroke.color,
                Stroke::NONE,
            ));
        }
        Tool::Eraser => {
            // A block eraser on its side, the rubbing end shaded.
            let block = [(0.05, 0.65), (0.5, 0.2), (0.95, 0.55), (0.5, 1.0)];
            painter.add(egui::Shape::closed_line(block.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
            let end = [(0.05, 0.65), (0.27, 0.43), (0.72, 0.78), (0.5, 1.0)];
            painter.add(egui::Shape::convex_polygon(
                end.iter().map(|&(x, y)| at(x, y)).collect(),
                stroke.color,
                Stroke::NONE,
            ));
        }
        Tool::Eyedropper => {
            // A pipette: a squeeze bulb at the top right, a tube down to a
            // fine tip at the bottom left.
            painter.circle_filled(at(0.8, 0.2), r.width() * 0.16, stroke.color);
            painter.line_segment(
                [at(0.72, 0.28), at(0.15, 0.85)],
                Stroke::new(stroke.width + 1.5, stroke.color),
            );
            painter.line_segment([at(0.15, 0.85), at(0.02, 0.98)], stroke);
        }
        Tool::Text => {
            // A capital T with small serifs, as in most apps' text tools.
            painter.line_segment([at(0.1, 0.05), at(0.9, 0.05)], stroke);
            painter.line_segment([at(0.5, 0.05), at(0.5, 0.95)], stroke);
            painter.line_segment([at(0.3, 0.95), at(0.7, 0.95)], stroke);
            painter.line_segment([at(0.1, 0.05), at(0.1, 0.2)], stroke);
            painter.line_segment([at(0.9, 0.05), at(0.9, 0.2)], stroke);
        }
    }
}
