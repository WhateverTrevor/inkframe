//! Drawing color glyphs: emoji made of colored vector layers.
//!
//! Color fonts in the COLR format (version 0, flat colored layers, or
//! version 1, which adds gradients, transforms and blending) describe each
//! colored glyph as a small program: "clip to this shape, fill it with this
//! gradient, then blend that layer over the rest". skrifa reads the program
//! and calls a [`ColorPainter`] for each step; this module carries the steps
//! out with tiny-skia. Because it's all vector shapes, emoji stay as sharp
//! as the text around them at any size.
//!
//! Inkframe bundles Google's Noto Color Emoji in this format, so emoji look
//! the same on every machine. (Apple's color emoji font stores pictures
//! instead, which would turn blurry when large.)

use skrifa::color::{Brush, ColorPainter, ColorStop, CompositeMode, Extend, Transform as FontTransform};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::DrawSettings;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{
    BlendMode, Color as SkiaColor, FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap,
    PixmapMut, PixmapPaint, RadialGradient, Rect, Shader, SpreadMode, SweepGradient, Transform,
};

use inkframe_core::{Color, Point};

use crate::text::GlyphPen;

/// Paints the color glyph `glyph` from `font` into `pixmap`, through
/// `transform` (font units to pixels). `foreground` is the text's fill,
/// which some glyphs use for parts that should match the surrounding text;
/// `tint`, when set, replaces every color (onion skins). Returns false if
/// the font has no color version of the glyph.
pub(crate) fn paint(
    pixmap: &mut PixmapMut,
    font: &FontRef,
    glyph: GlyphId,
    transform: Transform,
    foreground: Color,
    tint: Option<Color>,
) -> bool {
    let Some(color_glyph) = font.color_glyphs().get(glyph) else { return false };
    let palettes = font.color_palettes();
    let palette = palettes.get(0);
    let colors: Vec<Color> = palette
        .map(|p| p.colors().iter().map(|c| Color::rgba(c.red(), c.green(), c.blue(), c.alpha())).collect())
        .unwrap_or_default();
    let mut painter = Painter {
        target: pixmap,
        layers: Vec::new(),
        font,
        colors,
        foreground,
        tint,
        transforms: vec![transform],
        clips: Vec::new(),
    };
    // A glyph that fails to paint partway is left as far as it got.
    let _ = color_glyph.paint(LocationRef::default(), &mut painter);
    true
}

/// Whether `font` has a color version of `glyph`.
pub(crate) fn has_color(font: &FontRef, glyph: GlyphId) -> bool {
    font.color_glyphs().get(glyph).is_some()
}

struct Painter<'a, 'p> {
    target: &'a mut PixmapMut<'p>,
    /// Layers opened by `push_layer`, drawn separately and blended down when
    /// closed, with the blend mode each was opened with.
    layers: Vec<(Pixmap, CompositeMode)>,
    font: &'a FontRef<'a>,
    /// The font's first palette, as straight (not premultiplied) colors.
    colors: Vec<Color>,
    foreground: Color,
    tint: Option<Color>,
    /// Font units to pixels, with each `push_transform` multiplied in.
    transforms: Vec<Transform>,
    /// The clip so far, as a coverage mask over the target; each push
    /// narrows it further.
    clips: Vec<Mask>,
}

impl Painter<'_, '_> {
    fn transform(&self) -> Transform {
        *self.transforms.last().expect("the base transform is never popped")
    }

    fn size(&self) -> (u32, u32) {
        (self.target.width(), self.target.height())
    }

    /// A color from the palette, with an extra alpha multiplied in. Index
    /// 0xFFFF means "the text's own color".
    fn color(&self, index: u16, alpha: f32) -> SkiaColor {
        let c = if index == 0xFFFF {
            self.foreground
        } else {
            self.colors.get(index as usize).copied().unwrap_or(Color::BLACK)
        };
        let c = self.tint.map_or(c, |tint| tint.with_alpha(c.a));
        let a = (c.a as f32 * alpha.clamp(0.0, 1.0)) as u8;
        SkiaColor::from_rgba8(c.r, c.g, c.b, a)
    }

    fn stops(&self, stops: &[ColorStop]) -> Vec<GradientStop> {
        stops.iter().map(|s| GradientStop::new(s.offset, self.color(s.palette_index, s.alpha))).collect()
    }

