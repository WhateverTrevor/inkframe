//! Filling a canvas: the Paint Bucket and the Gradient.
//!
//! The bucket fills an area of similar colour: the pixels connected to the
//! one clicked (or, not contiguous, every matching pixel on the canvas)
//! whose colour is within `tolerance` of it. The gradient lays a smooth
//! blend from one colour to another across the whole canvas, along a line
//! (linear) or out from a point (radial).
//!
//! Both paint over what's there at an `opacity`, the way the brush does.

use crate::color::Color;
use crate::geometry::Point;
use crate::paint::{Canvas, premultiply};

/// How the Paint Bucket chooses what to fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BucketSettings {
    /// How different a pixel may be from the clicked one and still be
    /// filled: 0 only the exact colour, 255 everything.
    pub tolerance: u8,
    /// Fill only pixels connected to the clicked one, or every match.
    pub contiguous: bool,
    pub opacity: f32,
}

impl Default for BucketSettings {
    fn default() -> Self {
        Self { tolerance: 32, contiguous: true, opacity: 1.0 }
    }
}

/// Fills from pixel (`x`, `y`) with `color`. Returns false if the pixel is
/// off the canvas.
pub fn bucket_fill(canvas: &mut Canvas, x: i64, y: i64, color: Color, settings: BucketSettings) -> bool {
    let (width, height) = (canvas.width as i64, canvas.height as i64);
    if x < 0 || y < 0 || x >= width || y >= height {
        return false;
    }
    let mut pixels = canvas.read_region(0, 0, canvas.width, canvas.height);
    let at = |x: i64, y: i64| ((y * width + x) * 4) as usize;
    let target: [u8; 4] = pixels[at(x, y)..at(x, y) + 4].try_into().expect("four bytes");
    let similar = |p: &[u8]| p.iter().zip(target).all(|(a, b)| a.abs_diff(b) <= settings.tolerance);

    let mut fill = vec![false; (width * height) as usize];
    if settings.contiguous {
        // Scanline flood fill: from each seed, run left and right along
        // the row, then queue the rows above and below.
        let mut seeds = vec![(x, y)];
        while let Some((sx, sy)) = seeds.pop() {
            if fill[(sy * width + sx) as usize] || !similar(&pixels[at(sx, sy)..at(sx, sy) + 4]) {
                continue;
            }
            let matches =
                |x: i64| !fill[(sy * width + x) as usize] && similar(&pixels[at(x, sy)..at(x, sy) + 4]);
            let mut left = sx;
            while left > 0 && matches(left - 1) {
                left -= 1;
            }
            let mut right = sx;
            while right + 1 < width && matches(right + 1) {
                right += 1;
            }
            for x in left..=right {
                fill[(sy * width + x) as usize] = true;
                for ny in [sy - 1, sy + 1] {
                    if (0..height).contains(&ny) && !fill[(ny * width + x) as usize] {
                        seeds.push((x, ny));
                    }
                }
            }
        }
    } else {
        for (i, filled) in fill.iter_mut().enumerate() {
            *filled = similar(&pixels[i * 4..i * 4 + 4]);
        }
    }

    let source = premultiply(&[color.r, color.g, color.b, color.a]);
    let alpha = settings.opacity.clamp(0.0, 1.0);
    for (i, _) in fill.iter().enumerate().filter(|(_, f)| **f) {
        over(&mut pixels[i * 4..i * 4 + 4], source, alpha);
    }
    canvas.write_region(0, 0, canvas.width, canvas.height, &pixels);
    true
}

/// The shape of a gradient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GradientShape {
    /// Bands across the line from start to end.
    #[default]
    Linear,
    /// Rings out from the start, reaching the end colour at the end.
    Radial,
}

/// Lays a gradient over the whole canvas: `start` at `from`, blending to
/// `end` at `to` (canvas pixels), at `opacity`.
pub fn gradient(
    canvas: &mut Canvas,
    from: Point,
    to: Point,
    start: Color,
    end: Color,
    shape: GradientShape,
    opacity: f32,
) {
    let mut pixels = canvas.read_region(0, 0, canvas.width, canvas.height);
    let axis = to - from;
    let length_sq = axis.dot(axis).max(1e-6);
    let alpha = opacity.clamp(0.0, 1.0);
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let p = Point::new((i as u32 % canvas.width) as f32 + 0.5, (i as u32 / canvas.width) as f32 + 0.5);
        let t = match shape {
            GradientShape::Linear => (p - from).dot(axis) / length_sq,
            GradientShape::Radial => (p - from).length() / length_sq.sqrt(),
        }
        .clamp(0.0, 1.0);
        // Blended as straight colours, so a fade to transparent doesn't
        // darken through grey on the way.
        let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        let straight = [
            channel(start.r, end.r),
            channel(start.g, end.g),
            channel(start.b, end.b),
            channel(start.a, end.a),
        ];
        over(pixel, premultiply(&straight), alpha);
    }
    canvas.write_region(0, 0, canvas.width, canvas.height, &pixels);
}

