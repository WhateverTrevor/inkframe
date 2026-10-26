//! The vertical tool strip on the left, plus the colors new shapes get.
//!
//! Tool icons are drawn with vector shapes instead of an icon font. That
//! keeps them crisp at any display scale and avoids depending on which
//! glyphs a font happens to include.

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use crate::convert::color_button;
use crate::editor::{Editor, Tool};
use crate::i18n::tr;
use crate::icons::{Icon, IconButton};
use crate::shortcuts;

const BUTTON_SIZE: f32 = 36.0;
/// Room below the tools for the colours.
const COLOURS_HEIGHT: f32 = 190.0;

/// How many columns of tools to show in `space`: one, or two when one
/// column is too tall for it and there's room across, as Photoshop's
/// toolbar does. Otherwise the area scrolls.
fn columns(space: Vec2) -> usize {
    let one_column = Tool::GROUPS.len() as f32 * (BUTTON_SIZE + 4.0) + COLOURS_HEIGHT;
    if one_column > space.y && space.x >= BUTTON_SIZE * 2.0 + 16.0 { 2 } else { 1 }
}

pub fn show(ui: &mut egui::Ui, editor: &mut Editor) {
    let columns = columns(ui.available_size());
    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        for row in Tool::GROUPS.chunks(columns) {
            ui.horizontal(|ui| {
                // Centred: a lone button in a two-column row sits in the
                // middle rather than at the left.
                let used =
                    row.len() as f32 * BUTTON_SIZE + (row.len() - 1) as f32 * ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - used) / 2.0).max(0.0));
                for group in row {
                    group_button(ui, editor, group);
                }
            });
        }
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(4.0);

        // Colors for new shapes. Pencil strokes have their own color, since a
        // filled pencil line rarely makes sense.
        if editor.tool == Tool::Pencil {
            ui.weak(tr("Line"));
            color_button(ui, &mut editor.pencil_stroke.color);
            ui.add(egui::DragValue::new(&mut editor.pencil_stroke.width).range(0.5..=100.0).speed(0.1))
                .on_hover_text(tr("Line width"));
        } else {
            ui.weak(tr("Fill"));
            swatch(ui, &mut editor.draw_style.fill);
            ui.weak(tr("Stroke"));
            let mut stroke_color = editor.draw_style.stroke.map(|s| s.color);
            if swatch(ui, &mut stroke_color) {
                // A new colour keeps the line's width, ends and dashes.
                let previous = editor.draw_style.stroke;
                editor.draw_style.stroke = stroke_color.map(|color| inkframe_core::Stroke {
                    color,
                    ..previous.unwrap_or(inkframe_core::Stroke::new(color, 2.0))
                });
            }
        }
    });
}

/// One toolbar slot: the tool it shows, and, for a slot holding several,
/// a right-click menu of the others.
fn group_button(ui: &mut egui::Ui, editor: &mut Editor, group: &'static [Tool]) {
    let shown = if group.contains(&editor.tool) { editor.tool } else { editor.slot_choice(group) };
    let key = shortcuts::tool_key(shown).name();
    // Its name and key, what it does, and how to reach the others in the slot.
    let others = if group.len() > 1 {
        let names: Vec<&str> = group.iter().filter(|&&t| t != shown).map(|t| t.label()).collect();
        format!(
            "\n\n{}",
            crate::i18n::tr_args(
                "Also here: {tools} (Shift+{key}, or right-click)",
                &[("tools", &names.join(", ")), ("key", key)]
            )
        )
    } else {
        String::new()
    };
    let hint = format!("{}   {key}\n{}{others}", shown.label(), shown.tip());
    let response = tool_button(ui, shown, group.contains(&editor.tool), group.len() > 1).on_hover_text(hint);
    if response.clicked() {
        editor.use_tool(shown);
    }
    if group.len() > 1 {
        response.context_menu(|ui| {
            for &tool in group {
                if ui.selectable_label(editor.tool == tool, tool.label()).clicked() {
                    editor.use_tool(tool);
                    ui.close();
                }
            }
        });
    }
}

/// A color button and, beside it, a button to switch to no colour. With
/// no colour, the swatch shows Animate's red slash; clicking it brings a
/// colour back. Returns true on change.
fn swatch(ui: &mut egui::Ui, color: &mut Option<inkframe_core::Color>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let size = ui.spacing().interact_size;
        // Centred under the label, the pair taking a swatch and a button.
        ui.add_space(((ui.available_width() - size.x - 26.0) / 2.0).max(0.0));
        match color {
            Some(c) => changed |= color_button(ui, c),
            None => {
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_top()],
                    Stroke::new(2.0, Color32::from_rgb(0xe0, 0x30, 0x30)),
                );
                if response.on_hover_text(tr("No colour. Click to use one")).clicked() {
                    *color = Some(inkframe_core::Color::BLACK);
                    changed = true;
                }
            }
        }
        let none = IconButton::new(Icon::NoColor, tr("No colour")).selected(color.is_none());
        if ui.add(none).clicked() && color.is_some() {
            *color = None;
            changed = true;
        }
    });
    changed
}

