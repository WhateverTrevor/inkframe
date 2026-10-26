//! Texture: a pattern pressed into the paint, as if painting over a rough
//! surface, the way Photoshop's Texture section works. The pattern stays
//! put on the canvas (paper doesn't move with the brush), and is read as
//! heights, 0 (a hollow) to 1 (a peak). How it bites depends on its mode
//! and depth: Height catches paint only on the peaks, like chalk dragged
//! over paper, while Multiply and the others darken the paint into the
//! hollows. Dual Brush combines two tips with the same modes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::dynamics::Control;
use super::tip::TipImage;

/// The Texture section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Texture {
    pub on: bool,
    pub pattern: Pattern,
    pub invert: bool,
    /// How big the pattern is: 1 as made, 2 twice as big.
    pub scale: f32,
    /// Each −1 to 1, applied to the pattern's heights.
    pub brightness: f32,
    pub contrast: f32,
    /// Pressed into each dab, so the depth can vary dab by dab, rather than
    /// into the whole stroke at once.
    pub each_tip: bool,
    pub mode: Blend,
    /// How deep the texture bites, 0 to 1.
    pub depth: f32,
    /// With each tip: the least depth the jitter or control leaves.
    pub minimum_depth: f32,
    pub depth_jitter: f32,
    pub depth_control: Control,
}

impl Default for Texture {
    fn default() -> Self {
        Self {
            on: false,
            pattern: Pattern::Paper,
            invert: false,
            scale: 1.0,
            brightness: 0.0,
            contrast: 0.0,
            each_tip: false,
            mode: Blend::Multiply,
            depth: 1.0,
            minimum_depth: 0.0,
            depth_jitter: 0.0,
            depth_control: Control::Off,
        }
    }
}

impl Texture {
    /// The pattern's height at canvas pixel (`x`, `y`), after scale,
    /// brightness, contrast and inverting.
    pub fn height(&self, x: u32, y: u32) -> f32 {
        let scale = self.scale.max(0.05);
        let h = self.pattern.height(x as f32 / scale, y as f32 / scale);
        let h = if self.invert { 1.0 - h } else { h };
        ((h - 0.5) * (1.0 + self.contrast) + 0.5 + self.brightness).clamp(0.0, 1.0)
    }

    /// `amount` of paint with the texture pressed in at `depth`.
    pub fn apply(&self, amount: f32, x: u32, y: u32, depth: f32) -> f32 {
        if amount <= 0.0 {
            return 0.0;
        }
        self.mode.combine(amount, self.height(x, y), depth)
    }
}

/// A texture's pattern: made up on the fly (seamless, and the same every
/// time), or a greyscale picture repeated across the canvas.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum Pattern {
    /// Smooth bumps a few pixels across: drawing paper.
    #[default]
    Paper,
    /// Bumps of a few sizes together: watercolour paper.
    RoughPaper,
    /// A woven grid: canvas.
    Canvas,
    /// A fine speckle, a pixel or two: grainy and dry.
    Grain,
    /// Pitted blobs, like a sponge.
    Sponge,
    /// Long fibres, mostly one way, like wood or bark.
    Fibre,
    Picture(Arc<TipImage>),
}

impl Pattern {
    pub const MADE: [Pattern; 6] = [
        Pattern::Paper,
        Pattern::RoughPaper,
        Pattern::Canvas,
        Pattern::Grain,
        Pattern::Sponge,
        Pattern::Fibre,
    ];

    pub fn label(&self) -> &str {
        match self {
            Pattern::Paper => "Paper",
            Pattern::RoughPaper => "Rough Paper",
            Pattern::Canvas => "Canvas",
            Pattern::Grain => "Grain",
            Pattern::Sponge => "Sponge",
            Pattern::Fibre => "Fibre",
            Pattern::Picture(image) => &image.name,
        }
    }

