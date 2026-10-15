//! Rasterizes Inkframe documents with [tiny-skia].
//!
//! The editor's stage and every export path go through [`render_frame_into`],
//! so what you see while editing is pixel-for-pixel what gets exported.
//!
//! tiny-skia is a CPU rasterizer. That's slower than the GPU, but it's
//! deterministic across machines and has no driver quirks, which matters
//! more for final-quality output. A GPU preview path can be added later
//! without touching the export code.
//!
//! [tiny-skia]: https://github.com/linebender/tiny-skia

mod color_glyph;
mod images;
mod path;
mod text;

use inkframe_core::{Affine, Color, Document, Element, Layer, LayerId, Point, Shape, SymbolId, TimelineId};
use tiny_skia::{
    FillRule, FilterQuality, IntRect, LineCap, LineJoin, Mask, MaskType, Paint, PathBuilder, Pattern, Pixmap,
    PixmapMut, PixmapPaint, SpreadMode, Transform,
};

pub use images::{IMAGE_EXTENSIONS, ImageLibrary, image_size};
pub use path::build_path;
pub use text::{
    FontFamilies, TextLayout, add_font_file, font_families, fonts_generation, layout as layout_text,
    load_fonts, refresh_installed_fonts, sync_project_fonts,
};

use images::Mipmaps;

/// Settings for one render pass.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions<'a> {
    /// Output pixels per stage pixel. 1.0 renders at the document's size,
    /// 2.0 at double resolution (for Retina displays or high-res export).
    pub scale: f32,
    /// Fill the background with the document's stage color first. Turn this
    /// off to get a transparent background for alpha-capable exports.
    pub draw_background: bool,
    /// Paint every shape in this color instead of its own (keeping each
    /// shape's alpha). Used to tint onion-skin ghosts.
    pub tint: Option<Color>,
    /// Extra opacity multiplied into every layer (0.0–1.0).
    pub opacity: f32,
    /// The document's images, decoded. Without it (or for a file that
    /// couldn't be read) each image is drawn as a crossed-out placeholder.
    pub images: Option<&'a ImageLibrary>,
    /// Draw motion guide layers. They're shown while editing, but they're
    /// scaffolding, not artwork, so exports leave them out.
    pub show_guides: bool,
    /// A mask layer to draw faintly, so it can be seen while it's being
    /// positioned. Masks are otherwise never drawn, only used.
    pub show_mask: Option<LayerId>,
}

impl Default for RenderOptions<'_> {
    fn default() -> Self {
        Self {
            scale: 1.0,
            draw_background: true,
            tint: None,
            opacity: 1.0,
            images: None,
            show_guides: false,
            show_mask: None,
        }
    }
}

/// Creates a pixmap sized for the document at `options.scale` and renders
/// `frame` into it. Returns `None` if the size is zero or absurdly large.
pub fn render_frame(doc: &Document, frame: u32, options: &RenderOptions) -> Option<Pixmap> {
    let (width, height) = output_size(doc, options.scale);
    let mut pixmap = Pixmap::new(width, height)?;
    render_frame_into(&mut pixmap, doc, frame, options);
    Some(pixmap)
}

/// The pixel size of a render at `scale`, rounded to whole pixels and never
/// smaller than 1×1.
pub fn output_size(doc: &Document, scale: f32) -> (u32, u32) {
    let px = |v: u32| ((v as f32 * scale).round() as u32).max(1);
    (px(doc.width), px(doc.height))
}

/// Draws `frame` on top of whatever is already in `pixmap`.
///
/// Drawing on top (instead of clearing first) is what lets the editor stack
/// onion-skin frames under the current one in a single buffer.
pub fn render_frame_into(pixmap: &mut Pixmap, doc: &Document, frame: u32, options: &RenderOptions) {
    render_timeline_into(pixmap, doc, TimelineId::Main, frame, Affine::IDENTITY, None, options);
}

/// Like [`render_frame_into`], for any timeline, placed anywhere.
///
/// The editor uses this while a symbol is open: `to_stage` maps the
/// timeline's coordinates onto the stage (for a symbol edited in place,
/// that's its instance's transform), and `frame` is in the timeline's own
/// frames. `skip` leaves one layer out; the editor uses it to fade the
/// surrounding scene without also drawing a faded copy of the instance
/// being edited.
pub fn render_timeline_into(
    pixmap: &mut Pixmap,
    doc: &Document,
    timeline: TimelineId,
    frame: u32,
    to_stage: Affine,
    skip: Option<LayerId>,
    options: &RenderOptions,
) {
    if options.draw_background {
        pixmap.fill(to_skia_color(doc.background));
    }

    let to_pixels = Affine::scale(options.scale, options.scale).pre_concat(to_stage);
    let style = Style {
        tint: options.tint,
        images: options.images,
        show_guides: options.show_guides,
        show_mask: options.show_mask,
    };
    let layers = doc.layers_of(timeline);
    in_bands(pixmap, |band, shift| {
        let to_band = shift.pre_concat(to_pixels);
        render_layers(band, doc, layers, frame, to_band, options.opacity, style, skip, 0);
    });
}

