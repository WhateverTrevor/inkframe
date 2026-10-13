//! Keyboard shortcuts, collected in one place so menus can display them and
//! contributors can find (and remap) them easily.
//!
//! `Modifiers::COMMAND` is ⌘ on macOS and Ctrl on Windows and Linux.

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::editor::Tool;

const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

pub const NEW: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::N);
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::O);
pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::S);
pub const SAVE_AS: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::S);
pub const EXPORT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::E);

pub const UNDO: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Z);
pub const REDO: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::Z);
/// Windows users expect Ctrl+Y for redo, so accept it everywhere.
pub const REDO_ALT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Y);
pub const DUPLICATE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::D);

pub const ZOOM_FIT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Num0);
pub const ZOOM_100: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Num1);
pub const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Equals);
pub const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::Minus);

// Animation shortcuts follow Adobe Animate: Enter plays, F6 inserts a
// keyframe, Shift+F6 clears it, and comma/period step through frames.
pub const PLAY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Enter);
pub const INSERT_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F6);
pub const REMOVE_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F6);

/// Single-key tool shortcuts, matching Animate so muscle memory carries over.
pub fn tool_key(tool: Tool) -> Key {
    match tool {
        Tool::Select => Key::V,
        Tool::Subselect => Key::A,
        Tool::Pen => Key::P,
        Tool::Rectangle => Key::R,
        Tool::Ellipse => Key::O,
        Tool::Pencil => Key::Y,
    }
}
