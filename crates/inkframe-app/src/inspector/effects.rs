//! A layer's color effect and filters in the Properties panel.

use egui::DragValue;
use inkframe_core::{Color, LayerId, Property};

use super::animation::key_toggle;
use super::sections::section;
use crate::convert::color_button;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::icons::Icon;

/// The layer's colour effect and filters, as in Animate's Properties.
pub(super) fn effects_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
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
        layer.track.set_effects(frame, fx.clone());
        // The other selected layers take the change too, as with every
        // other setting here.
        editor.carry_effects(id, &fx);
    }
}

/// One filter's settings.
pub(super) fn filter_controls(ui: &mut egui::Ui, kind: &mut inkframe_core::FilterKind) {
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
