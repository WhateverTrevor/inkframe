use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// An sRGB color with straight (non-premultiplied) alpha, 8 bits per channel.
///
/// Straight alpha is what users see in color pickers and what we store on
/// disk. The renderer converts to premultiplied alpha internally.
///
/// Colors serialize as `"#rrggbbaa"` strings so project files stay readable
/// and diff nicely in version control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    pub fn is_opaque(self) -> bool {
        self.a == 255
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid color {0:?}; expected #rrggbb or #rrggbbaa")]
pub struct ParseColorError(String);

impl FromStr for Color {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_owned());
        let hex = s.strip_prefix('#').ok_or_else(err)?;
        if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
            return Err(err());
        }

        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        let alpha = if hex.len() == 8 { channel(6)? } else { 255 };
        Ok(Color::rgba(channel(0)?, channel(2)?, channel(4)?, alpha))
    }
}

impl From<Color> for String {
    fn from(color: Color) -> Self {
        color.to_string()
    }
}

impl TryFrom<String> for Color {
    type Error = ParseColorError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Red, green and blue (each 0 to 1) as hue (0 to 1, round the colour
/// wheel from red), saturation and value.
pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
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

/// [`rgb_to_hsv`] the other way round.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
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
    fn hsv_round_trips() {
        for (r, g, b) in [(0.2, 0.5, 0.9), (1.0, 0.0, 0.0), (0.3, 0.3, 0.3), (0.9, 0.8, 0.1)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((r - r2).abs() < 1e-4 && (g - g2).abs() < 1e-4 && (b - b2).abs() < 1e-4);
        }
    }

    #[test]
    fn hex_round_trip() {
        let c = Color::rgba(0x12, 0xab, 0xff, 0x80);
        assert_eq!(c.to_string(), "#12abff80");
        assert_eq!("#12abff80".parse::<Color>().unwrap(), c);
    }

    #[test]
    fn six_digit_hex_is_opaque() {
        assert_eq!("#ff0000".parse::<Color>().unwrap(), Color::rgb(255, 0, 0));
    }

    #[test]
    fn rejects_garbage() {
        assert!("ff0000".parse::<Color>().is_err());
        assert!("#ff00".parse::<Color>().is_err());
        assert!("#gg0000".parse::<Color>().is_err());
    }
}
