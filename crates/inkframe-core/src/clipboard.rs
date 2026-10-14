//! Copying and pasting layers and keyframes.
//!
//! What's copied is self-contained, so it can be pasted into another
//! project as well as the one it came from: a layer comes with every symbol
//! it uses (and the symbols those use), the images and fonts it needs, and
//! its keyframes. Linked files are recorded by their full path, then linked
//! afresh, relative to the new project where possible, when pasted.
//!
//! Pasting into the same project reuses its symbols rather than making
//! copies of them, so a pasted instance stays an instance of the same symbol.
//!
//! On the system clipboard it's JSON with a first line that marks it as
//! Inkframe's, so it can travel between windows (or two copies of the app)
//! without being mistaken for ordinary text.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::channel::Key;
use crate::document::{Document, Layer, LayerId, TimelineId};
use crate::font_file::FontFile;
use crate::geometry::Point;
use crate::image::{ImageAsset, ImageId};
use crate::shape::Shape;
use crate::symbol::{Symbol, SymbolId};
use crate::track::Track;

/// The first line of Inkframe data on the system clipboard.
const MARKER: &str = "inkframe-clipboard-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Clipboard {
    Layers(CopiedLayers),
    /// One layer's pose at one frame, for pasting onto another frame or layer.
    Keyframe(CopiedKeyframe),
}

impl Clipboard {
    /// The clipboard text: the marker line, then the data as JSON.
    pub fn to_text(&self) -> String {
        let json = serde_json::to_string(self).expect("clipboard data always serializes");
        format!("{MARKER}\n{json}")
    }

