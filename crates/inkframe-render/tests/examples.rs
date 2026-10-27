//! The sample projects in `examples/` must keep opening and drawing as
//! the file format and renderer change. (They're rebuilt with
//! `cargo xtask samples`.)

use std::path::Path;

use inkframe_core::Document;
use inkframe_render::{RenderOptions, render_frame};

fn example(name: &str) -> Document {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name);
    Document::load(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn every_example_opens_and_draws() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "inkf") {
            continue;
        }
        let doc = Document::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let options = RenderOptions { scale: 0.25, ..RenderOptions::default() };
        for frame in [0, doc.last_frame() / 2, doc.last_frame()] {
            assert!(render_frame(&doc, frame, &options).is_some(), "{} frame {frame}", path.display());
        }
        seen += 1;
    }
    assert!(seen >= 6, "found {seen} examples");
}

#[test]
fn the_flower_is_painted_under_its_lines() {
    let doc = example("ink-and-paint.inkf");
    let flower = doc.layers.iter().find(|l| l.name == "Flower").unwrap();
    // The middle, six petals and the leaf.
    assert_eq!(flower.drawings[0].colour_art.len(), 8);
    assert_eq!(doc.palette.swatches.len(), 3);
}

#[test]
fn the_landscape_is_painted_on_a_layer_per_part() {
    let doc = example("painted-landscape.inkf");
    let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect();
    for part in ["Sky", "Clouds", "Mountains", "Hills", "Trees", "Grass"] {
        let layer =
            doc.layers.iter().find(|l| l.name == part).unwrap_or_else(|| panic!("{part} in {names:?}"));
        let (canvas, ..) =
            doc.paint_canvas_at(layer.id, 0).unwrap_or_else(|| panic!("{part} is a paint layer"));
        assert!(!doc.canvas(canvas).unwrap().is_empty(), "{part} is painted");
    }
    // The clouds drift across the sky.
    let clouds = doc.layers.iter().find(|l| l.name == "Clouds").unwrap();
    let (start, end) = (clouds.transform_at(0), clouds.transform_at(doc.last_frame()));
    assert!(end.position.x > start.position.x + 50.0);
}
