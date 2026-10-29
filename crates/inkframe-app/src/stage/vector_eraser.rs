//! The Eraser on a drawn layer, as Animate's: rubbing across pencil lines,
//! pen paths and fills cuts them away (see `erase.rs` in the core). On a
//! paint layer the Eraser rubs out pixels instead (see `painting.rs`).

use egui::Pos2;
use inkframe_core::{LayerId, Point, Rub};

use super::{StageView, View, layer_to_local};
use crate::editor::Editor;

impl StageView {
    /// Erases shapes on the selected drawn layer as the pointer rubs over
    /// them, a little each frame. Returns whether the Eraser had a drawn
    /// layer to work on (so the pixel eraser stays out of it).
    pub(super) fn erase_shapes(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        let Some(layer) = editor.vector_erase_target() else {
            self.erasing = None;
            return false;
        };
        let (pressed, down, space_held, pointer) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.key_down(egui::Key::Space),
                i.pointer.latest_pos(),
            )
        });
        if pressed && response.is_pointer_button_down_on() && !space_held {
            self.erasing = pointer.and_then(|p| local_point(editor, layer, view, p));
        }
        if !down {
            self.erasing = None;
            return true;
        }
        let (Some(from), Some(to)) =
            (self.erasing, pointer.and_then(|p| local_point(editor, layer, view, p)))
        else {
            return true;
        };
        // The eraser's size is on the stage; the drawing may be scaled.
        let scale =
            editor.doc.layer(layer).map_or(1.0, |l| editor.doc.layer_matrix(l, editor.frame).average_scale());
        let radius = editor.eraser.size / 2.0 / scale.max(1e-3);
        editor.rub_shapes(layer, Rub { from, to, radius });
        self.erasing = Some(to);
        true
    }
}

/// A screen point in `layer`'s own coordinates.
fn local_point(editor: &Editor, layer: LayerId, view: View, screen: Pos2) -> Option<Point> {
    Some(layer_to_local(editor, layer)?.apply(view.to_local(screen)))
}
