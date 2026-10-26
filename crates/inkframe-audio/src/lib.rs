//! Audio for Inkframe: decoding sound files, mixing them against the
//! timeline, and writing the result out.
//!
//! Everything is converted on the way in to one internal format, a [`Clip`]:
//! interleaved stereo 32-bit float samples at [`SAMPLE_RATE`]. Mixing,
//! waveforms, playback and export then never need to care what the original
//! file was.

mod decode;
mod mix;
pub mod rhubarb;

use std::path::Path;

pub use decode::{Decoded, decode, decode_first};
pub use mix::{MixTrack, audible_seconds, mix, mix_document};

/// Every clip is resampled to this rate. 48 kHz is the standard for video,
/// so exports need no further conversion.
pub const SAMPLE_RATE: u32 = 48_000;
/// Every clip is stereo. Mono files are copied to both sides.
pub const CHANNELS: u16 = 2;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("couldn't read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't decode the audio: {0}")]
    Decode(#[from] symphonia::core::errors::Error),
    #[error("the file has no audio track")]
    NoAudioTrack,
    #[error("couldn't resample the audio: {0}")]
    Resample(String),
    #[error("couldn't write the WAV file: {0}")]
    Wav(#[from] hound::Error),
}

/// Decoded audio: interleaved stereo samples at [`SAMPLE_RATE`]
/// (left, right, left, right, …).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clip {
    pub samples: Vec<f32>,
}

impl Clip {
    /// Silence lasting `frames` sample frames (one sample per channel each).
    pub fn silence(frames: usize) -> Self {
        Self { samples: vec![0.0; frames * CHANNELS as usize] }
    }

    /// Length in sample frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / CHANNELS as usize
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / SAMPLE_RATE as f64
    }

    /// The loudest absolute sample in each animation frame, from the start
    /// of the clip: the data for drawing a waveform in the timeline.
    pub fn peaks_per_frame(&self, fps: f64) -> Vec<f32> {
        let per_frame = (SAMPLE_RATE as f64 / fps).max(1.0);
        let count = (self.frames() as f64 / per_frame).ceil() as usize;
        (0..count)
            .map(|i| {
                let start = (i as f64 * per_frame) as usize * CHANNELS as usize;
                let end = (((i + 1) as f64 * per_frame) as usize * CHANNELS as usize).min(self.samples.len());
                self.samples[start..end].iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
            })
            .collect()
    }

    /// The samples from `start_frame` (an animation frame) onward, for
    /// starting playback partway through.
    pub fn from_animation_frame(&self, start_frame: u32, fps: f64) -> &[f32] {
        let start = sample_frame(start_frame, fps).min(self.frames());
        &self.samples[start * CHANNELS as usize..]
    }

    /// Writes the clip as a 32-bit float WAV file, which keeps every bit of
    /// quality and which ffmpeg and every editor can read.
    pub fn write_wav(&self, path: &Path) -> Result<(), AudioError> {
        let spec = hound::WavSpec {
            channels: CHANNELS,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for &sample in &self.samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        Ok(())
    }
}

/// The sample frame where animation frame `frame` begins.
pub fn sample_frame(frame: u32, fps: f64) -> usize {
    (frame as f64 / fps * SAMPLE_RATE as f64).round() as usize
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::path::{Path, PathBuf};

    /// Writes a WAV file with a sine tone, for tests.
    pub fn write_tone(path: &Path, rate: u32, channels: u16, seconds: f32, amplitude: f32) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        let total = (rate as f32 * seconds) as usize;
        for i in 0..total {
            let value = (i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * amplitude;
            for _ in 0..channels {
                writer.write_sample((value * i16::MAX as f32) as i16).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkframe-audio-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_follow_loudness() {
        // Half a second of silence at 24 fps, then half a second of full scale.
        let mut clip = Clip::silence(SAMPLE_RATE as usize);
        let half = clip.samples.len() / 2;
        clip.samples[half..].fill(1.0);
        let peaks = clip.peaks_per_frame(24.0);
        assert_eq!(peaks.len(), 24);
        assert_eq!(peaks[0], 0.0);
        assert_eq!(peaks[23], 1.0);
    }

    #[test]
    fn wav_round_trip() {
        let dir = test_util::scratch("wav");
        let clip = Clip { samples: vec![0.25, -0.5, 0.75, -1.0] };
        let path = dir.join("out.wav");
        clip.write_wav(&path).unwrap();
        assert_eq!(decode(&path).unwrap(), clip);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
