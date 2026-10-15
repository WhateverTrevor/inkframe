//! The top-level application: window layout, menus, shortcuts and files.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use inkframe_core::{Clipboard, Document, SymbolId, TimelineId};
use inkframe_render::IMAGE_EXTENSIONS;

use crate::adjust_dialog::{AdjustDialog, AdjustmentKind};
use crate::audio_engine::AudioEngine;
use crate::editor::{Editor, Tool};
use crate::export_dialog::ExportDialog;
use crate::library::LibraryView;
use crate::lip_sync_dialog::LipSyncDialog;
use crate::screenshot::AutoScreenshot;
use crate::stage::StageView;
use crate::timeline::TimelineView;
use crate::xsheet::ExposureSheet;
use crate::{inspector, shortcuts, toolbar};

/// Everything the user can trigger from a menu or a keyboard shortcut.
/// Routing both through one enum keeps menus and shortcuts in sync.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Command {
    New,
    Open,
    ImportImage,
    ImportAudio,
    ImportFont,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    SelectAll,
    Deselect,
    /// Select › Inverse: the unselected pixels instead.
    InvertSelection,
    LayerViaCopy,
    LayerViaCut,
    MergeDown,
    /// Opens the Adjustments window with this adjustment.
    Adjust(AdjustmentKind),
    Cut,
    Copy,
    /// Pastes layers in the middle of the view (or a keyframe at the playhead).
    Paste,
    /// Pastes layers exactly where they were copied from.
    PasteInPlace,
    /// Copies the selected layer's pose on the current frame.
    CopyFrames,
    /// Pastes a copied pose onto the selected layer at the current frame.
    PasteFrames,
    DuplicateLayer,
    DeleteLayer,
    ConvertToSymbol,
    /// Makes the selected layer a mask over the layer below it.
    MaskLayerBelow,
    /// Opens the selected instance's symbol for editing.
    EditSymbol,
    /// Steps out of the symbol being edited.
    ExitSymbol,
    InsertKeyframe,
    RemoveKeyframe,
    LayerStartHere,
    LayerEndHere,
    /// Adds a blank drawing to the selected layer, shown from this frame.
    NewDrawing,
    /// Copies the drawing showing now into a new one, shown from this frame.
    DuplicateDrawing,
    /// Removes the drawing showing now.
    DeleteDrawing,
    PreviousDrawing,
    NextDrawing,
    /// Opens the Lip Sync window for the selected layer.
    LipSync,
    NewPaintLayer,
    PlayPause,
    StepBackward,
    StepForward,
    FirstFrame,
    LastFrame,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ZoomActualSize,
    /// A toolbar slot's key: its last-used tool, or with `cycle` the next
    /// tool in the slot.
    UseSlot {
        group: &'static [Tool],
        cycle: bool,
    },
    /// Ends the pen path being drawn, leaving it open.
    FinishPath,
    /// Removes the newest point of the pen path being drawn.
    RemovePathPoint,
    /// Deletes the anchor point picked with the Subselection tool.
    DeleteAnchor,
}

const FILE_EXTENSION: &str = "inkf";
/// How much `[` and `]` shrink or grow the brush.
const BRUSH_STEP: f32 = 1.2;
/// Font files Inkframe can import.
const FONT_EXTENSIONS: [&str; 3] = ["ttf", "otf", "ttc"];
/// Sound files Inkframe can import (see `inkframe-audio`).
const AUDIO_EXTENSIONS: [&str; 9] = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "aif", "aiff", "caf"];

pub struct InkframeApp {
    editor: Editor,
    stage: StageView,
    timeline: TimelineView,
    export: ExportDialog,
    lip_sync: LipSyncDialog,
    xsheet: ExposureSheet,
    adjust: AdjustDialog,
    library: LibraryView,
    audio: AudioEngine,
    /// The window title we last set, so we only send changes.
    title: String,
    /// Set once the user has confirmed quitting with unsaved changes.
    allow_close: bool,
    screenshot: Option<AutoScreenshot>,
    /// What was last copied. Kept here as well as on the system clipboard,
    /// because egui only reports a paste when the system clipboard has
    /// text, and the Edit menu has no other way to read it.
    clipboard: Option<Clipboard>,
    ctx: egui::Context,
}

