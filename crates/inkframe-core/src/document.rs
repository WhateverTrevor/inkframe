use std::fmt;
use std::path::Path as FsPath;

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::geometry::{Point, Rect};
use crate::shape::{Shape, Style};
use crate::track::Track;
use crate::transform::Transform;

/// Bumped whenever the file format changes in a way older builds can't read.
/// Loading a newer file than we understand fails with a clear error instead
/// of silently dropping data.
pub const FORMAT_VERSION: u32 = 3;

/// A complete animation: stage settings plus a stack of layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Length of the animation in frames.
    pub duration: u32,
    /// Stage color. Exports to formats with an alpha channel can leave it out.
    pub background: Color,
    /// Bottom-most layer first, so iterating in order paints back to front.
    /// (The timeline shows them the other way up, like every other editor.)
    pub layers: Vec<Layer>,
    next_layer_id: u64,
}

/// A stable identifier for a layer that survives reordering and undo.
/// Always look layers up by id rather than holding on to an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerId(u64);

impl fmt::Display for LayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A layer is a group of shapes that animate together, the way a tween layer
/// holding a single symbol does in Animate. Its [`Track`] moves, rotates,
/// scales and fades everything in it as one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Locked layers are drawn but can't be selected or edited on the stage.
    #[serde(default)]
    pub locked: bool,
    /// The first frame the layer appears on.
    #[serde(default)]
    pub start: u32,
    /// The last frame it appears on, inclusive. `None` means "until the
    /// animation ends", so making the animation longer makes the layer
    /// longer too, which is nearly always what's wanted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u32>,
    pub elements: Vec<Element>,
    pub track: Track,
}

/// A styled shape inside a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub shape: Shape,
    pub style: Style,
}

/// Frames per second as an exact fraction.
///
/// Broadcast rates such as 23.976 and 29.97 aren't representable exactly as
/// floats; they're really 24000/1001 and 30000/1001. Video encoders want the
/// exact fraction, and getting it wrong causes audio drift in long edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    pub const PRESETS: [FrameRate; 8] = [
        FrameRate::new(12, 1),
        FrameRate::new(24000, 1001),
        FrameRate::new(24, 1),
        FrameRate::new(25, 1),
        FrameRate::new(30000, 1001),
        FrameRate::new(30, 1),
        FrameRate::new(50, 1),
        FrameRate::new(60, 1),
    ];

    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self { numerator, denominator }
    }

    pub fn fps(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(f, "{} fps", self.numerator)
        } else {
            write!(f, "{:.3} fps", self.fps())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("couldn't read or write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the file isn't a valid Inkframe document: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(
        "this file was made by a newer version of Inkframe (format {found}, this build reads up to {FORMAT_VERSION})"
    )]
    UnsupportedVersion { found: u32 },
    #[error("invalid document: {0}")]
    Invalid(&'static str),
}

impl Default for Document {
    /// A 1920×1080, 24 fps, 2-second animation with no layers.
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(24, 1),
            duration: 48,
            background: Color::WHITE,
            layers: Vec::new(),
            next_layer_id: 1,
        }
    }
}

impl Document {
    pub fn stage_rect(&self) -> Rect {
        Rect::from_points(Point::ZERO, Point::new(self.width as f32, self.height as f32))
    }

    /// The last valid frame index.
    pub fn last_frame(&self) -> u32 {
        self.duration.saturating_sub(1)
    }

    /// Adds a layer on top of the stack and returns its id.
    ///
    /// The layer gets a single keyframe at `frame`, so it starts out still.
    pub fn add_layer(
        &mut self,
        name: impl Into<String>,
        elements: Vec<Element>,
        frame: u32,
        transform: Transform,
    ) -> LayerId {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;
        self.layers.push(Layer {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            start: 0,
            end: None,
            elements,
            track: Track::new(frame, transform),
        });
        id
    }

    /// Copies a layer and places the copy directly above the original.
    pub fn duplicate_layer(&mut self, id: LayerId) -> Option<LayerId> {
        let index = self.layer_index(id)?;
        let mut copy = self.layers[index].clone();
        copy.id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;
        copy.name = format!("{} copy", copy.name);
        let new_id = copy.id;
        self.layers.insert(index + 1, copy);
        Some(new_id)
    }

    pub fn remove_layer(&mut self, id: LayerId) -> Option<Layer> {
        let index = self.layer_index(id)?;
        Some(self.layers.remove(index))
    }

    /// Moves a layer up (`delta > 0`, toward the front) or down the stack.
    pub fn move_layer(&mut self, id: LayerId, delta: isize) {
        let Some(from) = self.layer_index(id) else { return };
        let to = from.saturating_add_signed(delta).min(self.layers.len() - 1);
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
    }

    pub fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// The front-most visible, unlocked layer under `point` (stage pixels).
    pub fn hit_test(&self, point: Point, frame: u32, tolerance: f32) -> Option<LayerId> {
        self.layers
            .iter()
            .rev()
            .filter(|l| l.is_shown_at(frame) && !l.locked)
            .find(|l| l.hit_test(point, frame, tolerance))
            .map(|l| l.id)
    }

