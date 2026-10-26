//! The small panels Animate keeps beside the stage: Swatches, Align and
//! History. Each is shown in an area of its own (see `layout.rs`).

use egui::{Color32, Sense, Stroke, Vec2, vec2};
use inkframe_core::{Align, Axis, Color};

use crate::convert::to_color32;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
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
    let (r, g, b) = inkframe_core::hsv_to_rgb(hue / 360.0, saturation, value);
    let byte = |v: f32| (v * 255.0).round() as u8;
    Color::rgb(byte(r), byte(g), byte(b))
}

pub fn swatches(ui: &mut egui::Ui, editor: &mut Editor, state: &mut PanelState) {
    // Which colour a click sets, shown as Animate's pair of fill and stroke
    // chips; the paint tools have one colour, so no choice there.
    if editor.tool.paints() {
        ui.weak(tr("Click a swatch to paint with it."));
    } else {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.stroke, false, tr("Fill"));
            ui.selectable_value(&mut state.stroke, true, tr("Stroke"));
        });
    }
    ui.add_space(4.0);
    let mut picked = None;
    palette_section(ui, editor, state, &mut picked);

    ui.add_space(8.0);
    egui::CollapsingHeader::new(egui::RichText::new(tr("BASIC COLOURS")).small().color(theme::TEXT_WEAK))
        .id_salt("basic_colours")
        .default_open(editor.doc.palette.swatches.is_empty())
        .show(ui, |ui| chip_grid(ui, &palette(), &mut picked, |_, _| {}));
    if let Some(color) = picked {
        editor.use_color(color, state.stroke);
    }
}

