//! The properties panel on the right: numeric editing of the selected layer,
//! its keyframe easing and shapes, and the document settings.

use egui::{Color32, DragValue, Sense, Stroke as EguiStroke, vec2};
use inkframe_core::{
    Color, Easing, FrameRate, ImageId, Layer, LayerId, Playback, Property, Shape, Stroke, Style, SymbolId,
    Text, TextAlign, Tween,
};

use crate::audio_engine::AudioEngine;
use crate::convert::color_button;
use crate::easing_editor;
use crate::editor::Editor;

/// Shows the panel's contents. The caller provides the scroll area, so the
/// library can share it.
pub fn show(ui: &mut egui::Ui, editor: &mut Editor) {
    if let Some(id) = editor.selection {
        layer_section(ui, editor, id);
        ui.add_space(8.0);
    }
    egui::CollapsingHeader::new("Document")
        .default_open(editor.selection.is_none())
        .show(ui, |ui| document_section(ui, editor));
}

fn layer_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let last_frame = editor.last_frame();
    let Some(layer) = editor.doc.layer_mut(id) else { return };

    ui.heading("Layer");
    ui.text_edit_singleline(&mut layer.name);
    ui.horizontal(|ui| {
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

    ui.add_space(8.0);
    transform_section(ui, editor, id, frame);
    ui.add_space(8.0);
    easing_section(ui, editor, id, frame);
    ui.add_space(8.0);
    instance_section(ui, editor, id);
    shapes_section(ui, editor, id);
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
        for (property, is_key) in Property::ALL.into_iter().zip(keyed) {
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
                Property::Opacity => {
                    changed |= ui.add(egui::Slider::new(&mut opacity_pct, 0.0..=100.0).suffix("%")).changed();
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
    }
    if let Some(property) = toggled {
        editor.toggle_property_key(id, property);
    }
    motion_path_section(ui, editor, id);
    mask_section(ui, editor, id);
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
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    let count = layer.elements.len();
    for (i, element) in layer.elements.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            let title = match &element.shape {
                Shape::Rect { .. } => "Rectangle",
                Shape::Ellipse { .. } => "Oval",
                Shape::Path(_) => "Stroke",
                Shape::Image { .. } => "Image",
                Shape::Text(_) => "Text",
            };
            ui.strong(if count > 1 { format!("{title} {}", i + 1) } else { title.to_owned() });
            if let Shape::Image { image, width, height } = &mut element.shape {
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
        Shape::Path(_) | Shape::Image { .. } | Shape::Text(_) => {}
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
                        if ui.small_button("🗑").on_hover_text("Remove this audio").clicked() {
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
