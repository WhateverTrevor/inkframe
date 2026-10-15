//! Layer blend modes: how a layer's colours mix with what's beneath it.
//!
//! Normal simply covers what's below. The others combine the two, the way
//! Photoshop's do: Multiply darkens (good for shading over line art),
//! Screen lightens (glows), Overlay adds contrast, and so on. They apply to
//! every kind of layer, drawn or painted.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    /// Photoshop's "Linear Dodge (Add)": the colours added together.
    Add,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// In Photoshop's menu order, grouped by what they do.
    pub const ALL: [BlendMode; 17] = {
        use BlendMode::*;
        [
            Normal, Darken, Multiply, ColorBurn, Lighten, Screen, ColorDodge, Add, Overlay, SoftLight,
            HardLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity,
        ]
    };

    pub fn label(self) -> &'static str {
        use BlendMode::*;
        match self {
            Normal => "Normal",
            Multiply => "Multiply",
            Screen => "Screen",
            Overlay => "Overlay",
            Darken => "Darken",
            Lighten => "Lighten",
            ColorDodge => "Color Dodge",
            ColorBurn => "Color Burn",
            Add => "Add",
            HardLight => "Hard Light",
            SoftLight => "Soft Light",
            Difference => "Difference",
            Exclusion => "Exclusion",
            Hue => "Hue",
            Saturation => "Saturation",
            Color => "Color",
            Luminosity => "Luminosity",
        }
    }

    pub fn is_normal(&self) -> bool {
        *self == BlendMode::Normal
    }
}