/// A toolbar button. `grouped` adds the small corner triangle that marks a
/// slot holding more than one tool, as Photoshop's do.
fn tool_button(ui: &mut egui::Ui, tool: Tool, selected: bool, grouped: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_SIZE), Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    let color = visuals.fg_stroke.color;
    draw_icon(ui.painter(), tool, icon_rect, Stroke::new(1.5, color));
    if grouped {
        let corner = rect.right_bottom() - vec2(4.0, 4.0);
        let triangle = vec![corner, corner - vec2(5.0, 0.0), corner - vec2(0.0, 5.0)];
        ui.painter().add(egui::Shape::convex_polygon(triangle, color.gamma_multiply(0.7), Stroke::NONE));
    }
    response
}

/// A dashed line: marching ants, for the marquee icons.
fn dashed(painter: &Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let dashes = 4;
    for i in 0..dashes {
        let (a, b) = (i as f32 / dashes as f32, (i as f32 + 0.6) / dashes as f32);
        painter.line_segment([from + (to - from) * a, from + (to - from) * b], stroke);
    }
}

/// The pointer-arrow shape shared by the two selection tools.
fn arrow_outline(at: impl Fn(f32, f32) -> Pos2) -> Vec<Pos2> {
    [(0.2, 0.0), (0.2, 0.85), (0.42, 0.65), (0.58, 1.0), (0.72, 0.93), (0.56, 0.6), (0.85, 0.6)]
        .iter()
        .map(|&(x, y)| at(x, y))
        .collect()
}

