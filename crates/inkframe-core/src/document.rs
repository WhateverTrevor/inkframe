use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::audio::AudioTrack;
use crate::blend::BlendMode;
use crate::color::Color;
use crate::controller::Controller;
use crate::deform::Deformer;
use crate::drawing::{Drawing, Exposure};
use crate::effects::{ColorEffect, Filter};
use crate::font_file::FontFile;
use crate::geometry::{Point, Rect};
use crate::ik::RotationLimits;
use crate::image::ImageAsset;
use crate::morph::ShapeTween;
use crate::paint::{Canvas, CanvasId};
use crate::palette::Palette;
use crate::particles::Emitter;
use crate::shape::{Shape, Style};
use crate::symbol::{Instance, Symbol, SymbolId};
use crate::track::Track;
use crate::transform::Transform;
use crate::warp::Warp;

/// Where a layer lives: on the main timeline, or inside a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TimelineId {
    #[default]
    Main,
    Symbol(SymbolId),
}

/// Bumped whenever the file format changes in a way older builds can't read.
/// Loading a newer file than we understand fails with a clear error instead
/// of silently dropping data.
pub const FORMAT_VERSION: u32 = 39;

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
    /// The library: reusable symbols that layers can hold instances of.
    #[serde(default)]
    pub symbols: Vec<Symbol>,
    /// Sound files laid against the main timeline.
    #[serde(default)]
    pub audio: Vec<AudioTrack>,
    /// Image files that layers place on the stage.
    #[serde(default)]
    pub images: Vec<ImageAsset>,
    /// Font files that go with the project (see `font_file.rs`).
    #[serde(default)]
    pub fonts: Vec<FontFile>,
    /// The canvases paint layers paint on (see `paint.rs`). Only their
    /// sizes are in the JSON; the pixels are saved beside it.
    #[serde(default)]
    pub canvases: BTreeMap<CanvasId, Canvas>,
    /// The colour palette: swatches that own their colours, in one or more
    /// versions (see `palette.rs`). Older builds ignore it.
    #[serde(default, skip_serializing_if = "Palette::is_empty")]
    pub palette: Palette,
    /// Master controllers: sliders that pose several layers at once (see
    /// `controller.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controllers: Vec<Controller>,
    /// Draw the main timeline's layers far to near by their depth on each
    /// frame, rather than in the timeline's order (see `camera.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stack_by_depth: bool,
    /// The Library's folders, kept here so an empty one stays (see
    /// `library.rs`). Symbols, images and sounds say which they're in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library_folders: Vec<String>,
    /// Guide lines dragged out of the rulers (see `ruler_guides.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ruler_guides: Vec<crate::RulerGuide>,
    /// What the web page export does on reaching frames of the main
    /// timeline (see `web_actions.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frame_actions: Vec<crate::FrameAction>,
    /// A perspective guide over the stage (see `perspective.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perspective: Option<crate::Perspective>,
    /// Where nodes have been moved in the node view (see `node_places.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_places: Vec<crate::NodePlace>,
    /// Plain colours, as the Swatches panel saved them before there was a
    /// palette; loading makes each a swatch.
    #[serde(default, rename = "swatches", skip_serializing)]
    pub(crate) old_swatches: Vec<Color>,
    pub(crate) next_layer_id: u64,
    #[serde(default = "first_id")]
    pub(crate) next_symbol_id: u64,
    #[serde(default = "first_id")]
    pub(crate) next_audio_id: u64,
    #[serde(default = "first_id")]
    pub(crate) next_image_id: u64,
    #[serde(default = "first_id")]
    pub(crate) next_font_id: u64,
    #[serde(default = "first_id")]
    pub(crate) next_canvas_id: u64,
    /// Skeletons shown at rest rather than posed: rig setup, as Moho's
    /// frame 0 (see `skeleton.rs`). How the document is being looked at,
    /// not part of it: never saved, and never makes an undo step.
    #[serde(skip)]
    pub rest_pose: crate::RestPoseView,
}

