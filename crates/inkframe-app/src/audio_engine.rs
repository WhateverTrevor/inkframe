//! Playing the document's audio in the editor.
//!
//! Sound files are decoded on background threads (an MP3 can take a moment)
//! and cached. When anything that affects the sound changes, the tracks are
//! re-mixed into one timeline-length clip, which is then played from
//! whatever frame playback starts on.
//!
//! While sound is playing, the *audio* clock decides which frame is shown
//! (see [`AudioEngine::sync`]). The sound card plays at a precise
//! rate, while UI frames arrive whenever the OS schedules them, so following
//! the sound is what keeps lip-sync locked over a long shot.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use inkframe_audio::{CHANNELS, Clip, MixTrack, SAMPLE_RATE};
use inkframe_core::{AudioTrack, Document, TimelineId};

use crate::editor::Editor;

/// A decoded file, or why it couldn't be decoded.
enum Decoded {
    Loading,
    Ready(Arc<Clip>),
    Failed(String),
}

/// A file's path and what decoding it produced, sent back from a decoder thread.
type DecodeResult = (PathBuf, Result<Clip, String>);

/// Everything the mix depends on. If none of it changed, the last mix is reused.
#[derive(PartialEq)]
struct MixKey {
    tracks: Vec<(PathBuf, u32, f32)>,
    fps: f64,
    duration: u32,
    /// Bumped when a file finishes decoding, since that changes the mix too.
    ready: usize,
}

pub struct AudioEngine {
    /// `None` if there's no output device; everything else still works.
    output: Option<rodio::OutputStream>,
    output_error: Option<String>,
    sink: Option<rodio::Sink>,
    clips: HashMap<PathBuf, Decoded>,
    /// Background decoders report back on this channel.
    finished: (Sender<DecodeResult>, Receiver<DecodeResult>),
    mix: Option<(MixKey, Arc<Clip>)>,
    peaks: HashMap<(PathBuf, u64), Arc<Vec<f32>>>,
    /// Where the current playback started, and whether it's a real
    /// playback (as opposed to a short scrub snippet).
    started_at: Option<u32>,
    last_frame: Option<u32>,
    /// The file playing on its own for the Library's preview, if any.
    previewing: Option<PathBuf>,
    /// Play a frame's worth of sound when the playhead moves while paused,
    /// which is how animators find the exact frame for each mouth shape.
    pub scrub: bool,
}

impl Default for AudioEngine {
    /// An engine playing through the default output device.
    fn default() -> Self {
        match rodio::OutputStreamBuilder::open_default_stream() {
            Ok(mut stream) => {
                // Dropping the stream at exit is expected; don't log it.
                stream.log_on_drop(false);
                Self { output: Some(stream), ..Self::without_output() }
            }
            Err(e) => Self { output_error: Some(e.to_string()), ..Self::without_output() },
        }
    }
}

impl AudioEngine {
    /// An engine that does everything except make sound. Used by tests, so
    /// they don't depend on (or tie up) the machine's audio device.
    pub fn without_output() -> Self {
        Self {
            output: None,
            output_error: None,
            sink: None,
            clips: HashMap::new(),
            finished: mpsc::channel(),
            mix: None,
            peaks: HashMap::new(),
            started_at: None,
            last_frame: None,
            previewing: None,
            scrub: true,
        }
    }

    /// Why sound can't play, if it can't.
    pub fn output_error(&self) -> Option<&str> {
        self.output_error.as_deref()
    }

    /// The state of a track's file, for the UI: `None` while it's loading
    /// or fine, or the error message if it failed.
    pub fn problem(&self, track: &AudioTrack, project_dir: Option<&Path>) -> Option<&str> {
        match self.clips.get(&track.resolve(project_dir)) {
            Some(Decoded::Failed(message)) => Some(message),
            _ => None,
        }
    }

    /// Whether any file is still being decoded.
    pub fn is_busy(&self) -> bool {
        self.clips.values().any(|c| matches!(c, Decoded::Loading))
    }

