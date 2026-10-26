//! The Scripts menu and the Script Console: running JavaScript on the open
//! project (see `inkframe-script` and docs/SCRIPTING.md).
//!
//! Scripts live in the Scripts folder in the settings folder, which starts
//! out holding the example scripts, and any other `.js` file can be run
//! with Run Script…. The Script Console is an area for typing a script
//! and seeing what it prints. A script runs on its own
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

/// The example scripts (`examples/scripts`), built into the app.
const EXAMPLES: [(&str, &str); 8] = [
    ("bouncing-ball.js", include_str!("../../../examples/scripts/bouncing-ball.js")),
    ("confetti.js", include_str!("../../../examples/scripts/confetti.js")),
    ("export-review.js", include_str!("../../../examples/scripts/export-review.js")),
    ("fade-in-out.js", include_str!("../../../examples/scripts/fade-in-out.js")),
    ("number-layers.js", include_str!("../../../examples/scripts/number-layers.js")),
    ("snowy-night.js", include_str!("../../../examples/scripts/snowy-night.js")),
    ("stagger-selected.js", include_str!("../../../examples/scripts/stagger-selected.js")),
    ("walk-cycle.js", include_str!("../../../examples/scripts/walk-cycle.js")),
];

/// Puts the example scripts in the Scripts folder the first time there is
/// one, so the Scripts menu has something in it to try and to learn from.
/// Once the folder exists it's left alone, so scripts someone deletes stay
/// deleted.
pub fn add_examples_if_new() {
    let Some(dir) = folder() else { return };
    if dir.exists() || std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for (name, source) in EXAMPLES {
        let _ = std::fs::write(dir.join(name), source);
    }
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
    /// Run from the Script Console, which shows what it prints.
    from_console: bool,
}

/// A line in the Script Console.
enum Line {
    /// The start of a run.
    Ran(String),
    Printed(String),
    Failed(String),
}

/// What the Script Console starts with.
const CONSOLE_START: &str = "// Type a script and press Run (Cmd+Enter).\nconsole.log(inkframe.document.layers.map((layer) => layer.name));\n";

/// What the last script said, shown until dismissed.
struct Report {
    name: String,
    log: Vec<String>,
    problem: Option<String>,
}

pub struct Scripts {
    running: Option<Running>,
    report: Option<Report>,
    console_source: String,
    console_lines: Vec<Line>,
}

impl Default for Scripts {
    fn default() -> Self {
        Self {
            running: None,
            report: None,
            console_source: CONSOLE_START.to_owned(),
            console_lines: Vec::new(),
        }
    }
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

    /// Puts `source` in the Script Console and runs it there, for
    /// screenshots.
    pub fn run_in_console(&mut self, source: String, editor: &Editor) {
        self.console_source = source.clone();
        self.console_lines.push(Line::Ran(format!("Ran at frame {}", editor.frame + 1)));
        self.run("Script Console".to_owned(), source, editor, true);
    }

