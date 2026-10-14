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

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use fontdb::{Database, Family, Query, Stretch, Weight};
use harfrust::{ShapeOptions, ShaperData, UnicodeBuffer};
use inkframe_core::{Document, Font, FontFileId, Point, Rect, Text, TextAlign};
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
    /// font isn't available.
    pub family: String,
    /// Where each line is, for placing a text cursor.
    pub lines: Vec<LineLayout>,
    /// How far letters reach above and below each baseline.
    pub ascent: f32,
    pub descent: f32,
}

/// One line of laid-out text.
pub struct LineLayout {
    /// The line's characters, as byte positions in the text's content (not
    /// counting the newline that ends it).
    pub range: Range<usize>,
    pub baseline: f32,
    /// Where a text cursor goes at each character boundary in the line,
    /// from `range.start` to `range.end`: (byte position, x).
    pub carets: Vec<(usize, f32)>,
}

/// Lays out `text`, reusing an earlier layout when nothing has changed.
pub fn layout(text: &Text) -> Arc<TextLayout> {
    let key = LayoutKey::of(text, read_fonts().generation);
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

/// The font families to offer in a menu: the project's own first, then
/// everything installed (plus the bundled fallback), each sorted.
#[derive(Debug, Default)]
pub struct FontFamilies {
    pub project: Vec<String>,
    pub installed: Vec<String>,
}

pub fn font_families() -> Arc<FontFamilies> {
    read_fonts().families.clone()
}

/// Indexes the installed fonts now rather than on the first text drawn.
/// Scanning the system's fonts takes a moment, so the editor does this on a
/// background thread at startup.
pub fn load_fonts() {
    fonts();
}

/// Scans the installed fonts again, so fonts installed while Inkframe is
/// running become available. Project font files that couldn't be read are
/// tried again too.
pub fn refresh_installed_fonts() {
    let mut fonts = write_fonts();
    fonts.files.retain(|_, result| result.is_ok());
    let (installed, fallback) = installed_fonts();
    fonts.installed = installed;
    fonts.fallback = fallback;
    fonts.changed();
}

/// Loads a project's font file so its families can be used, returning their
/// names. A file already loaded isn't read again.
pub fn add_font_file(path: &std::path::Path) -> Result<Vec<String>, String> {
    if let Some(result) = read_fonts().files.get(path) {
        return result.clone();
    }
    let mut fonts = write_fonts();
    let result = fonts.load_file(path);
    fonts.files.insert(path.to_path_buf(), result.clone());
    fonts.changed();
    result
}

/// Loads every font file `doc` links to that hasn't been loaded yet, and
/// returns the ones that couldn't be read, with the reason. Cheap when
/// nothing is new, so the editor calls it every frame.
///
/// A file that couldn't be read isn't retried until
/// [`refresh_installed_fonts`]; checking the disk every frame for a missing
/// file would be wasteful.
pub fn sync_project_fonts(
    doc: &Document,
    project_dir: Option<&std::path::Path>,
) -> Vec<(FontFileId, String)> {
    let mut problems = Vec::new();
    for file in &doc.fonts {
        if let Err(problem) = add_font_file(&file.resolve(project_dir)) {
            problems.push((file.id, problem));
        }
    }
    problems
}

/// Bumped whenever the available fonts change, so cached drawings that
/// contain text know to redraw.
pub fn fonts_generation() -> u64 {
    read_fonts().generation
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
    /// Which set of fonts the layout was made with. A font added since
    /// may now provide a family that was missing.
    fonts: u64,
}

impl LayoutKey {
    fn of(text: &Text, fonts: u64) -> Self {
        // f32 can't be hashed directly; its bit pattern can.
        Self {
            content: text.content.clone(),
            font: text.font.clone(),
            size: text.size.to_bits(),
            align: text.align,
            line_spacing: text.line_spacing.to_bits(),
            fonts,
        }
    }
}

fn cache() -> &'static Mutex<HashMap<LayoutKey, Arc<TextLayout>>> {
    static CACHE: OnceLock<Mutex<HashMap<LayoutKey, Arc<TextLayout>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Every font Inkframe can draw with.
///
/// It's shared by the whole program, because laying out text happens deep
/// inside rendering, on several threads at once. Project fonts are kept in a
/// database of their own so they can be searched first: a project's copy of
/// a font wins over a different version installed on the system.
struct Fonts {
    /// The installed fonts, plus the bundled fallback.
    installed: Database,
    /// The font bundled with Inkframe, used when no installed font matches.
    fallback: fontdb::ID,
    /// Fonts from projects' font files.
    project: Database,
    /// Each font file asked for, and what loading it gave.
    files: HashMap<PathBuf, Result<Vec<String>, String>>,
    families: Arc<FontFamilies>,
    generation: u64,
}

impl Fonts {
    fn load_file(&mut self, path: &std::path::Path) -> Result<Vec<String>, String> {
        let name =
            path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let bytes = std::fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("{name} is missing"),
            _ => format!("couldn't read {name}: {e}"),
        })?;
        // Check it's a font before adding it: fontdb quietly skips files it
        // can't parse, which would otherwise look like a successful import.
        let mut probe = Database::new();
        probe.load_font_data(bytes.clone());
        let families = family_names(&probe);
        if families.is_empty() {
            return Err(format!("{name} isn't a font file Inkframe can read"));
        }
        self.project.load_font_data(bytes);
        Ok(families)
    }

    /// Call after any change to the fonts.
    fn changed(&mut self) {
        self.families = Arc::new(FontFamilies {
            project: family_names(&self.project),
            installed: family_names(&self.installed),
        });
        self.generation += 1;
    }

    /// The face closest to `font`: the family by name among the project's
    /// fonts, then the installed fonts, then the system's sans-serif family,
    /// then the bundled font. Bold and italic pick the nearest weight and
    /// slant the family has.
    fn find_face(&self, font: &Font) -> (&Database, fontdb::ID) {
        let query = |families| Query {
            families,
            weight: if font.bold { Weight::BOLD } else { Weight::NORMAL },
            stretch: Stretch::Normal,
            style: if font.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        let by_name = [Family::Name(&font.family)];
        if let Some(id) = self.project.query(&query(&by_name)) {
            return (&self.project, id);
        }
        let or_sans_serif = [Family::Name(&font.family), Family::SansSerif];
        let id = self.installed.query(&query(&or_sans_serif));
        (&self.installed, id.unwrap_or(self.fallback))
    }
}