    /// The height at (`x`, `y`) in the pattern's own pixels, 0 to 1.
    pub fn height(&self, x: f32, y: f32) -> f32 {
        match self {
            Pattern::Paper => value_noise(x / 3.0, y / 3.0, 0),
            Pattern::RoughPaper => {
                0.5 * value_noise(x / 9.0, y / 9.0, 1)
                    + 0.3 * value_noise(x / 3.5, y / 3.5, 2)
                    + 0.2 * value_noise(x / 1.5, y / 1.5, 3)
            }
            Pattern::Canvas => {
                // Threads over and under in a grid 8 pixels across, with a
                // little roughness so it doesn't look printed.
                let (cx, cy) = ((x / 8.0).floor(), (y / 8.0).floor());
                let (fx, fy) = (x / 8.0 - cx, y / 8.0 - cy);
                let across = (std::f32::consts::PI * fx).sin();
                let down = (std::f32::consts::PI * fy).sin();
                let over = if (cx + cy) as i64 % 2 == 0 { across } else { down };
                (0.75 * over + 0.25 * value_noise(x / 2.0, y / 2.0, 4)).clamp(0.0, 1.0)
            }
            Pattern::Grain => value_noise(x / 1.2, y / 1.2, 5),
            Pattern::Sponge => {
                let n = 0.6 * value_noise(x / 6.0, y / 6.0, 6) + 0.4 * value_noise(x / 2.5, y / 2.5, 7);
                // Sharpened, so it's mostly solid with holes.
                ((n - 0.35) * 3.0).clamp(0.0, 1.0)
            }
            Pattern::Fibre => {
                0.7 * value_noise(x / 24.0, y / 1.5, 8) + 0.3 * value_noise(x / 6.0, y / 0.8, 9)
            }
            Pattern::Picture(image) => {
                let (w, h) = (image.width as f32, image.height as f32);
                let (u, v) = (x.rem_euclid(w) / w, y.rem_euclid(h) / h);
                // The picture's dark is paint for a tip; for a pattern it's
                // a hollow, as in Photoshop, so a dark pattern bites more.
                1.0 - image.sample(u, v, w)
            }
        }
    }
}

/// How a texture or a dual brush's tip combines with the paint: the modes
/// Photoshop offers for both. `paint` is the amount of paint (0 to 1),
/// `other` the texture's height or the second tip's amount, and `depth`
/// how strongly it applies.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Blend {
    #[default]
    Multiply,
    Subtract,
    Darken,
    Overlay,
    ColorDodge,
    ColorBurn,
    LinearBurn,
    HardMix,
    LinearHeight,
    Height,
}

impl Blend {
    pub const ALL: [Blend; 10] = [
        Blend::Multiply,
        Blend::Subtract,
        Blend::Darken,
        Blend::Overlay,
        Blend::ColorDodge,
        Blend::ColorBurn,
        Blend::LinearBurn,
        Blend::HardMix,
        Blend::LinearHeight,
        Blend::Height,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Blend::Multiply => "Multiply",
            Blend::Subtract => "Subtract",
            Blend::Darken => "Darken",
            Blend::Overlay => "Overlay",
            Blend::ColorDodge => "Color Dodge",
            Blend::ColorBurn => "Color Burn",
            Blend::LinearBurn => "Linear Burn",
            Blend::HardMix => "Hard Mix",
            Blend::LinearHeight => "Linear Height",
            Blend::Height => "Height",
        }
    }

    pub fn combine(self, paint: f32, other: f32, depth: f32) -> f32 {
        let depth = depth.clamp(0.0, 1.0);
        // The other, faded towards "no effect" (1) as depth lessens.
        let o = 1.0 - depth * (1.0 - other);
        let result = match self {
            Blend::Multiply => paint * o,
            Blend::Subtract => paint - (1.0 - o),
            Blend::Darken => paint.min(o),
            Blend::Overlay => {
                if paint < 0.5 { 2.0 * paint * o } else { 1.0 - 2.0 * (1.0 - paint) * (1.0 - o) }
                    .min(paint * 2.0)
            }
            Blend::ColorDodge => paint * (0.5 + o * 0.5) / (1.0 - (1.0 - o) * 0.5).max(0.05),
            Blend::ColorBurn => {
                if o <= 0.0 {
                    0.0
                } else {
                    1.0 - (1.0 - paint) / o
                }
            }
            Blend::LinearBurn => paint + o - 1.0,
            Blend::HardMix => smooth_step(1.0 - paint - 0.08, 1.0 - paint + 0.08, o) * paint,
            // Paint where the other is higher than how deep it bites: on
            // the peaks only. Linear Height's edge is softer.
            Blend::Height => paint * smooth_step(depth - 0.2, depth + 0.2, other),
            Blend::LinearHeight => paint * smooth_step(depth - 0.4, depth + 0.4, other),
        };
        result.clamp(0.0, 1.0)
    }
}

