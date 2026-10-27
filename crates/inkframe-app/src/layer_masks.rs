//! Photoshop's clipping masks and layer masks, from the editor (the model
//! is in the core's `clipping.rs` and `layer_mask.rs`, the drawing in the
//! renderer).
//!
//! Modify › Create Clipping Mask (⌥⌘G) clips the selected layers to the
//! layer beneath, so shading painted over a figure stays on the figure.
//! Modify › Layer Mask › Add gives the layer a mask; while it's being
//! painted (Paint the Mask, or clicking the mask in the timeline), the
//! brush, eraser and other painting tools paint the mask instead of the
//! layer: dark hides, light shows, erasing brings the layer back.

use inkframe_core::{Document, LayerId};

use crate::editor::Editor;

impl Editor {
    /// Whether the selected layer could be clipped to a layer beneath it,
    /// or is clipped and could be released.
    pub fn can_toggle_clipping(&self) -> bool {
        let Some(layer) = self.selected_layer() else { return false };
        if layer.clipped {
            return true;
        }
        let Some(timeline) = self.doc.timeline_of(layer.id) else { return false };
        let layers = self.doc.layers_of(timeline);
        let beneath = layers.iter().position(|l| l.id == layer.id).is_some_and(|i| i > 0);
        Document::can_clip(layer) && beneath
    }

    /// Clips the selected layers to the layer beneath, or releases them if
    /// the one the Properties panel shows is clipped already.
    pub fn toggle_clipping(&mut self) {
        let Some(clip) = self.selected_layer().map(|l| !l.clipped) else { return };
        for id in self.selected_ids() {
            self.doc.set_clipped(id, clip);
        }
    }

    /// Gives the selected layer a mask, and starts painting it.
    pub fn add_layer_mask(&mut self) -> bool {
        let Some(id) = self.selection else { return false };
        if self.doc.add_layer_mask(id).is_none() {
            return false;
        }
        self.editing_mask = true;
        true
    }

    /// Whether the selected layer has a mask.
    pub fn selected_mask(&self) -> Option<inkframe_core::LayerMask> {
        self.selected_layer()?.layer_mask
    }

    pub fn delete_layer_mask(&mut self) {
        if let Some(id) = self.selection {
            self.forget_mask_selection(id);
            self.doc.remove_layer_mask(id);
            self.editing_mask = false;
        }
    }

    /// Apply Layer Mask: on a paint layer, what the mask hides is erased.
    pub fn apply_layer_mask(&mut self) -> bool {
        let Some(id) = self.selection else { return false };
        self.forget_mask_selection(id);
        let applied = self.doc.apply_layer_mask(id, self.frame);
        if applied {
            self.editing_mask = false;
        }
        applied
    }

    /// Turns the selected layer's mask off (the whole layer showing) or
    /// back on, keeping what's painted on it.
    pub fn toggle_layer_mask(&mut self) {
        let Some(id) = self.selection else { return };
        if let Some(mask) = self.doc.layer_mut(id).and_then(|l| l.layer_mask.as_mut()) {
            mask.enabled = !mask.enabled;
        }
    }

    /// Paints the layer's mask (true) or the layer itself.
    pub fn set_editing_mask(&mut self, layer: LayerId, mask: bool) {
        self.select(Some(layer));
        self.editing_mask = mask && self.doc.layer(layer).is_some_and(|l| l.layer_mask.is_some());
    }

    /// Whether painting goes onto the selected layer's mask now.
    pub fn painting_mask(&self) -> bool {
        self.editing_mask && self.selected_mask().is_some()
    }

    /// A selection made on a mask that's going away selects nothing.
    fn forget_mask_selection(&mut self, id: LayerId) {
        let canvas = self.doc.layer(id).and_then(|l| l.layer_mask).map(|m| m.canvas);
        if canvas.is_some() && self.pixel_selection.as_ref().map(|(on, _)| *on) == canvas {
            self.pixel_selection = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Point, TimelineId, Transform};

    #[test]
    fn painting_goes_to_the_mask_while_its_being_edited() {
        let mut editor = Editor::new(Document::default());
        let layer = editor.doc.add_paint_layer(TimelineId::Main, "Paint", 0).unwrap();
        editor.select(Some(layer));
        let (_, painting, ..) = editor.paint_target().unwrap();
        assert!(editor.add_layer_mask());
        let mask = editor.selected_mask().unwrap().canvas;
        assert_eq!(editor.paint_target().map(|(_, c, ..)| c), Some(mask), "a new mask is painted at once");
        editor.set_editing_mask(layer, false);
        assert_eq!(editor.paint_target().map(|(_, c, ..)| c), Some(painting));
        // Choosing another layer goes back to painting layers.
        editor.set_editing_mask(layer, true);
        let other = editor.doc.add_layer("Shapes", Vec::new(), 0, Transform::at(Point::ZERO));
        editor.select(Some(other));
        assert!(!editor.painting_mask());
        // Any layer can have a mask to paint, not only paint layers.
        assert!(editor.add_layer_mask());
        assert!(editor.paint_target().is_some_and(|(id, ..)| id == other));
        editor.delete_layer_mask();
        assert!(editor.paint_target().is_none(), "a shape layer has nothing to paint again");
    }

    #[test]
    fn clipping_toggles_for_the_selected_layers() {
        let mut editor = Editor::new(Document::default());
        let at = Transform::at(Point::ZERO);
        let base = editor.doc.add_layer("Figure", Vec::new(), 0, at);
        let shade = editor.doc.add_layer("Shade", Vec::new(), 0, at);
        editor.select(Some(base));
        assert!(!editor.can_toggle_clipping(), "nothing beneath the bottom layer");
        editor.select(Some(shade));
        assert!(editor.can_toggle_clipping());
        editor.toggle_clipping();
        assert_eq!(editor.doc.clip_base(shade), Some(base));
        editor.toggle_clipping();
        assert!(!editor.doc.layer(shade).unwrap().clipped);
    }
}
