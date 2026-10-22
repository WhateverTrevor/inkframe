//! Moving a gradient fill's ends on the stage, as Animate's Gradient
//! Transform tool does: with the Selection tool, a shape with a gradient
//! shows a handle at each end, joined by a line. The round one is where the
//! gradient starts (a radial one's middle), the square one where it ends.
//!
//! The handles show for the one shape picked inside a layer (see
//! `inside.rs`). Not for a whole selected layer: a linear gradient's ends
//! start at the middle of the shape's sides, right where the layer's own
//! scale handles are.

use egui::{Color32, CursorIcon, Pos2, Rect, StrokeKind, Vec2};
use inkframe_core::{Affine, Element, LayerId, Point};

use super::{HANDLE_SIZE, SELECTION, StageView, View};
use crate::editor::{Editor, Tool};
use crate::inside::ShapeRef;

/// Which end of the gradient is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum GradientEnd {
    From,
    To,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GradientDrag {
    layer: LayerId,
    shape: ShapeRef,
    end: GradientEnd,
}

/// The shape whose gradient gets handles, if any.
fn target(editor: &Editor) -> Option<(LayerId, ShapeRef)> {
    // Not over a Distort or Envelope's handles.
    if editor.tool != Tool::Select || editor.lattice.is_some() {
        return None;
    }
    let layer = editor.inside_layer()?;
    let [shape] = editor.picked_shapes() else { return None };
    element(editor, layer, *shape)?.style.gradient.as_ref().map(|_| (layer, *shape))
}

fn element(editor: &Editor, layer: LayerId, shape: ShapeRef) -> Option<&Element> {
    let layer = editor.doc.layer(layer)?;
    let drawing = layer.drawings.get(layer.drawing_index_at(editor.frame))?;
    let part = if shape.colour_art { &drawing.colour_art } else { &drawing.elements };
    part.get(shape.index)
}

fn element_mut(editor: &mut Editor, layer: LayerId, shape: ShapeRef) -> Option<&mut Element> {
    let frame = editor.frame;
    let layer = editor.doc.layer_mut(layer)?;
    let index = layer.drawing_index_at(frame);
    let drawing = layer.drawings.get_mut(index)?;
    let part = if shape.colour_art { &mut drawing.colour_art } else { &mut drawing.elements };
    part.get_mut(shape.index)
}

/// From the layer's coordinates to the timeline's.
fn layer_matrix(editor: &Editor, layer: LayerId) -> Option<Affine> {
    Some(editor.doc.layer_matrix(editor.doc.layer(layer)?, editor.frame))
}

/// The two handles on screen: where the gradient starts and ends.
pub(super) fn handles(editor: &Editor, view: View) -> Option<(Pos2, Pos2)> {
    let (layer, shape) = target(editor)?;
    let element = element(editor, layer, shape)?;
    let (from, to) = element.style.gradient.as_ref()?.ends_in(element.shape.bounds());
    let m = layer_matrix(editor, layer)?;
    Some((view.to_screen(m.apply(from)), view.to_screen(m.apply(to))))
}

impl StageView {
    /// Starts, continues or ends dragging a gradient's handle. Returns
    /// whether the pointer is busy with one, so the tools leave it alone.
    pub(super) fn handle_gradient(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(press) = ui.input(|i| i.pointer.press_origin())
            && let Some((from, to)) = handles(editor, view)
            && let Some((layer, shape)) = target(editor)
        {
            let end = if press.distance(to) <= HANDLE_SIZE {
                Some(GradientEnd::To)
            } else if press.distance(from) <= HANDLE_SIZE {
                Some(GradientEnd::From)
            } else {
                None
            };
            self.gradient_drag = end.map(|end| GradientDrag { layer, shape, end });
        }
        let Some(drag) = self.gradient_drag else { return false };
        if let Some(pointer) = response.interact_pointer_pos()
            && let Some(to_local) = layer_matrix(editor, drag.layer).and_then(|m| m.inverse())
        {
            let local = to_local.apply(view.to_local(pointer));
            if let Some(element) = element_mut(editor, drag.layer, drag.shape) {
                // Kept as a fraction of the shape's box, so it goes with the
                // shape (see `ShapeGradient::from`).
                let bounds = element.shape.bounds();
                let size = Point::new(bounds.width().max(1e-3), bounds.height().max(1e-3));
                let fraction =
                    Point::new((local.x - bounds.min.x) / size.x, (local.y - bounds.min.y) / size.y);
                if let Some(gradient) = &mut element.style.gradient {
                    match drag.end {
                        GradientEnd::From => gradient.from = fraction,
                        GradientEnd::To => gradient.to = fraction,
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.gradient_drag = None;
        }
        true
    }

    /// The handles, and a cursor that says they can be moved.
    pub(super) fn draw_gradient_handles(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        editor: &Editor,
        view: View,
    ) {
        let Some((from, to)) = handles(editor, view) else { return };
        painter.line_segment([from, to], (1.5, Color32::from_black_alpha(140)));
        painter.line_segment([from, to], (1.0, Color32::WHITE));
        painter.circle(from, HANDLE_SIZE / 2.0 + 1.0, Color32::WHITE, (1.5, SELECTION));
        let square = Rect::from_center_size(to, Vec2::splat(HANDLE_SIZE + 1.0));
        painter.rect_filled(square, 1.0, Color32::WHITE);
        painter.rect_stroke(square, 1.0, (1.5, SELECTION), StrokeKind::Inside);
        let over = response
            .hover_pos()
            .is_some_and(|p| p.distance(from) <= HANDLE_SIZE || p.distance(to) <= HANDLE_SIZE);
        if self.gradient_drag.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if over {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
}