/// Rows per band in [`in_bands`].
const BAND_HEIGHT: u32 = 128;

/// Runs `draw` on horizontal strips of `pixmap` in parallel, one strip per
/// CPU core at a time.
///
/// Each strip is a view straight into the pixmap's rows (no copying), and
/// `draw` receives the transform that moves the image's coordinates up to
/// that strip's top. tiny-skia skips anything outside the strip, so every
/// core works on its own rows and the strips meet seamlessly.
///
/// tiny-skia clips each shape's outline at the strip's top and bottom, which
/// can shift the anti-aliasing of a pixel on that outline by a few levels
/// compared with drawing in one pass. Nothing inside a shape changes, and
/// it's invisible, but it means the split must never vary: strips are a
/// fixed height rather than "one per core", so an export comes out
/// byte-identical whichever machine renders it.
fn in_bands(pixmap: &mut Pixmap, draw: impl Fn(&mut PixmapMut, Affine) + Sync) {
    let (width, height) = (pixmap.width(), pixmap.height());
    let row_bytes = width as usize * 4;
    let mut bands: Vec<(u32, &mut [u8])> = pixmap
        .data_mut()
        .chunks_mut(row_bytes * BAND_HEIGHT as usize)
        .enumerate()
        .map(|(i, rows)| (i as u32 * BAND_HEIGHT, rows))
        .collect();

    let draw_band = |top: u32, rows: &mut [u8]| {
        let band_height = (rows.len() / row_bytes) as u32;
        let mut band =
            PixmapMut::from_bytes(rows, width, band_height).expect("band is a whole number of rows");
        draw(&mut band, Affine::translate(Point::new(0.0, -(top as f32))));
    };
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(bands.len());
    if workers <= 1 || height < 2 * BAND_HEIGHT {
        // Small images (thumbnails, tests) aren't worth starting threads for.
        bands.iter_mut().for_each(|(top, rows)| draw_band(*top, rows));
        return;
    }
    // Deal the bands out round-robin, so each worker gets a mix of the top,
    // middle and bottom of the image, where the busy areas tend to differ.
    let mut shares: Vec<Vec<(u32, &mut [u8])>> = (0..workers).map(|_| Vec::new()).collect();
    for (i, band) in bands.into_iter().enumerate() {
        shares[i % workers].push(band);
    }
    std::thread::scope(|scope| {
        for share in shares {
            let draw_band = &draw_band;
            scope.spawn(move || share.into_iter().for_each(|(top, rows)| draw_band(top, rows)));
        }
    });
}

/// Renders one frame of a symbol, scaled to fit a `size`×`size` square, for
/// library thumbnails. Returns `None` for symbols with nothing to draw.
pub fn render_symbol_thumbnail(
    doc: &Document,
    images: Option<&ImageLibrary>,
    symbol: SymbolId,
    frame: u32,
    size: u32,
) -> Option<Pixmap> {
    let symbol = doc.symbol(symbol)?;
    let bounds = symbol
        .layers
        .iter()
        .filter(|l| l.is_shown_at(frame))
        .filter_map(|l| {
            let local = doc.layer_bounds(l, frame)?;
            let to_symbol = doc.pose(l, frame).to_affine();
            inkframe_core::Rect::bounding(local.corners().map(|c| to_symbol.apply(c)))
        })
        .reduce(inkframe_core::Rect::union)?;

    // Fit the drawing into the square with a little margin, centered.
    let padding = 0.9;
    let scale = padding * size as f32 / bounds.width().max(bounds.height()).max(1e-3);
    let center = bounds.center();
    let half = size as f32 / 2.0;
    let fit = Affine::translate(Point::new(half, half))
        .pre_concat(Affine::scale(scale, scale))
        .pre_concat(Affine::translate(-center));

    let mut pixmap = Pixmap::new(size, size)?;
    let style = Style { tint: None, images, show_guides: true, show_mask: None };
    render_layers(&mut pixmap.as_mut(), doc, &symbol.layers, frame, fit, 1.0, style, None, 0);
    Some(pixmap)
}

