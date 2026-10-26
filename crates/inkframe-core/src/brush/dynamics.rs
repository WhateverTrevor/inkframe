//! How a brush's dabs vary along a stroke, as Photoshop's Brush Settings
//! sections do it: **Shape Dynamics** (size, angle, roundness, flips),
//! **Scattering** (dabs thrown off the path, several at a time), **Color
//! Dynamics** (hue, saturation and brightness wandering, or drifting to the
//! background colour) and **Transfer** (opacity and flow).
//!
//! Each varying setting has a **jitter**, how much it varies at random
//! (0 to 1), and a **control**, what else steers it: the pen's pressure or
//! tilt, the stroke's direction, or a fade over a number of dabs. A
//! control gives a value from 0 to 1, scaled up from the setting's
//! minimum.

use serde::{Deserialize, Serialize};

use crate::color::Color;

/// What steers a setting along a stroke, besides chance.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Control {
    /// Nothing: full strength throughout.
    #[default]
    Off,
    /// From full strength to the minimum over this many dabs.
    Fade(u32),
    PenPressure,
    /// The pen's lean: upright is full strength, laid flat the minimum.
    PenTilt,
    /// For an angle only: the way the stroke is heading.
    Direction,
    /// For an angle only: the way the stroke set off.
    InitialDirection,
}

impl Control {
    pub const ALL: [Control; 6] = [
        Control::Off,
        Control::Fade(25),
        Control::PenPressure,
        Control::PenTilt,
        Control::Direction,
        Control::InitialDirection,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Control::Off => "Off",
            Control::Fade(_) => "Fade",
            Control::PenPressure => "Pen Pressure",
            Control::PenTilt => "Pen Tilt",
            Control::Direction => "Direction",
            Control::InitialDirection => "Initial Direction",
        }
    }

    /// Whether it only makes sense for an angle (it gives a heading, not a
    /// strength).
    pub fn is_heading(self) -> bool {
        matches!(self, Control::Direction | Control::InitialDirection)
    }

    /// The control's strength for `dab`, 0 to 1 (headings count as full).
    pub fn strength(self, dab: &DabInput) -> f32 {
        match self {
            Control::Off | Control::Direction | Control::InitialDirection => 1.0,
            Control::Fade(steps) => 1.0 - (dab.index as f32 / steps.max(1) as f32).min(1.0),
            Control::PenPressure => dab.pressure,
            Control::PenTilt => {
                let lean = (dab.tilt.0 * dab.tilt.0 + dab.tilt.1 * dab.tilt.1).sqrt().min(1.0);
                1.0 - lean
            }
        }
    }

    /// `strength` scaled up from `minimum`: the value a setting is scaled
    /// by.
    pub fn scale(self, dab: &DabInput, minimum: f32) -> f32 {
        minimum + (1.0 - minimum) * self.strength(dab)
    }
}

/// What a dab knows of the pen and the stroke when it's laid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DabInput {
    /// 0 to 1; 1 for a mouse.
    pub pressure: f32,
    /// Sideways and forward lean, each −1 to 1.
    pub tilt: (f32, f32),
    /// The stroke's heading here and where it set off, in degrees
    /// (0 is rightward, counter-clockwise on screen as in Photoshop).
    pub direction: f32,
    pub initial_direction: f32,
    /// Which dab of the stroke it is, from 0, for fades.
    pub index: u32,
}

impl Default for DabInput {
    fn default() -> Self {
        Self { pressure: 1.0, tilt: (0.0, 0.0), direction: 0.0, initial_direction: 0.0, index: 0 }
    }
}

/// Shape Dynamics: each dab's size, angle, roundness and flips.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeDynamics {
    pub on: bool,
    pub size_jitter: f32,
    pub size_control: Control,
    /// The smallest a dab gets from the control or jitter, of the size.
    pub minimum_diameter: f32,
    /// With Pen Tilt steering size, how much bigger a flat pen paints.
    pub tilt_scale: f32,
    pub angle_jitter: f32,
    pub angle_control: Control,
    pub roundness_jitter: f32,
    pub roundness_control: Control,
    pub minimum_roundness: f32,
    pub flip_x_jitter: bool,
    pub flip_y_jitter: bool,
}

impl Default for ShapeDynamics {
    fn default() -> Self {
        Self {
            on: false,
            size_jitter: 0.0,
            size_control: Control::PenPressure,
            minimum_diameter: 0.15,
            tilt_scale: 2.0,
            angle_jitter: 0.0,
            angle_control: Control::Off,
            roundness_jitter: 0.0,
            roundness_control: Control::Off,
            minimum_roundness: 0.25,
            flip_x_jitter: false,
            flip_y_jitter: false,
        }
    }
}

/// Scattering: dabs thrown either side of the path, and how many per step.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scattering {
    pub on: bool,
    /// How far, in diameters (Photoshop's percent over 100; up to 10).
    pub scatter: f32,
    /// Along the path too, not only across it.
    pub both_axes: bool,
    pub scatter_control: Control,
    /// Dabs laid at each step, 1 to 16.
    pub count: u32,
    pub count_jitter: f32,
    pub count_control: Control,
}

impl Default for Scattering {
    fn default() -> Self {
        Self {
            on: false,
            scatter: 1.0,
            both_axes: false,
            scatter_control: Control::Off,
            count: 1,
            count_jitter: 0.0,
            count_control: Control::Off,
        }
    }
}

