//! End-to-end checks that each export format produces a real, playable file.
//!
//! Video tests need ffmpeg (and ffprobe next to it). They print a notice and
//! pass without checking anything when it's missing, so `cargo test` still
//! works on a fresh machine. CI installs ffmpeg and sets
//! `INKFRAME_REQUIRE_FFMPEG`, which turns a missing ffmpeg into a failure.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::Command;

use inkframe_audio::{Clip, SAMPLE_RATE};
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
        style: Style { fill: Some(Color::rgba(255, 0, 0, 128)), stroke: None, gradient: None },
    };
    let id = doc.add_layer("Square", vec![square], 0, Transform::at(Point::new(10.0, 18.0)));
    doc.layer_mut(id).unwrap().track.set_pose(2, Transform::at(Point::new(54.0, 18.0)));
    doc
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inkframe-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn settings(format: ExportFormat, output: PathBuf, transparent: bool) -> ExportSettings {
    ExportSettings { format, output, frames: 0..=2, scale: 1.0, transparent, project_dir: None }
}

/// Asks ffprobe for one stream property, e.g. `pix_fmt`.
fn probe(file: &Path, entry: &str) -> String {
    probe_stream(file, "v:0", entry)
}

/// Asks ffprobe for one property of a given stream, e.g. `a:0` for audio.
fn probe_stream(file: &Path, stream: &str, entry: &str) -> String {
    // ffprobe usually sits next to ffmpeg. A bundled ffmpeg (tested with
    // `INKFRAME_FFMPEG=Inkframe.app/Contents/MacOS/ffmpeg`) comes alone, so
    // fall back to whichever ffprobe is on PATH.
    let beside = ffmpeg::locate().unwrap().with_file_name(format!("ffprobe{}", std::env::consts::EXE_SUFFIX));
    let ffprobe = if beside.is_file() { beside } else { PathBuf::from("ffprobe") };
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-select_streams", stream, "-show_entries"])
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
        // CI sets this, so a failed ffmpeg install can't pass as a skip.
        assert!(std::env::var_os("INKFRAME_REQUIRE_FFMPEG").is_none(), "ffmpeg is required but wasn't found");
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

/// The sample animation plus a one-second tone as its soundtrack.
fn doc_with_audio(dir: &Path) -> Document {
    let tone = Clip {
        samples: (0..SAMPLE_RATE as usize)
            .flat_map(|i| {
                let s = (i as f32 / SAMPLE_RATE as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [s, s]
            })
            .collect(),
    };
    tone.write_wav(&dir.join("tone.wav")).unwrap();
    let mut doc = sample_doc();
    doc.add_audio(&dir.join("tone.wav"), Some(dir), 0);
    doc
}

#[test]
fn mp4_gets_aac_audio() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir("h264-audio");
    let out = dir.join("out.mp4");
    let doc = doc_with_audio(&dir);
    let mut settings = settings(ExportFormat::H264, out.clone(), false);
    settings.project_dir = Some(dir.clone());
    export(&doc, &settings, |_| ControlFlow::Continue(())).unwrap();

    assert_eq!(probe_stream(&out, "a:0", "codec_name"), "aac");
    assert_eq!(probe_stream(&out, "a:0", "sample_rate"), "48000");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prores_gets_uncompressed_audio() {
    if !ffmpeg_available() {
        return;
    }
    let dir = scratch_dir("prores-audio");
    let out = dir.join("out.mov");
    let doc = doc_with_audio(&dir);
    let mut settings = settings(ExportFormat::ProRes4444, out.clone(), true);
    settings.project_dir = Some(dir.clone());
    export(&doc, &settings, |_| ControlFlow::Continue(())).unwrap();

    assert_eq!(probe_stream(&out, "a:0", "codec_name"), "pcm_s24le");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn png_sequences_get_a_wav_alongside() {
    let dir = scratch_dir("png-audio");
    let doc = doc_with_audio(&dir);
    let mut settings = settings(ExportFormat::PngSequence, dir.join("shot.png"), true);
    settings.project_dir = Some(dir.clone());
    export(&doc, &settings, |_| ControlFlow::Continue(())).unwrap();

    // Three frames at 24 fps is an eighth of a second of sound.
    let wav = inkframe_audio::decode(&dir.join("shot.wav")).unwrap();
    assert_eq!(wav.frames(), SAMPLE_RATE as usize / 8);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_audio_fails_the_export_clearly() {
    let dir = scratch_dir("missing-audio");
    let mut doc = sample_doc();
    doc.add_audio(&dir.join("not-here.wav"), Some(&dir), 0);
    let mut settings = settings(ExportFormat::PngSequence, dir.join("shot.png"), true);
    settings.project_dir = Some(dir.clone());

    let result = export(&doc, &settings, |_| ControlFlow::Continue(()));
    assert!(matches!(result, Err(ExportError::Audio(ref message)) if message.contains("not-here.wav")));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_images_fail_the_export_clearly() {
    let dir = scratch_dir("missing-image");
    let mut doc = sample_doc();
    let image = doc.add_image(&dir.join("backdrop.png"), Some(&dir), (64, 36));
    let backdrop = Element {
        shape: Shape::Image { image, width: 64.0, height: 36.0 },
        style: Style { fill: None, stroke: None, gradient: None },
    };
    doc.add_layer("Backdrop", vec![backdrop], 0, Transform::at(Point::new(32.0, 18.0)));
    let mut settings = settings(ExportFormat::PngSequence, dir.join("shot.png"), true);
    settings.project_dir = Some(dir.clone());

    let result = export(&doc, &settings, |_| ControlFlow::Continue(()));
    assert!(
        matches!(result, Err(ExportError::Image(ref message)) if message.contains("backdrop")),
        "{result:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_fonts_fail_the_export_clearly() {
    let dir = scratch_dir("missing-font");
    let mut doc = sample_doc();
    doc.add_font_file(&dir.join("Brand.otf"), Some(&dir), vec!["Brand".into()]);
    let mut text = inkframe_core::Text::new("Logo", 20.0);
    text.font.family = "Brand".into();
    let style = Style { fill: Some(Color::BLACK), stroke: None, gradient: None };
    doc.add_layer(
        "Logo",
        vec![Element { shape: Shape::Text(text), style }],
        0,
        Transform::at(Point::new(4.0, 20.0)),
    );
    let mut settings = settings(ExportFormat::PngSequence, dir.join("shot.png"), true);
    settings.project_dir = Some(dir.clone());

    let result = export(&doc, &settings, |_| ControlFlow::Continue(()));
    assert!(
        matches!(result, Err(ExportError::Font(ref message)) if message.contains("Brand.otf")),
        "{result:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sprite_sheets_pack_frames_with_a_data_file() {
    let dir = scratch_dir("sheet");
    let mut doc = sample_doc();
    // Frame 4 holds frame 3's drawing, so it should share its cell.
    doc.duration = 4;
    let mut settings = settings(ExportFormat::SpriteSheet, dir.join("walk.png"), true);
    settings.frames = 0..=3;
    export(&doc, &settings, |_| ControlFlow::Continue(())).unwrap();

    let sheet = tiny_skia::Pixmap::load_png(dir.join("walk.png")).unwrap();
    let data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("walk.json")).unwrap()).unwrap();
    assert_eq!(data["meta"]["size"]["w"], sheet.width());
    assert_eq!(data["meta"]["size"]["h"], sheet.height());
    let names = data["animations"]["walk"].as_array().unwrap();
    assert_eq!(names.len(), 4);
    let cell = |n: usize| data["frames"][names[n].as_str().unwrap()]["frame"].clone();
    assert_ne!(cell(0), cell(1));
    assert_eq!(cell(2), cell(3));
    // Cropped to where the square goes: from its left edge on frame 1 to
    // its right edge on frame 3, and its height. Three distinct frames,
    // wide enough to stack in one column, 2 px apart.
    let used = &data["frames"][names[0].as_str().unwrap()]["spriteSourceSize"];
    let (w, h) = (used["w"].as_u64().unwrap() as u32, used["h"].as_u64().unwrap() as u32);
    assert!(w < 64 && h < 36, "{used}");
    assert_eq!((sheet.width(), sheet.height()), (w, 3 * h + 4));
    assert_eq!(data["frames"][names[0].as_str().unwrap()]["sourceSize"]["w"], 64);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_batch_keeps_going_past_a_failed_job() {
    use inkframe_export::batch::{self, BatchEvent, BatchJob};

    let dir = scratch_dir("batch");
    let project = dir.join("square.inkf");
    sample_doc().save(&project).unwrap();
    let jobs = [
        BatchJob::new(dir.join("missing.inkf"), ExportFormat::PngSequence),
        BatchJob::new(&project, ExportFormat::PngSequence),
        BatchJob::new(&project, ExportFormat::SpriteSheet),
    ];
    let out = dir.join("renders");
    let mut finished = Vec::new();
    let succeeded = batch::run(&jobs, &out, |event| {
        if let BatchEvent::Finished(i, result) = event {
            finished.push((i, result.is_ok()));
        }
        ControlFlow::Continue(())
    });

    assert_eq!(succeeded, 2);
    assert_eq!(finished, [(0, false), (1, true), (2, true)]);
    // The sequence and the sheet would both be `square.png`, so each
    // is named for its format.
    assert!(out.join("square-png/square-png_0001.png").is_file());
    assert!(out.join("square-sheet.png").is_file() && out.join("square-sheet.json").is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn stopping_a_batch_skips_the_rest() {
    use inkframe_export::batch::{self, BatchEvent, BatchJob};

    let dir = scratch_dir("batch-stop");
    let project = dir.join("square.inkf");
    sample_doc().save(&project).unwrap();
    let jobs = [
        BatchJob::new(&project, ExportFormat::PngSequence),
        BatchJob::new(&project, ExportFormat::SpriteSheet),
    ];
    let mut started = 0;
    let succeeded = batch::run(&jobs, &dir.join("renders"), |event| match event {
        BatchEvent::Started(_) => {
            started += 1;
            ControlFlow::Continue(())
        }
        BatchEvent::Progress(_, p) if p.frames_done == 2 => ControlFlow::Break(()),
        _ => ControlFlow::Continue(()),
    });
    assert_eq!((succeeded, started), (0, 1));
    std::fs::remove_dir_all(dir).unwrap();
}
