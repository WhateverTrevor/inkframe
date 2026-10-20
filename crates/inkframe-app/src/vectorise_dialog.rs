//! Modify › Vectorise: traces the selected paint or image layer into
//! vector shapes (see `inkframe_core::vectorise`).
//!
//! The window previews the trace of the drawing showing now, worked out on
//! a background thread so the sliders stay smooth on a large scan.
//! Vectorise traces every drawing of the layer into a new layer above it
//! and hides the original, so nothing is lost.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::TextureOptions;
use inkframe_core::{
    Bitmap, Color, Document, Element, LayerId, Shape, Style, TraceMode, TraceOptions, Transform, trace,
};
use inkframe_render::{RenderOptions, render_frame};

use crate::editor::Editor;

/// The preview's longest side, in pixels.
const PREVIEW: f32 = 360.0;

#[derive(Default)]
pub struct VectoriseDialog {
    open: Option<Open>,
    /// Remembered between uses, as most tools' settings are.
    options: TraceOptions,
}

struct Open {
    layer: LayerId,
    /// The picture showing when the window opened, for the preview.
    bitmap: Arc<Bitmap>,
    /// The settings the preview shows (or is being worked out for).
    previewed: Option<TraceOptions>,
    working: Option<Receiver<Preview>>,
    preview: Option<(egui::TextureHandle, usize)>,
}

/// A finished preview: the picture and how many shapes it has.
struct Preview {
    image: egui::ColorImage,
    shapes: usize,
}

impl VectoriseDialog {
    /// Opens the window for the selected layer, if it holds a painting or
    /// picture on this frame.
    pub fn open(&mut self, editor: &Editor) {
        let Some(layer) = editor.selected_layer() else { return };
        let bitmap = layer.elements_at(editor.frame).iter().find_map(|e| match e.shape {
            Shape::Paint { canvas, width, height } => {
                Some(Bitmap { width, height, pixels: editor.doc.canvas(canvas)?.to_pixels() })
            }
            Shape::Image { image, .. } => {
                let pixmap = editor.images.full_size(image)?;
                Some(Bitmap {
                    width: pixmap.width(),
                    height: pixmap.height(),
                    pixels: pixmap.data().to_vec(),
                })
            }
            _ => None,
        });
        if let Some(bitmap) = bitmap {
            self.open = Some(Open {
                layer: layer.id,
                bitmap: Arc::new(bitmap),
                previewed: None,
                working: None,
                preview: None,
            });
        }
    }

    pub fn set_mode(&mut self, mode: TraceMode) {
        self.options.mode = mode;
    }

    /// Whether the selected layer has something to vectorise.
    pub fn can_vectorise(editor: &Editor) -> bool {
        editor.selected_layer().is_some_and(|l| {
            l.elements_at(editor.frame)
                .iter()
                .any(|e| matches!(e.shape, Shape::Paint { .. } | Shape::Image { .. }))
        })
    }

    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        let Some(open) = &mut self.open else { return };
        let options = &mut self.options;
        let mut window_open = true;
        let mut finished = None;
        egui::Window::new("Vectorise").open(&mut window_open).collapsible(false).resizable(false).show(
            ctx,
            |ui| {
                controls(ui, options);
                ui.add_space(6.0);
                match &open.preview {
                    Some((texture, shapes)) => {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(texture.size_vec2()));
                        let working = if open.working.is_some() { " · updating…" } else { "" };
                        ui.weak(format!("{shapes} shapes{working}"));
                    }
                    None => {
                        ui.spinner();
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Vectorise").clicked() {
                        finished = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        finished = Some(false);
                    }
                });
            },
        );
        if !window_open {
            finished = Some(false);
        }
        update_preview(ctx, open, *options);

        if let Some(apply) = finished {
            let open = self.open.take().expect("open");
            if apply {
                vectorise(editor, open.layer, &self.options);
            }
        }
    }

    /// Vectorises straight away with the current settings, for screenshots.
    pub fn apply(&mut self, editor: &mut Editor) {
        if let Some(open) = self.open.take() {
            vectorise(editor, open.layer, &self.options);
        }
    }
}