    fn shader(&self, brush: &Brush) -> Option<Shader<'static>> {
        let spread = |extend: &Extend| match extend {
            Extend::Repeat => SpreadMode::Repeat,
            Extend::Reflect => SpreadMode::Reflect,
            _ => SpreadMode::Pad,
        };
        let point = |p: skrifa::raw::types::Point<f32>| tiny_skia::Point::from_xy(p.x, p.y);
        let transform = self.transform();
        match brush {
            Brush::Solid { palette_index, alpha } => {
                Some(Shader::SolidColor(self.color(*palette_index, *alpha)))
            }
            // A gradient with a single stop is that stop's color everywhere.
            Brush::LinearGradient { color_stops: [stop], .. }
            | Brush::RadialGradient { color_stops: [stop], .. }
            | Brush::SweepGradient { color_stops: [stop], .. } => {
                Some(Shader::SolidColor(self.color(stop.palette_index, stop.alpha)))
            }
            Brush::LinearGradient { p0, p1, color_stops, extend } => LinearGradient::new(
                point(*p0),
                point(*p1),
                self.stops(color_stops),
                spread(extend),
                transform,
            ),
            // tiny-skia can't draw negative radii, which stop normalization
            // can produce; trimming them to zero is close enough for emoji.
            Brush::RadialGradient { c0, r0, c1, r1, color_stops, extend } => RadialGradient::new(
                point(*c0),
                r0.max(0.0),
                point(*c1),
                r1.max(0.0),
                self.stops(color_stops),
                spread(extend),
                transform,
            ),
            Brush::SweepGradient { c0, start_angle, end_angle, color_stops, extend } => SweepGradient::new(
                point(*c0),
                *start_angle,
                *end_angle,
                self.stops(color_stops),
                spread(extend),
                transform,
            ),
        }
    }

    /// Narrows the clip to `path` (in font units).
    fn push_clip_path(&mut self, path: Option<tiny_skia::Path>) {
        let (width, height) = self.size();
        let mut mask = Mask::new(width, height).expect("the target has a size");
        if let Some(path) = path {
            mask.fill_path(&path, FillRule::Winding, true, self.transform());
        }
        // Inside an earlier clip too: keep only what both cover.
        if let Some(outer) = self.clips.last() {
            for (inner, outer) in mask.data_mut().iter_mut().zip(outer.data()) {
                *inner = ((*inner as u16 * *outer as u16) / 255) as u8;
            }
        }
        self.clips.push(mask);
    }

    /// Draws with `draw` onto the open layer, or the target if none is open.
    fn with_target(&mut self, draw: impl FnOnce(&mut PixmapMut)) {
        match self.layers.last_mut() {
            Some((layer, _)) => draw(&mut layer.as_mut()),
            None => draw(self.target),
        }
    }
}

impl ColorPainter for Painter<'_, '_> {
    fn push_transform(&mut self, transform: FontTransform) {
        // The font's matrix maps (x, y) to (xx·x + xy·y + dx, yx·x + yy·y + dy).
        let t = Transform::from_row(
            transform.xx,
            transform.yx,
            transform.xy,
            transform.yy,
            transform.dx,
            transform.dy,
        );
        let combined = self.transform().pre_concat(t);
        self.transforms.push(combined);
    }

    fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let mut path = PathBuilder::new();
        if let Some(outline) = self.font.outline_glyphs().get(glyph_id) {
            let mut pen = GlyphPen { path: &mut path, origin: Point::ZERO, scale: 1.0, flip: false };
            let _ = outline.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen);
        }
        self.push_clip_path(path.finish());
    }

    fn push_clip_box(&mut self, clip_box: skrifa::raw::types::BoundingBox<f32>) {
        let rect = Rect::from_ltrb(clip_box.x_min, clip_box.y_min, clip_box.x_max, clip_box.y_max);
        self.push_clip_path(rect.map(PathBuilder::from_rect));
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn fill(&mut self, brush: Brush<'_>) {
        let Some(shader) = self.shader(&brush) else { return };
        let (width, height) = self.size();
        let everything =
            Rect::from_xywh(0.0, 0.0, width as f32, height as f32).expect("the target has a size");
        let paint = Paint { shader, anti_alias: true, ..Paint::default() };
        // Fill the whole target through the clip mask: only the clipped
        // shape gets paint.
        let clip = self.clips.last().cloned();
        self.with_target(|target| target.fill_rect(everything, &paint, Transform::identity(), clip.as_ref()));
    }

    fn push_layer(&mut self, composite_mode: CompositeMode) {
        let (width, height) = self.size();
        self.layers.push((Pixmap::new(width, height).expect("the target has a size"), composite_mode));
    }

    fn pop_layer(&mut self) {
        let Some((layer, mode)) = self.layers.pop() else { return };
        let paint = PixmapPaint { blend_mode: blend_mode(mode), ..PixmapPaint::default() };
        self.with_target(|target| {
            target.draw_pixmap(0, 0, layer.as_ref(), &paint, Transform::identity(), None)
        });
    }
}

/// COLR's blend modes, which are Porter-Duff and the usual separable and
/// non-separable modes, mapped onto tiny-skia's equivalents.
fn blend_mode(mode: CompositeMode) -> BlendMode {
    use CompositeMode as C;
    match mode {
        C::Clear => BlendMode::Clear,
        C::Src => BlendMode::Source,
        C::Dest => BlendMode::Destination,
        C::DestOver => BlendMode::DestinationOver,
        C::SrcIn => BlendMode::SourceIn,
        C::DestIn => BlendMode::DestinationIn,
        C::SrcOut => BlendMode::SourceOut,
        C::DestOut => BlendMode::DestinationOut,
        C::SrcAtop => BlendMode::SourceAtop,
        C::DestAtop => BlendMode::DestinationAtop,
        C::Xor => BlendMode::Xor,
        C::Plus => BlendMode::Plus,
        C::Screen => BlendMode::Screen,
        C::Overlay => BlendMode::Overlay,
        C::Darken => BlendMode::Darken,
        C::Lighten => BlendMode::Lighten,
        C::ColorDodge => BlendMode::ColorDodge,
        C::ColorBurn => BlendMode::ColorBurn,
        C::HardLight => BlendMode::HardLight,
        C::SoftLight => BlendMode::SoftLight,
        C::Difference => BlendMode::Difference,
        C::Exclusion => BlendMode::Exclusion,
        C::Multiply => BlendMode::Multiply,
        C::HslHue => BlendMode::Hue,
        C::HslSaturation => BlendMode::Saturation,
        C::HslColor => BlendMode::Color,
        C::HslLuminosity => BlendMode::Luminosity,
        _ => BlendMode::SourceOver,
    }
}
