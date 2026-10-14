//! Masks: a layer whose shapes are a window onto other layers.
//!
//! As in Animate, a mask layer isn't drawn itself. Instead, each layer
//! masked by it only shows where the mask has something drawn. Whatever the
//! mask draws counts, at whatever opacity it's drawn: fills, strokes, text,
//! images with transparency, even a symbol. A half-transparent mask lets
//! half the masked layer through, which makes soft-edged reveals easy.
//!
//! Masks are ordinary layers otherwise, so they can be animated: a circle
//! moving across a dark scene is a spotlight.

use crate::document::{Document, Layer, LayerId};

impl Document {
    /// The mask layer that masks `layer` at `frame`, if any: the layer it
    /// names, if that's a mask, in the same timeline, and showing then.
    pub fn mask_for(&self, layer: &Layer, frame: u32) -> Option<&Layer> {
        let mask = self.layer(layer.masked_by?)?;
        let same_timeline = self.timeline_of(mask.id) == self.timeline_of(layer.id);
        (mask.mask && same_timeline && mask.is_shown_at(frame)).then_some(mask)
    }

    /// The mask layers a layer in the same timeline could be masked by.
    pub fn masks_beside(&self, layer: LayerId) -> Vec<&Layer> {
        let Some(timeline) = self.timeline_of(layer) else { return Vec::new() };
        self.layers_of(timeline).iter().filter(|l| l.mask && l.id != layer).collect()
    }

    /// Makes `mask` a mask over the layer directly below it in its
    /// timeline, as Animate's "Mask" command does. Returns the masked
    /// layer, or `None` if there's nothing below.
    pub fn mask_layer_below(&mut self, mask: LayerId) -> Option<LayerId> {
        let timeline = self.timeline_of(mask)?;
        let layers = self.layers_of(timeline);
        // Layers are stored bottom-first, so "below" is the one before.
        let index = layers.iter().position(|l| l.id == mask)?;
        let below = layers.get(index.checked_sub(1)?)?.id;
        self.layer_mut(mask)?.mask = true;
        let layer = self.layer_mut(below)?;
        layer.masked_by = Some(mask);
        Some(below)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Document, Point, Transform};

    #[test]
    fn masking_the_layer_below_links_them() {
        let mut doc = Document::default();
        let scene = doc.add_layer("Scene", vec![], 0, Transform::at(Point::ZERO));
        let spotlight = doc.add_layer("Spotlight", vec![], 0, Transform::at(Point::ZERO));

        assert_eq!(doc.mask_layer_below(spotlight), Some(scene));
        let scene_layer = doc.layer(scene).unwrap();
        assert_eq!(doc.mask_for(scene_layer, 0).map(|m| m.id), Some(spotlight));
        assert_eq!(doc.masks_beside(scene).len(), 1);

        // A layer that's no longer a mask no longer masks anything.
        doc.layer_mut(spotlight).unwrap().mask = false;
        assert!(doc.mask_for(doc.layer(scene).unwrap(), 0).is_none());
        // And the bottom layer has nothing below it to mask.
        assert_eq!(doc.mask_layer_below(scene), None);
    }
}
