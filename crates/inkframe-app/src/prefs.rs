//! Settings that carry over to the next time Inkframe runs: the framing
//! guides, the Batch Export queue, where drawing goes, whether the
//! first-run tour has been seen, and which Properties sections are folded.
//!
//! Like the window layout (see `layout.rs`), these are the person's, not
//! the document's, so they live in their own file in the settings folder.
//! The app gathers them each frame and saves them when they change.
//! Anything unreadable in the file (a format from a newer Inkframe, a
//! batch job for a format this build lacks) is skipped rather than
//! failing the whole file.

use std::path::PathBuf;

use inkframe_export::ExportFormat;
use inkframe_export::batch::BatchJob;
use serde::{Deserialize, Serialize};

use crate::guides::{Guides, SafeAreas};
use crate::layout::settings_dir;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub safe_areas: SafeAreas,
    pub field_guide: bool,
    pub batch: Vec<SavedJob>,
    /// Where Batch Export writes, if a folder was chosen.
    pub batch_folder: Option<PathBuf>,
    /// Whether "Draw into the selected layer" was turned off. Saved this
    /// way round so a missing value (an older file) means the default, on.
    pub shape_per_layer: bool,
    /// View › Rulers, and Guides' settings, saved so a missing value means
    /// the default (no rulers; guides shown, unlocked, snapped to).
    pub show_rulers: bool,
    pub hide_guides: bool,
    pub lock_guides: bool,
    pub no_guide_snapping: bool,
    /// Whether the first-run tour has been seen (finished or skipped).
    pub toured: bool,
    /// The Properties sections folded away.
    pub folded_sections: Vec<String>,
    /// The newest version whose What's New has been seen.
    pub seen_version: Option<String>,
    /// The language picked in View › Language, by its code (`de`,
    /// `pt_BR`); none follows the system's.
    pub language: Option<String>,
    /// The Brush and Eraser tools' brushes as last left, and the name of
    /// the brush last picked.
    pub brush: Option<inkframe_core::BrushSettings>,
    pub eraser: Option<inkframe_core::BrushSettings>,
    pub brush_name: Option<String>,
    /// Symmetry painting, as last left.
    pub symmetry: inkframe_core::Symmetry,
}

/// A Batch Export job as saved: the format by its short name, the same
/// one the command line uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedJob {
    pub project: PathBuf,
    pub format: String,
    pub scale: f32,
    pub transparent: bool,
}

impl Prefs {
    /// The settings as they are now.
    pub fn gather(guides: Guides, jobs: Vec<BatchJob>, batch_folder: Option<PathBuf>) -> Self {
        let batch = jobs
            .into_iter()
            .map(|job| SavedJob {
                project: job.project,
                format: job.format.slug().to_owned(),
                scale: job.scale,
                transparent: job.transparent,
            })
            .collect();
        Self {
            safe_areas: guides.safe_areas,
            field_guide: guides.field_guide,
            batch,
            batch_folder,
            shape_per_layer: false,
            show_rulers: false,
            hide_guides: false,
            lock_guides: false,
            no_guide_snapping: false,
            toured: false,
            folded_sections: Vec::new(),
            seen_version: None,
            language: None,
            brush: None,
            eraser: None,
            brush_name: None,
            symmetry: inkframe_core::Symmetry::Off,
        }
    }

    pub fn guides(&self) -> Guides {
        Guides { safe_areas: self.safe_areas, field_guide: self.field_guide }
    }

    /// The saved batch, leaving out jobs in formats this build can't make.
    pub fn batch_jobs(&self) -> Vec<BatchJob> {
        self.batch
            .iter()
            .filter_map(|job| {
                let format = ExportFormat::from_slug(&job.format).filter(|f| f.is_available())?;
                Some(BatchJob {
                    project: job.project.clone(),
                    format,
                    scale: job.scale,
                    transparent: job.transparent,
                })
            })
            .collect()
    }

    fn file() -> Option<PathBuf> {
        settings_dir().map(|dir| dir.join("prefs.json"))
    }

    pub fn load() -> Self {
        Self::file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::file() else { return };
        if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_string_pretty(self)) {
            let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, json));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_come_back_as_they_were_saved() {
        let guides = Guides { safe_areas: SafeAreas::Classic, field_guide: true };
        let jobs = vec![
            BatchJob { scale: 0.5, ..BatchJob::new("/shots/walk.inkf", ExportFormat::H264) },
            BatchJob::new("/shots/run.inkf", ExportFormat::SpriteSheet),
        ];
        let prefs = Prefs {
            language: Some("pt_BR".into()),
            ..Prefs::gather(guides, jobs.clone(), Some("/renders".into()))
        };
        let loaded: Prefs = serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(loaded.language.as_deref(), Some("pt_BR"));
        assert_eq!(loaded.guides(), guides);
        assert_eq!(loaded.batch_jobs(), jobs);
        assert_eq!(loaded.batch_folder, Some(PathBuf::from("/renders")));
    }

    #[test]
    fn unknown_formats_and_missing_settings_are_skipped() {
        let json = r#"{"batch": [
            {"project": "a.inkf", "format": "flipbook", "scale": 1.0, "transparent": true},
            {"project": "b.inkf", "format": "png", "scale": 1.0, "transparent": true}
        ]}"#;
        let prefs: Prefs = serde_json::from_str(json).unwrap();
        assert_eq!(prefs.guides(), Guides::default());
        let jobs = prefs.batch_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].format, ExportFormat::PngSequence);
    }
}