    /// A name like "Layer 3" that isn't already taken.
    pub fn next_layer_name(&self, prefix: &str) -> String {
        (1..)
            .map(|n| format!("{prefix} {n}"))
            .find(|name| !self.layers.iter().any(|l| &l.name == name))
            .expect("there is always an unused number")
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Parses a document, checking the version and repairing anything that
    /// could have been broken by hand-editing the file.
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        // Peek at the version before deserializing everything, so a file from
        // a newer release gives a useful error instead of a confusing
        // "unknown field" message.
        #[derive(Deserialize)]
        struct VersionOnly {
            version: u32,
        }
        let VersionOnly { version } = serde_json::from_str(json)?;
        if version > FORMAT_VERSION {
            return Err(DocumentError::UnsupportedVersion { found: version });
        }

        let mut doc: Document = serde_json::from_str(json)?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn load(path: impl AsRef<FsPath>) -> Result<Self, DocumentError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Saves atomically: write to a temporary file next to the target, then
    /// rename over it. A crash halfway through can't leave a corrupt file.
    pub fn save(&self, path: impl AsRef<FsPath>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        let tmp = path.with_extension("inkf.tmp");
        std::fs::write(&tmp, self.to_json()?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn validate(&mut self) -> Result<(), DocumentError> {
        if self.width == 0 || self.height == 0 {
            return Err(DocumentError::Invalid("stage size must be at least 1×1"));
        }
        if self.duration == 0 {
            return Err(DocumentError::Invalid("duration must be at least one frame"));
        }
        if self.frame_rate.numerator == 0 || self.frame_rate.denominator == 0 {
            return Err(DocumentError::Invalid("frame rate must be positive"));
        }
        if !self.layers.iter().all(|l| l.track.has_every_property()) {
            return Err(DocumentError::Invalid("every animated property needs at least one keyframe"));
        }
        // A hand-edited file could end a layer before it starts. Rather than
        // refuse the whole file, shrink the layer to a single frame.
        for layer in &mut self.layers {
            if layer.end.is_some_and(|end| end < layer.start) {
                layer.end = Some(layer.start);
            }
        }
        // Never hand out an id that's already in use, even if the stored
        // counter was edited or lost.
        let max_id = self.layers.iter().map(|l| l.id.0).max().unwrap_or(0);
        self.next_layer_id = self.next_layer_id.max(max_id + 1);
        self.version = FORMAT_VERSION;
        Ok(())
    }
}

impl Layer {
    /// Whether the layer exists on `frame`, i.e. it falls within its start and end.
    pub fn is_on(&self, frame: u32) -> bool {
        frame >= self.start && self.end.is_none_or(|end| frame <= end)
    }

    /// Whether the layer is drawn on `frame`: switched on and within its span.
    pub fn is_shown_at(&self, frame: u32) -> bool {
        self.visible && self.is_on(frame)
    }

    /// The last frame the layer is on, given the document's last frame.
    pub fn last_frame(&self, doc_last_frame: u32) -> u32 {
        self.end.unwrap_or(doc_last_frame).max(self.start)
    }

    /// Moves the start, pushing the end along if it would come first.
    pub fn set_start(&mut self, frame: u32) {
        self.start = frame;
        if let Some(end) = self.end
            && end < frame
        {
            self.end = Some(frame);
        }
    }

    /// Moves the end (`None` for "until the animation ends"), pulling the
    /// start back if it would come after.
    pub fn set_end(&mut self, end: Option<u32>) {
        self.end = end;
        if let Some(end) = end {
            self.start = self.start.min(end);
        }
    }

    pub fn transform_at(&self, frame: u32) -> Transform {
        self.track.sample(frame as f32)
    }

    /// Bounds of all elements in the layer's local coordinates.
    pub fn local_bounds(&self) -> Option<Rect> {
        self.elements.iter().map(|e| e.shape.bounds()).reduce(Rect::union)
    }

    /// Whether `point` (stage pixels) hits any element at `frame`.
    ///
    /// The point is mapped into the layer's local space with the inverse
    /// transform, so each shape can do its hit test in its own simple,
    /// unrotated coordinates.
    pub fn hit_test(&self, point: Point, frame: u32, tolerance: f32) -> bool {
        let transform = self.transform_at(frame);
        let Some(to_local) = transform.to_affine().inverse() else {
            return false; // Scaled to nothing: there's nothing to click on.
        };
        let local = to_local.apply(point);
        // Tolerance is given in stage pixels; convert it to local units so a
        // layer scaled up 4× doesn't get a 4× bigger click target.
        let scale = transform.scale.x.abs().max(transform.scale.y.abs()).max(1e-6);
        let local_tolerance = tolerance / scale;
        self.elements.iter().any(|e| e.shape.hit_test(local, &e.style, local_tolerance))
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Stroke;

    fn square_element() -> Element {
        Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style {
                fill: Some(Color::rgb(255, 0, 0)),
                stroke: Some(Stroke { color: Color::BLACK, width: 2.0 }),
            },
        }
    }

    #[test]
    fn json_round_trip() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square_element()], 0, Transform::at(Point::new(50.0, 50.0)));
        doc.layer_mut(id).unwrap().track.set_pose(24, Transform::at(Point::new(500.0, 50.0)));

        let loaded = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(loaded, doc);
    }

