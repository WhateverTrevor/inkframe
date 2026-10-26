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
            Normal => crate::i18n::tr_later("Normal"),
            Multiply => crate::i18n::tr_later("Multiply"),
            Screen => crate::i18n::tr_later("Screen"),
            Overlay => crate::i18n::tr_later("Overlay"),
            Darken => crate::i18n::tr_later("Darken"),
            Lighten => crate::i18n::tr_later("Lighten"),
            ColorDodge => crate::i18n::tr_later("Color Dodge"),
            ColorBurn => crate::i18n::tr_later("Color Burn"),
            Add => crate::i18n::tr_later("Add"),
            HardLight => crate::i18n::tr_later("Hard Light"),
            SoftLight => crate::i18n::tr_later("Soft Light"),
            Difference => crate::i18n::tr_later("Difference"),
            Exclusion => crate::i18n::tr_later("Exclusion"),
            Hue => crate::i18n::tr_later("Hue"),
            Saturation => crate::i18n::tr_later("Saturation"),
            Color => crate::i18n::tr_later("Color"),
            Luminosity => crate::i18n::tr_later("Luminosity"),
        }
    }

    pub fn is_normal(&self) -> bool {
        *self == BlendMode::Normal
    }
}
