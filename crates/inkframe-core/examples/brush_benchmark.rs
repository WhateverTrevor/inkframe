//! How long the brush engine takes with big brushes: each ready-made brush
//! at a few sizes, painting a stroke across a 4K canvas the way a pen
//! moves (a point every few pixels, each one a frame's worth of work).
//!
//! Run it in release mode:
//!
//! ```sh
//! cargo run --release -p inkframe-core --example brush_benchmark
//! ```
//!
//! It prints, for each brush and size, the slowest and the average time to
//! carry a stroke on by one pen sample. Over about 16 ms, painting can't
//! keep up with a 60 Hz screen.

use std::time::Instant;

use inkframe_core::{BrushMode, BrushSettings, BrushStroke, Canvas, Color, Point};

fn main() {
    let only = std::env::args().nth(1);
    println!("{:<16} {:>6} {:>10} {:>10}", "brush", "size", "average", "slowest");
    for preset in BrushSettings::presets() {
        if only.as_ref().is_some_and(|name| !preset.name.to_lowercase().contains(&name.to_lowercase())) {
            continue;
        }
        for size in [100.0, 500.0, 1500.0, 2500.0] {
            let settings = BrushSettings { size, ..preset.settings.clone() };
            let (average, slowest) = time_stroke(settings);
            println!("{:<16} {:>6} {:>8.1}ms {:>8.1}ms", preset.name, size, average, slowest);
        }
    }
}

/// Paints one stroke, a pen sample every 6 pixels across the canvas, and
/// times each sample.
fn time_stroke(settings: BrushSettings) -> (f64, f64) {
    let mut canvas = Canvas::new(3840, 2160);
    let mut stroke = BrushStroke::new(settings, BrushMode::Paint(Color::rgb(40, 90, 50)));
    let mut times = Vec::new();
    for i in 0..150 {
        let t = i as f32 / 149.0;
        let at = Point::new(400.0 + t * 3000.0, 1080.0 + (t * 6.0).sin() * 300.0);
        let pressure = 0.3 + 0.7 * (t * std::f32::consts::PI).sin();
        let start = Instant::now();
        stroke.add_point(&mut canvas, at, pressure);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let average = times.iter().sum::<f64>() / times.len() as f64;
    let slowest = times.iter().copied().fold(0.0, f64::max);
    (average, slowest)
}
