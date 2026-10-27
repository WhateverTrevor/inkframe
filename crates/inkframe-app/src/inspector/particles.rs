//! A particle emitter's settings in the Properties panel.

use egui::DragValue;
use inkframe_core::LayerId;

use super::sections::section;
use crate::convert::color_button;
use crate::editor::Editor;
use crate::i18n::tr;
use crate::icons::Icon;

/// A particle layer's emitter: how many particles, where from, how they
/// move and how they change as they age (see `particles.rs` in the core).
pub(super) fn particles_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
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