/// Traces `layer` into a new layer above it, and selects that.
fn vectorise(editor: &mut Editor, layer: LayerId, options: &TraceOptions) {
    let images = &editor.images;
    let traced = editor.doc.vectorise_layer(layer, options, |id| {
        let pixmap = images.full_size(id)?;
        Some(Bitmap { width: pixmap.width(), height: pixmap.height(), pixels: pixmap.data().to_vec() })
    });
    if let Some(id) = traced {
        editor.select(Some(id));
    }
}

fn controls(ui: &mut egui::Ui, options: &mut TraceOptions) {
    let ink = matches!(options.mode, TraceMode::Ink { .. });
    ui.horizontal(|ui| {
        if ui.selectable_label(ink, "Line art").on_hover_text("Dark lines on paper, as from a scan").clicked()
            && !ink
        {
            options.mode = TraceMode::Ink { threshold: 128 };
        }
        if ui.selectable_label(!ink, "Colours").on_hover_text("Every colour, each its own shapes").clicked()
            && ink
        {
            options.mode = TraceMode::Colours { colours: 8, skip_white: true };
        }
    });
    egui::Grid::new("vectorise").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        match &mut options.mode {
            TraceMode::Ink { threshold } => {
                ui.label("Threshold");
                ui.add(egui::Slider::new(threshold, 1..=254))
                    .on_hover_text("Pixels darker than this are ink. Raise it to pick up faint pencil.");
                ui.end_row();
            }
            TraceMode::Colours { colours, skip_white } => {
                ui.label("Colours");
                ui.add(egui::Slider::new(colours, 2..=32));
                ui.end_row();
                ui.label("");
                ui.checkbox(skip_white, "Leave out white paper");
                ui.end_row();
            }
        }
        ui.label("Minimum area");
        ui.add(egui::Slider::new(&mut options.min_area, 1..=400).logarithmic(true).suffix(" px"))
            .on_hover_text("Specks smaller than this are dropped, and holes this small filled");
        ui.end_row();
        ui.label("Smoothing");
        ui.add(egui::Slider::new(&mut options.smoothing, 0.25..=4.0).fixed_decimals(2))
            .on_hover_text("Higher follows the pixels less closely, for smoother curves and fewer points");
        ui.end_row();
    });
}

/// Starts working out a preview when the settings have changed (and one
/// isn't already being worked out), and picks up a finished one.
fn update_preview(ctx: &egui::Context, open: &mut Open, options: TraceOptions) {
    if let Some(working) = &open.working {
        match working.try_recv() {
            Ok(preview) => {
                let texture = ctx.load_texture("vectorise-preview", preview.image, TextureOptions::LINEAR);
                open.preview = Some((texture, preview.shapes));
                open.working = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint();
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => open.working = None,
        }
    }
    if open.previewed == Some(options) {
        return;
    }
    open.previewed = Some(options);
    let (sender, receiver) = mpsc::channel();
    let bitmap = Arc::clone(&open.bitmap);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        if let Some(preview) = render_preview(&bitmap, &options) {
            let _ = sender.send(preview);
            ctx.request_repaint();
        }
    });
    open.working = Some(receiver);
}

/// The trace of `bitmap`, drawn small on white.
fn render_preview(bitmap: &Bitmap, options: &TraceOptions) -> Option<Preview> {
    let traced = trace(bitmap, options);
    let mut doc = Document::default();
    doc.width = bitmap.width;
    doc.height = bitmap.height;
    doc.background = Color::WHITE;
    let elements = traced
        .iter()
        .map(|t| Element {
            shape: Shape::Path(t.path.clone()),
            style: Style { fill: Some(t.color), stroke: None, gradient: None },
        })
        .collect();
    doc.add_layer("Traced", elements, 0, Transform::default());
    let scale = PREVIEW / bitmap.width.max(bitmap.height).max(1) as f32;
    let pixmap = render_frame(&doc, 0, &RenderOptions { scale, ..RenderOptions::default() })?;
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    Some(Preview {
        image: egui::ColorImage::from_rgba_premultiplied(size, pixmap.data()),
        shapes: traced.len(),
    })
}
