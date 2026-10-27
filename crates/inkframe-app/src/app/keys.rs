//! Keyboard shortcuts: turning the keys pressed this frame into commands.

use egui::{Key, Modifiers};
use inkframe_core::{Clipboard, TimelineId};

use super::{Command, InkframeApp};
use crate::editor::Tool;
use crate::shortcuts;

impl InkframeApp {
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use shortcuts::*;
        // Find a Command has the keyboard while it's open.
        if self.finder.is_open() {
            return;
        }

        // `consume_shortcut` ignores extra Shift, so ⌘⇧S would also match ⌘S.
        // Check the shifted variants first so they win.
        let file_shortcuts = [
            (SAVE_AS, Command::SaveAs),
            (SAVE, Command::Save),
            (OPEN, Command::Open),
            // Before ⌘N, which would otherwise match it too.
            (NEW_LAYER, Command::NewLayer),
            (NEW, Command::New),
            (EXPORT, Command::Export),
            (IMPORT, Command::ImportImage),
            (FIND_COMMAND, Command::OpenFinder),
        ];
        let editing_shortcuts = [
            (REDO, Command::Redo),
            (UNDO, Command::Undo),
            (REDO_ALT, Command::Redo),
            (DUPLICATE, Command::DuplicateLayer),
            // Deselect (⌘⇧A) before Select All (⌘A), since shortcuts
            // ignore extra Shift.
            (DESELECT_ALL, Command::Deselect),
            (SELECT_ALL, Command::SelectAll),
            (INVERT_SELECTION, Command::InvertSelection),
            (LAYER_VIA_CUT, Command::LayerViaCut),
            (LAYER_VIA_COPY, Command::LayerViaCopy),
            (CONVERT_TO_SYMBOL, Command::ConvertToSymbol),
            (BREAK_APART, Command::BreakApart),
            // The modified ones first: shortcuts ignore extra Shift and Alt.
            (RULERS, Command::ToggleRulers),
            (RIG_SETUP, Command::ToggleRigSetup),
            (SNAP_TO_GUIDES, Command::ToggleSnapToGuides),
            (LOCK_GUIDES, Command::ToggleLockGuides),
            (SHOW_GUIDES, Command::ToggleGuides),
            (ADD_SHAPE_HINT, Command::AddShapeHint),
            (ZOOM_FIT, Command::ZoomFit),
            (ZOOM_100, Command::ZoomActualSize),
            (ZOOM_IN, Command::ZoomIn),
            (ZOOM_OUT, Command::ZoomOut),
            (REMOVE_KEYFRAME, Command::RemoveKeyframe),
            // Shift+F5 before F5, since shortcuts ignore extra Shift.
            (REMOVE_FRAMES, Command::RemoveFrames),
            (INSERT_FRAMES, Command::InsertFrames),
            (INSERT_BLANK_KEYFRAME, Command::InsertBlankKeyframe),
            (LAYER_START_HERE, Command::LayerStartHere),
            (LAYER_END_HERE, Command::LayerEndHere),
            // After the ⌥ versions above, so those don't also swap; the
            // Shift ones first, since shortcuts ignore extra Shift.
            (SOFTER_BRUSH, Command::SofterBrush),
            (HARDER_BRUSH, Command::HarderBrush),
            (SOFTER_BRUSH_CURLY, Command::SofterBrush),
            (HARDER_BRUSH_CURLY, Command::HarderBrush),
            (PREVIOUS_DRAWING, Command::PreviousDrawing),
            (NEXT_DRAWING, Command::NextDrawing),
            (INSERT_KEYFRAME, Command::InsertKeyframe),
            (PLAY, Command::PlayPause),
        ];

