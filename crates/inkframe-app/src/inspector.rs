//! The properties panel on the right: numeric editing of the selected layer,
//! its keyframe easing and shapes, and the document settings.

use egui::{Color32, DragValue, Sense, Stroke as EguiStroke, vec2};
use inkframe_core::{
    Color, Deformer, Easing, FrameRate, ImageId, Layer, LayerId, Playback, Property, RotationLimits, Shape,
    Stroke, Style, SymbolId, Text, TextAlign, Tween,
};

use crate::audio_engine::AudioEngine;
use crate::convert::color_button;
use crate::easing_editor;
use crate::editor::Editor;

/// The Properties panel's tabs, as Animate's: the current tool, the
/// selected layer, and the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Tool,
    Layer,
    Document,
}

/// What the tabs last saw, so picking a new layer or tool can bring its tab
/// forward, as Animate does, while a tab chosen by hand stays put otherwise.
#[derive(Clone, Copy, PartialEq)]
struct Seen {
    selection: Option<LayerId>,
    tool: crate::editor::Tool,
}

/// Shows the Properties panel: a row of tabs, then the chosen one in a
/// scroll area. Returns an action from the audio section for the app.
pub fn show(ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) -> Option<AudioAction> {
    let tab_id = ui.id().with("properties_tab");
    let seen_id = ui.id().with("properties_seen");
    let now = Seen { selection: editor.selection, tool: editor.tool };
    let mut tab = ui.data(|d| d.get_temp::<Tab>(tab_id)).unwrap_or(Tab::Layer);
    let seen = ui.data(|d| d.get_temp::<Seen>(seen_id));
    if seen != Some(now) {
        if seen.is_some_and(|s| s.tool != now.tool) && (now.tool.paints() || now.tool.selects()) {
            tab = Tab::Tool;
        } else if seen.is_some_and(|s| s.selection != now.selection) {
            tab = if now.selection.is_some() { Tab::Layer } else { Tab::Document };
        } else if seen.is_none() && now.selection.is_none() {
            tab = Tab::Document;
        }
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (choice, label) in [(Tab::Tool, "Tool"), (Tab::Layer, "Layer"), (Tab::Document, "Document")] {
            ui.selectable_value(&mut tab, choice, label);
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(tab_id, tab);
        d.insert_temp(seen_id, now);
    });
    ui.separator();

    let mut action = None;
    egui::ScrollArea::vertical().id_salt("properties_scroll").show(ui, |ui| match tab {
        Tab::Tool => tool_settings(ui, editor),
        Tab::Layer => layer_tab(ui, editor),
        Tab::Document => {
            document_section(ui, editor);
            ui.add_space(8.0);
            action = audio_section(ui, editor, audio);
        }
    });
    action
}

fn layer_tab(ui: &mut egui::Ui, editor: &mut Editor) {
    // With several layers selected, the panel shows the one picked last.
    let count = editor.selected_ids().len();
    if count > 1 {
        ui.weak(format!(
            "{count} layers selected. Below is the last one picked; drag on the stage to move them all."
        ));
        ui.add_space(4.0);
    }
    match editor.selection {
        Some(id) => layer_section(ui, editor, id),
        None => {
            ui.weak("Select a layer on the stage or in the timeline to see its properties.");
        }
    }
}

/// The current tool's settings: the brush or selection options, or the
/// colours and line width new shapes get.
pub fn tool_settings(ui: &mut egui::Ui, editor: &mut Editor) {
    use crate::editor::Tool;
    if editor.tool.paints() {
        brush_section(ui, editor);
    } else if editor.tool.selects() {
        selection_section(ui, editor);
    } else if editor.tool == Tool::Pencil {
        ui.heading(editor.tool.name());
        egui::Grid::new("pencil").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Line");
            color_button(ui, &mut editor.pencil_stroke.color);
            ui.end_row();
            ui.label("Width");
            ui.add(DragValue::new(&mut editor.pencil_stroke.width).range(0.5..=100.0).speed(0.1));
            ui.end_row();
        });
        ui.checkbox(&mut editor.pencil_pressure, "Pressure")
            .on_hover_text("Press harder with a pen for a thicker line, lighter for a thinner one");
        ui.checkbox(&mut editor.pencil_taper, "Taper ends")
            .on_hover_text("Lines thin out towards their ends, as inked lines do");
        if editor.pencil_pressure || editor.pencil_taper {
            ui.weak("Shape a line's thickness afterwards with the Width tool (Shift+A).");
        }
        drawing_target(ui, editor);
    } else if matches!(editor.tool, Tool::Rectangle | Tool::Ellipse | Tool::Pen | Tool::Text) {
        ui.heading(editor.tool.name());
        shape_style(ui, editor);
        if editor.tool != Tool::Text {
            drawing_target(ui, editor);
        }
    } else {
        ui.weak(format!("The {} tool has no settings.", editor.tool.name()));
    }
}

/// Where the drawing tools put new shapes: each on a layer of its own (as
/// in Animate), or into the selected layer's drawing (as in Harmony), and
/// then into its line art or the colour art beneath.
fn drawing_target(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.add_space(6.0);
    ui.checkbox(&mut editor.draw_into_layer, "Draw into the selected layer").on_hover_text(
        "Strokes go into the selected layer's drawing, for inking and painting a drawing together. \
         Off, each shape gets a layer of its own.",
    );
    ui.add_enabled_ui(editor.draw_into_layer, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut editor.colour_art, false, "Line Art");
            ui.selectable_value(&mut editor.colour_art, true, "Colour Art")
                .on_hover_text("Beneath the lines, where the paint bucket's fills go");
        });
    });
}

/// Fill, stroke and stroke width for the shapes the drawing tools make.
fn shape_style(ui: &mut egui::Ui, editor: &mut Editor) {
    let style = &mut editor.draw_style;
    egui::Grid::new("shape_style").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Fill");
        ui.horizontal(|ui| {
            let mut on = style.fill.is_some();
            if ui.checkbox(&mut on, "").changed() {
                style.fill = on.then_some(style.fill.unwrap_or(Color::BLACK));
            }
            if let Some(fill) = &mut style.fill {
                color_button(ui, fill);
            }
        });
        ui.end_row();
        ui.label("Stroke");
        ui.horizontal(|ui| {
            let mut on = style.stroke.is_some();
            if ui.checkbox(&mut on, "").changed() {
                style.stroke =
                    on.then_some(style.stroke.unwrap_or(Stroke { color: Color::BLACK, width: 2.0 }));
            }
            if let Some(stroke) = &mut style.stroke {
                color_button(ui, &mut stroke.color);
                ui.add(DragValue::new(&mut stroke.width).range(0.1..=200.0).speed(0.1).suffix(" px"));
            }
        });
        ui.end_row();
    });
}

/// The document settings on their own, for a Document area.
pub fn document(ui: &mut egui::Ui, editor: &mut Editor) {
    document_section(ui, editor);
}

