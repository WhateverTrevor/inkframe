//! The painting tools' settings in the Properties panel: the brush and its
//! preset, the Pattern Stamp's pattern, and the controls they share.

use crate::convert::color_button;
use crate::editor::Editor;
use crate::i18n::tr;

/// The painting tool's settings, while one is in use, as Photoshop's
/// options bar shows them: just the ones that tool has.
pub(super) fn brush_section(ui: &mut egui::Ui, editor: &mut Editor) {
    use crate::editor::Tool;
    let tool = editor.tool;
    ui.heading(tool.label());
    // What a stroke looks like with these settings, first, so it's in view
    // while adjusting them.
    match tool {
        Tool::Brush => crate::brush_panel::stroke_preview(
            ui,
            &editor.brush,
            Some(editor.paint_color),
            "tool-brush-preview",
        ),
        Tool::Eraser => crate::brush_panel::stroke_preview(ui, &editor.eraser, None, "tool-eraser-preview"),
        _ => {}
    }
    egui::Grid::new("paint_tool").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        if matches!(
            tool,
            Tool::Brush
                | Tool::Mixer
                | Tool::PatternStamp
                | Tool::PaintBucket
                | Tool::Gradient
                | Tool::LassoFill
                | Tool::Eyedropper
        ) {
            ui.label(tr("Colour"));
            color_button(ui, &mut editor.paint_color);
            ui.end_row();
        }
        match tool {
            Tool::Brush | Tool::Eraser => {
                ui.label(tr("Brush"));
                brush_preset_menu(ui, editor, tool);
                ui.end_row();
                if let Some(settings) = editor.brush_of(tool) {
                    brush_controls(ui, settings, true);
                }
                ui.label("");
                if ui
                    .button(tr("Brush Settings…"))
                    .on_hover_text(tr("Every setting of the brush: its tip, dynamics, texture and more"))
                    .clicked()
                {
                    editor.brush_request = Some(crate::brush_panel::Asked::ShowBrushSettings);
                }
                ui.end_row();
            }
            Tool::PatternStamp => pattern_stamp_settings(ui, editor),
            Tool::PaintBucket => {
                ui.label(tr("Tolerance"))
                    .on_hover_text(tr("How different a colour may be and still be filled"));
                ui.add(egui::Slider::new(&mut editor.bucket.tolerance, 0..=255));
                ui.end_row();
                ui.label(tr("Contiguous")).on_hover_text(tr(
                    "Fill only the area connected to where you click, or every matching pixel",
                ));
                ui.checkbox(&mut editor.bucket.contiguous, "");
                ui.end_row();
                ui.label(tr("Opacity"));
                ui.add(percent(&mut editor.bucket.opacity));
                ui.end_row();
                // On a vector layer the bucket fills between lines instead.
                ui.label(tr("Close gaps")).on_hover_text(tr(
                    "On a vector layer: how wide a gap between lines still counts as closed when filling",
                ));
                let sizes =
                    [(0.0, tr("None")), (4.0, tr("Small")), (10.0, tr("Medium")), (20.0, tr("Large"))];
                let current =
                    sizes.iter().find(|(v, _)| *v == editor.close_gaps).map_or(tr("Custom"), |(_, n)| n);
                egui::ComboBox::from_id_salt("close_gaps").selected_text(current).show_ui(ui, |ui| {
                    for (value, name) in sizes {
                        ui.selectable_value(&mut editor.close_gaps, value, name);
                    }
                });
                ui.end_row();
            }
            Tool::Gradient => {
                ui.label(tr("To"));
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!editor.gradient_to_clear, |ui| {
                        color_button(ui, &mut editor.second_color)
                    });
                    ui.checkbox(&mut editor.gradient_to_clear, tr("Transparent"));
                });
                ui.end_row();
                ui.label(tr("Shape"));
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut editor.gradient_shape,
                        inkframe_core::GradientShape::Linear,
                        tr("Linear"),
                    );
                    ui.radio_value(
                        &mut editor.gradient_shape,
                        inkframe_core::GradientShape::Radial,
                        tr("Radial"),
                    );
                });
                ui.end_row();
                ui.label(tr("Opacity"));
                ui.add(percent(&mut editor.gradient_opacity));
                ui.end_row();
            }
            Tool::Eyedropper => {}
            _ => {
                let (label, hint) = match tool {
                    Tool::Dodge | Tool::Burn => {
                        (tr("Exposure"), tr("How much each pass lightens or darkens"))
                    }
                    Tool::CloneStamp => (tr("Opacity"), tr("How strongly the copy covers what's there")),
                    Tool::Mixer => {
                        (tr("Wet"), tr("How much of the paint under the brush it picks up and smears"))
                    }
                    _ => (tr("Strength"), tr("How strongly each pass works")),
                };
                if let Some(retouch) = editor.retouch_of(tool) {
                    ui.label(label).on_hover_text(hint);
                    ui.add(percent(&mut retouch.strength));
                    ui.end_row();
                }
                if tool == Tool::Mixer {
                    ui.label(tr("Load"))
                        .on_hover_text(tr("How much paint the brush holds: with little, it runs dry"));
                    ui.add(percent(&mut editor.mixer_load));
                    ui.end_row();
                    ui.label(tr("Mix"))
                        .on_hover_text(tr("How much of the colour it picks up mixes into its paint"));
                    ui.add(percent(&mut editor.mixer_mix));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut editor.mixer_sample_all, tr("Sample All Layers"))
                        .on_hover_text(tr("Pick up colour from everything as seen, not only this layer"));
                    ui.end_row();
                }
                if tool == Tool::Liquify {
                    ui.label(tr("Mode")).on_hover_text(tr(
                        "Push drags paint along the stroke; the others work round the brush's middle, while it's held still too",
                    ));
                    egui::ComboBox::from_id_salt("liquify_mode")
                        .selected_text(tr(editor.liquify_mode.label()))
                        .show_ui(ui, |ui| {
                            for mode in inkframe_core::LiquifyMode::ALL {
                                ui.selectable_value(&mut editor.liquify_mode, mode, tr(mode.label()));
                            }
                        });
                    ui.end_row();
                }
                if matches!(tool, Tool::Dodge | Tool::Burn) {
                    ui.label(tr("Range")).on_hover_text(tr("Which tones it works on most"));
                    egui::ComboBox::from_id_salt("tone_range")
                        .selected_text(tr(editor.tone_range.label()))
                        .show_ui(ui, |ui| {
                            for range in inkframe_core::ToneRange::ALL {
                                ui.selectable_value(&mut editor.tone_range, range, tr(range.label()));
                            }
                        });
                    ui.end_row();
                }
                if let Some(retouch) = editor.retouch_of(tool) {
                    brush_controls(ui, &mut retouch.brush, false);
                }
            }
        }
        if tool.uses_brush() {
            ui.label(tr("Smoothing"))
                .on_hover_text(tr("Steadies the line by letting it trail behind the pen"));
            ui.add(percent(&mut editor.smoothing));
            ui.end_row();
            ui.label(tr("Pen feel")).on_hover_text(tr(
                "How pressing the pen feels: soft paints fully with a light touch, firm takes pressing hard",
            ));
            ui.horizontal(|ui| {
                // Narrow enough that the words either side fit beside it.
                ui.spacing_mut().slider_width = 110.0;
                ui.weak(tr("Soft"));
                ui.add(egui::Slider::new(&mut editor.pressure_feel, -1.0..=1.0).show_value(false));
                ui.weak(tr("Firm"));
            });
            ui.end_row();
        }
    });
    let hint = match tool {
        Tool::CloneStamp if editor.clone_source.is_none() => {
            Some(tr("Option/Alt-click on the paint layer to choose where to copy from."))
        }
        Tool::CloneStamp => Some(tr("Option/Alt-click to copy from somewhere else.")),
        Tool::Eyedropper => {
            Some(tr("Click the stage to pick up a colour (or Option/Alt-click with the brush)."))
        }
        Tool::Gradient => {
            Some(tr("Drag across the paint layer, from where the colour starts to where it ends."))
        }
        _ if tool.uses_brush() && tablet_hint_needed() => {
            Some(tr("Pen pressure is used when drawing with a tablet."))
        }
        _ => None,
    };
    if let Some(hint) = hint {
        ui.weak(hint);
    }
}

