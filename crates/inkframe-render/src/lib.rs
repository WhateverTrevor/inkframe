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

mod path;

use inkframe_core::{Affine, Color, Document, Element, Layer, Point, SymbolId, TimelineId};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Pixmap, PixmapPaint, Transform};

pub use path::build_path;

/// Settings for one render pass.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
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
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { scale: 1.0, draw_background: true, tint: None, opacity: 1.0 }
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
    render_timeline_into(pixmap, doc, TimelineId::Main, frame, Point::ZERO, options);
}

/// Like [`render_frame_into`], for any timeline. The editor uses this to
/// show a symbol on its own while it's being edited: `origin` is where the
/// timeline's (0, 0) lands, in stage pixels, and `frame` is in the
/// timeline's own frames.
pub fn render_timeline_into(
    pixmap: &mut Pixmap,
    doc: &Document,
    timeline: TimelineId,
    frame: u32,
    origin: Point,
    options: &RenderOptions,
) {
    if options.draw_background {
        pixmap.fill(to_skia_color(doc.background));
    }

    let to_pixels = Affine::scale(options.scale, options.scale).pre_concat(Affine::translate(origin));
    let style = Style { tint: options.tint };
    render_layers(pixmap, doc, doc.layers_of(timeline), frame, to_pixels, options.opacity, style, 0);
}

/// Renders one frame of a symbol, scaled to fit a `size`×`size` square, for
/// library thumbnails. Returns `None` for symbols with nothing to draw.
pub fn render_symbol_thumbnail(doc: &Document, symbol: SymbolId, frame: u32, size: u32) -> Option<Pixmap> {
    let symbol = doc.symbol(symbol)?;
    let bounds = symbol
        .layers
        .iter()
        .filter(|l| l.is_shown_at(frame))
        .filter_map(|l| {
            let local = doc.layer_bounds(l, frame)?;
            let to_symbol = l.transform_at(frame).to_affine();
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
    render_layers(&mut pixmap, doc, &symbol.layers, frame, fit, 1.0, Style { tint: None }, 0);
    Some(pixmap)
}

/// Settings that apply to everything drawn in one pass, however deeply
/// nested inside symbols.
#[derive(Clone, Copy)]
struct Style {
    tint: Option<Color>,
}

/// Draws a stack of layers (the document's, or a symbol's) at `frame`.
///
/// `parent` maps the layers' coordinate space to pixels: the stage scale at
/// the top level, and the instance's full transform inside a symbol. Each
/// nested symbol calls back in here with its own frame and transform, so
/// symbols can contain symbols to any depth (up to a safety limit).
#[allow(clippy::too_many_arguments)]
fn render_layers(
    pixmap: &mut Pixmap,
    doc: &Document,
    layers: &[Layer],
    frame: u32,
    parent: Affine,
    opacity: f32,
    style: Style,
    depth: usize,
) {
    // Layers are stored bottom-first, so a plain loop paints back to front.
    for layer in layers.iter().filter(|l| l.is_shown_at(frame)) {
        let transform = layer.transform_at(frame);
        let opacity = (transform.opacity * opacity).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        let matrix = parent.pre_concat(transform.to_affine());

        if opacity >= 1.0 {
            draw_layer(pixmap, doc, layer, frame, matrix, style, depth);
            continue;
        }

        // A translucent layer must fade as a single unit. If we simply drew
        // each shape at reduced opacity, overlapping shapes inside the layer
        // would show through each other. So we draw the layer fully opaque
        // into a scratch buffer, then blend that buffer down in one go.
        // Symbols nest, so each translucent layer needs a buffer of its own.
        let mut scratch =
            Pixmap::new(pixmap.width(), pixmap.height()).expect("same size as an existing pixmap");
        draw_layer(&mut scratch, doc, layer, frame, matrix, style, depth);
        let paint = PixmapPaint { opacity, ..PixmapPaint::default() };
        pixmap.draw_pixmap(0, 0, scratch.as_ref(), &paint, Transform::identity(), None);
    }
}

/// Draws a layer's own shapes, then its symbol instance if it has one.
fn draw_layer(
    pixmap: &mut Pixmap,
    doc: &Document,
    layer: &Layer,
    frame: u32,
    matrix: Affine,
    style: Style,
    depth: usize,
) {
    let skia_matrix = to_skia_transform(matrix);
    for element in &layer.elements {
        draw_element(pixmap, element, skia_matrix, style.tint);
    }
    // `instance_view` stops at the nesting limit, so a malformed file can't
    // recurse forever.
    if let Some((symbol, symbol_frame)) = doc.instance_view(layer, frame, depth) {
        render_layers(pixmap, doc, &symbol.layers, symbol_frame, matrix, 1.0, style, depth + 1);
    }
}

fn draw_element(pixmap: &mut Pixmap, element: &Element, matrix: Transform, tint: Option<Color>) {
    let Some(path) = build_path(&element.shape) else {
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
        let thumb = render_symbol_thumbnail(&doc, symbol, 0, 64).unwrap();
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