/// The painting tool's settings, while one is in use, as Photoshop's
/// options bar shows them: just the ones that tool has.
fn brush_section(ui: &mut egui::Ui, editor: &mut Editor) {
    use crate::editor::Tool;
    let tool = editor.tool;
    ui.heading(tool.name());
    egui::Grid::new("paint_tool").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        if matches!(tool, Tool::Brush | Tool::PaintBucket | Tool::Gradient | Tool::Eyedropper) {
            ui.label("Colour");
            color_button(ui, &mut editor.paint_color);
            ui.end_row();
        }
        match tool {
            Tool::Brush => {
                ui.label("Preset");
                egui::ComboBox::from_id_salt("brush_preset").selected_text("Choose…").show_ui(ui, |ui| {
                    for (name, preset) in inkframe_core::BrushSettings::presets() {
                        if ui.button(name).clicked() {
                            editor.brush = preset;
                        }
                    }
                });
                ui.end_row();
                brush_controls(ui, &mut editor.brush, true);
            }
            Tool::Eraser => brush_controls(ui, &mut editor.eraser, true),
            Tool::PaintBucket => {
                ui.label("Tolerance").on_hover_text("How different a colour may be and still be filled");
                ui.add(egui::Slider::new(&mut editor.bucket.tolerance, 0..=255));
                ui.end_row();
                ui.label("Contiguous").on_hover_text(
                    "Fill only the area connected to where you click, or every matching pixel",
                );
                ui.checkbox(&mut editor.bucket.contiguous, "");
                ui.end_row();
                ui.label("Opacity");
                ui.add(percent(&mut editor.bucket.opacity));
                ui.end_row();
                // On a vector layer the bucket fills between lines instead.
                ui.label("Close gaps").on_hover_text(
                    "On a vector layer: how wide a gap between lines still counts as closed when filling",
                );
                let sizes = [(0.0, "None"), (4.0, "Small"), (10.0, "Medium"), (20.0, "Large")];
                let current =
                    sizes.iter().find(|(v, _)| *v == editor.close_gaps).map_or("Custom", |(_, n)| n);
                egui::ComboBox::from_id_salt("close_gaps").selected_text(current).show_ui(ui, |ui| {
                    for (value, name) in sizes {
                        ui.selectable_value(&mut editor.close_gaps, value, name);
                    }
                });
                ui.end_row();
            }
            Tool::Gradient => {
                ui.label("To");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!editor.gradient_to_clear, |ui| {
                        color_button(ui, &mut editor.second_color)
                    });
                    ui.checkbox(&mut editor.gradient_to_clear, "Transparent");
                });
                ui.end_row();
                ui.label("Shape");
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut editor.gradient_shape,
                        inkframe_core::GradientShape::Linear,
                        "Linear",
                    );
                    ui.radio_value(
                        &mut editor.gradient_shape,
                        inkframe_core::GradientShape::Radial,
                        "Radial",
                    );
                });
                ui.end_row();
                ui.label("Opacity");
                ui.add(percent(&mut editor.gradient_opacity));
                ui.end_row();
            }
            Tool::Eyedropper => {}
            _ => {
                let (label, hint) = match tool {
                    Tool::Dodge | Tool::Burn => ("Exposure", "How much each pass lightens or darkens"),
                    Tool::CloneStamp => ("Opacity", "How strongly the copy covers what's there"),
                    _ => ("Strength", "How strongly each pass works"),
                };
                if let Some(retouch) = editor.retouch_of(tool) {
                    ui.label(label).on_hover_text(hint);
                    ui.add(percent(&mut retouch.strength));
                    ui.end_row();
                }
                if matches!(tool, Tool::Dodge | Tool::Burn) {
                    ui.label("Range").on_hover_text("Which tones it works on most");
                    egui::ComboBox::from_id_salt("tone_range")
                        .selected_text(editor.tone_range.label())
                        .show_ui(ui, |ui| {
                            for range in inkframe_core::ToneRange::ALL {
                                ui.selectable_value(&mut editor.tone_range, range, range.label());
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
            ui.label("Smoothing").on_hover_text("Steadies the line by letting it trail behind the pen");
            ui.add(percent(&mut editor.smoothing));
            ui.end_row();
        }
    });
    let hint = match tool {
        Tool::CloneStamp if editor.clone_source.is_none() => {
            Some("Option/Alt-click on the paint layer to choose where to copy from.")
        }
        Tool::CloneStamp => Some("Option/Alt-click to copy from somewhere else."),
        Tool::Eyedropper => Some("Click the stage to pick up a colour (or Option/Alt-click with the brush)."),
        Tool::Gradient => Some("Drag across the paint layer, from where the colour starts to where it ends."),
        _ if tool.uses_brush() && tablet_hint_needed() => {
            Some("Pen pressure is used when drawing with a tablet.")
        }
        _ => None,
    };
    if let Some(hint) = hint {
        ui.weak(hint);
    }
}

/// The settings every brush tip has. `paint` adds the ones only painting
/// (rather than retouching) uses: opacity, flow and the paper grain.
fn brush_controls(ui: &mut egui::Ui, settings: &mut inkframe_core::BrushSettings, paint: bool) {
    ui.label("Size").on_hover_text("[ and ] make it smaller and bigger");
    ui.add(egui::Slider::new(&mut settings.size, 1.0..=500.0).logarithmic(true).suffix(" px"));
    ui.end_row();
    ui.label("Hardness")
        .on_hover_text("How crisp the edge is: 100% is a hard disc, 0% fades from the middle");
    ui.add(percent(&mut settings.hardness));
    ui.end_row();
    if paint {
        ui.label("Opacity").on_hover_text("The most a single stroke covers, however often it crosses itself");
        ui.add(percent(&mut settings.opacity));
        ui.end_row();
        ui.label("Flow")
            .on_hover_text("How much each touch of the brush adds: low flow builds up like an airbrush");
        ui.add(percent(&mut settings.flow));
        ui.end_row();
        ui.label("Grain")
            .on_hover_text("A paper texture that breaks the paint up, like chalk or a dry brush");
        ui.add(percent(&mut settings.grain));
        ui.end_row();
    }
    ui.label("Spacing").on_hover_text("Distance between dabs, as a share of the brush's size");
    ui.add(
        egui::Slider::new(&mut settings.spacing, 0.02..=1.0)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    ui.label("Pressure").on_hover_text("What pressing harder with a pen does");
    ui.horizontal(|ui| {
        ui.checkbox(&mut settings.pressure_size, "Size");
        ui.checkbox(&mut settings.pressure_flow, if paint { "Flow" } else { "Strength" });
    });
    ui.end_row();
    if settings.pressure_size {
        ui.label("Lightest").on_hover_text("The size at the lightest touch, as a share of the full size");
        ui.add(percent(&mut settings.min_size));
        ui.end_row();
    }
}

/// The selection tools' options, and what to do with a selection.
fn selection_section(ui: &mut egui::Ui, editor: &mut Editor) {
    use crate::editor::Tool;
    ui.heading(editor.tool.name());
    if editor.tool == Tool::MagicWand {
        egui::Grid::new("wand").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Tolerance").on_hover_text("How different a colour may be and still be selected");
            ui.add(egui::Slider::new(&mut editor.wand_tolerance, 0..=255));
            ui.end_row();
            ui.label("Contiguous").on_hover_text("Select only the area connected to where you click");
            ui.checkbox(&mut editor.wand_contiguous, "");
            ui.end_row();
        });
    }
    ui.weak(match editor.tool {
        Tool::PolygonLasso => {
            "Click each corner; click the first again, double-click or press Enter to close."
        }
        Tool::MagicWand => "Click an area of colour to select it.",
        _ => "Drag to select.",
    });
    ui.weak("Shift adds to the selection, Option/Alt takes away, both keep the overlap.");
    let selected = editor.active_selection().is_some();
    ui.horizontal(|ui| {
        if ui.add_enabled(editor.paint_target().is_some(), egui::Button::new("All")).clicked() {
            editor.select_all_pixels();
        }
        if ui.add_enabled(selected, egui::Button::new("Deselect")).clicked() {
            editor.deselect_pixels();
        }
        if ui.add_enabled(editor.paint_target().is_some(), egui::Button::new("Inverse")).clicked() {
            editor.invert_pixel_selection();
        }
    });
    if selected {
        ui.weak(
            "Drag inside the selection with the Selection tool (V) to move those pixels; Delete clears them.",
        );
    }
}

/// A slider for a 0–1 setting, shown as a percentage.
fn percent(value: &mut f32) -> egui::Slider<'_> {
    egui::Slider::new(value, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
}

/// Whether to mention that a tablet changes the stroke: until a pen has
/// been seen, people with only a mouse may wonder what Pressure does.
fn tablet_hint_needed() -> bool {
    crate::tablet::current().is_none()
}