/// Color Dynamics: the paint's colour wandering, dab by dab or stroke by
/// stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorDynamics {
    pub on: bool,
    /// Each dab gets its own colour, or each stroke one for all of it.
    pub per_tip: bool,
    /// How far towards the background colour it may wander.
    pub fg_bg_jitter: f32,
    pub fg_bg_control: Control,
    pub hue_jitter: f32,
    pub saturation_jitter: f32,
    pub brightness_jitter: f32,
    /// −1 (grey) to 1 (fully saturated): a shift to every colour's
    /// saturation.
    pub purity: f32,
}

impl Default for ColorDynamics {
    fn default() -> Self {
        Self {
            on: false,
            per_tip: true,
            fg_bg_jitter: 0.0,
            fg_bg_control: Control::Off,
            hue_jitter: 0.0,
            saturation_jitter: 0.0,
            brightness_jitter: 0.0,
            purity: 0.0,
        }
    }
}

impl ColorDynamics {
    /// The colour of a dab (or stroke): `foreground` wandered as set,
    /// using `random` for each kind of wandering.
    pub fn color(&self, foreground: Color, background: Color, dab: &DabInput, random: &mut Random) -> Color {
        if !self.on {
            return foreground;
        }
        // Towards the background: with a control, as it weakens (lighter
        // pressure, later in a fade), as Photoshop does; then at random.
        let controlled =
            if self.fg_bg_control == Control::Off { 0.0 } else { 1.0 - self.fg_bg_control.strength(dab) };
        let towards = (controlled + self.fg_bg_jitter * random.next()).min(1.0);
        let mix = |a: u8, b: u8| a as f32 + (b as f32 - a as f32) * towards;
        let (r, g, b) = (
            mix(foreground.r, background.r),
            mix(foreground.g, background.g),
            mix(foreground.b, background.b),
        );
        let (mut h, mut s, mut v) = rgb_to_hsv(r / 255.0, g / 255.0, b / 255.0);
        h = (h + self.hue_jitter * (random.next() - 0.5)).rem_euclid(1.0);
        s = (s * (1.0 - self.saturation_jitter * random.next())).clamp(0.0, 1.0);
        v = (v * (1.0 - self.brightness_jitter * random.next())).clamp(0.0, 1.0);
        s = if self.purity >= 0.0 { s + (1.0 - s) * self.purity } else { s * (1.0 + self.purity) };
        let (r, g, b) = hsv_to_rgb(h, s, v);
        let byte = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        Color { r: byte(r), g: byte(g), b: byte(b), a: foreground.a }
    }
}

/// Transfer: each dab's opacity and flow.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transfer {
    pub on: bool,
    pub opacity_jitter: f32,
    pub opacity_control: Control,
    pub minimum_opacity: f32,
    pub flow_jitter: f32,
    pub flow_control: Control,
    pub minimum_flow: f32,
}

impl Default for Transfer {
    fn default() -> Self {
        Self {
            on: false,
            opacity_jitter: 0.0,
            opacity_control: Control::Off,
            minimum_opacity: 0.0,
            flow_jitter: 0.0,
            flow_control: Control::Off,
            minimum_flow: 0.0,
        }
    }
}

/// A small random number maker: the same seed, the same stroke, so a
/// stroke redraws exactly as it was painted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        // Never zero, which xorshift would stay at.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// The next number, 0 (inclusive) to 1 (exclusive).
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// −1 to 1.
    pub fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let sector = (h * 6.0).rem_euclid(6.0);
    let f = sector - sector.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match sector as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_give_strengths_from_the_pen_and_the_stroke() {
        let dab = DabInput { pressure: 0.25, tilt: (0.6, 0.8), index: 5, ..DabInput::default() };
        assert_eq!(Control::Off.strength(&dab), 1.0);
        assert_eq!(Control::PenPressure.strength(&dab), 0.25);
        assert!(Control::PenTilt.strength(&dab) < 0.01, "laid flat");
        assert_eq!(Control::Fade(10).strength(&dab), 0.5);
        assert_eq!(Control::Fade(10).strength(&DabInput { index: 20, ..dab }), 0.0);
        assert_eq!(Control::PenPressure.scale(&dab, 0.2), 0.4);
    }

    #[test]
    fn random_numbers_repeat_for_a_seed_and_stay_in_range() {
        let (mut a, mut b) = (Random::new(7), Random::new(7));
        for _ in 0..1000 {
            let n = a.next();
            assert_eq!(n, b.next());
            assert!((0.0..1.0).contains(&n));
        }
    }

    #[test]
    fn colour_dynamics_wander_from_the_foreground() {
        let (red, blue) = (Color::rgb(255, 0, 0), Color::rgb(0, 0, 255));
        let mut random = Random::new(1);
        let off = ColorDynamics::default();
        assert_eq!(off.color(red, blue, &DabInput::default(), &mut random), red);
        let to_background = ColorDynamics { on: true, fg_bg_control: Control::PenPressure, ..off };
        let light = DabInput { pressure: 0.0, ..DabInput::default() };
        assert_eq!(to_background.color(red, blue, &light, &mut random), blue, "no pressure: all background");
        let grey = ColorDynamics { on: true, purity: -1.0, ..off };
        let c = grey.color(red, blue, &DabInput::default(), &mut random);
        assert_eq!((c.r, c.g, c.b), (255, 255, 255), "red with no saturation is its brightness");
    }

    #[test]
    fn hsv_round_trips() {
        for (r, g, b) in [(0.2, 0.5, 0.9), (1.0, 0.0, 0.0), (0.3, 0.3, 0.3), (0.9, 0.8, 0.1)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((r - r2).abs() < 1e-4 && (g - g2).abs() < 1e-4 && (b - b2).abs() < 1e-4);
        }
    }
}