/// Each family name in a database, sorted case-insensitively.
fn family_names(database: &Database) -> Vec<String> {
    let mut families: Vec<String> =
        database.faces().filter_map(|face| face.families.first().map(|(name, _)| name.clone())).collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.dedup();
    families
}

fn installed_fonts() -> (Database, fontdb::ID) {
    let mut database = Database::new();
    database.load_system_fonts();
    database.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
    // Fonts are added in order, so the bundled one is the last face.
    let fallback = database.faces().last().expect("the bundled font was just added").id;
    (database, fallback)
}

fn fonts() -> &'static RwLock<Fonts> {
    static FONTS: OnceLock<RwLock<Fonts>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let (installed, fallback) = installed_fonts();
        let mut fonts = Fonts {
            installed,
            fallback,
            project: Database::new(),
            files: HashMap::new(),
            families: Arc::default(),
            generation: 0,
        };
        fonts.changed();
        RwLock::new(fonts)
    })
}

// A panic while the lock is held can't leave the fonts half-updated in a
// way that matters (each change is a whole value), so carry on regardless.
fn read_fonts() -> RwLockReadGuard<'static, Fonts> {
    fonts().read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_fonts() -> RwLockWriteGuard<'static, Fonts> {
    fonts().write().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lay_out(text: &Text) -> TextLayout {
    let fonts = read_fonts();
    let (database, face) = fonts.find_face(&text.font);
    let family = database.face(face).and_then(|f| f.families.first()).map(|(name, _)| name.clone());
    let family = family.unwrap_or_default();
    // A font file that can't be parsed draws nothing rather than failing.
    let blank = |family| TextLayout {
        path: None,
        bounds: Rect::from_points(Point::ZERO, Point::ZERO),
        family,
        lines: Vec::new(),
        ascent: 0.0,
        descent: 0.0,
    };
    database
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
    let mut line_layouts = Vec::new();
    let mut line_start = 0;
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
        // Where each cluster (the glyphs made from a run of characters)
        // starts and how wide it is, for placing the text cursor.
        let mut clusters: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
        for (info, position) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let advance = position.x_advance as f32 * scale;
            let cluster = clusters.entry(info.cluster as usize).or_insert((pen_x, 0.0));
            cluster.0 = cluster.0.min(pen_x);
            cluster.1 += advance;
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
            pen_x += advance;
        }
        let carets = caret_positions(line, &clusters, start);
        let range = line_start..line_start + line.len();
        line_layouts.push(LineLayout {
            carets: carets.into_iter().map(|(byte, x)| (range.start + byte, x)).collect(),
            range,
            baseline,
        });
        line_start += line.len() + 1; // the newline
    }

    let bottom = (lines.len() - 1) as f32 * line_height + descent;
    TextLayout {
        path: path.finish(),
        bounds: Rect::from_points(Point::new(left, -ascent), Point::new(right, bottom)),
        family,
        lines: line_layouts,
        ascent,
        descent,
    }
}