impl InkframeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        // Indexing the system's fonts takes a moment; do it now, off the UI
        // thread, so the first text drawn doesn't pause the app.
        std::thread::spawn(inkframe_render::load_fonts);
        crate::tablet::install();

        let editor = match open_path {
            Some(path) => Editor::open(&path).unwrap_or_else(|err| {
                show_error(&format!("Couldn't open {}", path.display()), &err.to_string());
                Editor::new(Document::default())
            }),
            None => Editor::new(Document::default()),
        };

        let mut app = Self {
            editor,
            stage: StageView::default(),
            timeline: TimelineView::default(),
            export: ExportDialog::default(),
            lip_sync: LipSyncDialog::default(),
            xsheet: ExposureSheet::default(),
            adjust: AdjustDialog::default(),
            library: LibraryView::default(),
            audio: AudioEngine::default(),
            title: String::new(),
            allow_close: false,
            screenshot: AutoScreenshot::from_env(),
            clipboard: None,
            ctx: cc.egui_ctx.clone(),
        };
        if app.screenshot.is_some() {
            AutoScreenshot::set_up_scene(&mut app.editor);
            if let Some(name) = AutoScreenshot::type_into() {
                app.stage.type_into(&mut app.editor, &name);
            }
            app.timeline.set_graph_mode(AutoScreenshot::graph());
            app.xsheet.open = AutoScreenshot::exposure_sheet();
            if AutoScreenshot::lip_sync() {
                app.lip_sync.open(&app.editor);
            }
            if AutoScreenshot::expand_timeline() {
                for layer in app.editor.doc.all_layers() {
                    app.timeline.set_expanded(layer.id, true);
                }
            }
        }
        app
    }

    fn execute(&mut self, command: Command) {
        let editor = &mut self.editor;
        match command {
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
            // With a painting or selection tool, Select All and Deselect
            // work on pixels, as in Photoshop; otherwise on layers.
            Command::SelectAll if editor.tool.on_pixels() && editor.paint_target().is_some() => {
                editor.select_all_pixels()
            }
            Command::Deselect if editor.tool.on_pixels() => editor.deselect_pixels(),
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
                self.copy(false);
                self.editor.delete_selected_layer();
            }
            Command::Copy => self.copy(false),
            Command::CopyFrames => self.copy(true),
            Command::Paste => {
                let center = self.stage.visible_center(&self.editor);
                if let Some(clipboard) = &self.clipboard {
                    self.editor.paste(clipboard, Some(center));
                }
            }
            Command::PasteInPlace | Command::PasteFrames => {
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
            Command::ConvertToSymbol => {
                editor.convert_selected_to_symbol();
            }
            Command::MaskLayerBelow => {
                if let Some(id) = editor.selection {
                    editor.doc.mask_layer_below(id);
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
            Command::PreviousDrawing => editor.cycle_drawing(-1),
            Command::NextDrawing => editor.cycle_drawing(1),
            Command::NewPaintLayer => {
                editor.add_paint_layer();
            }
            Command::LipSync => self.lip_sync.open(editor),
            Command::PlayPause => editor.toggle_playback(),
            Command::StepBackward => editor.step(-1),
            Command::StepForward => editor.step(1),
            Command::FirstFrame => editor.set_frame(0),
            Command::LastFrame => editor.set_frame(editor.last_frame()),
            Command::ZoomIn => self.stage.zoom_by(&editor.doc, 1.25),
            Command::ZoomOut => self.stage.zoom_by(&editor.doc, 0.8),
            Command::ZoomFit => self.stage.zoom_to_fit(),
            Command::ZoomActualSize => self.stage.zoom_to_actual_size(&editor.doc),
            Command::UseSlot { group, cycle } => editor.use_slot(group, cycle),
            Command::FinishPath => self.stage.finish_path(editor, false),
            Command::RemovePathPoint => self.stage.remove_last_path_point(),
            Command::DeleteAnchor => self.stage.delete_selected_anchor(editor),
        }
    }

    // ------------------------------------------------------------ shortcuts

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use shortcuts::*;

        // `consume_shortcut` ignores extra Shift, so ⌘⇧S would also match ⌘S.
        // Check the shifted variants first so they win.
        let file_shortcuts = [
            (SAVE_AS, Command::SaveAs),
            (SAVE, Command::Save),
            (OPEN, Command::Open),
            (NEW, Command::New),
            (EXPORT, Command::Export),
            (IMPORT, Command::ImportImage),
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
            (ZOOM_FIT, Command::ZoomFit),
            (ZOOM_100, Command::ZoomActualSize),
            (ZOOM_IN, Command::ZoomIn),
            (ZOOM_OUT, Command::ZoomOut),
            (REMOVE_KEYFRAME, Command::RemoveKeyframe),
            (LAYER_START_HERE, Command::LayerStartHere),
            (LAYER_END_HERE, Command::LayerEndHere),
            // After the ⌥ versions above, so those don't also swap.
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
        let drawing_path = self.stage.is_drawing_path();
        let anchor_selected = self.stage.has_anchor_selection();
        let in_symbol = self.editor.timeline() != TimelineId::Main;
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
            // not keys. ⌥ makes them act on keyframes, and ⇧ pastes in place.
            for event in &input.events {
                let (alt, shift) = (input.modifiers.alt, input.modifiers.shift);
                match event {
                    egui::Event::Copy => commands.push(if alt { Command::CopyFrames } else { Command::Copy }),
                    egui::Event::Cut => commands.push(Command::Cut),
                    egui::Event::Paste(text) => match Clipboard::from_text(text) {
                        // Inkframe data, perhaps from another window: paste
                        // that, and remember it for the Edit menu.
                        Some(clipboard) => {
                            let command = match (&clipboard, shift) {
                                (Clipboard::Keyframe(_), _) => Command::PasteFrames,
                                (Clipboard::Layers(_), true) => Command::PasteInPlace,
                                (Clipboard::Layers(_), false) => Command::Paste,
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
            } else if anchor_selected {
                &[(Key::Backspace, Command::DeleteAnchor), (Key::Delete, Command::DeleteAnchor)]
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
                (Key::Backspace, Command::DeleteLayer),
            ];
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
            let center = self.stage.visible_center(&self.editor);
            self.editor.paste_text(&text, center);
        }
    }

    /// Copies the selected layer, or with `keyframe` its pose on this frame,
    /// to the system clipboard (so it can be pasted into another window)
    /// and to our own.
    fn copy(&mut self, keyframe: bool) {
        let copied = if keyframe { self.editor.copy_keyframe() } else { self.editor.copy_selection() };
        if let Some(clipboard) = copied {
            self.ctx.copy_text(clipboard.to_text());
            self.clipboard = Some(clipboard);
        }
    }

    // ---------------------------------------------------------------- menus

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
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

            ui.menu_button("File", |ui| {
                item(ui, "New", Some(shortcuts::NEW), Command::New, true);
                item(ui, "Open…", Some(shortcuts::OPEN), Command::Open, true);
                item(ui, "Import to Stage…", Some(shortcuts::IMPORT), Command::ImportImage, true);
                item(ui, "Import Audio…", None, Command::ImportAudio, true);
                item(ui, "Import Font…", None, Command::ImportFont, true);
                ui.separator();
                item(ui, "Save", Some(shortcuts::SAVE), Command::Save, true);
                item(ui, "Save As…", Some(shortcuts::SAVE_AS), Command::SaveAs, true);
                ui.separator();
                item(ui, "Export…", Some(shortcuts::EXPORT), Command::Export, true);
            });
            ui.menu_button("Edit", |ui| {
                item(ui, "Undo", Some(shortcuts::UNDO), Command::Undo, self.editor.history.can_undo());
                item(ui, "Redo", Some(shortcuts::REDO), Command::Redo, self.editor.history.can_redo());
                ui.separator();
                item(
                    ui,
                    "Duplicate Layer",
                    Some(shortcuts::DUPLICATE),
                    Command::DuplicateLayer,
                    has_selection,
                );
                item(ui, "Delete Layer", None, Command::DeleteLayer, has_selection);
                ui.separator();
                let layers = matches!(self.clipboard, Some(Clipboard::Layers(_)));
                let keyframe = matches!(self.clipboard, Some(Clipboard::Keyframe(_)));
                item(ui, "Select All", Some(shortcuts::SELECT_ALL), Command::SelectAll, true);
                item(ui, "Deselect All", Some(shortcuts::DESELECT_ALL), Command::Deselect, has_selection);
                ui.separator();
                item(ui, "Cut", Some(shortcuts::CUT), Command::Cut, has_selection);
                item(ui, "Copy", Some(shortcuts::COPY), Command::Copy, has_selection);
                item(ui, "Paste in Center", Some(shortcuts::PASTE), Command::Paste, layers);
                item(ui, "Paste in Place", Some(shortcuts::PASTE_IN_PLACE), Command::PasteInPlace, layers);
                ui.separator();
                item(ui, "Copy Frames", Some(shortcuts::COPY_FRAMES), Command::CopyFrames, has_selection);
                item(
                    ui,
                    "Paste Frames",
                    Some(shortcuts::PASTE_FRAMES),
                    Command::PasteFrames,
                    keyframe && has_selection,
                );
            });
            ui.menu_button("Modify", |ui| {
                item(
                    ui,
                    "Convert to Symbol",
                    Some(shortcuts::CONVERT_TO_SYMBOL),
                    Command::ConvertToSymbol,
                    has_selection,
                );
                let instance_selected = self.editor.selected_layer().is_some_and(|l| l.instance.is_some());
                item(ui, "Edit Symbol", None, Command::EditSymbol, instance_selected);
                let in_symbol = self.editor.timeline() != TimelineId::Main;
                item(ui, "Exit Symbol", None, Command::ExitSymbol, in_symbol);
                ui.separator();
                // Only offered when there's a layer below to mask.
                let below = self
                    .editor
                    .selection
                    .is_some_and(|id| self.editor.layers().first().is_some_and(|bottom| bottom.id != id));
                item(ui, "Mask", None, Command::MaskLayerBelow, below);
                ui.separator();
                item(ui, "New Paint Layer", None, Command::NewPaintLayer, true);
                let below_is_paint = self.editor.selection.is_some_and(|id| {
                    let Some(timeline) = self.editor.doc.timeline_of(id) else { return false };
                    let layers = self.editor.doc.layers_of(timeline);
                    let index = layers.iter().position(|l| l.id == id).unwrap_or(0);
                    index > 0
                        && self.editor.doc.paint_canvas_at(layers[index - 1].id, self.editor.frame).is_some()
                });
                item(ui, "Merge Down", None, Command::MergeDown, below_is_paint);
                let paint = self.editor.paint_target().is_some();
                ui.menu_button("Adjustments", |ui| {
                    for kind in AdjustmentKind::ALL {
                        let label = format!("{}…", kind.neutral().name());
                        item(ui, &label, None, Command::Adjust(kind), paint);
                    }
                });
                // Drawing substitution: only for layers with their own
                // artwork, not symbol instances.
                let drawn = self.editor.selected_layer().filter(|l| l.instance.is_none());
                let swaps = drawn.is_some_and(|l| l.drawings.len() > 1);
                item(ui, "New Drawing", None, Command::NewDrawing, drawn.is_some());
                item(ui, "Duplicate Drawing", None, Command::DuplicateDrawing, drawn.is_some());
                item(ui, "Delete Drawing", None, Command::DeleteDrawing, swaps);
                item(
                    ui,
                    "Previous Drawing",
                    Some(shortcuts::PREVIOUS_DRAWING),
                    Command::PreviousDrawing,
                    swaps,
                );
                item(ui, "Next Drawing", Some(shortcuts::NEXT_DRAWING), Command::NextDrawing, swaps);
                item(ui, "Lip Sync…", None, Command::LipSync, swaps);
            });
            ui.menu_button("Select", |ui| {
                let paint = self.editor.paint_target().is_some();
                let selected = self.editor.active_selection().is_some();
                item(ui, "All Pixels", Some(shortcuts::SELECT_ALL), Command::SelectAll, paint);
                item(ui, "Deselect", Some(shortcuts::DESELECT_ALL), Command::Deselect, selected);
                item(ui, "Inverse", Some(shortcuts::INVERT_SELECTION), Command::InvertSelection, paint);
                ui.separator();
                item(ui, "Layer via Copy", Some(shortcuts::LAYER_VIA_COPY), Command::LayerViaCopy, selected);
                item(ui, "Layer via Cut", Some(shortcuts::LAYER_VIA_CUT), Command::LayerViaCut, selected);
                ui.separator();
                ui.weak(
                    "All Pixels and Deselect use ⌘A and ⌘⇧A while a\npainting or selection tool is in use.",
                );
            });
            ui.menu_button("Timeline", |ui| {
                let play = if self.editor.playback.playing { "Pause" } else { "Play" };
                item(ui, play, Some(shortcuts::PLAY), Command::PlayPause, true);
                ui.separator();
                item(
                    ui,
                    "Insert Keyframe",
                    Some(shortcuts::INSERT_KEYFRAME),
                    Command::InsertKeyframe,
                    has_selection,
                );
                item(
                    ui,
                    "Remove Keyframe",
                    Some(shortcuts::REMOVE_KEYFRAME),
                    Command::RemoveKeyframe,
                    has_selection,
                );
                ui.separator();
                item(
                    ui,
                    "Start Layer Here",
                    Some(shortcuts::LAYER_START_HERE),
                    Command::LayerStartHere,
                    has_selection,
                );
                item(
                    ui,
                    "End Layer Here",
                    Some(shortcuts::LAYER_END_HERE),
                    Command::LayerEndHere,
                    has_selection,
                );
                ui.separator();
                item(ui, "First Frame", None, Command::FirstFrame, true);
                item(ui, "Last Frame", None, Command::LastFrame, true);
                ui.separator();
                ui.checkbox(&mut self.xsheet.open, "Exposure Sheet");
            });
            ui.menu_button("View", |ui| {
                item(ui, "Zoom In", Some(shortcuts::ZOOM_IN), Command::ZoomIn, true);
                item(ui, "Zoom Out", Some(shortcuts::ZOOM_OUT), Command::ZoomOut, true);
                item(ui, "Fit Stage", Some(shortcuts::ZOOM_FIT), Command::ZoomFit, true);
                item(ui, "Actual Size", Some(shortcuts::ZOOM_100), Command::ZoomActualSize, true);
                ui.separator();
                ui.checkbox(&mut self.editor.onion.enabled, "Onion Skin");
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("{:.0}%", self.stage.zoom_percent(&self.editor.doc)));
            });
        });

        if let Some(command) = chosen {
            self.execute(command);
        }
    }

    // ---------------------------------------------------------------- files

    fn replace_editor(&mut self, editor: Editor) {
        let tool = self.editor.tool;
        self.editor = editor;
        self.editor.tool = tool;
        self.stage.zoom_to_fit();
    }

    fn open(&mut self) {
        if !self.confirm_discard_changes() {
            return;
        }
        let Some(path) =
            rfd::FileDialog::new().add_filter("Inkframe document", &[FILE_EXTENSION]).pick_file()
        else {
            return;
        };
        match Editor::open(&path) {
            Ok(editor) => self.replace_editor(editor),
            Err(err) => show_error(&format!("Couldn't open {}", file_name(&path)), &err.to_string()),
        }
    }

    fn import_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import to stage")
            .add_filter("Images", &IMAGE_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        if let Err(message) = self.editor.import_image(&path) {
            show_error("Couldn't import the image", &message);
        }
    }

    /// Imports image and sound files dropped onto the window, the same as
    /// choosing them from the File menu.
    fn import_dropped_files(&mut self, ctx: &egui::Context) {
        // A file dropped without a path on disk can't be linked to, so skip it.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
                if let Err(message) = self.editor.import_image(&path) {
                    show_error("Couldn't import the image", &message);
                }
            } else if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
                self.editor.import_audio(&path);
            } else if FONT_EXTENSIONS.contains(&extension.as_str()) {
                self.import_font(&path);
            } else {
                show_error(
                    "Can't import that file",
                    &format!("{} isn't an image, sound or font file Inkframe can read.", file_name(&path)),
                );
            }
        }
    }

    /// Several at once, since a family often comes as separate files for
    /// regular, bold and italic.
    fn import_fonts(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import fonts")
            .add_filter("Fonts", &FONT_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        for path in paths {
            self.import_font(&path);
        }
    }

    fn import_font(&mut self, path: &Path) {
        if let Err(message) = self.editor.import_font(path) {
            show_error("Couldn't import the font", &message);
        }
    }

    fn import_audio(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import audio")
            .add_filter("Audio", &AUDIO_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        self.editor.import_audio(&path);
    }

    /// Saves to the current file, asking for a location if there isn't one.
    /// Returns whether the document was saved.
    fn save(&mut self) -> bool {
        match self.editor.path.clone() {
            Some(path) => self.save_to(&path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Inkframe document", &[FILE_EXTENSION])
            .set_file_name(format!("{}.{FILE_EXTENSION}", self.editor.display_name()))
            .save_file()
        else {
            return false;
        };
        self.save_to(&path)
    }

    fn save_to(&mut self, path: &Path) -> bool {
        match self.editor.save_to(path) {
            Ok(()) => true,
            Err(err) => {
                show_error(&format!("Couldn't save {}", file_name(path)), &err.to_string());
                false
            }
        }
    }

    /// If there are unsaved changes, asks whether to save them. Returns
    /// `true` if it's OK to continue (saved or discarded), `false` to stay put.
    fn confirm_discard_changes(&mut self) -> bool {
        if !self.editor.has_unsaved_changes() {
            return true;
        }
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Unsaved changes")
            .set_description(format!("Save changes to “{}” first?", self.editor.display_name()))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match answer {
            rfd::MessageDialogResult::Yes => self.save(),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        let export_ok = !self.export.is_running() || {
            let answer = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Export in progress")
                .set_description(
                    "Quitting now will stop the export and leave an incomplete file. Quit anyway?",
                )
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show();
            answer == rfd::MessageDialogResult::Ok
        };
        if export_ok && self.confirm_discard_changes() {
            self.allow_close = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.editor.has_unsaved_changes() { " •" } else { "" };
        let title = format!("{}{dirty} — Inkframe", self.editor.display_name());
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for InkframeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);
        self.import_dropped_files(&ctx);
        // Decode any image that's new (just imported, or brought back by
        // undo) before the stage draws.
        let project_dir = self.editor.project_dir();
        self.editor.images.sync(&self.editor.doc, project_dir.as_deref());
        self.editor.font_problems =
            inkframe_render::sync_project_fonts(&self.editor.doc, project_dir.as_deref());
        self.editor.remeasure_text();

        // Panel order decides who owns the corners: the timeline is added
        // before the side panels so it spans the full window width.
        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("timeline")
            .resizable(true)
            .default_size(230.0)
            .min_size(140.0)
            .show(ui, |ui| self.timeline.show(ui, &mut self.editor, &mut self.audio));
        let toolbar_width = toolbar::width(ui.available_height());
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(toolbar_width)
            .show(ui, |ui| toolbar::show(ui, &mut self.editor));
        egui::Panel::right("inspector").resizable(true).default_size(290.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                inspector::show(ui, &mut self.editor);
                ui.add_space(8.0);
                self.library.show(ui, &mut self.editor);
                ui.add_space(8.0);
                if let Some(inspector::AudioAction::Import) =
                    inspector::audio_section(ui, &mut self.editor, &mut self.audio)
                {
                    self.import_audio();
                }
            });
        });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if self.editor.timeline() != TimelineId::Main {
                breadcrumbs(ui, &mut self.editor);
            }
            self.stage.show(ui, &mut self.editor);
            if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
                drop_hint(ui);
            }
        });

        let name = self.editor.display_name();
        self.export.show(&ctx, &self.editor.doc, &name, self.editor.project_dir());
        self.lip_sync.show(&ctx, &mut self.editor, &mut self.audio);
        self.xsheet.show(&ctx, &mut self.editor, &mut self.audio);
        self.adjust.show(&ctx, &mut self.editor);

        let dt = ctx.input(|i| i.stable_dt) as f64;
        let project_dir = self.editor.project_dir();
        self.audio.sync(&ctx, &mut self.editor, project_dir.as_deref(), dt);
        if self.editor.playback.playing {
            ctx.request_repaint();
        }

        // Record an undo step once the user has let go of the mouse and
        // finished typing, so each gesture or edit is a single step.
        let interacting = self.stage.is_busy()
            || self.timeline.is_busy()
            || ctx.input(|i| i.pointer.any_down())
            || ctx.egui_wants_keyboard_input();
        if !interacting {
            self.editor.commit_history();
        }

        self.update_title(&ctx);
        self.handle_close_request(&ctx);
        if let Some(screenshot) = &mut self.screenshot {
            // Once the sound is ready, so loudness can be measured.
            if AutoScreenshot::lip_sync_apply() && !self.audio.is_busy() {
                self.lip_sync.apply_once(&mut self.editor, &mut self.audio);
            }
            screenshot.update(&ctx, self.audio.is_busy() || self.lip_sync.is_busy());
        }
    }
}

