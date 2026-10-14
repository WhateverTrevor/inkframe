//! Laying out text as outlines.
//!
//! Text goes through three steps, each handled by a library:
//!
//! 1. **Finding the font.** [fontdb] indexes the fonts installed on the
//!    system and picks the closest match for a family, weight and slant.
//! 2. **Shaping.** [harfrust] (a Rust port of HarfBuzz) turns a string into
//!    positioned glyphs. That's more than one glyph per letter: it applies
//!    kerning (tucking "A" under "V"), ligatures ("fi" as one glyph), and
//!    the joining and reordering scripts like Arabic and Devanagari need.
//! 3. **Outlines.** [skrifa] reads each glyph's outline, which becomes part
//!    of an ordinary path, filled and stroked like any other shape. Text is
//!    therefore crisp at any size and rotation, and exports exactly as shown.
//!
//! Laying text out is slow next to drawing it, so finished layouts are
//! cached by everything that affects them.
//!
//! [fontdb]: https://docs.rs/fontdb
//! [harfrust]: https://docs.rs/harfrust
//! [skrifa]: https://docs.rs/skrifa

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use fontdb::{Database, Family, Query, Stretch, Weight};
use harfrust::{ShapeOptions, ShaperData, UnicodeBuffer};
use inkframe_core::{Font, Point, Rect, Text, TextAlign};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};
use tiny_skia::{Path, PathBuilder};

/// Text laid out in local coordinates, with the first line's baseline at
/// y = 0 (see [`TextAlign`]).
pub struct TextLayout {
    /// Every glyph's outline in one path, or `None` for blank text.
    pub path: Option<Path>,
    /// The block's extent: the full width of its longest line, from the top
    /// of the first line to the bottom of the last.
    pub bounds: Rect,
    /// The family actually used. Differs from the one asked for when that
    /// font isn't installed.
    pub family: String,
}

/// Lays out `text`, reusing an earlier layout when nothing has changed.
pub fn layout(text: &Text) -> Arc<TextLayout> {
    let key = LayoutKey::of(text);
    if let Some(layout) = cache().lock().expect("text cache lock").get(&key) {
        return layout.clone();
    }
    let layout = Arc::new(lay_out(text));
    let mut cache = cache().lock().expect("text cache lock");
    // Every edit to the text makes a new entry. Clearing out now and then
    // keeps a long editing session from piling them up.
    if cache.len() > 4096 {
        cache.clear();
    }
    cache.insert(key, layout.clone());
    layout
}

/// The families of every installed font (plus the bundled fallback),
/// sorted, for a font menu.
pub fn font_families() -> Vec<String> {
    let mut families: Vec<String> = fonts()
        .database
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.dedup();
    families
}

/// Indexes the installed fonts now rather than on the first text drawn.
/// Scanning the system's fonts takes a moment, so the editor does this on a
/// background thread at startup.
pub fn load_fonts() {
    fonts();
}

/// Everything that changes a layout. Fill, stroke and the layer's
/// transform don't: they're applied to the finished outlines.
#[derive(PartialEq, Eq, Hash)]
struct LayoutKey {
    content: String,
    font: Font,
    size: u32,
    align: TextAlign,
    line_spacing: u32,
}

impl LayoutKey {
    fn of(text: &Text) -> Self {
        // f32 can't be hashed directly; its bit pattern can.
        Self {
            content: text.content.clone(),
            font: text.font.clone(),
            size: text.size.to_bits(),
            align: text.align,
            line_spacing: text.line_spacing.to_bits(),
        }
    }
}

fn cache() -> &'static Mutex<HashMap<LayoutKey, Arc<TextLayout>>> {
    static CACHE: OnceLock<Mutex<HashMap<LayoutKey, Arc<TextLayout>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

struct Fonts {
    database: Database,
    /// The font bundled with Inkframe, used when no installed font matches.
    fallback: fontdb::ID,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        database.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        // Fonts are added in order, so the bundled one is the last face.
        let fallback = database.faces().last().expect("the bundled font was just added").id;
        Fonts { database, fallback }
    })
}

/// The installed face closest to `font`: the family by name, then the
/// system's sans-serif family, then the bundled font. Bold and italic pick
/// the nearest weight and slant the family has.
fn find_face(font: &Font) -> fontdb::ID {
    let fonts = fonts();
    let query = Query {
        families: &[Family::Name(&font.family), Family::SansSerif],
        weight: if font.bold { Weight::BOLD } else { Weight::NORMAL },
        stretch: Stretch::Normal,
        style: if font.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
    };
    fonts.database.query(&query).unwrap_or(fonts.fallback)
}

fn lay_out(text: &Text) -> TextLayout {
    let fonts = fonts();
    let face = find_face(&text.font);
    let family = fonts.database.face(face).and_then(|f| f.families.first()).map(|(name, _)| name.clone());
    let family = family.unwrap_or_default();
    // A font file that can't be parsed draws nothing rather than failing.
    let blank =
        |family| TextLayout { path: None, bounds: Rect::from_points(Point::ZERO, Point::ZERO), family };
    fonts
        .database
        .with_face_data(face, |data, index| match skrifa::FontRef::from_index(data, index) {
            Ok(font) => lay_out_with(text, &font, family.clone()),
            Err(_) => blank(family.clone()),
        })
        .unwrap_or_else(|| blank(family))
}

