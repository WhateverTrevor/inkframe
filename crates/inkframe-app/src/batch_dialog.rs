//! File › Batch Export: a queue of projects and formats, exported one
//! after another on a background thread into one folder.
//!
//! The work itself is `inkframe_export::batch`, shared with
//! `inkframe --export`. Jobs read their projects from disk, so the open
//! project is exported as last saved; the window says so when that
//! matters.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use inkframe_export::batch::{self, BatchEvent, BatchJob};
use inkframe_export::{ExportFormat, Progress, ffmpeg};

use crate::app::FILE_EXTENSION;
use crate::export_dialog::{file_label, reveal_in_file_manager};
use crate::icons::{Icon, IconButton};

const SIZES: [(&str, f32); 4] = [("25%", 0.25), ("50%", 0.5), ("100%", 1.0), ("200%", 2.0)];

#[derive(Debug, Clone)]
enum Status {
    Waiting,
    Running(Progress),
    Done(PathBuf),
    Failed(String),
}

struct Row {
    job: BatchJob,
    status: Status,
}

/// A batch running on its thread.
struct Running {
    events: Receiver<BatchEvent>,
    cancel: Arc<AtomicBool>,
    /// The folder it's writing to, for Show Folder afterwards.
    folder: PathBuf,
}

#[derive(Default)]
pub struct BatchDialog {
    pub open: bool,
    rows: Vec<Row>,
    /// Where exports go; until one is chosen, an Exports folder beside the
    /// first project.
    folder: Option<PathBuf>,
    running: Option<Running>,
    /// The folder the last finished batch wrote to.
    finished: Option<PathBuf>,
    ffmpeg_found: bool,
}

