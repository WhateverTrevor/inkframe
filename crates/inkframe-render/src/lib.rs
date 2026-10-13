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

use inkframe_core::{Affine, Color, Document, Element, Layer};
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
    if options.draw_background {
        pixmap.fill(to_skia_color(doc.background));
    }

    let stage_to_pixels = Affine::scale(options.scale, options.scale);

    // Layers are stored bottom-first, so a plain loop paints back to front.
    // Reuse one scratch buffer across layers instead of allocating per layer.
    let mut scratch: Option<Pixmap> = None;
    for layer in doc.layers.iter().filter(|l| l.visible) {
        let transform = layer.transform_at(frame);
        let opacity = (transform.opacity * options.opacity).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        let matrix = to_skia_transform(stage_to_pixels.pre_concat(transform.to_affine()));

        if opacity >= 1.0 {
            draw_layer(pixmap, layer, matrix, options.tint);
            continue;
        }

        // A translucent layer must fade as a single unit. If we simply drew
        // each shape at reduced opacity, overlapping shapes inside the layer
        // would show through each other. So we draw the layer fully opaque
        // into a scratch buffer, then blend that buffer down in one go.
        let buffer = match &mut scratch {
            Some(buffer) => {
                buffer.fill(tiny_skia::Color::TRANSPARENT);
                buffer
            }
            None => scratch.insert(
                Pixmap::new(pixmap.width(), pixmap.height()).expect("same size as an existing pixmap"),
            ),
        };
        draw_layer(buffer, layer, matrix, options.tint);
        let paint = PixmapPaint { opacity, ..PixmapPaint::default() };
        pixmap.draw_pixmap(0, 0, buffer.as_ref(), &paint, Transform::identity(), None);
    }
}

fn draw_layer(pixmap: &mut Pixmap, layer: &Layer, matrix: Transform, tint: Option<Color>) {
    for element in &layer.elements {
        draw_element(pixmap, element, matrix, tint);
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
