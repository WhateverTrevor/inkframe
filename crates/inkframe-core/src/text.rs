//! Text on the stage.
//!
//! A text shape is a few lines of characters in one font, size and
//! alignment. It's painted with its element's fill and stroke like any
//! other shape, so text can be outlined, faded and animated the same way.
//!
//! Turning characters into outlines needs fonts, which live in
//! `inkframe-render`, so this crate can't lay text out itself. Instead the
//! editor measures the laid-out text and stores the result in
//! [`Text::measured`]; selection and hit-testing use that.

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// The characters. A newline starts a new line; lines don't wrap.
    pub content: String,
    pub font: Font,
    /// The font size in stage pixels at 100% scale: roughly the height of
    /// a line, from the top of the tallest letters to below the lowest.
    pub size: f32,
    #[serde(default)]
    pub align: TextAlign,
    /// Distance between baselines, as a multiple of `size`.
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f32,
    /// The laid-out text's extent in local coordinates, as last measured by
    /// the editor (see the module docs). Saved with the document so a file
    /// opened where its font isn't installed can still be selected.
    #[serde(default = "unmeasured")]
    pub measured: Rect,
}

/// Which font to draw with. Faces are looked up by family name, then the
/// closest weight and slant the system has.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Font {
    pub family: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

/// Where lines sit relative to the layer's origin, as in SVG's
/// `text-anchor`: left-aligned lines start at it, centered lines are
/// centered on it, right-aligned lines end at it. Vertically, the origin is
/// on the first line's baseline (the line letters sit on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];

    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => crate::i18n::tr_later("Left"),
            TextAlign::Center => crate::i18n::tr_later("Center"),
            TextAlign::Right => crate::i18n::tr_later("Right"),
        }
    }
}

impl Text {
    /// Text in a common font, ready to be measured.
    pub fn new(content: impl Into<String>, size: f32) -> Self {
        Self {
            content: content.into(),
            font: Font { family: DEFAULT_FAMILY.to_owned(), bold: false, italic: false },
            size,
            align: TextAlign::Left,
            line_spacing: default_line_spacing(),
            measured: unmeasured(),
        }
    }
}

/// Installed on every Mac and Windows PC, so new text looks the same
/// wherever the project is opened.
pub const DEFAULT_FAMILY: &str = "Arial";

fn default_line_spacing() -> f32 {
    1.2
}

/// Before measuring, a zero-size box at the origin.
fn unmeasured() -> Rect {
    Rect::from_points(Point::ZERO, Point::ZERO)
}