/// The camera: where it looks, how far it's zoomed and how it's turned on
/// this frame (each keyed as it's changed, like a layer's transform), and
/// whether it's on.
fn camera_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(camera) = editor.doc.layer(id) else { return };
    let mut pose = editor.doc.pose(camera, frame);
    let mut on = camera.visible;
    ui.heading("Camera");
    if ui.checkbox(&mut on, "On").on_hover_text("Off, the stage shows the scene without the camera").changed()
    {
        editor.set_visible(id, on);
    }
    let before = pose;
    let mut zoom = pose.scale.x * 100.0;
    egui::Grid::new("camera").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Looks at").on_hover_text("The point of the scene in the middle of the stage");
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut pose.position.x).prefix("x ").speed(1.0));
            ui.add(DragValue::new(&mut pose.position.y).prefix("y ").speed(1.0));
        });
        ui.end_row();
        ui.label("Zoom");
        ui.add(DragValue::new(&mut zoom).range(5.0..=5000.0).suffix("%").speed(1.0));
        ui.end_row();
        ui.label("Rotation");
        ui.add(DragValue::new(&mut pose.rotation).suffix("°").speed(0.5));
        ui.end_row();
    });
    pose.scale = inkframe_core::Point::new(zoom / 100.0, zoom / 100.0);
    if pose != before {
        editor.set_pose(id, pose);
    }
    if ui
        .button("Reset Camera")
        .on_hover_text("Back to looking straight at the stage, on this frame")
        .clicked()
    {
        let centre = editor.doc.stage_rect().center();
        editor.set_pose(id, inkframe_core::Transform::at(centre));
    }
    ui.add_space(4.0);
    ui.weak("With the Camera tool (C): drag to pan, Shift-drag to turn, Alt-drag to zoom. Set each layer's Depth for parallax.");
}

/// A folder: its name, and showing or locking everything in it.
fn folder_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some(folder) = editor.doc.layer_mut(id) else { return };
    ui.heading("Folder");
    ui.text_edit_singleline(&mut folder.name);
    let (mut visible, mut locked) = (folder.visible, folder.locked);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut visible, "Visible").changed() {
            editor.set_visible(id, visible);
        }
        if ui.checkbox(&mut locked, "Locked").changed() {
            editor.set_locked(id, locked);
        }
    });
    let count = editor.doc.folder_block(id).len() - 1;
    ui.weak(match count {
        0 => "Empty. Drag layers just under the folder's name in the timeline to put them in it.".to_owned(),
        1 => "Holds 1 layer.".to_owned(),
        n => format!("Holds {n} layers."),
    });
}

fn layer_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let last_frame = editor.last_frame();
    let editor_timeline_is_main = editor.doc.timeline_of(id) == Some(inkframe_core::TimelineId::Main);
    if editor.doc.layer(id).is_some_and(|l| l.folder) {
        folder_section(ui, editor, id);
        return;
    }
    if editor.doc.layer(id).is_some_and(|l| l.camera) {
        camera_section(ui, editor, id);
        return;
    }
    let Some(layer) = editor.doc.layer_mut(id) else { return };

    ui.heading("Layer");
    ui.text_edit_singleline(&mut layer.name);
    // Wrapped, so a narrow panel shows all four rather than cutting one off.
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut layer.visible, "Visible");
        ui.checkbox(&mut layer.locked, "Locked");
        ui.checkbox(&mut layer.guide, "Guide").on_hover_text(
            "A motion guide: other layers can follow its path. Shown while editing, never exported.",
        );
        ui.checkbox(&mut layer.mask, "Mask").on_hover_text(
            "A mask: layers masked by it only show where it has something drawn. It isn't drawn itself.",
        );
    });
    span_editor(ui, layer, last_frame);
    // Depth, for the camera's parallax: only layers at the top of a rig on
    // the main timeline sit at a depth; the rest go with them.
    let top_level = layer.parent.is_none() && editor_timeline_is_main;
    if top_level {
        ui.horizontal(|ui| {
            ui.label("Depth").on_hover_text(
                "How far behind the stage the layer sits, for the camera: deeper layers look smaller \
                 and move less as the camera pans. Negative comes forward.",
            );
            ui.add(DragValue::new(&mut layer.depth).range(-900.0..=100_000.0).speed(5.0));
        });
    }
    ui.horizontal(|ui| {
        ui.label("Blend").on_hover_text("How the layer mixes with what's beneath it");
        egui::ComboBox::from_id_salt("blend").selected_text(layer.blend.label()).show_ui(ui, |ui| {
            for mode in inkframe_core::BlendMode::ALL {
                ui.selectable_value(&mut layer.blend, mode, mode.label());
            }
        });
    });

    ui.add_space(8.0);
    transform_section(ui, editor, id, frame);
    rig_section(ui, editor, id, frame);
    ui.add_space(8.0);
    easing_section(ui, editor, id, frame);
    ui.add_space(8.0);
    instance_section(ui, editor, id);
    effects_section(ui, editor, id);
    deformer_section(ui, editor, id);
    warp_section(ui, editor, id);
    drawings_section(ui, editor, id);
    shapes_section(ui, editor, id);
}

/// The layer's colour effect and filters, as in Animate's Properties.
fn effects_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    use inkframe_core::{ColorEffect, Filter, FilterKind};
    // The effects on this frame, edited as a copy and keyed back if they
    // change, as dragging a layer keys its position.
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer(id) else { return };
    let before = layer.track.effects_at(frame as f32);
    let keyed = layer.track.effects.key_at(frame).is_some();
    let animated = layer.track.effects.keys().len() > 1;
    let mut fx = before.clone();
    ui.horizontal(|ui| {
        if key_toggle(ui, Property::Effects, keyed).clicked() {
            editor.toggle_property_key(id, Property::Effects);
        }
        ui.strong("Effects");
        if animated {
            ui.weak("animated");
        }
    })
    .response
    .on_hover_text("Change them on another frame to animate them, as with the transform");

    egui::CollapsingHeader::new("Colour Effect")
        .id_salt("color_effect")
        .default_open(!fx.color.is_none())
        .show(ui, |ui| {
            let effect = &mut fx.color;
            egui::ComboBox::from_id_salt("color_effect_kind").selected_text(effect.label()).show_ui(
                ui,
                |ui| {
                    let choices = [
                        ColorEffect::None,
                        ColorEffect::Brightness { amount: 0.5 },
                        ColorEffect::Tint { color: Color::rgb(255, 0, 0), amount: 0.5 },
                        ColorEffect::Advanced { multiply: [1.0; 4], add: [0.0; 4] },
                    ];
                    for choice in choices {
                        if ui.selectable_label(effect.label() == choice.label(), choice.label()).clicked()
                            && effect.label() != choice.label()
                        {
                            *effect = choice;
                        }
                    }
                },
            );
            match effect {
                ColorEffect::None => {}
                ColorEffect::Brightness { amount } => {
                    ui.add(
                        egui::Slider::new(amount, -1.0..=1.0)
                            .custom_formatter(|v, _| format!("{:+.0}%", v * 100.0)),
                    );
                }
                ColorEffect::Tint { color, amount } => {
                    ui.horizontal(|ui| {
                        color_button(ui, color);
                        ui.add(
                            egui::Slider::new(amount, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        );
                    });
                }
                ColorEffect::Advanced { multiply, add } => {
                    egui::Grid::new("advanced_effect").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
                        for (i, name) in ["Red", "Green", "Blue", "Alpha"].into_iter().enumerate() {
                            ui.label(name);
                            let mut percent = multiply[i] * 100.0;
                            if ui
                                .add(DragValue::new(&mut percent).range(-100.0..=100.0).suffix("% ×"))
                                .changed()
                            {
                                multiply[i] = percent / 100.0;
                            }
                            ui.add(DragValue::new(&mut add[i]).range(-255.0..=255.0).prefix("+ "));
                            ui.end_row();
                        }
                    });
                }
            }
        });

    egui::CollapsingHeader::new(format!("Filters ({})", fx.filters.len()))
        .id_salt("filters")
        .default_open(!fx.filters.is_empty())
        .show(ui, |ui| {
            let mut remove = None;
            let mut raise = None;
            for (i, filter) in fx.filters.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut filter.enabled, filter.kind.label());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(crate::icons::IconButton::new(
                                    crate::icons::Icon::Trash,
                                    "Remove this filter",
                                ))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                            if i > 0
                                && ui
                                    .add(crate::icons::IconButton::new(
                                        crate::icons::Icon::Up,
                                        "Apply this one earlier",
                                    ))
                                    .clicked()
                            {
                                raise = Some(i);
                            }
                        });
                    });
                    ui.add_enabled_ui(filter.enabled, |ui| filter_controls(ui, &mut filter.kind));
                });
            }
            if let Some(i) = remove {
                fx.filters.remove(i);
            }
            if let Some(i) = raise {
                fx.filters.swap(i, i - 1);
            }
            ui.menu_button("Add Filter", |ui| {
                for kind in FilterKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        fx.filters.push(Filter { enabled: true, kind });
                        ui.close();
                    }
                }
            });
        });
    if fx != before
        && let Some(layer) = editor.doc.layer_mut(id)
    {
        layer.track.set_effects(frame, fx);
    }
}