/// The x position for a text cursor at every character boundary of `line`.
///
/// Shaping reports which characters each glyph came from as a "cluster":
/// the byte where its run of characters starts. Usually that's one glyph
/// per character, but a ligature like "fi" is one glyph for two, and
/// accented letters can be two glyphs for one character. A boundary at a
/// cluster's start goes at the cluster's edge; one inside a cluster (between
/// the "f" and "i" of a ligature) goes the matching fraction of the way
/// across it.
///
/// In right-to-left scripts the first character is drawn rightmost, so
/// clusters further into the text sit further left, and a cursor before a
/// cluster goes on its right edge.
fn caret_positions(line: &str, clusters: &BTreeMap<usize, (f32, f32)>, start: f32) -> Vec<(usize, f32)> {
    if clusters.is_empty() {
        return vec![(0, start)];
    }
    let first = clusters.values().next().map_or(0.0, |c| c.0);
    let last = clusters.values().next_back().map_or(0.0, |c| c.0);
    let right_to_left = last < first;

    let boundaries = line.char_indices().map(|(i, _)| i).chain(std::iter::once(line.len()));
    boundaries
        .map(|byte| {
            let (&cluster, &(x, width)) = clusters
                .range(..=byte)
                .next_back()
                .unwrap_or_else(|| clusters.iter().next().expect("not empty"));
            let next = clusters.range(cluster + 1..).next().map_or(line.len(), |(&next, _)| next);
            let chars = line[cluster..next].chars().count().max(1) as f32;
            let into = line[cluster..byte.min(next)].chars().count() as f32 / chars;
            let along = if right_to_left { 1.0 - into } else { into };
            (byte, x + width * along)
        })
        .collect()
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
    fn project_font_files_provide_their_families() {
        // egui's bundled "Hack" stands in for a font someone downloaded.
        let dir = std::env::temp_dir().join(format!("inkframe-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Hack-Regular.ttf");
        std::fs::write(&path, epaint_default_fonts::HACK_REGULAR).unwrap();

        let mut doc = Document::default();
        doc.add_font_file(&path, None, vec!["Hack".into()]);
        assert!(sync_project_fonts(&doc, None).is_empty());
        assert!(font_families().project.contains(&"Hack".to_owned()));

        let mut hack = text("Hi");
        hack.font.family = "Hack".into();
        assert_eq!(layout(&hack).family, "Hack");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_font_files_say_why() {
        let dir = std::env::temp_dir().join(format!("inkframe-bad-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let notes = dir.join("notes.ttf");
        std::fs::write(&notes, "definitely not a font").unwrap();
        assert!(add_font_file(&notes).unwrap_err().contains("isn't a font"));
        assert!(add_font_file(&dir.join("gone.otf")).unwrap_err().contains("missing"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn carets_sit_between_letters_on_each_line() {
        let layout = layout(&text("ab\nc"));
        assert_eq!(layout.lines.len(), 2);
        let first = &layout.lines[0];
        assert_eq!(first.range, 0..2);
        let bytes: Vec<usize> = first.carets.iter().map(|c| c.0).collect();
        assert_eq!(bytes, [0, 1, 2]);
        // Each caret is further right than the one before, and the last is
        // at the end of the line.
        assert!(first.carets.windows(2).all(|w| w[1].1 > w[0].1));
        assert_eq!(first.carets[0].1, 0.0);
        // The second line starts after the newline, one line spacing down.
        let second = &layout.lines[1];
        assert_eq!(second.range, 3..4);
        assert!((second.baseline - 120.0).abs() < 1e-3);
    }

    #[test]
    fn empty_lines_still_have_a_caret() {
        let layout = layout(&text(""));
        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].carets, [(0, 0.0)]);
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
