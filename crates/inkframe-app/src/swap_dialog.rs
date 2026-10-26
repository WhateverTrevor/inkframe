//! Modify › Swap Symbol: the window for giving instances another symbol,
//! as Animate's does. Every symbol is shown as a picture; pick one and
//! Swap. The instances keep their animation, timing and playback.
//!
//! It swaps for the whole layer, or from the current frame on (a hand
//! swapped for another pose partway through, the arm it's on still
//! moving; see `Layer::symbol_swaps`). Duplicate Symbol makes a copy of the
//! one picked, to swap to and change without changing the original.

use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{LayerId, Symbol, SymbolId};

use crate::editor::Editor;
use crate::i18n::{tr, tr_args};

const CARD: f32 = 84.0;

/// Asks for the window from elsewhere (the properties panel), through
/// egui's memory, so the panel needn't reach the app.
pub fn request(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("swap-symbol-requested"), true));
}

fn take_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("swap-symbol-requested"))).unwrap_or(false)
}

#[derive(Default)]
pub struct SwapDialog {
    open: Option<Open>,
    /// Pictures of the symbols, re-made when a symbol changes.
    thumbnails: HashMap<SymbolId, (Symbol, Option<egui::TextureHandle>)>,
}

struct Open {
    /// The instance layers it swaps.
    layers: Vec<LayerId>,
    chosen: SymbolId,
    /// From the current frame on, rather than the whole layer.
    from_here: bool,
}

/// The selected instance layers.
fn instances(editor: &Editor) -> Vec<LayerId> {
    editor
        .selected_ids()
        .into_iter()
        .filter(|&id| editor.doc.layer(id).is_some_and(|l| l.instance.is_some()))
        .collect()
}

impl SwapDialog {
    pub fn can_open(editor: &Editor) -> bool {
        !instances(editor).is_empty()
    }

    /// Opens on the selected instances, with the symbol the last-picked one
    /// shows now chosen.
    pub fn open(&mut self, editor: &Editor) {
        let layers = instances(editor);
        let Some(&last) = layers.last() else { return };
        let Some(chosen) = editor.doc.layer(last).and_then(|l| l.symbol_at(editor.frame)) else { return };
        self.open = Some(Open { layers, chosen, from_here: false });
    }

    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        if take_request(ctx) {
            self.open(editor);
        }
        let Some(open) = &mut self.open else { return };
        open.layers.retain(|&id| editor.doc.layer(id).is_some_and(|l| l.instance.is_some()));
        if open.layers.is_empty() {
            self.open = None;
            return;
        }
        // From here on only means something past where every layer starts.
        let frame = editor.frame;
        let can_from_here =
            open.layers.iter().all(|&id| editor.doc.layer(id).is_some_and(|l| frame > l.start));
        if !can_from_here {
            open.from_here = false;
        }

        let mut keep_open = true;
        let mut swap = false;
        let mut duplicate = false;
        let symbols: Vec<SymbolId> = editor.doc.symbols.iter().map(|s| s.id).collect();
        let timeline = editor.timeline();
        egui::Window::new(tr("Swap Symbol"))
            .open(&mut keep_open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                let count = open.layers.len();
                ui.weak(if count == 1 {
                    tr("Pick a symbol for the selected instance. It keeps its animation.").to_owned()
                } else {
                    tr_args(
                        "Pick a symbol for the {count} selected instances. They keep their animation.",
                        &[("count", &count.to_string())],
                    )
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                        for &id in &symbols {
                            // Inside a symbol, leave out anything that would
                            // end up inside itself.
                            if !editor.doc.can_place(timeline, id) {
                                continue;
                            }
                            let response = card(ui, editor, &mut self.thumbnails, id, open.chosen == id);
                            if response.clicked() {
                                open.chosen = id;
                            }
                            if response.double_clicked() {
                                open.chosen = id;
                                swap = true;
                            }
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.radio_value(&mut open.from_here, false, tr("Whole layer"));
                    ui.add_enabled_ui(can_from_here, |ui| {
                        ui.radio_value(
                            &mut open.from_here,
                            true,
                            tr_args("From frame {frame} on", &[("frame", &(frame + 1).to_string())]),
                        )
                        .on_disabled_hover_text(tr("Move to a frame after the layer starts"));
                    });
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Duplicate Symbol"))
                        .on_hover_text(tr(
                            "Copy the chosen symbol, to swap to and change without changing the original",
                        ))
                        .clicked()
                    {
                        duplicate = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Swap")).clicked() {
                            swap = true;
                        }
                    });
                });
            });

        if duplicate && let Some(copy) = editor.doc.duplicate_symbol(open.chosen) {
            open.chosen = copy;
        }
        if swap {
            let from = open.from_here.then_some(frame);
            for &layer in &open.layers {
                editor.doc.swap_symbol(layer, open.chosen, from);
            }
            keep_open = false;
        }
        if !keep_open {
            self.open = None;
        }
        self.thumbnails.retain(|id, _| editor.doc.symbol(*id).is_some());
    }
}

