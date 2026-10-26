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
use crate::i18n::{tr, tr_args, tr_later};
use crate::icons::Icon;

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
        for (choice, label) in
            [(Tab::Tool, tr("Tool")), (Tab::Layer, tr("Layer")), (Tab::Document, tr("Document"))]
        {
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
    // With several layers selected, the panel shows the one picked last,
    // and what's changed here is carried to the rest (see `multi_edit.rs`).
    let count = editor.selected_ids().len();
    if count > 1 {
        ui.label(
            egui::RichText::new(tr_args("{count} layers selected", &[("count", &count.to_string())]))
                .strong(),
        );
        ui.weak(tr(
            "Showing the last one picked. Changes to the layer and transform settings apply to all of them: \
             position moves them together, other values are set on each. • marks a value that differs.",
        ));
        ui.add_space(4.0);
    }
    let Some(id) = editor.selection else {
        ui.weak(tr("Select a layer on the stage or in the timeline to see its properties."));
        return;
    };
    search_box(ui, editor);
    editor.properties_matches = 0;
    layer_section(ui, editor, id);
    if !editor.properties_search.trim().is_empty() && editor.properties_matches == 0 {
        ui.add_space(8.0);
        ui.weak(tr_args(
            "No settings of this layer match “{search}”.",
            &[("search", editor.properties_search.trim())],
        ));
    }
}

/// The search box at the top of the layer tab: typing in it shows only the
/// sections with matching settings (see `searched_for`).
fn search_box(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        crate::icons::paint(ui.painter(), Icon::Search, rect, crate::theme::TEXT_WEAK);
        let searching = !editor.properties_search.is_empty();
        let width = ui.available_width() - if searching { 26.0 } else { 0.0 };
        let field = ui.add(
            egui::TextEdit::singleline(&mut editor.properties_search)
                .hint_text(tr("Search settings"))
                .desired_width(width),
        );
        // Escape clears it, as in most search boxes.
        if field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            editor.properties_search.clear();
        }
        if searching && ui.small_button("×").on_hover_text(tr("Clear the search")).clicked() {
            editor.properties_search.clear();
        }
    });
    ui.add_space(2.0);
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
        ui.heading(editor.tool.label());
        egui::Grid::new("pencil").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr("Line"));
            color_button(ui, &mut editor.pencil_stroke.color);
            ui.end_row();
            ui.label(tr("Width"));
            ui.add(DragValue::new(&mut editor.pencil_stroke.width).range(0.5..=100.0).speed(0.1));
            ui.end_row();
        });
        ui.checkbox(&mut editor.pencil_pressure, tr("Pressure"))
            .on_hover_text(tr("Press harder with a pen for a thicker line, lighter for a thinner one"));
        ui.checkbox(&mut editor.pencil_taper, tr("Taper ends"))
            .on_hover_text(tr("Lines thin out towards their ends, as inked lines do"));
        if editor.pencil_pressure || editor.pencil_taper {
            ui.weak(tr("Shape a line's thickness afterwards with the Width tool (Shift+A)."));
        }
        drawing_target(ui, editor);
    } else if matches!(editor.tool, Tool::Rectangle | Tool::Ellipse | Tool::Pen | Tool::Text) {
        ui.heading(editor.tool.label());
        shape_style(ui, editor);
        if editor.tool != Tool::Text {
            drawing_target(ui, editor);
        }
    } else {
        ui.weak(crate::i18n::tr_args("The {tool} tool has no settings.", &[("tool", editor.tool.label())]));
    }
}

/// Where the drawing tools put new shapes: into the selected layer's
/// drawing (as in Animate and Harmony), or each on a layer of its own, and
/// then into its line art or the colour art beneath.
fn drawing_target(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.add_space(6.0);
    ui.checkbox(&mut editor.draw_into_layer, tr("Draw into the selected layer")).on_hover_text(
        tr("Strokes and shapes go into the selected layer, so a drawing builds up on one layer. With no layer \
         selected (or a locked one, a symbol or a paint layer), they start a new layer. Off, every shape gets \
         a layer of its own."),
    );
    ui.add_enabled_ui(editor.draw_into_layer, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut editor.colour_art, false, tr("Line Art"));
            ui.selectable_value(&mut editor.colour_art, true, tr("Colour Art"))
                .on_hover_text(tr("Beneath the lines, where the paint bucket's fills go"));
        });
    });
}

/// Fill, stroke and stroke width for the shapes the drawing tools make.
fn shape_style(ui: &mut egui::Ui, editor: &mut Editor) {
    let symbols = brush_symbols(editor);
    style_editor(ui, &mut editor.draw_style, &symbols);
}

/// The library's symbols, to draw strokes with as brushes.
fn brush_symbols(editor: &Editor) -> Vec<(inkframe_core::SymbolId, String)> {
    editor.doc.symbols.iter().map(|s| (s.id, s.name.clone())).collect()
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
        if matches!(tool, Tool::Brush | Tool::Mixer | Tool::PaintBucket | Tool::Gradient | Tool::Eyedropper) {
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
                    editor.show_brush_settings = true;
                }
                ui.end_row();
            }
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

fn brush_controls(ui: &mut egui::Ui, settings: &mut inkframe_core::BrushSettings, paint: bool) {
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
fn brush_preset_menu(ui: &mut egui::Ui, editor: &mut Editor, tool: crate::editor::Tool) {
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

/// The selection tools' options, and what to do with a selection.
fn selection_section(ui: &mut egui::Ui, editor: &mut Editor) {
    use crate::editor::Tool;
    ui.heading(editor.tool.label());
    if editor.tool == Tool::MagicWand {
        egui::Grid::new("wand").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr("Tolerance"))
                .on_hover_text(tr("How different a colour may be and still be selected"));
            ui.add(egui::Slider::new(&mut editor.wand_tolerance, 0..=255));
            ui.end_row();
            ui.label(tr("Contiguous")).on_hover_text(tr("Select only the area connected to where you click"));
            ui.checkbox(&mut editor.wand_contiguous, "");
            ui.end_row();
        });
    }
    ui.weak(match editor.tool {
        Tool::PolygonLasso => {
            tr("Click each corner; click the first again, double-click or press Enter to close.")
        }
        Tool::MagicWand => tr("Click an area of colour to select it."),
        _ => tr("Drag to select."),
    });
    ui.weak(tr("Shift adds to the selection, Option/Alt takes away, both keep the overlap."));
    let selected = editor.active_selection().is_some();
    ui.horizontal(|ui| {
        if ui.add_enabled(editor.paint_target().is_some(), egui::Button::new(tr("All"))).clicked() {
            editor.select_all_pixels();
        }
        if ui.add_enabled(selected, egui::Button::new(tr("Deselect"))).clicked() {
            editor.deselect_pixels();
        }
        if ui.add_enabled(editor.paint_target().is_some(), egui::Button::new(tr("Inverse"))).clicked() {
            editor.invert_pixel_selection();
        }
    });
    if selected {
        ui.weak(tr(
            "Drag inside the selection with the Selection tool (V) to move those pixels; Delete clears them.",
        ));
    }
}

/// A slider for a 0–1 setting, shown as a percentage.
pub(crate) fn percent(value: &mut f32) -> egui::Slider<'_> {
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
    ui.heading(tr("Camera"));
    if ui
        .checkbox(&mut on, tr("On"))
        .on_hover_text(tr("Off, the stage shows the scene without the camera"))
        .changed()
    {
        editor.set_visible(id, on);
    }
    let before = pose;
    let mut zoom = pose.scale.x * 100.0;
    let dolly_before = editor.doc.layer(id).map_or(0.0, |c| c.track.depth_at(frame as f32));
    let mut dolly = dolly_before;
    egui::Grid::new("camera").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Looks at")).on_hover_text(tr("The point of the scene in the middle of the stage"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut pose.position.x).prefix("x ").speed(1.0));
            ui.add(DragValue::new(&mut pose.position.y).prefix("y ").speed(1.0));
        });
        ui.end_row();
        ui.label(tr("Zoom"));
        ui.add(DragValue::new(&mut zoom).range(5.0..=5000.0).suffix("%").speed(1.0));
        ui.end_row();
        ui.label(tr("Rotation"));
        ui.add(DragValue::new(&mut pose.rotation).suffix("°").speed(0.5));
        ui.end_row();
        ui.label(tr("Dolly")).on_hover_text(tr(
            "How far the camera has moved in among the layers. Unlike zoom, near layers grow \
             faster than far ones, as with a real multiplane camera; layers it reaches fade away.",
        ));
        ui.add(DragValue::new(&mut dolly).suffix(" px").speed(5.0));
        ui.end_row();
    });
    if dolly != dolly_before {
        editor.set_depth(id, dolly);
    }
    pose.scale = inkframe_core::Point::new(zoom / 100.0, zoom / 100.0);
    if pose != before {
        editor.set_pose(id, pose);
    }
    if ui
        .button(tr("Reset Camera"))
        .on_hover_text(tr("Back to looking straight at the stage, on this frame"))
        .clicked()
    {
        let centre = editor.doc.stage_rect().center();
        editor.set_pose(id, inkframe_core::Transform::at(centre));
    }
    ui.add_space(4.0);
    ui.weak(tr("With the Camera tool (C): drag to pan, Shift-drag to turn, Alt-drag to zoom. Set each layer's Depth for parallax."));
}

/// A folder: its name, and showing or locking everything in it.
fn folder_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some(folder) = editor.doc.layer_mut(id) else { return };
    ui.heading(tr("Folder"));
    ui.text_edit_singleline(&mut folder.name);
    let (mut visible, mut locked) = (folder.visible, folder.locked);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut visible, tr("Visible")).changed() {
            editor.set_visible(id, visible);
        }
        if ui.checkbox(&mut locked, tr("Locked")).changed() {
            editor.set_locked(id, locked);
        }
    });
    let count = editor.doc.folder_block(id).len() - 1;
    ui.weak(match count {
        0 => tr("Empty. Drag layers just under the folder's name in the timeline to put them in it.")
            .to_owned(),
        1 => tr("Holds 1 layer.").to_owned(),
        n => tr_args("Holds {count} layers.", &[("count", &n.to_string())]),
    });
    switch_controls(ui, editor, id);
    // A folder is where a skeleton lives, rigging what's in it.
    if editor.doc.layer(id).is_some_and(|l| l.skeleton.is_none()) {
        ui.add_space(8.0);
        let add = ui.button(tr("Add Skeleton")).on_hover_text(
            tr("Bones over everything in the folder, which bend with them, painted layers too. Draw them with the Add Bone tool."),
        );
        if add.clicked() {
            if let Some(folder) = editor.doc.layer_mut(id) {
                folder.skeleton = Some(inkframe_core::Skeleton::default());
            }
            editor.selected_bone = None;
            editor.use_tool(crate::editor::Tool::AddBone);
        }
    }
    skeleton_section(ui, editor, id);
}

