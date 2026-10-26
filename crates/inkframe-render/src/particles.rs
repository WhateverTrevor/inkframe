//! Drawing a particle layer's particles (see `particles.rs` in the core).
//!
//! Dots are drawn as circles, softened with a radial gradient. A soft dot
//! fades to nothing at its edge, so its whole square is filled with the
//! gradient instead, without anti-aliasing: the same picture, without the
//! cost of a smooth circle's edge, which was most of drawing particles. A layer
//! whose own drawing is the particle is drawn into a small picture once,
//! at the size the biggest particle shows, and that picture is stamped at
//! each particle: thousands of copies of a drawing would otherwise mean
//! building its outlines thousands of times.

use inkframe_core::{Affine, Document, Layer, Point, Shape, Sprite};
use tiny_skia::{
    FillRule, FilterQuality, GradientStop, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint,
    RadialGradient, SpreadMode, Transform,
};

use super::{Style, draw_canvas, draw_element, to_skia_color, to_skia_transform};

/// The biggest a drawing sprite is drawn before stamping, in pixels.
const MOST_SPRITE_PIXELS: f32 = 512.0;

pub(crate) fn draw_particles(
    pixmap: &mut PixmapMut,
    doc: &Document,
    layer: &Layer,
    frame: u32,
    matrix: Affine,
    style: Style<'_>,
) {
    let Some(emitter) = &layer.particles else { return };
    let (width, height) = (pixmap.width() as f32, pixmap.height() as f32);
    // Particles on screen, leaving out any wholly off the image.
    let particles: Vec<_> = doc
        .particles_on_layer(layer, frame as f32)
        .into_iter()
        .filter(|(p, _)| p.opacity > 0.0 && p.size > 0.0)
        .map(|(p, placement)| (p, matrix.pre_concat(placement)))
        .filter(|(_, m)| {
            let centre = m.apply(Point::ZERO);
            let reach = m.average_scale();
            centre.x > -reach && centre.y > -reach && centre.x < width + reach && centre.y < height + reach
        })
        .collect();
    if particles.is_empty() {
        return;
    }

    match emitter.sprite {
        Sprite::Dot { softness } => {
            let softness = softness.clamp(0.0, 1.0);
            let shape = if softness <= 0.0 {
                PathBuilder::from_circle(0.0, 0.0, 0.5)
            } else {
                tiny_skia::Rect::from_xywh(-0.5, -0.5, 1.0, 1.0).map(PathBuilder::from_rect)
            };
            let Some(shape) = shape else { return };
            for (particle, m) in &particles {
                let colour = style.tint.unwrap_or(particle.color);
                let colour = colour.with_alpha((colour.a as f32 * particle.opacity).round() as u8);
                // Only a hard dot has an edge to smooth.
                let mut paint = Paint { anti_alias: softness <= 0.0, ..Paint::default() };
                let solid = to_skia_color(colour);
                if softness <= 0.0 {
                    paint.set_color(solid);
                } else {
                    let mut clear = solid;
                    clear.set_alpha(0.0);
                    let stops = vec![
                        GradientStop::new(0.0, solid),
                        GradientStop::new(1.0 - softness, solid),
                        GradientStop::new(1.0, clear),
                    ];
                    let Some(shader) = RadialGradient::new(
                        tiny_skia::Point::zero(),
                        0.0,
                        tiny_skia::Point::zero(),
                        0.5,
                        stops,
                        SpreadMode::Pad,
                        Transform::identity(),
                    ) else {
                        continue;
                    };
                    paint.shader = shader;
                }
                pixmap.fill_path(&shape, &paint, FillRule::Winding, to_skia_transform(*m), None);
            }
        }
        Sprite::Drawing => {
            let elements = layer.shown_elements(frame);
            let Some(bounds) = doc.element_bounds(&elements) else { return };
            // The drawing, fitted into a unit square about the origin.
            let fit = Affine::scale(
                1.0 / bounds.width().max(bounds.height()).max(1e-3),
                1.0 / bounds.width().max(bounds.height()).max(1e-3),
            )
            .pre_concat(Affine::translate(Point::ZERO - bounds.center()));
            let biggest = particles.iter().map(|(_, m)| m.average_scale()).fold(0.0, f32::max);
            let side = biggest.ceil().clamp(4.0, MOST_SPRITE_PIXELS);
            let Some(mut sprite) = Pixmap::new(side as u32, side as u32) else { return };
            let to_sprite =
                Affine::scale(side, side).pre_concat(Affine::translate(Point::new(0.5, 0.5))).pre_concat(fit);
            let skia = to_skia_transform(to_sprite);
            for element in elements.iter() {
                match element.shape {
                    Shape::Paint { canvas, width, height } => {
                        if let Some(canvas) = doc.canvas(canvas) {
                            draw_canvas(&mut sprite.as_mut(), canvas, width, height, skia, style.tint);
                        }
                    }
                    _ => draw_element(&mut sprite.as_mut(), element, skia, style),
                }
            }
            // Stamp it: the sprite's pixels back onto the unit square.
            let from_sprite =
                Affine::translate(Point::new(-0.5, -0.5)).pre_concat(Affine::scale(1.0 / side, 1.0 / side));
            for (particle, m) in &particles {
                let paint = PixmapPaint {
                    opacity: particle.opacity,
                    quality: FilterQuality::Bilinear,
                    ..PixmapPaint::default()
                };
                pixmap.draw_pixmap(
                    0,
                    0,
                    sprite.as_ref(),
                    &paint,
                    to_skia_transform(m.pre_concat(from_sprite)),
                    None,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use inkframe_core::{
        Color, Document, Element, Emitter, EmitterShape, Point, Preset, Shape, Sprite, Style,
    };

    use crate::{RenderOptions, render_frame};

    /// A dark stage with a steady column of white dots rising from (50, 90).
    fn column(sprite: Sprite) -> Document {
        let mut doc = Document::default();
        doc.width = 100;
        doc.height = 100;
        doc.background = Color::BLACK;
        let id = doc.add_particle_layer(Preset::Sparks, 0, Point::new(50.0, 90.0)).unwrap();
        let layer = doc.layer_mut(id).unwrap();
        layer.blend = inkframe_core::BlendMode::Normal;
        layer.particles = Some(Emitter {
            rate: 60.0,
            shape: EmitterShape::Point,
            direction: -90.0,
            spread: 0.0,
            speed: 60.0,
            speed_variance: 0.0,
            gravity: 0.0,
            drag: 0.0,
            lifetime: 1.0,
            lifetime_variance: 0.0,
            size_start: 6.0,
            size_end: 6.0,
            size_variance: 0.0,
            opacity_start: 1.0,
            opacity_end: 1.0,
            color_start: Color::WHITE,
            color_end: Color::WHITE,
            sprite,
            ..Emitter::default()
        });
        doc
    }

    fn brightness(doc: &Document, frame: u32, x: u32, y: u32) -> u8 {
        let pixmap = render_frame(doc, frame, &RenderOptions::default()).unwrap();
        pixmap.pixel(x, y).unwrap().red()
    }

    #[test]
    fn dots_are_drawn_along_their_path() {
        let doc = column(Sprite::Dot { softness: 0.0 });
        // A second in, the column reaches from the emitter up 60 px.
        assert!(brightness(&doc, 24, 50, 60) > 200, "on the path");
        assert!(brightness(&doc, 24, 20, 60) < 10, "beside it");
        assert!(brightness(&doc, 24, 50, 10) < 10, "past where they live");
    }

    #[test]
    fn a_drawing_can_be_the_particle() {
        let mut doc = column(Sprite::Drawing);
        let square = Element {
            shape: Shape::Rect { width: 10.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::rgb(255, 0, 0)), stroke: None, gradient: None },
        };
        let id = doc.layers.last().unwrap().id;
        doc.layer_mut(id).unwrap().drawings[0].elements = vec![square];
        let pixmap = render_frame(&doc, 24, &RenderOptions::default()).unwrap();
        let p = pixmap.pixel(50, 60).unwrap();
        assert!(p.red() > 200 && p.green() < 30, "a red square, not a white dot: {p:?}");
    }
}
