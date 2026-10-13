//! Decoding compressed formats, using files made on the spot with ffmpeg.
//! Skipped (with a notice) where ffmpeg isn't installed.

use std::path::{Path, PathBuf};
use std::process::Command;

use inkframe_audio::{SAMPLE_RATE, decode};

fn ffmpeg() -> Option<PathBuf> {
    ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .or_else(|| Command::new("ffmpeg").arg("-version").output().ok().map(|_| PathBuf::from("ffmpeg")))
}

/// Makes a 2-second 440 Hz test tone in the format implied by `path`'s extension.
fn make_tone(ffmpeg: &Path, path: &Path) {
    let status = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=2",
        ])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg couldn't make {}", path.display());
}

#[test]
fn decodes_compressed_formats() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("ffmpeg not found; skipping compressed-format decoding test");
        return;
    };
    let dir = std::env::temp_dir().join(format!("inkframe-audio-formats-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    for extension in ["mp3", "m4a", "flac"] {
        let path = dir.join(format!("tone.{extension}"));
        make_tone(&ffmpeg, &path);
        let clip = decode(&path).unwrap_or_else(|e| panic!("{extension}: {e}"));
        // Lossy codecs add a little padding at the start and end.
        let seconds = clip.frames() as f64 / SAMPLE_RATE as f64;
        assert!((seconds - 2.0).abs() < 0.1, "{extension}: {seconds}s");
        let peak = clip.samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(peak > 0.05, "{extension}: silent");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
