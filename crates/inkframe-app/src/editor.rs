//! Editor state and the operations the UI can perform on it.
//!
//! Everything here is UI-toolkit agnostic: panels call these methods instead
//! of poking at the document directly, which keeps the rules ("a new
//! keyframe goes on the current frame", "selection follows undo") in one
//! place.

use std::path::{Path, PathBuf};

use inkframe_core::{
    Color, Document, DocumentError, History, Layer, LayerId, Property, Stroke, Style, SymbolId, Transform,
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
        if self.selection.is_some_and(|id| self.doc.layer(id).is_none()) {
            self.selection = None;
        }
        self.frame = self.frame.min(self.doc.last_frame());
    }

    // ------------------------------------------------------------- timeline

    pub fn set_frame(&mut self, frame: u32) {
        self.frame = frame.min(self.doc.last_frame());
    }

    pub fn step(&mut self, delta: i64) {
        let frame = (self.frame as i64 + delta).clamp(0, self.doc.last_frame() as i64);
        self.frame = frame as u32;
    }

    pub fn toggle_playback(&mut self) {
        self.playback.playing = !self.playback.playing;
        self.playback.elapsed = 0.0;
        // Pressing play on the last frame restarts from the top rather than
        // doing nothing, which is what every video player does.
        if self.playback.playing && self.frame == self.doc.last_frame() {
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
            if self.frame < self.doc.last_frame() {
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
    pub fn place_symbol(&mut self, symbol: SymbolId) {
        let center = self.doc.stage_rect().center();
        self.selection = self.doc.add_instance_layer(symbol, self.frame, center);
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