    pub fn run_file(&mut self, path: &Path, editor: &Editor) {
        match std::fs::read_to_string(path) {
            Ok(source) => self.run(name_of(path), source, editor, false),
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
    pub fn run(&mut self, name: String, source: String, editor: &Editor, from_console: bool) {
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
        self.running = Some(Running {
            name,
            stop,
            started: Instant::now(),
            before: editor.doc.clone(),
            result,
            from_console,
        });
    }

    /// Shows a running script's progress, and what the last one said; takes
    /// a finished script's result.
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        self.finish(editor);
        // The console has its own Stop button and shows its own output.
        if let Some(running) = self.running.as_ref().filter(|r| !r.from_console) {
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
        let report = match result {
            Ok(outcome) if editor.doc != running.before => Some(Report {
                name,
                log: outcome.log,
                problem: Some(
                    "The project was edited while the script ran, so the script's changes weren't applied. Run it again to apply them."
                        .to_owned(),
                ),
            }),
            Ok(outcome) => {
                // The script's copy becomes the project: one undo step,
                // named after the script.
                editor.commit_history();
                editor.doc = outcome.document;
                let label = if name == "Script Console" { name.clone() } else { format!("Run Script “{name}”") };
                editor.history.commit_as(&editor.doc, label);
                editor.path = outcome.path;
                if let Some(selection) = outcome.selection {
                    editor.select_many(&selection);
                }
                (!outcome.log.is_empty()).then_some(Report { name, log: outcome.log, problem: None })
            }
            Err(error) => Some(Report { name, log: error.log().to_vec(), problem: Some(error.to_string()) }),
        };
        if !running.from_console {
            self.report = report;
            return;
        }
        if let Some(report) = report {
            self.console_lines.extend(report.log.into_iter().map(Line::Printed));
            if let Some(problem) = report.problem {
                self.console_lines.push(Line::Failed(problem));
            }
        }
    }

    /// The Script Console: a box to type a script in, Run and Stop, and
    /// what scripts printed.
    pub fn console(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let running = self.running.as_ref().filter(|r| r.from_console);
        let mut run = false;
        ui.horizontal(|ui| {
            match running {
                Some(running) => {
                    if ui.button("Stop").clicked() {
                        running.stop.store(true, Ordering::Relaxed);
                    }
                    ui.spinner();
                }
                None => {
                    run |= ui
                        .add_enabled(self.running.is_none(), egui::Button::new("Run"))
                        .on_hover_text(
                            "Run the script on the open project (Cmd+Enter). Edit › Undo takes it back.",
                        )
                        .clicked();
                }
            }
            if ui.button("Clear").on_hover_text("Clear the output").clicked() {
                self.console_lines.clear();
            }
        });
        let height = ui.available_height();
        let editor_box =
            egui::ScrollArea::vertical().id_salt("console-source").max_height(height * 0.55).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.console_source)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(8),
                )
            });
        // Cmd+Enter runs, from the box. (Enter alone types a new line; the
        // timeline's Enter-to-play stands aside while typing.)
        if editor_box.inner.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
        {
            run = true;
        }
        if run && self.running.is_none() {
            self.console_lines.push(Line::Ran(format!("Ran at frame {}", editor.frame + 1)));
            self.run("Script Console".to_owned(), self.console_source.clone(), editor, true);
        }
        ui.separator();
        egui::ScrollArea::vertical().id_salt("console-output").stick_to_bottom(true).auto_shrink(false).show(
            ui,
            |ui| {
                if self.console_lines.is_empty() {
                    ui.weak("What the script prints with console.log appears here.");
                }
                for line in &self.console_lines {
                    match line {
                        Line::Ran(text) => {
                            ui.add_space(4.0);
                            ui.weak(text);
                        }
                        Line::Printed(text) => {
                            ui.monospace(text);
                        }
                        Line::Failed(text) => {
                            ui.colored_label(ui.visuals().error_fg_color, text);
                        }
                    }
                }
            },
        );
    }
}

/// "stagger-layers.js" → "stagger-layers".
fn name_of(path: &Path) -> String {
    path.file_stem().map_or_else(|| "script".to_owned(), |s| s.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_script_is_built_in() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scripts");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".js"))
            .collect();
        on_disk.sort();
        let built_in: Vec<&str> = EXAMPLES.iter().map(|(name, _)| *name).collect();
        assert_eq!(on_disk, built_in, "add new example scripts to EXAMPLES");
    }

    #[test]
    fn a_scripts_changes_are_one_undo_step_named_after_it() {
        let mut editor = Editor::new(inkframe_core::Document::default());
        let mut scripts = Scripts::default();
        let source = r#"inkframe.document.addLayer("Ball"); inkframe.document.addLayer("Floor");"#;
        scripts.run("bouncing-ball".into(), source.into(), &editor, false);
        let started = Instant::now();
        while scripts.running.is_some() && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(10));
            scripts.finish(&mut editor);
        }
        assert_eq!(editor.doc.layers.len(), 2);
        assert_eq!(editor.history.next_labels().0, Some("Run Script “bouncing-ball”"));
        editor.undo();
        assert!(editor.doc.layers.is_empty(), "both layers go in one step");
    }
}
