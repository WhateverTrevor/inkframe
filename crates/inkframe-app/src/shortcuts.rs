//! Keyboard shortcuts, collected in one place so menus can display them and
//! contributors can find (and remap) them easily.
//!
//! `Modifiers::COMMAND` is ⌘ on macOS and Ctrl on Windows and Linux.

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::editor::Tool;

const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
const CMD_ALT: Modifiers = Modifiers::COMMAND.plus(Modifiers::ALT);

pub const NEW: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::N);
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::O);
pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::S);
pub const SAVE_AS: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::S);
pub const EXPORT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::E);
/// Animate's File › Import › Import to Stage.
pub const IMPORT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::R);

pub const UNDO: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Z);
pub const REDO: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::Z);
/// Windows users expect Ctrl+Y for redo, so accept it everywhere.
pub const REDO_ALT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Y);
pub const DUPLICATE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::D);

// Copy and paste arrive from the system as clipboard events rather than
// keys (see `InkframeApp::handle_shortcuts`); these are only for showing in
// menus. Animate's keys: ⌘V pastes in the center of the view, ⌘⇧V in place,
// and ⌥ copies and pastes keyframes instead of layers.
pub const CUT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::X);
pub const COPY: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::C);
pub const PASTE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::V);
pub const PASTE_IN_PLACE: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::V);
pub const COPY_FRAMES: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::C);
pub const PASTE_FRAMES: KeyboardShortcut = KeyboardShortcut::new(CMD_ALT, Key::V);
/// F8, as in Animate.
pub const CONVERT_TO_SYMBOL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F8);

pub const ZOOM_FIT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Num0);
pub const ZOOM_100: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Num1);
pub const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Equals);
pub const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Minus);

// Animation shortcuts follow Adobe Animate: Enter plays, F6 inserts a
// keyframe, Shift+F6 clears it, and comma/period step through frames.
pub const PLAY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Enter);
pub const INSERT_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F6);
pub const REMOVE_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F6);

// Trimming a layer to the playhead uses After Effects' shortcuts, since
// Animate has no direct equivalent: Alt/Option + [ and ].
pub const LAYER_START_HERE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::OpenBracket);
pub const LAYER_END_HERE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::CloseBracket);

/// Single-key tool shortcuts, matching Animate so muscle memory carries over.
pub fn tool_key(tool: Tool) -> Key {
    match tool {
        Tool::Select => Key::V,
        Tool::Subselect => Key::A,
        Tool::Pen => Key::P,
        Tool::Rectangle => Key::R,
        Tool::Ellipse => Key::O,
        Tool::Pencil => Key::Y,
        Tool::Text => Key::T,
    }
}
