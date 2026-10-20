//! Every example script in `examples/scripts` runs, on a copy of the rig
//! sample with its top three layers selected, as someone trying them would.

use std::path::{Path, PathBuf};

use inkframe_core::Document;
use inkframe_script::{Script, run};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Runs the example script `name` on a fresh copy of the rig.
fn try_script(name: &str) -> (Document, inkframe_script::Outcome) {
    // A folder of its own, since tests run side by side.
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run_number = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("inkframe-example-script-{}-{run_number}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("rig.inkf");
    std::fs::copy(examples().join("rig.inkf"), &project).unwrap();
    let doc = Document::load(&project).unwrap();
    let selection = doc.layers.iter().rev().take(3).map(|l| l.id).collect();
    let source = std::fs::read_to_string(examples().join("scripts").join(name)).unwrap();
    let script = Script {
        path: Some(project),
        selection,
        limit: Some(std::time::Duration::from_secs(30)),
        ..Script::new(source, doc.clone())
    };
    let outcome = run(script).unwrap_or_else(|e| panic!("{name}: {e}"));
    let _ = std::fs::remove_dir_all(&dir);
    (doc, outcome)
}

#[test]
fn every_example_script_runs() {
    let mut names: Vec<String> = std::fs::read_dir(examples().join("scripts"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".js"))
        .collect();
    names.sort();
    assert!(names.len() >= 7, "{names:?}");
    for name in names {
        // Making a video needs ffmpeg, which a fresh machine may not have.
        // (CI has it, and requires it for the export tests.)
        if name == "export-review.js" && inkframe_export::ffmpeg::locate().is_none() {
            eprintln!("ffmpeg not found; skipping {name}");
            continue;
        }
        let (_, outcome) = try_script(&name);
        assert!(!outcome.log.is_empty(), "{name} says what it did");
    }
}

#[test]
fn the_examples_do_what_they_say() {
    let (before, after) = try_script("bouncing-ball.js");
    assert_eq!(after.document.layers.len(), before.layers.len() + 2);

    let (_, after) = try_script("number-layers.js");
    let names: Vec<&str> = after.document.layers.iter().rev().map(|l| l.name.as_str()).collect();
    assert_eq!(&names[..2], ["Drawing 01", "Drawing 02"]);

    let (before, after) = try_script("stagger-selected.js");
    let first_key = |doc: &Document, i: usize| doc.layers[doc.layers.len() - 1 - i].track.key_frames()[0];
    assert_eq!(
        first_key(&after.document, 2),
        first_key(&before, 2) + 4,
        "third selected layer two steps later"
    );

    let (_, after) = try_script("snowy-night.js");
    assert!(after.document.layers.iter().any(|l| l.particles.is_some()));
    assert!(after.document.camera_layer().is_some());

    let (_, after) = try_script("confetti.js");
    assert_eq!(after.document.layers.iter().filter(|l| l.name.starts_with("Confetti")).count(), 40);
}