fn lay_out_with(text: &Text, font: &skrifa::FontRef, family: String) -> TextLayout {
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    // Fonts are designed on a grid of "units per em"; this converts those
    // units to the text's size in stage pixels.
    let scale = text.size / metrics.units_per_em.max(1) as f32;
    let ascent = metrics.ascent * scale;
    let descent = -metrics.descent * scale; // stored negative (below the baseline)
    let line_height = text.size * text.line_spacing;

    let shaper_data = ShaperData::new(font);
    let shaper = shaper_data.shaper(font).build();
    let outlines = font.outline_glyphs();

    let mut path = PathBuilder::new();
    let (mut left, mut right) = (0.0f32, 0.0f32);
    let lines: Vec<&str> = text.content.split('\n').collect();
    for (number, line) in lines.iter().enumerate() {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(line);
        // Works out the script and direction (left-to-right or not) from
        // the characters themselves.
        buffer.guess_segment_properties();
        let glyphs = shaper.shape(buffer, ShapeOptions::new());

        let width: f32 = glyphs.glyph_positions().iter().map(|p| p.x_advance as f32 * scale).sum();
        let start = match text.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -width / 2.0,
            TextAlign::Right => -width,
        };
        left = left.min(start);
        right = right.max(start + width);

        let baseline = number as f32 * line_height;
        let mut pen_x = start;
        for (info, position) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let origin = Point::new(
                pen_x + position.x_offset as f32 * scale,
                baseline - position.y_offset as f32 * scale,
            );
            if let Some(glyph) = outlines.get(GlyphId::new(info.glyph_id)) {
                let mut pen = GlyphPen { path: &mut path, origin, scale };
                // A glyph that fails to draw is left out rather than
                // failing the whole line.
                let _ =
                    glyph.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen);
            }
            pen_x += position.x_advance as f32 * scale;
        }
    }

    let bottom = (lines.len() - 1) as f32 * line_height + descent;
    TextLayout {
        path: path.finish(),
        bounds: Rect::from_points(Point::new(left, -ascent), Point::new(right, bottom)),
        family,
    }
}

/// Adds a glyph's outline to a path, placed at `origin` and scaled from
/// font units. Fonts measure y upward; the stage measures it downward.
struct GlyphPen<'a> {
    path: &'a mut PathBuilder,
    origin: Point,
    scale: f32,
}

impl GlyphPen<'_> {
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        (self.origin.x + x * self.scale, self.origin.y - y * self.scale)
    }
}

impl OutlinePen for GlyphPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let ((cx, cy), (x, y)) = (self.at(cx, cy), self.at(x, y));
        self.path.quad_to(cx, cy, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let ((cx0, cy0), (cx1, cy1), (x, y)) = (self.at(cx0, cy0), self.at(cx1, cy1), self.at(x, y));
        self.path.cubic_to(cx0, cy0, cx1, cy1, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text in the bundled font, which every machine has, so these tests
    /// don't depend on what's installed.
    fn text(content: &str) -> Text {
        let mut text = Text::new(content, 100.0);
        text.font.family = "Ubuntu".into();
        text
    }

    #[test]
    fn lines_start_at_the_origin_and_sit_on_the_baseline() {
        let layout = layout(&text("Hello"));
        assert!(layout.path.is_some());
        let b = layout.bounds;
        assert_eq!(b.min.x, 0.0);
        assert!(b.max.x > 150.0, "five letters at 100 px are wider than that: {}", b.max.x);
        // Ascent is above the baseline (negative y), descent below.
        assert!(b.min.y < -50.0 && b.max.y > 0.0 && b.max.y < 50.0, "{b:?}");
    }

    #[test]
    fn alignment_moves_lines_around_the_origin() {
        let left = layout(&text("Wide")).bounds;
        let mut centered = text("Wide");
        centered.align = TextAlign::Center;
        let centered = layout(&centered).bounds;
        let mut right = text("Wide");
        right.align = TextAlign::Right;
        let right = layout(&right).bounds;

        assert!((centered.min.x + centered.max.x).abs() < 1e-3, "centered on the origin");
        assert!(right.max.x.abs() < 1e-3 && (right.min.x + left.max.x).abs() < 1e-3, "ends at the origin");
    }

    #[test]
    fn each_line_adds_one_line_spacing() {
        let one = layout(&text("A")).bounds;
        let three = layout(&text("A\nB\nC")).bounds;
        // Line spacing is 1.2 × 100 px.
        assert!((three.max.y - one.max.y - 240.0).abs() < 1e-3);
    }

    #[test]
    fn kerning_tucks_letters_together() {
        // "AV" is the classic kerning pair: shaped together, the V slides
        // under the A, so the pair is narrower than the letters apart.
        let pair = layout(&text("AV")).bounds.width();
        let apart = layout(&text("A")).bounds.width() + layout(&text("V")).bounds.width();
        assert!(pair <= apart, "{pair} vs {apart}");
    }

    #[test]
    fn missing_fonts_fall_back_to_one_that_exists() {
        let mut text = text("Hi");
        text.font.family = "No Such Font, Surely".into();
        let layout = layout(&text);
        assert!(layout.path.is_some());
        assert_ne!(layout.family, text.font.family);
    }
}