/// The "Scene ▸ Symbol ▸ …" bar shown while editing a symbol. Clicking an
/// earlier step goes back out to it.
fn breadcrumbs(ui: &mut egui::Ui, editor: &mut Editor) {
    let path: Vec<(SymbolId, String)> = editor
        .open_symbols()
        .map(|id| (id, editor.doc.symbol(id).map_or_else(|| "?".to_owned(), |s| s.name.clone())))
        .collect();
    let mut exit_to = None;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).fill(ui.visuals().panel_fill).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui
                    .link("Scene")
                    .on_hover_text("Back to the main timeline (Esc steps out one level)")
                    .clicked()
                {
                    exit_to = Some(0);
                }
                for (depth, (_, name)) in path.iter().enumerate() {
                    ui.weak("›");
                    // The last step is where we are, so it isn't a link.
                    if depth + 1 == path.len() {
                        ui.strong(name);
                    } else if ui.link(name).clicked() {
                        exit_to = Some(depth + 1);
                    }
                }
            });
        },
    );
    if let Some(depth) = exit_to {
        editor.exit_to(depth);
    }
}

/// Shown over the stage while files are dragged over the window.
fn drop_hint(ui: &egui::Ui) {
    let area = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(area, 0.0, egui::Color32::from_black_alpha(140));
    painter.text(
        area.center(),
        egui::Align2::CENTER_CENTER,
        "Drop images, sounds or fonts to import them",
        egui::FontId::proportional(20.0),
        egui::Color32::WHITE,
    );
}

fn show_error(title: &str, detail: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(detail)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}