/// One filter's settings.
fn filter_controls(ui: &mut egui::Ui, kind: &mut inkframe_core::FilterKind) {
    use inkframe_core::FilterKind;
    fn pixels(value: &mut f32) -> DragValue<'_> {
        DragValue::new(value).range(0.0..=255.0).speed(0.2).suffix(" px")
    }
    fn percent(value: &mut f32, max: f32) -> egui::Slider<'_> {
        egui::Slider::new(value, 0.0..=max).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
    }
    fn signed(value: &mut f32) -> egui::Slider<'_> {
        egui::Slider::new(value, -1.0..=1.0).custom_formatter(|v, _| format!("{:+.0}", v * 100.0))
    }
    egui::Grid::new("filter").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match kind {
        FilterKind::DropShadow { distance, angle, blur, color, strength } => {
            ui.label("Colour");
            color_button(ui, color);
            ui.end_row();
            ui.label("Distance");
            ui.add(DragValue::new(distance).range(-255.0..=255.0).speed(0.2).suffix(" px"));
            ui.end_row();
            ui.label("Angle");
            ui.add(DragValue::new(angle).speed(1.0).suffix("°"));
            ui.end_row();
            ui.label("Blur");
            ui.add(pixels(blur));
            ui.end_row();
            ui.label("Strength");
            ui.add(percent(strength, 10.0));
            ui.end_row();
        }
        FilterKind::Blur { x, y } => {
            ui.label("Blur");
            ui.horizontal(|ui| {
                ui.add(pixels(x).prefix("x "));
                ui.add(pixels(y).prefix("y "));
            });
            ui.end_row();
        }
        FilterKind::Glow { blur, color, strength, inner } => {
            ui.label("Colour");
            color_button(ui, color);
            ui.end_row();
            ui.label("Blur");
            ui.add(pixels(blur));
            ui.end_row();
            ui.label("Strength");
            ui.add(percent(strength, 10.0));
            ui.end_row();
            ui.label("");
            ui.checkbox(inner, "Inner glow");
            ui.end_row();
        }
        FilterKind::MotionBlur { shutter, samples } => {
            ui.label("Shutter").on_hover_text(
                "How much of each frame the blur covers: 180° is the usual film look, 360° the whole frame",
            );
            ui.add(egui::Slider::new(shutter, 0.0..=360.0).suffix("°"));
            ui.end_row();
            ui.label("Quality").on_hover_text(
                "The fewest moments drawn and blended. Fast movement gets more automatically, so the smear stays smooth",
            );
            ui.add(egui::Slider::new(samples, 2..=32));
            ui.end_row();
        }
        FilterKind::AdjustColor { brightness, contrast, saturation, hue } => {
            ui.label("Brightness");
            ui.add(signed(brightness));
            ui.end_row();
            ui.label("Contrast");
            ui.add(signed(contrast));
            ui.end_row();
            ui.label("Saturation");
            ui.add(signed(saturation));
            ui.end_row();
            ui.label("Hue");
            ui.add(egui::Slider::new(hue, -180.0..=180.0).suffix("°"));
            ui.end_row();
        }
    });
}

/// Settings for a layer holding a symbol instance: which symbol, and how its
/// own timeline plays as the main one advances.
fn instance_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some(mut instance) = editor.doc.layer(id).and_then(|l| l.instance) else { return };
    let symbols: Vec<(SymbolId, String)> =
        editor.doc.symbols.iter().map(|s| (s.id, s.name.clone())).collect();
    let Some(duration) = editor.doc.symbol(instance.symbol).map(|s| s.duration) else {
        ui.colored_label(ui.visuals().error_fg_color, "This instance's symbol is missing.");
        return;
    };

    ui.strong("Symbol instance");
    egui::Grid::new("instance").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Symbol");
        let current = symbols.iter().find(|(s, _)| *s == instance.symbol).map_or("", |(_, n)| n.as_str());
        egui::ComboBox::from_id_salt("instance_symbol").selected_text(current).show_ui(ui, |ui| {
            // Inside a symbol, leave out anything that would end up
            // containing itself.
            for (symbol, name) in symbols.iter().filter(|(s, _)| editor.can_place(*s)) {
                ui.selectable_value(&mut instance.symbol, *symbol, name);
            }
        });
        ui.end_row();

        ui.label("Playback");
        egui::ComboBox::from_id_salt("instance_playback").selected_text(instance.playback.label()).show_ui(
            ui,
            |ui| {
                for playback in Playback::ALL {
                    ui.selectable_value(&mut instance.playback, playback, playback.label());
                }
            },
        );
        ui.end_row();

        // Shown 1-based, like every other frame number in the interface.
        ui.label("First frame");
        let mut shown = instance.first_frame + 1;
        if ui.add(DragValue::new(&mut shown).range(1..=duration)).changed() {
            instance.first_frame = shown - 1;
        }
        ui.end_row();
    });

    if let Some(layer) = editor.doc.layer_mut(id) {
        layer.instance = Some(instance);
    }
    if let Some(symbol) = editor.doc.symbol_mut(instance.symbol) {
        ui.horizontal(|ui| {
            ui.label("Symbol length");
            ui.add(DragValue::new(&mut symbol.duration).range(1..=100_000).suffix(" frames"))
                .on_hover_text("Changes the symbol everywhere it's used");
        });
    }
    ui.add_space(8.0);
}

/// The frames a layer appears on, shown 1-based like the timeline. The end
/// can be a fixed frame or "end", which follows the animation's length.
fn span_editor(ui: &mut egui::Ui, layer: &mut Layer, last_frame: u32) {
    ui.horizontal(|ui| {
        ui.label("Frames");
        let mut start = layer.start + 1;
        if ui.add(DragValue::new(&mut start).range(1..=last_frame + 1)).changed() {
            layer.set_start(start - 1);
        }
        ui.label("to");

        let mut open_ended = layer.end.is_none();
        if let Some(end) = layer.end {
            let mut shown = end + 1;
            if ui.add(DragValue::new(&mut shown).range(layer.start + 1..=last_frame + 1)).changed() {
                layer.set_end(Some(shown - 1));
            }
        }
        if ui
            .checkbox(&mut open_ended, "end")
            .on_hover_text("Stay until the animation ends, even if it gets longer")
            .changed()
        {
            layer.set_end(if open_ended { None } else { Some(last_frame) });
        }
    });
}