impl BatchDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.ffmpeg_found = ffmpeg::locate().is_some();
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Adds `project` to the queue, in the format of the last row (or
    /// ProRes 4444, as the Export window starts with).
    pub fn add(&mut self, project: PathBuf) {
        let job = match self.rows.last() {
            Some(last) => BatchJob { project, ..last.job.clone() },
            None => BatchJob::new(project, ExportFormat::ProRes4444),
        };
        self.rows.push(Row { job, status: Status::Waiting });
        self.finished = None;
    }

    /// Puts back a queue saved last time (see `prefs.rs`).
    pub fn restore(&mut self, jobs: Vec<BatchJob>, folder: Option<PathBuf>) {
        self.rows = jobs.into_iter().map(|job| Row { job, status: Status::Waiting }).collect();
        self.folder = folder;
    }

    /// The folder chosen to export to, if one was.
    pub fn chosen_folder(&self) -> Option<PathBuf> {
        self.folder.clone()
    }

    pub fn set_folder(&mut self, folder: PathBuf) {
        self.folder = Some(folder);
    }

    /// Sets the format of the last row added, for screenshots.
    pub fn set_last_format(&mut self, format: ExportFormat) {
        if let Some(row) = self.rows.last_mut() {
            row.job.format = format;
        }
    }

    /// `current` is the open project's file, if it has one, and whether it
    /// has changes that aren't saved.
    pub fn show(&mut self, ctx: &egui::Context, current: Option<(&Path, bool)>) {
        self.poll();
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Batch Export").open(&mut open).collapsible(false).default_width(620.0).show(
            ctx,
            |ui| {
                ui.add_enabled_ui(self.running.is_none(), |ui| self.queue_ui(ui, current));
                ui.separator();
                self.action_ui(ui, ctx);
            },
        );
        // As with Export, the window stays while the batch runs, so it can
        // still be watched and cancelled.
        self.open = open || self.running.is_some();
        if self.running.is_some() {
            ctx.request_repaint();
        }
    }

    fn queue_ui(&mut self, ui: &mut egui::Ui, current: Option<(&Path, bool)>) {
        ui.horizontal(|ui| {
            if ui.button("Add Projects…").clicked()
                && let Some(paths) = rfd::FileDialog::new()
                    .set_title("Add projects to the batch")
                    .add_filter("Inkframe document", &[FILE_EXTENSION])
                    .pick_files()
            {
                for path in paths {
                    self.add(path);
                }
            }
            let this = current.map(|(path, _)| path.to_owned());
            if ui
                .add_enabled(this.is_some(), egui::Button::new("Add This Project"))
                .on_disabled_hover_text("Save the project first")
                .clicked()
                && let Some(path) = this
            {
                self.add(path);
            }
        });
        ui.add_space(4.0);

        if self.rows.is_empty() {
            ui.weak("Add projects to export them one after another. Add a project more than once to export it in several formats.");
            return;
        }
        let outputs = batch::outputs(&self.jobs(), &self.output_folder());
        let mut remove = None;
        let mut duplicate = None;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            egui::Grid::new("batch_rows").num_columns(6).spacing([10.0, 6.0]).striped(true).show(ui, |ui| {
                for (i, row) in self.rows.iter_mut().enumerate() {
                    ui.label(row.job.name()).on_hover_text(row.job.project.display().to_string());
                    format_picker(ui, i, &mut row.job.format);
                    egui::ComboBox::from_id_salt(("batch_size", i))
                        .selected_text(SIZES.iter().find(|s| s.1 == row.job.scale).map_or("", |s| s.0))
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            for (label, scale) in SIZES {
                                ui.selectable_value(&mut row.job.scale, scale, label);
                            }
                        });
                    let alpha = row.job.format.supports_alpha();
                    let mut transparent = row.job.transparent && alpha;
                    ui.add_enabled(alpha, egui::Checkbox::new(&mut transparent, "Transparent"))
                        .on_disabled_hover_text("This format has no alpha channel");
                    if alpha {
                        row.job.transparent = transparent;
                    }
                    status_ui(ui, &row.status, &row.job.project, &outputs[i]);
                    ui.horizontal(|ui| {
                        if ui
                            .add(IconButton::new(Icon::Duplicate, "Add again, to export in another format"))
                            .clicked()
                        {
                            duplicate = Some(i);
                        }
                        if ui.add(IconButton::new(Icon::Trash, "Remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.end_row();
                }
            });
        });
        if let Some(i) = duplicate {
            let job = self.rows[i].job.clone();
            self.rows.insert(i + 1, Row { job, status: Status::Waiting });
        }
        if let Some(i) = remove {
            self.rows.remove(i);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Save to");
            ui.monospace(self.output_folder().display().to_string());
            if ui.button("Choose…").clicked()
                && let Some(folder) = rfd::FileDialog::new().set_title("Save batch exports to").pick_folder()
            {
                self.folder = Some(folder);
            }
        });
        if let Some((path, true)) = current
            && self.rows.iter().any(|r| r.job.project == path)
        {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "This project has unsaved changes. The batch exports the saved file, so save first to include them.",
            );
        }
        if !self.ffmpeg_found && self.rows.iter().any(|r| r.job.format.needs_ffmpeg()) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "ffmpeg isn't installed, so the video exports will fail. PNG sequences and sprite sheets still work.",
            );
        }
    }

    fn action_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(running) = &self.running {
            let (index, progress) = self
                .rows
                .iter()
                .enumerate()
                .find_map(|(i, r)| match r.status {
                    Status::Running(p) => Some((i, p)),
                    _ => None,
                })
                .unwrap_or((0, Progress { frames_done: 0, frames_total: 1 }));
            let done =
                self.rows.iter().filter(|r| matches!(r.status, Status::Done(_) | Status::Failed(_))).count();
            let overall = (done as f32 + progress.fraction()) / self.rows.len().max(1) as f32;
            ui.add(egui::ProgressBar::new(overall).text(format!(
                "Job {} of {} · frame {} of {}",
                index + 1,
                self.rows.len(),
                progress.frames_done,
                progress.frames_total
            )));
            if ui.button("Cancel").clicked() {
                running.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }

        ui.horizontal(|ui| {
            if ui.add_enabled(!self.rows.is_empty(), egui::Button::new("Export All")).clicked() {
                self.start(ctx);
            }
            if let Some(folder) = &self.finished {
                let done = self.rows.iter().filter(|r| matches!(r.status, Status::Done(_))).count();
                ui.label(format!("{done} of {} exported", self.rows.len()));
                if ui.small_button("Show Folder").clicked() {
                    reveal_in_file_manager(folder);
                }
            }
        });
    }

    pub fn jobs(&self) -> Vec<BatchJob> {
        self.rows.iter().map(|r| r.job.clone()).collect()
    }

    fn output_folder(&self) -> PathBuf {
        self.folder.clone().unwrap_or_else(|| {
            let beside = self.rows.first().and_then(|r| r.job.project.parent()).unwrap_or(Path::new("."));
            beside.join("Exports")
        })
    }

    pub fn start(&mut self, ctx: &egui::Context) {
        let jobs = self.jobs();
        let folder = self.output_folder();
        for row in &mut self.rows {
            row.status = Status::Waiting;
        }
        let (sender, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::clone(&cancel);
        let ctx = ctx.clone();
        let thread_folder = folder.clone();
        std::thread::spawn(move || {
            batch::run(&jobs, &thread_folder, |event| {
                let _ = sender.send(event);
                ctx.request_repaint();
                if cancel_flag.load(Ordering::Relaxed) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
            // Dropping the sender tells `poll` the batch is over.
        });
        self.finished = None;
        self.running = Some(Running { events, cancel, folder });
    }

    fn poll(&mut self) {
        let Some(running) = &self.running else { return };
        loop {
            match running.events.try_recv() {
                Ok(BatchEvent::Started(i)) => {
                    self.rows[i].status = Status::Running(Progress { frames_done: 0, frames_total: 1 })
                }
                Ok(BatchEvent::Progress(i, progress)) => self.rows[i].status = Status::Running(progress),
                Ok(BatchEvent::Finished(i, result)) => {
                    self.rows[i].status = match result {
                        Ok(path) => Status::Done(path),
                        Err(message) => Status::Failed(message),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.finished = Some(running.folder.clone());
                    self.running = None;
                    return;
                }
            }
        }
    }
}

fn format_picker(ui: &mut egui::Ui, row: usize, format: &mut ExportFormat) {
    egui::ComboBox::from_id_salt(("batch_format", row)).selected_text(format.label()).width(200.0).show_ui(
        ui,
        |ui| {
            for choice in ExportFormat::ALL.into_iter().filter(|f| f.is_available()) {
                ui.selectable_value(format, choice, choice.label());
            }
        },
    );
}

fn status_ui(ui: &mut egui::Ui, status: &Status, project: &Path, output: &Path) {
    match status {
        // A queue kept from last time may name a project that's since
        // moved; better to say so now than when the batch reaches it.
        Status::Waiting if !project.exists() => {
            ui.colored_label(ui.visuals().warn_fg_color, "Project missing")
                .on_hover_text(project.display().to_string());
        }
        Status::Waiting => {
            ui.weak(format!("→ {}", file_label(output))).on_hover_text(output.display().to_string());
        }
        Status::Running(progress) => {
            ui.add(egui::ProgressBar::new(progress.fraction()).desired_width(110.0));
        }
        Status::Done(path) => {
            if ui.link(format!("✔ {}", file_label(path))).on_hover_text("Show in folder").clicked() {
                reveal_in_file_manager(path);
            }
        }
        Status::Failed(message) => {
            ui.colored_label(ui.visuals().error_fg_color, "Failed").on_hover_text(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_projects_take_the_last_rows_settings() {
        let mut dialog = BatchDialog::default();
        dialog.add("/shots/walk.inkf".into());
        dialog.rows[0].job.format = ExportFormat::H264;
        dialog.rows[0].job.scale = 0.5;
        dialog.add("/shots/run.inkf".into());
        let run = &dialog.rows[1].job;
        assert_eq!((run.name().as_str(), run.format, run.scale), ("run", ExportFormat::H264, 0.5));
    }

    #[test]
    fn exports_go_beside_the_first_project_until_a_folder_is_chosen() {
        let mut dialog = BatchDialog::default();
        dialog.add("/shots/walk.inkf".into());
        assert_eq!(dialog.output_folder(), Path::new("/shots/Exports"));
        dialog.set_folder("/renders".into());
        assert_eq!(dialog.output_folder(), Path::new("/renders"));
    }
}
