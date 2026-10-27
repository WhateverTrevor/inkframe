//! How a layer moves in the Properties panel: its transform (each value
//! keyed as it's changed), its motion path, the easing of its tweens and
//! the shape tween's controls.

use egui::{Color32, DragValue, Sense, Stroke as EguiStroke, vec2};
use inkframe_core::{Easing, LayerId, Property, Tween};

use super::sections::section;
use super::{apply_to_selected_button, mask_section};
use crate::easing_editor;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::icons::Icon;

pub(super) fn transform_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
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

/// The shape tween through this frame's span: whether it morphs into the
/// next swap, its easing, and its shape hints.
pub(super) fn shape_tween_controls(ui: &mut egui::Ui, editor: &mut Editor) {
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

/// Options for a layer that moves: following a motion guide, turning to
/// face its direction of travel, and undoing any curves in its motion path.
pub(super) fn motion_path_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
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
pub(super) fn key_toggle(ui: &mut egui::Ui, property: Property, is_key: bool) -> egui::Response {
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

pub(super) fn easing_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
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
pub(super) fn easing_combo(ui: &mut egui::Ui, id_salt: Property, easing: &mut Easing) -> bool {
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