/// Premultiplied `source` over `pixel`, at `alpha`.
fn over(pixel: &mut [u8], source: [u8; 4], alpha: f32) {
    let a = source[3] as f32 / 255.0 * alpha;
    for c in 0..4 {
        pixel[c] = (source[c] as f32 * alpha + pixel[c] as f32 * (1.0 - a)).round().min(255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 60×20 canvas: a black outline of a box from x 10 to 30, the rest
    /// empty.
    fn outlined() -> Canvas {
        let mut canvas = Canvas::new(60, 20);
        let black = [0, 0, 0, 255];
        for y in 5..15 {
            for x in [10, 30] {
                canvas.write_region(x, y, 1, 1, &black);
            }
        }
        canvas.write_region(10, 5, 21, 1, &black.repeat(21));
        canvas.write_region(10, 14, 21, 1, &black.repeat(21));
        canvas
    }

    #[test]
    fn the_bucket_fills_inside_an_outline() {
        let mut canvas = outlined();
        let red = Color::rgb(255, 0, 0);
        assert!(bucket_fill(&mut canvas, 20, 10, red, BucketSettings::default()));
        assert_eq!(canvas.pixel(20, 10), [255, 0, 0, 255]);
        assert_eq!(canvas.pixel(11, 6), [255, 0, 0, 255]);
        assert_eq!(canvas.pixel(10, 10), [0, 0, 0, 255], "the outline stays");
        assert_eq!(canvas.pixel(40, 10), [0; 4], "outside isn't reached");
    }

    #[test]
    fn not_contiguous_fills_every_match() {
        let mut canvas = outlined();
        let settings = BucketSettings { contiguous: false, ..BucketSettings::default() };
        bucket_fill(&mut canvas, 20, 10, Color::rgb(0, 0, 255), settings);
        assert_eq!(canvas.pixel(40, 10), [0, 0, 255, 255]);
        assert_eq!(canvas.pixel(10, 10), [0, 0, 0, 255]);
    }

    #[test]
    fn tolerance_decides_what_counts_as_the_same_colour() {
        let mut canvas = Canvas::new(10, 1);
        let row: Vec<u8> = (0..10).flat_map(|x| [x * 20, 0, 0, 255]).collect();
        canvas.write_region(0, 0, 10, 1, &row);
        let settings = BucketSettings { tolerance: 45, ..BucketSettings::default() };
        bucket_fill(&mut canvas, 0, 0, Color::rgb(0, 255, 0), settings);
        // 0, 20 and 40 are within 45 of 0; 60 isn't.
        assert_eq!(canvas.pixel(2, 0), [0, 255, 0, 255]);
        assert_eq!(canvas.pixel(3, 0), [60, 0, 0, 255]);
    }

    #[test]
    fn a_linear_gradient_blends_along_its_line() {
        let mut canvas = Canvas::new(101, 3);
        let (black, white) = (Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
        gradient(
            &mut canvas,
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            black,
            white,
            GradientShape::Linear,
            1.0,
        );
        assert_eq!(canvas.pixel(0, 1)[0], 1);
        assert!((canvas.pixel(50, 1)[0] as i32 - 128).abs() <= 2);
        assert_eq!(canvas.pixel(100, 1), [255, 255, 255, 255]);
    }

    #[test]
    fn a_fade_to_transparent_keeps_its_colour() {
        let mut canvas = Canvas::new(100, 1);
        let red = Color::rgb(200, 0, 0);
        let clear = Color { a: 0, ..red };
        gradient(&mut canvas, Point::ZERO, Point::new(100.0, 0.0), red, clear, GradientShape::Radial, 1.0);
        let middle = crate::paint::unpremultiply(&canvas.pixel(50, 0));
        assert_eq!(middle[0], 200, "still red, just thinner: {middle:?}");
        assert!((100..=155).contains(&middle[3]));
    }
}
