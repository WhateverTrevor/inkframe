//! Small pictures of each layer at the current frame, for the timeline's
//! layer rows, as Animate and Photoshop show.
//!
//! A thumbnail is only redrawn when something that shows in it changes: the
//! frame, the layer itself, where its parents put it, the pixels of any
//! canvas it paints, the symbol it's an instance of, or a picture or font
//! finishing loading. Each of those is kept beside the texture and compared
//! next time; canvases and symbols share their data with the document, so
//! the comparison is mostly pointer checks.

use std::collections::HashMap;

use egui::{TextureHandle, TextureOptions};
use inkframe_core::{Affine, Canvas, Layer, LayerId, Shape, Symbol};

use crate::editor::Editor;

/// How many thumbnails may be redrawn in one UI frame. More than that wait
/// for the next frame, so scrubbing through a long stack stays smooth.
const REDRAWS_PER_FRAME: usize = 6;

#[derive(Default)]
pub struct LayerThumbnails {
    entries: HashMap<LayerId, Entry>,
    /// Redraws left this UI frame; reset by [`Self::begin_frame`].
    budget: usize,
}

struct Entry {
    source: Source,
    /// `None` when the layer drew nothing.
    texture: Option<TextureHandle>,
}

/// Everything a thumbnail depends on.
#[derive(PartialEq)]
struct Source {
    frame: u32,
    size: [u32; 2],
    layer: Layer,
    matrix: Affine,
    canvases: Vec<Canvas>,
    symbol: Option<Symbol>,
    /// The image library's and the font index's generations, for a layer
    /// that shows pictures or text (zero otherwise, so a font finishing
    /// loading doesn't redraw every thumbnail).
    loaded: (u64, u64),
}

impl LayerThumbnails {
    /// Call once per UI frame before asking for thumbnails; also forgets
    /// layers that have been deleted.
    pub fn begin_frame(&mut self, editor: &Editor) {
        self.budget = REDRAWS_PER_FRAME;
        self.entries.retain(|id, _| editor.doc.layer(*id).is_some());
    }

    /// The thumbnail of `id`, `size` pixels, redrawn first if it's out of
    /// date and there's budget left (otherwise the old one, and another UI
    /// frame is requested to catch up).
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        editor: &Editor,
        id: LayerId,
        size: [u32; 2],
    ) -> Option<egui::TextureId> {
        let source = source(editor, id, size)?;
        let stale = self.entries.get(&id).is_none_or(|entry| entry.source != source);
        if stale {
            if self.budget == 0 {
                ctx.request_repaint();
            } else {
                self.budget -= 1;
                let texture = inkframe_render::render_layer_thumbnail(
                    &editor.doc,
                    Some(&editor.images),
                    id,
                    editor.frame,
                    size[0],
                    size[1],
                )
                .map(|pixmap| {
                    let image =
                        egui::ColorImage::from_rgba_premultiplied(size.map(|n| n as usize), pixmap.data());
                    ctx.load_texture(format!("layer-{id:?}"), image, TextureOptions::LINEAR)
                });
                self.entries.insert(id, Entry { source, texture });
            }
        }
        self.entries.get(&id)?.texture.as_ref().map(TextureHandle::id)
    }
}

fn source(editor: &Editor, id: LayerId, size: [u32; 2]) -> Option<Source> {
    let doc = &editor.doc;
    let layer = doc.layer(id)?;
    let frame = editor.frame;
    let canvases = layer
        .shown_elements(frame)
        .iter()
        .filter_map(|element| match element.shape {
            Shape::Paint { canvas, .. } => doc.canvas(canvas).cloned(),
            _ => None,
        })
        .collect();
    // A symbol could hold either, so an instance counts as both.
    let shows = |wanted: fn(&Shape) -> bool| {
        layer.instance.is_some() || layer.all_elements().any(|e| wanted(&e.shape))
    };
    let images = if shows(|s| matches!(s, Shape::Image { .. })) { editor.images.generation() } else { 0 };
    let fonts = if shows(|s| matches!(s, Shape::Text(_))) { inkframe_render::fonts_generation() } else { 0 };
    Some(Source {
        frame,
        size,
        layer: layer.clone(),
        matrix: doc.layer_matrix(layer, frame),
        canvases,
        symbol: layer.instance.and_then(|instance| doc.symbol(instance.symbol)).cloned(),
        loaded: (images, fonts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Color, Document, Element, Point, Style, Transform};

    fn square_editor() -> (Editor, LayerId) {
        let mut doc = Document::default();
        let square = Element {
            shape: Shape::Rect { width: 400.0, height: 400.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::rgb(255, 0, 0)), stroke: None, gradient: None },
        };
        let id = doc.add_layer("Square", vec![square], 0, Transform::at(Point::new(960.0, 540.0)));
        (Editor::new(doc), id)
    }

    #[test]
    fn thumbnails_are_redrawn_only_when_the_layer_changes() {
        let (mut editor, id) = square_editor();
        let ctx = egui::Context::default();
        let mut thumbnails = LayerThumbnails::default();
        thumbnails.begin_frame(&editor);
        let first = thumbnails.get(&ctx, &editor, id, [48, 27]).expect("drawn");
        assert_eq!(thumbnails.budget, REDRAWS_PER_FRAME - 1);

        thumbnails.begin_frame(&editor);
        assert_eq!(thumbnails.get(&ctx, &editor, id, [48, 27]), Some(first));
        assert_eq!(thumbnails.budget, REDRAWS_PER_FRAME, "unchanged, so not redrawn");

        editor.doc.layer_mut(id).unwrap().track.set_pose(0, Transform::at(Point::new(100.0, 100.0)));
        thumbnails.begin_frame(&editor);
        thumbnails.get(&ctx, &editor, id, [48, 27]);
        assert_eq!(thumbnails.budget, REDRAWS_PER_FRAME - 1, "moved, so redrawn");
    }

    #[test]
    fn redraws_wait_their_turn_beyond_the_budget() {
        let (editor, id) = square_editor();
        let ctx = egui::Context::default();
        let mut thumbnails = LayerThumbnails::default();
        thumbnails.begin_frame(&editor);
        thumbnails.budget = 0;
        assert_eq!(thumbnails.get(&ctx, &editor, id, [48, 27]), None);
        thumbnails.begin_frame(&editor);
        assert!(thumbnails.get(&ctx, &editor, id, [48, 27]).is_some());
    }
}