fn transform_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    let Some(layer) = editor.doc.layer(id) else { return };
    let has_key = |property: Property| layer.track.channel(property).has_key(frame);
    let keyed = Property::ALL.map(has_key);
    let mut pose = editor.doc.pose(layer, frame);

    ui.strong(format!("Transform at frame {}", frame + 1));

    // Scale and opacity are stored as fractions but edited as percentages,
    // which is what people expect to type.
    let mut scale_pct = [pose.scale.x * 100.0, pose.scale.y * 100.0];
    let mut opacity_pct = pose.opacity * 100.0;
    let mut changed = false;
    let mut toggled = None;

    // Each row starts with a diamond showing whether that property has a key
    // on this frame. Clicking it adds or removes the key. Editing a value
    // adds one automatically.
    egui::Grid::new("transform").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
        // Effects have a section of their own, further down.
        let transform = Property::ALL.into_iter().zip(keyed).filter(|(p, _)| *p != Property::Effects);
        for (property, is_key) in transform {
            if key_toggle(ui, property, is_key).clicked() {
                toggled = Some(property);
            }
            ui.label(property.name());
            ui.horizontal(|ui| match property {
                Property::Position => {
                    changed |= ui.add(DragValue::new(&mut pose.position.x).prefix("x ").speed(1.0)).changed();
                    changed |= ui.add(DragValue::new(&mut pose.position.y).prefix("y ").speed(1.0)).changed();
                }
                Property::Rotation => {
                    changed |= ui.add(DragValue::new(&mut pose.rotation).suffix("°").speed(1.0)).changed();
                }
                Property::Scale => {
                    for (axis, value) in ["x ", "y "].into_iter().zip(&mut scale_pct) {
                        changed |=
                            ui.add(DragValue::new(value).prefix(axis).suffix("%").speed(1.0)).changed();
                    }
                }
                Property::Skew => {
                    // At ±90° a layer would slant into a line.
                    for (axis, value) in ["x ", "y "].into_iter().zip([&mut pose.skew.x, &mut pose.skew.y]) {
                        let drag =
                            DragValue::new(value).prefix(axis).suffix("°").speed(0.5).range(-89.0..=89.0);
                        changed |= ui.add(drag).changed();
                    }
                }
                Property::Opacity => {
                    changed |= ui.add(egui::Slider::new(&mut opacity_pct, 0.0..=100.0).suffix("%")).changed();
                }
                Property::Effects => {}
            });
            ui.end_row();
        }
    });

    if changed {
        pose.scale.x = scale_pct[0] / 100.0;
        pose.scale.y = scale_pct[1] / 100.0;
        pose.opacity = opacity_pct / 100.0;
        editor.set_pose(id, pose);
    }
    if let Some(property) = toggled {
        editor.toggle_property_key(id, property);
    }
    motion_path_section(ui, editor, id);
    mask_section(ui, editor, id);
}

/// How the layer hangs in a character rig: the layer it's attached to, and
/// the pivot it turns around.
fn rig_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    let Some(layer) = editor.doc.layer(id) else { return };
    // Only layers it can attach to without making a loop.
    let parents: Vec<(LayerId, String)> = editor
        .layers()
        .iter()
        .filter(|other| editor.doc.can_parent(id, other.id))
        .map(|other| (other.id, other.name.clone()))
        .collect();
    let (old_parent, old_pivot, old_limits) = (layer.parent, layer.pivot, layer.rotation_limits);
    let (mut parent, mut pivot, mut limits) = (old_parent, old_pivot, old_limits);
    let rotation = editor.doc.pose(layer, frame).rotation;
    let bounds = editor.doc.layer_bounds(layer, frame);

    egui::Grid::new("rig").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label("Parent").on_hover_text(
            "The layer this one is attached to. It moves, turns and scales with its parent, \
             and its own position is measured from the parent.",
        );
        let current = parents.iter().find(|(p, _)| Some(*p) == parent).map_or("None", |(_, name)| name);
        egui::ComboBox::from_id_salt("parent").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut parent, None, "None");
            for (other, name) in &parents {
                ui.selectable_value(&mut parent, Some(*other), name);
            }
        });
        ui.end_row();

        ui.label("Pivot").on_hover_text(
            "The point the layer turns and scales around, in its own coordinates. \
             Option-drag (Alt-drag) the ring on the stage to move it.",
        );
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut pivot.x).prefix("x ").speed(1.0));
            ui.add(DragValue::new(&mut pivot.y).prefix("y ").speed(1.0));
            if let Some(bounds) = bounds
                && ui
                    .small_button("Center")
                    .on_hover_text("Put the pivot in the middle of the layer")
                    .clicked()
            {
                pivot = bounds.center();
            }
        });
        ui.end_row();

        // Only joints bend, so only attached layers have limits.
        if parent.is_some() {
            ui.label("Bends").on_hover_text(
                "How far the Bone tool may turn this joint, as the lowest and highest rotation, \
                 so an elbow or knee only bends one way.",
            );
            ui.horizontal(|ui| {
                let mut limited = limits.is_some();
                if ui.checkbox(&mut limited, "").changed() {
                    // Start from a range around where it is now.
                    limits = limited.then_some(RotationLimits { min: rotation - 45.0, max: rotation + 45.0 });
                }
                match &mut limits {
                    Some(range) => {
                        ui.add(DragValue::new(&mut range.min).suffix("°").speed(1.0));
                        ui.label("to");
                        ui.add(DragValue::new(&mut range.max).suffix("°").speed(1.0));
                    }
                    None => {
                        ui.weak("freely");
                    }
                }
            });
            ui.end_row();
        }
    });

    // Both keep the layer where it is on screen: attaching it re-measures
    // its keys from the parent, and moving the pivot shifts its position.
    if parent != old_parent {
        editor.doc.set_parent(id, parent, frame);
    }
    if pivot != old_pivot {
        editor.doc.set_pivot(id, pivot);
    }
    if limits != old_limits
        && let Some(layer) = editor.doc.layer_mut(id)
    {
        layer.rotation_limits = limits;
    }
}

/// A bone deformer bending the layer's drawing: adding one, its bones'
/// bends on this frame, and how soft the bends are.
fn deformer_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer_mut(id).filter(|l| l.instance.is_none()) else { return };
    // The drawing as drawn, not bent, is what the bones are laid along.
    let rest_bounds = layer.elements_at(frame).iter().map(|e| e.bounds()).reduce(|a, b| a.union(b));

    ui.strong("Deformer");
    let Some(deformer) = &mut layer.deformer else {
        let add = ui
            .add_enabled(rest_bounds.is_some(), egui::Button::new("Add Bend Deformer"))
            .on_hover_text("Bones through the drawing that bend it smoothly, for rubber-hose limbs and tails. Pose them with the Bone tool.");
        if add.clicked()
            && let Some(bounds) = rest_bounds
        {
            layer.deformer = Some(Deformer::along(bounds, 2, frame));
        }
        ui.add_space(8.0);
        return;
    };

    let mut remove = false;
    let mut bones = deformer.bone_count();
    egui::Grid::new("deformer").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label("Bones").on_hover_text("Changing the count lays the bones out straight again");
        ui.add(DragValue::new(&mut bones).range(1..=8));
        ui.end_row();
        ui.label("Smoothness").on_hover_text("How soft each bend is: 0 bends like a hinge, 1 like a hose");
        ui.add(egui::Slider::new(&mut deformer.smoothness, 0.0..=1.0));
        ui.end_row();
        for (i, mut bend) in deformer.bends_at(frame).into_iter().enumerate() {
            ui.label(format!("Bend {}", i + 1));
            if ui.add(DragValue::new(&mut bend).suffix("°").speed(1.0)).changed() {
                deformer.bends[i].set(frame, bend);
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        ui.weak("Drag its joints with the Bone tool (K).");
        remove = ui.small_button("Remove").clicked();
    });
    if bones != deformer.bone_count()
        && let Some(bounds) = rest_bounds
    {
        *deformer = Deformer { smoothness: deformer.smoothness, ..Deformer::along(bounds, bones, frame) };
    }
    if remove {
        layer.deformer = None;
    }
    ui.add_space(8.0);
}

