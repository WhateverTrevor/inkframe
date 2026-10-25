//! Editor state and the operations the UI can perform on it.
//!
//! Everything here is UI-toolkit agnostic: panels call these methods instead
//! of poking at the document directly, which keeps the rules ("a new
//! keyframe goes on the current frame", "selection follows undo") in one
//! place.

use std::path::{Path, PathBuf};

use inkframe_core::FontFileId;
use inkframe_core::{
    Affine, BrushSettings, BucketSettings, CanvasId, Color, Document, DocumentError, Drawing, Element,
    GradientShape, History, Layer, LayerId, Point, Property, Rect, Selection, Shape, Stroke, Style, SymbolId,
    Text, TimelineId, ToneRange, Transform,
};
use inkframe_core::{Asset, Clipboard, CopiedKeyframe, ImageId};
use inkframe_render::ImageLibrary;

use crate::guides::Guides;

/// The tools in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Subselect,
    /// Shapes a pencil line's thickness point by point (see `line.rs`).
    Width,
    Pen,
    Rectangle,
    Ellipse,
    Pencil,
    Text,
    /// Poses a rig by dragging: the joints above the dragged layer turn so
    /// it follows the pointer (inverse kinematics).
    Bone,
    /// Draws a skeleton's bones and moves them at rest (see `skeleton.rs`).
    AddBone,
    /// Pans, zooms and turns the camera (see `camera.rs` in the core).
    Camera,
    /// Paints pixels on a paint layer (see `paint.rs` and `brush.rs`).
    Brush,
    Eraser,
    /// Picks the paint colour from the stage.
    Eyedropper,
    PaintBucket,
    Gradient,
    Smudge,
    Blur,
    Sharpen,
    Dodge,
    Burn,
    CloneStamp,
    /// The selection tools: what the painting tools may change.
    RectMarquee,
    EllipseMarquee,
    Lasso,
    PolygonLasso,
    MagicWand,
}

impl Tool {
    /// The toolbar, top to bottom. Tools that do similar jobs share a
    /// slot, as in Photoshop: the slot shows whichever was used last, its
    /// key picks that one, and Shift with the key steps through the rest.
    pub const GROUPS: [&'static [Tool]; 19] = [
        &[Tool::Select],
        &[Tool::Subselect, Tool::Width],
        &[Tool::Bone, Tool::AddBone],
        &[Tool::Camera],
        &[Tool::Pen],
        &[Tool::Rectangle],
        &[Tool::Ellipse],
        &[Tool::Pencil],
        &[Tool::Text],
        &[Tool::RectMarquee, Tool::EllipseMarquee],
        &[Tool::Lasso, Tool::PolygonLasso],
        &[Tool::MagicWand],
        &[Tool::Brush],
        &[Tool::Eraser],
        &[Tool::PaintBucket, Tool::Gradient],
        &[Tool::Smudge, Tool::Blur, Tool::Sharpen],
        &[Tool::Dodge, Tool::Burn],
        &[Tool::CloneStamp],
        &[Tool::Eyedropper],
    ];

    /// Every tool, in toolbar order.
    pub fn all() -> impl Iterator<Item = Tool> {
        Self::GROUPS.into_iter().flatten().copied()
    }

    /// The tools sharing this one's toolbar slot (just itself, for most).
    pub fn group(self) -> &'static [Tool] {
        Self::GROUPS.into_iter().find(|g| g.contains(&self)).expect("every tool is in a group")
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Selection",
            Tool::Subselect => "Subselection",
            Tool::Width => "Width",
            Tool::Pen => "Pen",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Oval",
            Tool::Pencil => "Pencil",
            Tool::Text => "Text",
            Tool::Bone => "Bone",
            Tool::AddBone => "Add Bone",
            Tool::Camera => "Camera",
            Tool::Brush => "Brush",
            Tool::Eraser => "Eraser",
            Tool::Eyedropper => "Eyedropper",
            Tool::PaintBucket => "Paint Bucket",
            Tool::Gradient => "Gradient",
            Tool::Smudge => "Smudge",
            Tool::Blur => "Blur",
            Tool::Sharpen => "Sharpen",
            Tool::Dodge => "Dodge",
            Tool::Burn => "Burn",
            Tool::CloneStamp => "Clone Stamp",
            Tool::RectMarquee => "Rectangular Marquee",
            Tool::EllipseMarquee => "Elliptical Marquee",
            Tool::Lasso => "Lasso",
            Tool::PolygonLasso => "Polygonal Lasso",
            Tool::MagicWand => "Magic Wand",
        }
    }

    /// What the tool does, in a line: for its tooltip and the context bar.
    pub fn description(self) -> &'static str {
        match self {
            Tool::Select => {
                "Click a layer to pick it; drag to move, handles to scale and turn. Double-click to work on its shapes"
            }
            Tool::Subselect => {
                "Drag a shape's points and their handles. Alt-drag a point to pull curve handles out of it"
            }
            Tool::Width => "Drag sideways from a pencil line to make it wider or narrower there",
            Tool::Pen => {
                "Click for corners, drag for curves; click the first point to close. Edits points between paths"
            }
            Tool::Rectangle => "Drag to draw a rectangle; Shift for a square",
            Tool::Ellipse => "Drag to draw an oval; Shift for a circle",
            Tool::Pencil => "Draw freehand lines that follow the pen's pressure",
            Tool::Text => "Click to type on the stage",
            Tool::Bone => "Drag a bone or a rig's layer to pose it; joints above turn to follow",
            Tool::AddBone => "Drag to draw a skeleton's bones; each new one hangs from the last",
            Tool::Camera => "Drag to pan the camera, Shift-drag to turn it, Alt-drag to zoom",
            Tool::Brush => "Paint on a paint layer; Alt-click picks a colour",
            Tool::Eraser => "Rub paint away",
            Tool::Eyedropper => "Click to pick a colour from the stage",
            Tool::PaintBucket => "Fill an area with colour; on drawn lines, fills what they enclose",
            Tool::Gradient => "Drag to lay a gradient across the selection or the canvas",
            Tool::Smudge => "Push paint around, as with a finger",
            Tool::Blur => "Soften the paint under the brush",
            Tool::Sharpen => "Crisp up the paint under the brush",
            Tool::Dodge => "Lighten the paint under the brush",
            Tool::Burn => "Darken the paint under the brush",
            Tool::CloneStamp => "Alt-click where to copy from, then paint a copy elsewhere",
            Tool::RectMarquee => "Drag a rectangle to choose which pixels the paint tools change",
            Tool::EllipseMarquee => "Drag an oval to choose which pixels the paint tools change",
            Tool::Lasso => "Draw round the pixels the paint tools may change",
            Tool::PolygonLasso => "Click corners round the pixels the paint tools may change",
            Tool::MagicWand => "Click an area of one colour to choose it",
        }
    }

    /// Whether the tool selects pixels.
    pub fn selects(self) -> bool {
        matches!(
            self,
            Tool::RectMarquee | Tool::EllipseMarquee | Tool::Lasso | Tool::PolygonLasso | Tool::MagicWand
        )
    }

    /// Whether the tool works on a paint layer's pixels: painting or
    /// selecting them.
    pub fn on_pixels(self) -> bool {
        self.paints() || self.selects()
    }

    /// Whether the tool works on a paint layer's pixels.
    pub fn paints(self) -> bool {
        self.uses_brush() || matches!(self, Tool::PaintBucket | Tool::Gradient | Tool::Eyedropper)
    }

    /// Whether the tool paints strokes with a brush tip (and so has a size,
    /// hardness and so on).
    pub fn uses_brush(self) -> bool {
        matches!(
            self,
            Tool::Brush
                | Tool::Eraser
                | Tool::Smudge
                | Tool::Blur
                | Tool::Sharpen
                | Tool::Dodge
                | Tool::Burn
                | Tool::CloneStamp
        )
    }
}

/// A retouching tool's brush and how strongly it works (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetouchBrush {
    pub brush: BrushSettings,
    pub strength: f32,
}

impl RetouchBrush {
    fn new(size: f32, strength: f32) -> Self {
        Self {
            brush: BrushSettings { size, hardness: 0.3, pressure_size: false, ..BrushSettings::default() },
            strength,
        }
    }
}

/// Ghost images of nearby frames, drawn faintly behind the current frame so
/// animators can judge spacing and motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnionSkin {
    pub enabled: bool,
    pub before: u32,
    pub after: u32,
}

#[derive(Debug, Default)]
pub struct Playback {
    pub playing: bool,
    pub looping: bool,
    /// Time accumulated since the last frame advance, in seconds.
    elapsed: f64,
}

/// One level of "editing inside a symbol", remembered so that leaving it
/// puts the playhead and selection back where they were.
#[derive(Debug, Clone, Copy)]
struct OpenSymbol {
    symbol: SymbolId,
    /// The instance layer it was opened through, for editing in place. `None`
    /// when opened from the library, which shows the symbol on its own.
    instance: Option<LayerId>,
    /// The frame of the timeline outside, when the symbol was opened. That's
    /// where we return to, and also the frame the surrounding scene is
    /// shown at while editing in place.
    return_frame: u32,
    return_selection: Option<LayerId>,
}

/// One level of the scene around a symbol being edited in place, drawn
/// faded behind it. See [`Editor::surroundings`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surrounding {
    pub timeline: TimelineId,
    pub frame: u32,
    /// Maps this timeline's coordinates onto the stage.
    pub to_stage: Affine,
    /// The instance layer that leads to the symbol being edited, which is
    /// drawn at full strength rather than faded, so it's left out here.
    pub skip: LayerId,
}

pub struct Editor {
    pub doc: Document,
    pub history: History,
    /// Where the document was last opened from or saved to.
    pub path: Option<PathBuf>,
    /// What to call the project before it's saved, if not "Untitled".
    untitled_name: Option<String>,
    /// The document as it was last saved, to detect unsaved changes.
    saved: Document,