    pub fn is_loading(&self, track: &AudioTrack, project_dir: Option<&Path>) -> bool {
        matches!(self.clips.get(&track.resolve(project_dir)), Some(Decoded::Loading))
    }

    /// The loudest sample in each animation frame of a track's file, for
    /// drawing its waveform. `None` until the file has been decoded.
    pub fn peaks(
        &mut self,
        track: &AudioTrack,
        project_dir: Option<&Path>,
        fps: f64,
    ) -> Option<Arc<Vec<f32>>> {
        let path = track.resolve(project_dir);
        let Some(Decoded::Ready(clip)) = self.clips.get(&path) else { return None };
        let key = (path, fps.to_bits());
        Some(self.peaks.entry(key).or_insert_with(|| Arc::new(clip.peaks_per_frame(fps))).clone())
    }

    /// Plays one sound's file on its own from the start, at its volume, for
    /// the Library's preview. Returns false if the file isn't decoded yet
    /// (or couldn't be). Timeline playback or scrubbing replaces it.
    pub fn preview(&mut self, track: &AudioTrack, project_dir: Option<&Path>) -> bool {
        let path = track.resolve(project_dir);
        let Some(Decoded::Ready(clip)) = self.clips.get(&path) else { return false };
        let samples: Vec<f32> = clip.samples.iter().map(|s| s * track.volume).collect();
        self.started_at = None;
        self.play(&samples);
        self.previewing = Some(path);
        true
    }

    pub fn stop_preview(&mut self) {
        if self.previewing.is_some() {
            self.stop();
        }
    }

    /// How far into a sound's file the preview has played, in seconds, if
    /// it's the one playing.
    pub fn preview_position(&self, track: &AudioTrack, project_dir: Option<&Path>) -> Option<f64> {
        if self.previewing.as_ref() != Some(&track.resolve(project_dir)) {
            return None;
        }
        let sink = self.sink.as_ref().filter(|s| !s.empty())?;
        Some(sink.get_pos().as_secs_f64())
    }

    /// A sound file's length in seconds, once decoded.
    pub fn duration(&self, track: &AudioTrack, project_dir: Option<&Path>) -> Option<f64> {
        let Some(Decoded::Ready(clip)) = self.clips.get(&track.resolve(project_dir)) else { return None };
        Some(clip.samples.len() as f64 / (CHANNELS as f64 * SAMPLE_RATE as f64))
    }

    /// Advances playback and keeps sound and picture together. Call once
    /// per UI frame, after the panels have handled input, in place of
    /// `Editor::advance_playback`.
    ///
    /// The order matters:
    /// 1. If the playhead moved since last time, the user moved it (a click,
    ///    a key, a scrub). That's a jump to honour, not something to override.
    /// 2. The editor's own clock advances, which also handles looping and
    ///    stopping at the end.
    /// 3. While sound is playing, the frame it has reached replaces the
    ///    clock's estimate, so the picture follows the sound.
    pub fn sync(&mut self, ctx: &egui::Context, editor: &mut Editor, project_dir: Option<&Path>, dt: f64) {
        self.collect_decoded();
        self.start_decoding(ctx, &editor.doc, project_dir);

        // Sound belongs to the main timeline; inside a symbol, frames count
        // the symbol's time, which has nothing to do with the soundtrack.
        if editor.timeline() != TimelineId::Main {
            // A Library preview isn't the soundtrack, so it can play on.
            if self.previewing.is_none() {
                self.stop();
            }
            self.started_at = None;
            self.last_frame = None;
            editor.advance_playback(dt);
            return;
        }

        let user_moved = self.last_frame.is_some_and(|last| last != editor.frame);
        let before = editor.frame;
        editor.advance_playback(dt);
        let looped = editor.frame < before;
        let fps = editor.doc.frame_rate.fps();
        let mix = self.current_mix(&editor.doc, project_dir);

        if editor.playback.playing {
            if self.started_at.is_none() || user_moved || looped {
                match &mix {
                    Some(mix) => self.play(mix.from_animation_frame(editor.frame, fps)),
                    None => self.stop(),
                }
                self.started_at = Some(editor.frame);
            } else if let Some(frame) = self.playing_frame(fps) {
                editor.frame = frame.min(editor.last_frame());
            }
        } else if self.started_at.take().is_some() {
            self.stop();
        } else if user_moved
            && self.scrub
            && let Some(mix) = &mix
        {
            // About a frame and a half of sound: a single frame is too short
            // to make out at 24 fps.
            let samples = mix.from_animation_frame(editor.frame, fps);
            let length = ((1.5 / fps) * SAMPLE_RATE as f64) as usize * CHANNELS as usize;
            self.play(&samples[..length.min(samples.len())]);
        }
        self.last_frame = Some(editor.frame);
    }