fn layer_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let last_frame = editor.last_frame();
    if editor.doc.layer(id).is_some_and(|l| l.folder) {
        folder_section(ui, editor, id);
        return;
    }
    if editor.doc.layer(id).is_some_and(|l| l.camera) {
        camera_section(ui, editor, id);
        return;
    }
    let Some(before) = editor.doc.layer(id).map(crate::multi_edit::LayerSettings::of) else { return };
    // The layer's own settings, at the top, are found by search too.
    let header = searched_for(editor, "layer", tr("Layer"));
    let Some(layer) = editor.doc.layer_mut(id) else { return };

    if header {
        ui.heading(tr("Layer"));
        ui.text_edit_singleline(&mut layer.name);
        // Wrapped, so a narrow panel shows all four rather than cutting one off.
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut layer.visible, tr("Visible"));
            ui.checkbox(&mut layer.locked, tr("Locked"));
            ui.checkbox(&mut layer.guide, tr("Guide")).on_hover_text(tr(
                "A motion guide: other layers can follow its path. Shown while editing, never exported.",
            ));
            ui.checkbox(&mut layer.mask, tr("Mask")).on_hover_text(tr(
                "A mask: layers masked by it only show where it has something drawn. It isn't drawn itself.",
            ));
        });
        span_editor(ui, layer, last_frame);
        ui.horizontal(|ui| {
            ui.label(tr("Blend")).on_hover_text(tr("How the layer mixes with what's beneath it"));
            egui::ComboBox::from_id_salt("blend").selected_text(tr(layer.blend.label())).show_ui(ui, |ui| {
                for mode in inkframe_core::BlendMode::ALL {
                    ui.selectable_value(&mut layer.blend, mode, tr(mode.label()));
                }
            });
        });
        // The grain the layer's lines are inked with (see `line.rs` in the core).
        if layer.draws_vectors() {
            use inkframe_core::{LineTexture, TextureKind};
            ui.horizontal(|ui| {
                ui.label(tr("Lines")).on_hover_text(tr(
                    "Ink the layer's pencil lines and strokes with a paper grain, as Harmony's line textures",
                ));
                let current = layer.line_texture.map(|t| t.kind);
                let mut chosen = current;
                egui::ComboBox::from_id_salt("line texture")
                    .selected_text(current.map_or(tr("Flat"), |kind| tr(kind.label())))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, None, tr("Flat"));
                        for kind in TextureKind::ALL {
                            ui.selectable_value(&mut chosen, Some(kind), tr(kind.label()));
                        }
                    });
                if chosen != current {
                    let amount = layer.line_texture.map_or(0.6, |t| t.amount);
                    layer.line_texture = chosen.map(|kind| LineTexture { kind, amount });
                }
                if let Some(texture) = &mut layer.line_texture {
                    ui.add(egui::Slider::new(&mut texture.amount, 0.0..=1.0).show_value(false))
                        .on_hover_text(tr("How strongly the grain shows"));
                }
            });
        }
        // What the layer does when clicked, in a web page export.
        ui.horizontal(|ui| {
        ui.label(tr("Web button")).on_hover_text(
            tr("In a Web Page export, clicking the layer does this, and it lights up under the pointer, as Animate's buttons"),
        );
        web_action_editor(ui, "layer button", &mut layer.button, last_frame);
    });
    }
    editor.carry_settings(id, before);

    ui.add_space(8.0);
    transform_section(ui, editor, id, frame);
    rig_section(ui, editor, id, frame);
    skeleton_section(ui, editor, id);
    ui.add_space(8.0);
    easing_section(ui, editor, id, frame);
    ui.add_space(8.0);
    instance_section(ui, editor, id);
    particles_section(ui, editor, id);
    effects_section(ui, editor, id);
    deformer_section(ui, editor, id);
    warp_section(ui, editor, id);
    drawings_section(ui, editor, id);
    shapes_section(ui, editor, id);
}

/// A skeleton's picked bone, for its folder; for a layer in that folder,
/// how it follows the bones (see `skeleton.rs` in the core).
fn skeleton_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer(id) else { return };
    if layer.skeleton.is_some() {
        if !section(ui, editor, "skeleton", tr("Skeleton"), Icon::Bones) {
            return;
        }
        let picked = editor.selected_bone;
        let mut delete = false;
        let Some(skeleton) = editor.doc.layer_mut(id).and_then(|l| l.skeleton.as_mut()) else { return };
        let count = skeleton.bones.len();
        ui.weak(match count {
            0 => tr("No bones yet: draw them with the Add Bone tool (Shift+K).").to_owned(),
            1 => tr("1 bone. Pose it with the Bone tool (K).").to_owned(),
            n => tr_args("{count} bones. Pose them with the Bone tool (K).", &[("count", &n.to_string())]),
        });
        // The bones the picked one could reach for: any but those it
        // carries (or itself).
        let target_choices: Vec<(usize, String, bool)> = match picked {
            Some(b) => skeleton
                .bones
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != b)
                .map(|(i, other)| (i, other.name.clone(), !skeleton.target_chain(b, i).is_empty()))
                .collect(),
            None => Vec::new(),
        };
        if let Some(bone) = picked.and_then(|b| skeleton.bones.get_mut(b)) {
            egui::Grid::new("bone").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
                ui.label(tr("Bone"));
                ui.text_edit_singleline(&mut bone.name);
                ui.end_row();
                ui.label(tr("Strength")).on_hover_text(
                    tr("How far it reaches into the drawings around it: points about this far away follow it about as much as the next bone"),
                );
                ui.add(DragValue::new(&mut bone.strength).range(1.0..=2000.0).speed(1.0).suffix(" px"));
                ui.end_row();
                ui.label(tr("Turn"));
                let mut turn = bone.turn.sample(frame as f32).unwrap_or(0.0);
                if ui.add(DragValue::new(&mut turn).speed(1.0).suffix("°")).changed() {
                    bone.turn.set(frame, turn);
                }
                ui.end_row();
                ui.label(tr("Stretch")).on_hover_text(tr("How long it is against how it was drawn; Shift-drag it with the Bone tool"));
                let mut stretch = bone.stretch.as_ref().and_then(|c| c.sample(frame as f32)).unwrap_or(1.0) * 100.0;
                if ui.add(DragValue::new(&mut stretch).range(5.0..=2000.0).speed(1.0).suffix("%")).changed() {
                    let channel = bone.stretch.get_or_insert_with(|| inkframe_core::Channel::new(0, 1.0));
                    channel.set(frame, stretch / 100.0);
                }
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut bone.squash, tr("Keep area")).on_hover_text(
                    tr("Stretching it longer makes it thinner, and squashing it fatter, as a bouncing ball does"),
                );
                ui.end_row();
                ui.label(tr("Limits")).on_hover_text(tr("How far it may turn from rest, in degrees"));
                ui.horizontal(|ui| {
                    let mut limited = bone.limits.is_some();
                    if ui.checkbox(&mut limited, "").changed() {
                        bone.limits = limited.then_some(inkframe_core::RotationLimits { min: -90.0, max: 90.0 });
                    }
                    if let Some(limits) = &mut bone.limits {
                        ui.add(DragValue::new(&mut limits.min).range(-360.0..=360.0).speed(1.0).suffix("°"));
                        ui.label(tr("to"));
                        ui.add(DragValue::new(&mut limits.max).range(-360.0..=360.0).speed(1.0).suffix("°"));
                    }
                });
                ui.end_row();
                ui.label(tr("Target")).on_hover_text(
                    tr("A bone its end reaches for, the bones above it bending to get it there: plant a foot on one"),
                );
                let names_and_ok = &target_choices;
                let current = bone.target;
                let label = |t: Option<usize>| {
                    t.and_then(|t| names_and_ok.iter().find(|(i, _, _)| *i == t)).map_or(tr("None").to_owned(), |(_, n, _)| n.clone())
                };
                let mut chosen = current;
                egui::ComboBox::from_id_salt("bone target").selected_text(label(current)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, tr("None"));
                    for (i, name, ok) in names_and_ok {
                        ui.add_enabled_ui(*ok, |ui| {
                            ui.selectable_value(&mut chosen, Some(*i), name);
                        });
                    }
                });
                if chosen != current {
                    bone.target = chosen;
                }
                ui.end_row();
                ui.label(tr("Dynamics")).on_hover_text(
                    tr("It swings after the bones it hangs from, overshoots and settles, as hair or a tail does"),
                );
                let mut swings = bone.dynamics.is_some();
                if ui.checkbox(&mut swings, "").changed() {
                    bone.dynamics = swings.then(inkframe_core::Dynamics::default);
                }
                ui.end_row();
                if let Some(dynamics) = &mut bone.dynamics {
                    ui.label(tr("  Spring")).on_hover_text(tr("How hard it's pulled back to its pose: low is floppy"));
                    ui.add(egui::Slider::new(&mut dynamics.spring, 0.01..=1.0));
                    ui.end_row();
                    ui.label(tr("  Damping")).on_hover_text(tr("How quickly its swing dies away"));
                    ui.add(egui::Slider::new(&mut dynamics.damping, 0.0..=1.0));
                    ui.end_row();
                    ui.label(tr("  Swing")).on_hover_text(tr("The furthest it may swing from its pose"));
                    ui.add(egui::Slider::new(&mut dynamics.limit, 0.0..=180.0).suffix("°"));
                    ui.end_row();
                }
            });
            delete = ui.small_button(tr("Delete Bone")).clicked();
        }
        if delete {
            editor.delete_bone();
        } else if let Some(bone) = picked.filter(|&b| b < count) {
            smart_bone_section(ui, editor, id, bone);
        }
        ui.add_space(8.0);
        return;
    }
    if editor.doc.skeleton_layer_of(layer).is_none()
        || !section(ui, editor, "bones", tr("Bones"), Icon::Bones)
    {
        return;
    }
    let Some(layer) = editor.doc.layer(id) else { return };
    let Some(owner) = editor.doc.skeleton_layer_of(layer) else { return };
    let names: Vec<String> =
        owner.skeleton.iter().flat_map(|s| s.bones.iter().map(|b| b.name.clone())).collect();
    let current = layer.bone_binding;
    // Bones with others hanging from them, which a layer can follow as a limb.
    let limbs: Vec<bool> = owner
        .skeleton
        .iter()
        .flat_map(|s| (0..s.bones.len()).map(|i| s.limb(i).iter().filter(|&&b| b).count() > 1))
        .collect();
    let mut chosen = current;
    ui.horizontal(|ui| {
        ui.label(tr("Follows")).on_hover_text(
            tr("Nearby bones bends the drawing round its joints; a limb bends it with one bone and those below it only (for legs drawn over each other); one bone moves it whole, as a hand or a hat should"),
        );
        chosen = binding_menu(ui, "bone binding", current, &names, &limbs);
    });
    if chosen != current {
        editor.set_bone_binding(id, chosen);
    }
    pin_controls(ui, editor, id, &names);
    ui.add_space(8.0);
}

/// Whether a folder is a switch, showing one of its layers at a time, and
/// which it shows from this frame on.
fn switch_controls(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(folder) = editor.doc.layer(id) else { return };
    let mut switch = folder.switch;
    ui.add_space(4.0);
    let toggled = ui
        .checkbox(&mut switch, tr("Switch"))
        .on_hover_text(tr("Show one of the folder's layers at a time: hand poses, mouth shapes"))
        .changed();
    if toggled && let Some(folder) = editor.doc.layer_mut(id) {
        folder.switch = switch;
    }
    let Some(folder) = editor.doc.layer(id).filter(|l| l.switch) else { return };
    let children: Vec<(LayerId, String)> =
        editor.doc.folder_children(folder).iter().map(|c| (c.id, c.name.clone())).collect();
    let showing = editor.doc.switch_choice(folder, frame);
    let smart = editor.doc.smart_switch(folder, frame).is_some();
    let mut chosen = showing;
    let label = |id: Option<LayerId>| {
        id.and_then(|id| children.iter().find(|(c, _)| *c == id))
            .map_or(tr("(empty)").to_owned(), |(_, n)| n.clone())
    };
    ui.horizontal(|ui| {
        ui.label(tr("Showing")).on_hover_text(tr("Keyed on this frame; it holds until the next key"));
        egui::ComboBox::from_id_salt("switch showing").selected_text(label(showing)).show_ui(ui, |ui| {
            for (child, _) in &children {
                ui.selectable_value(&mut chosen, Some(*child), label(Some(*child)));
            }
        });
    });
    if smart {
        ui.weak(tr("A smart bone is choosing for now."));
    }
    if chosen != showing
        && let Some(child) = chosen
    {
        editor.doc.set_switch(id, frame, child);
    }
}

/// Pinning the shapes picked inside this layer, or the point picked with
/// the Subselection tool, to a bone of their own.
fn pin_controls(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, names: &[String]) {
    if names.is_empty() {
        return;
    }
    let shapes = if editor.inside_layer() == Some(id) { editor.picked_shapes().len() } else { 0 };
    let point = editor.picked_anchor.filter(|(layer, _, _)| *layer == id);
    if shapes == 0 && point.is_none() {
        ui.weak(tr("To pin shapes or points to one bone, pick them: double-click the layer, or use the Subselection tool (A)."));
        return;
    }
    let what = match (point, shapes) {
        (Some(_), _) => tr("Pin point to").to_owned(),
        (None, 1) => tr("Pin shape to").to_owned(),
        (None, n) => tr_args("Pin {count} shapes to", &[("count", &n.to_string())]),
    };
    let pinned = match point {
        Some((layer, element, anchor)) => editor.pinned_bone(layer, element, anchor),
        None => editor.shapes_pinned_bone(),
    };
    let mut chosen = pinned;
    let label = |bone: Option<usize>| {
        bone.and_then(|b| names.get(b).cloned()).unwrap_or_else(|| tr("(not pinned)").into())
    };
    ui.horizontal(|ui| {
        ui.label(what)
            .on_hover_text(tr("They follow that bone alone, whatever the rest of the drawing does"));
        egui::ComboBox::from_id_salt("pin bone").selected_text(label(chosen)).show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, label(None));
            for i in 0..names.len() {
                ui.selectable_value(&mut chosen, Some(i), label(Some(i)));
            }
        });
    });
    if chosen != pinned {
        match point {
            Some((layer, element, anchor)) => editor.pin_point(layer, element, anchor, chosen),
            None => {
                editor.pin_shapes(chosen);
            }
        }
    }
}

