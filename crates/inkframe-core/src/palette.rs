//! The document's colour palette, as Harmony's: named swatches that own
//! their colours, so changing a swatch recolours every shape painted with
//! it, and versions of the whole palette (day, night) to switch between.
//!
//! A swatch owns its exact colour: every fill and stroke of precisely that
//! colour, on any layer, drawing or symbol, is that swatch's. Shapes store
//! real colours, as they always have, so drawing and saving don't change;
//! the palette just finds and rewrites them. For that to be unambiguous no
//! two swatches share a colour in the same version of the palette, which
//! [`Palette`] keeps true by nudging a clashing colour by one step of blue,
//! too little to see.
//!
//! Each swatch has a colour per version. Switching versions maps every
//! swatch's colour in the old version to its colour in the new one, all at
//! once, so swatches that trade colours don't end up merged.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::document::Document;
use crate::shape::Shape;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    /// The versions' names; there's always at least one.
    pub versions: Vec<String>,
    /// The version in use.
    pub active: usize,
    pub swatches: Vec<Swatch>,
}

impl Default for Palette {
    fn default() -> Self {
        Self { versions: vec!["Default".to_owned()], active: 0, swatches: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub name: String,
    /// One colour per palette version.
    pub colors: Vec<Color>,
}

impl Palette {
    pub fn is_empty(&self) -> bool {
        self.swatches.is_empty() && self.versions.len() <= 1
    }

    /// A swatch's colour in the version in use.
    pub fn color(&self, swatch: usize) -> Option<Color> {
        self.swatches.get(swatch)?.colors.get(self.active).copied()
    }

    /// `color`, moved off any other swatch's colour in `version` (by a step
    /// of blue at a time) so it can be told apart.
    fn unique(&self, version: usize, color: Color, except: Option<usize>) -> Color {
        let taken = |c: Color| {
            self.swatches
                .iter()
                .enumerate()
                .any(|(i, s)| Some(i) != except && s.colors.get(version) == Some(&c))
        };
        let mut color = color;
        while taken(color) {
            color.b = if color.b == 255 { 0 } else { color.b + 1 };
        }
        color
    }
}

impl Document {
    /// Adds a swatch of `color`, the same in every version of the palette,
    /// and returns its index. A colour another swatch has is nudged.
    pub fn add_swatch(&mut self, name: impl Into<String>, color: Color) -> usize {
        let palette = &self.palette;
        let colors = (0..palette.versions.len()).map(|v| palette.unique(v, color, None)).collect();
        self.palette.swatches.push(Swatch { name: name.into(), colors });
        self.palette.swatches.len() - 1
    }

    /// Changes a swatch's colour in the version in use, and every shape of
    /// its old colour with it.
    pub fn set_swatch_color(&mut self, swatch: usize, color: Color) {
        let active = self.palette.active;
        let Some(old) = self.palette.color(swatch) else { return };
        let new = self.palette.unique(active, color, Some(swatch));
        if new == old {
            return;
        }
        self.palette.swatches[swatch].colors[active] = new;
        self.recolor(&[(old, new)]);
    }

    pub fn remove_swatch(&mut self, swatch: usize) {
        if swatch < self.palette.swatches.len() {
            self.palette.swatches.remove(swatch);
        }
    }

    /// Adds a version of the palette, a copy of the one in use, and returns
    /// its index. It isn't switched to.
    pub fn add_palette_version(&mut self, name: impl Into<String>) -> usize {
        let active = self.palette.active;
        self.palette.versions.push(name.into());
        for swatch in &mut self.palette.swatches {
            let color = swatch.colors[active];
            swatch.colors.push(color);
        }
        self.palette.versions.len() - 1
    }

    /// Switches to another version of the palette, recolouring every shape
    /// of each swatch's colour to that swatch's colour in `version`.
    pub fn use_palette_version(&mut self, version: usize) {
        let from = self.palette.active;
        if version >= self.palette.versions.len() || version == from {
            return;
        }
        let map: Vec<(Color, Color)> = self
            .palette
            .swatches
            .iter()
            .map(|s| (s.colors[from], s.colors[version]))
            .filter(|(a, b)| a != b)
            .collect();
        self.recolor(&map);
        self.palette.active = version;
    }

    /// Removes a version of the palette (not the last one), switching away
    /// from it first if it's in use.
    pub fn remove_palette_version(&mut self, version: usize) {
        let palette = &self.palette;
        if palette.versions.len() <= 1 || version >= palette.versions.len() {
            return;
        }
        if palette.active == version {
            self.use_palette_version(if version == 0 { 1 } else { version - 1 });
        }
        self.palette.versions.remove(version);
        for swatch in &mut self.palette.swatches {
            swatch.colors.remove(version);
        }
        if self.palette.active > version {
            self.palette.active -= 1;
        }
    }

    /// Rewrites every fill and stroke colour in the document by `map` (old
    /// colour to new), all at once.
    pub fn recolor(&mut self, map: &[(Color, Color)]) {
        if map.is_empty() {
            return;
        }
        let swap = |c: &mut Color| {
            if let Some((_, new)) = map.iter().find(|(old, _)| old == c) {
                *c = *new;
            }
        };
        let layers = self.layers.iter_mut().chain(self.symbols.iter_mut().flat_map(|s| s.layers.iter_mut()));
        for element in layers.flat_map(|l| l.all_elements_mut()) {
            // Pictures and paint have their own pixels.
            if matches!(element.shape, Shape::Image { .. } | Shape::Paint { .. }) {
                continue;
            }
            if let Some(fill) = &mut element.style.fill {
                swap(fill);
            }
            if let Some(stroke) = &mut element.style.stroke {
                swap(&mut stroke.color);
            }
        }
    }

    /// How many fills and strokes use a swatch now.
    pub fn swatch_uses(&self, swatch: usize) -> usize {
        let Some(color) = self.palette.color(swatch) else { return 0 };
        self.all_layers()
            .flat_map(|l| l.all_elements())
            .filter(|e| !matches!(e.shape, Shape::Image { .. } | Shape::Paint { .. }))
            .map(|e| {
                usize::from(e.style.fill == Some(color))
                    + usize::from(e.style.stroke.map(|s| s.color) == Some(color))
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, Stroke, Style, Transform};

    const SKIN: Color = Color::rgb(240, 180, 140);
    const SHIRT: Color = Color::rgb(60, 90, 200);

    /// A document with a skin-coloured face outlined in shirt blue, and a
    /// palette holding both colours.
    fn doc() -> Document {
        let mut doc = Document::default();
        let face = Element {
            shape: crate::Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(SKIN), stroke: Some(Stroke { color: SHIRT, width: 2.0 }) },
        };
        doc.add_layer("Face", vec![face], 0, Transform::default());
        doc.add_swatch("Skin", SKIN);
        doc.add_swatch("Shirt", SHIRT);
        doc
    }

    fn face_style(doc: &Document) -> Style {
        doc.layers[0].drawings[0].elements[0].style
    }

    #[test]
    fn changing_a_swatch_recolours_everything_painted_with_it() {
        let mut doc = doc();
        doc.set_swatch_color(0, Color::rgb(200, 140, 100));
        assert_eq!(face_style(&doc).fill, Some(Color::rgb(200, 140, 100)));
        assert_eq!(face_style(&doc).stroke.unwrap().color, SHIRT, "the other swatch's colour stays");
        assert_eq!(doc.swatch_uses(0), 1);
    }

    #[test]
    fn swatches_never_share_a_colour() {
        let mut doc = doc();
        let copy = doc.add_swatch("Also skin", SKIN);
        assert_ne!(doc.palette.color(copy), Some(SKIN));
        // Setting one to another's colour nudges it, so the two stay apart.
        doc.set_swatch_color(1, SKIN);
        assert_ne!(doc.palette.color(1), Some(SKIN));
        assert_eq!(face_style(&doc).fill, Some(SKIN), "the skin swatch's shapes untouched");
    }

    #[test]
    fn switching_versions_recolours_the_document_and_back() {
        let mut doc = doc();
        let night = doc.add_palette_version("Night");
        doc.palette.swatches[0].colors[night] = Color::rgb(90, 70, 110);
        // Night shirt takes day skin's colour: swapping all at once keeps
        // the two apart.
        doc.palette.swatches[1].colors[night] = SKIN;
        doc.use_palette_version(night);
        assert_eq!(face_style(&doc).fill, Some(Color::rgb(90, 70, 110)));
        assert_eq!(face_style(&doc).stroke.unwrap().color, SKIN);
        doc.use_palette_version(0);
        assert_eq!(face_style(&doc), doc_style());
    }

    fn doc_style() -> Style {
        face_style(&doc())
    }

    #[test]
    fn removing_the_version_in_use_switches_away_first() {
        let mut doc = doc();
        let night = doc.add_palette_version("Night");
        doc.palette.swatches[0].colors[night] = Color::rgb(1, 2, 3);
        doc.use_palette_version(night);
        doc.remove_palette_version(night);
        assert_eq!(doc.palette.versions.len(), 1);
        assert_eq!(face_style(&doc).fill, Some(SKIN));
    }

    #[test]
    fn palettes_save_and_old_swatches_load_as_one() {
        let doc = doc();
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(back.palette, doc.palette);

        let mut json: serde_json::Value =
            serde_json::from_str(&Document::default().to_json().unwrap()).unwrap();
        json["swatches"] = serde_json::json!([Color::rgb(1, 2, 3)]);
        let old = Document::from_json(&json.to_string()).unwrap();
        assert_eq!(old.palette.color(0), Some(Color::rgb(1, 2, 3)));
    }
}