/// A stable identifier for a layer that survives reordering and undo.
/// Always look layers up by id rather than holding on to an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayerId(pub(crate) u64);

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
    /// The layer's artwork: usually one drawing, or several to swap
    /// between (a mouth's shapes), with `exposure` picking which shows on
    /// each frame. Never empty. See `drawing.rs`.
    #[serde(alias = "elements", deserialize_with = "crate::drawing::drawings_or_elements")]
    pub drawings: Vec<Drawing>,
    /// Which drawing shows from each key's frame on. Empty for a layer
    /// that always shows its first drawing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposure: Vec<Exposure>,
    /// Swaps that are morphed into rather than jumped to (see `morph.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape_tweens: Vec<ShapeTween>,
    /// A symbol drawn by this layer, through the layer's transform. Instance
    /// layers normally have no elements of their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<Instance>,
    pub track: Track,
    /// A motion guide: a path other layers can follow (see `guide.rs`).
    /// Shown while editing, never exported.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub guide: bool,
    /// The guide layer this layer travels along, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<LayerId>,
    /// A mask: its drawing is a window onto the layers it masks, rather
    /// than drawn itself (see `mask.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mask: bool,
    /// The mask layer this layer is seen through, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub masked_by: Option<LayerId>,
    /// The layer whose motion this one inherits, if any (see `rig.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<LayerId>,
    /// The point, in the layer's own coordinates, that it rotates and
    /// scales around; its position is where this point sits.
    #[serde(default, skip_serializing_if = "is_origin")]
    pub pivot: Point,
    /// How far IK may turn this joint (see `ik.rs`). `None` is free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_limits: Option<RotationLimits>,
    /// Bones that bend the drawing itself (see `deform.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deformer: Option<Deformer>,
    /// A free-form grid that warps the drawing (see `warp.rs`), after any
    /// bones have bent it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warp: Option<Warp>,
    /// For a folder, a skeleton the layers in it bind to (see
    /// `skeleton.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<crate::Skeleton>,
    /// For a layer in a skeleton's folder, how it follows the bones.
    #[serde(default, skip_serializing_if = "crate::BoneBinding::is_region")]
    pub bone_binding: crate::BoneBinding,
    /// In the web page export, the layer's a button that does this when
    /// clicked (see `web_actions.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<crate::WebAction>,
    /// The grain its lines and strokes are drawn with (see `line.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_texture: Option<crate::LineTexture>,
    /// This folder is a switch: it shows one of its layers at a time (see
    /// `switch.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub switch: bool,
    /// Which layer a switch shows, from each key's frame on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub switch_keys: Vec<crate::SwitchKey>,
    /// How the layer mixes with what's beneath it (see `blend.rs`).
    #[serde(default, skip_serializing_if = "BlendMode::is_normal")]
    pub blend: BlendMode,
    /// The colour chosen for the layer in the timeline and on the stage;
    /// `None` uses one picked by its id (see `folder.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// This layer is a folder: it draws nothing and holds the layers that
    /// name it in `in_folder`, which sit directly below it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
    /// The folder this layer is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_folder: Option<LayerId>,
    /// This layer is the camera (see `camera.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub camera: bool,
    /// A depth as format 28 and older stored it, before it could animate.
    /// Loading moves it into the track's Depth property (see `validate`);
    /// it's never saved.
    #[serde(default, rename = "depth", skip_serializing)]
    pub(crate) unanimated_depth: f32,
    /// Makes this a particle layer, spraying particles (see `particles.rs`).
    /// Its drawing, if any, is what each particle looks like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub particles: Option<Emitter>,
    /// For an instance, other symbols it switches to partway through, in
    /// order of frame (see `Layer::symbol_at`): a hand swapped for another
    /// pose while the arm it's on keeps moving.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_swaps: Vec<crate::SymbolSwap>,
    /// A colour effect and filters as format 22 stored them, before they
    /// could animate. Loading moves them into the track's Effects property
    /// (see `validate`); they're never saved.
    #[serde(default, rename = "color_effect", skip_serializing)]
    pub(crate) unanimated_color_effect: ColorEffect,
    #[serde(default, rename = "filters", skip_serializing)]
    pub(crate) unanimated_filters: Vec<Filter>,
}

fn is_origin(p: &Point) -> bool {
    *p == Point::ZERO
}

/// A styled shape inside a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub shape: Shape,
    pub style: Style,
}