/// Settings that apply to everything drawn in one pass, however deeply
/// nested inside symbols.
#[derive(Clone, Copy)]
struct Style<'a> {
    tint: Option<Color>,
    images: Option<&'a ImageLibrary>,
    show_guides: bool,
    show_mask: Option<LayerId>,
}

/// How strongly a mask layer is drawn when it's shown for positioning.
const MASK_PREVIEW_OPACITY: f32 = 0.35;

/// Draws a stack of layers (the document's, or a symbol's) at `frame`.
///
/// `parent` maps the layers' coordinate space to pixels: the stage scale at
/// the top level, and the instance's full transform inside a symbol. Each
/// nested symbol calls back in here with its own frame and transform, so
/// symbols can contain symbols to any depth (up to a safety limit).
#[allow(clippy::too_many_arguments)]
fn render_layers(
    pixmap: &mut PixmapMut,
    doc: &Document,
    layers: &[Layer],
    frame: u32,
    parent: Affine,
    opacity: f32,
    style: Style<'_>,
    skip: Option<LayerId>,
    depth: usize,
) {
    // Layers are stored bottom-first, so a plain loop paints back to front.
    // Guides are only drawn while editing, and masks only when one is being
    // shown for positioning; otherwise a mask just shapes what it masks.
    let drawn = |l: &&Layer| {
        l.is_shown_at(frame)
            && Some(l.id) != skip
            && (style.show_guides || !l.guide)
            && (!l.mask || style.show_mask == Some(l.id))
    };
    for layer in layers.iter().filter(drawn) {
        let transform = doc.pose(layer, frame);
        let preview = if layer.mask { MASK_PREVIEW_OPACITY } else { 1.0 };
        let opacity = (transform.opacity * opacity * preview).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        let matrix = parent.pre_concat(transform.to_affine());
        let mask = doc.mask_for(layer, frame).filter(|_| !layer.mask);

        if opacity >= 1.0 && mask.is_none() {
            draw_layer(pixmap, doc, layer, frame, matrix, style, depth);
            continue;
        }

        // A translucent layer must fade as a single unit. If we simply drew
        // each shape at reduced opacity, overlapping shapes inside the layer
        // would show through each other. So we draw the layer fully opaque
        // into a scratch buffer, then blend that buffer down in one go.
        // Symbols nest, so each translucent layer needs a buffer of its own.
        // A masked layer goes through a scratch buffer too, so the mask can
        // be cut out of it before it's blended down.
        //
        // The buffer only needs to cover the pixels the layer can touch.
        // A whole-image buffer would cost a full-screen allocation and blend
        // per translucent layer (about 20 ms each at Retina sizes), which
        // made onion skinning, where every ghost is translucent, crawl.
        let Some(region) = covered_pixels(doc, layer, frame, matrix, pixmap.width(), pixmap.height()) else {
            continue; // Empty, or entirely outside the image.
        };
        let mut scratch = Pixmap::new(region.width(), region.height()).expect("region is non-empty");
        // Shifting by whole pixels lines the scratch buffer up exactly with
        // the image beneath it, so the result is identical to drawing in place.
        let offset = Affine::translate(Point::new(-region.x() as f32, -region.y() as f32));
        draw_layer(&mut scratch.as_mut(), doc, layer, frame, offset.pre_concat(matrix), style, depth);
        if let Some(mask) = mask {
            let stencil = mask_stencil(doc, mask, frame, offset.pre_concat(parent), &scratch, style, depth);
            scratch.apply_mask(&stencil);
        }
        let paint = PixmapPaint { opacity, ..PixmapPaint::default() };
        pixmap.draw_pixmap(region.x(), region.y(), scratch.as_ref(), &paint, Transform::identity(), None);
    }
}

/// A mask layer's drawing as a stencil the size of `scratch`: how much of
/// each pixel shows through, from its coverage. `parent` maps the layers'
/// coordinates onto `scratch`.
///
/// The mask is drawn untinted (an onion skin's tint shouldn't change what
/// shows through it), and its own opacity scales the whole stencil, so a
/// mask faded to 50% lets half of the masked layer through.
fn mask_stencil(
    doc: &Document,
    mask: &Layer,
    frame: u32,
    parent: Affine,
    scratch: &Pixmap,
    style: Style<'_>,
    depth: usize,
) -> Mask {
    let pose = doc.pose(mask, frame);
    let mut drawing =
        Pixmap::new(scratch.width(), scratch.height()).expect("same size as the scratch buffer");
    let untinted = Style { tint: None, ..style };
    draw_layer(&mut drawing.as_mut(), doc, mask, frame, parent.pre_concat(pose.to_affine()), untinted, depth);
    let mut stencil = Mask::from_pixmap(drawing.as_ref(), MaskType::Alpha);
    let opacity = pose.opacity.clamp(0.0, 1.0);
    if opacity < 1.0 {
        for coverage in stencil.data_mut() {
            *coverage = (*coverage as f32 * opacity).round() as u8;
        }
    }
    stencil
}

