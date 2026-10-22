//! SVG export: a frame as vector graphics, for the web, print, or another
//! drawing program. One file for one frame; a numbered file per frame for
//! several (`walk_0001.svg`, `walk_0002.svg`…).
//!
//! It draws what the renderer draws, the same way round: each layer a group
//! with its opacity, blend mode and mask, holding its shapes (as paths, with
//! their fills, gradients and line styles) and, for a symbol, the symbol's
//! own layers. Pictures and paintings go in as embedded PNGs. What SVG
//! can't say as shapes (a layer's filters and colour effects, particles)
//! goes in as that layer drawn by the renderer, so nothing is left out.

use std::fmt::Write as _;
use std::ops::ControlFlow;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use inkframe_core::{
    Affine, BlendMode, Color, Document, Element, GradientShape, Layer, LineCap, LineJoin, Shape,
    ShapeGradient, Stroke, TimelineId,
};
use inkframe_render::ImageLibrary;

use crate::png_sequence::{digits_for, frame_path};
use crate::{ExportError, ExportSettings, Progress};

pub(crate) fn write(
    doc: &Document,
    images: &ImageLibrary,
    settings: &ExportSettings,
    mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    let frames_total = settings.frames.end() - settings.frames.start() + 1;
    let digits = digits_for(*settings.frames.end() + 1);
    let mut written = Vec::new();
    for (done, frame) in settings.frames.clone().enumerate() {
        let path = if frames_total == 1 {
            settings.output.clone()
        } else {
            frame_path(&settings.output, frame, digits, "svg")
        };
        std::fs::write(&path, frame_svg(doc, images, frame, settings.scale, settings.transparent))?;
        written.push(path);
        if on_progress(Progress { frames_done: done as u32 + 1, frames_total }).is_break() {
            for path in written {
                let _ = std::fs::remove_file(path);
            }
            return Err(ExportError::Cancelled);
        }
    }
    Ok(())
}

/// One frame of the main timeline as an SVG document, `scale` times the
/// stage's size.
pub fn frame_svg(doc: &Document, images: &ImageLibrary, frame: u32, scale: f32, transparent: bool) -> String {
    let (width, height) = inkframe_render::output_size(doc, scale);
    let mut svg =
        Svg { doc, images, size: (width, height), defs: String::new(), body: String::new(), ids: 0 };
    if !transparent {
        let _ =
            writeln!(svg.body, r#"<rect width="{width}" height="{height}"{}/>"#, fill_attrs(doc.background));
    }
    let layers = doc.stacking_order(TimelineId::Main, frame);
    svg.layers(&layers, frame, Affine::scale(scale, scale), 0);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\">\n<defs>\n{}</defs>\n{}</svg>\n",
        svg.defs, svg.body
    )
}

struct Svg<'a> {
    doc: &'a Document,
    images: &'a ImageLibrary,
    size: (u32, u32),
    defs: String,
    body: String,
    /// For naming gradients, masks and images.
    ids: usize,
}

