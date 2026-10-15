//! Editor state and the operations the UI can perform on it.
//!
//! Everything here is UI-toolkit agnostic: panels call these methods instead
//! of poking at the document directly, which keeps the rules ("a new
//! keyframe goes on the current frame", "selection follows undo") in one
//! place.

use std::path::{Path, PathBuf};

use inkframe_core::FontFileId;
use inkframe_core::{
    Affine, Anchor, Color, Document, DocumentError, Drawing, Element, History, Layer, LayerId, Point,
    Property, Shape, Stroke, Style, SymbolId, Text, TimelineId, Transform, path_from_anchors,
};
use inkframe_core::{Clipboard, CopiedKeyframe};
use inkframe_render::ImageLibrary;

/// The tools in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Subselect,
    Pen,
    Rectangle,
    Ellipse,
    Pencil,
    Text,
    /// Poses a rig by dragging: the joints above the dragged layer turn so
    /// it follows the pointer (inverse kinematics).
    Bone,
}

impl Tool {
    pub const ALL: [Tool; 8] = [
        Tool::Select,
        Tool::Subselect,
        Tool::Bone,
        Tool::Pen,
        Tool::Rectangle,
        Tool::Ellipse,
        Tool::Pencil,
        Tool::Text,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Selection",
            Tool::Subselect => "Subselection",
            Tool::Pen => "Pen",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Oval",
            Tool::Pencil => "Pencil",
            Tool::Text => "Text",
            Tool::Bone => "Bone",
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
            frame: 0,
            selection: None,
            also_selected: Vec::new(),
            also_selected_with: None,
            tool: Tool::Select,
            draw_style: Style {
                fill: Some(Color::rgb(0x3d, 0x8b, 0xfd)),
                stroke: Some(Stroke { color: Color::rgb(0x1a, 0x1a, 0x1a), width: 2.0 }),
            },
            pencil_stroke: Stroke { color: Color::rgb(0x1a, 0x1a, 0x1a), width: 4.0 },
            playback: Playback { looping: true, ..Playback::default() },
            onion: OnionSkin { enabled: false, before: 2, after: 2 },
            focused_property: Property::Position,
            images: ImageLibrary::default(),
            font_problems: Vec::new(),
            open_symbols: Vec::new(),
        }
    }

    // ---------------------------------------------------------------- files

    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        let mut editor = Self::new(Document::load(path)?);
        editor.path = Some(path.to_owned());
        Ok(editor)
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), DocumentError> {
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

    /// Places an image file on a new layer, on the current frame, in the
    /// middle of the stage, at its own size in pixels (or shrunk to fit the
    /// stage, so a 6000-pixel photo doesn't land as a giant).
    pub fn import_image(&mut self, path: &Path) -> Result<(), String> {
        let (width, height) = inkframe_render::image_size(path)?;
        let project_dir = self.project_dir();
        let image = self.doc.add_image(path, project_dir.as_deref(), (width, height));

        let fit = (self.doc.width as f32 / width as f32).min(self.doc.height as f32 / height as f32).min(1.0);
        let element = Element {
            shape: Shape::Image { image, width: width as f32 * fit, height: height as f32 * fit },
            style: Style { fill: None, stroke: None },
        };
        // The stage's center, in the coordinates of the timeline being
        // edited, so inside a symbol the picture still appears mid-stage.
        let stage_center = self.doc.stage_rect().center();
        let center =
            self.timeline_transform().inverse().map_or(stage_center, |to_local| to_local.apply(stage_center));

        let name = self.doc.image(image).map_or_else(|| "Image".to_owned(), |i| i.name.clone());
        let taken = self.layers().iter().any(|l| l.name == name);
        let name = if taken { self.doc.next_layer_name_in(self.timeline(), &name) } else { name };
        self.add_named_layer(name, element, center);
        Ok(())
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
            style: Style { fill: Some(Color::BLACK), stroke: None },
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
        self.path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(|| "Untitled".to_owned(), |s| s.to_string_lossy().into_owned())
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

    /// Undo can remove layers or shorten the animation; make sure the editor
    /// isn't left pointing at things that no longer exist.
    fn after_document_replaced(&mut self) {
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
        let Some(layer) = self
            .selected_layer()
            .filter(|l| l.drawings.len() > 1 && !l.locked && l.is_shown_at(frame) && l.instance.is_none())
        else {
            return false;
        };
        let to_local = self.doc.layer_matrix(layer, frame).inverse();
        let Some(to_local) = to_local.map(|m| m.pre_concat(Affine::translate(position))) else {
            return false;
        };
        let (anchors, closed) = element.shape.to_anchors();
        let moved: Vec<Anchor> = anchors
            .iter()
            .map(|a| Anchor {
                point: to_local.apply(a.point),
                handle_in: to_local.apply(a.handle_in),
                handle_out: to_local.apply(a.handle_out),
            })
            .collect();
        let Some(path) = path_from_anchors(&moved, closed) else { return false };
        let id = layer.id;
        if let Some(layer) = self.doc.layer_mut(id) {
            layer.elements_at_mut(frame).push(Element { shape: Shape::Path(path), style: element.style });
        }
        true
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
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut().filter(|l| l.instance.is_none()) {
            let elements = if copy { layer.elements_at(frame).to_vec() } else { Vec::new() };
            let index = layer.add_drawing(Drawing::new(layer.next_drawing_name(), elements));
            layer.expose(frame, index);
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
    pub fn convert_selected_to_symbol(&mut self) -> Option<SymbolId> {
        let id = self.selection?;
        self.doc.convert_layer_to_symbol(id, self.frame)
    }

    /// Adds a new layer with an instance of `symbol` in the middle of the
    /// stage, starting on the current frame, and selects it.
    ///
    /// Inside a symbol, a symbol that contains it (or itself) can't be
    /// placed; see [`Editor::can_place`].
    pub fn place_symbol(&mut self, symbol: SymbolId) {
        // The middle of the stage, in the coordinates of the timeline being
        // edited (which may be a rotated or scaled instance).
        let center = self
            .timeline_transform()
            .inverse()
            .map_or(Point::ZERO, |to_timeline| to_timeline.apply(self.doc.stage_rect().center()));
        if let Some(id) = self.doc.add_instance_layer(self.timeline(), symbol, self.frame, center) {
            self.selection = Some(id);
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
                layer.track.insert_key(frame);
            }
        }
    }

    pub fn remove_keyframe(&mut self) {
        let frame = self.frame;
        for id in self.selected_ids() {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.track.remove_key(frame);
            }
        }
    }

    /// Deletes every selected layer.
    pub fn delete_selected_layer(&mut self) {
        let frame = self.frame;
        for id in self.selected_ids() {
            // Its children stay where they are, hanging from what it hung from.
            self.doc.release_children(id, frame);
            self.doc.remove_layer(id);
        }
        self.select(None);
    }

    // ------------------------------------------------------------ clipboard

    /// The selected layers, with everything they need, for the clipboard.
    pub fn copy_selection(&self) -> Option<Clipboard> {
        let ids = self.selected_ids();
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

    pub fn move_selected_layer(&mut self, delta: isize) {
        if let Some(id) = self.selection {
            self.doc.move_layer(id, delta);
        }
    }
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
            style: Style { fill: Some(Color::BLACK), stroke: None },
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
            style: Style { fill: Some(Color::BLACK), stroke: None },
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
            style: Style { fill: Some(Color::BLACK), stroke: None },
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
            style: Style { fill: Some(Color::BLACK), stroke: None },
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
    fn shapes_drawn_with_a_one_drawing_layer_selected_get_their_own_layer() {
        let (mut editor, _) = editor_with_layer();
        let layers = editor.layers().len();
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        editor.add_drawn_shape("Oval", dot, Point::new(460.0, 400.0));
        assert_eq!(editor.layers().len(), layers + 1);
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
        editor.place_symbol(symbol);
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
            style: editor.draw_style,
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
}
