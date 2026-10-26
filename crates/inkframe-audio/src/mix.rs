//! Mixing audio tracks into one clip aligned with the timeline.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use inkframe_core::Document;

use crate::{AudioError, CHANNELS, Clip, decode_first, sample_frame};

/// One clip's place in a mix.
pub struct MixTrack<'a> {
    pub clip: &'a Clip,
    /// The animation frame the clip starts on.
    pub start_frame: u32,
    pub volume: f32,
}

/// Mixes `tracks` into a single clip covering `duration_frames` of
/// animation at `fps`. Sound before frame 0 or past the end is cut off.
///
/// Samples are simply added, so loud tracks played together can exceed
/// full scale. That's kept as-is (floats don't clip), and it's left to the
/// final encoder, which is the same thing every editing application does.
pub fn mix(tracks: &[MixTrack], fps: f64, duration_frames: u32) -> Clip {
    let mut out = Clip::silence(sample_frame(duration_frames, fps));
    for track in tracks {
        let offset = sample_frame(track.start_frame, fps) * CHANNELS as usize;
        let Some(room) = out.samples.len().checked_sub(offset) else { continue };
        for (dst, src) in out.samples[offset..].iter_mut().zip(&track.clip.samples).take(room) {
            *dst += src * track.volume;
        }
    }
    out
}

/// How many seconds of the file at `path` the animation can play: from
/// each track's start to the end of the animation, the longest of the
/// tracks that use it (none if muted or starting after the end).
pub fn audible_seconds(doc: &Document, path: &Path, project_dir: Option<&Path>) -> f64 {
    let fps = doc.frame_rate.fps();
    doc.audio
        .iter()
        .filter(|t| !t.muted && t.resolve(project_dir) == path)
        .map(|t| doc.duration.saturating_sub(t.start) as f64 / fps)
        .fold(0.0, f64::max)
}

/// Decodes and mixes every unmuted audio track in `doc`.
///
/// `project_dir` resolves relative paths (see `AudioTrack::resolve`).
/// Returns `None` when there's nothing to hear. Files that can't be read are
/// reported in the error list but don't stop the rest from being mixed:
/// one missing sound effect shouldn't block an export.
pub fn mix_document(
    doc: &Document,
    project_dir: Option<&Path>,
) -> (Option<Clip>, Vec<(PathBuf, AudioError)>) {
    let mut clips: HashMap<PathBuf, Clip> = HashMap::new();
    let mut errors = Vec::new();
    for track in doc.audio.iter().filter(|t| !t.muted) {
        let path = track.resolve(project_dir);
        if clips.contains_key(&path) || errors.iter().any(|(p, _)| *p == path) {
            continue;
        }
        // Only what the animation plays of each file: a long song under a
        // short shot needn't be decoded whole.
        match decode_first(&path, audible_seconds(doc, &path, project_dir)) {
            Ok(decoded) => {
                clips.insert(path, decoded.clip);
            }
            Err(e) => errors.push((path, e)),
        }
    }

    let tracks: Vec<MixTrack> = doc
        .audio
        .iter()
        .filter(|t| !t.muted)
        .filter_map(|t| {
            let clip = clips.get(&t.resolve(project_dir))?;
            Some(MixTrack { clip, start_frame: t.start, volume: t.volume })
        })
        .collect();
    if tracks.is_empty() {
        return (None, errors);
    }
    (Some(mix(&tracks, doc.frame_rate.fps(), doc.duration)), errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SAMPLE_RATE;
    use crate::test_util::{scratch, write_tone};

    fn constant(value: f32, frames: usize) -> Clip {
        Clip { samples: vec![value; frames * CHANNELS as usize] }
    }

    #[test]
    fn tracks_land_on_their_start_frames() {
        let fps = 24.0;
        let one_frame = SAMPLE_RATE as usize / 24;
        let a = constant(0.5, one_frame * 2);
        let b = constant(0.25, one_frame * 2);
        let tracks = [
            MixTrack { clip: &a, start_frame: 0, volume: 1.0 },
            MixTrack { clip: &b, start_frame: 1, volume: 2.0 },
        ];
        let out = mix(&tracks, fps, 4);
        let at = |frame: usize| out.samples[(frame * one_frame + one_frame / 2) * 2];

        assert_eq!(out.frames(), one_frame * 4);
        assert_eq!(at(0), 0.5);
        assert_eq!(at(1), 1.0); // 0.5 + 0.25 × 2
        assert_eq!(at(2), 0.5);
        assert_eq!(at(3), 0.0);
    }

    #[test]
    fn sound_past_the_end_is_cut_off() {
        let long = constant(1.0, SAMPLE_RATE as usize * 10);
        let out = mix(&[MixTrack { clip: &long, start_frame: 0, volume: 1.0 }], 24.0, 24);
        assert_eq!(out.frames(), SAMPLE_RATE as usize);
    }

    #[test]
    fn documents_mix_their_unmuted_tracks() {
        let dir = scratch("mix");
        write_tone(&dir.join("tone.wav"), 48_000, 2, 0.5, 0.5);

        let mut doc = Document::default();
        let id = doc.add_audio(&dir.join("tone.wav"), Some(&dir), 0);
        doc.add_audio(&dir.join("missing.wav"), Some(&dir), 0);

        let (clip, errors) = mix_document(&doc, Some(&dir));
        assert!(clip.is_some());
        assert_eq!(errors.len(), 1, "the missing file is reported");

        doc.audio_track_mut(id).unwrap().muted = true;
        doc.audio.retain(|t| t.id == id);
        assert!(mix_document(&doc, Some(&dir)).0.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