/// The pixels a layer can paint when drawn with `matrix`, clipped to a
/// `width`×`height` image, or `None` if that's nothing.
fn covered_pixels(
    doc: &Document,
    layer: &Layer,
    frame: u32,
    matrix: Affine,
    width: u32,
    height: u32,
) -> Option<IntRect> {
    let local = doc.layer_bounds(layer, frame)?;
    let bounds = inkframe_core::Rect::bounding(local.corners().map(|c| matrix.apply(c)))?;
    // Anti-aliasing can tint the pixel just past a shape's edge, so round
    // outward and allow one more.
    let left = (bounds.min.x.floor() as i32 - 1).max(0);
    let top = (bounds.min.y.floor() as i32 - 1).max(0);
    let right = (bounds.max.x.ceil() as i32 + 1).min(width as i32);
    let bottom = (bounds.max.y.ceil() as i32 + 1).min(height as i32);
    IntRect::from_ltrb(left, top, right, bottom)
}

/// Draws a layer's own shapes, then its symbol instance if it has one.
fn draw_layer(
    pixmap: &mut PixmapMut,
    doc: &Document,
    layer: &Layer,
    frame: u32,
    matrix: Affine,
    style: Style<'_>,
    depth: usize,
) {
    let skia_matrix = to_skia_transform(matrix);
    // Skip shapes that can't reach this image. tiny-skia would discard them
    // too, but only after building their outlines, and with the image split
    // into bands every band would build every shape's outline.
    let image = inkframe_core::Rect::from_points(
        Point::ZERO,
        Point::new(pixmap.width() as f32, pixmap.height() as f32),
    );
    let visible = |element: &Element| {
        let corners = element.bounds().corners().map(|c| matrix.apply(c));
        inkframe_core::Rect::bounding(corners).is_some_and(|b| overlaps(b, image))
    };
    for element in layer.elements.iter().filter(|e| visible(e)) {
        draw_element(pixmap, element, skia_matrix, style);
    }
    // `instance_view` stops at the nesting limit, so a malformed file can't
    // recurse forever.
    if let Some((symbol, symbol_frame)) = doc.instance_view(layer, frame, depth) {
        render_layers(pixmap, doc, &symbol.layers, symbol_frame, matrix, 1.0, style, None, depth + 1);
    }
}

/// Whether two rectangles share any area, allowing a pixel's slack for
/// anti-aliasing just past a shape's edge.
fn overlaps(a: inkframe_core::Rect, b: inkframe_core::Rect) -> bool {
    a.min.x <= b.max.x + 1.0
        && a.max.x >= b.min.x - 1.0
        && a.min.y <= b.max.y + 1.0
        && a.max.y >= b.min.y - 1.0
}