    pub frame: u32,
    /// The selected layer: the one the properties panel shows and the
    /// transform handles are on. When several layers are selected, this is
    /// the one picked last, and the others are in `also_selected`.
    pub selection: Option<LayerId>,
    /// The other selected layers, if several are. They only count while
    /// `selection` is still the layer they were selected alongside
    /// (`also_selected_with`), so code that simply sets `selection` to pick
    /// one layer ends a multi-selection without having to know about it.
    also_selected: Vec<LayerId>,
    also_selected_with: Option<LayerId>,
    pub tool: Tool,
    /// Fill and stroke given to newly drawn shapes.
    pub draw_style: Style,
    /// Stroke given to pencil lines (which are never filled).
    pub pencil_stroke: Stroke,
    /// The colour the brush paints with.
    pub paint_color: Color,
    /// The brush's and eraser's settings, kept apart so switching between
    /// them doesn't change either, as in Photoshop.
    pub brush: BrushSettings,
    pub eraser: BrushSettings,
    /// How much a stroke's path is smoothed, 0 to 1: it trails behind the
    /// pen, steadying a shaky hand.
    pub smoothing: f32,
    /// The gradient's other colour (Photoshop's background colour).
    pub second_color: Color,
    pub smudge: RetouchBrush,
    pub blur: RetouchBrush,
    pub sharpen: RetouchBrush,
    pub dodge: RetouchBrush,
    pub burn: RetouchBrush,
    /// The Clone Stamp's brush; its strength is the opacity it paints at.
    pub clone: RetouchBrush,
    /// Which tones Dodge and Burn work on most.
    pub tone_range: ToneRange,
    pub bucket: BucketSettings,
    /// How wide a gap the paint bucket closes on a vector layer, in stage
    /// units (see `vector_fill.rs`).
    pub close_gaps: f32,
    /// Draw new shapes into the selected layer, as Animate and Harmony do,
    /// rather than each onto a layer of its own. On by default; turning it
    /// off is remembered between runs (see `prefs.rs`).
    pub draw_into_layer: bool,
    /// Which part of the drawing new shapes go in: the line art, or the
    /// colour art beneath it.
    pub colour_art: bool,
    /// Fade every layer but the selected one, to draw on it (Harmony's
    /// light table).
    pub light_table: bool,
    /// The layer double-clicked into, to pick out its shapes (see
    /// `inside.rs`).
    pub inside: Option<crate::inside::Inside>,
    /// A Distort or Envelope under way on the picked shapes (see
    /// `envelope.rs`).
    pub lattice: Option<crate::envelope::LatticeEdit>,
    /// The bone picked in the skeleton being worked on (see `skeleton.rs`):
    /// the one new bones hang from.
    pub selected_bone: Option<usize>,
    /// The point picked with the Subselection tool (or the Pen), as the
    /// stage last saw it: its layer, its shape in the line art, and its
    /// place in the shape's outline. For pinning it to a bone.
    pub picked_anchor: Option<(LayerId, usize, usize)>,
    /// Rig setup, as Moho's frame 0: skeletons shown at rest, to draw and
    /// paint on and rig, rather than posed (see `skeleton.rs`).
    pub rig_setup: bool,
    /// A smart bone's correction being drawn (see `skeleton.rs`).
    pub correcting: Option<crate::skeleton::Correcting>,
    /// A smart bone being shown at a turn of its own, its actions scrubbed
    /// through on their timeline (see `action_timeline.rs`): its skeleton,
    /// the bone, and the frame it began on.
    pub previewing: Option<(LayerId, usize, u32)>,
    /// Frames picked in the timeline (first and last, either way round),
    /// for the frame commands (see `frame_commands.rs`).
    pub frame_range: Option<(u32, u32)>,
    /// Edit Multiple Frames: dragging on the stage moves layers across the
    /// picked frames (or all of them) at once.
    pub edit_multiple: bool,
    /// View › Rulers, and the guides dragged out of them (see
    /// `stage/rulers.rs`): showing, locked in place, snapped to.
    pub show_rulers: bool,
    pub show_guides: bool,
    pub lock_guides: bool,
    pub snap_to_guides: bool,
    /// Safe areas and the field guide over the stage.
    pub guides: Guides,
    /// Pencil lines follow the pen's pressure.
    pub pencil_pressure: bool,
    /// Pencil lines narrow towards their ends.
    pub pencil_taper: bool,
    pub gradient_shape: GradientShape,
    /// Fade the gradient to transparent rather than to `second_color`.
    pub gradient_to_clear: bool,
    pub gradient_opacity: f32,
    /// Where the Clone Stamp copies from, set by Alt-clicking: the layer,
    /// and the canvas pixel.
    pub clone_source: Option<(LayerId, Point)>,
    /// Once a stroke has started from a source, the distance from where it
    /// paints to where it copies stays the same for later strokes, so a
    /// picture can be copied over in several strokes ("aligned" in
    /// Photoshop). Cleared when a new source is set.
    pub clone_offset: Option<Point>,
    /// The tool last used from each toolbar slot, by the slot's first tool.
    last_in_group: Vec<(Tool, Tool)>,
    /// The selected pixels, on the canvas they belong to. Painting on that
    /// canvas changes only these. Not part of the document (Photoshop
    /// doesn't save selections either), and so not undone.
    pub pixel_selection: Option<(CanvasId, Selection)>,
    /// The Magic Wand's tolerance and whether it selects only connected
    /// pixels.
    pub wand_tolerance: u8,
    pub wand_contiguous: bool,
    pub playback: Playback,
    pub onion: OnionSkin,
    /// The property whose easing curve the inspector is showing and editing.
    pub focused_property: Property,
    /// The document's image files, decoded. Not part of the document (and so
    /// not part of undo): it's a cache, rebuilt from the files on disk.
    pub images: ImageLibrary,
    /// Linked font files that couldn't be loaded, with the reason. Updated
    /// every frame, for the properties panel to explain a substituted font.
    pub font_problems: Vec<(FontFileId, String)>,
    /// The symbols being edited, outermost first. Empty while editing the
    /// main timeline. `frame` and `selection` always refer to the innermost.
    open_symbols: Vec<OpenSymbol>,
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Self {
            history: History::new(&doc),
            saved: doc.clone(),
            doc,
            path: None,
            untitled_name: None,
            frame: 0,
            selection: None,
            also_selected: Vec::new(),
            also_selected_with: None,
            tool: Tool::Select,
            draw_style: Style {
                fill: Some(Color::rgb(0x3d, 0x8b, 0xfd)),
                stroke: Some(Stroke::new(Color::rgb(0x1a, 0x1a, 0x1a), 2.0)),
                gradient: None,
            },
            pencil_stroke: Stroke::new(Color::rgb(0x1a, 0x1a, 0x1a), 4.0),
            paint_color: Color::rgb(0x1a, 0x1a, 0x1a),
            close_gaps: 4.0,
            draw_into_layer: true,
            colour_art: false,
            light_table: false,
            inside: None,
            lattice: None,
            selected_bone: None,
            picked_anchor: None,
            rig_setup: false,
            correcting: None,
            previewing: None,
            frame_range: None,
            edit_multiple: false,
            show_rulers: false,
            show_guides: true,
            lock_guides: false,
            snap_to_guides: true,
            guides: Guides::default(),
            pencil_pressure: true,
            pencil_taper: true,
            brush: BrushSettings::default(),
            eraser: BrushSettings {
                size: 40.0,
                hardness: 0.6,
                pressure_size: false,
                ..BrushSettings::default()
            },
            smoothing: 0.3,
            second_color: Color::WHITE,
            smudge: RetouchBrush::new(40.0, 0.6),
            blur: RetouchBrush::new(40.0, 0.5),
            sharpen: RetouchBrush::new(40.0, 0.5),
            dodge: RetouchBrush::new(60.0, 0.3),
            burn: RetouchBrush::new(60.0, 0.3),
            clone: RetouchBrush::new(50.0, 1.0),
            tone_range: ToneRange::Midtones,
            bucket: BucketSettings::default(),
            gradient_shape: GradientShape::Linear,
            gradient_to_clear: false,
            gradient_opacity: 1.0,
            clone_source: None,
            clone_offset: None,
            last_in_group: Vec::new(),
            pixel_selection: None,
            wand_tolerance: 32,
            wand_contiguous: true,
            playback: Playback { looping: true, ..Playback::default() },
            onion: OnionSkin { enabled: false, before: 2, after: 2 },
            focused_property: Property::Position,
            images: ImageLibrary::default(),
            font_problems: Vec::new(),
            open_symbols: Vec::new(),
        }
    }

    // ---------------------------------------------------------------- files

    /// Work brought back after a crash (see `autosave.rs`): `doc` as the
    /// project at `path`, with its changes still to be saved.
    pub fn recovered(doc: Document, path: Option<PathBuf>) -> Self {
        let mut editor = Self::new(doc);
        editor.path = path;
        // Nothing of it is saved yet. (A recovered copy only exists because
        // it had changes, so it's never the blank document.)
        editor.saved = Document::default();
        editor
    }

    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        let mut editor = Self::new(Document::load(path)?);
        editor.path = Some(path.to_owned());
        Ok(editor)
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), DocumentError> {
        // A correction under way is keyed first, so it's saved whole.
        self.finish_correction();
        self.doc.save(path)?;
        self.saved = self.doc.clone();
        self.path = Some(path.to_owned());
        Ok(())
    }

    /// The folder the project file is in, which audio paths are relative to.
    pub fn project_dir(&self) -> Option<PathBuf> {
        self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf)
    }

    /// Adds a sound file to the main timeline, starting on the current frame
    /// (or frame 1 while editing inside a symbol).
    pub fn import_audio(&mut self, path: &Path) {
        let start = if self.timeline() == TimelineId::Main { self.frame } else { 0 };
        let project_dir = self.project_dir();
        self.doc.add_audio(path, project_dir.as_deref(), start);
    }

    /// Adds a Photoshop file's layers as paint layers, from the current
    /// frame, and selects the top one. Unlike an image, the pixels are
    /// copied into the project, so they can be painted on.
    pub fn import_psd(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let psd = inkframe_core::Psd::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let name =
            path.file_stem().map_or_else(|| "Photoshop".to_owned(), |s| s.to_string_lossy().into_owned());
        let added = self.doc.import_psd(&psd, self.timeline(), self.frame, &name);
        self.select(added.last().copied());
        Ok(())
    }

    /// Writes the current frame as a Photoshop file (see
    /// `inkframe_render::export_psd`).
    pub fn export_psd(&self, path: &Path) -> Result<(), String> {
        let psd = inkframe_render::export_psd(&self.doc, self.frame, Some(&self.images));
        std::fs::write(path, psd.to_bytes()).map_err(|e| e.to_string())
    }

    /// Places an image file on a new layer, on the current frame, in the
    /// middle of the stage, at its own size in pixels (or shrunk to fit the
    /// stage, so a 6000-pixel photo doesn't land as a giant).
    ///
    /// A Photoshop file comes in as paint layers instead, one per layer
    /// (see [`Editor::import_psd`]).
    pub fn import_image(&mut self, path: &Path) -> Result<(), String> {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd")) {
            return self.import_psd(path);
        }
        let image = self.add_library_image(path)?;
        self.place_asset_at(Asset::Image(image), self.doc.stage_rect().center());
        Ok(())
    }

    /// Adds an image file to the Library without putting it on the stage,
    /// as dropping it onto the Library does.
    pub fn add_library_image(&mut self, path: &Path) -> Result<ImageId, String> {
        let size = inkframe_render::image_size(path)?;
        let project_dir = self.project_dir();
        Ok(self.doc.add_image(path, project_dir.as_deref(), size))
    }

    /// Puts a Library asset on the stage at `point` (in stage coordinates)
    /// on the current frame: a symbol or image on a new layer, selected; a
    /// sound as another copy on the timeline, starting here. Returns the
    /// new layer, if there is one.
    pub fn place_asset_at(&mut self, asset: Asset, point: Point) -> Option<LayerId> {
        // In the coordinates of the timeline being edited, which may be a
        // turned or scaled instance's.
        let at = self.timeline_transform().inverse().map_or(point, |to_timeline| to_timeline.apply(point));
        let layer = match asset {
            Asset::Symbol(symbol) => {
                if !self.can_place(symbol) {
                    return None;
                }
                self.doc.add_instance_layer(self.timeline(), symbol, self.frame, at)?
            }
            Asset::Image(image) => {
                let asset = self.doc.image(image)?;
                let (width, height) = (asset.width as f32, asset.height as f32);
                // Big pictures are fitted to the stage, as when imported.
                let fit = (self.doc.width as f32 / width).min(self.doc.height as f32 / height).min(1.0);
                let element = Element {
                    shape: Shape::Image { image, width: width * fit, height: height * fit },
                    style: Style { fill: None, stroke: None, gradient: None },
                };
                let name = asset.name.clone();
                let taken = self.layers().iter().any(|l| l.name == name);
                let name = if taken { self.doc.next_layer_name_in(self.timeline(), &name) } else { name };
                self.add_named_layer(name, element, at);
                self.selection?
            }
            Asset::Sound(sound) => {
                // Sound plays against the main timeline only.
                let start = if self.timeline() == TimelineId::Main { self.frame } else { 0 };
                let track = self.doc.audio_track(sound)?.clone();
                let project_dir = self.project_dir();
                let copy =
                    self.doc.add_audio(&track.resolve(project_dir.as_deref()), project_dir.as_deref(), start);
                if let Some(added) = self.doc.audio_track_mut(copy) {
                    added.name = track.name;
                    added.volume = track.volume;
                    added.folder = track.folder;
                }
                return None;
            }
        };
        self.select(Some(layer));
        Some(layer)
    }

    /// Puts a Library asset where it was dropped on the timeline: from
    /// `frame`, mid-stage, on a new layer just above `above` (or on top).
    pub fn place_asset_on_timeline(&mut self, asset: Asset, frame: u32, above: Option<LayerId>) {
        self.set_frame(frame.min(self.last_frame()));
        let Some(layer) = self.place_asset_at(asset, self.doc.stage_rect().center()) else { return };
        if let Some(index) = above.and_then(|id| self.layers().iter().position(|l| l.id == id)) {
            let folder = self.layers()[index].in_folder;
            self.doc.move_layer_to(layer, index + 1);
            if let Some(placed) = self.doc.layer_mut(layer) {
                placed.in_folder = folder;
            }
        }
    }

    /// Links a font file to the project so its families can be used, and
    /// sets the selected text in it, since that's usually why it was
    /// imported. Returns the families the file provides.
    pub fn import_font(&mut self, path: &Path) -> Result<Vec<String>, String> {
        let families = inkframe_render::add_font_file(path)?;
        let project_dir = self.project_dir();
        self.doc.add_font_file(path, project_dir.as_deref(), families.clone());
        let frame = self.frame;
        if let Some(layer) = self.selection.and_then(|id| self.doc.layer_mut(id)) {
            for element in layer.elements_at_mut(frame) {
                if let Shape::Text(text) = &mut element.shape {
                    text.font.family = families[0].clone();
                }
            }
        }
        Ok(families)
    }

    /// Starts a new, empty text layer with its first line's baseline at
    /// `at`, ready to be typed into, and returns it.
    pub fn add_text(&mut self, at: Point) -> Option<LayerId> {
        let element = Element {
            shape: Shape::Text(Text::new("", 48.0)),
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let name = self.doc.next_layer_name_in(self.timeline(), "Text");
        self.add_named_layer(name, element, at);
        self.remeasure_text();
        self.selection
    }

    /// Shape `index` of the drawing `layer` shows on the current frame.
    pub fn element(&self, layer: LayerId, index: usize) -> Option<&Element> {
        self.doc.layer(layer)?.elements_at(self.frame).get(index)
    }

    pub fn element_mut(&mut self, layer: LayerId, index: usize) -> Option<&mut Element> {
        let frame = self.frame;
        self.doc.layer_mut(layer)?.elements_at_mut(frame).get_mut(index)
    }

    /// Brings every text shape's stored size up to date with its layout.
    ///
    /// The document model can't lay out text (that needs fonts), so it
    /// relies on these stored sizes for selection and hit-testing. Laying
    /// out is cached, so calling this every frame is cheap; it catches every
    /// way text can change: typing, undo, a file opened on a machine with
    /// different fonts.
    pub fn remeasure_text(&mut self) {
        let doc = &mut self.doc;
        let layers = doc.layers.iter_mut().chain(doc.symbols.iter_mut().flat_map(|s| s.layers.iter_mut()));
        for element in layers.flat_map(|layer| layer.all_elements_mut()) {
            if let Shape::Text(text) = &mut element.shape {
                let bounds = inkframe_render::layout_text(text).bounds;
                if text.measured != bounds {
                    text.measured = bounds;
                }
            }
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.doc != self.saved
    }

    /// File name for the title bar and export defaults.
    pub fn display_name(&self) -> String {
        match self.path.as_deref().and_then(Path::file_stem) {
            Some(stem) => stem.to_string_lossy().into_owned(),
            None => self.untitled_name.clone().unwrap_or_else(|| "Untitled".to_owned()),
        }
    }

    /// A copy of a project that isn't saved anywhere yet, called `name`
    /// until it is (an example opened from File › Open Example).
    pub fn untitled_copy(doc: Document, name: String) -> Self {
        let mut editor = Self::new(doc);
        editor.untitled_name = Some(name);
        editor
    }

    // -------------------------------------------------------------- history

    /// Records an undo step if the document changed. Call once per UI frame,
    /// but only when no drag is in progress, so a whole gesture becomes one step.
    pub fn commit_history(&mut self) {
        self.history.commit(&self.doc);
    }

    pub fn undo(&mut self) {
        if self.history.undo(&mut self.doc) {
            self.after_document_replaced();
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.doc) {
            self.after_document_replaced();
        }
    }

    /// Undoes or redoes several steps at once (the History panel).
    pub fn jump_history(&mut self, steps: isize) {
        self.history.commit(&self.doc);
        if self.history.jump(&mut self.doc, steps) {
            self.after_document_replaced();
        }
    }

    // -------------------------------------------------------------- align

    pub fn align(&mut self, how: inkframe_core::Align, to_stage: bool) {
        let ids = self.selected_ids();
        self.doc.align_layers(&ids, self.frame, how, to_stage);
    }

    pub fn distribute(&mut self, axis: inkframe_core::Axis, to_stage: bool) {
        let ids = self.selected_ids();
        self.doc.distribute_layers(&ids, self.frame, axis, to_stage);
    }

    pub fn match_size(&mut self, axis: inkframe_core::Axis, to_stage: bool) {
        let ids = self.selected_ids();
        self.doc.match_size(&ids, self.frame, axis, to_stage);
    }

    /// Uses a colour picked from the Swatches panel: for the paint tools,
    /// the paint colour; otherwise the fill (or stroke) for new shapes and,
    /// as in Animate, for the shapes of the selected layers on this frame.
    pub fn use_color(&mut self, color: Color, stroke: bool) {
        if self.tool.paints() {
            self.paint_color = color;
            return;
        }
        if self.tool == Tool::Pencil {
            self.pencil_stroke.color = color;
            return;
        }
        let width = self.draw_style.stroke.map_or(2.0, |s| s.width);
        if stroke {
            self.draw_style.stroke = Some(Stroke::new(color, width));
        } else {
            self.draw_style.fill = Some(color);
        }
        let frame = self.frame;
        for id in self.selected_ids() {
            let Some(layer) = self.doc.layer_mut(id).filter(|l| !l.locked) else { continue };
            for element in layer.elements_at_mut(frame) {
                if matches!(element.shape, Shape::Paint { .. } | Shape::Image { .. }) {
                    continue;
                }
                if stroke {
                    let width = element.style.stroke.map_or(width, |s| s.width);
                    element.style.stroke = Some(Stroke::new(color, width));
                } else {
                    element.style.fill = Some(color);
                }
            }
        }
    }

    /// Undo can remove layers or shorten the animation; make sure the editor
    /// isn't left pointing at things that no longer exist.
    fn after_document_replaced(&mut self) {
        // Undo or redo puts back a document from before or after the
        // correction began, with its drawing and action as they were then.
        self.correcting = None;
        // A document put back is a copy, which never keeps a preview.
        self.previewing = None;
        // The lattice was bending shapes that may no longer be there as
        // they were.
        self.lattice = None;
        // Leave any symbol that no longer exists (undoing its creation, say).
        if let Some(gone) = self.open_symbols.iter().position(|o| self.doc.symbol(o.symbol).is_none()) {
            self.exit_to(gone);
        }
        let timeline = self.timeline();
        if self.selection.is_some_and(|id| self.doc.timeline_of(id) != Some(timeline)) {
            self.selection = None;
        }
        self.frame = self.frame.min(self.last_frame());
    }

    // ------------------------------------------------------- symbol editing

    /// The timeline being edited: the main one, or the innermost open symbol.
    pub fn timeline(&self) -> TimelineId {
        self.open_symbols.last().map_or(TimelineId::Main, |o| TimelineId::Symbol(o.symbol))
    }

    /// The symbols being edited, outermost first, for the breadcrumb bar.
    pub fn open_symbols(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.open_symbols.iter().map(|o| o.symbol)
    }

    /// The layers of the timeline being edited.
    pub fn layers(&self) -> &[Layer] {
        self.doc.layers_of(self.timeline())
    }

    /// The length of the timeline being edited.
    pub fn duration(&self) -> u32 {
        self.doc.duration_of(self.timeline())
    }

    pub fn last_frame(&self) -> u32 {
        self.duration().saturating_sub(1)
    }

    /// Maps the coordinates of the timeline being edited onto the stage.
    ///
    /// On the main timeline that's the identity. A symbol opened through an
    /// instance sits exactly where that instance does: the product of the
    /// instance transforms on the way in, each taken at the frame its
    /// timeline was on when we stepped inside. A symbol opened from the
    /// library has no instance to follow, so its registration point is put
    /// in the middle of the stage instead.
    pub fn timeline_transform(&self) -> Affine {
        let mut to_stage = Affine::IDENTITY;
        for open in &self.open_symbols {
            let instance = open.instance.and_then(|id| self.doc.layer(id));
            to_stage = match instance {
                Some(layer) => to_stage.pre_concat(self.doc.layer_matrix(layer, open.return_frame)),
                None => Affine::translate(self.doc.stage_rect().center()),
            };
        }
        to_stage
    }

    /// The scene around a symbol being edited in place, outermost first,
    /// for drawing faded behind it. Empty on the main timeline and for a
    /// symbol opened from the library.
    pub fn surroundings(&self) -> Vec<Surrounding> {
        let mut levels = Vec::new();
        let mut timeline = TimelineId::Main;
        let mut to_stage = Affine::IDENTITY;
        for open in &self.open_symbols {
            match open.instance.and_then(|id| self.doc.layer(id)) {
                Some(layer) => {
                    levels.push(Surrounding { timeline, frame: open.return_frame, to_stage, skip: layer.id });
                    to_stage = to_stage.pre_concat(self.doc.layer_matrix(layer, open.return_frame));
                }
                // Opened on its own: nothing from further out is shown.
                None => {
                    levels.clear();
                    to_stage = Affine::translate(self.doc.stage_rect().center());
                }
            }
            timeline = TimelineId::Symbol(open.symbol);
        }
        levels
    }

    /// Opens `symbol` on its own for editing, starting on `frame` of its
    /// timeline.
    pub fn enter_symbol(&mut self, symbol: SymbolId, frame: u32) {
        self.open_symbol(symbol, frame, None);
    }

    fn open_symbol(&mut self, symbol: SymbolId, frame: u32, instance: Option<LayerId>) {
        if self.doc.symbol(symbol).is_none() {
            return;
        }
        // Already open further out? Go back to that level rather than
        // stacking the same symbol twice.
        if let Some(depth) = self.open_symbols.iter().position(|o| o.symbol == symbol) {
            self.exit_to(depth + 1);
            self.set_frame(frame);
            return;
        }
        self.open_symbols.push(OpenSymbol {
            symbol,
            instance,
            return_frame: self.frame,
            return_selection: self.selection,
        });
        self.selection = None;
        self.playback.playing = false;
        self.set_frame(frame);
    }

    /// Opens the symbol an instance layer shows, in place: where the
    /// instance sits, on the frame it's showing right now, with the rest of
    /// the scene faded around it.
    pub fn enter_instance(&mut self, id: LayerId) {
        let Some(layer) = self.doc.layer(id) else { return };
        let Some((symbol, frame)) = self.doc.instance_view(layer, self.frame, 0).map(|(s, f)| (s.id, f))
        else {
            return;
        };
        self.open_symbol(symbol, frame, Some(id));
    }

    /// Steps out one level, back to the timeline that contains this symbol.
    pub fn exit_symbol(&mut self) {
        if !self.open_symbols.is_empty() {
            self.exit_to(self.open_symbols.len() - 1);
        }
    }

    /// Leaves symbols until only the first `depth` remain open (0 returns to
    /// the main timeline), restoring the frame and selection from back then.
    pub fn exit_to(&mut self, depth: usize) {
        if depth >= self.open_symbols.len() {
            return;
        }
        let restored = self.open_symbols[depth];
        self.open_symbols.truncate(depth);
        self.playback.playing = false;
        self.selection = restored.return_selection.filter(|id| self.doc.layer(*id).is_some());
        self.set_frame(restored.return_frame);
    }

    /// The front-most layer of the current timeline under `point`, which is
    /// in the timeline's own coordinates.
    /// The layer under `point` on the stage. Masks aren't drawn, so clicks
    /// go through them, except for a selected mask, which can be dragged
    /// into place.
    pub fn hit_test(&self, point: Point, tolerance: f32) -> Option<LayerId> {
        if let Some(mask) = self.selected_layer().filter(|l| l.mask && !l.locked && l.is_shown_at(self.frame))
            && self.doc.hits_layer(mask, point, self.frame, tolerance)
        {
            return Some(mask.id);
        }
        self.doc.hit_test_in(self.timeline(), point, self.frame, tolerance)
    }

    /// Puts something just drawn, centered on `position`, onto the stage.
    ///
    /// Usually that's a new layer in the timeline being edited, appearing
    /// from the current frame onward, as in Animate. But when the selected
    /// layer swaps between drawings (a mouth, a hand), the shape is added to
    /// the drawing it shows on this frame instead: that's how a new, blank
    /// drawing gets drawn into.
    pub fn add_drawn_shape(&mut self, prefix: &str, element: Element, position: Point) {
        if self.add_to_shown_drawing(&element, position) {
            return;
        }
        let name = self.doc.next_layer_name_in(self.timeline(), prefix);
        self.add_named_layer(name, element, position);
    }

    /// Adds `element` to the selected layer's showing drawing, if it swaps
    /// drawings. The shape becomes a path in the layer's own coordinates,
    /// since a turned or scaled layer would otherwise turn or scale it too.
    fn add_to_shown_drawing(&mut self, element: &Element, position: Point) -> bool {
        let frame = self.frame;
        let into = self.draw_into_layer;
        let Some(layer) = self.selected_layer().filter(|l| {
            (l.drawings.len() > 1 || into) && l.draws_vectors() && !l.locked && l.is_shown_at(frame)
        }) else {
            return false;
        };
        let to_local = self.doc.layer_matrix(layer, frame).inverse();
        let Some(to_local) = to_local.map(|m| m.pre_concat(Affine::translate(position))) else {
            return false;
        };
        // The shape's points (and a pencil line's widths) mapped into the
        // layer's coordinates; text and pictures can't be, so they get a
        // layer of their own.
        let Some(mut element) = element.transformed(to_local) else { return false };
        // On a frame where a skeleton bends the layer, what's drawn goes in
        // where it bends to what was drawn, so it shows just as drawn.
        if let Some(bend) = self.doc.layer_bend(layer, frame) {
            let area = unbend_area(layer, frame, &element);
            element = element.mapped(|p| bend.unbend(p, area).unwrap_or(p)).unwrap_or(element);
        }
        let id = layer.id;
        self.push_into_drawing(id, element)
    }

    /// Adds `element` to the drawing `layer` shows now: its line art, or
    /// its colour art if that's what's being drawn on.
    fn push_into_drawing(&mut self, layer: LayerId, element: Element) -> bool {
        let (frame, colour_art) = (self.frame, self.colour_art);
        let Some(layer) = self.doc.layer_mut(layer) else { return false };
        let index = layer.drawing_index_at(frame);
        let drawing = &mut layer.drawings[index];
        if colour_art {
            drawing.colour_art.push(element)
        } else {
            drawing.elements.push(element)
        }
        true
    }

    /// Fills the area enclosed by a vector layer's line art at `point` (on
    /// the stage) with the paint colour, or recolours the fill there. See
    /// `vector_fill.rs`.
    pub fn fill_vector(&mut self, id: LayerId, point: Point) -> inkframe_core::FillResult {
        let frame = self.frame;
        let (color, gap) = (self.paint_color, self.close_gaps);
        let local = self.doc.layer(id).and_then(|l| {
            let local = self.doc.layer_matrix(l, frame).inverse()?.apply(point);
            // On a bent frame, the fill goes where it's seen.
            Some(match self.doc.layer_bend(l, frame) {
                Some(bend) => {
                    let area =
                        l.elements_at(frame).iter().map(Element::bounds).reduce(Rect::union)?.expand(64.0);
                    bend.unbend(local, area).unwrap_or(local)
                }
                None => local,
            })
        });
        match (local, self.doc.layer_mut(id)) {
            (Some(local), Some(layer)) => layer.fill_enclosed(frame, local, color, gap),
            _ => inkframe_core::FillResult::NotEnclosed,
        }
    }

    /// Adds a layer holding `element` to the timeline being edited, starting
    /// on the current frame, and selects it.
    fn add_named_layer(&mut self, name: String, element: Element, position: Point) {
        let timeline = self.timeline();
        let id = self.doc.add_layer_to(timeline, name, vec![element], self.frame, Transform::at(position));
        if let Some(layer) = id.and_then(|id| self.doc.layer_mut(id)) {
            layer.start = self.frame;
        }
        self.selection = id;
    }

    // ------------------------------------------------------------- painting

    /// The canvas the brush paints on: the selected layer's, if it's a
    /// paint layer that can be painted on now, with the canvas's size.
    pub fn paint_target(&self) -> Option<(LayerId, CanvasId, u32, u32)> {
        let layer = self.selected_layer().filter(|l| !l.locked && l.is_shown_at(self.frame))?;
        layer.elements_at(self.frame).iter().find_map(|e| match e.shape {
            Shape::Paint { canvas, width, height } => Some((layer.id, canvas, width, height)),
            _ => None,
        })
    }

    /// The selection on `canvas`, if there's one (on another canvas doesn't
    /// count).
    pub fn selection_on(&self, canvas: CanvasId) -> Option<&Selection> {
        self.pixel_selection.as_ref().filter(|(on, _)| *on == canvas).map(|(_, s)| s)
    }

    /// The selection on the paint layer being painted, if there's one.
    pub fn active_selection(&self) -> Option<(CanvasId, &Selection)> {
        let (_, canvas, ..) = self.paint_target()?;
        self.selection_on(canvas).map(|s| (canvas, s))
    }

    /// Select › All: the whole of the selected paint layer's canvas.
    pub fn select_all_pixels(&mut self) {
        if let Some((_, canvas, width, height)) = self.paint_target() {
            self.pixel_selection = Some((canvas, Selection::all(width, height)));
        }
    }

    pub fn deselect_pixels(&mut self) {
        self.pixel_selection = None;
    }

    /// Select › Inverse.
    pub fn invert_pixel_selection(&mut self) {
        let Some((_, canvas, width, height)) = self.paint_target() else { return };
        let inverted = match self.selection_on(canvas) {
            Some(selection) => selection.inverted(),
            None => Selection::all(width, height),
        };
        self.pixel_selection = Some((canvas, inverted));
    }

    /// Clears the selected pixels (Delete with a selection).
    pub fn clear_selected_pixels(&mut self) -> bool {
        let Some((canvas, selection)) = self.active_selection().map(|(c, s)| (c, s.clone())) else {
            return false;
        };
        if let Some(pixels) = self.doc.canvas_mut(canvas) {
            *pixels = selection.clear(pixels);
        }
        true
    }

    /// Layer › New › Layer via Copy (or Cut): the selected pixels onto a
    /// layer of their own, which is then selected.
    pub fn layer_via_copy(&mut self, cut: bool) {
        let frame = self.frame;
        let Some((layer, ..)) = self.paint_target() else { return };
        let Some((_, selection)) = self.active_selection().map(|(c, s)| (c, s.clone())) else { return };
        if let Some(new) = self.doc.layer_via_copy(layer, frame, &selection, cut) {
            self.select(Some(new));
            self.pixel_selection = None;
        }
    }

    /// Merge Down: paints the selected layer into the paint layer beneath.
    pub fn merge_down(&mut self) -> bool {
        let Some(id) = self.selection else { return false };
        let below = self.doc.timeline_of(id).and_then(|t| {
            let layers = self.doc.layers_of(t);
            let index = layers.iter().position(|l| l.id == id)?;
            index.checked_sub(1).map(|i| layers[i].id)
        });
        if inkframe_render::merge_down(&mut self.doc, id, self.frame, Some(&self.images)) {
            self.select(below);
            return true;
        }
        false
    }

    /// Makes the current tool's brush `factor` times bigger.
    pub fn resize_brush(&mut self, factor: f32) {
        if let Some(settings) = self.brush_of(self.tool) {
            settings.size = (settings.size * factor).clamp(1.0, 500.0);
        }
    }

    /// A copy of the brush a tool paints with, if it paints with one.
    pub fn brush_for(&self, tool: Tool) -> Option<BrushSettings> {
        Some(match tool {
            Tool::Brush => self.brush,
            Tool::Eraser => self.eraser,
            Tool::Smudge => self.smudge.brush,
            Tool::Blur => self.blur.brush,
            Tool::Sharpen => self.sharpen.brush,
            Tool::Dodge => self.dodge.brush,
            Tool::Burn => self.burn.brush,
            Tool::CloneStamp => self.clone.brush,
            _ => return None,
        })
    }

    /// The brush a tool paints with, if it paints with one.
    pub fn brush_of(&mut self, tool: Tool) -> Option<&mut BrushSettings> {
        Some(match tool {
            Tool::Brush => &mut self.brush,
            Tool::Eraser => &mut self.eraser,
            _ => &mut self.retouch_of(tool)?.brush,
        })
    }

    pub fn retouch_of(&mut self, tool: Tool) -> Option<&mut RetouchBrush> {
        Some(match tool {
            Tool::Smudge => &mut self.smudge,
            Tool::Blur => &mut self.blur,
            Tool::Sharpen => &mut self.sharpen,
            Tool::Dodge => &mut self.dodge,
            Tool::Burn => &mut self.burn,
            Tool::CloneStamp => &mut self.clone,
            _ => return None,
        })
    }

    /// Switches to `tool`, remembering it as its toolbar slot's choice.
    pub fn use_tool(&mut self, tool: Tool) {
        let slot = tool.group()[0];
        self.last_in_group.retain(|(s, _)| *s != slot);
        self.last_in_group.push((slot, tool));
        self.tool = tool;
    }

    /// The tool a toolbar slot shows: the one last used from it.
    pub fn slot_choice(&self, group: &[Tool]) -> Tool {
        self.last_in_group.iter().find(|(slot, _)| *slot == group[0]).map_or(group[0], |(_, tool)| *tool)
    }

    /// A toolbar slot's key: picks its tool, or with `cycle` (Shift) steps
    /// to the next one in the slot.
    pub fn use_slot(&mut self, group: &[Tool], cycle: bool) {
        let choice = self.slot_choice(group);
        let tool = if cycle && group.contains(&self.tool) {
            let i = group.iter().position(|t| *t == self.tool).unwrap_or(0);
            group[(i + 1) % group.len()]
        } else {
            choice
        };
        self.use_tool(tool);
    }

    /// Adds a paint layer on top of the timeline being edited, from the
    /// current frame, and selects it.
    pub fn add_paint_layer(&mut self) -> Option<LayerId> {
        let timeline = self.timeline();
        let name = self.doc.next_layer_name_in(timeline, "Paint");
        let id = self.doc.add_paint_layer(timeline, name, self.frame)?;
        self.select(Some(id));
        Some(id)
    }

    /// Sets a layer's depth on the current frame, keying it if it changed.
    pub fn set_depth(&mut self, id: LayerId, depth: f32) {
        let frame = self.frame;
        if let Some(layer) = self.doc.layer_mut(id) {
            layer.track.set_depth(frame, depth);
        }
    }

    /// Adds an empty layer just above the selected one (in the same folder),
    /// or on top if nothing is selected, and selects it, ready to draw
    /// into: Animate's New Layer. Its origin is the middle of the stage, so
    /// it turns and scales around there until its pivot is moved.
    pub fn add_empty_layer(&mut self) -> Option<LayerId> {
        let timeline = self.timeline();
        let name = self.doc.next_layer_name_in(timeline, "Layer");
        let above = self.selection.and_then(|id| {
            let index = self.layers().iter().position(|l| l.id == id)?;
            Some((index, self.layers()[index].in_folder))
        });
        let middle = self.doc.stage_rect().center();
        let id = self.doc.add_layer_to(timeline, name, Vec::new(), 0, Transform::at(middle))?;
        // A folder sits just above what's in it, so one place up from the
        // selected row is outside a selected folder but inside the folder
        // a selected layer is in.
        if let Some((index, folder)) = above {
            self.doc.move_layer_to(id, index + 1);
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.in_folder = folder;
            }
        }
        self.select(Some(id));
        Some(id)
    }

    /// Adds a particle layer from `preset`, starting on the current frame:
    /// snow along the top of the stage, the others in the middle.
    pub fn add_particle_layer(&mut self, preset: inkframe_core::Preset) -> Option<LayerId> {
        let stage = self.doc.stage_rect();
        let at = match preset {
            inkframe_core::Preset::Snow => inkframe_core::Point::new(stage.center().x, stage.min.y - 40.0),
            _ => stage.center(),
        };
        let id = self.doc.add_particle_layer(preset, self.frame, at)?;
        self.select(Some(id));
        Some(id)
    }

    // ------------------------------------------------------------- drawings

    /// Shows the selected layer's drawing `index` from the current frame on.
    pub fn show_drawing(&mut self, index: usize) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            layer.expose(frame, index);
        }
    }

    /// Swaps the selected layer to its next (`delta` 1) or previous (−1)
    /// drawing from the current frame on, wrapping around: flipping through
    /// mouth shapes until the right one shows.
    pub fn cycle_drawing(&mut self, delta: isize) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            let count = layer.drawings.len() as isize;
            let next = (layer.drawing_index_at(frame) as isize + delta).rem_euclid(count);
            layer.expose(frame, next as usize);
        }
    }

    /// Adds a drawing to the selected layer and shows it from the current
    /// frame on: a blank one, or (`copy`) a copy of the one showing now, to
    /// change a little.
    pub fn new_drawing(&mut self, copy: bool) {
        if let Some(id) = self.selection {
            self.new_drawing_on(id, self.frame, copy);
        }
    }

    /// Adds a drawing to `id` and shows it from `frame` on (see
    /// [`Editor::new_drawing`]).
    pub fn new_drawing_on(&mut self, id: LayerId, frame: u32, copy: bool) {
        if self.doc.layer(id).is_none_or(|l| l.instance.is_some() || l.folder || l.camera) {
            return;
        }
        let Some(mut elements) = self.doc.layer(id).map(|l| l.elements_at(frame).to_vec()) else { return };
        // A paint layer's new drawing is a canvas of its own: a copy of the
        // one showing, or a blank one. Other layers' blank drawings are empty.
        let paints = elements.iter().any(|e| matches!(e.shape, Shape::Paint { .. }));
        if paints {
            elements.retain(|e| matches!(e.shape, Shape::Paint { .. }) || copy);
            self.doc.separate_canvases(&mut elements, !copy);
        } else if !copy {
            elements.clear();
        }
        if let Some(layer) = self.doc.layer_mut(id) {
            let mut drawing = Drawing::new(layer.next_drawing_name(), elements);
            // A copy keeps its shape hints, so a shape tween between the
            // two lines up from the start.
            if copy {
                let showing = &layer.drawings[layer.drawing_index_at(frame)];
                drawing.hints = showing.hints.clone();
                drawing.colour_art = showing.colour_art.clone();
            }
            let index = layer.add_drawing(drawing);
            layer.expose(frame, index);
        }
    }

    /// Adds a camera to the main timeline (or finds the one there is) and
    /// selects it.
    pub fn add_camera(&mut self) {
        let camera = self.doc.add_camera();
        self.select(Some(camera));
    }

    // --------------------------------------------------------- shape tweens

    /// Whether the selected layer morphs through the current frame's span:
    /// `None` if it can't, there being no later swap to morph into.
    pub fn shape_tween_here(&self) -> Option<bool> {
        let layer = self.selected_layer()?;
        layer.exposure.iter().find(|key| key.frame > self.frame)?;
        Some(layer.shape_tween_at(self.frame).is_some())
    }

    /// Makes the selected layer morph (or stop morphing) through the
    /// current frame's span into the next swap, as Animate's Create Shape
    /// Tween.
    /// Draws the inbetweens for the span the current frame is in on the
    /// selected layer (see `Layer::make_inbetweens`). Returns how many.
    pub fn make_inbetweens(&mut self) -> usize {
        let frame = self.frame;
        self.selected_layer_mut().filter(|l| !l.locked).map_or(0, |layer| layer.make_inbetweens(frame))
    }

    pub fn set_shape_tween(&mut self, on: bool) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            layer.set_shape_tween(frame, on);
        }
    }

    /// The drawings a shape tween through the current frame runs between:
    /// (from, to), as indices into the selected layer's drawings.
    pub fn shape_tween_drawings(&self) -> Option<(usize, usize)> {
        let layer = self.selected_layer()?;
        // On a swap, the tween that ends there (the drawing showing is the
        // one it morphs into), or else the one that starts there.
        let on_swap = layer.exposure.iter().any(|k| k.frame == self.frame);
        let ending =
            on_swap.then(|| self.frame.checked_sub(1)).flatten().and_then(|f| layer.shape_tween_at(f));
        let (start, end, _) = ending.or_else(|| layer.shape_tween_at(self.frame))?;
        Some((layer.drawing_index_at(start), layer.drawing_index_at(end)))
    }

    /// Adds a shape hint to both drawings of the shape tween here, on the
    /// outline of each drawing's first shape, ready to be dragged into
    /// place. Each new hint goes a little further round than the last, from
    /// the upper left, so they don't pile up on one spot.
    pub fn add_shape_hint(&mut self) {
        let Some((from, to)) = self.shape_tween_drawings() else { return };
        let Some(layer) = self.selected_layer_mut() else { return };
        for index in [from, to] {
            let drawing = &mut layer.drawings[index];
            let Some(first) = drawing.elements.first() else { continue };
            let bounds = first.shape.bounds();
            let (anchors, closed) = first.shape.to_anchors();
            let angle = -2.4 + 0.7 * drawing.hints.len() as f32;
            let reach = bounds.width().max(bounds.height());
            let toward = bounds.center() + Point::new(angle.cos(), angle.sin()) * reach;
            let on_outline =
                inkframe_core::nearest_on_anchors(&anchors, closed, toward).map_or(toward, |hit| hit.point);
            drawing.hints.push(on_outline);
        }
    }

    /// Removes every shape hint from both drawings of the shape tween here.
    pub fn clear_shape_hints(&mut self) {
        let Some((from, to)) = self.shape_tween_drawings() else { return };
        if let Some(layer) = self.selected_layer_mut() {
            layer.drawings[from].hints.clear();
            layer.drawings[to].hints.clear();
        }
    }

    /// Removes the drawing the selected layer shows on the current frame,
    /// unless it's the layer's only one.
    pub fn delete_shown_drawing(&mut self) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            let index = layer.drawing_index_at(frame);
            layer.remove_drawing(index);
        }
    }

    // ------------------------------------------------------------- timeline

    pub fn set_frame(&mut self, frame: u32) {
        self.frame = frame.min(self.last_frame());
    }

    pub fn step(&mut self, delta: i64) {
        let frame = (self.frame as i64 + delta).clamp(0, self.last_frame() as i64);
        self.frame = frame as u32;
    }

    pub fn toggle_playback(&mut self) {
        self.playback.playing = !self.playback.playing;
        self.playback.elapsed = 0.0;
        // Pressing play on the last frame restarts from the top rather than
        // doing nothing, which is what every video player does.
        if self.playback.playing && self.frame == self.last_frame() {
            self.frame = 0;
        }
    }

    /// Advances playback by `dt` seconds of wall-clock time.
    ///
    /// The UI may repaint faster or slower than the animation's frame rate,
    /// so we accumulate real time and step a whole frame each time a frame's
    /// worth has passed. If the UI stalls, several frames are skipped at once,
    /// which keeps playback in sync with the clock instead of slowing down.
    pub fn advance_playback(&mut self, dt: f64) {
        if !self.playback.playing {
            return;
        }
        let fps = self.doc.frame_rate.fps();
        self.playback.elapsed += dt;
        // Count whole frames by multiplying rather than repeatedly
        // subtracting 1/fps: the subtraction accumulates rounding error and
        // can come up a frame short. The epsilon absorbs the last bit of
        // floating-point noise when time lands exactly on a frame boundary.
        let frames = (self.playback.elapsed * fps + 1e-9).floor();
        self.playback.elapsed = (self.playback.elapsed - frames / fps).max(0.0);
        for _ in 0..frames as u64 {
            if self.frame < self.last_frame() {
                self.frame += 1;
            } else if self.playback.looping {
                self.frame = 0;
            } else {
                self.playback.playing = false;
                break;
            }
        }
    }

    // --------------------------------------------------------------- layers

    pub fn selected_layer(&self) -> Option<&Layer> {
        self.selection.and_then(|id| self.doc.layer(id))
    }

    /// Every selected layer that still exists, bottom-most first (the order
    /// layers are stacked in, which is how they're copied and deleted).
    pub fn selected_ids(&self) -> Vec<LayerId> {
        let Some(primary) = self.selection else { return Vec::new() };
        let others = if self.also_selected_with == Some(primary) { &self.also_selected[..] } else { &[] };
        let timeline = self.timeline();
        self.doc
            .layers_of(timeline)
            .iter()
            .map(|l| l.id)
            .filter(|id| *id == primary || others.contains(id))
            .collect()
    }

    pub fn is_selected(&self, id: LayerId) -> bool {
        self.selected_ids().contains(&id)
    }

    /// Selects just `id` (or nothing).
    pub fn select(&mut self, id: Option<LayerId>) {
        self.selection = id;
        self.also_selected.clear();
    }

    /// Selects several layers, the last becoming the one the properties
    /// panel shows.
    pub fn select_many(&mut self, ids: &[LayerId]) {
        self.selection = ids.last().copied();
        self.also_selected = ids.iter().copied().filter(|id| Some(*id) != self.selection).collect();
        self.also_selected_with = self.selection;
    }

    /// Adds `id` to the selection, or takes it out if it's already in:
    /// Shift-click, as in Animate.
    pub fn toggle_selected(&mut self, id: LayerId) {
        let mut ids = self.selected_ids();
        if let Some(index) = ids.iter().position(|selected| *selected == id) {
            ids.remove(index);
        } else {
            ids.push(id);
        }
        self.select_many(&ids);
    }

    /// Selects every layer in the timeline being edited that can be
    /// edited (shown and unlocked).
    pub fn select_all(&mut self) {
        let frame = self.frame;
        let ids: Vec<LayerId> =
            self.layers().iter().filter(|l| l.is_shown_at(frame) && !l.locked).map(|l| l.id).collect();
        self.select_many(&ids);
    }

    pub fn selected_layer_mut(&mut self) -> Option<&mut Layer> {
        self.selection.and_then(|id| self.doc.layer_mut(id))
    }

    /// Changes a layer's pose on the current frame.
    ///
    /// This is "auto-keying": each property that changed gets a key on this
    /// frame. Move to frame 24, drag a shape, and you've made a position
    /// tween, without disturbing any rotation or fade that's in progress.
    pub fn set_pose(&mut self, id: LayerId, transform: Transform) {
        self.doc.set_pose(id, self.frame, transform);
    }

    /// Adds or removes the current frame's key for one property: the little
    /// diamond buttons in the inspector.
    pub fn toggle_property_key(&mut self, id: LayerId, property: Property) {
        let frame = self.frame;
        if let Some(layer) = self.doc.layer_mut(id) {
            let channel = layer.track.channel_mut(property);
            if !channel.remove_key(frame) {
                channel.insert_key(frame);
            }
        }
    }

    /// Makes the selected layer appear from the current frame on.
    pub fn set_layer_start_here(&mut self) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            layer.set_start(frame);
        }
    }

    /// Makes the selected layer disappear after the current frame.
    pub fn set_layer_end_here(&mut self) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            layer.set_end(Some(frame));
        }
    }

    /// Turns the selected layer into a symbol in the library, leaving an
    /// instance of it in the layer's place. Like F8 in Animate.
    /// Makes the selected layers a symbol (F8), as Animate does: one layer
    /// becomes an instance of a symbol holding it; several (a folder
    /// counting as its layers) move into one symbol together, and an
    /// instance takes their place. The instance is selected.
    pub fn convert_selected_to_symbol(&mut self) -> Result<SymbolId, String> {
        let ids: Vec<LayerId> = self
            .selected_with_folders()
            .into_iter()
            .filter(|id| self.doc.layer(*id).is_some_and(|l| !l.folder && !l.camera))
            .collect();
        match ids.as_slice() {
            [] => Err("Select the layers to make into a symbol.".to_owned()),
            [one] => {
                let id = *one;
                let symbol = self
                    .doc
                    .convert_layer_to_symbol(id, self.frame)
                    .ok_or("That layer can't become a symbol.")?;
                self.select(Some(id));
                Ok(symbol)
            }
            several => {
                let (symbol, instance) =
                    self.doc.convert_layers_to_symbol(several, self.frame).map_err(|e| e.to_string())?;
                self.select(Some(instance));
                Ok(symbol)
            }
        }
    }

    pub fn can_place(&self, symbol: SymbolId) -> bool {
        self.doc.can_place(self.timeline(), symbol)
    }

    /// Keys every selected layer on the current frame.
    pub fn insert_keyframe(&mut self) {
        let frame = self.frame;
        for id in self.selected_ids() {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.insert_key(frame);
            }
        }
    }

    pub fn remove_keyframe(&mut self) {
        let frame = self.frame;
        for id in self.selected_ids() {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.remove_key(frame);
            }
        }
    }

    /// Deletes every selected layer.
    pub fn delete_selected_layer(&mut self) {
        let frame = self.frame;
        // A folder goes with everything in it, as in Animate.
        let mut doomed: Vec<LayerId> = Vec::new();
        for id in self.selected_ids() {
            for id in self.block_of(id) {
                if !doomed.contains(&id) {
                    doomed.push(id);
                }
            }
        }
        for id in doomed {
            // Its children stay where they are, hanging from what it hung from.
            self.doc.release_children(id, frame);
            self.doc.remove_layer(id);
        }
        self.doc.tidy_controllers();
        self.select(None);
    }

    /// Puts the selected layers in a new folder, where the topmost of them
    /// was (or adds an empty folder on top), and selects it.
    pub fn new_folder(&mut self) {
        let timeline = self.timeline();
        let ids: Vec<LayerId> = self
            .selected_ids()
            .into_iter()
            .filter(|&id| self.doc.timeline_of(id) == Some(timeline))
            .collect();
        if let Some(folder) = self.doc.add_folder(timeline, &ids) {
            self.select(Some(folder));
        }
    }

    /// Shows or hides a layer, and with a folder, everything in it.
    pub fn set_visible(&mut self, id: LayerId, visible: bool) {
        for id in self.block_of(id) {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.visible = visible;
            }
        }
    }

    /// Locks or unlocks a layer, and with a folder, everything in it.
    pub fn set_locked(&mut self, id: LayerId, locked: bool) {
        for id in self.block_of(id) {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.locked = locked;
            }
        }
    }

    /// The selected layers, with everything in any selected folder.
    pub fn selected_with_folders(&self) -> Vec<LayerId> {
        let mut ids: Vec<LayerId> = Vec::new();
        for id in self.selected_ids().into_iter().flat_map(|id| self.block_of(id)) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Adds a template's layers to the timeline being edited, starting at
    /// the playhead, and selects them.
    pub fn insert_template(&mut self, template: &inkframe_core::Template) {
        let (timeline, frame) = (self.timeline(), self.frame);
        let project_dir = self.project_dir();
        let ids = self.doc.insert_template(template, timeline, frame, project_dir.as_deref());
        self.select_many(&ids);
    }

    /// A layer, or a folder and its contents.
    fn block_of(&self, id: LayerId) -> Vec<LayerId> {
        if self.doc.layer(id).is_some_and(|l| l.folder) { self.doc.folder_block(id) } else { vec![id] }
    }

    // ------------------------------------------------------------ clipboard

    /// The selected layers, with everything they need, for the clipboard;
    /// or, inside a layer with shapes picked, just those shapes.
    pub fn copy_selection(&self) -> Option<Clipboard> {
        if let Some(shapes) = self.copy_shapes() {
            return Some(Clipboard::Shapes(shapes));
        }
        let ids = self.selected_with_folders();
        Some(Clipboard::Layers(self.doc.copy_layers(&ids, self.project_dir().as_deref())?))
    }

    /// The selected layer's pose on the current frame, for the clipboard.
    pub fn copy_keyframe(&self) -> Option<Clipboard> {
        let layer = self.selected_layer()?;
        Some(Clipboard::Keyframe(CopiedKeyframe::from_track(&layer.track, self.frame)?))
    }

    /// Pastes clipboard contents. Layers land on top of the timeline being
    /// edited, either where they were (`centered_on` is `None`: "Paste in
    /// Place") or moved so they're centered on a point; a keyframe is
    /// pasted onto the selected layer at the current frame.
    pub fn paste(&mut self, clipboard: &Clipboard, centered_on: Option<Point>) {
        match clipboard {
            Clipboard::Layers(copied) => {
                // Where the copies are on this frame, on average, so the
                // group lands centered rather than by its first layer.
                let offset = centered_on.map_or(Point::ZERO, |center| {
                    let positions: Vec<Point> =
                        copied.layers.iter().map(|l| l.track.sample(self.frame as f32).position).collect();
                    let sum = positions.iter().fold(Point::ZERO, |sum, p| sum + *p);
                    center - sum * (1.0 / positions.len().max(1) as f32)
                });
                let project_dir = self.project_dir();
                let pasted = self.doc.paste_layers(copied, self.timeline(), offset, project_dir.as_deref());
                self.select_many(&pasted);
            }
            Clipboard::Keyframe(keyframe) => {
                let frame = self.frame;
                if let Some(layer) = self.selection.and_then(|id| self.doc.layer_mut(id)) {
                    keyframe.paste_into(&mut layer.track, frame);
                }
            }
            Clipboard::Shapes(shapes) => self.paste_shapes(shapes, centered_on),
        }
    }

    /// Text copied from another app becomes a new text layer at `at`.
    pub fn paste_text(&mut self, text: &str, at: Point) {
        if let Some(id) = self.add_text(at)
            && let Some(layer) = self.doc.layer_mut(id)
            && let Some(Shape::Text(t)) = layer.elements_at_mut(self.frame).first_mut().map(|e| &mut e.shape)
        {
            t.content = text.to_owned();
        }
        self.remeasure_text();
    }

    /// Duplicates every selected layer, selecting the copies.
    pub fn duplicate_selected_layer(&mut self) {
        let copies: Vec<LayerId> =
            self.selected_ids().into_iter().filter_map(|id| self.doc.duplicate_layer(id)).collect();
        self.select_many(&copies);
    }

    /// Moves the selected layer up or down past its neighbours, stepping
    /// over folders rather than into them.
    pub fn move_selected_layer(&mut self, delta: isize) {
        if let Some(id) = self.selection {
            self.doc.move_among_siblings(id, delta);
        }
    }
}

