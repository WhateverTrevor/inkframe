//! The Library panel: the document's symbols, with a thumbnail of each.
//!
//! Symbols are made with Modify › Convert to Symbol (F8). From here they can
//! be renamed, placed on the stage again, or deleted once nothing uses them.

use std::collections::HashMap;

use egui::{Sense, Vec2};
use inkframe_core::{Symbol, SymbolId};

use crate::editor::Editor;

const THUMBNAIL_SIZE: f32 = 40.0;

/// A thumbnail plus the symbol it was rendered from, so we can tell when
/// it's out of date.
struct Thumbnail {
    source: Symbol,
    /// The image library's and fonts' generations when rendered, so a
    /// thumbnail redraws once a picture or font inside the symbol has loaded.
    images: (u64, u64),
    texture: Option<egui::TextureHandle>,
}

#[derive(Default)]
pub struct LibraryView {
    thumbnails: HashMap<SymbolId, Thumbnail>,
}

impl LibraryView {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        // Forget thumbnails of deleted symbols.
        self.thumbnails.retain(|id, _| editor.doc.symbol(*id).is_some());

        let header = format!("Library ({})", editor.doc.symbols.len());
        egui::CollapsingHeader::new(header).id_salt("library").default_open(true).show(ui, |ui| {
            if editor.doc.symbols.is_empty() {
                ui.weak("No symbols yet. Select a layer and choose Modify › Convert to Symbol (F8).");
                return;
            }

            let ids: Vec<SymbolId> = editor.doc.symbols.iter().map(|s| s.id).collect();
            let can_place: Vec<SymbolId> = ids.iter().copied().filter(|id| editor.can_place(*id)).collect();
            let mut place = None;
            let mut edit = None;
            let mut delete = None;
            for id in ids {
                ui.push_id(id, |ui| {
                    ui.horizontal(|ui| {
                        self.thumbnail(ui, editor, id);
                        ui.vertical(|ui| {
                            let uses = editor.doc.symbol_use_count(id);
                            let Some(symbol) = editor.doc.symbol_mut(id) else { return };
                            ui.add(egui::TextEdit::singleline(&mut symbol.name).desired_width(140.0));
                            ui.horizontal(|ui| {
                                let plural = if uses == 1 { "" } else { "s" };
                                ui.weak(format!("{} frames · {uses} use{plural}", symbol.duration));
                                if ui
                                    .small_button("Edit")
                                    .on_hover_text("Open this symbol to change it")
                                    .clicked()
                                {
                                    edit = Some(id);
                                }
                                let placeable = can_place.contains(&id);
                                if ui
                                    .add_enabled(placeable, egui::Button::new("Place").small())
                                    .on_hover_text("Add an instance to the stage")
                                    .on_disabled_hover_text("A symbol can't be placed inside itself")
                                    .clicked()
                                {
                                    place = Some(id);
                                }
                                let tip = if uses == 0 { "Delete this symbol" } else { "Still in use" };
                                if ui
                                    .add_enabled(uses == 0, egui::Button::new("🗑").small())
                                    .on_hover_text(tip)
                                    .on_disabled_hover_text(tip)
                                    .clicked()
                                {
                                    delete = Some(id);
                                }
                            });
                        });
                    });
                });
                ui.add_space(4.0);
            }

            if let Some(id) = place {
                editor.place_symbol(id);
            }
            if let Some(id) = edit {
                editor.enter_symbol(id, 0);
            }
            if let Some(id) = delete {
                editor.doc.remove_symbol(id);
            }
        });
    }

    /// Draws a symbol's thumbnail, re-rendering it only when the symbol has
    /// changed since last time.
    fn thumbnail(&mut self, ui: &mut egui::Ui, editor: &Editor, id: SymbolId) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(THUMBNAIL_SIZE), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let Some(symbol) = editor.doc.symbol(id) else { return };

        // Either changing means a picture or some text inside may look different.
        let images = (editor.images.generation(), inkframe_render::fonts_generation());
        let stale = self.thumbnails.get(&id).is_none_or(|t| t.source != *symbol || t.images != images);
        if stale {
            // Render at the display's pixel density so thumbnails are sharp.
            let pixels = (THUMBNAIL_SIZE * ui.ctx().pixels_per_point()).round() as u32;
            let texture =
                inkframe_render::render_symbol_thumbnail(&editor.doc, Some(&editor.images), id, 0, pixels)
                    .map(|pixmap| {
                        let size = [pixmap.width() as usize, pixmap.height() as usize];
                        let image = egui::ColorImage::from_rgba_premultiplied(size, pixmap.data());
                        ui.ctx().load_texture(format!("symbol-{id:?}"), image, egui::TextureOptions::LINEAR)
                    });
            self.thumbnails.insert(id, Thumbnail { source: symbol.clone(), images, texture });
        }

        if let Some(texture) = self.thumbnails.get(&id).and_then(|t| t.texture.as_ref()) {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture.id(), rect.shrink(2.0), uv, egui::Color32::WHITE);
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "empty",
                egui::FontId::proportional(10.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
}