fn draw_element(pixmap: &mut PixmapMut, element: &Element, matrix: Transform, style: Style<'_>) {
    if let Shape::Image { image, width, height } = element.shape {
        let pixels = style.images.and_then(|library| library.get(image));
        draw_image(pixmap, pixels, width, height, matrix);
        return;
    }
    let tint = style.tint;
    let layout = match &element.shape {
        Shape::Text(text) => Some(text::layout(text)),
        _ => None,
    };
    let path = match &layout {
        Some(layout) => layout.path.clone(),
        None => build_path(&element.shape),
    };
    // Emoji and other color glyphs paint themselves, after the rest of the
    // text, keeping their own colors.
    if let Some(layout) = &layout {
        let foreground = element.style.fill.unwrap_or(Color::BLACK);
        text::paint_color_glyphs(pixmap, layout, matrix, foreground, tint);
    }
    let Some(path) = path else {
        return; // Degenerate geometry, e.g. a zero-size rectangle.
    };
    let paint_for = |color: Color| {
        let color = tint.map_or(color, |t| t.with_alpha(color.a));
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint
    };

    if let Some(fill) = element.style.fill {
        pixmap.fill_path(&path, &paint_for(fill), FillRule::Winding, matrix, None);
    }
    if let Some(stroke) = element.style.stroke.filter(|s| s.width > 0.0) {
        let style = tiny_skia::Stroke {
            width: stroke.width,
            // Round joins and caps suit hand-drawn strokes and avoid the
            // spiky miter artifacts sharp pencil turns would otherwise get.
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(&path, &paint_for(stroke.color), &style, matrix, None);
    }
}

/// Draws a picture stretched over the `width`×`height` rectangle centered on
/// the origin, or a crossed-out placeholder if its pixels aren't available.
///
/// The rectangle is filled with the picture as a pattern rather than copied
/// with `draw_pixmap`, which doesn't anti-alias: a rotated image would get
/// jagged edges. Onion-skin tinting doesn't apply; a ghost of a picture is
/// drawn as the picture, faded.
fn draw_image(pixmap: &mut PixmapMut, pixels: Option<&Mipmaps>, width: f32, height: f32, matrix: Transform) {
    let Some(rect) = tiny_skia::Rect::from_xywh(-width / 2.0, -height / 2.0, width, height) else {
        return; // Zero-sized.
    };
    let outline = PathBuilder::from_rect(rect);

    let Some(mips) = pixels else {
        let mut paint = Paint { anti_alias: true, ..Paint::default() };
        paint.set_color_rgba8(160, 160, 160, 90);
        pixmap.fill_path(&outline, &paint, FillRule::Winding, matrix, None);
        let mut cross = PathBuilder::new();
        cross.move_to(rect.left(), rect.top());
        cross.line_to(rect.right(), rect.bottom());
        cross.move_to(rect.right(), rect.top());
        cross.line_to(rect.left(), rect.bottom());
        cross.push_rect(rect);
        paint.set_color_rgba8(110, 110, 110, 255);
        // Width 0 is a hairline: one pixel wide however the image is scaled.
        let hairline = tiny_skia::Stroke { width: 0.0, ..tiny_skia::Stroke::default() };
        if let Some(cross) = cross.finish() {
            pixmap.stroke_path(&cross, &paint, &hairline, matrix, None);
        }
        return;
    };

    // How many output pixels each of the image's own pixels covers: the
    // stretch to `width`×`height`, times however much the layer and stage
    // scale it (the square root of the matrix's area scale).
    let stretch = (width / mips.width() as f32, height / mips.height() as f32);
    let area = (matrix.sx * matrix.sy - matrix.kx * matrix.ky).abs();
    let scale = area.sqrt() * (stretch.0 * stretch.1).abs().sqrt();
    let (level, factor) = mips.level_for(scale);

    // The pattern's transform places the chosen copy's pixels on the
    // rectangle; `factor` makes up for it being smaller than the original.
    let to_rect = Transform::from_translate(-width / 2.0, -height / 2.0)
        .pre_scale(stretch.0 * factor, stretch.1 * factor);
    // Bilinear is exact at 1:1 and, with a suitable copy to start from,
    // smooth when shrinking. Bicubic gives smoother edges when enlarging.
    let quality = if scale > 1.0 { FilterQuality::Bicubic } else { FilterQuality::Bilinear };
    let shader = Pattern::new(level.as_ref(), SpreadMode::Pad, quality, 1.0, to_rect);
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    pixmap.fill_path(&outline, &paint, FillRule::Winding, matrix, None);
}

fn to_skia_transform(m: Affine) -> Transform {
    Transform::from_row(m.a, m.b, m.c, m.d, m.e, m.f)
}

fn to_skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Point, Shape, Style, Transform as Pose};

    fn red_square_doc() -> Document {
        let mut doc = Document::default();
        doc.width = 100;
        doc.height = 100;
        doc.background = Color::WHITE;
        let square = Element {
            shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::rgb(255, 0, 0)), stroke: None },
        };
        let id = doc.add_layer("Square", vec![square], 0, Pose::at(Point::new(20.0, 50.0)));
        doc.layer_mut(id).unwrap().track.set_pose(10, Pose::at(Point::new(80.0, 50.0)));
        doc
    }

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = pixmap.pixel(x, y).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn renders_the_tweened_position() {
        let doc = red_square_doc();
        let start = render_frame(&doc, 0, &RenderOptions::default()).unwrap();
        let end = render_frame(&doc, 10, &RenderOptions::default()).unwrap();
        assert_eq!(pixel(&start, 20, 50), [255, 0, 0, 255]);
        assert_eq!(pixel(&start, 80, 50), [255, 255, 255, 255]);
        assert_eq!(pixel(&end, 80, 50), [255, 0, 0, 255]);
    }

    #[test]
    fn transparent_background_leaves_alpha_zero() {
        let doc = red_square_doc();
        let options = RenderOptions { draw_background: false, ..RenderOptions::default() };
        let pixmap = render_frame(&doc, 0, &options).unwrap();
        assert_eq!(pixel(&pixmap, 90, 10)[3], 0);
        assert_eq!(pixel(&pixmap, 20, 50)[3], 255);
    }

    #[test]
    fn scale_changes_output_size() {
        let doc = red_square_doc();
        let options = RenderOptions { scale: 2.0, ..RenderOptions::default() };
        let pixmap = render_frame(&doc, 0, &options).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (200, 200));
        assert_eq!(pixel(&pixmap, 40, 100), [255, 0, 0, 255]);
    }

    #[test]
    fn converting_to_a_symbol_renders_identically() {
        let mut doc = red_square_doc();
        let id = doc.layers[0].id;
        // Some rotation and a fade, to exercise the full transform chain.
        doc.layer_mut(id).unwrap().track.rotation.set(10, 45.0);
        doc.layer_mut(id).unwrap().track.opacity.set(10, 0.5);
        let before: Vec<Vec<u8>> =
            (0..=10).map(|f| render_frame(&doc, f, &RenderOptions::default()).unwrap().take()).collect();

        doc.convert_layer_to_symbol(id, 5).unwrap();
        for (frame, expected) in before.iter().enumerate() {
            let actual = render_frame(&doc, frame as u32, &RenderOptions::default()).unwrap().take();
            // Allow off-by-one rounding from the extra matrix multiply.
            let worst = expected.iter().zip(&actual).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(worst <= 2, "frame {frame} differs by up to {worst}");
        }
    }

    #[test]
    fn thumbnails_fit_the_symbol() {
        let mut doc = red_square_doc();
        let id = doc.layers[0].id;
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        let thumb = render_symbol_thumbnail(&doc, None, symbol, 0, 64).unwrap();
        // The red square fills the middle of the thumbnail.
        assert_eq!(pixel(&thumb, 32, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(&thumb, 0, 0)[3], 0);
    }

    #[test]
    fn layers_are_hidden_outside_their_span() {
        let mut doc = red_square_doc();
        doc.layers[0].set_start(5);
        let before = render_frame(&doc, 0, &RenderOptions::default()).unwrap();
        assert_eq!(pixel(&before, 20, 50), [255, 255, 255, 255]);
    }

    #[test]
    fn translucent_layers_match_blending_a_full_image() {
        // A rotated, stroked, half-transparent square: the case where the
        // scratch buffer covers only part of the image.
        let mut doc = red_square_doc();
        doc.layers[0].elements[0].style.stroke =
            Some(inkframe_core::Stroke { color: Color::BLACK, width: 3.0 });
        doc.layers[0].track.rotation.set(0, 30.0);
        let mut opaque = doc.clone();
        doc.layers[0].track.opacity.set(0, 0.5);
        let options = RenderOptions::default();
        let actual = render_frame(&doc, 0, &options).unwrap();

        // The straightforward way: draw the layer opaque on its own, as
        // big as the whole image, then blend it over the background at 50%.
        opaque.background = Color::TRANSPARENT;
        let layer = render_frame(&opaque, 0, &options).unwrap();
        let mut expected = Pixmap::new(100, 100).unwrap();
        expected.fill(to_skia_color(doc.background));
        let half = PixmapPaint { opacity: 0.5, ..PixmapPaint::default() };
        expected.draw_pixmap(0, 0, layer.as_ref(), &half, Transform::identity(), None);

        assert!(actual.data() == expected.data(), "the bounded scratch buffer changed the result");
    }

    #[test]
    fn bands_only_touch_anti_aliased_edges() {
        // Shapes straddling band edges, rendered in bands and in one pass.
        let mut doc = Document::default();
        doc.width = 300;
        doc.height = 4 * BAND_HEIGHT + 37; // a short last band, too
        for i in 0..12 {
            let circle = Element {
                shape: Shape::Ellipse { radius_x: 40.0, radius_y: 70.0 },
                style: Style {
                    fill: Some(Color::rgba(200, 40 * (i % 6) as u8, 90, 200)),
                    stroke: Some(inkframe_core::Stroke { color: Color::BLACK, width: 5.0 }),
                },
            };
            let mut pose = Pose::at(Point::new(25.0 * i as f32 + 20.0, 45.0 * i as f32 + 30.0));
            pose.rotation = 17.0 * i as f32;
            pose.opacity = if i % 2 == 0 { 1.0 } else { 0.6 };
            doc.add_layer(format!("Circle {i}"), vec![circle], 0, pose);
        }
        let options = RenderOptions { scale: 1.3, ..RenderOptions::default() };
        let banded = render_frame(&doc, 0, &options).unwrap();

        let mut whole = Pixmap::new(banded.width(), banded.height()).unwrap();
        whole.fill(to_skia_color(doc.background));
        let to_pixels = Affine::scale(1.3, 1.3);
        let style = super::Style { tint: None, images: None, show_guides: false, show_mask: None };
        render_layers(&mut whole.as_mut(), &doc, &doc.layers, 0, to_pixels, 1.0, style, None, 0);

        // tiny-skia clips each shape's outline where a band starts and ends,
        // which can nudge the anti-aliasing of pixels on that outline. That's
        // all that may differ: no pixel inside a shape or in open background
        // (one that matches its neighbours) may change, and few may change.
        let pixel = |image: &Pixmap, p: usize| image.data()[p * 4..p * 4 + 4].to_vec();
        let width = whole.width() as usize;
        let count = whole.data().len() / 4;
        let on_an_edge = |p: usize| {
            [p.wrapping_sub(1), p + 1, p.wrapping_sub(width), p + width]
                .into_iter()
                .any(|q| q < count && pixel(&whole, q) != pixel(&whole, p))
        };
        let changed: Vec<usize> = (0..count).filter(|&p| pixel(&banded, p) != pixel(&whole, p)).collect();
        assert!(changed.iter().all(|&p| on_an_edge(p)), "a pixel away from any edge changed");
        assert!(changed.len() * 100 < count, "{} of {count} pixels changed", changed.len());

        // And the split never varies, so the result is the same every time.
        assert!(render_frame(&doc, 0, &options).unwrap().data() == banded.data());
    }

    #[test]
    fn images_are_drawn_from_their_files() {
        // A 2×1 picture, red on the left and blue on the right, stretched
        // over a 40×20 rectangle in the middle of the stage.
        let dir = std::env::temp_dir().join(format!("inkframe-render-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("flag.png");
        let mut picture = image::RgbaImage::new(2, 1);
        picture.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        picture.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
        picture.save(&path).unwrap();

        let mut doc = red_square_doc();
        doc.layers.clear();
        let image = doc.add_image(&path, None, (2, 1));
        let element = Element {
            shape: Shape::Image { image, width: 40.0, height: 20.0 },
            style: Style { fill: None, stroke: None },
        };
        doc.add_layer("Flag", vec![element], 0, Pose::at(Point::new(50.0, 50.0)));

        let library = ImageLibrary::load(&doc, None);
        let options = RenderOptions { images: Some(&library), ..RenderOptions::default() };
        let pixmap = render_frame(&doc, 0, &options).unwrap();
        assert_eq!(pixel(&pixmap, 34, 50), [255, 0, 0, 255]);
        assert_eq!(pixel(&pixmap, 66, 50), [0, 0, 255, 255]);
        assert_eq!(pixel(&pixmap, 50, 30), [255, 255, 255, 255], "nothing outside the rectangle");

        // Without the pixels, a placeholder marks where the image goes.
        std::fs::remove_dir_all(&dir).unwrap();
        let missing = ImageLibrary::load(&doc, None);
        assert_eq!(missing.problems(&doc).count(), 1);
        let options = RenderOptions { images: Some(&missing), ..RenderOptions::default() };
        let placeholder = render_frame(&doc, 0, &options).unwrap();
        assert_ne!(pixel(&placeholder, 40, 50), [255, 255, 255, 255]);
    }

    #[test]
    fn text_is_filled_and_stroked_like_a_shape() {
        let mut doc = red_square_doc();
        doc.layers.clear();
        let mut text = inkframe_core::Text::new("I", 80.0);
        text.font.family = "Ubuntu".into(); // bundled, so the same everywhere
        let style = Style { fill: Some(Color::rgb(0, 0, 255)), stroke: None };
        doc.add_layer(
            "Label",
            vec![Element { shape: Shape::Text(text), style }],
            0,
            Pose::at(Point::new(40.0, 80.0)),
        );

        let pixmap = render_frame(&doc, 0, &RenderOptions::default()).unwrap();
        // The "I" is a vertical bar standing on the baseline at y = 80.
        let blue = (0..100).filter(|&x| pixel(&pixmap, x, 60) == [0, 0, 255, 255]).count();
        assert!(blue >= 3, "the letter's stem should be solid blue, found {blue} pixels");
        assert_eq!(pixel(&pixmap, 40, 90), [255, 255, 255, 255], "nothing below the baseline");
    }

    #[test]
    fn guides_are_shown_while_editing_but_never_exported() {
        let mut doc = red_square_doc();
        doc.layers[0].guide = true;
        let exported = render_frame(&doc, 0, &RenderOptions::default()).unwrap();
        assert_eq!(pixel(&exported, 20, 50), [255, 255, 255, 255]);
        let editing = RenderOptions { show_guides: true, ..RenderOptions::default() };
        assert_eq!(pixel(&render_frame(&doc, 0, &editing).unwrap(), 20, 50), [255, 0, 0, 255]);
    }

    /// A red 100×100 page, masked by a 20×20 square in the middle.
    fn masked_page() -> (Document, LayerId) {
        let mut doc = Document::default();
        (doc.width, doc.height) = (100, 100);
        doc.background = Color::WHITE;
        let red = Style { fill: Some(Color::rgb(255, 0, 0)), stroke: None };
        let page =
            Element { shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 }, style: red };
        let window =
            Element { shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 }, style: red };
        doc.add_layer("Page", vec![page], 0, Pose::at(Point::new(50.0, 50.0)));
        let mask = doc.add_layer("Window", vec![window], 0, Pose::at(Point::new(50.0, 50.0)));
        doc.mask_layer_below(mask);
        (doc, mask)
    }

    #[test]
    fn masked_layers_only_show_through_the_mask() {
        let (doc, _) = masked_page();
        let pixmap = render_frame(&doc, 0, &RenderOptions::default()).unwrap();
        assert_eq!(pixel(&pixmap, 50, 50), [255, 0, 0, 255], "inside the window");
        assert_eq!(pixel(&pixmap, 10, 10), [255, 255, 255, 255], "outside it, only the background");
    }

    #[test]
    fn a_half_faded_mask_lets_half_through() {
        let (mut doc, mask) = masked_page();
        doc.layer_mut(mask).unwrap().track.opacity.set(0, 0.5);
        let options = RenderOptions { draw_background: false, ..RenderOptions::default() };
        let [_, _, _, alpha] = pixel(&render_frame(&doc, 0, &options).unwrap(), 50, 50);
        assert!((126..=129).contains(&alpha), "alpha was {alpha}");
    }

    #[test]
    fn masks_are_never_drawn_unless_shown_for_positioning() {
        // Unlink the page, so only the mask itself could draw anything.
        let (mut doc, mask) = masked_page();
        doc.layers[0].masked_by = None;
        doc.layers[0].visible = false;
        let options = RenderOptions { draw_background: false, ..RenderOptions::default() };
        assert_eq!(pixel(&render_frame(&doc, 0, &options).unwrap(), 50, 50)[3], 0);
        let shown = RenderOptions { show_mask: Some(mask), ..options };
        let [_, _, _, alpha] = pixel(&render_frame(&doc, 0, &shown).unwrap(), 50, 50);
        assert!(alpha > 0 && alpha < 255, "a faint preview, alpha {alpha}");
    }

    #[test]
    fn emoji_are_drawn_in_their_own_colors() {
        let mut doc = Document::default();
        (doc.width, doc.height) = (200, 200);
        doc.background = Color::WHITE;
        let mut text = inkframe_core::Text::new("😀", 150.0);
        text.font.family = "Ubuntu".into(); // has no emoji, so they fall back
        // Black text: any yellow must come from the emoji's own colors.
        let style = Style { fill: Some(Color::BLACK), stroke: None };
        doc.add_layer(
            "Smile",
            vec![Element { shape: Shape::Text(text), style }],
            0,
            Pose::at(Point::new(20.0, 160.0)),
        );

        let pixmap = render_frame(&doc, 0, &RenderOptions::default()).unwrap();
        let yellow = (0..200)
            .flat_map(|y| (0..200).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b, _] = pixel(&pixmap, x, y);
                r > 200 && g > 150 && b < 120
            })
            .count();
        assert!(yellow > 2000, "a big yellow face, found {yellow} yellow pixels");
    }

    #[test]
    fn layer_opacity_fades_the_whole_layer() {
        let mut doc = red_square_doc();
        let layer = &mut doc.layers[0];
        layer.track.opacity.set(0, 0.5);
        // A second, overlapping shape: it must not show through the first.
        layer.elements.push(layer.elements[0].clone());

        let options = RenderOptions { draw_background: false, ..RenderOptions::default() };
        let pixmap = render_frame(&doc, 0, &options).unwrap();
        let [r, _, _, a] = pixel(&pixmap, 20, 50);
        assert_eq!(r, 255);
        assert!((126..=129).contains(&a), "alpha was {a}");
    }
}
