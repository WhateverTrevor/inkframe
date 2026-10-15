//! The small panels Animate keeps beside the stage: Swatches, Align and
//! History. Each is shown in an area of its own (see `layout.rs`).

use egui::{Color32, Sense, Stroke, Vec2, vec2};
use inkframe_core::{Align, Axis, Color};

use crate::convert::to_color32;
use crate::editor::Editor;
use crate::icons::{Icon, IconButton};
use crate::theme;

/// Panel settings that last while the app runs.
#[derive(Default)]
pub struct PanelState {
    /// Whether swatches set the stroke rather than the fill.
    pub stroke: bool,
    /// Whether the Align panel lines layers up with the stage.
    pub to_stage: bool,
}

const CHIP: f32 = 16.0;

/// The swatches every document starts with: greys, then rows of hues from
/// light to dark, much like Animate's default palette.
fn palette() -> Vec<Color> {
    let greys = (0..12).map(|i| {
        let v = (255.0 * i as f32 / 11.0) as u8;
        Color::rgb(v, v, v)
    });
    let hues = [0.0, 30.0, 50.0, 90.0, 140.0, 180.0, 200.0, 220.0, 260.0, 290.0, 320.0, 345.0];
    let rows = [(0.35, 0.95), (0.6, 0.85), (0.75, 0.65), (0.6, 0.4)];
    let colours = rows.into_iter().flat_map(move |(saturation, value)| {
        hues.into_iter().map(move |hue| hsv(hue, saturation + 0.25 * (1.0 - saturation), value))
    });
    greys.chain(colours).collect()
}

fn hsv(hue: f32, saturation: f32, value: f32) -> Color {
    let c = value * saturation;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    let byte = |v: f32| ((v + m) * 255.0).round() as u8;
    Color::rgb(byte(r), byte(g), byte(b))
}

pub fn swatches(ui: &mut egui::Ui, editor: &mut Editor, state: &mut PanelState) {
    // Which colour a click sets, shown as Animate's pair of fill and stroke
    // chips; the paint tools have one colour, so no choice there.
    if editor.tool.paints() {
        ui.weak("Click a swatch to paint with it.");
    } else {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.stroke, false, "Fill");
            ui.selectable_value(&mut state.stroke, true, "Stroke");
        });
    }
    ui.add_space(4.0);

    let mut picked = None;
    chip_grid(ui, &palette(), &mut picked, |_, _| {});

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("DOCUMENT").small().color(theme::TEXT_WEAK));
        let current = current_color(editor, state.stroke);
        if ui.add(IconButton::new(Icon::Plus, "Save the current colour as a swatch")).clicked()
            && let Some(color) = current
            && !editor.doc.swatches.contains(&color)
        {
            editor.doc.swatches.push(color);
        }
    });
    let saved = editor.doc.swatches.clone();
    if saved.is_empty() {
        ui.weak("Colours saved here stay with the document.");
    }
    let mut remove = None;
    chip_grid(ui, &saved, &mut picked, |i, response| {
        response.context_menu(|ui| {
            if ui.button("Remove Swatch").clicked() {
                remove = Some(i);
                ui.close();
            }
        });
    });
    if let Some(i) = remove {
        editor.doc.swatches.remove(i);
    }
    if let Some(color) = picked {
        editor.use_color(color, state.stroke);
    }
}

/// The colour the next swatch would replace: what "+" saves.
fn current_color(editor: &Editor, stroke: bool) -> Option<Color> {
    if editor.tool.paints() {
        Some(editor.paint_color)
    } else if stroke {
        editor.draw_style.stroke.map(|s| s.color)
    } else {
        editor.draw_style.fill
    }
}

/// Colour chips in rows that wrap to the panel's width. `each` gets each
/// chip's response, for a context menu.
fn chip_grid(
    ui: &mut egui::Ui,
    colors: &[Color],
    picked: &mut Option<Color>,
    mut each: impl FnMut(usize, &egui::Response),
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        for (i, &color) in colors.iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(CHIP), Sense::click());
            ui.painter().rect_filled(rect, 2.0, to_color32(color));
            if response.hovered() {
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(1.5, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
            let response = response.on_hover_text(format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b));
            if response.clicked() {
                *picked = Some(color);
            }
            each(i, &response);
        }
    });
}

