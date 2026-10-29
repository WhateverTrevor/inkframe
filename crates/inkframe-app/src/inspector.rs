//! The properties panel on the right: numeric editing of the selected layer,
//! its keyframe easing and shapes, and the document settings.

mod animation;
mod brush;
mod document;
mod effects;
mod particles;
mod rigging;
mod sections;
mod shapes;

use egui::DragValue;
use inkframe_core::{Layer, LayerId, Playback, SymbolId};

use crate::audio_engine::AudioEngine;
use crate::convert::color_button;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::icons::Icon;
use animation::{easing_section, shape_tween_controls, transform_section};
use brush::brush_section;
use document::document_section;
use effects::{apply_to_selected_button, effects_section};
use particles::particles_section;
use rigging::{deformer_section, rig_section, skeleton_section, switch_controls, warp_section};
use sections::{search_box, searched_for, section};
use shapes::{choice, shapes_section, style_editor};

// The parts of the panel used elsewhere, kept at their old paths.
pub(crate) use brush::percent;
pub(crate) use document::{AudioAction, STACK_BY_DEPTH, audio_section, web_action_editor};
pub(crate) use rigging::binding_menu;
pub(crate) use sections::section_text;

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
        // A painting or selecting tool newly picked (or picked when the
        // panel first shows) wants its settings at hand.
        let tool_changed = seen.is_none_or(|s| s.tool != now.tool);
        if tool_changed && (now.tool.paints() || now.tool.selects()) {
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
            if inkframe_core::Document::can_clip(layer) {
                ui.checkbox(&mut layer.clipped, tr("Clipped")).on_hover_text(tr(
                    "Clipped to the layer beneath: shown only where that layer has paint, as Photoshop's clipping mask (⌥⌘G)",
                ));
            }
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
    if header {
        layer_mask_controls(ui, editor, id);
    }

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

/// The layer's mask (see `layer_masks.rs`): a button to add one, or, with
/// one, whether the brush paints it or the layer, whether it's on, and
/// applying or deleting it.
fn layer_mask_controls(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some(layer) = editor.doc.layer(id) else { return };
    if !inkframe_core::Document::can_clip(layer) {
        return;
    }
    let mask = layer.layer_mask;
    let painting = editor.doc.paint_canvas_at(id, editor.frame).is_some();
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Layer mask")).on_hover_text(tr(
            "A mask painted over the layer: dark hides it, light shows it, and erasing the mask brings it back",
        ));
        let Some(mask) = mask else {
            if ui.button(tr("Add")).clicked() {
                editor.add_layer_mask();
            }
            return;
        };
        let mut paint_mask = editor.painting_mask();
        if ui.selectable_label(paint_mask, tr("Paint the mask")).clicked() {
            paint_mask = !paint_mask;
            editor.set_editing_mask(id, paint_mask);
        }
        let mut on = mask.enabled;
        if ui.checkbox(&mut on, tr("On")).changed() {
            editor.toggle_layer_mask();
        }
        if painting && ui.button(tr("Apply")).on_hover_text(tr("Erase what the mask hides, and remove it")).clicked() {
            editor.apply_layer_mask();
        }
        if ui.button(tr("Delete")).clicked() {
            editor.delete_layer_mask();
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

#[cfg(test)]
mod tests;