/// The document's palette: which version is in use, and each swatch with
/// its name. Clicking a swatch uses its colour; right-clicking edits it,
/// which recolours everything painted with it (see `palette.rs`).
fn palette_section(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    state: &mut PanelState,
    picked: &mut Option<Color>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr("PALETTE")).small().color(theme::TEXT_WEAK));
        let palette = &editor.doc.palette;
        let showing = palette.versions[palette.active].clone();
        let mut switch = None;
        let mut new_version = false;
        egui::ComboBox::from_id_salt("palette_version")
            .selected_text(showing)
            .width(100.0)
            .show_ui(ui, |ui| {
                for (i, name) in palette.versions.iter().enumerate() {
                    if ui.selectable_label(i == palette.active, name).clicked() {
                        switch = Some(i);
                    }
                }
                ui.separator();
                new_version = ui
                    .button(tr("New Version"))
                    .on_hover_text(tr("A copy of this version, to recolour: a night palette, say"))
                    .clicked();
            })
            .response
            .on_hover_text(tr("Switching versions recolours everything painted from the palette"));
        if let Some(version) = switch {
            editor.doc.use_palette_version(version);
        }
        if new_version {
            let name = tr_args(
                "Version {number}",
                &[("number", &(editor.doc.palette.versions.len() + 1).to_string())],
            );
            let version = editor.doc.add_palette_version(name);
            editor.doc.use_palette_version(version);
        }
        let current = current_color(editor, state.stroke);
        if ui.add(IconButton::new(Icon::Plus, tr("Add the current colour to the palette"))).clicked()
            && let Some(color) = current
        {
            let name = tr_args(
                "Colour {number}",
                &[("number", &(editor.doc.palette.swatches.len() + 1).to_string())],
            );
            editor.doc.add_swatch(name, color);
        }
    });

    let count = editor.doc.palette.swatches.len();
    if count == 0 {
        ui.weak(tr(
            "Add colours here to paint with. Change one later and everything painted with it changes too.",
        ));
    }
    let mut remove = None;
    for i in 0..count {
        ui.push_id(("swatch", i), |ui| {
            ui.horizontal(|ui| {
                let Some(color) = editor.doc.palette.color(i) else { return };
                let (rect, chip) = ui.allocate_exact_size(vec2(28.0, CHIP), Sense::click());
                ui.painter().rect_filled(rect, 3.0, to_color32(color));
                if chip.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(1.5, Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
                if chip.clicked() {
                    *picked = Some(color);
                }
                let uses = editor.doc.swatch_uses(i);
                chip.on_hover_text(tr_args(
                    "Use this colour. Right-click to change it ({count} shapes use it)",
                    &[("count", &uses.to_string())],
                ))
                .context_menu(|ui| {
                    // Recolours live as the picker moves.
                    let mut edited = to_color32(color);
                    if egui::color_picker::color_picker_color32(
                        ui,
                        &mut edited,
                        egui::color_picker::Alpha::OnlyBlend,
                    ) {
                        let [r, g, b, a] = edited.to_srgba_unmultiplied();
                        editor.doc.set_swatch_color(i, Color::rgba(r, g, b, a));
                    }
                    ui.separator();
                    if ui.button(tr("Remove Swatch")).on_hover_text(tr("Shapes keep their colour")).clicked()
                    {
                        remove = Some(i);
                        ui.close();
                    }
                });
                let name = &mut editor.doc.palette.swatches[i].name;
                ui.add(
                    egui::TextEdit::singleline(name)
                        .desired_width(ui.available_width())
                        .frame(egui::Frame::NONE),
                );
            });
        });
    }
    if let Some(i) = remove {
        editor.doc.remove_swatch(i);
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
    ui.checkbox(&mut state.to_stage, tr("Align to stage"))
        .on_hover_text(tr("Line layers up with the stage rather than with each other"));
    let to_stage = state.to_stage;
    ui.add_enabled_ui(count > 0, |ui| {
        let row = |ui: &mut egui::Ui, title: &str, buttons: &[(Icon, &str)]| -> Option<usize> {
            ui.label(egui::RichText::new(title.to_uppercase()).small().color(theme::TEXT_WEAK));
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
            (Icon::AlignLeft, tr("Align left edges")),
            (Icon::AlignCenterX, tr("Align centres across")),
            (Icon::AlignRight, tr("Align right edges")),
            (Icon::AlignTop, tr("Align top edges")),
            (Icon::AlignCenterY, tr("Align centres down")),
            (Icon::AlignBottom, tr("Align bottom edges")),
        ];
        if let Some(i) = row(ui, tr("Align"), &aligns) {
            let how =
                [Align::Left, Align::CenterX, Align::Right, Align::Top, Align::CenterY, Align::Bottom][i];
            editor.align(how, to_stage);
        }
        let spread = [
            (Icon::DistributeX, tr("Space centres evenly across")),
            (Icon::DistributeY, tr("Space centres evenly down")),
        ];
        if let Some(i) = row(ui, tr("Distribute"), &spread) {
            editor.distribute([Axis::Horizontal, Axis::Vertical][i], to_stage);
        }
        let sizes = [
            (Icon::MatchWidth, tr("Make them as wide as the widest")),
            (Icon::MatchHeight, tr("Make them as tall as the tallest")),
        ];
        if let Some(i) = row(ui, tr("Match size"), &sizes) {
            editor.match_size([Axis::Horizontal, Axis::Vertical][i], to_stage);
        }
    });
    if count == 0 {
        ui.weak(tr("Select layers to line them up. One layer lines up with the stage."));
    }
}

/// Master controllers (see `controller.rs` in the core): a slider each,
/// its recorded poses, and buttons to record more.
pub fn controllers(ui: &mut egui::Ui, editor: &mut Editor) {
    let frame = editor.frame;
    let mut remove = None;
    for i in 0..editor.doc.controllers.len() {
        ui.push_id(("controller", i), |ui| {
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut editor.doc.controllers[i].name).desired_width(140.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(IconButton::new(Icon::Trash, tr("Remove this controller"))).clicked() {
                        remove = Some(i);
                    }
                });
            });
            let c = &editor.doc.controllers[i];
            let driven = c.layers.len();
            let mut value = c.value;
            let poses: Vec<(String, f32)> = c.poses.iter().map(|p| (p.name.clone(), p.at)).collect();
            // Usable with a single pose too: slide to where the next pose
            // goes, pose the layers, and record it.
            let slider = ui.add(egui::Slider::new(&mut value, 0.0..=1.0).show_value(false));
            if slider.changed() {
                editor.doc.set_controller(i, value, frame);
            }
            // Its poses, to jump to (right-click to forget one).
            let mut forget = None;
            ui.horizontal_wrapped(|ui| {
                for (j, (name, at)) in poses.iter().enumerate() {
                    let chip = ui.small_button(name).on_hover_text(tr_args("At {percent}%. Right-click to forget it", &[("percent", &format!("{:.0}", at * 100.0))]));
                    if chip.clicked() {
                        editor.doc.set_controller(i, *at, frame);
                    }
                    chip.context_menu(|ui| {
                        if ui.button(tr("Forget Pose")).clicked() {
                            forget = Some(j);
                            ui.close();
                        }
                    });
                }
            });
            if let Some(j) = forget {
                editor.doc.controllers[i].poses.remove(j);
            }
            ui.horizontal(|ui| {
                let at = editor.doc.controllers[i].value;
                let hint = tr("Pose the layers on the stage, then record how they look as a pose at this point on the slider");
                if ui.button(tr_args("Record Pose at {percent}%", &[("percent", &format!("{:.0}", at * 100.0))])).on_hover_text(hint).clicked() {
                    let name = tr_args("Pose {number}", &[("number", &(editor.doc.controllers[i].poses.len() + 1).to_string())]);
                    editor.doc.record_pose(i, name, at, frame);
                }
                ui.weak(tr_args("{count} layers", &[("count", &driven.to_string())]));
            });
        });
    }
    if let Some(i) = remove {
        editor.doc.controllers.remove(i);
    }
    ui.separator();
    let selected = editor.selected_ids();
    let new = ui
        .add_enabled(!selected.is_empty(), egui::Button::new(tr("New Controller from Selected Layers")))
        .on_hover_text(tr("A slider that poses these layers between poses you record along it"));
    if new.clicked() {
        let name =
            tr_args("Controller {number}", &[("number", &(editor.doc.controllers.len() + 1).to_string())]);
        let i = editor.doc.add_controller(name, &selected);
        editor.doc.record_pose(i, tr("Start"), 0.0, frame);
    }
    if editor.doc.controllers.is_empty() {
        ui.weak(tr("Select the layers of a pose (a face, a hand) and make a controller. Record a pose at one end of its slider, move the slider, pose the layers differently and record again; then the slider moves between them."));
    }
}

pub fn history(ui: &mut egui::Ui, editor: &mut Editor) {
    let (done, undone): (Vec<String>, Vec<String>) = {
        let (done, undone) = editor.history.labels();
        (done.into_iter().map(str::to_owned).collect(), undone.into_iter().map(str::to_owned).collect())
    };
    ui.weak(tr("Click a step to go back to it."));
    ui.add_space(4.0);
    let mut jump = None;
    ui.spacing_mut().item_spacing.y = 1.0;
    // The start, every step done (the last is where things stand), then the
    // steps undone, greyed, which can still be redone.
    let current = done.len();
    let start = std::iter::once(tr("Start").to_owned());
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
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
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