pub(crate) fn draw_icon(painter: &Painter, tool: Tool, r: Rect, stroke: Stroke) {
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
        Tool::Width => {
            // A line swelling in the middle, with a handle pulling it out.
            let outline = [
                (0.0, 0.62),
                (0.3, 0.5),
                (0.5, 0.42),
                (0.7, 0.5),
                (1.0, 0.62),
                (0.7, 0.72),
                (0.5, 0.78),
                (0.3, 0.72),
            ];
            let points: Vec<Pos2> = outline.iter().map(|&(x, y)| at(x, y)).collect();
            painter.add(egui::Shape::convex_polygon(points, stroke.color, Stroke::NONE));
            painter.line_segment([at(0.5, 0.42), at(0.5, 0.05)], stroke);
            painter.circle_filled(at(0.5, 0.05), 2.5, stroke.color);
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
        Tool::Camera => {
            // A film camera from the side: body, lens and two reels.
            let body = Rect::from_min_max(at(0.0, 0.42), at(0.7, 0.92));
            painter.rect_stroke(body, 2.0, stroke, StrokeKind::Middle);
            painter.add(egui::Shape::closed_line(
                vec![at(0.7, 0.55), at(1.0, 0.42), at(1.0, 0.92), at(0.7, 0.8)],
                stroke,
            ));
            painter.circle_stroke(at(0.18, 0.2), r.width() * 0.16, stroke);
            painter.circle_stroke(at(0.52, 0.2), r.width() * 0.16, stroke);
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
        Tool::AddBone => {
            // A bone with a plus beside it.
            let (start, end) = (at(0.1, 0.9), at(0.65, 0.35));
            let side = (end - start).rot90().normalized() * r.width() * 0.08;
            let quarter = start + (end - start) * 0.25;
            painter.add(egui::Shape::closed_line(vec![start, quarter + side, end, quarter - side], stroke));
            painter.line_segment([at(0.8, 0.0), at(0.8, 0.3)], stroke);
            painter.line_segment([at(0.65, 0.15), at(0.95, 0.15)], stroke);
        }
        Tool::Mixer => {
            // The brush, over a drop of paint being mixed in.
            painter
                .line_segment([at(0.95, 0.05), at(0.6, 0.4)], Stroke::new(stroke.width + 1.0, stroke.color));
            let bristles = [(0.52, 0.42), (0.6, 0.5), (0.42, 0.68), (0.32, 0.6)];
            painter.add(egui::Shape::convex_polygon(
                bristles.iter().map(|&(x, y)| at(x, y)).collect(),
                stroke.color,
                Stroke::NONE,
            ));
            painter.circle_stroke(at(0.25, 0.8), r.width() * 0.17, stroke);
            painter.circle_filled(at(0.25, 0.8), r.width() * 0.08, stroke.color.gamma_multiply(0.6));
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
        Tool::PaintBucket => {
            // A bucket tipped to the left, paint dripping from its lip.
            let bucket = [(0.2, 0.35), (0.6, 0.1), (0.95, 0.55), (0.55, 0.85)];
            painter.add(egui::Shape::closed_line(bucket.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
            painter.line_segment([at(0.4, 0.22), at(0.75, 0.7)], stroke);
            painter.circle_filled(at(0.12, 0.7), r.width() * 0.09, stroke.color);
            painter.line_segment([at(0.2, 0.35), at(0.12, 0.62)], stroke);
        }
        Tool::Gradient => {
            // A swatch fading from solid on the left to clear on the right.
            let strips = 8;
            for i in 0..strips {
                let x0 = i as f32 / strips as f32;
                let strip = Rect::from_min_max(at(x0, 0.2), at(x0 + 1.0 / strips as f32, 0.8));
                let alpha = 1.0 - x0;
                painter.rect_filled(strip, 0.0, stroke.color.gamma_multiply(alpha));
            }
            painter.rect_stroke(
                Rect::from_min_max(at(0.0, 0.2), at(1.0, 0.8)),
                1.0,
                stroke,
                StrokeKind::Middle,
            );
        }
        Tool::Smudge => {
            // A blob of paint dragged out into a tapering smear.
            painter.circle_filled(at(0.25, 0.7), r.width() * 0.2, stroke.color);
            let smear = [(0.25, 0.5), (0.95, 0.1), (0.4, 0.88)];
            painter.add(egui::Shape::convex_polygon(
                smear.iter().map(|&(x, y)| at(x, y)).collect(),
                stroke.color.gamma_multiply(0.6),
                Stroke::NONE,
            ));
        }
        Tool::Blur => {
            // A water drop.
            let drop: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0 * std::f32::consts::TAU;
                    let bulge = 0.32 * (1.0 - 0.5 * (t / 2.0).cos().powi(8));
                    at(0.5 + t.sin() * bulge, 0.62 - t.cos() * 0.38)
                })
                .collect();
            painter.add(egui::Shape::closed_line(drop, stroke));
        }
        Tool::Sharpen => {
            // A sharp point.
            let point = [(0.5, 0.0), (0.82, 1.0), (0.18, 1.0)];
            painter.add(egui::Shape::closed_line(point.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
        }
        Tool::Dodge | Tool::Burn => {
            // Darkroom tools on a stick: Dodge a hollow disc, Burn a solid one.
            painter.line_segment([at(0.55, 0.45), at(0.95, 0.95)], stroke);
            let (center, radius) = (at(0.38, 0.3), r.width() * 0.3);
            if tool == Tool::Burn {
                painter.circle_filled(center, radius, stroke.color);
            } else {
                painter.circle_stroke(center, radius, stroke);
            }
        }
        Tool::CloneStamp => {
            // A rubber stamp: a knob, a neck, and a wide base.
            painter.circle_stroke(at(0.5, 0.18), r.width() * 0.16, stroke);
            painter
                .line_segment([at(0.5, 0.34), at(0.5, 0.6)], Stroke::new(stroke.width + 1.0, stroke.color));
            painter.rect_filled(Rect::from_min_max(at(0.1, 0.6), at(0.9, 0.78)), 1.0, stroke.color);
            painter.line_segment([at(0.15, 0.92), at(0.85, 0.92)], stroke);
        }
        Tool::RectMarquee => {
            let corners = [(0.05, 0.15), (0.95, 0.15), (0.95, 0.85), (0.05, 0.85)].map(|(x, y)| at(x, y));
            for i in 0..4 {
                dashed(painter, corners[i], corners[(i + 1) % 4], stroke);
            }
        }
        Tool::EllipseMarquee => {
            let points: Vec<Pos2> = (0..=32)
                .map(|i| {
                    let t = i as f32 / 32.0 * std::f32::consts::TAU;
                    at(0.5 + t.cos() * 0.45, 0.5 + t.sin() * 0.36)
                })
                .collect();
            for (i, pair) in points.windows(2).enumerate() {
                if i % 2 == 0 {
                    painter.line_segment([pair[0], pair[1]], stroke);
                }
            }
        }
        Tool::Lasso | Tool::PolygonLasso => {
            // A loop of rope with its tail hanging below.
            let points: Vec<Pos2> = if tool == Tool::Lasso {
                (0..=24)
                    .map(|i| {
                        let t = i as f32 / 24.0 * std::f32::consts::TAU;
                        at(0.55 + t.cos() * 0.4, 0.38 + t.sin() * 0.28)
                    })
                    .collect()
            } else {
                [(0.15, 0.3), (0.55, 0.08), (0.95, 0.35), (0.6, 0.66), (0.25, 0.6), (0.15, 0.3)]
                    .map(|(x, y)| at(x, y))
                    .to_vec()
            };
            painter.add(egui::Shape::line(points, stroke));
            painter.line_segment([at(0.25, 0.6), at(0.15, 0.95)], stroke);
        }
        Tool::MagicWand => {
            // A wand with a sparkle at its tip.
            painter
                .line_segment([at(0.05, 0.95), at(0.6, 0.4)], Stroke::new(stroke.width + 1.0, stroke.color));
            let star = at(0.75, 0.25);
            for (dx, dy) in [(0.22, 0.0), (0.0, 0.22), (0.15, 0.15), (0.15, -0.15)] {
                let d = vec2(dx * r.width(), dy * r.height());
                painter.line_segment([star - d, star + d], stroke);
            }
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