impl Svg<'_> {
    fn id(&mut self, kind: &str) -> String {
        self.ids += 1;
        format!("{kind}{}", self.ids)
    }

    /// Layers bottom first, as the renderer paints them. `to_out` maps their
    /// timeline onto the SVG (for a symbol's layers, through its instance).
    fn layers(&mut self, layers: &[&Layer], frame: u32, to_out: Affine, depth: usize) {
        let doc = self.doc;
        for layer in layers.iter().copied() {
            // Guides are for editing, and masks only shape what they mask.
            if !doc.shows(layer, frame) || layer.guide || layer.mask {
                continue;
            }
            let opacity =
                (doc.pose(layer, frame).opacity * doc.camera_fade(layer, frame as f32)).clamp(0.0, 1.0);
            if opacity <= 0.0 {
                continue;
            }
            let mut attrs = String::new();
            if let Some(mask) = doc.mask_for(layer, frame) {
                let id = self.mask(mask, frame, to_out);
                let _ = write!(attrs, r#" mask="url(#{id})""#);
            }
            if let Some(mode) = css_blend(layer.blend) {
                let _ = write!(attrs, r#" style="mix-blend-mode:{mode}""#);
            }

            // What SVG can't draw as shapes is drawn by the renderer: the
            // layer whole, its opacity included. That includes pixels bent
            // by a skeleton.
            let fx = layer.track.effects_at(frame as f32);
            if layer.particles.is_some() || !fx.is_none() || crate::bends_pixels(doc, layer, frame) {
                let _ = writeln!(self.body, "<g{attrs}>");
                if let Some(pixels) = inkframe_render::render_layer_alone(
                    doc,
                    Some(self.images),
                    layer,
                    frame,
                    to_out,
                    self.size,
                ) {
                    let (w, h) = self.size;
                    let _ = writeln!(
                        self.body,
                        r#"<image width="{w}" height="{h}" href="{}"/>"#,
                        png_data(&pixels)
                    );
                }
                let _ = writeln!(self.body, "</g>");
                continue;
            }

            if opacity < 1.0 {
                let _ = write!(attrs, r#" opacity="{}""#, num(opacity));
            }
            let matrix = to_out.pre_concat(doc.layer_matrix(layer, frame));
            let _ = writeln!(self.body, "<g{attrs}>");
            let _ = writeln!(self.body, r#"<g transform="{}">"#, transform(matrix));
            for element in doc.posed_elements(layer, frame).iter() {
                for part in doc.drawn_as(element).iter() {
                    self.element(part, false);
                }
            }
            let _ = writeln!(self.body, "</g>");
            if let Some((symbol, symbol_frame)) = doc.instance_view(layer, frame, depth) {
                let inner: Vec<&Layer> = symbol.layers.iter().collect();
                self.layers(&inner, symbol_frame, matrix, depth + 1);
            }
            let _ = writeln!(self.body, "</g>");
        }
    }

    /// A mask from `mask`'s drawing, in white, so what it covers shows.
    fn mask(&mut self, mask: &Layer, frame: u32, to_out: Affine) -> String {
        let id = self.id("mask");
        let matrix = to_out.pre_concat(self.doc.layer_matrix(mask, frame));
        let (w, h) = self.size;
        let body = std::mem::take(&mut self.body);
        for element in self.doc.posed_elements(mask, frame).iter() {
            self.element(element, true);
        }
        let shapes = std::mem::replace(&mut self.body, body);
        let _ = writeln!(
            self.defs,
            "<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" \
             style=\"mask-type:alpha\">\n<g transform=\"{}\">\n{shapes}</g>\n</mask>",
            transform(matrix)
        );
        id
    }

    /// One shape, in its layer's coordinates. `white` draws it all white,
    /// for a mask.
    fn element(&mut self, element: &Element, white: bool) {
        match &element.shape {
            Shape::Image { image, width, height } => {
                if let Some(pixels) = self.images.full_size(*image) {
                    self.picture(pixels, *width, *height);
                }
                return;
            }
            Shape::Paint { canvas, width, height } => {
                if let Some(pixels) = inkframe_render::canvas_image(self.doc, *canvas, *width, *height) {
                    self.picture(&pixels, *width as f32, *height as f32);
                }
                return;
            }
            _ => {}
        }
        let Some(path) = inkframe_render::shape_outline(&element.shape) else { return };
        let d = path_data(&path);
        let style = &element.style;
        let mut attrs = String::new();
        // A pencil line is drawn as its filled outline (see `line.rs`).
        let line = matches!(element.shape, Shape::Line { .. });
        if white {
            attrs.push_str(r##" fill="#ffffff""##);
            if let Some(stroke) = style.stroke.filter(|s| s.width > 0.0 && !line) {
                attrs.push_str(&stroke_attrs(Stroke { color: Color::WHITE, ..stroke }));
            }
        } else {
            match (&style.gradient, style.fill) {
                (Some(gradient), _) => {
                    let id = self.gradient(gradient, &element.shape);
                    let _ = write!(attrs, r#" fill="url(#{id})""#);
                }
                (None, Some(fill)) => attrs.push_str(&fill_attrs(fill)),
                (None, None) => attrs.push_str(r#" fill="none""#),
            }
            if let Some(stroke) = style.stroke.filter(|s| s.width > 0.0 && !line) {
                attrs.push_str(&stroke_attrs(stroke));
            }
        }
        let _ = writeln!(self.body, r#"<path d="{d}"{attrs}/>"#);
    }

    /// A picture `width`×`height` centred on the origin, as embedded PNG.
    fn picture(&mut self, pixels: &tiny_skia::Pixmap, width: f32, height: f32) {
        let _ = writeln!(
            self.body,
            r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" href="{}"/>"#,
            num(-width / 2.0),
            num(-height / 2.0),
            num(width),
            num(height),
            png_data(pixels)
        );
    }

    /// A gradient definition laid over `shape`'s box, as the renderer lays
    /// it; returns its id.
    fn gradient(&mut self, gradient: &ShapeGradient, shape: &Shape) -> String {
        let id = self.id("gradient");
        let (from, to) = gradient.ends_in(shape.bounds());
        let stops: String = gradient
            .stops
            .iter()
            .map(|stop| {
                let c = stop.color;
                let opacity = if c.a < 255 {
                    format!(r#" stop-opacity="{}""#, num(c.a as f32 / 255.0))
                } else {
                    String::new()
                };
                format!(r#"<stop offset="{}" stop-color="{}"{opacity}/>"#, num(stop.at), hex(c))
            })
            .collect();
        let _ = match gradient.shape {
            GradientShape::Linear => writeln!(
                self.defs,
                r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}">{stops}</linearGradient>"#,
                num(from.x),
                num(from.y),
                num(to.x),
                num(to.y)
            ),
            GradientShape::Radial => writeln!(
                self.defs,
                r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{}">{stops}</radialGradient>"#,
                num(from.x),
                num(from.y),
                num(from.distance(to).max(0.01))
            ),
        };
        id
    }
}

/// A tiny-skia outline as SVG path data.
fn path_data(path: &tiny_skia::Path) -> String {
    use tiny_skia::PathSegment;
    let mut d = String::new();
    let p = |pt: tiny_skia::Point| format!("{} {}", num(pt.x), num(pt.y));
    for segment in path.segments() {
        let _ = match segment {
            PathSegment::MoveTo(a) => write!(d, "M{} ", p(a)),
            PathSegment::LineTo(a) => write!(d, "L{} ", p(a)),
            PathSegment::QuadTo(a, b) => write!(d, "Q{} {} ", p(a), p(b)),
            PathSegment::CubicTo(a, b, c) => write!(d, "C{} {} {} ", p(a), p(b), p(c)),
            PathSegment::Close => write!(d, "Z "),
        };
    }
    d.trim_end().to_owned()
}

fn fill_attrs(color: Color) -> String {
    let mut attrs = format!(r#" fill="{}""#, hex(color));
    if color.a < 255 {
        let _ = write!(attrs, r#" fill-opacity="{}""#, num(color.a as f32 / 255.0));
    }
    attrs
}

fn stroke_attrs(stroke: Stroke) -> String {
    let mut attrs = format!(r#" stroke="{}" stroke-width="{}""#, hex(stroke.color), num(stroke.width));
    if stroke.color.a < 255 {
        let _ = write!(attrs, r#" stroke-opacity="{}""#, num(stroke.color.a as f32 / 255.0));
    }
    let cap = match stroke.cap {
        LineCap::Round => "round",
        LineCap::Butt => "butt",
        LineCap::Square => "square",
    };
    let join = match stroke.join {
        LineJoin::Round => "round",
        LineJoin::Miter => "miter",
        LineJoin::Bevel => "bevel",
    };
    let _ = write!(attrs, r#" stroke-linecap="{cap}" stroke-linejoin="{join}""#);
    if let Some(dash) = stroke.dash {
        let _ = write!(
            attrs,
            r#" stroke-dasharray="{} {}""#,
            num(dash.length.max(0.01)),
            num(dash.gap.max(0.01))
        );
    }
    attrs
}

fn transform(m: Affine) -> String {
    format!("matrix({} {} {} {} {} {})", num(m.a), num(m.b), num(m.c), num(m.d), num(m.e), num(m.f))
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// A number with no more digits than matter: `12`, `0.5`, `-3.142`.
fn num(v: f32) -> String {
    let rounded = (v * 1000.0).round() / 1000.0;
    let text = format!("{rounded:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_owned() } else { text.to_owned() }
}

fn png_data(pixels: &tiny_skia::Pixmap) -> String {
    let png = pixels.encode_png().unwrap_or_default();
    format!("data:image/png;base64,{}", BASE64.encode(png))
}

/// SVG's (CSS's) name for a blend mode; `None` for normal. Add has no CSS
/// equivalent, so it's drawn as screen, the nearest.
fn css_blend(mode: BlendMode) -> Option<&'static str> {
    Some(match mode {
        BlendMode::Normal => return None,
        BlendMode::Multiply => "multiply",
        BlendMode::Screen | BlendMode::Add => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::SoftLight => "soft-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
    })
}

#[cfg(test)]
mod tests {
    use inkframe_core::{Dash, Point, Style, Transform};

    use super::*;

    #[test]
    fn shapes_become_paths_with_their_styles() {
        let mut doc = Document::default();
        let gradient = ShapeGradient::linear(Color::BLACK, Color::WHITE);
        let square = Element {
            shape: Shape::Rect { width: 20.0, height: 10.0, corner_radius: 0.0 },
            style: Style {
                fill: Some(Color::BLACK),
                stroke: Some(Stroke {
                    dash: Some(Dash { length: 4.0, gap: 2.0 }),
                    ..Stroke::new(Color::rgb(255, 0, 0), 2.0)
                }),
                gradient: Some(gradient),
            },
        };
        let id = doc.add_layer("Square", vec![square], 0, Transform::at(Point::new(100.0, 50.0)));
        let mut pose = doc.pose(doc.layer(id).unwrap(), 0);
        pose.opacity = 0.5;
        doc.set_pose(id, 0, pose);

        let svg = frame_svg(&doc, &ImageLibrary::default(), 0, 1.0, true);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1920\" height=\"1080\""));
        assert!(svg.contains(r#"<g opacity="0.5">"#), "{svg}");
        assert!(svg.contains(r#"transform="matrix(1 0 0 1 100 50)""#), "{svg}");
        assert!(svg.contains(r#"<path d="M-10 -5 L10 -5 L10 5 L-10 5 Z""#), "{svg}");
        assert!(svg.contains(r#"fill="url(#gradient1)""#));
        assert!(svg.contains(r#"<linearGradient id="gradient1" gradientUnits="userSpaceOnUse" x1="-10" y1="0" x2="10" y2="0">"#), "{svg}");
        assert!(svg.contains(r##"stroke="#ff0000" stroke-width="2""##));
        assert!(svg.contains(r#"stroke-dasharray="4 2""#));
        assert!(!svg.contains("<rect"), "transparent: no background");
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(-1.23456), "-1.235");
        assert_eq!(num(-0.0001), "0");
    }
}
