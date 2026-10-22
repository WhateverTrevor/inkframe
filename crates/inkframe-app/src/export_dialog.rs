//! The Export window and the background thread that does the work.
//!
//! Exporting can take a while (every frame is rendered at full quality, and
//! ProRes files are large), so it runs on its own thread. The UI polls a
//! channel for progress each frame and can cancel at any time.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use inkframe_core::Document;
use inkframe_export::{ExportFormat, ExportSettings, Progress, export, ffmpeg};

enum JobEvent {
    Progress(Progress),
    Done(Result<(), String>),
}

struct Job {
    events: Receiver<JobEvent>,
    cancel: Arc<AtomicBool>,
    progress: Progress,
    output: PathBuf,
}

enum Outcome {
    Success(PathBuf),
    Failure(String),
}

pub struct ExportDialog {
    pub open: bool,
    format: ExportFormat,
    transparent: bool,
    scale: f32,
    whole_animation: bool,
    /// 1-based, inclusive, as shown to the user.
    range: (u32, u32),
    job: Option<Job>,
    outcome: Option<Outcome>,
    /// Looked up when the dialog opens rather than every frame, since it
    /// touches the file system.
    ffmpeg_found: bool,
    project_dir: Option<PathBuf>,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            open: false,
            format: ExportFormat::ProRes4444,
            transparent: true,
            scale: 1.0,
            whole_animation: true,
            range: (1, 1),
            job: None,
            outcome: None,
            ffmpeg_found: false,
            project_dir: None,
        }
    }
}

impl ExportDialog {
    pub fn open(&mut self, doc: &Document) {
        self.open = true;
        self.outcome = None;
        self.ffmpeg_found = ffmpeg::locate().is_some();
        self.range = (1, doc.duration);
    }

