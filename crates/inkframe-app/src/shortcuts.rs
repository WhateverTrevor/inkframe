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
/// Animate's Insert › Timeline › Layer has no key; ⌘⇧N is Photoshop's.
pub const NEW_LAYER: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::N);
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
pub const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::A);
pub const DESELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::A);
// Photoshop's selection and layer shortcuts, for painting. (Merge Down's
// ⌘E is Export here, so it has none.)
pub const INVERT_SELECTION: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::I);
pub const LAYER_VIA_COPY: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::J);
pub const LAYER_VIA_CUT: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::J);

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
/// Animate's Break Apart.
pub const BREAK_APART: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::B);
pub const INSERT_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F6);
// Animate's frame keys (see `frame_commands.rs`).
pub const INSERT_FRAMES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F5);
pub const REMOVE_FRAMES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F5);
pub const INSERT_BLANK_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F7);
pub const REMOVE_KEYFRAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F6);

// Trimming a layer to the playhead uses After Effects' shortcuts, since
// Animate has no direct equivalent: Alt/Option + [ and ].
pub const LAYER_START_HERE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::OpenBracket);
pub const LAYER_END_HERE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::CloseBracket);

/// Animate's shortcut for adding a shape hint.
pub const ADD_SHAPE_HINT: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::H);

// Flipping a layer through its drawings (mouth shapes, hand poses) from the
// current frame on: plain [ and ], as in Toon Boom Harmony's drawing
// substitution.
pub const PREVIOUS_DRAWING: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::OpenBracket);
pub const NEXT_DRAWING: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::CloseBracket);

/// Single-key tool shortcuts, matching Animate (and, for painting,
/// Photoshop) so muscle memory carries over. Tools sharing a toolbar slot
/// share its key; Shift with it steps through them.
pub fn tool_key(tool: Tool) -> Key {
    match tool {
        Tool::PaintBucket | Tool::Gradient => Key::G,
        // Photoshop has no key for these by default; U is free and near.
        Tool::Smudge | Tool::Blur | Tool::Sharpen => Key::U,
        // O, Photoshop's, is the Oval here.
        Tool::Dodge | Tool::Burn => Key::D,
        Tool::CloneStamp => Key::S,
        Tool::RectMarquee | Tool::EllipseMarquee => Key::M,
        Tool::Lasso | Tool::PolygonLasso => Key::L,
        Tool::MagicWand => Key::W,
        Tool::Select => Key::V,
        Tool::Subselect | Tool::Width => Key::A,
        Tool::Pen => Key::P,
        Tool::Rectangle => Key::R,
        Tool::Ellipse => Key::O,
        Tool::Pencil => Key::Y,
        Tool::Text => Key::T,
        // Photoshop's keys for its painting tools; Bone, which had B
        // before painting arrived, takes K.
        Tool::Bone => Key::K,
        // Animate's.
        Tool::Camera => Key::C,
        Tool::Brush => Key::B,
        Tool::Eraser => Key::E,
        Tool::Eyedropper => Key::I,
    }
}
