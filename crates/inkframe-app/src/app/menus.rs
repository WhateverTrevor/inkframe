//! The menu bar: building each menu from the commands and the settings
//! they show, and the View › Language menu.

use inkframe_core::{Clipboard, TimelineId};

use super::{Command, GUIDEBOOK_URL, InkframeApp, open_in_browser};
use crate::adjust_dialog::AdjustmentKind;
use crate::guides::SafeAreas;
use crate::i18n::{tr, tr_args};
use crate::layout::Workspace;
use crate::shortcuts;
use crate::swap_dialog::SwapDialog;
use crate::vectorise_dialog::VectoriseDialog;

impl InkframeApp {
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        // The View menu's toggles, kept apart from `chosen`, which `item`
        // holds onto for the whole menu bar.
        let mut toggled = None;
        egui::MenuBar::new().ui(ui, |ui| {
            let ctx = ui.ctx().clone();
            let mut item = |ui: &mut egui::Ui,
                            label: &str,
                            shortcut: Option<egui::KeyboardShortcut>,
                            command,
                            enabled: bool| {
                let mut button = egui::Button::new(label);
                if let Some(shortcut) = shortcut {
                    button = button.shortcut_text(ctx.format_shortcut(&shortcut));
                }
                if ui.add_enabled(enabled, button).clicked() {
                    chosen = Some(command);
                    ui.close();
                }
            };
            let has_selection = self.editor.selection.is_some();

            ui.menu_button(tr("File"), |ui| {
                item(ui, tr("New"), Some(shortcuts::NEW), Command::New, true);
                item(ui, tr("Open…"), Some(shortcuts::OPEN), Command::Open, true);
                item(ui, tr("Open Example…"), None, Command::OpenExample, true);
                item(ui, tr("Import to Stage…"), Some(shortcuts::IMPORT), Command::ImportImage, true);
                item(ui, tr("Import Sprite Sheet…"), None, Command::ImportSpriteSheet, true);
                item(ui, tr("Import Audio…"), None, Command::ImportAudio, true);
                item(ui, tr("Import Font…"), None, Command::ImportFont, true);
                ui.separator();
                item(ui, tr("Save"), Some(shortcuts::SAVE), Command::Save, true);
                item(ui, tr("Save As…"), Some(shortcuts::SAVE_AS), Command::SaveAs, true);
                ui.separator();
                item(ui, tr("Export…"), Some(shortcuts::EXPORT), Command::Export, true);
                item(ui, tr("Export PSD…"), None, Command::ExportPsd, true);
                item(ui, tr("Batch Export…"), None, Command::BatchExport, true);
            });
            ui.menu_button(tr("Edit"), |ui| {
                // Named for what they'd undo and redo, as the History panel names it.
                let (undo, redo) = self.editor.history.next_labels();
                let undo = undo.map_or_else(|| tr("Undo").to_owned(), |l| tr_args("Undo {what}", &[("what", l)]));
                let redo = redo.map_or_else(|| tr("Redo").to_owned(), |l| tr_args("Redo {what}", &[("what", l)]));
                item(ui, &undo, Some(shortcuts::UNDO), Command::Undo, self.editor.history.can_undo());
                item(ui, &redo, Some(shortcuts::REDO), Command::Redo, self.editor.history.can_redo());
                ui.separator();
                item(
                    ui,
                    tr("Duplicate Layer"),
                    Some(shortcuts::DUPLICATE),
                    Command::DuplicateLayer,
                    has_selection,
                );
                item(ui, tr("Delete Layer"), None, Command::DeleteLayer, has_selection);
                ui.separator();
                let layers = matches!(self.clipboard, Some(Clipboard::Layers(_) | Clipboard::Shapes(_)));
                let keyframe = matches!(self.clipboard, Some(Clipboard::Keyframe(_)));
                item(ui, tr("Select All"), Some(shortcuts::SELECT_ALL), Command::SelectAll, true);
                item(ui, tr("Deselect All"), Some(shortcuts::DESELECT_ALL), Command::Deselect, has_selection);
                ui.separator();
                item(ui, tr("Cut"), Some(shortcuts::CUT), Command::Cut, has_selection);
                item(ui, tr("Copy"), Some(shortcuts::COPY), Command::Copy, has_selection);
                item(ui, tr("Paste in Center"), Some(shortcuts::PASTE), Command::Paste, layers);
                item(ui, tr("Paste in Place"), Some(shortcuts::PASTE_IN_PLACE), Command::PasteInPlace, layers);
                ui.separator();
                item(ui, tr("Copy Pose"), Some(shortcuts::COPY_POSE), Command::CopyPose, has_selection);
                item(ui, tr("Paste Pose"), Some(shortcuts::PASTE_POSE), Command::PastePose, keyframe && has_selection);
                ui.separator();
                let painting = self.editor.paint_target().is_some();
                item(ui, tr("Define Brush Preset…"), None, Command::DefineBrush, painting);
                item(ui, tr("Define Pattern…"), None, Command::DefinePattern, painting);
                item(ui, tr("Brushes"), None, Command::ShowBrushes, true);
                item(ui, tr("Brush Settings"), None, Command::ShowBrushSettings, true);
            });
            ui.menu_button(tr("Modify"), |ui| {
                item(
                    ui,
                    tr("Convert to Symbol"),
                    Some(shortcuts::CONVERT_TO_SYMBOL),
                    Command::ConvertToSymbol,
                    has_selection,
                );
                // On shapes picked inside a layer (see `inside.rs`).
                let picked = self.editor.picked_shapes().len();
                ui.menu_button(tr("Combine Shapes"), |ui| {
                    use inkframe_core::ShapeOp;
                    for (label, how) in [
                        (tr("Union"), ShapeOp::Union),
                        (tr("Intersect"), ShapeOp::Intersect),
                        (tr("Punch"), ShapeOp::Punch),
                        (tr("Crop"), ShapeOp::Crop),
                    ] {
                        item(ui, label, None, Command::CombineShapes(how), picked >= 2);
                    }
                    if picked < 2 {
                        ui.weak(tr("Double-click a layer, then pick two or more of its shapes."));
                    }
                });
                item(ui, tr("Break Apart"), Some(shortcuts::BREAK_APART), Command::BreakApart, has_selection);
                ui.menu_button(tr("Transform"), |ui| {
                    use crate::envelope::LatticeKind;
                    let bendable = has_selection && self.editor.lattice.is_none();
                    for kind in [LatticeKind::Distort, LatticeKind::Envelope] {
                        item(ui, tr(kind.name()), None, Command::StartLattice(kind), bendable);
                    }
                    ui.weak(tr("On the picked shapes, or everything in the selected layer."));
                });
                ui.separator();
                let instance_selected = self.editor.selected_layer().is_some_and(|l| l.instance.is_some());
                item(ui, tr("Edit Symbol"), None, Command::EditSymbol, instance_selected);
                item(ui, tr("Swap Symbol…"), None, Command::SwapSymbol, SwapDialog::can_open(&self.editor));
                let in_symbol = self.editor.timeline() != TimelineId::Main;
                item(ui, tr("Exit Symbol"), None, Command::ExitSymbol, in_symbol);
                ui.separator();
                // Only offered when there's a layer below to mask.
                let below = self
                    .editor
                    .selection
                    .is_some_and(|id| self.editor.layers().first().is_some_and(|bottom| bottom.id != id));
                item(ui, tr("Mask"), None, Command::MaskLayerBelow, below);
                let clipped = self.editor.selected_layer().is_some_and(|l| l.clipped);
                let clipping = if clipped { tr("Release Clipping Mask") } else { tr("Create Clipping Mask") };
                let can_clip = self.editor.can_toggle_clipping();
                item(ui, clipping, Some(shortcuts::CLIPPING_MASK), Command::ToggleClipping, can_clip);
                ui.menu_button(tr("Layer Mask"), |ui| {
                    let mask = self.editor.selected_mask();
                    let can_add = self.editor.selected_layer().is_some_and(|l| {
                        l.layer_mask.is_none() && !(l.folder || l.camera || l.guide || l.mask)
                    });
                    item(ui, tr("Add"), None, Command::AddLayerMask, can_add);
                    let paint = if self.editor.painting_mask() { tr("Paint the Layer") } else { tr("Paint the Mask") };
                    item(ui, paint, None, Command::EditLayerMask, mask.is_some());
                    let off = if mask.is_some_and(|m| !m.enabled) { tr("Turn On") } else { tr("Turn Off") };
                    item(ui, off, None, Command::ToggleLayerMask, mask.is_some());
                    // Only a painting can have the mask erased into it.
                    let painting = self.editor.selection.is_some_and(|id| {
                        self.editor.doc.paint_canvas_at(id, self.editor.frame).is_some()
                    });
                    item(ui, tr("Apply"), None, Command::ApplyLayerMask, mask.is_some() && painting);
                    item(ui, tr("Delete"), None, Command::DeleteLayerMask, mask.is_some());
                });
                ui.separator();
                let traceable = VectoriseDialog::can_vectorise(&self.editor);
                item(ui, tr("Vectorise…"), None, Command::Vectorise, traceable);
                item(ui, tr("New Layer"), Some(shortcuts::NEW_LAYER), Command::NewLayer, true);
                item(ui, tr("New Paint Layer"), None, Command::NewPaintLayer, true);
                ui.menu_button(tr("New Particle Layer"), |ui| {
                    for preset in inkframe_core::Preset::ALL {
                        item(ui, tr(preset.name()), None, Command::NewParticleLayer(preset), true);
                    }
                });
                item(ui, tr("New Folder"), None, Command::NewFolder, true);
                item(ui, tr("New Skeleton"), None, Command::NewSkeleton, true);
                item(ui, tr("New Switch"), None, Command::NewSwitch, true);
                ui.separator();
                let hints = self.editor.shape_tween_drawings().is_some();
                item(ui, tr("Add Shape Hint"), Some(shortcuts::ADD_SHAPE_HINT), Command::AddShapeHint, hints);
                item(ui, tr("Remove All Hints"), None, Command::RemoveShapeHints, hints);
                let below_is_paint = self.editor.selection.is_some_and(|id| {
                    let Some(timeline) = self.editor.doc.timeline_of(id) else { return false };
                    let layers = self.editor.doc.layers_of(timeline);
                    let index = layers.iter().position(|l| l.id == id).unwrap_or(0);
                    index > 0
                        && self.editor.doc.paint_canvas_at(layers[index - 1].id, self.editor.frame).is_some()
                });
                item(ui, tr("Merge Down"), None, Command::MergeDown, below_is_paint);
                let paint = self.editor.paint_target().is_some();
                ui.menu_button(tr("Adjustments"), |ui| {
                    for kind in AdjustmentKind::ALL {
                        let label = format!("{}…", tr(kind.neutral().name()));
                        item(ui, &label, None, Command::Adjust(kind), paint);
                    }
                });
                // Drawing substitution: only for layers with their own
                // artwork, not symbol instances.
                let drawn = self.editor.selected_layer().filter(|l| l.instance.is_none());
                let swaps = drawn.is_some_and(|l| l.drawings.len() > 1);
                item(ui, tr("New Drawing"), None, Command::NewDrawing, drawn.is_some());
                item(ui, tr("Duplicate Drawing"), None, Command::DuplicateDrawing, drawn.is_some());
                item(ui, tr("Delete Drawing"), None, Command::DeleteDrawing, swaps);
                item(
                    ui,
                    tr("Previous Drawing"),
                    Some(shortcuts::PREVIOUS_DRAWING),
                    Command::PreviousDrawing,
                    swaps,
                );
                item(ui, tr("Next Drawing"), Some(shortcuts::NEXT_DRAWING), Command::NextDrawing, swaps);
                item(ui, tr("Lip Sync…"), None, Command::LipSync, swaps);
            });
            ui.menu_button(tr("Select"), |ui| {
                let paint = self.editor.paint_target().is_some();
                let selected = self.editor.active_selection().is_some();
                item(ui, tr("All Pixels"), Some(shortcuts::SELECT_ALL), Command::SelectAll, paint);
                item(ui, tr("Deselect"), Some(shortcuts::DESELECT_ALL), Command::Deselect, selected);
                item(ui, tr("Inverse"), Some(shortcuts::INVERT_SELECTION), Command::InvertSelection, paint);
                ui.separator();
                item(ui, tr("Layer via Copy"), Some(shortcuts::LAYER_VIA_COPY), Command::LayerViaCopy, selected);
                item(ui, tr("Layer via Cut"), Some(shortcuts::LAYER_VIA_CUT), Command::LayerViaCut, selected);
                ui.separator();
                ui.weak(
                    tr("All Pixels and Deselect use ⌘A and ⌘⇧A while a\npainting or selection tool is in use."),
                );
            });
            ui.menu_button(tr("Timeline"), |ui| {
                let play = if self.editor.playback.playing { tr("Pause") } else { tr("Play") };
                item(ui, play, Some(shortcuts::PLAY), Command::PlayPause, true);
                ui.separator();
                item(
                    ui,
                    tr("Insert Keyframe"),
                    Some(shortcuts::INSERT_KEYFRAME),
                    Command::InsertKeyframe,
                    has_selection,
                );
                item(
                    ui,
                    tr("Remove Keyframe"),
                    Some(shortcuts::REMOVE_KEYFRAME),
                    Command::RemoveKeyframe,
                    has_selection,
                );
                ui.separator();
                // Animate's frame commands, on the picked frames (or the
                // current one); see `frame_commands.rs`.
                item(
                    ui,
                    tr("Insert Frames"),
                    Some(shortcuts::INSERT_FRAMES),
                    Command::InsertFrames,
                    has_selection,
                );
                item(
                    ui,
                    tr("Remove Frames"),
                    Some(shortcuts::REMOVE_FRAMES),
                    Command::RemoveFrames,
                    has_selection,
                );
                item(
                    ui,
                    tr("Insert Blank Keyframe"),
                    Some(shortcuts::INSERT_BLANK_KEYFRAME),
                    Command::InsertBlankKeyframe,
                    has_selection,
                );
                let picked = self.editor.frame_range.is_some();
                item(ui, tr("Convert to Keyframes"), None, Command::ConvertToKeyframes, has_selection && picked);
                item(
                    ui,
                    tr("Convert to Blank Keyframes"),
                    None,
                    Command::ConvertToBlankKeyframes,
                    has_selection && picked,
                );
                item(ui, tr("Reverse Frames"), None, Command::ReverseFrames, has_selection && picked);
                ui.separator();
                let frames_copied = matches!(self.clipboard, Some(Clipboard::Frames(_)));
                item(ui, tr("Cut Frames"), Some(shortcuts::CUT), Command::CutFrames, has_selection && picked);
                item(ui, tr("Copy Frames"), Some(shortcuts::COPY), Command::CopyFrames, has_selection && picked);
                item(ui, tr("Paste Frames"), Some(shortcuts::PASTE), Command::PasteFrames, has_selection && frames_copied);
                if ui
                    .checkbox(&mut self.editor.edit_multiple, tr("Edit Multiple Frames"))
                    .on_hover_text(
                        tr("Dragging on the stage moves layers across the picked frames (or all of them)"),
                    )
                    .clicked()
                {
                    ui.close();
                }
                ui.separator();
                item(
                    ui,
                    tr("Start Layer Here"),
                    Some(shortcuts::LAYER_START_HERE),
                    Command::LayerStartHere,
                    has_selection,
                );
                item(
                    ui,
                    tr("End Layer Here"),
                    Some(shortcuts::LAYER_END_HERE),
                    Command::LayerEndHere,
                    has_selection,
                );
                ui.separator();
                // Morphing into the next drawing swap; see `morph.rs`.
                let tween = self.editor.shape_tween_here();
                item(ui, tr("Create Shape Tween"), None, Command::CreateShapeTween, tween == Some(false));
                item(ui, tr("Remove Shape Tween"), None, Command::RemoveShapeTween, tween == Some(true));
                item(ui, tr("Draw Inbetweens"), None, Command::MakeInbetweens, tween.is_some());
                ui.separator();
                // What a Web Page export does on reaching this frame.
                let main = self.editor.timeline() == TimelineId::Main;
                ui.add_enabled_ui(main, |ui| {
                    ui.menu_button(tr("Frame Action"), |ui| {
                        let (frame, last) = (self.editor.frame, self.editor.last_frame());
                        let mut action = self.editor.doc.frame_action(frame).cloned();
                        let before = action.clone();
                        ui.horizontal(|ui| {
                            ui.label(tr_args("On frame {frame}:", &[("frame", &(frame + 1).to_string())]));
                            crate::inspector::web_action_editor(ui, "frame action", &mut action, last);
                        });
                        if action != before {
                            self.editor.doc.set_frame_action(frame, action);
                        }
                        ui.weak(tr("What a Web Page export does on reaching this frame,\nas Animate's stop() and gotoAndPlay()."));
                    });
                });
                ui.separator();
                let label =
                    if self.editor.doc.camera_layer().is_some() { tr("Select Camera") } else { tr("Add Camera") };
                item(ui, label, None, Command::AddCamera, true);
                ui.separator();
                item(ui, tr("First Frame"), None, Command::FirstFrame, true);
                item(ui, tr("Last Frame"), None, Command::LastFrame, true);
                ui.separator();
                ui.checkbox(&mut self.xsheet.open, tr("Exposure Sheet"));
            });
            ui.menu_button(tr("View"), |ui| {
                item(ui, tr("Zoom In"), Some(shortcuts::ZOOM_IN), Command::ZoomIn, true);
                item(ui, tr("Zoom Out"), Some(shortcuts::ZOOM_OUT), Command::ZoomOut, true);
                item(ui, tr("Fit Stage"), Some(shortcuts::ZOOM_FIT), Command::ZoomFit, true);
                item(ui, tr("Actual Size"), Some(shortcuts::ZOOM_100), Command::ZoomActualSize, true);
                ui.separator();
                item(ui, tr("Rotate View Left (4)"), None, Command::RotateViewLeft, true);
                item(ui, tr("Rotate View Right (6)"), None, Command::RotateViewRight, true);
                item(ui, tr("Reset Rotation (5)"), None, Command::ResetRotation, true);
                ui.weak(tr("Or Shift+Space and drag to turn it freely."));
                ui.separator();
                ui.checkbox(&mut self.editor.onion.enabled, tr("Onion Skin"));
                ui.checkbox(&mut self.editor.light_table, tr("Light Table"))
                    .on_hover_text(tr("Fade every layer but the selected one, to draw on it"));
                ui.separator();
                ui.menu_button(tr("Safe Areas"), |ui| {
                    for kind in SafeAreas::ALL {
                        ui.radio_value(&mut self.editor.guides.safe_areas, kind, tr(kind.label()));
                    }
                    ui.weak(tr("Keep action inside the outer line\nand text inside the inner one."));
                });
                ui.checkbox(&mut self.editor.guides.field_guide, tr("Field Guide"))
                    .on_hover_text(tr("A 12-field chart over the camera's frame, for planning moves in fields"));
                ui.separator();
                // On or off, shown as a highlighted item, with its keys.
                let toggle = |ui: &mut egui::Ui, on: bool, label: &str, keys: egui::KeyboardShortcut| {
                    let button =
                        egui::Button::new(label).selected(on).shortcut_text(ctx.format_shortcut(&keys));
                    let clicked = ui.add(button).clicked();
                    if clicked {
                        ui.close();
                    }
                    clicked
                };
                if toggle(ui, self.editor.show_rulers, tr("Rulers"), shortcuts::RULERS) {
                    toggled = Some(Command::ToggleRulers);
                }
                if toggle(ui, self.editor.rig_setup, tr("Rig Setup (Skeletons at Rest)"), shortcuts::RIG_SETUP) {
                    toggled = Some(Command::ToggleRigSetup);
                }
                ui.menu_button(tr("Perspective Guide"), |ui| {
                    let current = self.editor.doc.perspective.as_ref().map(|g| g.kind);
                    let mut options = vec![(None, tr("None"))];
                    options.extend(inkframe_core::PerspectiveKind::ALL.map(|k| (Some(k), tr(k.label()))));
                    for (kind, label) in options {
                        if ui.add(egui::Button::new(label).selected(current == kind)).clicked() {
                            toggled = Some(Command::SetPerspective(kind));
                            ui.close();
                        }
                    }
                    ui.weak(tr("Drag its vanishing points on the stage.\nShift with the Pen snaps lines to it."));
                });
                ui.menu_button(tr("Guides"), |ui| {
                    let editor = &self.editor;
                    let toggles = [
                        (editor.show_guides, tr("Show Guides"), shortcuts::SHOW_GUIDES, Command::ToggleGuides),
                        (
                            editor.lock_guides,
                            tr("Lock Guides"),
                            shortcuts::LOCK_GUIDES,
                            Command::ToggleLockGuides,
                        ),
                        (
                            editor.snap_to_guides,
                            tr("Snap to Guides"),
                            shortcuts::SNAP_TO_GUIDES,
                            Command::ToggleSnapToGuides,
                        ),
                    ];
                    for (on, label, keys, command) in toggles {
                        if toggle(ui, on, label, keys) {
                            toggled = Some(command);
                        }
                    }
                    if ui
                        .add_enabled(!editor.doc.ruler_guides.is_empty(), egui::Button::new(tr("Clear Guides")))
                        .clicked()
                    {
                        toggled = Some(Command::ClearGuides);
                        ui.close();
                    }
                    ui.weak(tr("Drag from a ruler onto the stage for a guide;\ndrag it back to take it away."));
                });
                ui.separator();
                ui.menu_button(tr("Workspace"), |ui| {
                    for workspace in Workspace::ALL {
                        item(ui, tr(workspace.name()), None, Command::Workspace(workspace), true);
                    }
                });
                item(ui, tr("Reset Workspace"), None, Command::ResetLayout, true);
                ui.weak(tr("Drag an area's corner to split or join it,\nCtrl+Space to fill the window with it."));
                ui.separator();
                self.language_menu(ui);
            });
            ui.menu_button(tr("Scripts"), |ui| self.scripts.menu(ui, &self.editor));
            let help = ui.menu_button(tr("Help"), |ui| {
                item(ui, tr("Find a Command…"), Some(shortcuts::FIND_COMMAND), Command::OpenFinder, true);
                item(ui, tr("Take the Tour"), None, Command::StartTour, true);
                item(ui, tr("What's New"), None, Command::WhatsNew, true);
                item(ui, tr("Check for Updates…"), None, Command::CheckForUpdates, true);
                ui.checkbox(&mut self.update_prefs.automatic, tr("Check for Updates Automatically")).on_hover_text(tr(
                    "Once a day at startup, Inkframe asks GitHub for its latest release; nothing else is sent",
                ));
                ui.separator();
                if ui.button(tr("Inkframe for Artists (Guidebook)")).clicked() {
                    open_in_browser(GUIDEBOOK_URL);
                    ui.close();
                }
                if ui.button(tr("Quick Start (Five Minutes)")).clicked() {
                    open_in_browser(GUIDE_URL);
                    ui.close();
                }
                if ui.button(tr("Report a Problem…")).clicked() {
                    open_in_browser(ISSUES_URL);
                    ui.close();
                }
                ui.weak(tr_args("Inkframe {version}", &[("version", env!("CARGO_PKG_VERSION"))]));
            });
            crate::tour::Targets::note(&mut self.tour_targets.help, help.response.rect);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // The workspace switcher, at the top right as in Animate.
                let current = self.layout.workspace_name();
                egui::ComboBox::from_id_salt("workspace").selected_text(tr(current.name())).width(110.0).show_ui(
                    ui,
                    |ui| {
                        for workspace in Workspace::ALL {
                            if ui.selectable_label(workspace == current, tr(workspace.name())).clicked() {
                                chosen = Some(Command::Workspace(workspace));
                            }
                        }
                        ui.separator();
                        if ui.button(tr_args("Reset {workspace}", &[("workspace", tr(current.name()))])).clicked() {
                            chosen = Some(Command::ResetLayout);
                        }
                    },
                );
                ui.add_space(8.0);
                ui.weak(format!("{:.0}%", self.views.stage().zoom_percent(&self.editor.doc)));
                // Find a Command, where it's easy to see.
                ui.add_space(8.0);
                let keys = ctx.format_shortcut(&shortcuts::FIND_COMMAND);
                let find = ui
                    .add(crate::icons::LabelledButton::new(crate::icons::Icon::Search, &tr_args("Find a command  {keys}", &[("keys", &keys)])))
                    .on_hover_text(tr("Search every command and tool by name"));
                crate::tour::Targets::note(&mut self.tour_targets.finder, find.rect);
                if find.clicked() {
                    chosen = Some(Command::OpenFinder);
                }
            });
        });

        if let Some(command) = chosen.or(toggled) {
            self.execute(command);
        }
    }

    /// Notes that the tour has been seen, so it doesn't open by itself
    /// again, and does what its last card asked.
    pub(super) fn tour_ended(&mut self, ended: crate::tour::Ended) {
        self.toured = true;
        if ended == crate::tour::Ended::OpenExamples {
            self.execute(Command::OpenExample);
        }
    }

    /// The settings to keep for next time, as they are now.
    /// View › Language: the system's, or one picked. Inkframe starts in it
    /// next time, as texts already shown can't change language.
    pub(super) fn language_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(tr("Language"), |ui| {
            if ui.radio(self.language.is_none(), tr("Same as the System")).clicked() {
                self.language = None;
            }
            for (code, name) in crate::i18n::languages() {
                if ui.radio(self.language.as_deref() == Some(code), name).clicked() {
                    self.language = Some(code.to_owned());
                }
            }
            ui.weak(tr("Inkframe starts in the language picked\nthe next time it opens."));
        });
    }
}

/// A one-page start that walks through a first animation, pointing on to
/// the guidebook (Help menu).
pub(super) const GUIDE_URL: &str =
    "https://github.com/WhateverTrevor/inkframe/blob/main/docs/GETTING_STARTED.md";

/// Where problems are reported (Help menu).
pub(super) const ISSUES_URL: &str = "https://github.com/WhateverTrevor/inkframe/issues";
