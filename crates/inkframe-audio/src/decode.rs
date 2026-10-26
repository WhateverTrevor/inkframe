//! Reading audio files into [`Clip`]s.
//!
//! Decoding is done by symphonia, a pure-Rust library covering WAV, AIFF,
//! MP3, FLAC, AAC/M4A, Ogg Vorbis and more, so there are no system codecs
//! to install. Afterwards the audio is converted to stereo and resampled to
//! [`SAMPLE_RATE`].

use std::fs::File;
use std::path::Path;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, Resampler, SincInterpolationParameters};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{AudioError, CHANNELS, Clip, SAMPLE_RATE};

/// Decodes a whole audio file into memory.
///
/// A minute of audio is about 23 MB this way, which is fine for the
/// dialogue and music animators work with and keeps playback and scrubbing
/// simple: every sample is already at hand. For a long file under a short
/// animation, [`decode_first`] stops once it has what can be heard.
pub fn decode(path: &Path) -> Result<Clip, AudioError> {
    decode_first(path, f64::INFINITY).map(|decoded| decoded.clip)
}

/// The start of a file, decoded, and what's known of the rest.
pub struct Decoded {
    pub clip: Clip,
    /// The whole file's length in seconds, if it says.
    pub seconds: Option<f64>,
    /// Whether all of the file was decoded.
    pub complete: bool,
}

/// Decodes the first `seconds` of an audio file (a little more, to the end
/// of a packet), or all of it if it's shorter.
pub fn decode_first(path: &Path, seconds: f64) -> Result<Decoded, AudioError> {
    let source = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(AudioError::NoAudioTrack)?;
    let track_id = track.id;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();
    let mut rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);
    let mut channels = track.codec_params.channels.map_or(1, |c| c.count());
    let length = track.codec_params.n_frames.map(|frames| frames as f64 / rate as f64);
    let mut complete = true;

    loop {
        if samples.len() as f64 >= seconds * rate as f64 * channels as f64 {
            complete = false;
            break;
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // Running out of data is how symphonia signals the end of a file.
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                rate = spec.rate;
                channels = spec.channels.count();
                let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buffer.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buffer.samples());
            }
            // A damaged packet costs a few milliseconds of sound; skipping it
            // beats refusing to open the whole file.
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    let stereo = to_stereo(&samples, channels);
    drop(samples);
    let resampled = if rate == SAMPLE_RATE { stereo } else { resample(&stereo, rate)? };
    Ok(Decoded { clip: Clip { samples: resampled }, seconds: length, complete })
}

/// Converts interleaved audio with any channel count to stereo. Mono is
/// copied to both sides; for surround formats we keep the front left and
/// right, which carry nearly all dialogue and music.
fn to_stereo(samples: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        2 => samples.to_vec(),
        0 => Vec::new(),
        1 => samples.iter().flat_map(|&s| [s, s]).collect(),
        n => samples.chunks_exact(n).flat_map(|frame| [frame[0], frame[1]]).collect(),
    }
}

/// Converts stereo audio from `from_rate` to [`SAMPLE_RATE`].
///
/// Uses band-limited sinc interpolation, the high-quality kind used in
/// audio editors. Cheaper linear interpolation would add audible aliasing,
/// which matters because this is the audio that ends up in final exports.
fn resample(stereo: &[f32], from_rate: u32) -> Result<Vec<f32>, AudioError> {
    let error = |e: &dyn std::fmt::Display| AudioError::Resample(e.to_string());
    let channels = CHANNELS as usize;
    let frames = stereo.len() / channels;
    if frames == 0 {
        return Ok(Vec::new());
    }

    let ratio = SAMPLE_RATE as f64 / from_rate as f64;
    let mut resampler = Async::<f32>::new_sinc(
        ratio,
        1.1,
        &SincInterpolationParameters::default(),
        1024,
        channels,
        FixedAsync::Input,
    )
    .map_err(|e| error(&e))?;
    let input = InterleavedSlice::new(stereo, channels, frames).map_err(|e| error(&e))?;
    let output = resampler.process_all(&input, frames, None).map_err(|e| error(&e))?;
    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{scratch, write_tone};

    #[test]
    fn only_the_start_of_a_long_file_is_decoded_when_asked() {
        let path = scratch("first").join("ten-seconds.wav");
        write_tone(&path, SAMPLE_RATE, 2, 10.0, 0.5);
        let start = decode_first(&path, 2.0).unwrap();
        assert!(!start.complete);
        let seconds = start.clip.seconds();
        assert!((2.0..2.5).contains(&seconds), "about two seconds: {seconds}");
        assert_eq!(start.seconds, Some(10.0), "the whole file's length is still known");
        let all = decode_first(&path, 60.0).unwrap();
        assert!(all.complete && (all.clip.seconds() - 10.0).abs() < 0.01);
    }

    #[test]
    fn mono_44k_becomes_stereo_48k() {
        let dir = scratch("decode");
        let path = dir.join("tone.wav");
        write_tone(&path, 44_100, 1, 1.0, 0.5);

        let clip = decode(&path).unwrap();
        // One second in, one second out (give or take a few samples of
        // resampler edge handling).
        assert!((clip.frames() as i64 - SAMPLE_RATE as i64).abs() < 64, "{} frames", clip.frames());
        // Mono was copied to both channels.
        let mid = clip.frames() / 2 * 2;
        assert_eq!(clip.samples[mid], clip.samples[mid + 1]);
        // The tone's level survives resampling.
        let peak = clip.samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.05, "peak {peak}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_files_are_an_error() {
        assert!(matches!(decode(Path::new("/definitely/not/here.wav")), Err(AudioError::Io(_))));
    }

    #[test]
    fn surround_keeps_front_left_and_right() {
        let six = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(to_stereo(&six, 6), vec![1.0, 2.0]);
    }
}
