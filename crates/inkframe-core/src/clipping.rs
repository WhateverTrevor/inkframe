//! Clipping masks, as in Photoshop: a layer clipped to the one beneath it
//! shows only where that layer has paint. Shading painted on a layer
//! clipped to a figure stays inside the figure however freely it's painted.
//!
//! Several layers in a row can be clipped; they all clip to the nearest
//! layer under them that isn't clipped, their *base*. The base's opacity
//! and blend mode apply to the group as a whole, and hiding the base hides
//! the group. A clipped layer with nothing under it to clip to is drawn as
//! usual.

use crate::document::{Document, Layer, LayerId};

impl Document {
    /// Whether a layer can be clipped to the one beneath, or be a base:
    /// only layers that draw something of their own.
    pub fn can_clip(layer: &Layer) -> bool {
        !(layer.folder || layer.camera || layer.guide || layer.mask)
    }

    /// The layer a clipped layer is clipped to: the nearest under it in its
    /// stack that isn't clipped itself, if that's one that can be a base.
    pub fn clip_base(&self, id: LayerId) -> Option<LayerId> {
        let layers = self.layers_of(self.timeline_of(id)?);
        let index = layers.iter().position(|l| l.id == id)?;
        if !layers[index].clipped {
            return None;
        }
        // Layers are stored bottom-first, so "under" is earlier.
        let base = layers[..index].iter().rev().find(|l| !l.clipped)?;
        Self::can_clip(base).then_some(base.id)
    }

    /// Clips a layer to the one beneath it, or releases it. Returns whether
    /// anything changed.
    pub fn set_clipped(&mut self, id: LayerId, clipped: bool) -> bool {
        let Some(layer) = self.layer_mut(id) else { return false };
        if layer.clipped == clipped || (clipped && !Self::can_clip(layer)) {
            return false;
        }
        layer.clipped = clipped;
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::{Document, Point, TimelineId, Transform};

    #[test]
    fn clipped_layers_find_the_nearest_unclipped_layer_beneath() {
        let mut doc = Document::default();
        let at = Transform::at(Point::ZERO);
        let base = doc.add_layer("Figure", Vec::new(), 0, at);
        let shade = doc.add_layer("Shade", Vec::new(), 0, at);
        let light = doc.add_layer("Light", Vec::new(), 0, at);
        assert!(doc.set_clipped(shade, true) && doc.set_clipped(light, true));
        assert!(!doc.set_clipped(light, true), "already clipped");
        assert_eq!(doc.clip_base(shade), Some(base));
        assert_eq!(doc.clip_base(light), Some(base), "clipped to the same base, through Shade");
        assert_eq!(doc.clip_base(base), None, "the base isn't clipped");

        // Nothing beneath to clip to.
        let lone = doc.add_paint_layer(TimelineId::Main, "Lone", 0).unwrap();
        doc.move_layer_to(lone, 0);
        doc.set_clipped(lone, true);
        assert_eq!(doc.clip_base(lone), None);

        let camera = doc.add_camera();
        assert!(!doc.set_clipped(camera, true), "the camera can't be clipped");
    }
}
