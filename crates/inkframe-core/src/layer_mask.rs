//! Layer masks, as in Photoshop: a mask painted over a layer, black hiding
//! it and white showing it, so parts of the layer can be hidden and brought
//! back later without losing any of it.
//!
//! The mask is a canvas (see `paint.rs`) the size of the stage, placed in
//! the layer's own coordinates as a paint layer's canvas is, centred on its
//! origin. So it moves with the layer, and lines up with a paint layer's
//! pixels one for one. Unpainted, it's white and everything shows. What's
//! painted on it is read over white, by brightness, so any brush works on
//! it: black hides, grey half-hides, and erasing brings the layer back.
//!
//! Animate-style mask layers (`mask.rs`) are a different thing: a layer of
//! their own, whose drawing is a window onto the layers under it.

use serde::{Deserialize, Serialize};

use crate::document::{Document, LayerId};
use crate::paint::CanvasId;

/// A layer's mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerMask {
    pub canvas: CanvasId,
    /// Turned off, the mask is kept but the whole layer shows, as
    /// Shift-clicking a mask does in Photoshop.
    #[serde(default = "on", skip_serializing_if = "is_on")]
    pub enabled: bool,
}

fn on() -> bool {
    true
}

fn is_on(enabled: &bool) -> bool {
    *enabled
}

/// How much of the layer shows through one pixel of its mask (premultiplied
/// RGBA), from 0 to 255: the mask's brightness, painted over white.
pub fn mask_coverage([r, g, b, a]: [u8; 4]) -> u8 {
    // Over white, each channel is what's painted plus white where it isn't.
    let over = |c: u8| c as u32 + (255 - a as u32);
    // Brightness with Rec. 709's weights, in 256ths so they sum exactly.
    let light = (over(r) * 54 + over(g) * 183 + over(b) * 19 + 128) >> 8;
    light.min(255) as u8
}

impl Document {
    /// Gives a layer a mask that shows all of it, and returns the mask's
    /// canvas. `None` if it has one already, or is a kind of layer that
    /// draws nothing of its own: a folder, the camera, a guide or a mask.
    pub fn add_layer_mask(&mut self, id: LayerId) -> Option<CanvasId> {
        let layer = self.layer(id)?;
        if layer.layer_mask.is_some() || layer.folder || layer.camera || layer.guide || layer.mask {
            return None;
        }
        let canvas = self.add_canvas(self.width, self.height);
        self.layer_mut(id)?.layer_mask = Some(LayerMask { canvas, enabled: true });
        Some(canvas)
    }

    /// Takes a layer's mask away, showing all of it again. Returns whether
    /// it had one.
    pub fn remove_layer_mask(&mut self, id: LayerId) -> bool {
        let Some(mask) = self.layer_mut(id).and_then(|l| l.layer_mask.take()) else { return false };
        self.canvases.remove(&mask.canvas);
        true
    }

    /// Photoshop's Apply Layer Mask, for a paint layer: what the mask hides
    /// is erased from the painting the layer shows on `frame`, and the mask
    /// is removed. Returns false (changing nothing) for other layers.
    pub fn apply_layer_mask(&mut self, id: LayerId, frame: u32) -> bool {
        let Some(mask) = self.layer(id).and_then(|l| l.layer_mask) else { return false };
        let Some((painting, width, height)) = self.paint_canvas_at(id, frame) else { return false };
        let Some(mask_canvas) = self.canvas(mask.canvas) else { return false };
        let Some(canvas) = self.canvas(painting) else { return false };
        // Both are centred on the layer's origin, so a canvas made before
        // the stage was resized lines up by its middle.
        let (mask_width, mask_height) = (mask_canvas.width, mask_canvas.height);
        let dx = (width as i64 - mask_width as i64) / 2;
        let dy = (height as i64 - mask_height as i64) / 2;
        let mut pixels = canvas.to_pixels();
        for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let (x, y) = ((i as u32 % width) as i64, (i as u32 / width) as i64);
            let coverage = mask_coverage(mask_canvas.pixel(x - dx, y - dy)) as u32;
            for channel in pixel {
                *channel = ((*channel as u32 * coverage + 127) / 255) as u8;
            }
        }
        if let Some(canvas) = self.canvas_mut(painting) {
            canvas.write_region(0, 0, width, height, &pixels);
        }
        self.remove_layer_mask(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Point, TimelineId, Transform};

    #[test]
    fn unpainted_shows_all_black_hides_and_grey_half_hides() {
        assert_eq!(mask_coverage([0, 0, 0, 0]), 255, "unpainted is white");
        assert_eq!(mask_coverage([0, 0, 0, 255]), 0);
        assert_eq!(mask_coverage([255, 255, 255, 255]), 255);
        let grey = mask_coverage([128, 128, 128, 255]);
        assert!((126..=130).contains(&grey), "{grey}");
        // Black at half strength, over white: half shows.
        let soft = mask_coverage([0, 0, 0, 128]);
        assert!((126..=128).contains(&soft), "{soft}");
    }

    #[test]
    fn a_mask_is_added_once_removed_and_applied_to_a_painting() {
        let mut doc = Document::default();
        let layer = doc.add_paint_layer(TimelineId::Main, "Paint", 0).unwrap();
        let (painting, width, height) = doc.paint_canvas_at(layer, 0).unwrap();
        doc.canvas_mut(painting).unwrap().write_region(0, 0, 2, 1, &[200, 0, 0, 255, 200, 0, 0, 255]);

        let mask = doc.add_layer_mask(layer).expect("a paint layer takes a mask");
        assert_eq!(doc.add_layer_mask(layer), None, "only one");
        // Black over the first pixel hides it; the second stays.
        doc.canvas_mut(mask).unwrap().write_region(0, 0, 1, 1, &[0, 0, 0, 255]);
        assert!(doc.apply_layer_mask(layer, 0));
        let canvas = doc.canvas(painting).unwrap();
        assert_eq!(canvas.pixel(0, 0), [0, 0, 0, 0]);
        assert_eq!(canvas.pixel(1, 0), [200, 0, 0, 255]);
        assert!(doc.layer(layer).unwrap().layer_mask.is_none());
        assert!(doc.canvas(mask).is_none(), "the mask's canvas goes with it");
        assert_eq!((width, height), (doc.width, doc.height));

        // A camera draws nothing to mask.
        let camera = doc.add_camera();
        assert_eq!(doc.add_layer_mask(camera), None);
        let shapes = doc.add_layer("Shapes", Vec::new(), 0, Transform::at(Point::ZERO));
        assert!(doc.add_layer_mask(shapes).is_some());
        assert!(!doc.apply_layer_mask(shapes, 0), "only paint can be applied to");
        assert!(doc.remove_layer_mask(shapes));
    }
}