impl Element {
    /// The area the element paints, stroke included.
    ///
    /// A stroke is centred on the outline, so half its width lies outside.
    /// Strokes are drawn with round joins and caps, which never reach further
    /// than that half width (unlike mitered corners, which spike outward), so
    /// growing the outline's bounds by half the width is exact.
    pub fn bounds(&self) -> Rect {
        let half_stroke = self.style.stroke.map_or(0.0, |s| s.width.max(0.0) / 2.0);
        self.shape.bounds().expand(half_stroke)
    }
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
    #[error("the project file is damaged: {0}")]
    Corrupt(String),
}

impl Default for Document {
    /// A 1920×1080, 24 fps, 2-second animation with no layers.
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            rest_pose: crate::RestPoseView::default(),
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(24, 1),
            duration: 48,
            background: Color::WHITE,
            layers: Vec::new(),
            symbols: Vec::new(),
            audio: Vec::new(),
            images: Vec::new(),
            fonts: Vec::new(),
            next_layer_id: 1,
            next_symbol_id: 1,
            next_audio_id: 1,
            next_image_id: 1,
            next_font_id: 1,
            canvases: BTreeMap::new(),
            palette: Palette::default(),
            controllers: Vec::new(),
            stack_by_depth: false,
            library_folders: Vec::new(),
            ruler_guides: Vec::new(),
            perspective: None,
            node_places: Vec::new(),
            frame_actions: Vec::new(),
            old_swatches: Vec::new(),
            next_canvas_id: 1,
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

    /// The layers of a timeline, bottom-most first. Empty for a symbol that
    /// doesn't exist.
    pub fn layers_of(&self, timeline: TimelineId) -> &[Layer] {
        match timeline {
            TimelineId::Main => &self.layers,
            TimelineId::Symbol(id) => self.symbol(id).map_or(&[], |s| &s.layers),
        }
    }

    pub(crate) fn layers_of_mut(&mut self, timeline: TimelineId) -> Option<&mut Vec<Layer>> {
        match timeline {
            TimelineId::Main => Some(&mut self.layers),
            TimelineId::Symbol(id) => self.symbol_mut(id).map(|s| &mut s.layers),
        }
    }

    /// A timeline's length in frames: the document's, or a symbol's own.
    pub fn duration_of(&self, timeline: TimelineId) -> u32 {
        match timeline {
            TimelineId::Main => self.duration,
            TimelineId::Symbol(id) => self.symbol(id).map_or(1, |s| s.duration),
        }
    }

    /// Which timeline a layer lives in.
    pub fn timeline_of(&self, id: LayerId) -> Option<TimelineId> {
        if self.layers.iter().any(|l| l.id == id) {
            return Some(TimelineId::Main);
        }
        self.symbols.iter().find(|s| s.layers.iter().any(|l| l.id == id)).map(|s| TimelineId::Symbol(s.id))
    }

    /// Adds a layer on top of the main timeline's stack and returns its id.
    ///
    /// The layer gets a single keyframe at `frame`, so it starts out still.
    pub fn add_layer(
        &mut self,
        name: impl Into<String>,
        elements: Vec<Element>,
        frame: u32,
        transform: Transform,
    ) -> LayerId {
        self.add_layer_to(TimelineId::Main, name, elements, frame, transform)
            .expect("the main timeline always exists")
    }

    /// Adds a layer on top of any timeline's stack. Returns `None` if the
    /// timeline is a symbol that doesn't exist.
    pub fn add_layer_to(
        &mut self,
        timeline: TimelineId,
        name: impl Into<String>,
        elements: Vec<Element>,
        frame: u32,
        transform: Transform,
    ) -> Option<LayerId> {
        let id = self.allocate_layer_id();
        self.layers_of_mut(timeline)?.push(Layer {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            start: 0,
            end: None,
            drawings: vec![Drawing::new("1", elements)],
            exposure: Vec::new(),
            shape_tweens: Vec::new(),
            instance: None,
            track: Track::new(frame, transform),
            guide: false,
            follow: None,
            mask: false,
            masked_by: None,
            parent: None,
            pivot: Point::ZERO,
            rotation_limits: None,
            deformer: None,
            warp: None,
            skeleton: None,
            bone_binding: crate::BoneBinding::Region,
            switch: false,
            switch_keys: Vec::new(),
            line_texture: None,
            button: None,
            blend: BlendMode::Normal,
            color: None,
            folder: false,
            in_folder: None,
            camera: false,
            unanimated_depth: 0.0,
            particles: None,
            symbol_swaps: Vec::new(),
            unanimated_color_effect: ColorEffect::None,
            unanimated_filters: Vec::new(),
        });
        if timeline == TimelineId::Main {
            self.keep_camera_on_top();
        }
        Some(id)
    }

