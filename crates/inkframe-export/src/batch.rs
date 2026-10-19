//! Batch export: several project files, or one project in several formats,
//! exported one after another into a single folder, as Harmony's batch
//! render does.
//!
//! The File › Batch Export window and `inkframe --export` on the command
//! line both build a list of [`BatchJob`]s and run them with [`run`]. Each
//! job opens its project from disk, so a batch exports what was last
//! saved, not what's open in the editor.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use inkframe_core::Document;

use crate::{ExportError, ExportFormat, ExportSettings, Progress, export};

/// One project in one format.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchJob {
    pub project: PathBuf,
    pub format: ExportFormat,
    /// Resolution multiplier; 1.0 is the document's own size.
    pub scale: f32,
    /// Leave the background transparent, for formats with alpha.
    pub transparent: bool,
}

impl BatchJob {
    pub fn new(project: impl Into<PathBuf>, format: ExportFormat) -> Self {
        Self { project: project.into(), format, scale: 1.0, transparent: true }
    }

    /// The project's file name without its extension, which names what
    /// it exports.
    pub fn name(&self) -> String {
        self.project.file_stem().map_or_else(|| "untitled".to_owned(), |s| s.to_string_lossy().into_owned())
    }

    /// Opens the project and exports the whole animation to `output`.
    pub fn run(
        &self,
        output: &Path,
        on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<(), ExportError> {
        let doc = Document::load(&self.project).map_err(|e| ExportError::Open(e.to_string()))?;
        if let Some(folder) = output.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let settings = ExportSettings {
            format: self.format,
            output: output.to_owned(),
            frames: 0..=doc.last_frame(),
            scale: self.scale,
            transparent: self.transparent,
            project_dir: self.project.parent().map(Path::to_path_buf),
        };
        export(&doc, &settings, on_progress)
    }
}

/// Where each job's export goes in `folder`: named after its project
/// (`walk.mov`), with the format added where two jobs would otherwise
/// write the same file (`walk-prores4444.mov`, `walk-prores422hq.mov`).
/// PNG sequences get a folder of their own, so their frames don't mix
/// with everything else.
pub fn outputs(jobs: &[BatchJob], folder: &Path) -> Vec<PathBuf> {
    let plain = |job: &BatchJob| file_name(&job.name(), job.format);
    jobs.iter()
        .map(|job| {
            let name = plain(job);
            let clashes = jobs.iter().filter(|other| plain(other).eq_ignore_ascii_case(&name)).count() > 1;
            let stem = if clashes { format!("{}-{}", job.name(), job.format.slug()) } else { job.name() };
            match job.format {
                ExportFormat::PngSequence => folder.join(&stem).join(file_name(&stem, job.format)),
                _ => folder.join(file_name(&stem, job.format)),
            }
        })
        .collect()
}

fn file_name(stem: &str, format: ExportFormat) -> String {
    format!("{stem}.{}", format.extension())
}

/// Reported while a batch runs.
#[derive(Debug, Clone)]
pub enum BatchEvent {
    /// Job `index` has started.
    Started(usize),
    Progress(usize, Progress),
    /// Job `index` finished, or failed with the message.
    Finished(usize, Result<PathBuf, String>),
}

/// Runs `jobs` in order, exporting into `folder`. A job that fails is
/// reported and the rest still run, so one missing file doesn't cost a
/// whole night's batch. Returning `Break` from `on_event` stops the
/// running job (cleaning up after it, as a single export does) and skips
/// the rest. Returns how many jobs succeeded.
pub fn run(
    jobs: &[BatchJob],
    folder: &Path,
    mut on_event: impl FnMut(BatchEvent) -> ControlFlow<()>,
) -> usize {
    let mut succeeded = 0;
    for ((index, job), output) in jobs.iter().enumerate().zip(outputs(jobs, folder)) {
        if on_event(BatchEvent::Started(index)).is_break() {
            break;
        }
        let mut stopped = false;
        let result = job.run(&output, |progress| {
            let flow = on_event(BatchEvent::Progress(index, progress));
            stopped |= flow.is_break();
            flow
        });
        if result.is_ok() {
            succeeded += 1;
        }
        let flow = on_event(BatchEvent::Finished(index, result.map(|()| output).map_err(|e| e.to_string())));
        if stopped || flow.is_break() {
            break;
        }
    }
    succeeded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_are_named_after_projects() {
        let jobs = [
            BatchJob::new("/shots/walk.inkf", ExportFormat::H264),
            BatchJob::new("/shots/run.inkf", ExportFormat::H264),
        ];
        let out = outputs(&jobs, Path::new("/renders"));
        assert_eq!(out, [PathBuf::from("/renders/walk.mp4"), PathBuf::from("/renders/run.mp4")]);
    }

    #[test]
    fn formats_sharing_an_extension_get_told_apart() {
        let jobs = [
            BatchJob::new("/shots/walk.inkf", ExportFormat::ProRes4444),
            BatchJob::new("/shots/walk.inkf", ExportFormat::ProRes422Hq),
            BatchJob::new("/shots/walk.inkf", ExportFormat::H264),
        ];
        let out = outputs(&jobs, Path::new("/renders"));
        assert_eq!(out[0], Path::new("/renders/walk-prores4444.mov"));
        assert_eq!(out[1], Path::new("/renders/walk-prores422hq.mov"));
        assert_eq!(out[2], Path::new("/renders/walk.mp4"));
    }

    #[test]
    fn png_sequences_get_their_own_folder() {
        let jobs = [BatchJob::new("/shots/walk.inkf", ExportFormat::PngSequence)];
        assert_eq!(outputs(&jobs, Path::new("/renders"))[0], Path::new("/renders/walk/walk.png"));
    }
}
