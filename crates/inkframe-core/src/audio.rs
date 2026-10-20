//! Audio tracks: sound files laid against the main timeline.
//!
//! The document only *refers* to audio files; it never embeds them. Project
//! files stay small and readable, and the audio stays editable in other
//! tools. Paths are stored relative to the project file when possible, so a
//! project folder can be moved or shared as a whole (see [`AudioTrack::resolve`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AudioId(pub(crate) u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub id: AudioId,
    pub name: String,
    /// The sound file, relative to the project file's folder if it was
    /// inside it when imported, otherwise absolute.
    pub path: PathBuf,
    /// The frame where the sound starts playing.
    #[serde(default)]
    pub start: u32,
    /// 1.0 plays the file at its own level.
    #[serde(default = "unity")]
    pub volume: f32,
    #[serde(default)]
    pub muted: bool,
    /// The Library folder it's filed in; empty for the top level.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

impl AudioTrack {
    /// The file to actually open, given the folder the project file is in.
    ///
    /// Relative paths are taken from the project folder. If an absolute path
    /// no longer exists (the project was copied to another machine, say),
    /// a file of the same name next to the project is used instead.
    pub fn resolve(&self, project_dir: Option<&Path>) -> PathBuf {
        resolve_linked(&self.path, project_dir)
    }
}

/// Where a file linked from the project really is. Shared by every kind of
/// linked file (sounds, images), so they all follow the same rules.
pub(crate) fn resolve_linked(path: &Path, project_dir: Option<&Path>) -> PathBuf {
    match project_dir {
        Some(dir) if path.is_relative() => dir.join(path),
        Some(dir) if !path.exists() => match path.file_name() {
            Some(name) if dir.join(name).exists() => dir.join(name),
            _ => path.to_path_buf(),
        },
        _ => path.to_path_buf(),
    }
}

impl Document {
    /// Adds an audio track starting at `start`. `path` is stored relative to
    /// `project_dir` when the file is inside it.
    pub fn add_audio(&mut self, path: &Path, project_dir: Option<&Path>, start: u32) -> AudioId {
        let id = AudioId(self.next_audio_id);
        self.next_audio_id += 1;
        let stored = project_dir
            .and_then(|dir| path.strip_prefix(dir).ok())
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf);
        let name = path.file_stem().map_or_else(|| "Audio".to_owned(), |s| s.to_string_lossy().into_owned());
        self.audio.push(AudioTrack {
            id,
            name,
            path: stored,
            start,
            volume: 1.0,
            muted: false,
            folder: String::new(),
        });
        id
    }

    pub fn audio_track(&self, id: AudioId) -> Option<&AudioTrack> {
        self.audio.iter().find(|a| a.id == id)
    }

    pub fn audio_track_mut(&mut self, id: AudioId) -> Option<&mut AudioTrack> {
        self.audio.iter_mut().find(|a| a.id == id)
    }

    pub fn remove_audio(&mut self, id: AudioId) -> Option<AudioTrack> {
        let index = self.audio.iter().position(|a| a.id == id)?;
        Some(self.audio.remove(index))
    }
}

fn unity() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_inside_the_project_are_stored_relative() {
        let mut doc = Document::default();
        let dir = Path::new("/projects/walk");
        let inside = doc.add_audio(&dir.join("sound/voice.wav"), Some(dir), 0);
        let outside = doc.add_audio(Path::new("/music/song.mp3"), Some(dir), 12);

        assert_eq!(doc.audio_track(inside).unwrap().path, Path::new("sound/voice.wav"));
        assert_eq!(doc.audio_track(inside).unwrap().name, "voice");
        assert_eq!(doc.audio_track(outside).unwrap().path, Path::new("/music/song.mp3"));
        assert_eq!(doc.audio_track(outside).unwrap().start, 12);
    }

    #[test]
    fn relative_paths_resolve_against_the_project() {
        let mut doc = Document::default();
        let dir = Path::new("/projects/walk");
        let id = doc.add_audio(&dir.join("voice.wav"), Some(dir), 0);
        assert_eq!(
            doc.audio_track(id).unwrap().resolve(Some(Path::new("/elsewhere"))),
            Path::new("/elsewhere/voice.wav")
        );
    }

    #[test]
    fn audio_round_trips_through_json() {
        let mut doc = Document::default();
        let id = doc.add_audio(Path::new("/music/song.mp3"), None, 3);
        doc.audio_track_mut(id).unwrap().volume = 0.5;
        let loaded = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(loaded.audio, doc.audio);
        // New ids keep counting up after a reload.
        let mut loaded = loaded;
        assert_ne!(loaded.add_audio(Path::new("/a.wav"), None, 0), id);
    }
}