/// Where to look, in `layer`'s coordinates, for the point at rest that a
/// skeleton bends onto each point of `element` (see `LayerBend::unbend`):
/// the layer's drawing and the new shape, with room around them, as bones
/// can carry a point a long way.
fn unbend_area(layer: &Layer, frame: u32, element: &Element) -> Rect {
    let drawn = layer.elements_at(frame).iter().map(Element::bounds).fold(element.bounds(), Rect::union);
    drawn.expand(drawn.width().max(drawn.height()).max(200.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{FrameRate, Point};

    /// Writes a blank PNG of the given size to a temporary file.
    fn png(name: &str, width: u32, height: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkframe-editor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        tiny_skia::Pixmap::new(width, height).unwrap().save_png(&path).unwrap();
        path
    }

    #[test]
    fn imported_images_land_mid_stage_at_their_own_size() {
        let mut editor = Editor::new(Document::default());
        editor.frame = 5;
        editor.import_image(&png("logo.png", 300, 200)).unwrap();

        let layer = editor.selected_layer().expect("the new layer is selected");
        assert_eq!(layer.name, "logo");
        assert_eq!(layer.start, 5, "it starts on the current frame");
        assert_eq!(layer.transform_at(5).position, Point::new(960.0, 540.0));
        assert!(matches!(
            layer.drawings[0].elements[0].shape,
            Shape::Image { width: 300.0, height: 200.0, .. }
        ));
    }

    #[test]
    fn images_bigger_than_the_stage_are_shrunk_to_fit() {
        let mut editor = Editor::new(Document::default());
        editor.import_image(&png("panorama.png", 3840, 1080)).unwrap();
        let layer = editor.selected_layer().unwrap();
        // Half size fits the 1920-pixel-wide stage, keeping the proportions.
        assert!(matches!(
            layer.drawings[0].elements[0].shape,
            Shape::Image { width: 1920.0, height: 540.0, .. }
        ));
    }

    #[test]
    fn importing_something_that_isnt_an_image_explains_why() {
        let path = png("notes.png", 1, 1).with_file_name("notes.txt");
        std::fs::write(&path, "not a picture").unwrap();
        let mut editor = Editor::new(Document::default());
        assert!(editor.import_image(&path).is_err());
        assert!(editor.doc.layers.is_empty() && editor.doc.images.is_empty());
    }

    #[test]
    fn importing_a_font_sets_the_selected_text_in_it() {
        let dir = std::env::temp_dir().join(format!("inkframe-editor-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Hack-Regular.ttf");
        std::fs::write(&path, epaint_default_fonts::HACK_REGULAR).unwrap();

        let mut editor = Editor::new(Document::default());
        editor.add_text(Point::new(100.0, 100.0));
        assert_eq!(editor.import_font(&path).unwrap(), ["Hack"]);
        assert_eq!(editor.doc.fonts.len(), 1);
        let Shape::Text(text) = &editor.selected_layer().unwrap().drawings[0].elements[0].shape else {
            panic!("not text")
        };
        assert_eq!(text.font.family, "Hack");

        // Importing it again doesn't add a second copy.
        editor.import_font(&path).unwrap();
        assert_eq!(editor.doc.fonts.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clicks_go_through_masks_unless_the_mask_is_selected() {
        let mut editor = Editor::new(Document::default());
        let square = |size: f32| Element {
            shape: Shape::Rect { width: size, height: size, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let scene =
            editor.doc.add_layer("Scene", vec![square(400.0)], 0, Transform::at(Point::new(200.0, 200.0)));
        let mask =
            editor.doc.add_layer("Window", vec![square(100.0)], 0, Transform::at(Point::new(200.0, 200.0)));
        editor.doc.mask_layer_below(mask);

        let middle = Point::new(200.0, 200.0);
        assert_eq!(editor.hit_test(middle, 1.0), Some(scene), "the mask isn't drawn, so the scene is hit");
        editor.selection = Some(mask);
        assert_eq!(editor.hit_test(middle, 1.0), Some(mask), "a selected mask can be grabbed");
    }

    #[test]
    fn copied_layers_paste_in_the_center_or_in_place() {
        let mut editor = Editor::new(Document::default());
        let square = Element {
            shape: Shape::Rect { width: 10.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let id = editor.doc.add_layer("Box", vec![square], 0, Transform::at(Point::new(100.0, 100.0)));
        editor.selection = Some(id);
        let copied = editor.copy_selection().unwrap();

        editor.paste(&copied, Some(Point::new(960.0, 540.0)));
        let centered = editor.selected_layer().unwrap();
        assert_ne!(centered.id, id);
        assert_eq!(centered.transform_at(0).position, Point::new(960.0, 540.0));

        editor.paste(&copied, None);
        assert_eq!(editor.selected_layer().unwrap().transform_at(0).position, Point::new(100.0, 100.0));
        assert_eq!(editor.doc.layers.len(), 3);
    }

    #[test]
    fn text_from_another_app_pastes_as_a_text_layer() {
        let mut editor = Editor::new(Document::default());
        editor.paste_text("Hello from elsewhere", Point::new(200.0, 200.0));
        let Shape::Text(text) = &editor.selected_layer().unwrap().drawings[0].elements[0].shape else {
            panic!("not text")
        };
        assert_eq!(text.content, "Hello from elsewhere");
        assert!(text.measured.width() > 0.0);
    }

    #[test]
    fn several_selected_layers_are_copied_deleted_and_keyed_together() {
        let mut editor = Editor::new(Document::default());
        let ids: Vec<LayerId> =
            (0..3).map(|i| editor.doc.add_layer(format!("L{i}"), vec![], 0, Transform::default())).collect();
        editor.select_many(&[ids[0], ids[2]]);
        assert_eq!(editor.selected_ids(), [ids[0], ids[2]]);

        editor.frame = 5;
        editor.insert_keyframe();
        assert!(editor.doc.layer(ids[0]).unwrap().track.has_key(5));
        assert!(!editor.doc.layer(ids[1]).unwrap().track.has_key(5));

        let Some(Clipboard::Layers(copied)) = editor.copy_selection() else { panic!("nothing copied") };
        assert_eq!(copied.layers.len(), 2);

        editor.delete_selected_layer();
        assert_eq!(editor.doc.layers.len(), 1);
        assert!(editor.selected_ids().is_empty());
    }

    #[test]
    fn picking_one_layer_ends_a_multi_selection() {
        let mut editor = Editor::new(Document::default());
        let a = editor.doc.add_layer("A", vec![], 0, Transform::default());
        let b = editor.doc.add_layer("B", vec![], 0, Transform::default());
        editor.select_many(&[a, b]);
        // Older code that just sets the selection still gets one layer.
        editor.selection = Some(a);
        assert_eq!(editor.selected_ids(), [a]);
    }

    fn editor_with_layer() -> (Editor, LayerId) {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("A", vec![], 0, Transform::default());
        editor.selection = Some(id);
        (editor, id)
    }

    #[test]
    fn keyframes_include_a_deformers_bends() {
        let (mut editor, id) = editor_with_layer();
        let bounds = inkframe_core::Rect::from_center_size(Point::ZERO, 100.0, 10.0);
        editor.doc.layer_mut(id).unwrap().deformer = Some(inkframe_core::Deformer::along(bounds, 2, 0));
        editor.set_frame(8);
        editor.insert_keyframe();
        let layer = editor.doc.layer(id).unwrap();
        assert_eq!(layer.deformer.as_ref().unwrap().key_frames(), [0, 8]);
        assert_eq!(layer.key_frames(), [0, 8]);
        editor.remove_keyframe();
        assert_eq!(editor.doc.layer(id).unwrap().deformer.as_ref().unwrap().key_frames(), [0]);
    }

    #[test]
    fn layer_via_cut_then_merge_down_puts_the_pixels_back_moved() {
        let mut editor = Editor::new(Document::default());
        let base = editor.add_paint_layer().unwrap();
        let (_, canvas, w, h) = editor.paint_target().unwrap();
        editor.doc.canvas_mut(canvas).unwrap().write_region(100, 100, 10, 10, &[0, 90, 0, 255].repeat(100));
        let selection = Selection::rectangle(w, h, Point::new(90.0, 90.0), Point::new(120.0, 120.0));
        editor.pixel_selection = Some((canvas, selection));

        editor.layer_via_copy(true);
        let cut = editor.selection.unwrap();
        assert_ne!(cut, base);
        assert_eq!(editor.doc.canvas(canvas).unwrap().pixel(105, 105), [0; 4]);
        // Move the new layer 50 pixels right, then merge it back down.
        let pose = editor.doc.pose(editor.doc.layer(cut).unwrap(), 0);
        editor.set_pose(cut, Transform { position: pose.position + Point::new(50.0, 0.0), ..pose });
        assert!(editor.merge_down());
        assert_eq!(editor.selection, Some(base));
        assert_eq!(editor.doc.canvas(canvas).unwrap().pixel(155, 105), [0, 90, 0, 255]);
        assert_eq!(editor.layers().len(), 1);
    }

    #[test]
    fn toolbar_slots_remember_their_tool_and_shift_cycles_them() {
        let mut editor = Editor::new(Document::default());
        let fill = Tool::PaintBucket.group();
        editor.use_slot(fill, false);
        assert_eq!(editor.tool, Tool::PaintBucket);
        editor.use_slot(fill, true);
        assert_eq!(editor.tool, Tool::Gradient);
        editor.use_tool(Tool::Brush);
        // The slot's key brings back the one used last.
        editor.use_slot(fill, false);
        assert_eq!(editor.tool, Tool::Gradient);
        let retouch = Tool::Smudge.group();
        editor.use_slot(retouch, false);
        editor.use_slot(retouch, true);
        editor.use_slot(retouch, true);
        editor.use_slot(retouch, true);
        assert_eq!(editor.tool, Tool::Smudge, "cycling wraps around");
    }

    #[test]
    fn new_drawings_show_from_the_current_frame() {
        let (mut editor, id) = editor_with_layer();
        editor.set_frame(6);
        editor.new_drawing(false);
        editor.set_frame(10);
        editor.new_drawing(false);
        let layer = editor.doc.layer(id).unwrap();
        let names: Vec<&str> = layer.drawings.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["1", "2", "3"]);
        assert_eq!([0, 6, 9, 10].map(|f| layer.drawing_index_at(f)), [0, 1, 1, 2]);
    }

    #[test]
    fn flipping_through_drawings_wraps_around() {
        let (mut editor, id) = editor_with_layer();
        editor.new_drawing(false);
        editor.new_drawing(false);
        editor.set_frame(4);
        editor.cycle_drawing(1);
        assert_eq!(editor.doc.layer(id).unwrap().drawing_index_at(4), 0, "from the last back to the first");
        editor.cycle_drawing(-1);
        assert_eq!(editor.doc.layer(id).unwrap().drawing_index_at(4), 2);
        // Frames before keep what they showed.
        assert_eq!(editor.doc.layer(id).unwrap().drawing_index_at(3), 2);
    }

    #[test]
    fn duplicating_a_drawing_copies_what_shows() {
        let (mut editor, id) = editor_with_layer();
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 5.0, radius_y: 5.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        editor.doc.layer_mut(id).unwrap().drawings[0].elements.push(dot.clone());
        editor.set_frame(3);
        editor.new_drawing(true);
        let layer = editor.doc.layer(id).unwrap();
        assert_eq!(layer.drawing_index_at(3), 1);
        assert_eq!(layer.elements_at(3), [dot]);
    }

    #[test]
    fn shapes_drawn_on_a_swapping_layer_go_into_the_drawing_shown() {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer(
            "Mouth",
            vec![],
            0,
            Transform { scale: Point::new(2.0, 2.0), ..Transform::at(Point::new(400.0, 400.0)) },
        );
        editor.selection = Some(id);
        editor.new_drawing(false);
        let layers = editor.layers().len();

        let dot = Element {
            shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        editor.add_drawn_shape("Oval", dot, Point::new(460.0, 400.0));

        assert_eq!(editor.layers().len(), layers, "no new layer");
        let layer = editor.doc.layer(id).unwrap();
        assert!(layer.drawings[0].elements.is_empty(), "the other drawing is untouched");
        // In the layer's own coordinates, which are scaled 2×: 30 to the
        // right, 5 across.
        let bounds = layer.shape_bounds(0).unwrap();
        assert!(bounds.center().distance(Point::new(30.0, 0.0)) < 0.01, "{bounds:?}");
        assert!((bounds.width() - 10.0).abs() < 0.01);
    }

    #[test]
    fn shapes_go_into_the_selected_layer_unless_turned_off() {
        let (mut editor, id) = editor_with_layer();
        let layers = editor.layers().len();
        let shapes = |editor: &Editor| editor.doc.layer(id).unwrap().elements_at(0).len();
        let before = shapes(&editor);
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        editor.add_drawn_shape("Oval", dot.clone(), Point::new(460.0, 400.0));
        assert_eq!(editor.layers().len(), layers, "no new layer");
        assert_eq!(shapes(&editor), before + 1);

        editor.draw_into_layer = false;
        editor.add_drawn_shape("Oval", dot.clone(), Point::new(460.0, 400.0));
        assert_eq!(editor.layers().len(), layers + 1, "a layer of its own");

        // With nothing selected there's nowhere to draw into.
        editor.draw_into_layer = true;
        editor.select(None);
        editor.add_drawn_shape("Oval", dot, Point::new(460.0, 400.0));
        assert_eq!(editor.layers().len(), layers + 2);
    }

    #[test]
    fn library_assets_are_placed_where_they_are_dropped() {
        let (mut editor, layer) = editor_with_layer();
        let symbol = editor.doc.convert_layer_to_symbol(layer, 0).unwrap();
        let image = editor.doc.add_image(Path::new("/art/sky.png"), None, (3840, 1080));
        let sound = editor.doc.add_audio(Path::new("/sound/boing.wav"), None, 0);
        editor.set_frame(5);

        let star = editor.place_asset_at(Asset::Symbol(symbol), Point::new(300.0, 200.0)).unwrap();
        let placed = editor.doc.layer(star).unwrap();
        assert_eq!(placed.instance.map(|i| i.symbol), Some(symbol));
        assert_eq!(placed.track.position.sample(5.0), Some(Point::new(300.0, 200.0)));
        assert_eq!(editor.selection, Some(star));

        let sky = editor.place_asset_at(Asset::Image(image), Point::new(960.0, 540.0)).unwrap();
        let sky = editor.doc.layer(sky).unwrap();
        assert_eq!(sky.name, "sky");
        // Twice the stage's width, so fitted to it, as when imported.
        let Shape::Image { width, .. } = sky.elements_at(5)[0].shape else { panic!("not a picture") };
        assert_eq!(width, 1920.0);

        // A sound goes on the timeline again, from this frame.
        assert_eq!(editor.place_asset_at(Asset::Sound(sound), Point::ZERO), None);
        assert_eq!(editor.doc.audio.len(), 2);
        assert_eq!(editor.doc.audio[1].start, 5);
        assert_eq!(editor.doc.audio[1].name, "boing");
    }

    #[test]
    fn a_symbol_cant_be_placed_inside_itself_from_the_library() {
        let (mut editor, layer) = editor_with_layer();
        let symbol = editor.doc.convert_layer_to_symbol(layer, 0).unwrap();
        editor.enter_symbol(symbol, 0);
        let layers = editor.layers().len();
        assert_eq!(editor.place_asset_at(Asset::Symbol(symbol), Point::ZERO), None);
        assert_eq!(editor.layers().len(), layers);
    }

    #[test]
    fn a_new_layer_goes_above_the_selected_one_and_takes_drawing() {
        let (mut editor, id) = editor_with_layer();
        let other = editor.doc.add_layer("Top", Vec::new(), 0, Transform::default());
        editor.select(Some(id));
        let new = editor.add_empty_layer().unwrap();
        let order: Vec<_> = editor.layers().iter().map(|l| l.id).collect();
        assert_eq!(order, vec![id, new, other], "just above the selected layer");
        assert_eq!(editor.selection, Some(new));
        assert_eq!(editor.doc.layer(new).unwrap().name, "Layer 1");
        assert!(editor.doc.layer(new).unwrap().elements_at(0).is_empty());

        let dot = Element {
            shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        editor.add_drawn_shape("Oval", dot, Point::new(100.0, 100.0));
        let layer = editor.doc.layer(new).unwrap();
        assert_eq!(layer.elements_at(0).len(), 1, "drawn into the new layer");
        // Where it was drawn on the stage: the layer's origin is the middle
        // of the stage, so in its own coordinates it's up and to the left.
        let middle = editor.doc.stage_rect().center();
        let bounds = editor.doc.layer_bounds(layer, 0).unwrap();
        let on_stage = Point::new(bounds.center().x + middle.x, bounds.center().y + middle.y);
        assert!(on_stage.distance(Point::new(100.0, 100.0)) < 0.5, "{bounds:?}");
    }

    #[test]
    fn set_pose_auto_keys_the_current_frame() {
        let (mut editor, id) = editor_with_layer();
        editor.set_frame(12);
        editor.set_pose(id, Transform::at(Point::new(5.0, 5.0)));
        assert!(editor.doc.layer(id).unwrap().track.position.key_at(12).is_some());
    }

    #[test]
    fn layer_span_commands_use_the_current_frame() {
        let (mut editor, id) = editor_with_layer();
        editor.set_frame(30);
        editor.set_layer_end_here();
        editor.set_frame(10);
        editor.set_layer_start_here();
        let layer = editor.doc.layer(id).unwrap();
        assert_eq!((layer.start, layer.end), (10, Some(30)));
    }

    #[test]
    fn convert_and_place_symbols() {
        let (mut editor, id) = editor_with_layer();
        let symbol = editor.convert_selected_to_symbol().unwrap();
        assert!(editor.doc.layer(id).unwrap().instance.is_some());

        editor.set_frame(6);
        editor.place_asset_at(Asset::Symbol(symbol), editor.doc.stage_rect().center());
        let placed = editor.selected_layer().unwrap();
        assert_ne!(placed.id, id);
        assert_eq!(placed.start, 6);
        assert_eq!(editor.doc.symbol_use_count(symbol), 2);
    }

    #[test]
    fn entering_and_leaving_a_symbol_restores_frame_and_selection() {
        let (mut editor, id) = editor_with_layer();
        let symbol = editor.convert_selected_to_symbol().unwrap();
        editor.doc.symbol_mut(symbol).unwrap().duration = 10;
        editor.set_frame(25);

        editor.enter_instance(id);
        assert_eq!(editor.timeline(), TimelineId::Symbol(symbol));
        assert_eq!(editor.duration(), 10);
        // The instance loops a 10-frame symbol, so main frame 25 shows frame 5.
        assert_eq!(editor.frame, 5);
        assert_eq!(editor.selection, None);
        assert_eq!(editor.layers().len(), 1);

        editor.exit_symbol();
        assert_eq!(editor.timeline(), TimelineId::Main);
        assert_eq!((editor.frame, editor.selection), (25, Some(id)));
    }

    #[test]
    fn editing_in_place_follows_the_instance() {
        let (mut editor, id) = editor_with_layer();
        let symbol = editor.convert_selected_to_symbol().unwrap();
        let instance = Transform { rotation: 90.0, ..Transform::at(Point::new(300.0, 200.0)) };
        editor.doc.layer_mut(id).unwrap().track.set_pose(0, instance);

        editor.enter_instance(id);
        let to_stage = editor.timeline_transform();
        // The symbol's x axis now points down the screen, from (300, 200).
        let p = to_stage.apply(Point::new(10.0, 0.0));
        assert!(p.distance(Point::new(300.0, 210.0)) < 1e-3, "{p:?}");

        let around = editor.surroundings();
        assert_eq!(around.len(), 1);
        assert_eq!((around[0].timeline, around[0].skip), (TimelineId::Main, id));

        // Opening from the library instead shows the symbol on its own.
        editor.exit_symbol();
        editor.enter_symbol(symbol, 0);
        assert!(editor.surroundings().is_empty());
        assert_eq!(editor.timeline_transform().apply(Point::ZERO), editor.doc.stage_rect().center());
    }

    #[test]
    fn drawing_inside_a_symbol_adds_to_the_symbol() {
        let (mut editor, _) = editor_with_layer();
        let symbol = editor.convert_selected_to_symbol().unwrap();
        editor.enter_symbol(symbol, 0);
        let shape = Element {
            shape: inkframe_core::Shape::Ellipse { radius_x: 5.0, radius_y: 5.0 },
            style: editor.draw_style.clone(),
        };
        editor.add_drawn_shape("Oval", shape, Point::ZERO);

        assert_eq!(editor.doc.layers_of(TimelineId::Symbol(symbol)).len(), 2);
        assert_eq!(editor.doc.layers.len(), 1);
    }

    #[test]
    fn undo_out_of_a_deleted_symbol() {
        let (mut editor, _) = editor_with_layer();
        editor.commit_history();
        let symbol = editor.convert_selected_to_symbol().unwrap();
        editor.commit_history();
        editor.enter_symbol(symbol, 0);
        // Undoing the conversion removes the symbol we're inside.
        editor.undo();
        assert_eq!(editor.timeline(), TimelineId::Main);
    }

    #[test]
    fn playback_follows_the_clock() {
        let mut editor = Editor::new(Document::default());
        editor.doc.frame_rate = FrameRate::new(24, 1);
        editor.toggle_playback();
        editor.advance_playback(0.5); // Half a second at 24 fps.
        assert_eq!(editor.frame, 12);
    }

    #[test]
    fn playback_loops_or_stops_at_the_end() {
        let mut editor = Editor::new(Document::default());
        editor.set_frame(editor.doc.last_frame());
        editor.toggle_playback(); // Restarts from 0.
        assert_eq!(editor.frame, 0);

        editor.set_frame(editor.doc.last_frame());
        editor.playback.looping = false;
        editor.advance_playback(1.0);
        assert!(!editor.playback.playing);
        assert_eq!(editor.frame, editor.doc.last_frame());
    }

    #[test]
    fn undo_clears_selection_of_removed_layers() {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("A", vec![], 0, Transform::default());
        editor.commit_history();
        editor.selection = Some(id);
        editor.undo();
        assert_eq!(editor.selection, None);
    }

    #[test]
    fn shape_tweens_and_hints_through_the_editor() {
        use inkframe_core::{Color, Element, Style};
        let square = Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let mut doc = Document::default();
        let id = doc.add_layer("Morph", vec![square], 0, inkframe_core::Transform::default());
        let mut editor = Editor::new(doc);
        editor.select(Some(id));
        assert_eq!(editor.shape_tween_here(), None, "nothing to morph into yet");

        // A copy of the drawing from frame 10 on, then a tween into it.
        editor.set_frame(10);
        editor.new_drawing(true);
        editor.set_frame(4);
        assert_eq!(editor.shape_tween_here(), Some(false));
        editor.set_shape_tween(true);
        assert_eq!(editor.shape_tween_here(), Some(true));
        assert_eq!(editor.shape_tween_drawings(), Some((0, 1)));

        // A hint lands on both drawings, on the square's outline.
        editor.add_shape_hint();
        let layer = editor.selected_layer().unwrap();
        let hint = layer.drawings[0].hints[0];
        assert!((hint.x.abs() - 50.0).abs() < 1e-3 || (hint.y.abs() - 50.0).abs() < 1e-3);
        assert_eq!(layer.drawings[1].hints.len(), 1);
        editor.clear_shape_hints();
        assert!(editor.selected_layer().unwrap().drawings.iter().all(|d| d.hints.is_empty()));
    }
}
