//! Editor state and the operations the UI can perform on it.
//!
//! Everything here is UI-toolkit agnostic: panels call these methods instead
//! of poking at the document directly, which keeps the rules ("a new
//! keyframe goes on the current frame", "selection follows undo") in one
//! place.

use std::path::{Path, PathBuf};

use inkframe_core::{
    Affine, Color, Document, DocumentError, Element, History, Layer, LayerId, Point, Property, Stroke, Style,
    SymbolId, TimelineId, Transform,
};

/// The tools in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Subselect,
    Pen,
    Rectangle,
    Ellipse,
    Pencil,
}

impl Tool {
    pub const ALL: [Tool; 6] =
        [Tool::Select, Tool::Subselect, Tool::Pen, Tool::Rectangle, Tool::Ellipse, Tool::Pencil];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Selection",
            Tool::Subselect => "Subselection",
            Tool::Pen => "Pen",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Oval",
            Tool::Pencil => "Pencil",
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
    pub selection: Option<LayerId>,
    pub tool: Tool,
    /// Fill and stroke given to newly drawn shapes.
    pub draw_style: Style,
    /// Stroke given to pencil lines (which are never filled).
    pub pencil_stroke: Stroke,
    pub playback: Playback,
    pub onion: OnionSkin,
    /// The property whose easing curve the inspector is showing and editing.
    pub focused_property: Property,
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
            tool: Tool::Select,
            draw_style: Style {
                fill: Some(Color::rgb(0x3d, 0x8b, 0xfd)),
                stroke: Some(Stroke { color: Color::rgb(0x1a, 0x1a, 0x1a), width: 2.0 }),
            },
            pencil_stroke: Stroke { color: Color::rgb(0x1a, 0x1a, 0x1a), width: 4.0 },
            playback: Playback { looping: true, ..Playback::default() },
            onion: OnionSkin { enabled: false, before: 2, after: 2 },
            focused_property: Property::Position,
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
                Some(layer) => to_stage.pre_concat(layer.transform_at(open.return_frame).to_affine()),
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
                    to_stage = to_stage.pre_concat(layer.transform_at(open.return_frame).to_affine());
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
    pub fn hit_test(&self, point: Point, tolerance: f32) -> Option<LayerId> {
        self.doc.hit_test_in(self.timeline(), point, self.frame, tolerance)
    }

    /// Adds a layer for something just drawn, to the timeline being edited.
    /// Like Animate, the drawing appears from the current frame onward.
    pub fn add_drawn_layer(&mut self, prefix: &str, element: Element, position: Point) {
        let timeline = self.timeline();
        let name = self.doc.next_layer_name_in(timeline, prefix);
        let id = self.doc.add_layer_to(timeline, name, vec![element], self.frame, Transform::at(position));
        if let Some(layer) = id.and_then(|id| self.doc.layer_mut(id)) {
            layer.start = self.frame;
        }
        self.selection = id;
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

    pub fn selected_layer_mut(&mut self) -> Option<&mut Layer> {
        self.selection.and_then(|id| self.doc.layer_mut(id))
    }

    /// Changes a layer's pose on the current frame.
    ///
    /// This is "auto-keying": each property that changed gets a key on this
    /// frame. Move to frame 24, drag a shape, and you've made a position
    /// tween, without disturbing any rotation or fade that's in progress.
    pub fn set_pose(&mut self, id: LayerId, transform: Transform) {
        let frame = self.frame;
        if let Some(layer) = self.doc.layer_mut(id) {
            layer.track.set_pose(frame, transform);
        }
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

    pub fn insert_keyframe(&mut self) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            layer.track.insert_key(frame);
        }
    }

    pub fn remove_keyframe(&mut self) {
        let frame = self.frame;
        if let Some(layer) = self.selected_layer_mut() {
            layer.track.remove_key(frame);
        }
    }

    pub fn delete_selected_layer(&mut self) {
        if let Some(id) = self.selection.take() {
            self.doc.remove_layer(id);
        }
    }

    pub fn duplicate_selected_layer(&mut self) {
        if let Some(id) = self.selection {
            self.selection = self.doc.duplicate_layer(id);
        }
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

    fn editor_with_layer() -> (Editor, LayerId) {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("A", vec![], 0, Transform::default());
        editor.selection = Some(id);
        (editor, id)
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
        editor.add_drawn_layer("Oval", shape, Point::ZERO);

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