        let mut commands = Vec::new();
        let mut pasted_text = None;
        // While a text field has focus, keys like ⌘Z and Delete belong to
        // the text field. File shortcuts still work everywhere.
        let typing = ctx.egui_wants_keyboard_input();
        let drawing_path = self.views.stage().is_drawing_path();
        let anchor_selected = self.views.stage().has_anchor_selection();
        let in_symbol = self.editor.timeline() != TimelineId::Main;
        let inside_layer = self.editor.inside_layer().is_some();
        let shapes_picked = !self.editor.picked_shapes().is_empty();
        let bending = self.editor.lattice.is_some();
        let correcting = self.editor.correcting.is_some();
        let bone_picked = self.editor.tool == Tool::AddBone && self.editor.selected_bone.is_some();
        ctx.input_mut(|input| {
            for (shortcut, command) in file_shortcuts {
                if input.consume_shortcut(&shortcut) {
                    commands.push(command);
                }
            }
            if typing {
                return;
            }
            // Copy, cut and paste come from the system as clipboard events,
            // not keys. With frames picked in the timeline they act on those
            // frames' keyframes, as Animate's do; ⌥ copies just the pose on
            // this frame, and ⇧ pastes in place.
            let frames_picked = self.editor.frame_range.is_some();
            for event in &input.events {
                let (alt, shift) = (input.modifiers.alt, input.modifiers.shift);
                match event {
                    egui::Event::Copy => commands.push(match (alt, frames_picked) {
                        (true, _) => Command::CopyPose,
                        (false, true) => Command::CopyFrames,
                        (false, false) => Command::Copy,
                    }),
                    egui::Event::Cut => {
                        commands.push(if frames_picked { Command::CutFrames } else { Command::Cut })
                    }
                    egui::Event::Paste(text) => match Clipboard::from_text(text) {
                        // Inkframe data, perhaps from another window: paste
                        // that, and remember it for the Edit menu.
                        Some(clipboard) => {
                            let command = match (&clipboard, shift) {
                                (Clipboard::Keyframe(_), _) => Command::PastePose,
                                (Clipboard::Frames(_), _) => Command::PasteFrames,
                                (Clipboard::Layers(_) | Clipboard::Shapes(_), true) => Command::PasteInPlace,
                                (Clipboard::Layers(_) | Clipboard::Shapes(_), false) => Command::Paste,
                            };
                            self.clipboard = Some(clipboard);
                            commands.push(command);
                        }
                        // Text from another app becomes a text layer.
                        None => pasted_text = Some(text.clone()),
                    },
                    _ => {}
                }
            }
            // Some keys mean something different while the stage is busy:
            // with a pen path in progress, Enter and Escape finish it and
            // Backspace takes back a point; with an anchor picked, Backspace
            // deletes that point rather than the whole layer. Checked first
            // so they win over the usual meanings (play, delete layer).
            let stage_keys: &[(Key, Command)] = if drawing_path {
                &[
                    (Key::Enter, Command::FinishPath),
                    (Key::Escape, Command::FinishPath),
                    (Key::Backspace, Command::RemovePathPoint),
                    (Key::Delete, Command::RemovePathPoint),
                ]
            } else if correcting && !shapes_picked && !anchor_selected {
                // Enter keys the correction being drawn; Escape drops it.
                &[(Key::Enter, Command::FinishCorrection), (Key::Escape, Command::CancelCorrection)]
            } else if bending {
                // Enter or Escape leaves a Distort or Envelope's shapes bent.
                &[(Key::Enter, Command::FinishLattice), (Key::Escape, Command::FinishLattice)]
            } else if bone_picked && self.editor.previewing.is_some() {
                // Escape puts a bone previewed at another turn back.
                &[
                    (Key::Backspace, Command::DeleteBone),
                    (Key::Delete, Command::DeleteBone),
                    (Key::Escape, Command::EndPreview),
                ]
            } else if bone_picked {
                &[(Key::Backspace, Command::DeleteBone), (Key::Delete, Command::DeleteBone)]
            } else if self.editor.previewing.is_some() {
                &[(Key::Escape, Command::EndPreview)]
            } else if anchor_selected {
                &[(Key::Backspace, Command::DeleteAnchor), (Key::Delete, Command::DeleteAnchor)]
            } else if shapes_picked {
                // Inside a layer, Delete takes out the picked shapes, not
                // the layer, and Escape steps back out.
                &[
                    (Key::Backspace, Command::DeleteShapes),
                    (Key::Delete, Command::DeleteShapes),
                    (Key::Escape, Command::ExitLayer),
                ]
            } else if inside_layer {
                &[(Key::Escape, Command::ExitLayer)]
            } else if in_symbol {
                // Escape steps back out of a symbol, as in Animate.
                &[(Key::Escape, Command::ExitSymbol)]
            } else {
                &[]
            };
            for &(key, command) in stage_keys {
                if input.consume_key(Modifiers::NONE, key) {
                    commands.push(command);
                }
            }
            for (shortcut, command) in editing_shortcuts {
                if input.consume_shortcut(&shortcut) {
                    commands.push(command);
                }
            }

            let plain_keys = [
                (Key::Comma, Command::StepBackward),
                (Key::ArrowLeft, Command::StepBackward),
                (Key::Period, Command::StepForward),
                (Key::ArrowRight, Command::StepForward),
                (Key::Home, Command::FirstFrame),
                (Key::End, Command::LastFrame),
                (Key::Delete, Command::DeleteLayer),
                // Krita's keys for turning the view: 4 and 6 either way, 5
                // back upright.
                (Key::Num4, Command::RotateViewLeft),
                (Key::Num6, Command::RotateViewRight),
                (Key::Num5, Command::ResetRotation),
                (Key::Backspace, Command::DeleteLayer),
            ];
            // Ctrl+Space: the area under the pointer fills the window, or
            // goes back, as in Blender.
            if input.consume_key(Modifiers::CTRL, Key::Space) {
                commands.push(Command::ToggleMaximize);
            }
            for (key, command) in plain_keys {
                if input.consume_key(Modifiers::NONE, key) {
                    commands.push(command);
                }
            }
            for group in Tool::GROUPS {
                let key = tool_key(group[0]);
                if input.consume_key(Modifiers::SHIFT, key) {
                    commands.push(Command::UseSlot { group, cycle: true });
                } else if input.consume_key(Modifiers::NONE, key) {
                    commands.push(Command::UseSlot { group, cycle: false });
                }
            }
        });

        for command in commands {
            self.execute(command);
        }
        if let Some(text) = pasted_text {
            let center = self.views.stage().visible_center(&self.editor);
            self.editor.paste_text(&text, center);
        }
    }
}
