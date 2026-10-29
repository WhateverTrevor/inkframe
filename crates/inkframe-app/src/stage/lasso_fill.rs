//! Lasso Fill, as Clip Studio's and Procreate's: draw round an area and it
//! fills with the paint colour when you let go, the quickest way to block
//! in colour. On a paint layer it fills pixels (inside any selection); on
//! a drawn layer it adds a filled shape to the colour art, under the inked
//! lines, as the Paint Bucket's fills go.

use egui::{Color32, Pos2};
use inkframe_core::{Element, Point, Shape, Style, fit_freehand_path, lasso_fill};

use super::painting::canvas_point;
use super::{StageView, View, layer_to_local};
use crate::editor::Editor;

/// How far the pointer moves before another point of the outline is
/// taken, in screen points.
const STEP: f32 = 3.0;

impl StageView {
    /// Takes the outline while the pointer is held, and fills it on release.
    pub(super) fn handle_lasso_fill(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) {
        let (pressed, down, space_held, pointer) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.key_down(egui::Key::Space),
                i.pointer.latest_pos(),
            )
        });
        if pressed && response.is_pointer_button_down_on() && !space_held {
            self.lasso_fill = pointer.map(|p| vec![p]);
            return;
        }
        let Some(outline) = &mut self.lasso_fill else { return };
        if down {
            if let Some(p) = pointer.filter(|p| outline.last().is_none_or(|last| last.distance(*p) >= STEP)) {
                outline.push(p);
            }
            return;
        }
        let outline = self.lasso_fill.take().unwrap_or_default();
        if outline.len() >= 3 {
            fill(editor, view, &outline);
        }
    }

    /// The outline being drawn, closed back to where it began.
    pub(super) fn draw_lasso_fill(&self, painter: &egui::Painter, editor: &Editor) {
        let Some(outline) = self.lasso_fill.as_ref().filter(|o| o.len() > 1) else { return };
        let colour = crate::convert::to_color32(editor.paint_color);
        // In the paint colour over a dark edge, so it shows on any colour.
        let mut closed: Vec<Pos2> = outline.clone();
        closed.push(outline[0]);
        painter.add(egui::Shape::line(closed.clone(), (3.0, Color32::from_black_alpha(120))));
        painter.add(egui::Shape::line(closed, (1.5, colour.gamma_multiply(0.9).to_opaque())));
    }
}

/// Fills inside `outline` (screen points) on the selected layer: pixels on
/// a paint layer (one is made if nothing paintable is selected), or a
/// shape in the colour art of a drawn layer.
fn fill(editor: &mut Editor, view: View, outline: &[Pos2]) {
    let color = editor.paint_color;
    if editor.paint_target().is_none() {
        if let Some(layer) = editor.vector_erase_target() {
            let Some(to_local) = layer_to_local(editor, layer) else { return };
            let points: Vec<Point> = outline.iter().map(|p| to_local.apply(view.to_local(*p))).collect();
            // Smoothed into curves, as the Pencil's lines are, then closed.
            let Some(mut path) = fit_freehand_path(&points, 1.5 / view.zoom.max(1e-3)) else { return };
            path.closed = true;
            let shape = Element {
                shape: Shape::Path(path),
                style: Style { fill: Some(color), stroke: None, gradient: None },
            };
            let frame = editor.frame;
            if let Some(layer) = editor.doc.layer_mut(layer) {
                let index = layer.drawing_index_at(frame);
                if let Some(drawing) = layer.drawings.get_mut(index) {
                    drawing.colour_art.push(shape);
                }
            }
            return;
        }
        editor.add_paint_layer();
    }
    let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
    let points: Vec<Point> =
        outline.iter().filter_map(|p| canvas_point(editor, view, layer, (width, height), *p)).collect();
    let selection = editor.selection_on(canvas).cloned();
    if let Some(canvas) = editor.doc.canvas_mut(canvas) {
        lasso_fill(canvas, &points, color, selection.as_ref());
    }
}
