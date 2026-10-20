//! Autosave and crash recovery.
//!
//! While the open project has unsaved changes, a copy of it is written
//! every [`INTERVAL`] to the Recovery folder in the settings folder,
//! beside a small note saying which project it is and when it was saved.
//! Saving, or quitting normally (even choosing not to save), deletes the
//! copy. So a copy that's still there when Inkframe starts means it didn't
//! close properly, and it's offered back (see `app.rs`).
//!
//! The copy is written on a background thread from a snapshot of the
//! document, so a large project never makes drawing stutter. It goes to a
//! temporary name first and is then renamed into place, so a crash while
//! writing leaves the previous copy rather than half a file.
//!
//! Several copies of Inkframe can run at once, each with its own copy
//! (named for its session). Each one refreshes its note's heartbeat while
//! it runs, and only copies whose heartbeat has gone quiet count as left
//! behind, so one Inkframe never offers to recover another's work while
//! it's still open.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use inkframe_core::Document;
use serde::{Deserialize, Serialize};

use crate::editor::Editor;
use crate::layout::settings_dir;

/// How often unsaved work is copied.
pub const INTERVAL: Duration = Duration::from_secs(30);
/// How often a running Inkframe says it's still there.
const HEARTBEAT: Duration = Duration::from_secs(30);
/// How long a copy's heartbeat must be quiet before it's left behind.
const STALE: u64 = 120;

/// The note beside each copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Note {
    /// The project's own file, if it has been saved before.
    original: Option<PathBuf>,
    /// What to call it: the file's name, or "Untitled".
    name: String,
    /// When the copy was written, in seconds since 1970.
    saved_at: u64,
    /// When its Inkframe last said it was still running.
    heartbeat: u64,
}

/// A copy left behind by an Inkframe that didn't close properly.
#[derive(Debug, Clone, PartialEq)]
pub struct Recovery {
    pub name: String,
    pub original: Option<PathBuf>,
    /// How long ago it was saved, in seconds.
    pub age: u64,
    copy: PathBuf,
    note: PathBuf,
}

impl Recovery {
    /// Opens the copy as the project it was, with its changes unsaved.
    pub fn open(&self) -> Result<Editor, String> {
        let bytes = std::fs::read(&self.copy).map_err(|e| e.to_string())?;
        let doc = Document::from_bytes(&bytes).map_err(|e| e.to_string())?;
        Ok(Editor::recovered(doc, self.original.clone()))
    }

    /// Deletes the copy and its note.
    pub fn discard(&self) {
        let _ = std::fs::remove_file(&self.copy);
        let _ = std::fs::remove_file(&self.note);
    }

    /// "3 minutes ago", for the recovery window.
    pub fn when(&self) -> String {
        match self.age {
            0..60 => "just now".to_owned(),
            60..120 => "a minute ago".to_owned(),
            120..3600 => format!("{} minutes ago", self.age / 60),
            3600..7200 => "an hour ago".to_owned(),
            7200..86_400 => format!("{} hours ago", self.age / 3600),
            _ => format!("{} days ago", self.age / 86_400),
        }
    }
}

pub struct Autosave {
    /// Where copies go; `None` if there's no settings folder.
    dir: Option<PathBuf>,
    /// This session's name for its copy.
    session: String,
    /// The document as last copied, so an unchanged one isn't copied again.
    copied: Option<Document>,
    last_check: Instant,
    last_heartbeat: Instant,
    writing: Option<JoinHandle<()>>,
}

impl Autosave {
    pub fn new() -> Self {
        Self::in_dir(settings_dir().map(|dir| dir.join("Recovery")))
    }

    fn in_dir(dir: Option<PathBuf>) -> Self {
        // The process and the moment it started: different for every run,
        // and for copies of Inkframe running side by side.
        let started = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        Self {
            dir,
            session: format!("{}-{started}", std::process::id()),
            copied: None,
            last_check: Instant::now(),
            last_heartbeat: Instant::now(),
            writing: None,
        }
    }

    /// Leaves `editor`'s work in `dir` as a crashed Inkframe would, and
    /// returns an autosave looking there: for showing the recovery window
    /// in screenshots.
    pub fn simulate_crash(editor: &Editor, dir: PathBuf) -> Self {
        let _ = std::fs::remove_dir_all(&dir);
        let mut crashed = Self::in_dir(Some(dir.clone()));
        crashed.copy(editor);
        crashed.finish();
        if let Some(note) = crashed.note_path()
            && let Ok(json) = std::fs::read_to_string(&note)
            && let Ok(mut contents) = serde_json::from_str::<Note>(&json)
        {
            // Saved a few minutes before it stopped.
            contents.saved_at -= 4 * 60;
            contents.heartbeat -= STALE + 1;
            write_json(&note, &contents);
        }
        Self::in_dir(Some(dir))
    }