/// What a layer following `binding` is called in a menu, with the bones'
/// `names`.
pub(crate) fn binding_label(binding: inkframe_core::BoneBinding, names: &[String]) -> String {
    use inkframe_core::BoneBinding;
    let name = |i: usize| names.get(i).cloned();
    match binding {
        BoneBinding::Region => tr("Nearby bones").to_owned(),
        BoneBinding::Limb(i) => name(i)
            .map_or_else(|| tr("Nearby bones").to_owned(), |n| tr_args("{bone} and below", &[("bone", &n)])),
        BoneBinding::Bone(i) => name(i).unwrap_or_else(|| tr("Nearby bones").to_owned()),
        BoneBinding::Unbound => tr("None").to_owned(),
    }
}

/// A menu of how a layer can follow a skeleton's bones: nearby bones, a
/// limb (each bone with others hanging from it, marked in `limbs`), one
/// bone whole, or none. Returns the choice.
pub(crate) fn binding_menu(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    current: inkframe_core::BoneBinding,
    names: &[String],
    limbs: &[bool],
) -> inkframe_core::BoneBinding {
    use inkframe_core::BoneBinding;
    let mut chosen = current;
    egui::ComboBox::from_id_salt(id).selected_text(binding_label(current, names)).show_ui(ui, |ui| {
        let mut option = |ui: &mut egui::Ui, binding| {
            ui.selectable_value(&mut chosen, binding, binding_label(binding, names));
        };
        option(ui, BoneBinding::Region);
        if limbs.iter().any(|&l| l) {
            ui.label(section_text(tr("Bend with a limb")));
            for i in (0..names.len()).filter(|&i| limbs.get(i).copied().unwrap_or(false)) {
                option(ui, BoneBinding::Limb(i));
            }
        }
        ui.label(section_text(tr("Move with one bone")));
        for i in 0..names.len() {
            option(ui, BoneBinding::Bone(i));
        }
        ui.separator();
        option(ui, BoneBinding::Unbound);
    });
    chosen
}

/// A switch folder for the smart bone section: its id and name, and its
/// layers' ids and names.
type SwitchChoices = (LayerId, String, Vec<(LayerId, String)>);

/// What a smart bone does as it turns: each action on its own row, with
/// buttons to add more.
fn smart_bone_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, bone: usize) {
    use inkframe_core::SmartAction;
    let frame = editor.frame;
    if !section(ui, editor, "smart bone", tr("Smart bone"), Icon::Dial) {
        return;
    }
    // First, as it may preview the bone at another turn, or retime actions.
    action_timeline(ui, editor, id, bone);
    let members = editor.skeleton_members(id);
    let name = |id: LayerId| editor.doc.layer(id).map_or_else(|| tr("(gone)").to_owned(), |l| l.name.clone());
    // Layers that can be reshaped (they have another drawing to morph
    // toward), and switches (with their layers).
    let morphable: Vec<(LayerId, String, Vec<String>)> = members
        .iter()
        .filter_map(|&m| editor.doc.layer(m))
        .filter(|l| l.drawings.len() > 1)
        .map(|l| (l.id, l.name.clone(), l.drawings.iter().map(|d| d.name.clone()).collect()))
        .collect();
    let switches: Vec<SwitchChoices> = members
        .iter()
        .filter_map(|&m| editor.doc.layer(m))
        .filter(|l| l.switch)
        .map(|l| {
            let children = editor.doc.folder_children(l).iter().map(|c| (c.id, c.name.clone())).collect();
            (l.id, l.name.clone(), children)
        })
        .collect();
    let Some(skeleton) = editor.skeleton(id) else { return };
    let Some(the_bone) = skeleton.bones.get(bone) else { return };
    // As shown: previewed on the action timeline, or as animated.
    let now = the_bone.shown_turn(frame as f32);

    // Layers a correction can reshape: any drawn layer it rigs.
    let correctable: Vec<(LayerId, String)> = members
        .iter()
        .filter_map(|&m| editor.doc.layer(m))
        .filter(|l| !l.folder && l.draws_vectors())
        .map(|l| (l.id, l.name.clone()))
        .collect();

    let mut actions = the_bone.actions.clone();
    // A correction being drawn on this bone: say so, and how to finish.
    if let Some(c) = editor.correcting.as_ref().filter(|c| c.skeleton == id && c.bone == bone) {
        let what = tr_args(
            "Drawing the correction of {layer} at {turn}°: reshape it with any tool.",
            &[("layer", &name(c.layer)), ("turn", &format!("{:.0}", c.turn))],
        );
        ui.label(egui::RichText::new(what).strong());
        let (mut finish, mut cancel) = (false, false);
        ui.horizontal(|ui| {
            finish = ui.button(tr("Finish")).on_hover_text(tr("Key the shapes at this turn")).clicked();
            cancel = ui.button(tr("Cancel")).clicked();
        });
        if finish {
            editor.finish_correction();
        } else if cancel {
            editor.cancel_correction();
        }
        return;
    }
    // Starting or adding a correction changes the document, so it waits
    // until the actions have been written back.
    enum Request {
        Edit(usize),
        Add(LayerId),
    }
    let mut request = None;
    let mut remove = None;
    for (i, action) in actions.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| match action {
            SmartAction::Morph { layer, drawing, turn } => {
                ui.label(tr("Reshape"));
                egui::ComboBox::from_id_salt(("smart layer", i)).selected_text(name(*layer)).show_ui(
                    ui,
                    |ui| {
                        for (m, n, _) in &morphable {
                            ui.selectable_value(layer, *m, n);
                        }
                    },
                );
                let drawings = morphable
                    .iter()
                    .find(|(m, _, _)| m == layer)
                    .map(|(_, _, d)| d.clone())
                    .unwrap_or_default();
                let shown = drawings.get(*drawing).cloned().unwrap_or_else(|| "?".into());
                ui.label(tr("to"));
                egui::ComboBox::from_id_salt(("smart drawing", i)).selected_text(shown).show_ui(ui, |ui| {
                    for (d, n) in drawings.iter().enumerate() {
                        ui.selectable_value(drawing, d, n);
                    }
                });
                ui.label(tr("at"));
                ui.add(DragValue::new(turn).speed(1.0).suffix("°"));
                remove = remove.or(ui.small_button("×").clicked().then_some(i));
            }
            SmartAction::Switch { folder, layer, turn } => {
                ui.label(tr("Switch"));
                egui::ComboBox::from_id_salt(("smart switch", i)).selected_text(name(*folder)).show_ui(
                    ui,
                    |ui| {
                        for (m, n, _) in &switches {
                            ui.selectable_value(folder, *m, n);
                        }
                    },
                );
                let children = switches
                    .iter()
                    .find(|(m, _, _)| m == folder)
                    .map(|(_, _, c)| c.clone())
                    .unwrap_or_default();
                ui.label(tr("to"));
                egui::ComboBox::from_id_salt(("smart child", i)).selected_text(name(*layer)).show_ui(
                    ui,
                    |ui| {
                        for (c, n) in &children {
                            ui.selectable_value(layer, *c, n);
                        }
                    },
                );
                ui.label(tr("past"));
                ui.add(DragValue::new(turn).speed(1.0).suffix("°"));
                remove = remove.or(ui.small_button("×").clicked().then_some(i));
            }
            SmartAction::Correct { layer, keys, .. } => {
                ui.label(tr_args("Correct {layer}", &[("layer", &name(*layer))]));
                // The keys themselves are on the action timeline above.
                ui.weak(match keys.len() {
                    0 => tr("(no keys yet)").to_owned(),
                    1 => tr("1 key").to_owned(),
                    n => tr_args("{count} keys", &[("count", &n.to_string())]),
                });
                let edit = ui
                    .add_enabled(
                        now.abs() >= 1.0,
                        egui::Button::new(tr_args("Edit at {turn}°", &[("turn", &format!("{now:.0}"))]))
                            .small(),
                    )
                    .on_hover_text(tr(
                        "Reshape the layer as it should be at the bone's turn now, keying it there",
                    ))
                    .on_disabled_hover_text(tr("Turn the bone first: a correction is keyed at a turn"));
                if edit.clicked() {
                    request = Some(Request::Edit(i));
                }
                remove = remove.or(ui.small_button(tr("Remove")).clicked().then_some(i));
            }
        });
    }
    // A new action is reached at the bone's turn now, if it's turned, so
    // the way to set one up is to pose the bone, then add it.
    let at = if now.abs() > 1.0 { now } else { 90.0 };
    ui.horizontal(|ui| {
        if let Some((layer, _)) = correctable.iter().find(|(l, _)| Some(*l) == editor.selection).or(correctable.first())
            && ui
                .add_enabled(now.abs() >= 1.0, egui::Button::new(tr("+ Correction")).small())
                .on_hover_text(tr("Reshape a layer by hand at this turn of the bone, as Moho's smart bone actions; key more turns as you go"))
                .on_disabled_hover_text(tr("Turn the bone first: a correction is keyed at a turn"))
                .clicked()
        {
            request = Some(Request::Add(*layer));
        }
        if let Some((layer, _, _)) = morphable.first()
            && ui
                .small_button(tr("+ Reshape"))
                .on_hover_text(tr("Morph a layer toward another of its drawings as the bone turns"))
                .clicked()
        {
            actions.push(SmartAction::Morph { layer: *layer, drawing: 1, turn: at });
        }
        if let Some((folder, _, children)) = switches.first()
            && let Some((child, _)) = children.last()
            && ui
                .small_button(tr("+ Switch"))
                .on_hover_text(tr("Have a switch show another of its layers once the bone's turned far enough"))
                .clicked()
        {
            actions.push(SmartAction::Switch { folder: *folder, layer: *child, turn: at });
        }
    });
    if morphable.is_empty() && switches.is_empty() {
        ui.weak(tr("To make it smart, give a layer in the skeleton a second drawing to reshape toward, or add a switch (Modify › New Switch)."));
    }
    if let Some(i) = remove {
        actions.remove(i);
    }
    let changed = editor.skeleton(id).and_then(|s| s.bones.get(bone)).is_some_and(|b| b.actions != actions);
    if changed
        && let Some(b) =
            editor.doc.layer_mut(id).and_then(|l| l.skeleton.as_mut()).and_then(|s| s.bones.get_mut(bone))
    {
        b.actions = actions;
    }
    let started = match request {
        Some(Request::Edit(i)) => editor.start_correction(id, bone, i),
        Some(Request::Add(layer)) => editor.add_correction(id, bone, layer).map(|_| ()),
        None => Ok(()),
    };
    if let Err(problem) = started {
        crate::app::show_error(tr("Couldn't draw the correction"), &problem);
    }
}

/// The bone's action timeline (see `action_timeline.rs`): its actions laid
/// out along its turns, to scrub through and retime. What it changes is
/// written straight back, before the rest of the section reads the actions.
fn action_timeline(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, bone: usize) {
    use crate::action_timeline::{self, Asked};
    use inkframe_core::SmartAction;
    let Some(the_bone) = editor.skeleton(id).and_then(|s| s.bones.get(bone)) else { return };
    let now = the_bone.shown_turn(editor.frame as f32);
    if the_bone.actions.is_empty() {
        return;
    }
    let mut actions = the_bone.actions.clone();
    let name = |id: LayerId| editor.doc.layer(id).map_or_else(|| tr("(gone)").to_owned(), |l| l.name.clone());
    let names: Vec<String> = actions
        .iter()
        .map(|action| match action {
            SmartAction::Morph { layer, .. } => tr_args("Reshape {layer}", &[("layer", &name(*layer))]),
            SmartAction::Switch { folder, .. } => tr_args("Switch {folder}", &[("folder", &name(*folder))]),
            SmartAction::Correct { layer, .. } => tr_args("Correct {layer}", &[("layer", &name(*layer))]),
        })
        .collect();
    let span = action_timeline::span(&actions, the_bone.limits, now);
    let previewing = editor.previewed_turn(id, bone).is_some();
    let asked = action_timeline::show(ui, &mut actions, &names, span, now, previewing);
    if let Some(b) =
        editor.doc.layer_mut(id).and_then(|l| l.skeleton.as_mut()).and_then(|s| s.bones.get_mut(bone))
        && b.actions != actions
    {
        b.actions = actions;
    }
    match asked {
        Some(Asked::Preview(turn)) => editor.preview_turn(id, bone, turn),
        Some(Asked::EndPreview) => editor.end_preview(),
        Some(Asked::Edit { action, turn }) => {
            editor.preview_turn(id, bone, turn);
            if let Err(problem) = editor.start_correction(id, bone, action) {
                crate::app::show_error(tr("Couldn't draw the correction"), &problem);
            }
        }
        None => {}
    }
}