/// The free-form deformer: a grid of control points over the drawing that
/// warp it when dragged (see `warp.rs` in the core).
fn warp_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer_mut(id).filter(|l| l.instance.is_none()) else { return };
    let rest_bounds = layer.elements_at(frame).iter().map(|e| e.bounds()).reduce(|a, b| a.union(b));

    ui.strong("Free-Form Deformer");
    let Some(warp) = &mut layer.warp else {
        ui.horizontal(|ui| {
            for size in [3, 4] {
                let add = ui
                    .add_enabled(rest_bounds.is_some(), egui::Button::new(format!("Add {size}×{size} Grid")))
                    .on_hover_text("A grid of points over the drawing: drag them with the Bone tool to squash, bulge and sway it.");
                if add.clicked()
                    && let Some(bounds) = rest_bounds
                {
                    layer.warp = Some(inkframe_core::Warp::over(bounds, size, size, frame));
                }
            }
        });
        ui.add_space(8.0);
        return;
    };
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.weak(format!("{}×{} grid. Drag its points with the Bone tool (K).", warp.columns, warp.rows));
        if ui.small_button("Straighten").on_hover_text("Put every point back on this frame").clicked() {
            for channel in &mut warp.offsets {
                if channel.sample(frame as f32).is_some_and(|o| o != inkframe_core::Point::ZERO)
                    || channel.key_at(frame).is_some()
                {
                    channel.set(frame, inkframe_core::Point::ZERO);
                }
            }
        }
        remove = ui.small_button("Remove").clicked();
    });
    if remove {
        layer.warp = None;
    }
    ui.add_space(8.0);
}

/// The layer's drawings, for swapping between them over time: click one
/// to show it from the current frame on.
fn drawings_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer(id).filter(|l| l.instance.is_none()) else { return };
    let showing = layer.drawing_index_at(frame);
    let names: Vec<String> = layer.drawings.iter().map(|d| d.name.clone()).collect();

    ui.horizontal(|ui| {
        ui.strong("Drawings");
        ui.weak("[ ] to flip through");
    });
    if names.len() == 1 {
        ui.weak("Add drawings to swap between, like mouth shapes or hand poses.");
    }
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (index, name) in names.iter().enumerate() {
            if ui.selectable_label(index == showing, name).clicked() {
                picked = Some(index);
            }
        }
    });
    let (mut add, mut copy, mut delete) = (false, false, false);
    ui.horizontal(|ui| {
        add = ui
            .button("New")
            .on_hover_text("A blank drawing, shown from this frame. Draw into it on the stage.")
            .clicked();
        copy = ui
            .button("Duplicate")
            .on_hover_text("A copy of the drawing showing now, to change a little")
            .clicked();
        let can_delete = names.len() > 1;
        delete = ui
            .add_enabled(can_delete, egui::Button::new("Delete"))
            .on_hover_text("Remove the drawing showing now")
            .clicked();
    });
    if let Some(drawing) = editor.doc.layer_mut(id).and_then(|l| l.drawings.get_mut(showing)) {
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut drawing.name);
        });
    }

    if let Some(index) = picked {
        editor.show_drawing(index);
    }
    if add || copy {
        editor.new_drawing(copy);
    }
    if delete {
        editor.delete_shown_drawing();
    }
    shape_tween_controls(ui, editor);
    ui.add_space(8.0);
}

/// The shape tween through this frame's span: whether it morphs into the
/// next swap, its easing, and its shape hints.
fn shape_tween_controls(ui: &mut egui::Ui, editor: &mut Editor) {
    let Some(mut on) = editor.shape_tween_here() else { return };
    ui.add_space(4.0);
    if ui
        .checkbox(&mut on, "Shape tween into the next drawing")
        .on_hover_text("Morph from the drawing showing here into the next one, instead of swapping at once")
        .changed()
    {
        editor.set_shape_tween(on);
    }
    let frame = editor.frame;
    let Some(tween) = editor.selected_layer_mut().and_then(|l| l.shape_tween_mut(frame)) else { return };
    ui.horizontal(|ui| {
        ui.label("Easing");
        egui::ComboBox::from_id_salt("shape_tween_easing").selected_text(tween.easing.label()).show_ui(
            ui,
            |ui| {
                for preset in Easing::PRESETS {
                    ui.selectable_value(&mut tween.easing, preset, preset.label());
                }
            },
        );
    });
    let hints = editor
        .shape_tween_drawings()
        .map_or(0, |(from, _)| editor.selected_layer().map_or(0, |l| l.drawings[from].hints.len()));
    ui.horizontal(|ui| {
        if ui
            .button("Add Shape Hint")
            .on_hover_text("⌘⇧H. Drag the lettered marks onto matching points of the two drawings")
            .clicked()
        {
            editor.add_shape_hint();
        }
        if ui.add_enabled(hints > 0, egui::Button::new("Remove Hints")).clicked() {
            editor.clear_shape_hints();
        }
    });
}

/// Which mask, if any, the layer is seen through.
fn mask_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let masks: Vec<(LayerId, String)> =
        editor.doc.masks_beside(id).into_iter().map(|m| (m.id, m.name.clone())).collect();
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    if layer.mask || (masks.is_empty() && layer.masked_by.is_none()) {
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Masked by");
        let current =
            masks.iter().find(|(m, _)| Some(*m) == layer.masked_by).map_or("None", |(_, name)| name);
        egui::ComboBox::from_id_salt("masked_by").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut layer.masked_by, None, "None");
            for (mask, name) in &masks {
                ui.selectable_value(&mut layer.masked_by, Some(*mask), name);
            }
        });
    });
}

/// Options for a layer that moves: following a motion guide, turning to
/// face its direction of travel, and undoing any curves in its motion path.
fn motion_path_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let guides: Vec<(LayerId, String)> =
        editor.doc.guides_beside(id).into_iter().map(|g| (g.id, g.name.clone())).collect();
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    if layer.guide {
        return;
    }

    if !guides.is_empty() || layer.follow.is_some() {
        ui.horizontal(|ui| {
            ui.label("Follow guide");
            let current =
                guides.iter().find(|(g, _)| Some(*g) == layer.follow).map_or("None", |(_, name)| name);
            egui::ComboBox::from_id_salt("follow").selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut layer.follow, None, "None");
                for (guide, name) in &guides {
                    ui.selectable_value(&mut layer.follow, Some(*guide), name);
                }
            });
        });
        if layer.follow.is_some() && layer.track.position.keys().len() < 2 {
            ui.weak(
                "Add a second position keyframe: the layer travels the guide between its first and last.",
            );
        }
    }

    let track = &mut layer.track;
    if track.position.keys().len() < 2 {
        return;
    }
    let following = layer.follow.is_some();
    ui.horizontal(|ui| {
        ui.checkbox(&mut track.orient_to_path, "Orient to path")
            .on_hover_text("Turn the layer to face the way it's moving");
        // A layer on a guide takes its path from the guide.
        let curved = !following && track.position.keys().iter().any(|k| k.bend.is_some());
        if ui
            .add_enabled(curved, egui::Button::new("Straighten path").small())
            .on_hover_text("Make every tween a straight line again. Drag the path on the stage to curve it.")
            .clicked()
        {
            track.position.straighten();
        }
    });
}

/// A small diamond button: filled when the property has a key on this frame.
fn key_toggle(ui: &mut egui::Ui, property: Property, is_key: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
    let accent = Color32::from_rgb(0x2f, 0x9b, 0xff);
    let color = if response.hovered() { ui.visuals().strong_text_color() } else { accent };
    let c = rect.center();
    let r = 5.0;
    let points = vec![c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)];
    let fill = if is_key { color } else { Color32::TRANSPARENT };
    ui.painter().add(egui::Shape::convex_polygon(points, fill, EguiStroke::new(1.2, color)));

    let tip = if is_key {
        format!("Remove the {} keyframe on this frame", property.name().to_lowercase())
    } else {
        format!("Add a {} keyframe on this frame", property.name().to_lowercase())
    };
    response.on_hover_text(tip)
}