    /// Copies a layer and places the copy directly above the original, in
    /// whichever timeline it lives in.
    pub fn duplicate_layer(&mut self, id: LayerId) -> Option<LayerId> {
        let (timeline, index) = self.locate(id)?;
        let new_id = self.allocate_layer_id();
        let layers = self.layers_of_mut(timeline)?;
        let mut copy = layers[index].clone();
        copy.id = new_id;
        copy.name = format!("{} copy", copy.name);
        // There's only ever one camera.
        copy.camera = false;
        // Painting on the copy mustn't paint on the original.
        for drawing in &mut copy.drawings {
            self.separate_canvases(&mut drawing.elements, false);
        }
        self.layers_of_mut(timeline)?.insert(index + 1, copy);
        Some(new_id)
    }

    pub fn remove_layer(&mut self, id: LayerId) -> Option<Layer> {
        let (timeline, index) = self.locate(id)?;
        Some(self.layers_of_mut(timeline)?.remove(index))
    }

    /// Moves a layer up (`delta > 0`, toward the front) or down its stack.
    pub fn move_layer(&mut self, id: LayerId, delta: isize) {
        let Some((timeline, from)) = self.locate(id) else { return };
        let Some(layers) = self.layers_of_mut(timeline) else { return };
        let to = from.saturating_add_signed(delta).min(layers.len() - 1);
        let layer = layers.remove(from);
        layers.insert(to, layer);
        self.keep_camera_on_top();
    }

    /// Moves a layer to position `index` in its timeline's stack (0 is the
    /// bottom), shifting the others along.
    pub fn move_layer_to(&mut self, id: LayerId, index: usize) {
        let Some((timeline, from)) = self.locate(id) else { return };
        let Some(layers) = self.layers_of_mut(timeline) else { return };
        let layer = layers.remove(from);
        layers.insert(index.min(layers.len()), layer);
        self.keep_camera_on_top();
    }

