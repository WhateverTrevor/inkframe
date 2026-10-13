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

#[cfg(test)]
mod tests {
    use super::*;

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
