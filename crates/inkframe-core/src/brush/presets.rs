//! Brush presets: named brushes, ready-made or saved by the person.
//!
//! The ready-made ones are chosen for painting scenes and backgrounds, as
//! a painter's starter set: round brushes, pencil, chalk and charcoal,
//! bristle and flat brushes, watercolour, and stamping brushes for foliage,
//! grass, clouds, rocks and spatter. Their sampled tips are drawn here in
//! code (seeded, so the same every time) rather than shipped as pictures.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::i18n::tr_later;

use super::BrushSettings;
use super::dynamics::{ColorDynamics, Control, Random, Scattering, ShapeDynamics, Transfer};
use super::texture::{Blend, Pattern, Texture};
use super::tip::{Tip, TipImage};

/// A brush with a name, as the presets list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrushPreset {
    pub name: String,
    pub settings: BrushSettings,
}

impl BrushPreset {
    pub fn new(name: impl Into<String>, settings: BrushSettings) -> Self {
        Self { name: name.into(), settings }
    }
}

/// The ready-made brushes.
pub fn built_in() -> Vec<BrushPreset> {
    let base = BrushSettings::default();
    let pressure_flow = Transfer { on: true, flow_control: Control::PenPressure, ..Transfer::default() };
    let no_pressure = ShapeDynamics { on: false, ..ShapeDynamics::default() };
    let paper = |depth: f32, pattern: Pattern| Texture {
        on: true,
        pattern,
        mode: Blend::Height,
        depth,
        ..Texture::default()
    };
    let stamping = |scatter: f32, count: u32| Scattering {
        on: true,
        scatter,
        both_axes: true,
        count,
        ..Scattering::default()
    };
    let wander = |fg_bg: f32, hue: f32, brightness: f32| ColorDynamics {
        on: true,
        per_tip: true,
        fg_bg_jitter: fg_bg,
        hue_jitter: hue,
        brightness_jitter: brightness,
        ..ColorDynamics::default()
    };
    vec![
        BrushPreset::new(
            tr_later("Hard Round"),
            BrushSettings { hardness: 1.0, spacing: 0.1, ..base.clone() },
        ),
        BrushPreset::new(tr_later("Soft Round"), BrushSettings { size: 60.0, hardness: 0.0, ..base.clone() }),
        BrushPreset::new(
            tr_later("Airbrush"),
            BrushSettings {
                size: 90.0,
                hardness: 0.0,
                flow: 0.08,
                spacing: 0.08,
                shape: no_pressure,
                transfer: pressure_flow,
                build_up: true,
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Inking Pen"),
            BrushSettings {
                size: 8.0,
                hardness: 0.95,
                spacing: 0.05,
                shape: ShapeDynamics { on: true, minimum_diameter: 0.05, ..ShapeDynamics::default() },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Pencil"),
            BrushSettings {
                size: 4.0,
                hardness: 0.9,
                spacing: 0.08,
                texture: paper(0.35, Pattern::Grain),
                transfer: Transfer {
                    on: true,
                    opacity_control: Control::PenPressure,
                    minimum_opacity: 0.2,
                    ..Transfer::default()
                },
                shape: ShapeDynamics { on: true, minimum_diameter: 0.6, ..ShapeDynamics::default() },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Chalk"),
            BrushSettings {
                tip: sampled("Chalk", chalk_tip()),
                size: 30.0,
                spacing: 0.15,
                texture: paper(0.45, Pattern::Paper),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Charcoal"),
            BrushSettings {
                tip: sampled("Charcoal", charcoal_tip()),
                size: 40.0,
                spacing: 0.12,
                texture: paper(0.5, Pattern::RoughPaper),
                shape: ShapeDynamics {
                    on: true,
                    angle_control: Control::Direction,
                    minimum_diameter: 0.4,
                    ..ShapeDynamics::default()
                },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Dry Brush"),
            BrushSettings {
                tip: sampled("Bristles", bristle_tip(7)),
                size: 50.0,
                flow: 0.8,
                spacing: 0.08,
                // Across the stroke, so each bristle drags a streak.
                angle: 90.0,
                texture: Texture { on: true, mode: Blend::LinearHeight, depth: 0.4, ..Texture::default() },
                shape: ShapeDynamics {
                    on: true,
                    angle_control: Control::Direction,
                    minimum_diameter: 0.6,
                    ..ShapeDynamics::default()
                },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Oil Bristle"),
            BrushSettings {
                tip: sampled("Bristles", bristle_tip(11)),
                size: 60.0,
                spacing: 0.06,
                angle: 90.0,
                shape: ShapeDynamics {
                    on: true,
                    angle_control: Control::Direction,
                    minimum_diameter: 0.5,
                    ..ShapeDynamics::default()
                },
                transfer: Transfer { minimum_flow: 0.45, ..pressure_flow },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Flat Brush"),
            BrushSettings {
                size: 40.0,
                hardness: 0.9,
                roundness: 0.25,
                angle: 90.0,
                spacing: 0.05,
                shape: ShapeDynamics {
                    on: true,
                    angle_control: Control::Direction,
                    minimum_diameter: 0.6,
                    ..ShapeDynamics::default()
                },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Watercolour"),
            BrushSettings {
                size: 70.0,
                hardness: 0.2,
                flow: 0.35,
                spacing: 0.08,
                wet_edges: true,
                texture: Texture {
                    on: true,
                    pattern: Pattern::RoughPaper,
                    mode: Blend::Multiply,
                    depth: 0.35,
                    ..Texture::default()
                },
                transfer: pressure_flow,
                shape: ShapeDynamics { on: true, minimum_diameter: 0.5, ..ShapeDynamics::default() },
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Sponge"),
            BrushSettings {
                tip: sampled("Sponge", sponge_tip()),
                size: 70.0,
                spacing: 0.35,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::Off,
                    size_jitter: 0.3,
                    angle_jitter: 1.0,
                    flip_x_jitter: true,
                    ..ShapeDynamics::default()
                },
                scattering: stamping(0.4, 1),
                color: wander(0.2, 0.02, 0.15),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Foliage"),
            BrushSettings {
                tip: sampled("Leaf", leaf_tip()),
                size: 36.0,
                spacing: 0.4,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::PenPressure,
                    minimum_diameter: 0.3,
                    size_jitter: 0.6,
                    angle_jitter: 1.0,
                    flip_x_jitter: true,
                    ..ShapeDynamics::default()
                },
                scattering: stamping(1.5, 3),
                color: wander(0.5, 0.04, 0.25),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Grass"),
            BrushSettings {
                tip: sampled("Blade", blade_tip()),
                size: 60.0,
                spacing: 0.15,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::PenPressure,
                    minimum_diameter: 0.3,
                    size_jitter: 0.5,
                    angle_jitter: 0.08,
                    flip_x_jitter: true,
                    ..ShapeDynamics::default()
                },
                scattering: Scattering { on: true, scatter: 0.6, count: 3, ..Scattering::default() },
                color: wander(0.5, 0.03, 0.2),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Clouds"),
            BrushSettings {
                tip: sampled("Cloud", cloud_tip()),
                size: 140.0,
                flow: 0.3,
                spacing: 0.3,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::Off,
                    size_jitter: 0.5,
                    angle_jitter: 1.0,
                    roundness_jitter: 0.3,
                    minimum_roundness: 0.5,
                    ..ShapeDynamics::default()
                },
                scattering: stamping(0.6, 2),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Rocks"),
            BrushSettings {
                tip: sampled("Sponge", sponge_tip()),
                size: 80.0,
                spacing: 0.5,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::Off,
                    size_jitter: 0.5,
                    angle_jitter: 1.0,
                    ..ShapeDynamics::default()
                },
                texture: Texture {
                    on: true,
                    pattern: Pattern::Sponge,
                    mode: Blend::Multiply,
                    depth: 0.6,
                    scale: 2.0,
                    ..Texture::default()
                },
                color: wander(0.4, 0.0, 0.3),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Spatter"),
            BrushSettings {
                tip: sampled("Spatter", spatter_tip()),
                size: 60.0,
                spacing: 0.5,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::Off,
                    size_jitter: 0.8,
                    angle_jitter: 1.0,
                    ..ShapeDynamics::default()
                },
                scattering: stamping(2.0, 1),
                ..base.clone()
            },
        ),
        BrushPreset::new(
            tr_later("Stars and Snow"),
            BrushSettings {
                size: 6.0,
                hardness: 0.6,
                spacing: 1.5,
                shape: ShapeDynamics {
                    on: true,
                    size_control: Control::Off,
                    size_jitter: 0.9,
                    ..ShapeDynamics::default()
                },
                scattering: stamping(8.0, 2),
                transfer: Transfer { on: true, opacity_jitter: 0.6, ..Transfer::default() },
                ..base
            },
        ),
    ]
}

fn sampled(name: &str, (size, amounts): (u32, Vec<u8>)) -> Tip {
    Tip::Sampled(Arc::new(TipImage::new(name, size, amounts.len() as u32 / size, amounts)))
}

/// A square tip `size` across, from how much paint `at` gives each point
/// (each −1 to 1 across it).
fn draw(size: u32, at: impl Fn(f32, f32) -> f32) -> (u32, Vec<u8>) {
    let amounts = (0..size * size)
        .map(|i| {
            let x = (i % size) as f32 / (size - 1) as f32 * 2.0 - 1.0;
            let y = (i / size) as f32 / (size - 1) as f32 * 2.0 - 1.0;
            (at(x, y).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect();
    (size, amounts)
}

fn noise(x: f32, y: f32, scale: f32, layer: u32) -> f32 {
    super::texture::value_noise((x + 1.0) * scale, (y + 1.0) * scale, layer)
}

/// A disc with a crumbly edge.
fn chalk_tip() -> (u32, Vec<u8>) {
    draw(64, |x, y| {
        let r = (x * x + y * y).sqrt() + (noise(x, y, 10.0, 20) - 0.5) * 0.35;
        ((0.9 - r) * 6.0) * (0.75 + 0.25 * noise(x, y, 25.0, 21))
    })
}

/// A rough, slightly long stick end.
fn charcoal_tip() -> (u32, Vec<u8>) {
    draw(64, |x, y| {
        let r = ((x * 0.8).powi(2) + (y * 1.4).powi(2)).sqrt() + (noise(x, y, 8.0, 22) - 0.5) * 0.5;
        ((0.85 - r) * 5.0) * (0.5 + 0.5 * noise(x, y, 30.0, 23))
    })
}

/// A row of bristle prints, each a soft dot, some fainter: a flat brush's
/// mark. `seed` changes which.
fn bristle_tip(seed: u64) -> (u32, Vec<u8>) {
    let mut random = Random::new(seed);
    let bristles: Vec<(f32, f32, f32, f32)> = (0..40)
        .map(|_| {
            (
                random.signed() * 0.92,
                random.signed() * 0.3,
                0.04 + random.next() * 0.06,
                0.4 + random.next() * 0.6,
            )
        })
        .collect();
    draw(96, |x, y| {
        bristles
            .iter()
            .map(|&(bx, by, r, strength)| {
                let d = ((x - bx).powi(2) + (y - by).powi(2)).sqrt();
                ((r - d) / r * 2.0).clamp(0.0, 1.0) * strength
            })
            .fold(0.0, f32::max)
    })
}

/// Holes and blobs inside a rough circle: a natural sponge's print.
fn sponge_tip() -> (u32, Vec<u8>) {
    draw(96, |x, y| {
        let edge = 1.0 - (x * x + y * y).sqrt() - (noise(x, y, 4.0, 24) - 0.5) * 0.5;
        let holes = noise(x, y, 9.0, 25) * 0.6 + noise(x, y, 20.0, 26) * 0.4;
        (edge * 4.0).min(1.0) * ((holes - 0.42) * 5.0)
    })
}

/// A pointed leaf, darker along its middle.
fn leaf_tip() -> (u32, Vec<u8>) {
    draw(64, |x, y| {
        // Widest a little below the middle, pointed at the top.
        let width = (1.0 - y * y).max(0.0).sqrt() * (0.55 - y * 0.15);
        let inside = (width - x.abs()) / width.max(0.01);
        (inside * 4.0).min(1.0) * (0.75 + 0.25 * (1.0 - x.abs() * 3.0).max(0.0))
    })
}

/// A blade of grass: wide at the bottom, tapering to a point at the top,
/// with a slight lean.
fn blade_tip() -> (u32, Vec<u8>) {
    draw(64, |x, y| {
        let t = (y + 1.0) / 2.0; // 0 at the top, 1 at the bottom
        let centre = (1.0 - t).powi(2) * 0.25;
        let width = 0.12 * t.sqrt();
        ((width - (x - centre).abs()) / width.max(0.005) * 3.0).min(1.0)
    })
}

/// A lumpy, soft cloud puff.
fn cloud_tip() -> (u32, Vec<u8>) {
    draw(96, |x, y| {
        let lumps = noise(x, y, 3.0, 27) * 0.6 + noise(x, y, 7.0, 28) * 0.4;
        let r = (x * x + y * y * 1.4).sqrt();
        ((1.0 - r) * 1.6 + (lumps - 0.5) * 1.2).clamp(0.0, 1.0).powf(1.5)
    })
}

/// Drops of paint, a big one and many small, flicked.
fn spatter_tip() -> (u32, Vec<u8>) {
    let mut random = Random::new(5);
    let drops: Vec<(f32, f32, f32)> = (0..30)
        .map(|i| {
            let r = if i == 0 { 0.18 } else { 0.015 + random.next().powi(3) * 0.08 };
            (random.signed() * 0.85, random.signed() * 0.85, r)
        })
        .collect();
    draw(96, |x, y| {
        drops
            .iter()
            .map(|&(dx, dy, r)| ((r - ((x - dx).powi(2) + (y - dy).powi(2)).sqrt()) / 0.02).clamp(0.0, 1.0))
            .fold(0.0, f32::max)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ready_made_brush_has_a_name_and_paint_in_its_tip() {
        let presets = built_in();
        assert!(presets.len() >= 15);
        let mut names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), presets.len(), "names are unique");
        for preset in &presets {
            if let Tip::Sampled(image) = &preset.settings.tip {
                let painted = image.amounts.iter().filter(|&&a| a > 128).count();
                assert!(painted > 20, "{} has paint in its tip", preset.name);
            }
        }
    }

    #[test]
    fn presets_save_and_load() {
        for preset in built_in() {
            let json = serde_json::to_string(&preset).unwrap();
            assert_eq!(serde_json::from_str::<BrushPreset>(&json).unwrap(), preset, "{}", preset.name);
        }
    }
}
