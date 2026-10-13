//! The properties panel on the right: numeric editing of the selected layer,
//! its keyframe easing and shapes, and the document settings.

use egui::{Color32, DragValue, Sense, Stroke as EguiStroke, vec2};
use inkframe_core::{Color, Easing, FrameRate, LayerId, Shape, Stroke, Style};

use crate::convert::color_button;
use crate::editor::Editor;

pub fn show(ui: &mut egui::Ui, editor: &mut Editor) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(id) = editor.selection {
            layer_section(ui, editor, id);
            ui.add_space(8.0);
        }
        egui::CollapsingHeader::new("Document")
            .default_open(editor.selection.is_none())
            .show(ui, |ui| document_section(ui, editor));
    });
}

fn layer_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer_mut(id) else { return };

    ui.heading("Layer");
    ui.text_edit_singleline(&mut layer.name);
    ui.horizontal(|ui| {
        ui.checkbox(&mut layer.visible, "Visible");
        ui.checkbox(&mut layer.locked, "Locked");
    });

    ui.add_space(8.0);
    transform_section(ui, editor, id, frame);
    ui.add_space(8.0);
    easing_section(ui, editor, id, frame);
    ui.add_space(8.0);
    shapes_section(ui, editor, id);
}

fn transform_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    let Some(layer) = editor.doc.layer(id) else { return };
    let is_key = layer.track.key_at(frame).is_some();
    let mut pose = layer.transform_at(frame);

    ui.horizontal(|ui| {
        ui.strong(format!("Transform at frame {}", frame + 1));
        let (badge, tip) = if is_key {
            ("keyframe", "This frame has a keyframe")
        } else {
            ("tweened", "Changing a value here adds a keyframe on this frame")
        };
        ui.weak(badge).on_hover_text(tip);
    });

    // Scale and opacity are stored as fractions but edited as percentages,
    // which is what people expect to type.
    let mut scale_pct = [pose.scale.x * 100.0, pose.scale.y * 100.0];
    let mut opacity_pct = pose.opacity * 100.0;
    let mut changed = false;

    egui::Grid::new("transform").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Position");
        ui.horizontal(|ui| {
            changed |= ui.add(DragValue::new(&mut pose.position.x).prefix("x ").speed(1.0)).changed();
            changed |= ui.add(DragValue::new(&mut pose.position.y).prefix("y ").speed(1.0)).changed();
        });
        ui.end_row();

        ui.label("Rotation");
        changed |= ui.add(DragValue::new(&mut pose.rotation).suffix("°").speed(1.0)).changed();
        ui.end_row();

        ui.label("Scale");
        ui.horizontal(|ui| {
            changed |=
                ui.add(DragValue::new(&mut scale_pct[0]).prefix("x ").suffix("%").speed(1.0)).changed();
            changed |=
                ui.add(DragValue::new(&mut scale_pct[1]).prefix("y ").suffix("%").speed(1.0)).changed();
        });
        ui.end_row();

        ui.label("Opacity");
        changed |= ui.add(egui::Slider::new(&mut opacity_pct, 0.0..=100.0).suffix("%")).changed();
        ui.end_row();
    });

    if changed {
        pose.scale.x = scale_pct[0] / 100.0;
        pose.scale.y = scale_pct[1] / 100.0;
        pose.opacity = opacity_pct / 100.0;
        editor.set_pose(id, pose);
    }
}

fn easing_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    let Some(track) = editor.doc.layer_mut(id).map(|l| &mut l.track) else { return };
    let next_frame = track.next_key(frame).map(|k| k.frame);
    let Some(key) = track.tween_start_mut(frame) else {
        ui.weak("No tween here. Move to a later frame and change the layer to create one.");
        return;
    };

    let end = next_frame.expect("a tween always has a following key");
    ui.strong(format!("Tween, frames {}–{}", key.frame + 1, end + 1));

    ui.horizontal(|ui| {
        ui.label("Easing");
        egui::ComboBox::from_id_salt("easing").selected_text(key.easing.label()).show_ui(ui, |ui| {
            for preset in Easing::PRESETS {
                ui.selectable_value(&mut key.easing, preset, preset.label());
            }
            // "Custom" starts from the current curve so it's a tweak, not a reset.
            let [x1, y1, x2, y2] = key.easing.control_points().unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let custom = Easing::CubicBezier { x1, y1, x2, y2 };
            let is_custom = matches!(key.easing, Easing::CubicBezier { .. });
            if ui.selectable_label(is_custom, "Custom").clicked() && !is_custom {
                key.easing = custom;
            }
        });
    });

    if let Easing::CubicBezier { x1, y1, x2, y2 } = &mut key.easing {
        ui.horizontal(|ui| {
            ui.add(DragValue::new(x1).range(0.0..=1.0).speed(0.01).prefix("x1 "));
            ui.add(DragValue::new(y1).speed(0.01).prefix("y1 "));
            ui.add(DragValue::new(x2).range(0.0..=1.0).speed(0.01).prefix("x2 "));
            ui.add(DragValue::new(y2).speed(0.01).prefix("y2 "));
        });
    }
    easing_graph(ui, key.easing);
}

/// A small plot of the easing curve: time across, progress up.
fn easing_graph(ui: &mut egui::Ui, easing: Easing) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(220.0), 90.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

    // Leave headroom so overshooting curves (y outside 0–1) stay visible.
    let plot = rect.shrink2(vec2(8.0, 18.0));
    let to_screen =
        |t: f32, v: f32| egui::pos2(plot.left() + t * plot.width(), plot.bottom() - v * plot.height());

    let guide = EguiStroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4));
    painter.line_segment([to_screen(0.0, 0.0), to_screen(1.0, 1.0)], guide);

    let points = (0..=48)
        .map(|i| {
            let t = i as f32 / 48.0;
            to_screen(t, easing.apply(t))
        })
        .collect();
    painter.line(points, (2.0, Color32::from_rgb(0x8a, 0x7c, 0xd8)));
}

fn shapes_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    let count = layer.elements.len();
    for (i, element) in layer.elements.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            let title = match &element.shape {
                Shape::Rect { .. } => "Rectangle",
                Shape::Ellipse { .. } => "Oval",
                Shape::Path(_) => "Stroke",
            };
            ui.strong(if count > 1 { format!("{title} {}", i + 1) } else { title.to_owned() });
            shape_size_editor(ui, &mut element.shape);
            style_editor(ui, &mut element.style);
        });
    }
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
        // Freehand paths are edited by redrawing, for now.
        Shape::Path(_) => {}
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