    #[test]
    fn rejects_newer_versions() {
        let json = Document::default()
            .to_json()
            .unwrap()
            .replace(&format!("\"version\": {FORMAT_VERSION}"), "\"version\": 999");
        assert!(matches!(Document::from_json(&json), Err(DocumentError::UnsupportedVersion { found: 999 })));
    }

    #[test]
    fn hit_test_follows_animation_and_stacking() {
        let mut doc = Document::default();
        let bottom = doc.add_layer("A", vec![square_element()], 0, Transform::at(Point::new(100.0, 100.0)));
        let top = doc.add_layer("B", vec![square_element()], 0, Transform::at(Point::new(100.0, 100.0)));
        doc.layer_mut(top).unwrap().track.set_pose(10, Transform::at(Point::new(800.0, 100.0)));

        // Frame 0: both overlap; the top one wins.
        assert_eq!(doc.hit_test(Point::new(100.0, 100.0), 0, 0.0), Some(top));
        // Frame 10: the top layer has moved away, revealing the bottom one.
        assert_eq!(doc.hit_test(Point::new(100.0, 100.0), 10, 0.0), Some(bottom));
        assert_eq!(doc.hit_test(Point::new(800.0, 100.0), 10, 0.0), Some(top));
    }

    #[test]
    fn locked_layers_are_not_hit() {
        let mut doc = Document::default();
        let id = doc.add_layer("A", vec![square_element()], 0, Transform::at(Point::new(100.0, 100.0)));
        doc.layer_mut(id).unwrap().locked = true;
        assert_eq!(doc.hit_test(Point::new(100.0, 100.0), 0, 0.0), None);
    }

    #[test]
    fn layers_only_exist_within_their_span() {
        let mut doc = Document::default();
        let id = doc.add_layer("A", vec![square_element()], 0, Transform::at(Point::new(100.0, 100.0)));
        let layer = doc.layer_mut(id).unwrap();
        layer.set_start(10);
        layer.set_end(Some(20));

        assert!(!layer.is_on(9) && layer.is_on(10) && layer.is_on(20) && !layer.is_on(21));
        // Off-span layers can't be clicked either.
        assert_eq!(doc.hit_test(Point::new(100.0, 100.0), 5, 0.0), None);
        assert_eq!(doc.hit_test(Point::new(100.0, 100.0), 15, 0.0), Some(id));
    }

    #[test]
    fn span_edits_keep_start_before_end() {
        let mut doc = Document::default();
        let id = doc.add_layer("A", vec![], 0, Transform::default());
        let layer = doc.layer_mut(id).unwrap();
        layer.set_end(Some(5));
        layer.set_start(8);
        assert_eq!((layer.start, layer.end), (8, Some(8)));
        layer.set_end(Some(3));
        assert_eq!((layer.start, layer.end), (3, Some(3)));
    }

    #[test]
    fn open_ended_layers_follow_the_duration() {
        let mut doc = Document::default();
        let id = doc.add_layer("A", vec![], 0, Transform::default());
        assert_eq!(doc.layer(id).unwrap().last_frame(doc.last_frame()), 47);
        doc.duration = 100;
        assert_eq!(doc.layer(id).unwrap().last_frame(doc.last_frame()), 99);
    }

    #[test]
    fn inverted_spans_are_repaired_on_load() {
        let mut doc = Document::default();
        doc.add_layer("A", vec![], 0, Transform::default());
        let json = doc.to_json().unwrap().replace("\"start\": 0", "\"start\": 9, \"end\": 4");
        let loaded = Document::from_json(&json).unwrap();
        assert_eq!((loaded.layers[0].start, loaded.layers[0].end), (9, Some(9)));
    }

    #[test]
    fn ids_are_never_reused() {
        let mut doc = Document::default();
        let a = doc.add_layer("A", vec![], 0, Transform::default());
        doc.remove_layer(a);
        let b = doc.add_layer("B", vec![], 0, Transform::default());
        assert_ne!(a, b);
    }

    #[test]
    fn move_layer_clamps_to_the_stack() {
        let mut doc = Document::default();
        let a = doc.add_layer("A", vec![], 0, Transform::default());
        let b = doc.add_layer("B", vec![], 0, Transform::default());
        doc.move_layer(a, 10);
        assert_eq!(doc.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [b, a]);
        doc.move_layer(a, -10);
        assert_eq!(doc.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, b]);
    }

    #[test]
    fn next_layer_name_skips_taken_names() {
        let mut doc = Document::default();
        doc.add_layer("Layer 1", vec![], 0, Transform::default());
        assert_eq!(doc.next_layer_name("Layer"), "Layer 2");
    }
}