fn easing_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    let Some(layer) = editor.doc.layer(id) else { return };
    let tweens: Vec<(Property, Tween)> = Property::ALL
        .into_iter()
        .filter_map(|p| layer.track.channel(p).tween_at(frame).map(|t| (p, t)))
        .collect();
    if tweens.is_empty() {
        ui.weak("No tweens here. Move to a later frame and change a property to create one.");
        return;
    }
    // Keep the graph on a property that's actually tweening here.
    if !tweens.iter().any(|(p, _)| *p == editor.focused_property) {
        editor.focused_property = tweens[0].0;
    }

    ui.strong("Easing");
    let mut changes = Vec::new();
    egui::Grid::new("easing").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
        for (property, tween) in &tweens {
            let focused = editor.focused_property == *property;
            if ui.selectable_label(focused, property.name()).on_hover_text("Show this curve below").clicked()
            {
                editor.focused_property = *property;
            }
            ui.weak(format!("{}–{}", tween.start + 1, tween.end + 1));
            let mut easing = tween.easing;
            if easing_combo(ui, *property, &mut easing) {
                changes.push((*property, easing));
            }
            ui.end_row();
        }
    });

    let focused = editor.focused_property;
    let Some(tween) = tweens.iter().find(|(p, _)| *p == focused).map(|(_, t)| *t) else { return };
    let mut easing = tween.easing;
    let progress = (frame - tween.start) as f32 / (tween.end - tween.start).max(1) as f32;
    if easing_editor::show(ui, &mut easing, Some(progress)) {
        changes.push((focused, easing));
    }
    // Exact numbers, for matching a curve from elsewhere (CSS, say).
    if let Easing::CubicBezier { x1, y1, x2, y2 } = &mut easing {
        let mut edited = false;
        ui.horizontal(|ui| {
            edited |= ui.add(DragValue::new(x1).range(0.0..=1.0).speed(0.01).prefix("x1 ")).changed();
            edited |= ui.add(DragValue::new(y1).speed(0.01).prefix("y1 ")).changed();
            edited |= ui.add(DragValue::new(x2).range(0.0..=1.0).speed(0.01).prefix("x2 ")).changed();
            edited |= ui.add(DragValue::new(y2).speed(0.01).prefix("y2 ")).changed();
        });
        if edited {
            changes.push((focused, easing));
        }
    }

    if let Some(layer) = editor.doc.layer_mut(id) {
        for (property, easing) in changes {
            if let Some(e) = layer.track.channel_mut(property).tween_easing_mut(frame) {
                *e = easing;
            }
        }
    }
}

/// The easing picker. Returns true when the choice changed.
fn easing_combo(ui: &mut egui::Ui, id_salt: Property, easing: &mut Easing) -> bool {
    let before = *easing;
    egui::ComboBox::from_id_salt(("easing", id_salt)).selected_text(easing.label()).show_ui(ui, |ui| {
        for preset in Easing::PRESETS {
            ui.selectable_value(easing, preset, preset.label());
        }
        // "Custom" starts from the current curve so it's a tweak, not a reset.
        let is_custom = matches!(easing, Easing::CubicBezier { .. });
        if ui.selectable_label(is_custom, "Custom").clicked() && !is_custom {
            let [x1, y1, x2, y2] = easing.control_points().unwrap_or([0.0, 0.0, 1.0, 1.0]);
            *easing = Easing::CubicBezier { x1, y1, x2, y2 };
        }
    });
    *easing != before
}

fn shapes_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    // What the image rows show, gathered before the layer is borrowed for
    // editing (it lives inside the same document).
    let images: Vec<ImageInfo> = editor
        .doc
        .images
        .iter()
        .map(|asset| ImageInfo {
            id: asset.id,
            name: asset.name.clone(),
            size: (asset.width, asset.height),
            problem: editor.images.problem(asset.id).map(str::to_owned),
        })
        .collect();

    // Why each family from a project font file is unavailable, if it is.
    let font_problems: Vec<(String, String)> = editor
        .font_problems
        .iter()
        .filter_map(|(id, problem)| {
            editor.doc.fonts.iter().find(|f| f.id == *id).map(|f| (&f.families, problem))
        })
        .flat_map(|(families, problem)| families.iter().map(move |family| (family.clone(), problem.clone())))
        .collect();
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    let elements = layer.elements_at_mut(frame);
    let count = elements.len();
    for (i, element) in elements.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            let title = match &element.shape {
                Shape::Rect { .. } => "Rectangle",
                Shape::Ellipse { .. } => "Oval",
                Shape::Path(_) => "Stroke",
                Shape::Line { .. } => "Pencil Line",
                Shape::Image { .. } => "Image",
                Shape::Text(_) => "Text",
                Shape::Paint { .. } => "Painting",
            };
            ui.strong(if count > 1 { format!("{title} {}", i + 1) } else { title.to_owned() });
            if let Shape::Paint { width, height, .. } = element.shape {
                // Painted with the brush tools, so nothing to set here
                // beyond saying how big the canvas is.
                ui.weak(format!("{width} × {height} px canvas. Paint with the Brush (B) and Eraser (E)."));
            } else if let Shape::Image { image, width, height } = &mut element.shape {
                // An image paints its own pixels, so it has no fill or stroke.
                if let Some(info) = images.iter().find(|info| info.id == *image) {
                    image_editor(ui, info, width, height);
                }
            } else if let Shape::Text(text) = &mut element.shape {
                let problem =
                    font_problems.iter().find(|(f, _)| *f == text.font.family).map(|(_, p)| p.as_str());
                text_editor(ui, text, problem);
                style_editor(ui, &mut element.style);
            } else {
                shape_size_editor(ui, &mut element.shape);
                style_editor(ui, &mut element.style);
            }
        });
    }
}

struct ImageInfo {
    id: ImageId,
    name: String,
    /// The file's own size in pixels.
    size: (u32, u32),
    problem: Option<String>,
}

fn image_editor(ui: &mut egui::Ui, info: &ImageInfo, width: &mut f32, height: &mut f32) {
    egui::Grid::new("image").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("File");
        match &info.problem {
            Some(problem) => ui.colored_label(ui.visuals().error_fg_color, problem),
            None => ui.label(&info.name),
        };
        ui.end_row();
        ui.label("Size");
        ui.horizontal(|ui| {
            ui.add(DragValue::new(width).range(0.0..=f32::MAX).prefix("w "));
            ui.add(DragValue::new(height).range(0.0..=f32::MAX).prefix("h "));
        });
        ui.end_row();
        ui.label("");
        let (w, h) = info.size;
        if ui
            .button("Actual size")
            .on_hover_text(format!("{w} × {h}, one stage pixel per image pixel"))
            .clicked()
        {
            (*width, *height) = (w as f32, h as f32);
        }
        ui.end_row();
    });
}

