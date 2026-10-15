//! Drawing substitution: a layer with several drawings that it swaps between.
//!
//! A character's mouth is a handful of drawings (closed, "A", "E", "O",
//! smile) that never move on their own; what changes over time is *which*
//! one shows. Harmony and OpenToonz build their characters this way, and
//! call the frame-by-frame choice of drawing the **exposure**. Here each
//! layer holds a list of named [`Drawing`]s, and [`Exposure`] keys say "from
//! this frame on, show that drawing". Nothing tweens between drawings: a
//! swap is instant, like a hold key.
//!
//! Most layers have a single drawing and no exposure keys at all, which is
//! exactly how layers worked before drawings existed. Files from before
//! version 14 stored that one drawing's shapes directly as the layer's
//! `elements`, and still load (see [`drawings_or_elements`]).
//!
//! Drawings are referred to by their index in the layer's list. Removing
//! one renumbers the exposure keys after it, which [`Layer::remove_drawing`]
//! takes care of.

use serde::{Deserialize, Deserializer, Serialize};

use crate::document::{Element, Layer};

/// One of a layer's drawings: a set of shapes it can show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    /// What the animator calls it: "1", "2", or for a mouth, "A" or "O".
    pub name: String,
    pub elements: Vec<Element>,
}

impl Drawing {
    pub fn new(name: impl Into<String>, elements: Vec<Element>) -> Self {
        Self { name: name.into(), elements }
    }
}

/// From `frame` on (until the next key), the layer shows `drawing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exposure {
    pub frame: u32,
    /// An index into the layer's drawings.
    pub drawing: usize,
}

impl Layer {
    /// Which drawing shows on `frame`: the latest exposure key at or before
    /// it, or the first drawing before any key.
    pub fn drawing_index_at(&self, frame: u32) -> usize {
        let key = self.exposure.iter().take_while(|key| key.frame <= frame).last();
        key.map_or(0, |key| key.drawing).min(self.drawings.len().saturating_sub(1))
    }

    /// The shapes showing on `frame`.
    pub fn elements_at(&self, frame: u32) -> &[Element] {
        self.drawings.get(self.drawing_index_at(frame)).map_or(&[], |d| &d.elements)
    }

    /// The shapes showing on `frame`, to edit: drawing on the stage changes
    /// whichever drawing is exposed there.
    pub fn elements_at_mut(&mut self, frame: u32) -> &mut Vec<Element> {
        let index = self.drawing_index_at(frame);
        &mut self.drawings[index].elements
    }

    /// Every shape in every drawing, for questions like "is this image used
    /// anywhere?".
    pub fn all_elements(&self) -> impl Iterator<Item = &Element> {
        self.drawings.iter().flat_map(|d| &d.elements)
    }

    pub fn all_elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.drawings.iter_mut().flat_map(|d| &mut d.elements)
    }

    /// Shows `drawing` from `frame` on, until the next exposure key.
    pub fn expose(&mut self, frame: u32, drawing: usize) {
        if drawing >= self.drawings.len() {
            return;
        }
        match self.exposure.binary_search_by_key(&frame, |key| key.frame) {
            Ok(i) => self.exposure[i].drawing = drawing,
            Err(i) => self.exposure.insert(i, Exposure { frame, drawing }),
        }
        self.tidy_exposure();
    }

    /// Adds a drawing to the end of the list and returns its index. It
    /// isn't shown anywhere until it's exposed.
    pub fn add_drawing(&mut self, drawing: Drawing) -> usize {
        self.drawings.push(drawing);
        self.drawings.len() - 1
    }

    /// A name for a new drawing: the lowest number no drawing is called yet,
    /// which is how Harmony numbers them.
    pub fn next_drawing_name(&self) -> String {
        let taken = |n: &usize| self.drawings.iter().any(|d| d.name == n.to_string());
        (1..).find(|n| !taken(n)).expect("some number is free").to_string()
    }

    /// Removes a drawing, and the exposure keys that showed it. A layer
    /// always keeps at least one drawing, so removing the last does nothing.
    pub fn remove_drawing(&mut self, index: usize) -> bool {
        if self.drawings.len() <= 1 || index >= self.drawings.len() {
            return false;
        }
        self.drawings.remove(index);
        self.exposure.retain(|key| key.drawing != index);
        for key in &mut self.exposure {
            if key.drawing > index {
                key.drawing -= 1;
            }
        }
        self.tidy_exposure();
        true
    }

    /// Drops keys that change nothing: ones showing the drawing that was
    /// already showing.
    fn tidy_exposure(&mut self) {
        let mut showing = 0;
        self.exposure.retain(|key| {
            let changes = key.drawing != showing;
            showing = key.drawing;
            changes
        });
    }
}

