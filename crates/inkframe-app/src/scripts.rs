//! The Scripts menu: running JavaScript on the open project (see
//! `inkframe-script` and docs/SCRIPTING.md).
//!
//! Scripts live in the Scripts folder in the settings folder, and any
//! other `.js` file can be run with Run Script…. A script runs on its own
//! thread, against a copy of the project, with a Stop button; when it
//! finishes its copy becomes the project, as one undo step. If it fails,
//! nothing changes and the error is shown with its line. If the project is
//! edited while a script runs, the script's result is set aside rather
//! than overwrite those edits.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use inkframe_core::Document;
use inkframe_script::{Outcome, Script, ScriptError};

use crate::editor::Editor;
use crate::export_dialog::reveal_in_file_manager;
use crate::layout::settings_dir;

/// The folder scripts are listed from.
pub fn folder() -> Option<PathBuf> {
    settings_dir().map(|dir| dir.join("Scripts"))
}

/// The scripts in the Scripts folder, by name.
fn list() -> Vec<PathBuf> {
    let Some(dir) = folder() else { return Vec::new() };
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "js"))
        .collect();
    scripts.sort();
    scripts
}

/// A script running on its thread.
struct Running {
    name: String,
    stop: Arc<AtomicBool>,
    started: Instant,
    /// The project as it was when the script started, to tell whether it
    /// was edited meanwhile.
    before: Document,
    result: Receiver<Result<Outcome, ScriptError>>,
}

/// What the last script said, shown until dismissed.
struct Report {
    name: String,
    log: Vec<String>,
    problem: Option<String>,
}

#[derive(Default)]
pub struct Scripts {
    running: Option<Running>,
    report: Option<Report>,
}

impl Scripts {
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// The Scripts menu.
    pub fn menu(&mut self, ui: &mut egui::Ui, editor: &Editor) {
        let scripts = list();
        if scripts.is_empty() {
            ui.weak("No scripts yet: put .js files in\nthe Scripts folder to list them here.");
        }
        for path in scripts {
            let name = name_of(&path);
            if ui.add_enabled(!self.is_running(), egui::Button::new(&name)).clicked() {
                self.run_file(&path, editor);
                ui.close();
            }
        }
        ui.separator();
        if ui.add_enabled(!self.is_running(), egui::Button::new("Run Script…")).clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("JavaScript", &["js"]).pick_file() {
                self.run_file(&path, editor);
            }
            ui.close();
        }
        if ui.button("Open Scripts Folder").clicked() {
            if let Some(dir) = folder() {
                let _ = std::fs::create_dir_all(&dir);
                reveal_in_file_manager(&dir);
            }
            ui.close();
        }
        if ui.button("Scripting Guide").clicked() {
            crate::app::open_in_browser(crate::app::SCRIPTING_URL);
            ui.close();
        }
    }

    pub fn run_file(&mut self, path: &Path, editor: &Editor) {
        match std::fs::read_to_string(path) {
            Ok(source) => self.run(name_of(path), source, editor),
            Err(err) => {
                self.report = Some(Report {
                    name: name_of(path),
                    log: Vec::new(),
                    problem: Some(format!("couldn't read {}: {err}", path.display())),
                })
            }
        }
    }

    /// Starts `source` on its own thread, against a copy of the project.
    pub fn run(&mut self, name: String, source: String, editor: &Editor) {
        if self.is_running() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let script = Script {
            path: editor.path.clone(),
            selection: editor.selected_ids(),
            stop: Arc::clone(&stop),
            ..Script::new(source, editor.doc.clone())
        };
        let (sender, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(inkframe_script::run(script));
        });
        self.report = None;
        self.running =
            Some(Running { name, stop, started: Instant::now(), before: editor.doc.clone(), result });
    }

    /// Shows a running script's progress, and what the last one said; takes
    /// a finished script's result.
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        self.finish(editor);
        if let Some(running) = &self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            egui::Window::new("Running Script")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "{} ({:.0} s)",
                            running.name,
                            running.started.elapsed().as_secs_f32()
                        ));
                    });
                    if ui.button("Stop").clicked() {
                        running.stop.store(true, Ordering::Relaxed);
                    }
                });
        }
        let mut open = true;
        if let Some(report) = &self.report {
            let title = if report.problem.is_some() { "Script Failed" } else { "Script Output" };
            egui::Window::new(title)
                .id(egui::Id::new("script-report"))
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.strong(&report.name);
                    if let Some(problem) = &report.problem {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                    }
                    if !report.log.is_empty() {
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            for line in &report.log {
                                ui.monospace(line);
                            }
                        });
                    }
                });
        }
        if !open {
            self.report = None;
        }
    }

    /// Applies a finished script's result, or reports why not.
    fn finish(&mut self, editor: &mut Editor) {
        let Some(running) = &self.running else { return };
        let Ok(result) = running.result.try_recv() else { return };
        let running = self.running.take().expect("running");
        let name = running.name;
        self.report = match result {
            Ok(outcome) if editor.doc != running.before => Some(Report {
                name,
                log: outcome.log,
                problem: Some(
                    "The project was edited while the script ran, so the script's changes weren't applied. Run it again to apply them."
                        .to_owned(),
                ),
            }),
            Ok(outcome) => {
                // The script's copy becomes the project: one undo step.
                editor.doc = outcome.document;
                editor.path = outcome.path;
                if let Some(selection) = outcome.selection {
                    editor.select_many(&selection);
                }
                (!outcome.log.is_empty()).then_some(Report { name, log: outcome.log, problem: None })
            }
            Err(error) => Some(Report { name, log: error.log().to_vec(), problem: Some(error.to_string()) }),
        };
    }
}

/// "stagger-layers.js" → "stagger-layers".
fn name_of(path: &Path) -> String {
    path.file_stem().map_or_else(|| "script".to_owned(), |s| s.to_string_lossy().into_owned())
}
