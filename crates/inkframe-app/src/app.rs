//! The top-level application: window layout, menus, shortcuts and files.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use inkframe_core::{Document, SymbolId, TimelineId};

use crate::editor::{Editor, Tool};
use crate::export_dialog::ExportDialog;
use crate::library::LibraryView;
use crate::screenshot::AutoScreenshot;
use crate::stage::StageView;
use crate::timeline::TimelineView;
use crate::{inspector, shortcuts, toolbar};

/// Everything the user can trigger from a menu or a keyboard shortcut.
/// Routing both through one enum keeps menus and shortcuts in sync.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    DuplicateLayer,
    DeleteLayer,
    ConvertToSymbol,
    /// Opens the selected instance's symbol for editing.
    EditSymbol,
    /// Steps out of the symbol being edited.
    ExitSymbol,
    InsertKeyframe,
    RemoveKeyframe,
    LayerStartHere,
    LayerEndHere,
    PlayPause,
    StepBackward,
    StepForward,
    FirstFrame,
    LastFrame,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ZoomActualSize,
    UseTool(Tool),
    /// Ends the pen path being drawn, leaving it open.
    FinishPath,
    /// Removes the newest point of the pen path being drawn.
    RemovePathPoint,
    /// Deletes the anchor point picked with the Subselection tool.
    DeleteAnchor,
}

const FILE_EXTENSION: &str = "inkf";

pub struct InkframeApp {
    editor: Editor,
    stage: StageView,
    timeline: TimelineView,
    export: ExportDialog,
    library: LibraryView,
    /// The window title we last set, so we only send changes.
    title: String,
    /// Set once the user has confirmed quitting with unsaved changes.
    allow_close: bool,
    screenshot: Option<AutoScreenshot>,
}

impl InkframeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);

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
            library: LibraryView::default(),
            title: String::new(),
            allow_close: false,
            screenshot: AutoScreenshot::from_env(),
        };
        if app.screenshot.is_some() {
            AutoScreenshot::set_up_scene(&mut app.editor);
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
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
            Command::Export => self.export.open(&self.editor.doc),
            Command::Undo => editor.undo(),
            Command::Redo => editor.redo(),
            Command::DuplicateLayer => editor.duplicate_selected_layer(),
            Command::DeleteLayer => editor.delete_selected_layer(),
            Command::ConvertToSymbol => {
                editor.convert_selected_to_symbol();
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
            Command::PlayPause => editor.toggle_playback(),
            Command::StepBackward => editor.step(-1),
            Command::StepForward => editor.step(1),
            Command::FirstFrame => editor.set_frame(0),
            Command::LastFrame => editor.set_frame(editor.last_frame()),
            Command::ZoomIn => self.stage.zoom_by(&editor.doc, 1.25),
            Command::ZoomOut => self.stage.zoom_by(&editor.doc, 0.8),
            Command::ZoomFit => self.stage.zoom_to_fit(),
            Command::ZoomActualSize => self.stage.zoom_to_actual_size(&editor.doc),
            Command::UseTool(tool) => editor.tool = tool,
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
        ];
        let editing_shortcuts = [
            (REDO, Command::Redo),
            (UNDO, Command::Undo),
            (REDO_ALT, Command::Redo),
            (DUPLICATE, Command::DuplicateLayer),
            (CONVERT_TO_SYMBOL, Command::ConvertToSymbol),
            (ZOOM_FIT, Command::ZoomFit),
            (ZOOM_100, Command::ZoomActualSize),
            (ZOOM_IN, Command::ZoomIn),
            (ZOOM_OUT, Command::ZoomOut),
            (REMOVE_KEYFRAME, Command::RemoveKeyframe),
            (LAYER_START_HERE, Command::LayerStartHere),
            (LAYER_END_HERE, Command::LayerEndHere),
            (INSERT_KEYFRAME, Command::InsertKeyframe),
            (PLAY, Command::PlayPause),
        ];

        let mut commands = Vec::new();
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
            let tool_keys = Tool::ALL.map(|tool| (tool_key(tool), Command::UseTool(tool)));
            for (key, command) in plain_keys.into_iter().chain(tool_keys) {
                if input.consume_key(Modifiers::NONE, key) {
                    commands.push(command);
                }
            }
        });

        for command in commands {
            self.execute(command);
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

        // Panel order decides who owns the corners: the timeline is added
        // before the side panels so it spans the full window width.
        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("timeline")
            .resizable(true)
            .default_size(230.0)
            .min_size(140.0)
            .show(ui, |ui| self.timeline.show(ui, &mut self.editor));
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(60.0)
            .show(ui, |ui| toolbar::show(ui, &mut self.editor));
        egui::Panel::right("inspector").resizable(true).default_size(290.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                inspector::show(ui, &mut self.editor);
                ui.add_space(8.0);
                self.library.show(ui, &mut self.editor);
            });
        });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if self.editor.timeline() != TimelineId::Main {
                breadcrumbs(ui, &mut self.editor);
            }
            self.stage.show(ui, &mut self.editor);
        });

        let name = self.editor.display_name();
        self.export.show(&ctx, &self.editor.doc, &name);

        let dt = ctx.input(|i| i.stable_dt) as f64;
        self.editor.advance_playback(dt);
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
            screenshot.update(&ctx);
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