fn smooth_step(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth random bumps one unit apart, the same at the same place every
/// time, 0 to 1. `layer` makes independent sets.
pub(crate) fn value_noise(x: f32, y: f32, layer: u32) -> f32 {
    // Each layer's grid is turned a different way, so its bumps don't
    // line up in rows and columns, which reads as squares.
    let turn = 0.45 + layer as f32 * 0.83;
    let (sin, cos) = turn.sin_cos();
    let (x, y) = (x * cos - y * sin, x * sin + y * cos);
    let (cx, cy) = (x.floor(), y.floor());
    let (tx, ty) = (x - cx, y - cy);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(tx), smooth(ty));
    let (cx, cy) = (cx as i64 as u32, cy as i64 as u32);
    let h = |x: u32, y: u32| hash(x, y, layer);
    let top = h(cx, cy) + (h(cx.wrapping_add(1), cy) - h(cx, cy)) * sx;
    let bottom = h(cx, cy.wrapping_add(1))
        + (h(cx.wrapping_add(1), cy.wrapping_add(1)) - h(cx, cy.wrapping_add(1))) * sx;
    top + (bottom - top) * sy
}

/// A random-looking number for a grid point, 0 to 1.
pub(crate) fn hash(x: u32, y: u32, layer: u32) -> f32 {
    let mut n = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ layer.wrapping_mul(0xC2B2_AE3D);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    (n & 0xFFFF) as f32 / 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_stay_in_range_and_vary() {
        for pattern in Pattern::MADE {
            let heights: Vec<f32> =
                (0..400).map(|i| pattern.height((i % 20) as f32 * 1.7, (i / 20) as f32 * 1.3)).collect();
            assert!(heights.iter().all(|h| (0.0..=1.0).contains(h)), "{pattern:?}");
            let (lo, hi) = heights.iter().fold((1.0f32, 0.0f32), |(lo, hi), &h| (lo.min(h), hi.max(h)));
            assert!(hi - lo > 0.3, "{pattern:?} varies: {lo}..{hi}");
        }
    }

    #[test]
    fn no_depth_leaves_paint_as_it_was_in_the_darkening_modes() {
        for mode in [Blend::Multiply, Blend::Subtract, Blend::Darken, Blend::LinearBurn] {
            assert!((mode.combine(0.7, 0.2, 0.0) - 0.7).abs() < 1e-4, "{mode:?}");
        }
    }

    #[test]
    fn height_paints_the_peaks_and_not_the_hollows() {
        assert!(Blend::Height.combine(1.0, 0.9, 0.5) > 0.95);
        assert!(Blend::Height.combine(1.0, 0.1, 0.5) < 0.05);
        assert!(Blend::Multiply.combine(1.0, 0.2, 1.0) < 0.25);
    }

    #[test]
    fn brightness_contrast_and_invert_change_the_heights() {
        let texture = Texture { pattern: Pattern::Paper, ..Texture::default() };
        let h = texture.height(10, 10);
        assert!((Texture { invert: true, ..texture.clone() }.height(10, 10) - (1.0 - h)).abs() < 1e-4);
        assert_eq!(Texture { brightness: 1.0, ..texture.clone() }.height(10, 10), 1.0);
        let flat = Texture { contrast: -1.0, ..texture };
        assert!((flat.height(10, 10) - 0.5).abs() < 1e-4);
    }
}