/// A particle layer's emitter: how many particles, where from, how they
/// move and how they change as they age (see `particles.rs` in the core).
fn particles_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    use inkframe_core::{Emitter, EmitterShape, Preset, Sprite};
    if editor.doc.layer(id).is_none_or(|l| l.particles.is_none())
        || !section(ui, editor, "particles", tr("Particles"), Icon::Sparkle)
    {
        return;
    }
    let Some(emitter) = editor.doc.layer_mut(id).and_then(|l| l.particles.as_mut()) else { return };
    let number = |ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str| {
        ui.add(DragValue::new(value).range(range).speed(1.0).suffix(suffix.to_owned()));
    };
    let fraction = |ui: &mut egui::Ui, value: &mut f32| {
        let mut percent = *value * 100.0;
        if ui
            .add(DragValue::new(&mut percent).range(0.0..=100.0).speed(0.5).prefix("± ").suffix("%"))
            .changed()
        {
            *value = percent / 100.0;
        }
    };

    ui.scope(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.weak(tr("Start from"));
            for preset in Preset::ALL {
                if ui.small_button(tr(preset.name())).on_hover_text(tr("Replace these settings with the preset's")).clicked() {
                    *emitter = Emitter { seed: emitter.seed, ..Emitter::preset(preset) };
                }
            }
        });
        ui.add_space(4.0);
        egui::Grid::new("particles").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(tr("Rate"));
            number(ui, &mut emitter.rate, 0.0..=2000.0, " /s");
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut emitter.start_full, tr("Start full"))
                .on_hover_text(tr("Already flowing on the first frame, rather than starting from nothing"));
            ui.end_row();
            ui.label(tr("Variation"));
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut emitter.seed).prefix(format!("{} ", tr("seed"))));
                if ui.small_button(tr("New")).on_hover_text(tr("Another spray just like this one")).clicked() {
                    emitter.seed = emitter.seed.wrapping_mul(2_654_435_761).wrapping_add(1);
                }
            });
            ui.end_row();

            ui.label(tr("Emitter"));
            let names = [tr("Point"), tr("Line"), tr("Rectangle"), tr("Circle")];
            let current = match emitter.shape {
                EmitterShape::Point => 0,
                EmitterShape::Line { .. } => 1,
                EmitterShape::Rect { .. } => 2,
                EmitterShape::Circle { .. } => 3,
            };
            egui::ComboBox::from_id_salt("emitter_shape").selected_text(names[current]).show_ui(ui, |ui| {
                for (i, name) in names.iter().enumerate() {
                    if ui.selectable_label(i == current, *name).clicked() && i != current {
                        emitter.shape = match i {
                            0 => EmitterShape::Point,
                            1 => EmitterShape::Line { length: 200.0 },
                            2 => EmitterShape::Rect { width: 200.0, height: 100.0 },
                            _ => EmitterShape::Circle { radius: 60.0 },
                        };
                    }
                }
            });
            ui.end_row();
            match &mut emitter.shape {
                EmitterShape::Point => {}
                EmitterShape::Line { length } => {
                    ui.label(tr("Length"));
                    number(ui, length, 0.0..=20_000.0, " px");
                    ui.end_row();
                }
                EmitterShape::Rect { width, height } => {
                    ui.label(tr("Size"));
                    ui.horizontal(|ui| {
                        number(ui, width, 0.0..=20_000.0, " w");
                        number(ui, height, 0.0..=20_000.0, " h");
                    });
                    ui.end_row();
                }
                EmitterShape::Circle { radius } => {
                    ui.label(tr("Radius"));
                    number(ui, radius, 0.0..=10_000.0, " px");
                    ui.end_row();
                }
            }

            ui.label(tr("Direction")).on_hover_text(tr("0° is right, -90° straight up"));
            ui.horizontal(|ui| {
                number(ui, &mut emitter.direction, -360.0..=360.0, "°");
                ui.add(DragValue::new(&mut emitter.spread).range(0.0..=180.0).prefix("± ").suffix("°"));
            });
            ui.end_row();
            ui.label(tr("Speed"));
            ui.horizontal(|ui| {
                number(ui, &mut emitter.speed, 0.0..=10_000.0, " px/s");
                fraction(ui, &mut emitter.speed_variance);
            });
            ui.end_row();
            ui.label(tr("Gravity")).on_hover_text(tr("Pulls down; negative pulls up (rising heat)"));
            number(ui, &mut emitter.gravity, -10_000.0..=10_000.0, "");
            ui.end_row();
            ui.label(tr("Wind"));
            number(ui, &mut emitter.wind, -10_000.0..=10_000.0, "");
            ui.end_row();
            ui.label(tr("Drag")).on_hover_text(tr("How quickly the air slows them"));
            ui.add(DragValue::new(&mut emitter.drag).range(0.0..=20.0).speed(0.01));
            ui.end_row();
            ui.label(tr("Wander"));
            ui.horizontal(|ui| {
                number(ui, &mut emitter.turbulence, 0.0..=2000.0, " px");
                ui.add(DragValue::new(&mut emitter.turbulence_frequency).range(0.0..=20.0).speed(0.01).suffix(" Hz"));
            });
            ui.end_row();

            ui.label(tr("Lifetime"));
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut emitter.lifetime).range(0.01..=600.0).speed(0.05).suffix(" s"));
                fraction(ui, &mut emitter.lifetime_variance);
            });
            ui.end_row();
            ui.label(tr("Size")).on_hover_text(tr("At birth → at death, in pixels across"));
            ui.horizontal(|ui| {
                number(ui, &mut emitter.size_start, 0.0..=5000.0, "");
                ui.label("→");
                number(ui, &mut emitter.size_end, 0.0..=5000.0, "");
                fraction(ui, &mut emitter.size_variance);
            });
            ui.end_row();
            ui.label(tr("Opacity"));
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut emitter.opacity_start, 0.0..=1.0).show_value(false));
                ui.label("→");
                ui.add(egui::Slider::new(&mut emitter.opacity_end, 0.0..=1.0).show_value(false));
            });
            ui.end_row();
            ui.label(tr("Colour"));
            ui.horizontal(|ui| {
                color_button(ui, &mut emitter.color_start);
                ui.label("→");
                color_button(ui, &mut emitter.color_end);
            });
            ui.end_row();
            ui.label(tr("Spin"));
            ui.horizontal(|ui| {
                number(ui, &mut emitter.spin, -3600.0..=3600.0, "°/s");
                ui.add(DragValue::new(&mut emitter.spin_variance).range(0.0..=3600.0).prefix("± "));
            });
            ui.end_row();

            ui.label(tr("Look"));
            let mut drawing = emitter.sprite == Sprite::Drawing;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut drawing, false, tr("Dot"));
                ui.selectable_value(&mut drawing, true, tr("This layer's drawing"));
            });
            if drawing != (emitter.sprite == Sprite::Drawing) {
                emitter.sprite = if drawing { Sprite::Drawing } else { Sprite::Dot { softness: 0.5 } };
            }
            ui.end_row();
            if let Sprite::Dot { softness } = &mut emitter.sprite {
                ui.label(tr("Softness"));
                ui.add(egui::Slider::new(softness, 0.0..=1.0));
                ui.end_row();
            }
        });
        if emitter.sprite == Sprite::Drawing {
            ui.weak(tr("Each particle is a copy of what's drawn in this layer (draw into it with \"Draw into the selected layer\"), fitted to the particle's size. Colour doesn't apply."));
        }
    });
    ui.add_space(8.0);
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
    if !section(ui, editor, "effects", tr("Effects"), Icon::Effects) {
        return;
    }
    ui.horizontal(|ui| {
        if key_toggle(ui, Property::Effects, keyed).clicked() {
            editor.toggle_property_key(id, Property::Effects);
        }
        ui.weak(if animated { tr("Animated") } else { tr("Change them on another frame to animate them") });
    });

    // Searching opens them, so what was searched for is in view.
    let searching = (!editor.properties_search.trim().is_empty()).then_some(true);
    egui::CollapsingHeader::new(tr("Colour Effect"))
        .id_salt("color_effect")
        .default_open(!fx.color.is_none())
        .open(searching)
        .show(ui, |ui| {
            let effect = &mut fx.color;
            egui::ComboBox::from_id_salt("color_effect_kind").selected_text(tr(effect.label())).show_ui(
                ui,
                |ui| {
                    let choices = [
                        ColorEffect::None,
                        ColorEffect::Brightness { amount: 0.5 },
                        ColorEffect::Tint { color: Color::rgb(255, 0, 0), amount: 0.5 },
                        ColorEffect::Advanced { multiply: [1.0; 4], add: [0.0; 4] },
                    ];
                    for choice in choices {
                        if ui.selectable_label(effect.label() == choice.label(), tr(choice.label())).clicked()
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
                        for (i, name) in
                            [tr("Red"), tr("Green"), tr("Blue"), tr("Alpha")].into_iter().enumerate()
                        {
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

    egui::CollapsingHeader::new(tr_args("Filters ({count})", &[("count", &fx.filters.len().to_string())]))
        .id_salt("filters")
        .default_open(!fx.filters.is_empty())
        .open(searching)
        .show(ui, |ui| {
            let mut remove = None;
            let mut raise = None;
            for (i, filter) in fx.filters.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut filter.enabled, tr(filter.kind.label()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(crate::icons::IconButton::new(
                                    crate::icons::Icon::Trash,
                                    tr("Remove this filter"),
                                ))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                            if i > 0
                                && ui
                                    .add(crate::icons::IconButton::new(
                                        crate::icons::Icon::Up,
                                        tr("Apply this one earlier"),
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
            ui.menu_button(tr("Add Filter"), |ui| {
                for kind in FilterKind::ALL {
                    if ui.button(tr(kind.label())).clicked() {
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
    apply_to_selected_button(ui, editor, id, tr("these effects"), |editor| {
        editor.copy_effects_to_selected(id)
    });
}

/// With several layers selected, a button that gives the rest what this
/// layer has (its effects, its easing). A button rather than doing it on
/// every change, since those are things each layer may have set up itself.
fn apply_to_selected_button(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    id: LayerId,
    what: &str,
    apply: impl FnOnce(&mut Editor) -> usize,
) {
    let others = editor.also_editing(id).len();
    if others == 0 {
        return;
    }
    let label = if others == 1 {
        tr("Apply to the other selected layer").to_owned()
    } else {
        tr_args("Apply to the other {count} selected layers", &[("count", &others.to_string())])
    };
    if ui
        .button(label)
        .on_hover_text(tr_args("Gives them {what}, replacing their own on this frame", &[("what", what)]))
        .clicked()
    {
        apply(editor);
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
            ui.label(tr("Colour"));
            color_button(ui, color);
            ui.end_row();
            ui.label(tr("Distance"));
            ui.add(DragValue::new(distance).range(-255.0..=255.0).speed(0.2).suffix(" px"));
            ui.end_row();
            ui.label(tr("Angle"));
            ui.add(DragValue::new(angle).speed(1.0).suffix("°"));
            ui.end_row();
            ui.label(tr("Blur"));
            ui.add(pixels(blur));
            ui.end_row();
            ui.label(tr("Strength"));
            ui.add(percent(strength, 10.0));
            ui.end_row();
        }
        FilterKind::Blur { x, y } => {
            ui.label(tr("Blur"));
            ui.horizontal(|ui| {
                ui.add(pixels(x).prefix("x "));
                ui.add(pixels(y).prefix("y "));
            });
            ui.end_row();
        }
        FilterKind::Glow { blur, color, strength, inner } => {
            ui.label(tr("Colour"));
            color_button(ui, color);
            ui.end_row();
            ui.label(tr("Blur"));
            ui.add(pixels(blur));
            ui.end_row();
            ui.label(tr("Strength"));
            ui.add(percent(strength, 10.0));
            ui.end_row();
            ui.label("");
            ui.checkbox(inner, tr("Inner glow"));
            ui.end_row();
        }
        FilterKind::MotionBlur { shutter, samples } => {
            ui.label(tr("Shutter")).on_hover_text(
                tr("How much of each frame the blur covers: 180° is the usual film look, 360° the whole frame"),
            );
            ui.add(egui::Slider::new(shutter, 0.0..=360.0).suffix("°"));
            ui.end_row();
            ui.label(tr("Quality")).on_hover_text(
                tr("The fewest moments drawn and blended. Fast movement gets more automatically, so the smear stays smooth"),
            );
            ui.add(egui::Slider::new(samples, 2..=32));
            ui.end_row();
        }
        FilterKind::AdjustColor { brightness, contrast, saturation, hue } => {
            ui.label(tr("Brightness"));
            ui.add(signed(brightness));
            ui.end_row();
            ui.label(tr("Contrast"));
            ui.add(signed(contrast));
            ui.end_row();
            ui.label(tr("Saturation"));
            ui.add(signed(saturation));
            ui.end_row();
            ui.label(tr("Hue"));
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
        ui.colored_label(ui.visuals().error_fg_color, tr("This instance's symbol is missing."));
        return;
    };

    if !section(ui, editor, "instance", tr("Symbol instance"), Icon::Symbol) {
        return;
    }
    ui.weak(tr("Double-click it on the stage (or Modify › Edit Symbol) to work on the layers inside."));
    let frame = editor.frame;
    let (showing, swaps) = editor.doc.layer(id).map_or((instance.symbol, 0), |l| {
        (l.symbol_at(frame).unwrap_or(instance.symbol), l.symbol_swaps.len())
    });
    egui::Grid::new("instance").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Symbol"));
        ui.horizontal(|ui| {
            let name = symbols.iter().find(|(s, _)| *s == showing).map_or("", |(_, n)| n.as_str());
            ui.strong(name);
            // Modify › Swap Symbol, from the panel.
            if ui
                .button(tr("Swap…"))
                .on_hover_text(tr("Show another symbol here, keeping the animation"))
                .clicked()
            {
                crate::swap_dialog::request(ui.ctx());
            }
        });
        ui.end_row();
        if swaps > 0 {
            ui.label("");
            ui.weak(if swaps == 1 {
                tr("Swapped once along the way: see the timeline").to_owned()
            } else {
                tr_args(
                    "Swapped {count} times along the way: see the timeline",
                    &[("count", &swaps.to_string())],
                )
            });
            ui.end_row();
        }

        ui.label(tr("Playback"));
        egui::ComboBox::from_id_salt("instance_playback")
            .selected_text(tr(instance.playback.label()))
            .show_ui(ui, |ui| {
                for playback in Playback::ALL {
                    ui.selectable_value(&mut instance.playback, playback, tr(playback.label()));
                }
            });
        ui.end_row();

        // Shown 1-based, like every other frame number in the interface.
        ui.label(tr("First frame"));
        let mut shown = instance.first_frame + 1;
        if ui.add(DragValue::new(&mut shown).range(1..=duration)).changed() {
            instance.first_frame = shown - 1;
        }
        ui.end_row();
    });

    if let Some(layer) = editor.doc.layer_mut(id) {
        layer.instance = Some(instance);
    }
    // The symbol showing on this frame, which may be one swapped in.
    if let Some(symbol) = editor.doc.symbol_mut(showing) {
        ui.horizontal(|ui| {
            ui.label(tr("Symbol length"));
            ui.add(DragValue::new(&mut symbol.duration).range(1..=100_000).suffix(format!(" {}", tr("frames")))).on_hover_text(
                tr("How long the symbol's own timeline is. Playing on Loop, the instance repeats every this many \
                 frames: a 12-frame walk cycle loops for as long as the layer lasts. Changes the symbol everywhere \
                 it's used."),
            );
        });
    }
    ui.add_space(8.0);
}

/// The frames a layer appears on, shown 1-based like the timeline. The end
/// can be a fixed frame or "end", which follows the animation's length.
fn span_editor(ui: &mut egui::Ui, layer: &mut Layer, last_frame: u32) {
    ui.horizontal(|ui| {
        ui.label(tr("Frames"));
        let mut start = layer.start + 1;
        if ui.add(DragValue::new(&mut start).range(1..=last_frame + 1)).changed() {
            layer.set_start(start - 1);
        }
        ui.label(tr("to"));

        let mut open_ended = layer.end.is_none();
        if let Some(end) = layer.end {
            let mut shown = end + 1;
            if ui.add(DragValue::new(&mut shown).range(layer.start + 1..=last_frame + 1)).changed() {
                layer.set_end(Some(shown - 1));
            }
        }
        if ui
            .checkbox(&mut open_ended, tr("end"))
            .on_hover_text(tr("Stay until the animation ends, even if it gets longer"))
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
    let before = pose;
    let several = !editor.also_editing(id).is_empty();
    // Which rows differ between the selected layers, to mark them.
    let differs = |property: Property| {
        several
            && editor.pose_differs(id, |p| match property {
                Property::Position => [p.position.x, p.position.y],
                Property::Rotation => [p.rotation, 0.0],
                Property::Scale => [p.scale.x, p.scale.y],
                Property::Skew => [p.skew.x, p.skew.y],
                Property::Opacity => [p.opacity, 0.0],
                Property::Effects | Property::Depth => [0.0, 0.0],
            })
    };
    let mixed: Vec<Property> = Property::ALL.into_iter().filter(|&p| differs(p)).collect();
    // Depth, for the camera: only layers at the top of a rig on the main
    // timeline sit at a depth; the rest go with them.
    let has_depth =
        layer.parent.is_none() && editor.doc.timeline_of(id) == Some(inkframe_core::TimelineId::Main);
    let mut depth = layer.track.depth_at(frame as f32);
    let mut depth_changed = false;

    if !section(
        ui,
        editor,
        "transform",
        tr_args("Transform at frame {frame}", &[("frame", &(frame + 1).to_string())]),
        Icon::Transform,
    ) {
        return;
    }

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
        let transform = Property::ALL
            .into_iter()
            .zip(keyed)
            .filter(|(p, _)| *p != Property::Effects && (*p != Property::Depth || has_depth));
        for (property, is_key) in transform {
            if key_toggle(ui, property, is_key).clicked() {
                toggled = Some(property);
            }
            let label = if mixed.contains(&property) {
                ui.label(format!("{} •", tr(property.name()))).on_hover_text(tr(
                    "Differs between the selected layers. Changing it sets them all (position moves them all \
                     by the same amount).",
                ))
            } else {
                ui.label(tr(property.name()))
            };
            if property == Property::Depth {
                label.on_hover_text(tr(
                    "How far behind the stage the layer sits, for the camera: deeper layers look smaller \
                     and move less as it pans. Negative comes forward. For the camera, how far it has \
                     moved in.",
                ));
            }
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
                Property::Depth => {
                    depth_changed |= ui.add(DragValue::new(&mut depth).suffix(" px").speed(5.0)).changed();
                }
            });
            ui.end_row();
        }
    });

    if changed {
        pose.scale.x = scale_pct[0] / 100.0;
        pose.scale.y = scale_pct[1] / 100.0;
        pose.opacity = opacity_pct / 100.0;
        editor.set_pose(id, pose);
        editor.carry_pose_change(id, before, pose);
    }
    if depth_changed {
        editor.set_depth(id, depth);
        editor.carry_depth(id, depth);
    }
    if let Some(property) = toggled {
        editor.toggle_property_key(id, property);
        editor.carry_key(id, property);
    }
    motion_path_section(ui, editor, id);
    mask_section(ui, editor, id);
}

/// How the layer hangs in a character rig: the layer it's attached to, and
/// the pivot it turns around.
fn rig_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    if !section(ui, editor, "parent and pivot", tr("Parent and pivot"), Icon::Nodes) {
        return;
    }
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
        ui.label(tr("Parent")).on_hover_text(tr(
            "The layer this one is attached to. It moves, turns and scales with its parent, \
             and its own position is measured from the parent.",
        ));
        let current = parents.iter().find(|(p, _)| Some(*p) == parent).map_or(tr("None"), |(_, name)| name);
        egui::ComboBox::from_id_salt("parent").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut parent, None, tr("None"));
            for (other, name) in &parents {
                ui.selectable_value(&mut parent, Some(*other), name);
            }
        });
        ui.end_row();

        ui.label(tr("Pivot"))
            .on_hover_text(tr("The point the layer turns and scales around, in its own coordinates. \
             Option-drag (Alt-drag) the ring on the stage to move it."));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut pivot.x).prefix("x ").speed(1.0));
            ui.add(DragValue::new(&mut pivot.y).prefix("y ").speed(1.0));
            if let Some(bounds) = bounds
                && ui
                    .small_button(tr("Center"))
                    .on_hover_text(tr("Put the pivot in the middle of the layer"))
                    .clicked()
            {
                pivot = bounds.center();
            }
        });
        ui.end_row();

        // Only joints bend, so only attached layers have limits.
        if parent.is_some() {
            ui.label(tr("Bends")).on_hover_text(tr(
                "How far the Bone tool may turn this joint, as the lowest and highest rotation, \
                 so an elbow or knee only bends one way.",
            ));
            ui.horizontal(|ui| {
                let mut limited = limits.is_some();
                if ui.checkbox(&mut limited, "").changed() {
                    // Start from a range around where it is now.
                    limits = limited.then_some(RotationLimits { min: rotation - 45.0, max: rotation + 45.0 });
                }
                match &mut limits {
                    Some(range) => {
                        ui.add(DragValue::new(&mut range.min).suffix("°").speed(1.0));
                        ui.label(tr("to"));
                        ui.add(DragValue::new(&mut range.max).suffix("°").speed(1.0));
                    }
                    None => {
                        ui.weak(tr("freely"));
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
    if editor.doc.layer(id).is_none_or(|l| l.instance.is_some())
        || !section(ui, editor, "deformer", tr("Deformer"), Icon::Envelope)
    {
        return;
    }
    let Some(layer) = editor.doc.layer_mut(id).filter(|l| l.instance.is_none()) else { return };
    // The drawing as drawn, not bent, is what the bones are laid along.
    let rest_bounds = layer.elements_at(frame).iter().map(|e| e.bounds()).reduce(|a, b| a.union(b));

    let Some(deformer) = &mut layer.deformer else {
        let add = ui
            .add_enabled(rest_bounds.is_some(), egui::Button::new(tr("Add Bend Deformer")))
            .on_hover_text(tr("Bones through the drawing that bend it smoothly, for rubber-hose limbs and tails. Pose them with the Bone tool."));
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
        ui.label(tr("Bones")).on_hover_text(tr("Changing the count lays the bones out straight again"));
        ui.add(DragValue::new(&mut bones).range(1..=8));
        ui.end_row();
        ui.label(tr("Smoothness"))
            .on_hover_text(tr("How soft each bend is: 0 bends like a hinge, 1 like a hose"));
        ui.add(egui::Slider::new(&mut deformer.smoothness, 0.0..=1.0));
        ui.end_row();
        for (i, mut bend) in deformer.bends_at(frame).into_iter().enumerate() {
            ui.label(tr_args("Bend {number}", &[("number", &(i + 1).to_string())]));
            if ui.add(DragValue::new(&mut bend).suffix("°").speed(1.0)).changed() {
                deformer.bends[i].set(frame, bend);
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        ui.weak(tr("Drag its joints with the Bone tool (K)."));
        remove = ui.small_button(tr("Remove")).clicked();
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
    if editor.doc.layer(id).is_none_or(|l| l.instance.is_some())
        || !section(ui, editor, "free-form deformer", tr("Free-Form Deformer"), Icon::Distort)
    {
        return;
    }
    let Some(layer) = editor.doc.layer_mut(id).filter(|l| l.instance.is_none()) else { return };
    let rest_bounds = layer.elements_at(frame).iter().map(|e| e.bounds()).reduce(|a, b| a.union(b));

    let Some(warp) = &mut layer.warp else {
        ui.horizontal(|ui| {
            for size in [3, 4] {
                let add = ui
                    .add_enabled(rest_bounds.is_some(), egui::Button::new(tr_args("Add {size}×{size} Grid", &[("size", &size.to_string())])))
                    .on_hover_text(tr("A grid of points over the drawing: drag them with the Bone tool to squash, bulge and sway it."));
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
        ui.weak(tr_args(
            "{columns}×{rows} grid. Drag its points with the Bone tool (K).",
            &[("columns", &warp.columns.to_string()), ("rows", &warp.rows.to_string())],
        ));
        if ui.small_button(tr("Straighten")).on_hover_text(tr("Put every point back on this frame")).clicked()
        {
            for channel in &mut warp.offsets {
                if channel.sample(frame as f32).is_some_and(|o| o != inkframe_core::Point::ZERO)
                    || channel.key_at(frame).is_some()
                {
                    channel.set(frame, inkframe_core::Point::ZERO);
                }
            }
        }
        remove = ui.small_button(tr("Remove")).clicked();
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
    if editor.doc.layer(id).is_none_or(|l| l.instance.is_some())
        || !section(ui, editor, "drawings", tr("Drawings"), Icon::ExposureSheet)
    {
        return;
    }
    let Some(layer) = editor.doc.layer(id) else { return };
    let showing = layer.drawing_index_at(frame);
    let names: Vec<String> = layer.drawings.iter().map(|d| d.name.clone()).collect();

    if names.len() == 1 {
        ui.weak(tr("Add drawings to swap between, like mouth shapes or hand poses."));
    } else {
        ui.weak(tr("[ and ] flip through them."));
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
            .button(tr("New"))
            .on_hover_text(tr("A blank drawing, shown from this frame. Draw into it on the stage."))
            .clicked();
        copy = ui
            .button(tr("Duplicate"))
            .on_hover_text(tr("A copy of the drawing showing now, to change a little"))
            .clicked();
        let can_delete = names.len() > 1;
        delete = ui
            .add_enabled(can_delete, egui::Button::new(tr("Delete")))
            .on_hover_text(tr("Remove the drawing showing now"))
            .clicked();
    });
    if let Some(drawing) = editor.doc.layer_mut(id).and_then(|l| l.drawings.get_mut(showing)) {
        ui.horizontal(|ui| {
            ui.label(tr("Name"));
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
        .checkbox(&mut on, tr("Shape tween into the next drawing"))
        .on_hover_text(tr(
            "Morph from the drawing showing here into the next one, instead of swapping at once",
        ))
        .changed()
    {
        editor.set_shape_tween(on);
    }
    let frame = editor.frame;
    let Some(tween) = editor.selected_layer_mut().and_then(|l| l.shape_tween_mut(frame)) else { return };
    ui.horizontal(|ui| {
        ui.label(tr("Easing"));
        egui::ComboBox::from_id_salt("shape_tween_easing").selected_text(tr(tween.easing.label())).show_ui(
            ui,
            |ui| {
                for preset in Easing::PRESETS {
                    ui.selectable_value(&mut tween.easing, preset, tr(preset.label()));
                }
            },
        );
    });
    let hints = editor
        .shape_tween_drawings()
        .map_or(0, |(from, _)| editor.selected_layer().map_or(0, |l| l.drawings[from].hints.len()));
    ui.horizontal(|ui| {
        if ui
            .button(tr("Add Shape Hint"))
            .on_hover_text(tr("⌘⇧H. Drag the lettered marks onto matching points of the two drawings"))
            .clicked()
        {
            editor.add_shape_hint();
        }
        if ui.add_enabled(hints > 0, egui::Button::new(tr("Remove Hints"))).clicked() {
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
        ui.label(tr("Masked by"));
        let current =
            masks.iter().find(|(m, _)| Some(*m) == layer.masked_by).map_or(tr("None"), |(_, name)| name);
        egui::ComboBox::from_id_salt("masked_by").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut layer.masked_by, None, tr("None"));
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
            ui.label(tr("Follow guide"));
            let current =
                guides.iter().find(|(g, _)| Some(*g) == layer.follow).map_or(tr("None"), |(_, name)| name);
            egui::ComboBox::from_id_salt("follow").selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut layer.follow, None, tr("None"));
                for (guide, name) in &guides {
                    ui.selectable_value(&mut layer.follow, Some(*guide), name);
                }
            });
        });
        if layer.follow.is_some() && layer.track.position.keys().len() < 2 {
            ui.weak(tr(
                "Add a second position keyframe: the layer travels the guide between its first and last.",
            ));
        }
    }

    let track = &mut layer.track;
    if track.position.keys().len() < 2 {
        return;
    }
    let following = layer.follow.is_some();
    ui.horizontal(|ui| {
        ui.checkbox(&mut track.orient_to_path, tr("Orient to path"))
            .on_hover_text(tr("Turn the layer to face the way it's moving"));
        // A layer on a guide takes its path from the guide.
        let curved = !following && track.position.keys().iter().any(|k| k.bend.is_some());
        if ui
            .add_enabled(curved, egui::Button::new(tr("Straighten path")).small())
            .on_hover_text(tr(
                "Make every tween a straight line again. Drag the path on the stage to curve it.",
            ))
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
        tr_args(
            "Remove the {property} keyframe on this frame",
            &[("property", &tr(property.name()).to_lowercase())],
        )
    } else {
        tr_args(
            "Add a {property} keyframe on this frame",
            &[("property", &tr(property.name()).to_lowercase())],
        )
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
        ui.weak(tr("No tweens here. Move to a later frame and change a property to create one."));
        return;
    }
    // Keep the graph on a property that's actually tweening here.
    if !tweens.iter().any(|(p, _)| *p == editor.focused_property) {
        editor.focused_property = tweens[0].0;
    }

    if !section(ui, editor, "easing", tr("Easing"), Icon::Graph) {
        return;
    }
    let mut changes = Vec::new();
    egui::Grid::new("easing").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
        for (property, tween) in &tweens {
            let focused = editor.focused_property == *property;
            if ui
                .selectable_label(focused, tr(property.name()))
                .on_hover_text(tr("Show this curve below"))
                .clicked()
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
    apply_to_selected_button(ui, editor, id, tr("this easing, for each property tweening here"), |editor| {
        editor.copy_easing_to_selected(id)
    });
}

/// The easing picker. Returns true when the choice changed.
fn easing_combo(ui: &mut egui::Ui, id_salt: Property, easing: &mut Easing) -> bool {
    let before = *easing;
    egui::ComboBox::from_id_salt(("easing", id_salt)).selected_text(tr(easing.label())).show_ui(ui, |ui| {
        for preset in Easing::PRESETS {
            ui.selectable_value(easing, preset, tr(preset.label()));
        }
        // "Custom" starts from the current curve so it's a tweak, not a reset.
        let is_custom = matches!(easing, Easing::CubicBezier { .. });
        if ui.selectable_label(is_custom, tr("Custom")).clicked() && !is_custom {
            let [x1, y1, x2, y2] = easing.control_points().unwrap_or([0.0, 0.0, 1.0, 1.0]);
            *easing = Easing::CubicBezier { x1, y1, x2, y2 };
        }
    });
    *easing != before
}

fn shapes_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    if editor.doc.layer(id).is_none_or(|l| l.elements_at(frame).is_empty())
        || !section(ui, editor, "shapes", tr("Shapes"), Icon::Shapes)
    {
        return;
    }
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
    let symbols = brush_symbols(editor);
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    let elements = layer.elements_at_mut(frame);
    let count = elements.len();
    for (i, element) in elements.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            let title = match &element.shape {
                Shape::Rect { .. } => tr("Rectangle"),
                Shape::Ellipse { .. } => tr("Oval"),
                Shape::Path(_) => tr("Stroke"),
                Shape::Line { .. } => tr("Pencil Line"),
                Shape::Image { .. } => tr("Image"),
                Shape::Text(_) => tr("Text"),
                Shape::Paint { .. } => tr("Painting"),
            };
            ui.strong(if count > 1 { format!("{title} {}", i + 1) } else { title.to_owned() });
            if let Shape::Paint { width, height, .. } = element.shape {
                // Painted with the brush tools, so nothing to set here
                // beyond saying how big the canvas is.
                ui.weak(tr_args(
                    "{width} × {height} px canvas. Paint with the Brush (B) and Eraser (E).",
                    &[("width", &width.to_string()), ("height", &height.to_string())],
                ));
            } else if let Shape::Image { image, width, height } = &mut element.shape {
                // An image paints its own pixels, so it has no fill or stroke.
                if let Some(info) = images.iter().find(|info| info.id == *image) {
                    image_editor(ui, info, width, height);
                }
            } else if let Shape::Text(text) = &mut element.shape {
                let problem =
                    font_problems.iter().find(|(f, _)| *f == text.font.family).map(|(_, p)| p.as_str());
                text_editor(ui, text, problem);
                style_editor(ui, &mut element.style, &symbols);
            } else {
                shape_size_editor(ui, &mut element.shape);
                style_editor(ui, &mut element.style, &symbols);
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
        ui.label(tr("File"));
        match &info.problem {
            Some(problem) => ui.colored_label(ui.visuals().error_fg_color, problem),
            None => ui.label(&info.name),
        };
        ui.end_row();
        ui.label(tr("Size"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(width).range(0.0..=f32::MAX).prefix("w "));
            ui.add(DragValue::new(height).range(0.0..=f32::MAX).prefix("h "));
        });
        ui.end_row();
        ui.label("");
        let (w, h) = info.size;
        if ui
            .button(tr("Actual size"))
            .on_hover_text(tr_args(
                "{width} × {height}, one stage pixel per image pixel",
                &[("width", &w.to_string()), ("height", &h.to_string())],
            ))
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
        ui.label(tr("Font"));
        ui.horizontal(|ui| {
            let families = inkframe_render::font_families();
            egui::ComboBox::from_id_salt("font").selected_text(&text.font.family).width(160.0).show_ui(
                ui,
                |ui| {
                    // The project's own fonts first, where they're easy to find
                    // among hundreds of installed ones.
                    if !families.project.is_empty() {
                        ui.weak(tr("This project"));
                        for family in &families.project {
                            ui.selectable_value(&mut text.font.family, family.clone(), family);
                        }
                        ui.separator();
                        ui.weak(tr("Installed"));
                    }
                    for family in &families.installed {
                        ui.selectable_value(&mut text.font.family, family.clone(), family);
                    }
                },
            );
            if ui
                .small_button(tr("Refresh"))
                .on_hover_text(tr("Look again for fonts installed since Inkframe started"))
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
            let reason = font_problem.unwrap_or(tr("Not installed"));
            ui.label("");
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr_args("{reason}; showing {font}", &[("reason", reason), ("font", &used)]),
            );
            ui.end_row();
        }

        ui.label(tr("Style"));
        ui.horizontal(|ui| {
            ui.toggle_value(&mut text.font.bold, tr("Bold"));
            ui.toggle_value(&mut text.font.italic, tr("Italic"));
        });
        ui.end_row();
        ui.label(tr("Size"));
        ui.add(DragValue::new(&mut text.size).range(1.0..=2000.0).suffix(" px"));
        ui.end_row();
        ui.label(tr("Align"));
        ui.horizontal(|ui| {
            for align in TextAlign::ALL {
                ui.selectable_value(&mut text.align, align, tr(align.label()));
            }
        });
        ui.end_row();
        ui.label(tr("Line spacing"));
        ui.add(DragValue::new(&mut text.line_spacing).range(0.5..=5.0).speed(0.01).suffix("×"));
        ui.end_row();
    });
}

fn shape_size_editor(ui: &mut egui::Ui, shape: &mut Shape) {
    egui::Grid::new("shape").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match shape {
        Shape::Rect { width, height, corner_radius } => {
            ui.label(tr("Size"));
            ui.horizontal(|ui| {
                ui.add(DragValue::new(width).range(0.0..=f32::MAX).prefix("w "));
                ui.add(DragValue::new(height).range(0.0..=f32::MAX).prefix("h "));
            });
            ui.end_row();
            ui.label(tr("Corners"));
            ui.add(DragValue::new(corner_radius).range(0.0..=f32::MAX).suffix(" px"));
            ui.end_row();
        }
        Shape::Ellipse { radius_x, radius_y } => {
            ui.label(tr("Radius"));
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
            ui.label(tr("Width")).on_hover_text(tr(
                "Scales the whole line; the Width tool (Shift+A) shapes it point by point",
            ));
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
pub fn style_editor(ui: &mut egui::Ui, style: &mut Style, symbols: &[(inkframe_core::SymbolId, String)]) {
    use inkframe_core::{Dash, GradientShape, LineCap, LineJoin, ShapeGradient};
    egui::Grid::new("style").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        // Fill: none, a colour, or a gradient (see `ShapeGradient`).
        ui.label(tr("Fill"));
        ui.horizontal(|ui| {
            let mut on = style.fill.is_some() || style.gradient.is_some();
            if ui.checkbox(&mut on, "").changed() {
                style.fill = on.then_some(style.fill.unwrap_or(Color::rgb(0x3d, 0x8b, 0xfd)));
                if !on {
                    style.gradient = None;
                }
            }
            if !on {
                return;
            }
            let kind = style.gradient.as_ref().map(|g| g.shape);
            let label = match kind {
                None => tr("Solid"),
                Some(GradientShape::Linear) => tr("Linear"),
                Some(GradientShape::Radial) => tr("Radial"),
            };
            let base = style.fill.unwrap_or(Color::BLACK);
            egui::ComboBox::from_id_salt("fill_kind").selected_text(label).width(80.0).show_ui(ui, |ui| {
                if ui.selectable_label(kind.is_none(), tr("Solid")).clicked() {
                    style.gradient = None;
                }
                for (shape, name) in [(GradientShape::Linear, tr("Linear")), (GradientShape::Radial, tr("Radial"))] {
                    if ui.selectable_label(kind == Some(shape), name).clicked() && kind != Some(shape) {
                        // Switching between the two keeps the colours; from
                        // solid it fades the fill colour out to white.
                        let stops = style.gradient.take().map(|g| g.stops);
                        let mut gradient = match shape {
                            GradientShape::Linear => ShapeGradient::linear(base, Color::WHITE),
                            GradientShape::Radial => ShapeGradient::radial(base, Color::WHITE),
                        };
                        if let Some(stops) = stops {
                            gradient.stops = stops;
                        }
                        style.gradient = Some(gradient);
                    }
                }
            });
            if style.gradient.is_none()
                && let Some(fill) = &mut style.fill
            {
                color_button(ui, fill);
            }
        });
        ui.end_row();
        if let Some(gradient) = &mut style.gradient {
            ui.label("");
            ui.vertical(|ui| {
                gradient_stops(ui, gradient);
                ui.weak(tr("To move its ends, double-click the shape on the stage, then click it."));
            });
            ui.end_row();
            // What wants one colour (the bucket, palettes) goes by the first.
            style.fill = gradient.stops.first().map(|s| s.color);
        }

        ui.label(tr("Stroke"));
        ui.horizontal(|ui| {
            let mut on = style.stroke.is_some();
            if ui.checkbox(&mut on, "").changed() {
                style.stroke = on.then_some(style.stroke.unwrap_or(Stroke::new(Color::BLACK, 2.0)));
            }
            if let Some(stroke) = &mut style.stroke {
                color_button(ui, &mut stroke.color);
                ui.add(DragValue::new(&mut stroke.width).range(0.1..=200.0).speed(0.1).suffix(" px"));
            }
        });
        ui.end_row();
        let Some(stroke) = &mut style.stroke else { return };

        ui.label(tr("Ends"));
        let caps = [(LineCap::Round, tr("Round")), (LineCap::Butt, tr("Flat")), (LineCap::Square, tr("Square"))];
        choice(ui, "stroke_cap", &mut stroke.cap, &caps);
        ui.end_row();
        ui.label(tr("Corners"));
        let joins = [(LineJoin::Round, tr("Round")), (LineJoin::Miter, tr("Sharp")), (LineJoin::Bevel, tr("Bevelled"))];
        choice(ui, "stroke_join", &mut stroke.join, &joins);
        ui.end_row();

        ui.label(tr("Line"));
        ui.horizontal(|ui| {
            #[derive(Clone, Copy, PartialEq)]
            enum Kind {
                Solid,
                Dashed,
                Dotted,
            }
            let kind = match stroke.dash {
                None => Kind::Solid,
                Some(dash) if dash.is_dotted() => Kind::Dotted,
                Some(_) => Kind::Dashed,
            };
            let mut chosen = kind;
            choice(
                ui,
                "stroke_dash",
                &mut chosen,
                &[(Kind::Solid, tr("Solid")), (Kind::Dashed, tr("Dashed")), (Kind::Dotted, tr("Dotted"))],
            );
            if chosen != kind {
                stroke.dash = match chosen {
                    Kind::Solid => None,
                    Kind::Dashed => Some(Dash::dashed(stroke.width)),
                    Kind::Dotted => {
                        // Dots need round ends to be dots.
                        stroke.cap = LineCap::Round;
                        Some(Dash::dotted(stroke.width))
                    }
                };
            }
            if let Some(dash) = &mut stroke.dash {
                if !dash.is_dotted() {
                    ui.add(DragValue::new(&mut dash.length).range(0.5..=500.0).speed(0.2).prefix(format!("{} ", tr("dash"))));
                }
                ui.add(DragValue::new(&mut dash.gap).range(0.5..=500.0).speed(0.2).prefix(format!("{} ", tr("gap"))));
            }
        });
        ui.end_row();

        // Drawn as a symbol's picture laid along it (see `art_brush.rs`).
        ui.label(tr("Brush")).on_hover_text(
            tr("Draw the line as a symbol's picture: stretched along it (Art) or repeated (Pattern), as Animate's Paint Brush does"),
        );
        ui.horizontal(|ui| {
            use inkframe_core::{BrushKind, StrokeBrush};
            #[derive(Clone, Copy, PartialEq)]
            enum Kind {
                Plain,
                Art,
                Pattern,
            }
            let kind = match stroke.brush.map(|b| b.kind) {
                None => Kind::Plain,
                Some(BrushKind::Art) => Kind::Art,
                Some(BrushKind::Pattern) => Kind::Pattern,
            };
            let mut chosen = kind;
            ui.add_enabled_ui(!symbols.is_empty(), |ui| {
                choice(ui, "stroke_brush", &mut chosen, &[(Kind::Plain, tr("Plain")), (Kind::Art, tr("Art")), (Kind::Pattern, tr("Pattern"))]);
            });
            if chosen != kind {
                let symbol = stroke.brush.map(|b| b.symbol).or(symbols.first().map(|(id, _)| *id));
                stroke.brush = match (chosen, symbol) {
                    (Kind::Art, Some(symbol)) => Some(StrokeBrush { kind: BrushKind::Art, symbol }),
                    (Kind::Pattern, Some(symbol)) => Some(StrokeBrush { kind: BrushKind::Pattern, symbol }),
                    _ => None,
                };
            }
            if let Some(brush) = &mut stroke.brush {
                let name = symbols.iter().find(|(id, _)| *id == brush.symbol).map_or(tr("(gone)"), |(_, n)| n.as_str());
                egui::ComboBox::from_id_salt("stroke_brush_symbol").selected_text(name).width(110.0).show_ui(ui, |ui| {
                    for (id, name) in symbols {
                        ui.selectable_value(&mut brush.symbol, *id, name);
                    }
                });
            } else if symbols.is_empty() {
                ui.weak(tr("Make a symbol to brush with first."));
            }
        });
        ui.end_row();
    });
}

/// A drop-down to pick one of a few named values.
fn choice<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, value: &mut T, options: &[(T, &str)]) {
    let current = options.iter().find(|(v, _)| v == value).map_or("", |(_, name)| *name);
    egui::ComboBox::from_id_salt(id).selected_text(current).width(90.0).show_ui(ui, |ui| {
        for &(option, name) in options {
            ui.selectable_value(value, option, name);
        }
    });
}

/// A gradient's colours: each stop's colour and place along it, with
/// buttons to add one, take one away, or turn the gradient round.
fn gradient_stops(ui: &mut egui::Ui, gradient: &mut inkframe_core::ShapeGradient) {
    ui.vertical(|ui| {
        let mut remove = None;
        let count = gradient.stops.len();
        for (i, stop) in gradient.stops.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                color_button(ui, &mut stop.color);
                let mut percent = stop.at * 100.0;
                if ui.add(DragValue::new(&mut percent).range(0.0..=100.0).speed(0.5).suffix("%")).changed() {
                    stop.at = percent / 100.0;
                }
                // A gradient needs two colours to be one.
                if count > 2 && ui.small_button("×").on_hover_text(tr("Remove this colour")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            gradient.stops.remove(i);
        }
        ui.horizontal(|ui| {
            if ui
                .small_button(tr("+ Colour"))
                .on_hover_text(tr("Add a colour halfway along the widest gap"))
                .clicked()
            {
                let widest = gradient
                    .stops
                    .windows(2)
                    .enumerate()
                    .max_by(|a, b| (a.1[1].at - a.1[0].at).total_cmp(&(b.1[1].at - b.1[0].at)));
                if let Some((i, pair)) = widest {
                    let at = (pair[0].at + pair[1].at) / 2.0;
                    let color = gradient.color_at(at);
                    gradient.stops.insert(i + 1, inkframe_core::GradientStop { at, color });
                }
            }
            if ui.small_button(tr("Reverse")).on_hover_text(tr("Swap the start and end colours")).clicked() {
                for stop in &mut gradient.stops {
                    stop.at = 1.0 - stop.at;
                }
            }
        });
        gradient.tidy();
    });
}

fn document_section(ui: &mut egui::Ui, editor: &mut Editor) {
    let doc = &mut editor.doc;
    egui::Grid::new("document").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Stage size"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut doc.width).range(1..=16384).suffix(" w"));
            ui.add(DragValue::new(&mut doc.height).range(1..=16384).suffix(" h"));
        });
        ui.end_row();

        ui.label(tr("Frame rate"));
        egui::ComboBox::from_id_salt("frame_rate").selected_text(doc.frame_rate.to_string()).show_ui(
            ui,
            |ui| {
                for rate in FrameRate::PRESETS {
                    ui.selectable_value(&mut doc.frame_rate, rate, rate.to_string());
                }
            },
        );
        ui.end_row();

        ui.label(tr("Duration"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut doc.duration).range(1..=100_000).suffix(format!(" {}", tr("frames"))));
            ui.weak(format!("{:.2}s", doc.duration as f64 / doc.frame_rate.fps()));
        });
        ui.end_row();

        ui.label(tr("Background"));
        color_button(ui, &mut doc.background);
        ui.end_row();

        ui.label(tr("Stacking"));
        ui.checkbox(&mut doc.stack_by_depth, tr("By depth")).on_hover_text(STACK_BY_DEPTH);
        ui.end_row();
    });
    // Shortening the animation can strand the playhead past the end.
    editor.set_frame(editor.frame);
}

/// What "stack by depth" does, for its checkboxes.
pub const STACK_BY_DEPTH: &str = "Draw layers far to near by their Depth on each frame, so a layer that moves back \
     passes behind the others. Layers at the same depth keep the timeline's order. Off, the timeline's order \
     decides what's in front.";

/// The document's audio tracks: when each starts, how loud it is, and
/// whether it plays at all. Sound files are added with File › Import Audio.
pub fn audio_section(ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) -> Option<AudioAction> {
    let mut action = None;
    let header = tr_args("Audio ({count})", &[("count", &editor.doc.audio.len().to_string())]);
    egui::CollapsingHeader::new(header).id_salt("audio").default_open(!editor.doc.audio.is_empty()).show(
        ui,
        |ui| {
            if let Some(error) = audio.output_error() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr_args("No sound output: {error}", &[("error", error)]),
                );
            }
            ui.checkbox(&mut audio.scrub, tr("Scrub audio"))
                .on_hover_text(tr("Play each frame's sound as you move the playhead, for lip sync"));

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
                                tr("Remove this audio"),
                            ))
                            .clicked()
                        {
                            remove = Some(track.id);
                        }
                    });
                    egui::Grid::new("audio_track").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label(tr("Starts at"));
                        let mut shown = track.start + 1;
                        if ui
                            .add(
                                DragValue::new(&mut shown)
                                    .range(1..=last_frame + 1)
                                    .prefix(format!("{} ", tr("frame"))),
                            )
                            .changed()
                        {
                            track.start = shown - 1;
                        }
                        ui.end_row();

                        ui.label(tr("Volume"));
                        let mut percent = track.volume * 100.0;
                        if ui.add(egui::Slider::new(&mut percent, 0.0..=200.0).suffix("%")).changed() {
                            track.volume = percent / 100.0;
                        }
                        ui.end_row();

                        ui.label("");
                        ui.checkbox(&mut track.muted, tr("Mute"));
                        ui.end_row();
                    });
                    ui.weak(track.path.display().to_string());
                });
            }
            if let Some(id) = remove {
                editor.doc.remove_audio(id);
            }

            ui.add_space(4.0);
            if ui.button(tr("Import Audio…")).clicked() {
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
/// A section of the properties panel, under a title row: an arrow, an
/// icon, small spaced capitals and a hairline running on to the panel's
/// edge, so a long panel is easy to scan for the part you want. Clicking
/// the row folds the section away, or opens it again. Returns whether it's
/// open: a folded section shows nothing more.
///
/// What's folded is kept by `key` (a title can change, as the transform's
/// does with the frame), with the person's settings for next time.
fn section(ui: &mut egui::Ui, editor: &mut Editor, key: &str, title: impl Into<String>, icon: Icon) -> bool {
    let title = title.into();
    // While searching, a section is shown, open, only if it has what's
    // looked for.
    let searching = !editor.properties_search.trim().is_empty();
    if !searched_for(editor, key, &title) {
        return false;
    }
    let open = searching || !editor.folded_sections.contains(key);
    ui.add_space(8.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::click());
    // Named for screen readers (and tests), as egui's own folding headers are.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, &title));
    let colour = if response.hovered() { crate::theme::TEXT } else { crate::theme::TEXT_WEAK };
    let painter = ui.painter();
    let middle = rect.center().y;
    // The arrow: down when open, right when folded.
    let (x, s) = (rect.left() + 5.0, 3.5);
    let arrow = if open {
        vec![
            egui::pos2(x - s, middle - s / 2.0),
            egui::pos2(x, middle + s / 2.0),
            egui::pos2(x + s, middle - s / 2.0),
        ]
    } else {
        vec![
            egui::pos2(x - s / 2.0, middle - s),
            egui::pos2(x + s / 2.0, middle),
            egui::pos2(x - s / 2.0, middle + s),
        ]
    };
    painter.add(egui::Shape::line(arrow, egui::Stroke::new(1.4, colour)));
    crate::icons::paint(
        painter,
        icon,
        egui::Rect::from_center_size(egui::pos2(rect.left() + 21.0, middle), egui::vec2(13.0, 13.0)),
        colour,
    );
    let galley = egui::WidgetText::from(section_text(&title).color(colour)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        rect.width() - 40.0,
        egui::TextStyle::Body,
    );
    let text_at = egui::pos2(rect.left() + 33.0, middle - galley.size().y / 2.0);
    let text_end = text_at.x + galley.size().x;
    ui.painter().galley(text_at, galley, colour);
    if text_end + 8.0 < rect.right() {
        ui.painter().hline(
            text_end + 8.0..=rect.right(),
            middle,
            (1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if open {
        tr("Fold this section away")
    } else {
        tr("Show this section")
    });
    if response.clicked() {
        if open {
            editor.folded_sections.insert(key.to_owned());
        } else {
            editor.folded_sections.remove(key);
        }
    }
    // Folded or opened this frame, it's drawn as it now is from the next.
    open
}

/// What each section of the layer tab holds, by its key, for the search
/// box: the words someone might look for a setting by.
const SECTION_WORDS: &[(&str, &str)] = &[
    (
        "layer",
        tr_later(
            "name visible hidden show hide locked lock guide mask frames start end span blend mode multiply screen overlay lines texture pencil charcoal chalk grain web button click link",
        ),
    ),
    (
        "transform",
        tr_later(
            "position move x y rotation rotate turn angle scale size skew slant opacity alpha fade transparency depth z distance orient path straighten motion masked follow guide",
        ),
    ),
    (
        "parent and pivot",
        tr_later("parent attach rig child pivot centre center origin bends bend limits joint elbow knee"),
    ),
    ("easing", tr_later("easing ease in out linear hold curve timing tween speed slow overshoot anticipate")),
    (
        "skeleton",
        tr_later(
            "bone bones strength turn stretch keep area squash limits target ik dynamics spring damping swing delete",
        ),
    ),
    ("bones", tr_later("follows follow bind binding limb nearby region pin pins pinned")),
    (
        "smart bone",
        tr_later("smart action actions timeline reshape switch correction correct morph preview keys"),
    ),
    ("instance", tr_later("symbol instance loop play once single frame first length swap duplicate")),
    (
        "particles",
        tr_later(
            "particles particle emitter rate gravity wind drag wander turbulence speed spread direction lifetime size colour color spin seed preset sparks snow smoke fire",
        ),
    ),
    (
        "effects",
        tr_later(
            "effects effect colour color brightness tint advanced filters filter drop shadow blur glow adjust hue saturation contrast motion shutter",
        ),
    ),
    ("deformer", tr_later("deformer bend bends bones softness soft rubber hose")),
    ("free-form deformer", tr_later("free form freeform deformer grid warp points straighten bulge")),
    (
        "drawings",
        tr_later(
            "drawings drawing new duplicate delete swap substitution shape tween hints mouth lip sync flip",
        ),
    ),
    (
        "shapes",
        tr_later(
            "shapes shape fill stroke outline gradient linear radial colour color line width dash dashed dotted ends corners cap join brush art pattern text font size align spacing image picture actual size",
        ),
    ),
];

/// Whether the section `key` (titled `title`) has what the properties
/// panel's search box asks for: every word typed, somewhere in its title
/// or the words it holds. With nothing typed, every section does. Counts
/// the matches, so the panel can say when there are none.
fn searched_for(editor: &mut Editor, key: &str, title: &str) -> bool {
    let query = editor.properties_search.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let words = SECTION_WORDS.iter().find(|(k, _)| *k == key).map_or("", |(_, w)| *w);
    // The words in English as well as translated, so either finds it.
    let haystack = format!("{} {key} {words} {}", title.to_lowercase(), tr(words).to_lowercase());
    let found = query.split_whitespace().all(|term| haystack.contains(term));
    if found {
        editor.properties_matches += 1;
    }
    found
}

/// A section title's text, for titles that share their row with controls.
pub fn section_text(title: impl Into<String>) -> egui::RichText {
    egui::RichText::new(title.into().to_uppercase())
        .size(11.0)
        .extra_letter_spacing(0.6)
        .color(crate::theme::TEXT_WEAK)
}

/// Picks a web page action (see `web_actions.rs` in the core), or none:
/// what a button does, or a frame. Frames are shown 1-based.
pub fn web_action_editor(
    ui: &mut egui::Ui,
    id: &str,
    action: &mut Option<inkframe_core::WebAction>,
    last_frame: u32,
) {
    use inkframe_core::WebAction;
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        None,
        Play,
        Stop,
        GotoAndPlay,
        GotoAndStop,
        Link,
    }
    let kind = match action {
        None => Kind::None,
        Some(WebAction::Play) => Kind::Play,
        Some(WebAction::Stop) => Kind::Stop,
        Some(WebAction::GotoAndPlay(_)) => Kind::GotoAndPlay,
        Some(WebAction::GotoAndStop(_)) => Kind::GotoAndStop,
        Some(WebAction::OpenLink(_)) => Kind::Link,
    };
    let mut chosen = kind;
    choice(
        ui,
        id,
        &mut chosen,
        &[
            (Kind::None, tr("Nothing")),
            (Kind::Play, tr("Play")),
            (Kind::Stop, tr("Stop")),
            (Kind::GotoAndPlay, tr("Go to & play")),
            (Kind::GotoAndStop, tr("Go to & stop")),
            (Kind::Link, tr("Open link")),
        ],
    );
    if chosen != kind {
        let frame = match action {
            Some(WebAction::GotoAndPlay(f) | WebAction::GotoAndStop(f)) => *f,
            _ => 0,
        };
        *action = match chosen {
            Kind::None => None,
            Kind::Play => Some(WebAction::Play),
            Kind::Stop => Some(WebAction::Stop),
            Kind::GotoAndPlay => Some(WebAction::GotoAndPlay(frame)),
            Kind::GotoAndStop => Some(WebAction::GotoAndStop(frame)),
            Kind::Link => Some(WebAction::OpenLink("https://".into())),
        };
    }
    match action {
        Some(WebAction::GotoAndPlay(f) | WebAction::GotoAndStop(f)) => {
            let mut shown = *f + 1;
            if ui
                .add(DragValue::new(&mut shown).range(1..=last_frame + 1).prefix(format!("{} ", tr("frame"))))
                .changed()
            {
                *f = shown - 1;
            }
        }
        Some(WebAction::OpenLink(url)) => {
            ui.add(egui::TextEdit::singleline(url).desired_width(140.0));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use egui::vec2;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use inkframe_core::{Document, Point, Transform};

    use super::*;

    #[test]
    fn searching_shows_just_the_sections_that_have_it() {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("Ball", Vec::new(), 0, Transform::default());
        editor.select(Some(id));
        // Folded, but a search finds it all the same.
        editor.folded_sections.insert("effects".into());
        editor.properties_search = "blur".into();
        let mut h = Harness::new_ui_state(
            |ui, editor: &mut Editor| {
                egui::ScrollArea::vertical().show(ui, |ui| layer_tab(ui, editor));
            },
            editor,
        );
        h.set_size(vec2(360.0, 3000.0));
        h.run();
        assert!(h.query_by_label("Effects").is_some());
        assert!(h.query_by_label("Colour Effect").is_some(), "open while searching");
        assert!(h.query_by_label("Position").is_none());
        assert!(h.query_by_label("Easing").is_none());
        assert!(h.query_by_label("Blend").is_none(), "the layer's own settings hide too");

        h.state_mut().properties_search = "zzz".into();
        h.run();
        assert!(h.query_by_label_contains("No settings of this layer match").is_some());

        // Cleared, everything's back, folded sections folded.
        h.state_mut().properties_search.clear();
        h.run();
        assert!(h.query_by_label("Position").is_some() && h.query_by_label("Blend").is_some());
        assert!(h.query_by_label("Colour Effect").is_none());
    }

    #[test]
    fn a_section_folds_away_and_opens_again() {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("Ball", Vec::new(), 0, Transform::default());
        editor.select(Some(id));
        let mut h = Harness::new_ui_state(
            |ui, editor: &mut Editor| {
                egui::ScrollArea::vertical().show(ui, |ui| layer_tab(ui, editor));
            },
            editor,
        );
        h.set_size(vec2(360.0, 3000.0));
        h.run();
        assert!(h.query_by_label("Position").is_some());
        h.get_by_label("Transform at frame 1").click();
        h.run();
        h.run();
        assert!(h.query_by_label("Position").is_none(), "folded away");
        assert!(h.state().folded_sections.contains("transform"));
        h.get_by_label("Transform at frame 1").click();
        h.run();
        h.run();
        assert!(h.query_by_label("Position").is_some(), "open again");
        assert!(h.state().folded_sections.is_empty());
    }

    #[test]
    fn with_several_layers_selected_effects_can_be_given_to_them_all() {
        let mut editor = Editor::new(Document::default());
        let ids: Vec<LayerId> = (0..3)
            .map(|i| {
                editor.doc.add_layer(format!("L{i}"), Vec::new(), 0, Transform::at(Point::new(i as f32, 0.0)))
            })
            .collect();
        editor.select_many(&ids);
        let last = *ids.last().unwrap();
        let mut fx = editor.doc.layer(last).unwrap().track.effects_at(0.0);
        fx.color = inkframe_core::ColorEffect::Brightness { amount: 0.4 };
        editor.doc.layer_mut(last).unwrap().track.set_effects(0, fx.clone());

        let mut h = Harness::new_ui_state(
            |ui, editor: &mut Editor| {
                egui::ScrollArea::vertical().show(ui, |ui| layer_tab(ui, editor));
            },
            editor,
        );
        h.set_size(vec2(360.0, 3000.0));
        h.run();
        assert!(h.query_by_label("3 layers selected").is_some());
        h.get_by_label("Apply to the other 2 selected layers").click();
        h.run();
        for &id in &ids {
            assert_eq!(h.state().doc.layer(id).unwrap().track.effects_at(0.0), fx);
        }
    }
}
