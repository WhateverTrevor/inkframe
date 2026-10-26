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
//! It prints, for each brush and size, the average and the slowest time to
//! carry a stroke on by one pen sample (and the slowest after the first,
//! which also sets the stroke up), then the same for the retouching
//! tools (Smudge, Blur and the rest) over a painted canvas. Over about
//! 16 ms, painting can't keep up with a 60 Hz screen. Name a brush or tool
//! to time only that one.

use std::time::Instant;

use inkframe_core::{
    BrushMode, BrushSettings, BrushStroke, Canvas, Color, Point, Retouch, RetouchStroke, ToneRange,
};

fn main() {
    let only = std::env::args().nth(1);
    println!("{:<16} {:>6} {:>10} {:>10} {:>10}", "brush", "size", "average", "slowest", "then");
    for preset in BrushSettings::presets() {
        if only.as_ref().is_some_and(|name| !preset.name.to_lowercase().contains(&name.to_lowercase())) {
            continue;
        }
        for size in [100.0, 500.0, 1500.0, 2500.0] {
            let settings = BrushSettings { size, ..preset.settings.clone() };
            let (average, slowest, then) = time_stroke(settings);
            println!("{:<16} {:>6} {:>8.1}ms {:>8.1}ms {:>8.1}ms", preset.name, size, average, slowest, then);
        }
    }
    let tools = [
        ("Smudge", Retouch::Smudge { strength: 0.6 }),
        ("Blur", Retouch::Blur { strength: 0.6 }),
        ("Sharpen", Retouch::Sharpen { strength: 0.6 }),
        ("Dodge", Retouch::Dodge { exposure: 0.5, range: ToneRange::Midtones }),
        ("Burn", Retouch::Burn { exposure: 0.5, range: ToneRange::Midtones }),
        ("Clone Stamp", Retouch::Clone { offset: Point::new(-300.0, 0.0), opacity: 1.0 }),
    ];
    for (name, kind) in tools {
        if only.as_ref().is_some_and(|wanted| !name.to_lowercase().contains(&wanted.to_lowercase())) {
            continue;
        }
        for size in [100.0, 500.0, 1500.0, 2500.0] {
            let settings = BrushSettings { size, hardness: 0.3, ..BrushSettings::default() };
            let (average, slowest, then) = time_retouch(settings, kind);
            println!("{:<16} {:>6} {:>8.1}ms {:>8.1}ms {:>8.1}ms", name, size, average, slowest, then);
        }
    }
}

/// [`time_stroke`] for a retouching tool, over a canvas painted all over.
fn time_retouch(settings: BrushSettings, kind: Retouch) -> (f64, f64, f64) {
    let mut canvas = Canvas::new(3840, 2160);
    let colours: Vec<u8> =
        (0..3840 * 2160).flat_map(|i| [(i % 251) as u8, (i / 3840 % 241) as u8, 120, 255]).collect();
    canvas.write_region(0, 0, 3840, 2160, &colours);
    let mut stroke = RetouchStroke::new(settings, kind, &canvas);
    pen_path(|at, pressure| stroke.add_point(&mut canvas, at, pressure))
}

/// Paints one stroke, a pen sample every 6 pixels across the canvas, and
/// times each sample.
fn time_stroke(settings: BrushSettings) -> (f64, f64, f64) {
    let mut canvas = Canvas::new(3840, 2160);
    let mut stroke = BrushStroke::new(settings, BrushMode::Paint(Color::rgb(40, 90, 50)));
    pen_path(|at, pressure| stroke.add_point(&mut canvas, at, pressure))
}

/// Moves a pen across the canvas, a sample every 6 pixels or so, calling
/// `sample` at each and timing it: the average, the slowest and the
/// slowest after the first, in ms.
fn pen_path(mut sample: impl FnMut(Point, f32)) -> (f64, f64, f64) {
    let mut times = Vec::new();
    for i in 0..150 {
        let t = i as f32 / 149.0;
        let at = Point::new(400.0 + t * 3000.0, 1080.0 + (t * 6.0).sin() * 300.0);
        let pressure = 0.3 + 0.7 * (t * std::f32::consts::PI).sin();
        let start = Instant::now();
        sample(at, pressure);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let average = times.iter().sum::<f64>() / times.len() as f64;
    let slowest = times.iter().copied().fold(0.0, f64::max);
    let then = times[1..].iter().copied().fold(0.0, f64::max);
    (average, slowest, then)
}