pub fn align(ui: &mut egui::Ui, editor: &mut Editor, state: &mut PanelState) {
    let count = editor.selected_ids().len();
    ui.checkbox(&mut state.to_stage, "Align to stage")
        .on_hover_text("Line layers up with the stage rather than with each other");
    let to_stage = state.to_stage;
    ui.add_enabled_ui(count > 0, |ui| {
        let row = |ui: &mut egui::Ui, title: &str, buttons: &[(Icon, &str)]| -> Option<usize> {
            ui.label(egui::RichText::new(title).small().color(theme::TEXT_WEAK));
            let mut clicked = None;
            ui.horizontal(|ui| {
                for (i, (icon, hover)) in buttons.iter().enumerate() {
                    if ui.add(IconButton::new(*icon, hover)).clicked() {
                        clicked = Some(i);
                    }
                }
            });
            clicked
        };
        let aligns = [
            (Icon::AlignLeft, "Align left edges"),
            (Icon::AlignCenterX, "Align centres across"),
            (Icon::AlignRight, "Align right edges"),
            (Icon::AlignTop, "Align top edges"),
            (Icon::AlignCenterY, "Align centres down"),
            (Icon::AlignBottom, "Align bottom edges"),
        ];
        if let Some(i) = row(ui, "ALIGN", &aligns) {
            let how =
                [Align::Left, Align::CenterX, Align::Right, Align::Top, Align::CenterY, Align::Bottom][i];
            editor.align(how, to_stage);
        }
        let spread = [
            (Icon::DistributeX, "Space centres evenly across"),
            (Icon::DistributeY, "Space centres evenly down"),
        ];
        if let Some(i) = row(ui, "DISTRIBUTE", &spread) {
            editor.distribute([Axis::Horizontal, Axis::Vertical][i], to_stage);
        }
        let sizes = [
            (Icon::MatchWidth, "Make them as wide as the widest"),
            (Icon::MatchHeight, "Make them as tall as the tallest"),
        ];
        if let Some(i) = row(ui, "MATCH SIZE", &sizes) {
            editor.match_size([Axis::Horizontal, Axis::Vertical][i], to_stage);
        }
    });
    if count == 0 {
        ui.weak("Select layers to line them up. One layer lines up with the stage.");
    }
}

pub fn history(ui: &mut egui::Ui, editor: &mut Editor) {
    let (done, undone): (Vec<String>, Vec<String>) = {
        let (done, undone) = editor.history.labels();
        (done.into_iter().map(str::to_owned).collect(), undone.into_iter().map(str::to_owned).collect())
    };
    ui.weak("Click a step to go back to it.");
    ui.add_space(4.0);
    let mut jump = None;
    ui.spacing_mut().item_spacing.y = 1.0;
    // The start, every step done (the last is where things stand), then the
    // steps undone, greyed, which can still be redone.
    let current = done.len();
    let start = std::iter::once("Start".to_owned());
    for (i, label) in start.chain(done).enumerate() {
        if step(ui, &label, i == current, false).clicked() {
            jump = Some(i as isize - current as isize);
        }
    }
    for (i, label) in undone.iter().enumerate() {
        if step(ui, label, false, true).clicked() {
            jump = Some(i as isize + 1);
        }
    }
    if let Some(steps) = jump.filter(|&s| s != 0) {
        editor.jump_history(steps);
    }
}

/// One line of the History panel.
fn step(ui: &mut egui::Ui, label: &str, current: bool, undone: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    let fill = if current {
        Some(theme::ACCENT_DIM)
    } else if response.hovered() {
        Some(theme::CONTROL)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    let color = if undone { theme::TEXT_WEAK.gamma_multiply(0.7) } else { theme::TEXT };
    ui.painter().text(
        rect.left_center() + vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Body.resolve(ui.style()),
        color,
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    use inkframe_core::{Document, Element, Point, Shape, Style, Transform};

    fn editor_with_square() -> (Editor, inkframe_core::LayerId) {
        let mut doc = Document::default();
        let square = Element {
            shape: Shape::Rect { width: 10.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        let id = doc.add_layer("Square", vec![square], 0, Transform::at(Point::new(100.0, 100.0)));
        let mut editor = Editor::new(doc);
        editor.select(Some(id));
        (editor, id)
    }

    #[test]
    fn a_swatch_recolours_the_selected_shapes_and_new_ones() {
        let (mut editor, id) = editor_with_square();
        let red = Color::rgb(255, 0, 0);
        editor.use_color(red, false);
        assert_eq!(editor.draw_style.fill, Some(red));
        assert_eq!(editor.doc.layer(id).unwrap().elements_at(0)[0].style.fill, Some(red));
        editor.use_color(red, true);
        assert_eq!(editor.doc.layer(id).unwrap().elements_at(0)[0].style.stroke.map(|s| s.color), Some(red));
    }

    #[test]
    fn the_history_panel_jumps_back_and_forward() {
        let (mut editor, id) = editor_with_square();
        editor.commit_history();
        editor.doc.layer_mut(id).unwrap().name = "Box".to_owned();
        editor.commit_history();
        editor.doc.layer_mut(id).unwrap().visible = false;
        editor.commit_history();
        assert_eq!(editor.history.labels().0, ["Rename “Box”", "Hide “Box”"]);
        editor.jump_history(-2);
        assert_eq!(editor.doc.layer(id).unwrap().name, "Square");
        editor.jump_history(2);
        assert!(!editor.doc.layer(id).unwrap().visible);
    }

    #[test]
    fn aligning_one_layer_centres_it_on_the_stage() {
        let (mut editor, id) = editor_with_square();
        editor.align(Align::CenterX, false);
        editor.align(Align::CenterY, false);
        let centre = editor.doc.stage_bounds(id, 0).unwrap().center();
        assert_eq!(centre, Point::new(960.0, 540.0));
    }

    #[test]
    fn the_palette_has_greys_and_rows_of_hues() {
        let palette = palette();
        assert_eq!(palette.len(), 12 + 4 * 12);
        assert_eq!(palette[0], Color::rgb(0, 0, 0));
        assert_eq!(palette[11], Color::rgb(255, 255, 255));
        assert_eq!(hsv(0.0, 1.0, 1.0), Color::rgb(255, 0, 0));
        assert_eq!(hsv(120.0, 1.0, 1.0), Color::rgb(0, 255, 0));
    }
}