/// The text itself and its font settings. (Text can also be typed straight
/// onto the stage; see `text_edit.rs`.) `font_problem` explains why the
/// project's own font file for this family couldn't be used, if that's the case.
fn text_editor(ui: &mut egui::Ui, text: &mut Text, font_problem: Option<&str>) {
    egui::TextEdit::multiline(&mut text.content).desired_rows(2).desired_width(f32::INFINITY).show(ui);

    egui::Grid::new("text").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Font");
        ui.horizontal(|ui| {
            let families = inkframe_render::font_families();
            egui::ComboBox::from_id_salt("font").selected_text(&text.font.family).width(160.0).show_ui(
                ui,
                |ui| {
                    // The project's own fonts first, where they're easy to find
                    // among hundreds of installed ones.
                    if !families.project.is_empty() {
                        ui.weak("This project");
                        for family in &families.project {
                            ui.selectable_value(&mut text.font.family, family.clone(), family);
                        }
                        ui.separator();
                        ui.weak("Installed");
                    }
                    for family in &families.installed {
                        ui.selectable_value(&mut text.font.family, family.clone(), family);
                    }
                },
            );
            if ui
                .small_button("Refresh")
                .on_hover_text("Look again for fonts installed since Inkframe started")
                .clicked()
            {
                inkframe_render::refresh_installed_fonts();
            }
        });
        ui.end_row();
        // Name the stand-in when the chosen font isn't available (a project
        // from someone else's computer, say), and why, if it's known.
        let used = inkframe_render::layout_text(text).family.clone();
        if used != text.font.family {
            let reason = font_problem.unwrap_or("Not installed");
            ui.label("");
            ui.colored_label(ui.visuals().warn_fg_color, format!("{reason}; showing {used}"));
            ui.end_row();
        }

        ui.label("Style");
        ui.horizontal(|ui| {
            ui.toggle_value(&mut text.font.bold, "Bold");
            ui.toggle_value(&mut text.font.italic, "Italic");
        });
        ui.end_row();
        ui.label("Size");
        ui.add(DragValue::new(&mut text.size).range(1.0..=2000.0).suffix(" px"));
        ui.end_row();
        ui.label("Align");
        ui.horizontal(|ui| {
            for align in TextAlign::ALL {
                ui.selectable_value(&mut text.align, align, align.label());
            }
        });
        ui.end_row();
        ui.label("Line spacing");
        ui.add(DragValue::new(&mut text.line_spacing).range(0.5..=5.0).speed(0.01).suffix("×"));
        ui.end_row();
    });
}

fn shape_size_editor(ui: &mut egui::Ui, shape: &mut Shape) {
    egui::Grid::new("shape").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match shape {
        Shape::Rect { width, height, corner_radius } => {
            ui.label("Size");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(width).range(0.0..=f32::MAX).prefix("w "));
                ui.add(DragValue::new(height).range(0.0..=f32::MAX).prefix("h "));
            });
            ui.end_row();
            ui.label("Corners");
            ui.add(DragValue::new(corner_radius).range(0.0..=f32::MAX).suffix(" px"));
            ui.end_row();
        }
        Shape::Ellipse { radius_x, radius_y } => {
            ui.label("Radius");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(radius_x).range(0.0..=f32::MAX).prefix("x "));
                ui.add(DragValue::new(radius_y).range(0.0..=f32::MAX).prefix("y "));
            });
            ui.end_row();
        }
        // Freehand paths are edited by redrawing, for now. Images and text
        // have their own editors.
        // A pencil line's widths are set with the Width tool, on the stage;
        // here, all of them at once.
        Shape::Line { widths, .. } => {
            ui.label("Width")
                .on_hover_text("Scales the whole line; the Width tool (Shift+A) shapes it point by point");
            let widest = widths.iter().copied().fold(0.0f32, f32::max).max(0.01);
            let mut shown = widest;
            if ui.add(DragValue::new(&mut shown).range(0.1..=200.0).speed(0.1).suffix(" px")).changed() {
                let factor = shown / widest;
                widths.iter_mut().for_each(|w| *w *= factor);
            }
            ui.end_row();
        }
        Shape::Path(_) | Shape::Image { .. } | Shape::Text(_) | Shape::Paint { .. } => {}
    });
}

/// Fill and stroke controls. Shared by selected shapes and the default
/// style for new shapes.
pub fn style_editor(ui: &mut egui::Ui, style: &mut Style) {
    egui::Grid::new("style").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Fill");
        optional_color(ui, &mut style.fill, Color::rgb(0x3d, 0x8b, 0xfd));
        ui.end_row();

        ui.label("Stroke");
        ui.horizontal(|ui| {
            let mut enabled = style.stroke.is_some();
            if ui.checkbox(&mut enabled, "").changed() {
                style.stroke = enabled.then_some(Stroke { color: Color::BLACK, width: 2.0 });
            }
            if let Some(stroke) = &mut style.stroke {
                color_button(ui, &mut stroke.color);
                ui.add(DragValue::new(&mut stroke.width).range(0.0..=200.0).speed(0.1).suffix(" px"));
            }
        });
        ui.end_row();
    });
}

/// A checkbox that turns a color on or off, plus the color button.
fn optional_color(ui: &mut egui::Ui, color: &mut Option<Color>, default: Color) {
    ui.horizontal(|ui| {
        let mut enabled = color.is_some();
        if ui.checkbox(&mut enabled, "").changed() {
            *color = enabled.then_some(default);
        }
        if let Some(c) = color {
            color_button(ui, c);
        }
    });
}

fn document_section(ui: &mut egui::Ui, editor: &mut Editor) {
    let doc = &mut editor.doc;
    egui::Grid::new("document").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Stage size");
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut doc.width).range(1..=16384).suffix(" w"));
            ui.add(DragValue::new(&mut doc.height).range(1..=16384).suffix(" h"));
        });
        ui.end_row();

        ui.label("Frame rate");
        egui::ComboBox::from_id_salt("frame_rate").selected_text(doc.frame_rate.to_string()).show_ui(
            ui,
            |ui| {
                for rate in FrameRate::PRESETS {
                    ui.selectable_value(&mut doc.frame_rate, rate, rate.to_string());
                }
            },
        );
        ui.end_row();

        ui.label("Duration");
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut doc.duration).range(1..=100_000).suffix(" frames"));
            ui.weak(format!("{:.2}s", doc.duration as f64 / doc.frame_rate.fps()));
        });
        ui.end_row();

        ui.label("Background");
        color_button(ui, &mut doc.background);
        ui.end_row();
    });
    // Shortening the animation can strand the playhead past the end.
    editor.set_frame(editor.frame);
}

/// The document's audio tracks: when each starts, how loud it is, and
/// whether it plays at all. Sound files are added with File › Import Audio.
pub fn audio_section(ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) -> Option<AudioAction> {
    let mut action = None;
    let header = format!("Audio ({})", editor.doc.audio.len());
    egui::CollapsingHeader::new(header).id_salt("audio").default_open(!editor.doc.audio.is_empty()).show(
        ui,
        |ui| {
            if let Some(error) = audio.output_error() {
                ui.colored_label(ui.visuals().warn_fg_color, format!("No sound output: {error}"));
            }
            ui.checkbox(&mut audio.scrub, "Scrub audio")
                .on_hover_text("Play each frame's sound as you move the playhead, for lip sync");

            let last_frame = editor.doc.last_frame();
            let mut remove = None;
            for track in &mut editor.doc.audio {
                ui.push_id(track.id, |ui| {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut track.name).desired_width(150.0));
                        if ui
                            .add(crate::icons::IconButton::new(
                                crate::icons::Icon::Trash,
                                "Remove this audio",
                            ))
                            .clicked()
                        {
                            remove = Some(track.id);
                        }
                    });
                    egui::Grid::new("audio_track").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("Starts at");
                        let mut shown = track.start + 1;
                        if ui
                            .add(DragValue::new(&mut shown).range(1..=last_frame + 1).prefix("frame "))
                            .changed()
                        {
                            track.start = shown - 1;
                        }
                        ui.end_row();

                        ui.label("Volume");
                        let mut percent = track.volume * 100.0;
                        if ui.add(egui::Slider::new(&mut percent, 0.0..=200.0).suffix("%")).changed() {
                            track.volume = percent / 100.0;
                        }
                        ui.end_row();

                        ui.label("");
                        ui.checkbox(&mut track.muted, "Mute");
                        ui.end_row();
                    });
                    ui.weak(track.path.display().to_string());
                });
            }
            if let Some(id) = remove {
                editor.doc.remove_audio(id);
            }

            ui.add_space(4.0);
            if ui.button("Import Audio…").clicked() {
                action = Some(AudioAction::Import);
            }
        },
    );
    action
}

/// Something the audio section asks the app to do, because it needs a
/// file dialog.
pub enum AudioAction {
    Import,
}