/// Reads a layer's drawings, or, from a file before version 14, the bare
/// list of shapes that was its one drawing.
pub(crate) fn drawings_or_elements<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Drawing>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Drawings(Vec<Drawing>),
        Elements(Vec<Element>),
    }
    let drawings = match Stored::deserialize(deserializer)? {
        Stored::Drawings(drawings) if !drawings.is_empty() => drawings,
        // An empty list reads as either; both mean one empty drawing.
        Stored::Drawings(_) => vec![Drawing::new("1", Vec::new())],
        Stored::Elements(elements) => vec![Drawing::new("1", elements)],
    };
    Ok(drawings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Document, Point, Shape, Style, Transform};

    fn shape(width: f32) -> Element {
        Element {
            shape: Shape::Rect { width, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        }
    }

    /// A mouth layer with three drawings, "closed", "A" and "O", 10, 20 and
    /// 30 wide.
    fn mouth() -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("Mouth", vec![shape(10.0)], 0, Transform::at(Point::ZERO));
        let mut layer = doc.layer(id).unwrap().clone();
        layer.drawings[0].name = "closed".into();
        layer.add_drawing(Drawing::new("A", vec![shape(20.0)]));
        layer.add_drawing(Drawing::new("O", vec![shape(30.0)]));
        layer
    }

    fn width_at(layer: &Layer, frame: u32) -> f32 {
        match layer.elements_at(frame)[0].shape {
            Shape::Rect { width, .. } => width,
            _ => unreachable!(),
        }
    }

    #[test]
    fn exposure_keys_hold_until_the_next() {
        let mut layer = mouth();
        layer.expose(4, 1);
        layer.expose(10, 2);
        layer.expose(12, 0);
        let widths: Vec<f32> = [0, 3, 4, 9, 10, 11, 12, 40].map(|f| width_at(&layer, f)).to_vec();
        assert_eq!(widths, [10.0, 10.0, 20.0, 20.0, 30.0, 30.0, 10.0, 10.0]);
    }

    #[test]
    fn keys_that_change_nothing_are_dropped() {
        let mut layer = mouth();
        layer.expose(0, 0);
        assert!(layer.exposure.is_empty(), "the first drawing already shows from the start");
        layer.expose(5, 1);
        layer.expose(8, 1);
        assert_eq!(layer.exposure, [Exposure { frame: 5, drawing: 1 }]);
        // Re-exposing a key's drawing where it already shows leaves one key.
        layer.expose(5, 0);
        assert!(layer.exposure.is_empty());
    }

    #[test]
    fn removing_a_drawing_renumbers_the_keys_after_it() {
        let mut layer = mouth();
        layer.expose(4, 1);
        layer.expose(10, 2);
        assert!(layer.remove_drawing(1));
        assert_eq!(layer.drawings.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["closed", "O"]);
        assert_eq!(layer.exposure, [Exposure { frame: 10, drawing: 1 }]);
        assert_eq!(width_at(&layer, 6), 10.0);
        assert_eq!(width_at(&layer, 10), 30.0);
    }

    #[test]
    fn the_last_drawing_stays() {
        let mut layer = mouth();
        assert!(layer.remove_drawing(2));
        assert!(layer.remove_drawing(1));
        assert!(!layer.remove_drawing(0));
        assert_eq!(layer.drawings.len(), 1);
    }

    #[test]
    fn new_drawings_take_the_lowest_free_number() {
        let mut layer = mouth();
        assert_eq!(layer.next_drawing_name(), "1");
        layer.add_drawing(Drawing::new("1", Vec::new()));
        assert_eq!(layer.next_drawing_name(), "2");
    }

    #[test]
    fn old_layers_load_as_one_drawing() {
        let layer = mouth();
        let mut json = serde_json::to_value(&layer).unwrap();
        let object = json.as_object_mut().unwrap();
        object.remove("drawings");
        object.remove("exposure");
        object.insert("elements".into(), serde_json::to_value(vec![shape(10.0)]).unwrap());
        let old: Layer = serde_json::from_value(json).unwrap();
        assert_eq!(old.drawings, [Drawing::new("1", vec![shape(10.0)])]);
        assert!(old.exposure.is_empty());
    }

    #[test]
    fn drawings_round_trip() {
        let mut layer = mouth();
        layer.expose(3, 2);
        let json = serde_json::to_string(&layer).unwrap();
        assert_eq!(serde_json::from_str::<Layer>(&json).unwrap(), layer);
    }
}
