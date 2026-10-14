//! Times how long the renderer takes on scenes of different sizes, at the
//! resolution the editor's stage uses on a Retina display.
//!
//! ```sh
//! cargo run --release -p inkframe-render --example stage_benchmark
//! ```
//!
//! Each scene is a grid of layers, each holding a filled circle and a wavy
//! hand-drawn-style stroke, with every layer animated so no frame can reuse
//! another. "Translucent" scenes give every layer 60% opacity, which is the
//! expensive case: a translucent layer has to be drawn on its own and then
//! blended down as a unit.

use std::time::Instant;

use inkframe_core::{Color, Document, Element, Path, Point, Segment, Shape, Stroke, Style, Transform};
use inkframe_render::{RenderOptions, output_size, render_frame_into};

/// A 1080p stage shown about 1.5× larger on a Retina screen: 2880×1620.
const SCALE: f32 = 1.5;
const FRAMES: u32 = 24;

fn main() {
    println!("{:>8} {:>12} {:>14} {:>12}", "layers", "opacity", "ms per frame", "fps");
    for layers in [10, 100, 500, 2000] {
        for opacity in [1.0, 0.6] {
            let doc = scene(layers, opacity);
            let ms = time_frames(&doc);
            let label = if opacity < 1.0 { "translucent" } else { "opaque" };
            println!("{layers:>8} {label:>12} {ms:>14.1} {:>12.0}", 1000.0 / ms);
        }
    }
}

/// Average milliseconds to render one frame, as the stage does.
fn time_frames(doc: &Document) -> f64 {
    let (width, height) = output_size(doc, SCALE);
    let options = RenderOptions { scale: SCALE, ..RenderOptions::default() };
    let start = Instant::now();
    for frame in 0..FRAMES {
        let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
        render_frame_into(&mut pixmap, doc, frame, &options);
        std::hint::black_box(&pixmap);
    }
    start.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64
}

fn scene(layer_count: usize, opacity: f32) -> Document {
    let mut doc = Document::default();
    doc.duration = FRAMES;
    let columns = (layer_count as f32).sqrt().ceil() as usize;
    let cell = doc.width as f32 / columns as f32;
    for i in 0..layer_count {
        let (col, row) = (i % columns, i / columns);
        let at = Point::new((col as f32 + 0.5) * cell, (row as f32 + 0.5) * cell * 0.56);
        let circle = Element {
            shape: Shape::Ellipse { radius_x: cell * 0.3, radius_y: cell * 0.3 },
            style: Style {
                fill: Some(Color::rgb((i * 37 % 255) as u8, 120, 200)),
                stroke: Some(Stroke { color: Color::BLACK, width: 2.0 }),
            },
        };
        let squiggle = Element {
            shape: Shape::Path(wavy_line(cell)),
            style: Style { fill: None, stroke: Some(Stroke { color: Color::rgb(30, 30, 30), width: 3.0 }) },
        };
        let mut pose = Transform::at(at);
        pose.opacity = opacity;
        let id = doc.add_layer(format!("Layer {i}"), vec![circle, squiggle], 0, pose);
        // Drift every layer over the shot so each frame is different.
        let mut later = pose;
        later.position = at + Point::new(cell * 0.2, 0.0);
        later.rotation = 0.5;
        doc.layer_mut(id).unwrap().track.set_pose(FRAMES - 1, later);
    }
    doc
}

/// A pencil-like stroke: 40 curve segments wobbling across the cell.
fn wavy_line(cell: f32) -> Path {
    let segments = (1..=40)
        .map(|i| {
            let x = -cell * 0.4 + cell * 0.8 * i as f32 / 40.0;
            let y = (i as f32 * 0.7).sin() * cell * 0.1;
            Segment::Cubic {
                ctrl1: Point::new(x - cell * 0.01, y - cell * 0.03),
                ctrl2: Point::new(x - cell * 0.005, y + cell * 0.03),
                to: Point::new(x, y),
            }
        })
        .collect();
    Path { start: Point::new(-cell * 0.4, 0.0), segments, closed: false }
}