/// The Pattern Stamp's settings: the pattern and its colours, then its
/// brush.
pub(super) fn pattern_stamp_settings(ui: &mut egui::Ui, editor: &mut Editor) {
    use crate::brush_panel::Asked;
    ui.label(tr("Light colour"))
        .on_hover_text(tr("The pattern's light parts; its dark parts are the colour above"));
    color_button(ui, &mut editor.second_color);
    ui.end_row();
    ui.label(tr("Pattern"));
    let patterns = crate::brush_panel::pattern_choices(editor);
    crate::brush_panel::pattern_menu(ui, "stamp_pattern", &mut editor.stamp_pattern.pattern, &patterns);
    ui.end_row();
    ui.label("");
    ui.horizontal(|ui| {
        if ui.small_button(tr("Import…")).on_hover_text(tr("A pattern from a picture")).clicked() {
            editor.brush_request = Some(Asked::ImportPattern);
        }
        if ui
            .small_button(tr("From Selection"))
            .on_hover_text(tr("A pattern from the selected pixels"))
            .clicked()
        {
            editor.brush_request = Some(Asked::DefinePattern);
        }
    });
    ui.end_row();
    ui.label(tr("Scale"));
    ui.add(
        egui::Slider::new(&mut editor.stamp_pattern.scale, 0.1..=10.0)
            .logarithmic(true)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    ui.label("");
    ui.checkbox(&mut editor.stamp_pattern.invert, tr("Invert"));
    ui.end_row();
    brush_controls(ui, &mut editor.pattern_stamp, true);
    ui.label("");
    if ui.button(tr("Brush Settings…")).clicked() {
        editor.brush_request = Some(Asked::ShowBrushSettings);
    }
    ui.end_row();
}

pub(super) fn brush_controls(ui: &mut egui::Ui, settings: &mut inkframe_core::BrushSettings, paint: bool) {
    ui.label(tr("Size")).on_hover_text(tr("[ and ] make it smaller and bigger"));
    ui.add(egui::Slider::new(&mut settings.size, 1.0..=2500.0).logarithmic(true).suffix(" px"));
    ui.end_row();
    ui.label(tr("Hardness")).on_hover_text(tr(
        "How crisp the edge is: 100% is a hard disc, 0% fades from the middle. Shift+[ and Shift+] change it",
    ));
    ui.add_enabled(settings.tip == inkframe_core::Tip::Round, percent(&mut settings.hardness));
    ui.end_row();
    if paint {
        ui.label(tr("Opacity"))
            .on_hover_text(tr("The most a single stroke covers, however often it crosses itself"));
        ui.add(percent(&mut settings.opacity));
        ui.end_row();
        ui.label(tr("Flow"))
            .on_hover_text(tr("How much each touch of the brush adds: low flow builds up like an airbrush"));
        ui.add(percent(&mut settings.flow));
        ui.end_row();
    }
    ui.label(tr("Spacing")).on_hover_text(tr("Distance between dabs, as a share of the brush's size"));
    ui.add(
        egui::Slider::new(&mut settings.spacing, 0.01..=10.0)
            .logarithmic(true)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    ui.label(tr("Pressure")).on_hover_text(tr("What pressing harder with a pen does"));
    ui.horizontal(|ui| {
        if paint {
            crate::brush_panel::pressure_switches(ui, settings);
        } else {
            // The retouching tools: size, and strength through the flow.
            let mut size =
                settings.shape.on && settings.shape.size_control == inkframe_core::Control::PenPressure;
            if ui.checkbox(&mut size, tr("Size")).changed() {
                settings.shape.on = size;
                settings.shape.size_control = inkframe_core::Control::PenPressure;
            }
            let mut strength =
                settings.transfer.on && settings.transfer.flow_control == inkframe_core::Control::PenPressure;
            if ui.checkbox(&mut strength, tr("Strength")).changed() {
                settings.transfer.on = strength;
                settings.transfer.flow_control = inkframe_core::Control::PenPressure;
            }
        }
    });
    ui.end_row();
    if settings.shape.on && settings.shape.size_control == inkframe_core::Control::PenPressure {
        ui.label(tr("Lightest"))
            .on_hover_text(tr("The size at the lightest touch, as a share of the full size"));
        ui.add(percent(&mut settings.shape.minimum_diameter));
        ui.end_row();
    }
}

/// A menu of the brushes, ready-made and saved, for the Brush or Eraser.
pub(super) fn brush_preset_menu(ui: &mut egui::Ui, editor: &mut Editor, tool: crate::editor::Tool) {
    let current = editor.brushes.picked.clone().unwrap_or_else(|| tr("Custom").to_owned());
    let shown = crate::brush_panel::preset_name(&current);
    let mut picked = None;
    egui::ComboBox::from_id_salt("brush_preset").selected_text(shown).width(170.0).show_ui(ui, |ui| {
        for (preset, saved) in editor.brushes.all() {
            if saved && !editor.brushes.saved.is_empty() && std::ptr::eq(preset, &editor.brushes.saved[0]) {
                ui.separator();
            }
            if ui
                .selectable_label(current == preset.name, crate::brush_panel::preset_name(&preset.name))
                .clicked()
            {
                picked = Some(preset.clone());
            }
        }
    });
    if let Some(preset) = picked {
        if let Some(settings) = editor.brush_of(tool) {
            *settings = preset.settings;
        }
        editor.brushes.picked = Some(preset.name);
    }
}

/// A slider for a 0–1 setting, shown as a percentage.
pub(crate) fn percent(value: &mut f32) -> egui::Slider<'_> {
    egui::Slider::new(value, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
}

/// Whether to mention that a tablet changes the stroke: until a pen has
/// been seen, people with only a mouse may wonder what Pressure does.
pub(super) fn tablet_hint_needed() -> bool {
    crate::tablet::current().is_none()
}
