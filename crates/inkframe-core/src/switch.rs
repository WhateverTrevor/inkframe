//! Switch folders, as Moho's switch layers: a folder that shows one of the
//! layers in it at a time. A hand drawn in a few poses (open, fist,
//! pointing) goes in a switch, each pose its own layer (or folder, rigged
//! as fully as anything else), and the switch picks which shows.
//!
//! Which one is keyed frame by frame, as drawing swaps are, and holds until
//! the next key. A smart bone can pick instead (see `skeleton.rs`): turn
//! the wrist past an angle and the fist shows. Before any key, the topmost
//! layer in the folder shows.

use serde::{Deserialize, Serialize};

use crate::document::{Document, Layer, LayerId};

/// From `frame` on, the switch shows `layer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchKey {
    pub frame: u32,
    pub layer: LayerId,
}

/// The deepest folders are looked through for the switches a layer is in.
const MAX_DEPTH: usize = 32;

impl Document {
    /// The layers directly in folder `folder`, top first.
    pub fn folder_children(&self, folder: &Layer) -> Vec<&Layer> {
        let Some(timeline) = self.timeline_of(folder.id) else { return Vec::new() };
        self.layers_of(timeline).iter().rev().filter(|l| l.in_folder == Some(folder.id)).collect()
    }

    /// Which layer switch `folder` shows on `frame`: the one a smart bone
    /// picks, or else the one keyed last, or else the topmost.
    pub fn switch_choice(&self, folder: &Layer, frame: u32) -> Option<LayerId> {
        let children = self.folder_children(folder);
        let is_child = |id: LayerId| children.iter().any(|c| c.id == id);
        if let Some(chosen) = self.smart_switch(folder, frame).filter(|&id| is_child(id)) {
            return Some(chosen);
        }
        let keyed =
            folder.switch_keys.iter().take_while(|k| k.frame <= frame).filter(|k| is_child(k.layer)).last();
        keyed.map(|k| k.layer).or_else(|| children.first().map(|c| c.id))
    }

    /// Whether `layer` is hidden on `frame` by a switch it's in showing
    /// another of its layers.
    pub fn switched_off(&self, layer: &Layer, frame: u32) -> bool {
        let mut current = layer;
        for _ in 0..MAX_DEPTH {
            let Some(folder) = current.in_folder.and_then(|id| self.layer(id)) else { return false };
            if folder.switch && self.switch_choice(folder, frame) != Some(current.id) {
                return true;
            }
            current = folder;
        }
        false
    }

    /// Whether `layer` shows on `frame`: it's visible, on, and not
    /// switched off.
    pub fn shows(&self, layer: &Layer, frame: u32) -> bool {
        layer.is_shown_at(frame) && !self.switched_off(layer, frame)
    }

    /// Keys switch `folder` to show `child` from `frame` on.
    pub fn set_switch(&mut self, folder: LayerId, frame: u32, child: LayerId) {
        let Some(folder) = self.layer_mut(folder) else { return };
        folder.switch_keys.retain(|k| k.frame != frame);
        let at = folder.switch_keys.partition_point(|k| k.frame < frame);
        folder.switch_keys.insert(at, SwitchKey { frame, layer: child });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TimelineId, Transform};

    /// A switch holding three hands, "Open" on top.
    fn hands() -> (Document, LayerId, [LayerId; 3]) {
        let mut doc = Document::default();
        let fist = doc.add_layer("Fist", Vec::new(), 0, Transform::default());
        let point = doc.add_layer("Point", Vec::new(), 0, Transform::default());
        let open = doc.add_layer("Open", Vec::new(), 0, Transform::default());
        let folder = doc.add_folder(TimelineId::Main, &[fist, point, open]).unwrap();
        doc.layer_mut(folder).unwrap().switch = true;
        (doc, folder, [open, point, fist])
    }

    #[test]
    fn a_switch_shows_its_top_layer_until_keyed() {
        let (mut doc, folder, [open, point, fist]) = hands();
        let shows = |doc: &Document, id: LayerId, frame: u32| doc.shows(doc.layer(id).unwrap(), frame);
        assert!(shows(&doc, open, 0) && !shows(&doc, point, 0) && !shows(&doc, fist, 0));
        doc.set_switch(folder, 5, fist);
        assert!(shows(&doc, open, 4));
        assert!(shows(&doc, fist, 5) && !shows(&doc, open, 5));
        assert!(shows(&doc, fist, 40), "holds until the next key");
    }

    #[test]
    fn an_ordinary_folder_shows_everything() {
        let (mut doc, folder, ids) = hands();
        doc.layer_mut(folder).unwrap().switch = false;
        assert!(ids.iter().all(|&id| doc.shows(doc.layer(id).unwrap(), 0)));
    }
}