    /// While sound is playing, the frame it has reached.
    fn playing_frame(&self, fps: f64) -> Option<u32> {
        let start = self.started_at?;
        let sink = self.sink.as_ref().filter(|s| !s.empty())?;
        Some(start + (sink.get_pos().as_secs_f64() * fps) as u32)
    }

    fn play(&mut self, samples: &[f32]) {
        self.stop();
        let Some(output) = &self.output else { return };
        if samples.is_empty() {
            return;
        }
        let sink = rodio::Sink::connect_new(output.mixer());
        sink.append(rodio::buffer::SamplesBuffer::new(CHANNELS, SAMPLE_RATE, samples.to_vec()));
        self.sink = Some(sink);
    }

    fn stop(&mut self) {
        self.previewing = None;
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
    }

    /// Kicks off decoding for any file we haven't seen yet.
    fn start_decoding(&mut self, ctx: &egui::Context, doc: &Document, project_dir: Option<&Path>) {
        for track in &doc.audio {
            let path = track.resolve(project_dir);
            if self.clips.contains_key(&path) {
                continue;
            }
            self.clips.insert(path.clone(), Decoded::Loading);
            let sender = self.finished.0.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = inkframe_audio::decode(&path).map_err(|e| e.to_string());
                let _ = sender.send((path, result));
                ctx.request_repaint();
            });
        }
    }

    fn collect_decoded(&mut self) {
        while let Ok((path, result)) = self.finished.1.try_recv() {
            let state = match result {
                Ok(clip) => Decoded::Ready(Arc::new(clip)),
                Err(message) => Decoded::Failed(message),
            };
            self.clips.insert(path, state);
        }
    }

    /// The whole soundtrack mixed, re-mixing only when something changed.
    fn current_mix(&mut self, doc: &Document, project_dir: Option<&Path>) -> Option<Arc<Clip>> {
        let audible: Vec<&AudioTrack> = doc.audio.iter().filter(|t| !t.muted).collect();
        let key = MixKey {
            tracks: audible.iter().map(|t| (t.resolve(project_dir), t.start, t.volume)).collect(),
            fps: doc.frame_rate.fps(),
            duration: doc.duration,
            ready: self.clips.values().filter(|c| matches!(c, Decoded::Ready(_))).count(),
        };
        if let Some((cached, mix)) = &self.mix
            && *cached == key
        {
            return Some(mix.clone());
        }

        let tracks: Vec<MixTrack> = audible
            .iter()
            .filter_map(|t| match self.clips.get(&t.resolve(project_dir)) {
                Some(Decoded::Ready(clip)) => Some(MixTrack { clip, start_frame: t.start, volume: t.volume }),
                _ => None,
            })
            .collect();
        if tracks.is_empty() {
            self.mix = None;
            return None;
        }
        let mix = Arc::new(inkframe_audio::mix(&tracks, key.fps, key.duration));
        self.mix = Some((key, mix.clone()));
        Some(mix)
    }
}