    fn copy_path(&self) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{}.inkf", self.session)))
    }

    fn note_path(&self) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(format!("{}.json", self.session)))
    }

    /// Call every frame: copies unsaved work when it's time, keeps the
    /// heartbeat going, and drops the copy once the work is saved.
    pub fn tick(&mut self, editor: &Editor) {
        if self.last_check.elapsed() >= INTERVAL {
            self.last_check = Instant::now();
            if editor.has_unsaved_changes() {
                self.copy(editor);
            } else if self.copied.is_some() {
                self.clear();
            }
        }
        if self.copied.is_some() && self.last_heartbeat.elapsed() >= HEARTBEAT {
            self.last_heartbeat = Instant::now();
            if let Some(note) = self.note_path()
                && let Ok(json) = std::fs::read_to_string(&note)
                && let Ok(mut contents) = serde_json::from_str::<Note>(&json)
            {
                contents.heartbeat = now();
                write_json(&note, &contents);
            }
        }
    }

    /// Copies the open project now, on a background thread, unless it's
    /// unchanged since the last copy or a copy is still being written.
    fn copy(&mut self, editor: &Editor) {
        if self.copied.as_ref() == Some(&editor.doc)
            || self.writing.as_ref().is_some_and(|w| !w.is_finished())
        {
            return;
        }
        let (Some(copy), Some(note_path)) = (self.copy_path(), self.note_path()) else { return };
        let doc = editor.doc.clone();
        self.copied = Some(doc.clone());
        let note = Note {
            original: editor.path.clone(),
            name: editor.display_name(),
            saved_at: now(),
            heartbeat: now(),
        };
        self.writing = Some(std::thread::spawn(move || {
            let Ok(bytes) = doc.to_bytes() else { return };
            if let Some(dir) = copy.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let partial = copy.with_extension("partial");
            if std::fs::write(&partial, bytes).and_then(|_| std::fs::rename(&partial, &copy)).is_ok() {
                write_json(&note_path, &note);
            }
        }));
    }

    /// Deletes this session's copy: the work is saved, or was let go.
    pub fn clear(&mut self) {
        self.finish();
        self.copied = None;
        for path in [self.copy_path(), self.note_path()].into_iter().flatten() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Waits for a copy being written.
    fn finish(&mut self) {
        if let Some(writing) = self.writing.take() {
            let _ = writing.join();
        }
    }

    /// Copies left behind by Inkframes that didn't close properly, newest
    /// first. This session's own copy is never one of them.
    pub fn left_behind(&self) -> Vec<Recovery> {
        let Some(dir) = &self.dir else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let now = now();
        let mut found: Vec<Recovery> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter(|p| p.file_stem().is_some_and(|s| s.to_string_lossy() != self.session))
            .filter_map(|note_path| {
                let note: Note = serde_json::from_str(&std::fs::read_to_string(&note_path).ok()?).ok()?;
                let copy = note_path.with_extension("inkf");
                (copy.is_file() && now.saturating_sub(note.heartbeat) > STALE).then(|| Recovery {
                    name: note.name,
                    original: note.original,
                    age: now.saturating_sub(note.saved_at),
                    copy,
                    note: note_path,
                })
            })
            .collect();
        found.sort_by_key(|r| r.age);
        found
    }
}

impl Drop for Autosave {
    fn drop(&mut self) {
        self.finish();
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn write_json(path: &Path, note: &Note) {
    if let Ok(json) = serde_json::to_string_pretty(note) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Color, Element, Point, Shape, Style, Transform};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkframe-autosave-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// An editor for a saved project, with a change not yet saved.
    fn edited(project: &Path) -> Editor {
        let mut editor = Editor::new(Document::default());
        editor.path = Some(project.to_owned());
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 5.0, radius_y: 5.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        editor.doc.add_layer("Dot", vec![dot], 0, Transform::at(Point::new(10.0, 10.0)));
        editor
    }

    /// Makes `autosave`'s copy look like it belongs to an Inkframe that
    /// stopped long ago.
    fn go_quiet(autosave: &Autosave) {
        let note = autosave.note_path().unwrap();
        let mut contents: Note = serde_json::from_str(&std::fs::read_to_string(&note).unwrap()).unwrap();
        contents.heartbeat -= STALE + 1;
        write_json(&note, &contents);
    }

    #[test]
    fn unsaved_work_left_behind_comes_back_as_its_project() {
        let dir = scratch("recover");
        let project = PathBuf::from("/projects/walk.inkf");
        let editor = edited(&project);

        let mut crashed = Autosave::in_dir(Some(dir.clone()));
        crashed.copy(&editor);
        crashed.finish();
        go_quiet(&crashed);

        let next_run = Autosave::in_dir(Some(dir.clone()));
        let found = next_run.left_behind();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "walk");
        let recovered = found[0].open().unwrap();
        assert_eq!(recovered.doc, editor.doc);
        assert_eq!(recovered.path, Some(project));
        assert!(recovered.has_unsaved_changes(), "recovered work still needs saving");

        found[0].discard();
        assert!(next_run.left_behind().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_copy_another_inkframe_is_still_keeping_isnt_offered() {
        let dir = scratch("running");
        let mut running = Autosave::in_dir(Some(dir.clone()));
        running.copy(&edited(Path::new("/projects/a.inkf")));
        running.finish();
        assert!(Autosave::in_dir(Some(dir.clone())).left_behind().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saving_or_quitting_removes_the_copy() {
        let dir = scratch("clear");
        let mut autosave = Autosave::in_dir(Some(dir.clone()));
        autosave.copy(&edited(Path::new("/projects/a.inkf")));
        autosave.finish();
        assert!(autosave.copy_path().unwrap().is_file());
        autosave.clear();
        assert!(!autosave.copy_path().unwrap().exists() && !autosave.note_path().unwrap().exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ages_read_naturally() {
        let at = |age| Recovery { age, ..recovery() };
        assert_eq!(at(10).when(), "just now");
        assert_eq!(at(300).when(), "5 minutes ago");
        assert_eq!(at(3 * 86_400).when(), "3 days ago");
    }

    fn recovery() -> Recovery {
        Recovery { name: String::new(), original: None, age: 0, copy: PathBuf::new(), note: PathBuf::new() }
    }
}