    /// A layer's position in its own timeline's stack.
    pub fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.locate(id).map(|(_, index)| index)
    }

    fn locate(&self, id: LayerId) -> Option<(TimelineId, usize)> {
        let timeline = self.timeline_of(id)?;
        let index = self.layers_of(timeline).iter().position(|l| l.id == id)?;
        Some((timeline, index))
    }

    /// Finds a layer anywhere in the document. Layer ids are unique across
    /// every timeline, so there's no need to say which one to look in.
    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.all_layers().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        let nested = self.symbols.iter_mut().flat_map(|s| s.layers.iter_mut());
        self.layers.iter_mut().chain(nested).find(|l| l.id == id)
    }

    /// Every layer in the document: the main timeline's, then each symbol's.
    pub fn all_layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().chain(self.symbols.iter().flat_map(|s| s.layers.iter()))
    }

    /// The front-most visible, unlocked layer of the main timeline under
    /// `point` (stage pixels).
    pub fn hit_test(&self, point: Point, frame: u32, tolerance: f32) -> Option<LayerId> {
        self.hit_test_in(TimelineId::Main, point, frame, tolerance)
    }

    /// Whether `point` is on the layer's drawing at `frame`. `point` is in
    /// the coordinates of the layer's timeline.
    pub fn hits_layer(&self, layer: &Layer, point: Point, frame: u32, tolerance: f32) -> bool {
        self.layer_hit(layer, point, frame, tolerance)
    }

    /// Like [`Document::hit_test`], for any timeline. For a symbol, `point`
    /// is in the symbol's own coordinates and `frame` is a symbol frame.
    pub fn hit_test_in(
        &self,
        timeline: TimelineId,
        point: Point,
        frame: u32,
        tolerance: f32,
    ) -> Option<LayerId> {
        self.stacking_order(timeline, frame)
            .into_iter()
            .rev()
            // Mask layers aren't drawn, so clicking where one is should
            // reach what's visible there instead. (The editor still lets
            // a selected mask be dragged; see `Editor::hit_test`.)
            .filter(|l| self.shows(l, frame) && !l.locked && !l.mask)
            .find(|l| self.layer_hit(l, point, frame, tolerance))
            .map(|l| l.id)
    }

    /// A name like "Layer 3" that isn't already taken on the main timeline.
    pub fn next_layer_name(&self, prefix: &str) -> String {
        self.next_layer_name_in(TimelineId::Main, prefix)
    }

    /// A name like "Layer 3" that isn't already taken in `timeline`.
    pub fn next_layer_name_in(&self, timeline: TimelineId, prefix: &str) -> String {
        let layers = self.layers_of(timeline);
        (1..)
            .map(|n| format!("{prefix} {n}"))
            .find(|name| !layers.iter().any(|l| &l.name == name))
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

    pub(crate) fn allocate_layer_id(&mut self) -> LayerId {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;
        id
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
        // counters were edited or lost. Layer ids are unique across the
        // whole document, symbols included.
        let max_layer_id = self.all_layers().map(|l| l.id.0).max().unwrap_or(0);
        self.next_layer_id = self.next_layer_id.max(max_layer_id + 1);
        let max_symbol_id = self.symbols.iter().map(|s| s.id.0).max().unwrap_or(0);
        self.next_symbol_id = self.next_symbol_id.max(max_symbol_id + 1);
        let max_audio_id = self.audio.iter().map(|a| a.id.0).max().unwrap_or(0);
        self.next_audio_id = self.next_audio_id.max(max_audio_id + 1);
        let max_image_id = self.images.iter().map(|i| i.id.0).max().unwrap_or(0);
        self.next_image_id = self.next_image_id.max(max_image_id + 1);
        let max_font_id = self.fonts.iter().map(|f| f.id.0).max().unwrap_or(0);
        self.next_font_id = self.next_font_id.max(max_font_id + 1);

        if self.symbols.iter().any(|s| {
            s.layers.iter().flat_map(Layer::symbols_used).any(|used| self.symbol_contains(used, s.id))
        }) {
            return Err(DocumentError::Invalid("a symbol can't contain an instance of itself"));
        }
        self.tidy_controllers();
        for (i, color) in std::mem::take(&mut self.old_swatches).into_iter().enumerate() {
            self.add_swatch(format!("Colour {}", i + 1), color);
        }
        // Effects from format 22, which couldn't animate: they become the
        // Effects property's value throughout.
        let layers = self.layers.iter_mut().chain(self.symbols.iter_mut().flat_map(|s| s.layers.iter_mut()));
        for layer in layers {
            let depth = std::mem::take(&mut layer.unanimated_depth);
            if depth != 0.0 {
                let first = layer.track.key_frames().first().copied().unwrap_or(0);
                layer.track.depth = crate::channel::Channel::new(first, depth);
            }
            let color = std::mem::take(&mut layer.unanimated_color_effect);
            let filters = std::mem::take(&mut layer.unanimated_filters);
            if !color.is_none() || !filters.is_empty() {
                let first = layer.track.key_frames().first().copied().unwrap_or(0);
                layer.track.effects =
                    crate::channel::Channel::new(first, crate::effects::Effects { color, filters });
            }
        }
        // Folders that went missing or contents out of place in a
        // hand-edited file.
        let timelines: Vec<TimelineId> = std::iter::once(TimelineId::Main)
            .chain(self.symbols.iter().map(|s| TimelineId::Symbol(s.id)))
            .collect();
        for timeline in timelines {
            self.tidy_folders(timeline);
        }
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

    /// Bounds of what the layer's own shapes paint on `frame`, strokes
    /// included, in its local coordinates. For the full bounds, including a
    /// symbol instance, use [`Document::layer_bounds`].
    pub fn shape_bounds(&self, frame: u32) -> Option<Rect> {
        self.shown_elements(frame).iter().map(Element::bounds).reduce(Rect::union)
    }
}

fn default_true() -> bool {
    true
}

fn first_id() -> u64 {
    1
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
                stroke: Some(Stroke::new(Color::BLACK, 2.0)),
                gradient: None,
            },
        }
    }

    #[test]
    fn bounds_include_half_the_stroke() {
        // A 100-unit square with a 2-unit stroke paints 1 unit past each edge.
        let bounds = square_element().bounds();
        assert_eq!((bounds.min, bounds.max), (Point::new(-51.0, -51.0), Point::new(51.0, 51.0)));

        let mut unstroked = square_element();
        unstroked.style.stroke = None;
        assert_eq!(unstroked.bounds().width(), 100.0);
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