/// One symbol: its picture and name, outlined when chosen.
fn card(
    ui: &mut egui::Ui,
    editor: &Editor,
    thumbnails: &mut HashMap<SymbolId, (Symbol, Option<egui::TextureHandle>)>,
    id: SymbolId,
    chosen: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(CARD, CARD + 18.0), Sense::click());
    let Some(symbol) = editor.doc.symbol(id) else { return response };
    let name = symbol.name.clone();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    let picture = Rect::from_min_size(rect.min, Vec2::splat(CARD));
    let visuals = ui.visuals();
    ui.painter().rect_filled(picture, 6.0, visuals.extreme_bg_color);
    if thumbnails.get(&id).is_none_or(|(s, _)| s != symbol) {
        let pixels = ((CARD - 10.0) * ui.ctx().pixels_per_point()).round() as u32;
        let texture =
            inkframe_render::render_symbol_thumbnail(&editor.doc, Some(&editor.images), id, 0, pixels).map(
                |pixmap| {
                    let size = [pixmap.width() as usize, pixmap.height() as usize];
                    let image = egui::ColorImage::from_rgba_premultiplied(size, pixmap.data());
                    ui.ctx().load_texture(format!("swap-{id:?}"), image, egui::TextureOptions::LINEAR)
                },
            );
        thumbnails.insert(id, (symbol.clone(), texture));
    }
    if let Some((_, Some(texture))) = thumbnails.get(&id) {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter().image(texture.id(), picture.shrink(5.0), uv, Color32::WHITE);
    }
    let outline = if chosen {
        Stroke::new(2.0, visuals.selection.stroke.color)
    } else if response.hovered() {
        Stroke::new(1.0, visuals.widgets.hovered.bg_stroke.color)
    } else {
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
    };
    ui.painter().rect_stroke(picture, 6.0, outline, StrokeKind::Inside);
    ui.painter().text(
        pos2(rect.center().x, picture.bottom() + 2.0),
        Align2::CENTER_TOP,
        name,
        FontId::proportional(11.0),
        visuals.text_color(),
    );
    response
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use inkframe_core::{Color, Document, Element, Point, Shape, Style, TimelineId, Transform};

    use super::*;

    type State = (SwapDialog, Editor);

    #[test]
    fn swapping_from_this_frame_keeps_what_came_before() {
        let mut editor = Editor::new(Document::default());
        let square = Element {
            shape: Shape::Rect { width: 40.0, height: 40.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let symbol = |name: &str, editor: &mut Editor| {
            let layer = editor.doc.add_layer(name, vec![square.clone()], 0, Transform::default());
            let id = editor.doc.convert_layer_to_symbol(layer, 0).unwrap();
            editor.doc.symbol_mut(id).unwrap().name = name.into();
            id
        };
        let (open_hand, fist) = (symbol("Open hand", &mut editor), symbol("Fist", &mut editor));
        let hand =
            editor.doc.add_instance_layer(TimelineId::Main, open_hand, 0, Point::new(200.0, 200.0)).unwrap();
        editor.select(Some(hand));
        editor.set_frame(10);

        let mut h =
            Harness::new_ui_state(|ui, (dialog, editor): &mut State| dialog.show(ui.ctx(), editor), {
                let mut dialog = SwapDialog::default();
                dialog.open(&editor);
                (dialog, editor)
            });
        h.run();
        h.get_by_label("Fist").click();
        h.run();
        h.get_by_label("From frame 11 on").click();
        h.run();
        h.get_by_label("Swap").click();
        h.run();

        let layer = h.state().1.doc.layer(hand).unwrap();
        assert_eq!(layer.symbol_at(9), Some(open_hand));
        assert_eq!(layer.symbol_at(10), Some(fist));
        assert!(h.state().0.open.is_none(), "closes after swapping");
    }
}