    /// Inkframe data from clipboard text, or `None` if the text is anything else.
    pub fn from_text(text: &str) -> Option<Self> {
        let json = text.strip_prefix(MARKER)?.trim_start();
        serde_json::from_str(json).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopiedLayers {
    /// Bottom-most first, as in a timeline.
    pub layers: Vec<Layer>,
    /// Every symbol the layers use, directly or inside other symbols.
    pub symbols: Vec<Symbol>,
    /// The images they show, with full paths.
    pub images: Vec<ImageAsset>,
    /// The project font files their text uses, with full paths.
    pub fonts: Vec<FontFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopiedKeyframe {
    pub position: Key<Point>,
    pub rotation: Key<f32>,
    pub scale: Key<Point>,
    pub opacity: Key<f32>,
}

impl CopiedKeyframe {
    /// Copies a layer's keys at `frame` (each property's value there, with
    /// its easing and curve if it has a key on that frame).
    pub fn from_track(track: &Track, frame: u32) -> Option<Self> {
        Some(Self {
            position: track.position.key_or_sample(frame)?,
            rotation: track.rotation.key_or_sample(frame)?,
            scale: track.scale.key_or_sample(frame)?,
            opacity: track.opacity.key_or_sample(frame)?,
        })
    }

    /// Keys every property at `frame` with the copied values.
    pub fn paste_into(&self, track: &mut Track, frame: u32) {
        track.position.put(Key { frame, ..self.position });
        track.rotation.put(Key { frame, ..self.rotation });
        track.scale.put(Key { frame, ..self.scale });
        track.opacity.put(Key { frame, ..self.opacity });
    }
}

impl Document {
    /// Copies layers (all from one timeline), with everything they need.
    /// `project_dir` is where this project's linked files are relative to.
    pub fn copy_layers(&self, ids: &[LayerId], project_dir: Option<&Path>) -> Option<CopiedLayers> {
        let timeline = self.timeline_of(*ids.first()?)?;
        let layers: Vec<Layer> =
            self.layers_of(timeline).iter().filter(|l| ids.contains(&l.id)).cloned().collect();

        // Symbols used directly, then the ones those use, and so on.
        let mut symbols = BTreeSet::new();
        let mut to_visit: Vec<SymbolId> =
            layers.iter().filter_map(|l| l.instance.map(|i| i.symbol)).collect();
        while let Some(id) = to_visit.pop() {
            if let Some(symbol) = self.symbol(id).filter(|_| symbols.insert(id)) {
                to_visit.extend(symbol.layers.iter().filter_map(|l| l.instance.map(|i| i.symbol)));
            }
        }
        let symbols: Vec<Symbol> = symbols.into_iter().filter_map(|id| self.symbol(id).cloned()).collect();

        let every_layer = || layers.iter().chain(symbols.iter().flat_map(|s| &s.layers));
        let shapes = || every_layer().flat_map(|l| &l.elements).map(|e| &e.shape);
        let image_ids: BTreeSet<ImageId> = shapes()
            .filter_map(|s| if let Shape::Image { image, .. } = s { Some(*image) } else { None })
            .collect();
        let families: BTreeSet<&str> = shapes()
            .filter_map(|s| if let Shape::Text(t) = s { Some(t.font.family.as_str()) } else { None })
            .collect();

        // Linked files go on the clipboard by their full path, since the
        // project they're pasted into may be somewhere else entirely.
        let images = self
            .images
            .iter()
            .filter(|i| image_ids.contains(&i.id))
            .map(|i| ImageAsset { path: i.resolve(project_dir), ..i.clone() })
            .collect();
        let fonts = self
            .fonts
            .iter()
            .filter(|f| f.families.iter().any(|family| families.contains(family.as_str())))
            .map(|f| FontFile { path: f.resolve(project_dir), ..f.clone() })
            .collect();
        Some(CopiedLayers { layers, symbols, images, fonts })
    }

    /// Adds copied layers to the top of `timeline`, moved by `offset`, and
    /// returns their new ids, bottom-most first. `project_dir` is where
    /// this project's linked files are relative to.
    pub fn paste_layers(
        &mut self,
        copied: &CopiedLayers,
        timeline: TimelineId,
        offset: Point,
        project_dir: Option<&Path>,
    ) -> Vec<LayerId> {
        // Linked files: link each again from this project. Linking a file
        // the project already has reuses it.
        let images: BTreeMap<ImageId, ImageId> = copied
            .images
            .iter()
            .map(|i| (i.id, self.add_image(&i.path, project_dir, (i.width, i.height))))
            .collect();
        for font in &copied.fonts {
            self.add_font_file(&font.path, project_dir, font.families.clone());
        }

        // Symbols: reuse ones this project already has, unchanged, under the
        // same id (pasting into the project they came from). Everything else
        // becomes a new symbol.
        let mut symbols = BTreeMap::new();
        for symbol in &copied.symbols {
            let new = if self.symbol(symbol.id) == Some(symbol) {
                symbol.id
            } else {
                let id = SymbolId(self.next_symbol_id);
                self.next_symbol_id += 1;
                id
            };
            symbols.insert(symbol.id, new);
        }
        for symbol in &copied.symbols {
            let id = symbols[&symbol.id];
            if self.symbol(id).is_some() {
                continue; // reused as it was
            }
            let taken = self.symbols.iter().any(|s| s.name == symbol.name);
            let name = if taken { format!("{} copy", symbol.name) } else { symbol.name.clone() };
            let layers = self.fresh_layers(&symbol.layers, &images, &symbols, None, Point::ZERO);
            self.symbols.push(Symbol { id, name, duration: symbol.duration, layers });
        }

        let layers = self.fresh_layers(&copied.layers, &images, &symbols, Some(timeline), offset);
        let ids: Vec<LayerId> = layers.iter().map(|l| l.id).collect();
        if let Some(stack) = self.layers_of_mut(timeline) {
            stack.extend(layers);
        }
        ids
    }

    /// Copies of `layers` with new ids, their images and symbols swapped for
    /// the pasted ones, positions moved by `offset`, and links between them
    /// (guides, masks) kept. A link to a layer that wasn't copied is kept if
    /// that layer is in `timeline` (pasting beside it), and dropped otherwise.
    fn fresh_layers(
        &mut self,
        layers: &[Layer],
        images: &BTreeMap<ImageId, ImageId>,
        symbols: &BTreeMap<SymbolId, SymbolId>,
        timeline: Option<TimelineId>,
        offset: Point,
    ) -> Vec<Layer> {
        let ids: BTreeMap<LayerId, LayerId> =
            layers.iter().map(|l| (l.id, self.allocate_layer_id())).collect();
        let existing = |id: LayerId| timeline.is_some_and(|t| self.layers_of(t).iter().any(|l| l.id == id));
        let relink = |link: Option<LayerId>| {
            let link = link?;
            ids.get(&link).copied().or_else(|| existing(link).then_some(link))
        };
        let names: BTreeSet<String> =
            timeline.map(|t| self.layers_of(t).iter().map(|l| l.name.clone()).collect()).unwrap_or_default();

        layers
            .iter()
            .map(|layer| {
                let mut copy = layer.clone();
                copy.id = ids[&layer.id];
                if names.contains(&copy.name) {
                    copy.name = format!("{} copy", copy.name);
                }
                copy.follow = relink(layer.follow);
                copy.masked_by = relink(layer.masked_by);
                if let Some(instance) = &mut copy.instance {
                    instance.symbol = symbols.get(&instance.symbol).copied().unwrap_or(instance.symbol);
                }
                for element in &mut copy.elements {
                    if let Shape::Image { image, .. } = &mut element.shape {
                        *image = images.get(image).copied().unwrap_or(*image);
                    }
                }
                if offset != Point::ZERO {
                    copy.track.position.map_values(|p| p + offset);
                }
                copy
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Style, Transform};

    fn square() -> Element {
        Element {
            shape: Shape::Rect { width: 10.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        }
    }

    #[test]
    fn pasted_layers_keep_their_animation_and_get_new_ids() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::at(Point::new(10.0, 10.0)));
        doc.layer_mut(id).unwrap().track.position.set(10, Point::new(110.0, 10.0));

        let copied = doc.copy_layers(&[id], None).unwrap();
        let pasted = doc.paste_layers(&copied, TimelineId::Main, Point::new(0.0, 50.0), None);

        assert_eq!(pasted.len(), 1);
        assert_ne!(pasted[0], id);
        let copy = doc.layer(pasted[0]).unwrap();
        assert_eq!(copy.name, "Box copy");
        assert_eq!(copy.track.position.sample(10.0), Some(Point::new(110.0, 60.0)), "moved, still animated");
        assert_eq!(doc.layers.last().unwrap().id, pasted[0], "pasted on top");
    }

    #[test]
    fn instances_pasted_into_the_same_project_reuse_their_symbol() {
        let mut doc = Document::default();
        let id = doc.add_layer("Ball", vec![square()], 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        let copied = doc.copy_layers(&[id], None).unwrap();
        let pasted = doc.paste_layers(&copied, TimelineId::Main, Point::ZERO, None);

        assert_eq!(doc.symbols.len(), 1);
        assert_eq!(doc.layer(pasted[0]).unwrap().instance.unwrap().symbol, symbol);
    }

    #[test]
    fn instances_pasted_into_another_project_bring_their_symbol() {
        let mut source = Document::default();
        let id = source.add_layer("Ball", vec![square()], 0, Transform::default());
        source.convert_layer_to_symbol(id, 0).unwrap();
        let copied = source.copy_layers(&[id], None).unwrap();

        let mut target = Document::default();
        target.add_layer("Background", vec![square()], 0, Transform::default());
        let pasted = target.paste_layers(&copied, TimelineId::Main, Point::ZERO, None);

        assert_eq!(target.symbols.len(), 1);
        let symbol = target.layer(pasted[0]).unwrap().instance.unwrap().symbol;
        assert_eq!(target.symbol(symbol).unwrap().layers[0].elements, vec![square()]);
        // Layer ids inside the new symbol are fresh, not the source's.
        let inner = target.symbol(symbol).unwrap().layers[0].id;
        assert!(target.layers.iter().all(|l| l.id != inner));
    }

    #[test]
    fn links_between_copied_layers_follow_the_copies() {
        let mut doc = Document::default();
        let scene = doc.add_layer("Scene", vec![square()], 0, Transform::default());
        let mask = doc.add_layer("Mask", vec![square()], 0, Transform::default());
        doc.mask_layer_below(mask);

        let both = doc.copy_layers(&[scene, mask], None).unwrap();
        let pasted = doc.paste_layers(&both, TimelineId::Main, Point::ZERO, None);
        assert_eq!(doc.layer(pasted[0]).unwrap().masked_by, Some(pasted[1]));

        // Copied alone, the scene stays masked by the original mask beside it.
        let alone = doc.copy_layers(&[scene], None).unwrap();
        let pasted = doc.paste_layers(&alone, TimelineId::Main, Point::ZERO, None);
        assert_eq!(doc.layer(pasted[0]).unwrap().masked_by, Some(mask));
        // In another project there's no such mask, so the link is dropped.
        let mut other = Document::default();
        let pasted = other.paste_layers(&alone, TimelineId::Main, Point::ZERO, None);
        assert_eq!(other.layer(pasted[0]).unwrap().masked_by, None);
    }

    #[test]
    fn linked_files_are_relinked_from_the_new_project() {
        let mut source = Document::default();
        let image =
            source.add_image(Path::new("/old/shot/art/sky.png"), Some(Path::new("/old/shot")), (8, 8));
        let shape = Shape::Image { image, width: 8.0, height: 8.0 };
        let id = source.add_layer(
            "Sky",
            vec![Element { shape, style: Style { fill: None, stroke: None } }],
            0,
            Transform::default(),
        );
        let copied = source.copy_layers(&[id], Some(Path::new("/old/shot"))).unwrap();
        assert_eq!(copied.images[0].path, Path::new("/old/shot/art/sky.png"), "a full path on the clipboard");

        let mut target = Document::default();
        target.paste_layers(&copied, TimelineId::Main, Point::ZERO, Some(Path::new("/new/shot")));
        assert_eq!(target.images[0].path, Path::new("/old/shot/art/sky.png"));
    }

    #[test]
    fn keyframes_paste_onto_other_frames() {
        let mut track = Track::new(0, Transform::at(Point::new(5.0, 5.0)));
        track.rotation.set(0, 45.0);
        let copied = CopiedKeyframe::from_track(&track, 0).unwrap();
        copied.paste_into(&mut track, 20);
        assert_eq!(track.sample(20.0).rotation, 45.0);
        assert_eq!(track.position.key_at(20).unwrap().value, Point::new(5.0, 5.0));
    }

    #[test]
    fn clipboard_text_round_trips_and_ignores_ordinary_text() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        let clipboard = Clipboard::Layers(doc.copy_layers(&[id], None).unwrap());
        assert_eq!(Clipboard::from_text(&clipboard.to_text()), Some(clipboard));
        assert_eq!(Clipboard::from_text("just some words"), None);
    }
}
