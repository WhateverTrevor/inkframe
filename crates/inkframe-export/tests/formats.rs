//! End-to-end checks that each export format produces a real, playable file.
//!
//! Video tests need ffmpeg (and ffprobe next to it). They print a notice and
//! pass without checking anything when it's missing, so `cargo test` still
//! works on a fresh machine; CI installs ffmpeg so they run for real there.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::Command;

use inkframe_core::{Color, Document, Element, Point, Shape, Style, Transform};
use inkframe_export::{ExportError, ExportFormat, ExportSettings, export, ffmpeg};

/// A tiny 3-frame animation: a half-transparent square sliding right.
fn sample_doc() -> Document {
    let mut doc = Document::default();
    doc.width = 64;
    doc.height = 36;
    doc.duration = 3;
    let square = Element {
        shape: Shape::Rect { width: 16.0, height: 16.0, corner_radius: 4.0 },
        style: Style { fill: Some(Color::rgba(255, 0, 0, 128)), stroke: None },
    };
    let id = doc.add_layer("Square", vec![square], 0, Transform::at(Point::new(10.0, 18.0)));
    doc.layer_mut(id).unwrap().track.set(2, Transform::at(Point::new(54.0, 18.0)));
    doc
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inkframe-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn settings(format: ExportFormat, output: PathBuf, transparent: bool) -> ExportSettings {
    ExportSettings { format, output, frames: 0..=2, scale: 1.0, transparent }
}

/// Asks ffprobe for one stream property, e.g. `pix_fmt`.
fn probe(file: &Path, entry: &str) -> String {
    let ffprobe =
        ffmpeg::locate().unwrap().with_file_name(format!("ffprobe{}", std::env::consts::EXE_SUFFIX));
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
        .arg(format!("stream={entry}"))
        .args(["-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn ffmpeg_available() -> bool {
    let found = ffmpeg::locate().is_some();
    if !found {
        eprintln!("ffmpeg not found; skipping video export test");
    }
    found
}

#[test]
fn png_sequence_writes_numbered_frames() {
    let dir = scratch_dir("png");
    let settings = settings(ExportFormat::PngSequence, dir.join("shot.png"), true);
    export(&sample_doc(), &settings, |_| ControlFlow::Continue(())).unwrap();

    for n in 1..=3 {
        assert!(dir.join(format!("shot_{n:04}.png")).is_file(), "frame {n} missing");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancelling_removes_partial_output() {
    let dir = scratch_dir("cancel");
    let settings = settings(ExportFormat::PngSequence, dir.join("shot.png"), true);
    let result = export(&sample_doc(), &settings, |p| {
        if p.frames_done == 2 { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
    });

    assert!(matches!(result, Err(ExportError::Cancelled)));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prores_4444_keeps_alpha() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir("prores");
    let out = dir.join("out.mov");
    export(&sample_doc(), &settings(ExportFormat::ProRes4444, out.clone(), true), |_| {
        ControlFlow::Continue(())
    })
    .unwrap();

    assert_eq!(probe(&out, "codec_name"), "prores");
    assert!(probe(&out, "pix_fmt").starts_with("yuva444"), "no alpha plane");
    assert_eq!(probe(&out, "nb_frames"), "3");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prores_422_pads_odd_widths() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir("prores422");
    let out = dir.join("out.mov");
    let mut doc = sample_doc();
    doc.width = 63;
    export(&doc, &settings(ExportFormat::ProRes422Hq, out.clone(), false), |_| ControlFlow::Continue(()))
        .unwrap();

    assert_eq!(probe(&out, "width"), "64");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn h264_is_yuv420_mp4() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir("h264");
    let out = dir.join("out.mp4");
    export(&sample_doc(), &settings(ExportFormat::H264, out.clone(), true), |_| ControlFlow::Continue(()))
        .unwrap();

    assert_eq!(probe(&out, "codec_name"), "h264");
    assert_eq!(probe(&out, "pix_fmt"), "yuv420p");
    assert_eq!(probe(&out, "color_space"), "bt709");
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn hevc_with_alpha_encodes() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir("hevc");
    let out = dir.join("out.mov");
    export(&sample_doc(), &settings(ExportFormat::HevcWithAlpha, out.clone(), true), |_| {
        ControlFlow::Continue(())
    })
    .unwrap();

    assert_eq!(probe(&out, "codec_name"), "hevc");
    std::fs::remove_dir_all(dir).unwrap();
}