    /// Opens the dialog set to `format`, for screenshots.
    pub fn open_with(&mut self, doc: &Document, format: ExportFormat) {
        self.open(doc);
        self.format = format;
    }

    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// `name` is used for the default output file name; `project_dir` is
    /// where audio files with relative paths are looked for.
    pub fn show(&mut self, ctx: &egui::Context, doc: &Document, name: &str, project_dir: Option<PathBuf>) {
        self.project_dir = project_dir;
        self.poll_job();
        if !self.open {
            return;
        }

        let mut open = self.open;
        egui::Window::new("Export")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(self.job.is_none(), |ui| self.settings_ui(ui, doc));
                ui.separator();
                self.action_ui(ui, ctx, doc, name);
            });
        // Don't let the window close mid-export; the job would keep running
        // with no way to see or cancel it.
        self.open = open || self.job.is_some();

        if self.job.is_some() {
            ctx.request_repaint();
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, doc: &Document) {
        egui::Grid::new("export_settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Format");
            egui::ComboBox::from_id_salt("format").selected_text(self.format.label()).width(240.0).show_ui(
                ui,
                |ui| {
                    for format in ExportFormat::ALL.into_iter().filter(|f| f.is_available()) {
                        ui.selectable_value(&mut self.format, format, format.label());
                    }
                },
            );
            ui.end_row();

            ui.label("Background");
            let alpha = self.format.supports_alpha();
            let mut transparent = self.transparent && alpha;
            ui.add_enabled(alpha, egui::Checkbox::new(&mut transparent, "Transparent"))
                .on_disabled_hover_text("This format has no alpha channel; the stage color is used");
            if alpha {
                self.transparent = transparent;
            }
            ui.end_row();

            ui.label("Resolution");
            ui.horizontal(|ui| {
                for (label, scale) in [("25%", 0.25), ("50%", 0.5), ("100%", 1.0), ("200%", 2.0)] {
                    ui.selectable_value(&mut self.scale, scale, label);
                }
                let (w, h) = inkframe_render::output_size(doc, self.scale);
                ui.weak(format!("{w} × {h}"));
            });
            ui.end_row();

            ui.label("Frames");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.whole_animation, true, "All");
                ui.radio_value(&mut self.whole_animation, false, "Range");
                ui.add_enabled_ui(!self.whole_animation, |ui| {
                    let (start, end) = &mut self.range;
                    ui.add(egui::DragValue::new(start).range(1..=doc.duration));
                    ui.label("to");
                    ui.add(egui::DragValue::new(end).range(*start..=doc.duration));
                });
            });
            ui.end_row();
        });

        ui.weak(format!("{} · {}", doc.frame_rate, describe(self.format)));
        if self.format == ExportFormat::SpriteSheet {
            ui.weak("Frames are cropped to what's drawn, with a .json file beside the image.");
        }
        // The vector formats draw filters and particles as pictures, which
        // in a Lottie file means one per frame.
        if self.format.is_vector() {
            let pictures = doc
                .all_layers()
                .filter(|l| {
                    l.particles.is_some() || l.track.effects.keys().iter().any(|k| !k.value.is_none())
                })
                .count();
            if pictures > 0 {
                let what = if self.format == ExportFormat::Lottie {
                    "as a picture on every frame, which can make the file large; a smaller size helps"
                } else {
                    "as pictures, since SVG can't draw them as shapes"
                };
                let plural = if pictures == 1 { " has" } else { "s have" };
                ui.weak(format!(
                    "{pictures} layer{plural} filters, colour effects or particles: they go in {what}."
                ));
            }
        }
        if self.format.needs_ffmpeg() && !self.ffmpeg_found {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "ffmpeg isn't installed. Install it with `brew install ffmpeg`, or export a PNG sequence.",
            );
        }
    }

    fn action_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, doc: &Document, name: &str) {
        if let Some(job) = &self.job {
            let progress = job.progress;
            ui.add(
                egui::ProgressBar::new(progress.fraction())
                    .text(format!("Frame {} of {}", progress.frames_done, progress.frames_total)),
            );
            if ui.button("Cancel").clicked() {
                job.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }

        match &self.outcome {
            Some(Outcome::Success(path)) => {
                ui.horizontal(|ui| {
                    ui.label(format!("✔ Exported {}", file_label(path)));
                    if ui.small_button("Show").clicked() {
                        reveal_in_file_manager(path);
                    }
                });
            }
            Some(Outcome::Failure(message)) => {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
            None => {}
        }

        let can_export = !self.format.needs_ffmpeg() || self.ffmpeg_found;
        if ui.add_enabled(can_export, egui::Button::new("Export…")).clicked() {
            if let Some(output) = self.ask_for_output(name) {
                self.start(ctx, doc, output);
            }
        }
    }

    /// The 0-based frames to export.
    fn frames(&self, doc: &Document) -> std::ops::RangeInclusive<u32> {
        if self.whole_animation { 0..=doc.last_frame() } else { self.range.0 - 1..=self.range.1 - 1 }
    }

    fn ask_for_output(&self, name: &str) -> Option<PathBuf> {
        let extension = self.format.extension();
        let (title, filter) = match self.format {
            ExportFormat::PngSequence => {
                ("Export PNG sequence (frames are numbered automatically)", "PNG image")
            }
            ExportFormat::SpriteSheet => {
                ("Export sprite sheet (a .json file is written beside it)", "PNG image")
            }
            _ => ("Export video", "Video"),
        };
        rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(format!("{name}.{extension}"))
            .add_filter(filter, &[extension])
            .save_file()
    }

    fn start(&mut self, ctx: &egui::Context, doc: &Document, output: PathBuf) {
        let frames = self.frames(doc);
        let settings = ExportSettings {
            format: self.format,
            output: output.clone(),
            frames,
            scale: self.scale,
            transparent: self.transparent,
            project_dir: self.project_dir.clone(),
        };

        let (sender, events) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let total = settings.frames.end() - settings.frames.start() + 1;

        // The thread gets its own copy of the document, so the user can keep
        // editing while the export runs without affecting what's exported.
        let mut doc = doc.clone();
        // What's exported is always posed, whatever the stage is showing.
        doc.rest_pose.0 = false;
        let ctx = ctx.clone();
        let cancel_flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            let result = export(&doc, &settings, |progress| {
                let _ = sender.send(JobEvent::Progress(progress));
                // Wake the UI so the progress bar moves even if the mouse doesn't.
                ctx.request_repaint();
                if cancel_flag.load(Ordering::Relaxed) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
            let _ = sender.send(JobEvent::Done(result.map_err(|e| e.to_string())));
            ctx.request_repaint();
        });

        self.outcome = None;
        self.job =
            Some(Job { events, cancel, progress: Progress { frames_done: 0, frames_total: total }, output });
    }

    fn poll_job(&mut self) {
        let Some(job) = &mut self.job else { return };
        while let Ok(event) = job.events.try_recv() {
            match event {
                JobEvent::Progress(progress) => job.progress = progress,
                JobEvent::Done(result) => {
                    self.outcome = Some(match result {
                        Ok(()) => Outcome::Success(job.output.clone()),
                        Err(message) => Outcome::Failure(message),
                    });
                    self.job = None;
                    return;
                }
            }
        }
    }
}

/// One-line guidance on what each format is for.
fn describe(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::PngSequence => "lossless frames with alpha",
        ExportFormat::ProRes4444 => "master quality with alpha, for editing and compositing",
        ExportFormat::ProRes4444Xq => "highest-quality master with alpha",
        ExportFormat::ProRes422Hq => "high quality, no alpha, smaller files",
        ExportFormat::H264 => "small files for review and the web, no alpha",
        ExportFormat::HevcWithAlpha => "small files with alpha for Apple devices and Safari",
        ExportFormat::SpriteSheet => "every frame on one image, for game engines",
        ExportFormat::Gif => "a looping animation to share anywhere, 256 colours",
        ExportFormat::Svg => "vectors, one file per frame, for the web and drawing programs",
        ExportFormat::Lottie => "a vector animation for web pages and apps",
    }
}

pub(crate) fn file_label(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Opens Finder / Explorer with the exported file selected.
pub(crate) fn reveal_in_file_manager(path: &Path) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(path).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg("/select,").arg(path).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn()
    };
    if let Err(err) = result {
        eprintln!("couldn't open the file manager: {err}");
    }
}
