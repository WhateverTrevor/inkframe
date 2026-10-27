//! Carrying out a command, from a menu, a shortcut or a panel.

use inkframe_core::{Clipboard, Document};

use super::{Command, InkframeApp, show_error};
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::layout::{Kind, Layout};

impl InkframeApp {
    pub(super) fn execute(&mut self, command: Command) {
        let editor = &mut self.editor;
        match command {
            Command::OpenFinder => self.finder.open(),
            Command::UseTool(tool) => editor.use_tool(tool),
            Command::New => {
                if self.confirm_discard_changes() {
                    self.replace_editor(Editor::new(Document::default()));
                }
            }
            Command::Open => self.open(),
            Command::ImportImage => self.import_image(),
            Command::ImportAudio => self.import_audio(),
            Command::ImportFont => self.import_fonts(),
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
            Command::Export => self.export.open(&self.editor.doc),
            Command::BatchExport => self.batch.open(),
            Command::OpenExample => self.examples.open(&self.ctx),
            Command::StartTour => self.tour.start(),
            Command::WhatsNew => {
                if !self.whats_new.open() {
                    show_error(tr("What's New"), tr("This build has no notes yet: it's between releases."));
                }
            }
            Command::Vectorise => self.vectorise.open(&self.editor),
            Command::ExportPsd => self.export_psd(),
            Command::ToggleMaximize => {
                if let Some(area) = self.hovered_area {
                    self.layout.toggle_maximized(area);
                }
            }
            Command::ResetLayout => self.layout = Layout::workspace(self.layout.workspace_name()),
            Command::Workspace(workspace) => self.layout = Layout::workspace(workspace),
            // With a painting or selection tool, Select All and Deselect
            // work on pixels, as in Photoshop; otherwise on layers.
            Command::SelectAll if editor.tool.on_pixels() && editor.paint_target().is_some() => {
                editor.select_all_pixels()
            }
            Command::Deselect if editor.tool.on_pixels() => editor.deselect_pixels(),
            // Inside a layer, Select All picks every shape in it.
            Command::SelectAll if editor.inside_layer().is_some() => editor.pick_all_shapes(),
            Command::SelectAll => editor.select_all(),
            Command::Deselect => editor.select(None),
            Command::InvertSelection => editor.invert_pixel_selection(),
            Command::LayerViaCopy => editor.layer_via_copy(false),
            Command::LayerViaCut => editor.layer_via_copy(true),
            Command::MergeDown => {
                editor.merge_down();
            }
            Command::Adjust(adjustment) => self.adjust.open(editor, adjustment),
            Command::Cut => {
                let copied = self.editor.copy_selection();
                self.put_on_clipboard(copied);
                // Inside a layer with shapes picked, they're what was cut.
                if !self.editor.delete_shapes() {
                    self.editor.delete_selected_layer();
                }
            }
            Command::Copy => {
                let copied = self.editor.copy_selection();
                self.put_on_clipboard(copied);
            }
            Command::CopyPose => {
                let copied = self.editor.copy_keyframe();
                self.put_on_clipboard(copied);
            }
            Command::CopyFrames => {
                let copied = self.editor.copy_frames();
                self.put_on_clipboard(copied);
            }
            Command::CutFrames => {
                let cut = self.editor.cut_frames();
                self.put_on_clipboard(cut);
            }
            Command::Paste => {
                let center = self.views.stage().visible_center(&self.editor);
                if let Some(clipboard) = &self.clipboard {
                    self.editor.paste(clipboard, Some(center));
                }
            }
            // Paste Frames pastes copied frames only, never layers copied since.
            Command::PasteFrames => {
                if let Some(Clipboard::Frames(frames)) = &self.clipboard {
                    self.editor.paste_frames(frames);
                }
            }
            Command::PasteInPlace | Command::PastePose => {
                if let Some(clipboard) = &self.clipboard {
                    self.editor.paste(clipboard, None);
                }
            }
            Command::Undo => editor.undo(),
            Command::Redo => editor.redo(),
            Command::DuplicateLayer => editor.duplicate_selected_layer(),
            // Delete with selected pixels clears them, not the whole layer.
            Command::DeleteLayer if editor.active_selection().is_some() => {
                editor.clear_selected_pixels();
            }
            Command::DeleteLayer => editor.delete_selected_layer(),
            // Inside a layer with shapes picked, F8 makes a symbol of just them.
            Command::ConvertToSymbol if !editor.picked_shapes().is_empty() => {
                if let Err(problem) = editor.shapes_to_symbol() {
                    show_error(tr("Couldn't make a symbol"), &problem);
                }
            }
            Command::ConvertToSymbol => {
                if let Err(problem) = editor.convert_selected_to_symbol() {
                    show_error(tr("Couldn't make a symbol"), &problem);
                }
            }
            Command::MaskLayerBelow => {
                if let Some(id) = editor.selection {
                    editor.doc.mask_layer_below(id);
                }
            }
            Command::ToggleClipping => editor.toggle_clipping(),
            Command::AddLayerMask => {
                editor.add_layer_mask();
            }
            Command::DeleteLayerMask => editor.delete_layer_mask(),
            Command::ApplyLayerMask => {
                editor.apply_layer_mask();
            }
            Command::ToggleLayerMask => editor.toggle_layer_mask(),
            Command::EditLayerMask => {
                if let Some(id) = editor.selection {
                    let painting_mask = editor.painting_mask();
                    editor.set_editing_mask(id, !painting_mask);
                }
            }
            Command::EditSymbol => {
                if let Some(id) = editor.selection {
                    editor.enter_instance(id);
                }
            }
            Command::ExitSymbol => editor.exit_symbol(),
            Command::InsertKeyframe => editor.insert_keyframe(),
            Command::RemoveKeyframe => editor.remove_keyframe(),
            Command::LayerStartHere => editor.set_layer_start_here(),
            Command::LayerEndHere => editor.set_layer_end_here(),
            Command::NewDrawing => editor.new_drawing(false),
            Command::DuplicateDrawing => editor.new_drawing(true),
            Command::DeleteDrawing => editor.delete_shown_drawing(),
            // While painting, [ and ] change the brush size instead, as in
            // Photoshop.
            Command::PreviousDrawing if editor.tool.uses_brush() => editor.resize_brush(1.0 / BRUSH_STEP),
            Command::NextDrawing if editor.tool.uses_brush() => editor.resize_brush(BRUSH_STEP),
            Command::SofterBrush => editor.harden_brush(-0.25),
            Command::HarderBrush => editor.harden_brush(0.25),
            Command::DefineBrush => match editor.define_brush() {
                Ok(_) => self.layout.reveal(Kind::Brushes),
                Err(problem) => show_error(tr("Couldn't define a brush"), &problem),
            },
            Command::DefinePattern => {
                if let Err(problem) = editor.define_pattern() {
                    show_error(tr("Couldn't define a pattern"), &problem);
                }
            }
            Command::ShowBrushSettings => self.layout.reveal(Kind::BrushSettings),
            Command::ShowBrushes => self.layout.reveal(Kind::Brushes),
            Command::PreviousDrawing => editor.cycle_drawing(-1),
            Command::NextDrawing => editor.cycle_drawing(1),
            Command::CombineShapes(how) => {
                if let Err(problem) = editor.combine_shapes(how) {
                    show_error(tr("Couldn't combine the shapes"), &problem);
                }
            }
            Command::SwapSymbol => self.swap.open(editor),
            Command::ToggleRulers => editor.show_rulers = !editor.show_rulers,
            Command::ToggleRigSetup => editor.rig_setup = !editor.rig_setup,
            Command::SetPerspective(kind) => {
                let (w, h) = (editor.doc.width as f32, editor.doc.height as f32);
                editor.doc.perspective = match (kind, editor.doc.perspective.take()) {
                    (None, _) => None,
                    (Some(kind), Some(mut guide)) => {
                        guide.set_kind(kind, w, h);
                        Some(guide)
                    }
                    (Some(kind), None) => Some(inkframe_core::Perspective::new(kind, w, h)),
                };
            }
            Command::ToggleGuides => editor.show_guides = !editor.show_guides,
            Command::ToggleLockGuides => editor.lock_guides = !editor.lock_guides,
            Command::ToggleSnapToGuides => editor.snap_to_guides = !editor.snap_to_guides,
            Command::ClearGuides => editor.doc.ruler_guides.clear(),
            Command::StartLattice(kind) => {
                if let Err(problem) = editor.start_lattice(kind) {
                    show_error(&tr_args("Couldn't start {what}", &[("what", tr(kind.name()))]), &problem);
                }
            }
            Command::FinishLattice => editor.finish_lattice(),
            Command::NewSkeleton => editor.new_skeleton(),
            Command::FinishCorrection => editor.finish_correction(),
            Command::CancelCorrection => editor.cancel_correction(),
            Command::EndPreview => editor.end_preview(),
            Command::NewSwitch => editor.new_switch(),
            Command::DeleteBone => {
                editor.delete_bone();
            }
            Command::BreakApart => {
                if editor.break_apart() == 0 {
                    show_error(
                        tr("Nothing to break apart"),
                        tr(
                            "Break Apart turns text into a shape for each letter, and pencil lines into filled shapes. \
                         Select a text layer, or double-click a layer and pick its text or lines.",
                        ),
                    );
                }
            }
            Command::InsertFrames => editor.insert_frames(),
            Command::RemoveFrames => editor.remove_frames(),
            Command::InsertBlankKeyframe => editor.insert_blank_keyframe(),
            Command::ConvertToKeyframes => editor.convert_to_keyframes(false),
            Command::ConvertToBlankKeyframes => editor.convert_to_keyframes(true),
            Command::ReverseFrames => {
                if !editor.reverse_frames() {
                    show_error(
                        tr("Pick some frames first"),
                        tr(
                            "Reverse Frames plays a stretch backwards. Shift-click or Shift-drag along a layer's row in \
                         the timeline to pick the frames.",
                        ),
                    );
                }
            }
            Command::NewLayer => {
                editor.add_empty_layer();
            }
            Command::NewPaintLayer => {
                editor.add_paint_layer();
            }
            Command::NewParticleLayer(preset) => {
                editor.add_particle_layer(preset);
            }
            Command::NewFolder => editor.new_folder(),
            Command::CreateShapeTween => editor.set_shape_tween(true),
            Command::MakeInbetweens => {
                if editor.make_inbetweens() == 0 {
                    show_error(
                        tr("No inbetweens to draw"),
                        tr(
                            "Inbetweens are drawn between two key drawings: put the playhead between a drawing and the next one it swaps to, with frames to fill between them.",
                        ),
                    );
                }
            }
            Command::AddCamera => editor.add_camera(),
            Command::RemoveShapeTween => editor.set_shape_tween(false),
            Command::AddShapeHint => editor.add_shape_hint(),
            Command::RemoveShapeHints => editor.clear_shape_hints(),
            Command::LipSync => self.lip_sync.open(editor),
            Command::PlayPause => editor.toggle_playback(),
            Command::StepBackward => editor.step(-1),
            Command::StepForward => editor.step(1),
            Command::FirstFrame => editor.set_frame(0),
            Command::LastFrame => editor.set_frame(editor.last_frame()),
            Command::ZoomIn => self.views.stage().zoom_by(&editor.doc, 1.25),
            Command::ZoomOut => self.views.stage().zoom_by(&editor.doc, 0.8),
            Command::ZoomFit => self.views.stage().zoom_to_fit(),
            Command::ZoomActualSize => self.views.stage().zoom_to_actual_size(&editor.doc),
            Command::RotateViewLeft => self.views.stage().rotate_view(-15.0),
            Command::RotateViewRight => self.views.stage().rotate_view(15.0),
            Command::ResetRotation => self.views.stage().reset_rotation(),
            Command::UseSlot { group, cycle } => editor.use_slot(group, cycle),
            Command::FinishPath => self.views.stage().finish_path(editor, false),
            Command::RemovePathPoint => self.views.stage().remove_last_path_point(),
            Command::DeleteAnchor => self.views.stage().delete_selected_anchor(editor),
            Command::ExitLayer => editor.exit_layer(),
            Command::DeleteShapes => {
                editor.delete_shapes();
            }
        }
    }

    /// Puts what was copied on the system clipboard (so it can be pasted
    /// into another window) and on our own.
    pub(super) fn put_on_clipboard(&mut self, copied: Option<Clipboard>) {
        if let Some(clipboard) = copied {
            self.ctx.copy_text(clipboard.to_text());
            self.clipboard = Some(clipboard);
        }
    }
}

/// How much `[` and `]` shrink or grow the brush.
pub(super) const BRUSH_STEP: f32 = 1.2;
