//! The retouching tools: Smudge, Blur, Sharpen, Dodge, Burn and Clone
//! Stamp.
//!
//! They're strokes like the brush's, with dabs laid along the pen's path
//! the same way (see [`DabPath`]), but instead of adding colour each dab
//! reworks the pixels already under it:
//!
//! * **Smudge** drags paint along, as a finger through wet paint: each dab
//!   puts down what the previous one picked up, mixed with what's there.
//! * **Blur** softens, mixing each pixel toward its neighbours' average;
//!   **Sharpen** does the opposite, pushing each pixel away from that
//!   average (an "unsharp mask").
//! * **Dodge** lightens and **Burn** darkens, aimed at the shadows, the
//!   midtones or the highlights, like their darkroom namesakes.
//! * **Clone Stamp** paints with pixels copied from elsewhere on the canvas,
//!   a fixed distance away (Alt-click sets where from).
//!
//! Each dab works on a small square of pixels around itself, read from the
//! canvas, changed, and written back. Pixels are premultiplied, which is
//! the right form for mixing (a transparent pixel adds no colour).

use crate::brush::{BrushSettings, DabPath};
use crate::geometry::Point;
use crate::paint::Canvas;
use crate::selection::Selection;

/// Which part of the tonal range Dodge and Burn affect most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToneRange {
    Shadows,
    #[default]
    Midtones,
    Highlights,
}

impl ToneRange {
    pub const ALL: [ToneRange; 3] = [ToneRange::Shadows, ToneRange::Midtones, ToneRange::Highlights];

    pub fn label(self) -> &'static str {
        match self {
            ToneRange::Shadows => "Shadows",
            ToneRange::Midtones => "Midtones",
            ToneRange::Highlights => "Highlights",
        }
    }

    /// How much a pixel of `lightness` (0 to 1) is affected.
    fn weight(self, lightness: f32) -> f32 {
        match self {
            ToneRange::Shadows => (1.0 - lightness).powi(2),
            ToneRange::Midtones => 1.0 - (2.0 * lightness - 1.0).powi(2),
            ToneRange::Highlights => lightness.powi(2),
        }
    }
}

/// What a retouching stroke does, and how strongly (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Retouch {
    Smudge {
        strength: f32,
    },
    Blur {
        strength: f32,
    },
    Sharpen {
        strength: f32,
    },
    Dodge {
        exposure: f32,
        range: ToneRange,
    },
    Burn {
        exposure: f32,
        range: ToneRange,
    },
    /// Copies from `offset` away (source minus destination), at `opacity`.
    Clone {
        offset: Point,
        opacity: f32,
    },
}

/// Pixels picked up by the smudge: a square of premultiplied RGBA, and
/// where its centre was.
struct Patch {
    pixels: Vec<[f32; 4]>,
    size: u32,
    centre: (i64, i64),
}

/// One retouching stroke, from pen down to pen up.
pub struct RetouchStroke {
    settings: BrushSettings,
    kind: Retouch,
    path: DabPath,
    smudge: Option<Patch>,
    /// The canvas as it was when the stroke began, which Clone Stamp copies
    /// from, so a stroke never copies paint it has just put down.
    source: Canvas,
    /// Where it may change pixels, if not everywhere.
    selection: Option<Selection>,
}

impl RetouchStroke {
    pub fn new(settings: BrushSettings, kind: Retouch, canvas: &Canvas) -> Self {
        // Sharing the canvas's tiles, so this costs almost nothing.
        Self {
            settings,
            kind,
            path: DabPath::default(),
            smudge: None,
            source: canvas.clone(),
            selection: None,
        }
    }

    /// Changes pixels only within `selection`.
    pub fn within(mut self, selection: Option<Selection>) -> Self {
        self.selection = selection;
        self
    }

    /// Carries the stroke on to `at` (canvas pixels) with `pressure`.
    pub fn add_point(&mut self, canvas: &mut Canvas, at: Point, pressure: f32) {
        let mut dabs = Vec::new();
        self.path.advance(&self.settings, at, pressure, |at, pressure| dabs.push((at, pressure)));
        for (at, pressure) in dabs {
            self.dab(canvas, at, pressure);
        }
    }

    fn dab(&mut self, canvas: &mut Canvas, at: Point, pressure: f32) {
        let s = self.settings;
        let radius = s.radius(pressure);
        let flow = s.flow * if s.pressure_flow { pressure } else { 1.0 };
        // Blur and Sharpen look a few pixels past the dab's edge.
        let blur = blur_radius(radius);
        let margin = match self.kind {
            Retouch::Blur { .. } | Retouch::Sharpen { .. } => blur as i64,
            _ => 0,
        };
        let reach = radius.ceil() as i64 + 1 + margin;
        let (cx, cy) = (at.x.floor() as i64, at.y.floor() as i64);
        let (x0, y0, size) = (cx - reach, cy - reach, (reach * 2 + 1) as u32);
        let original = canvas.read_region(x0, y0, size, size);
        let mut pixels = to_floats(&original);

        // How much the dab covers each pixel of the square.
        let cover: Vec<f32> = (0..size * size)
            .map(|i| {
                let (x, y) = (x0 + (i % size) as i64, y0 + (i / size) as i64);
                let d = Point::new(x as f32 + 0.5, y as f32 + 0.5).distance(at);
                s.coverage(d, radius) * flow
            })
            .collect();

        match self.kind {
            Retouch::Smudge { strength } => {
                if let Some(patch) = &self.smudge {
                    for (i, pixel) in pixels.iter_mut().enumerate() {
                        let amount = cover[i] * strength;
                        let (x, y) = (x0 + (i as u32 % size) as i64, y0 + (i as u32 / size) as i64);
                        if let Some(carried) = patch.at(x - cx, y - cy).filter(|_| amount > 0.0) {
                            *pixel = mix(*pixel, carried, amount);
                        }
                    }
                }
                // Pick up what's here now, to put down at the next dab.
                self.smudge = Some(Patch { pixels: pixels.clone(), size, centre: (cx - x0, cy - y0) });
            }
            Retouch::Blur { strength } | Retouch::Sharpen { strength } => {
                let blurred = box_blur(&pixels, size, blur);
                let sharpen = matches!(self.kind, Retouch::Sharpen { .. });
                // Overlapping dabs sharpen the same pixels again and again,
                // and sharpening feeds on itself, so each dab does only a
                // little; at full strength it would soon overshoot into
                // noise.
                let strength = if sharpen { strength * SHARPEN_PER_DAB } else { strength };
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let amount = cover[i] * strength;
                    if amount > 0.0 {
                        let target = if sharpen { away(*pixel, blurred[i]) } else { blurred[i] };
                        *pixel = clamp_premultiplied(mix(*pixel, target, amount));
                    }
                }
            }
            Retouch::Dodge { exposure, range } | Retouch::Burn { exposure, range } => {
                let dodge = matches!(self.kind, Retouch::Dodge { .. });
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let a = pixel[3];
                    if cover[i] <= 0.0 || a <= 0.0 {
                        continue;
                    }
                    // Straight (not premultiplied) colour, 0–255.
                    let straight = [0, 1, 2].map(|c| pixel[c] * 255.0 / a);
                    let lightness = (straight[0] * 0.299 + straight[1] * 0.587 + straight[2] * 0.114) / 255.0;
                    let amount = cover[i] * exposure * range.weight(lightness.clamp(0.0, 1.0));
                    for c in 0..3 {
                        let v = straight[c];
                        let changed = if dodge { v + (255.0 - v) * amount } else { v * (1.0 - amount) };
                        pixel[c] = changed.clamp(0.0, 255.0) * a / 255.0;
                    }
                }
            }
            Retouch::Clone { offset, opacity } => {
                let (sx, sy) = (x0 + offset.x.round() as i64, y0 + offset.y.round() as i64);
                let from = to_floats(&self.source.read_region(sx, sy, size, size));
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let amount = cover[i] * opacity;
                    if amount > 0.0 {
                        *pixel = mix(*pixel, from[i], amount);
                    }
                }
            }
        }
        let mut changed = to_bytes(&pixels);
        if let Some(selection) = &self.selection {
            selection.limit(x0, y0, size, &original, &mut changed);
        }
        canvas.write_region(x0, y0, size, size, &changed);
    }
}

impl Patch {
    /// The picked-up pixel `dx`, `dy` from the patch's centre, if it has one
    /// there.
    fn at(&self, dx: i64, dy: i64) -> Option<[f32; 4]> {
        let (x, y) = (self.centre.0 + dx, self.centre.1 + dy);
        let inside = (0..self.size as i64).contains(&x) && (0..self.size as i64).contains(&y);
        inside.then(|| self.pixels[(y * self.size as i64 + x) as usize])
    }
}

/// The share of Sharpen's strength each dab applies.
const SHARPEN_PER_DAB: f32 = 0.2;

/// How far Blur and Sharpen look for neighbours: wider for bigger brushes,
/// so a big soft brush blurs broadly, but never very far, which would be
/// slow and wash colours out across the canvas.
fn blur_radius(brush_radius: f32) -> u32 {
    ((brush_radius * 0.12).round() as u32).clamp(1, 6)
}

/// Averages each pixel with its neighbours within `radius`, across then
/// down (a box blur, done as two quick passes).
fn box_blur(pixels: &[[f32; 4]], size: u32, radius: u32) -> Vec<[f32; 4]> {
    let size = size as i64;
    let radius = radius as i64;
    let pass = |source: &[[f32; 4]], horizontal: bool| -> Vec<[f32; 4]> {
        (0..size * size)
            .map(|i| {
                let (x, y) = (i % size, i / size);
                let mut sum = [0.0; 4];
                let mut count = 0.0;
                for k in -radius..=radius {
                    let (nx, ny) = if horizontal { (x + k, y) } else { (x, y + k) };
                    if (0..size).contains(&nx) && (0..size).contains(&ny) {
                        let p = source[(ny * size + nx) as usize];
                        for c in 0..4 {
                            sum[c] += p[c];
                        }
                        count += 1.0;
                    }
                }
                sum.map(|v| v / count)
            })
            .collect()
    };
    pass(&pass(pixels, true), false)
}

/// The colour as far past `pixel` from `average` as `pixel` is from it.
fn away(pixel: [f32; 4], average: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|c| pixel[c] + (pixel[c] - average[c]))
}

fn mix(from: [f32; 4], to: [f32; 4], amount: f32) -> [f32; 4] {
    std::array::from_fn(|c| from[c] + (to[c] - from[c]) * amount)
}

/// Keeps a premultiplied colour valid: alpha 0–255, and no channel
/// brighter than the alpha allows.
fn clamp_premultiplied(p: [f32; 4]) -> [f32; 4] {
    let a = p[3].clamp(0.0, 255.0);
    [p[0].clamp(0.0, a), p[1].clamp(0.0, a), p[2].clamp(0.0, a), a]
}

/// Bytes to colours to work on: still premultiplied, each 0 to 255.
fn to_floats(bytes: &[u8]) -> Vec<[f32; 4]> {
    bytes.chunks_exact(4).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]).collect()
}

fn to_bytes(pixels: &[[f32; 4]]) -> Vec<u8> {
    pixels.iter().flat_map(|p| p.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(size: f32) -> BrushSettings {
        BrushSettings { size, hardness: 1.0, pressure_size: false, ..BrushSettings::default() }
    }

    /// A canvas with its left half solid `left` and right half solid
    /// `right`, 100×40.
    fn halves(left: [u8; 4], right: [u8; 4]) -> Canvas {
        let mut canvas = Canvas::new(100, 40);
        let pixels: Vec<u8> = (0..100 * 40).flat_map(|i| if i % 100 < 50 { left } else { right }).collect();
        canvas.write_region(0, 0, 100, 40, &pixels);
        canvas
    }

    fn stroke(kind: Retouch, size: f32, canvas: &mut Canvas, points: &[(f32, f32)]) {
        let mut stroke = RetouchStroke::new(settings(size), kind, canvas);
        for &(x, y) in points {
            stroke.add_point(canvas, Point::new(x, y), 1.0);
        }
    }

    #[test]
    fn smudging_drags_colour_into_empty_space() {
        let mut canvas = halves([200, 0, 0, 255], [0; 4]);
        stroke(Retouch::Smudge { strength: 0.8 }, 16.0, &mut canvas, &[(40.0, 20.0), (70.0, 20.0)]);
        assert!(canvas.pixel(60, 20)[3] > 60, "red was pulled across: {:?}", canvas.pixel(60, 20));
    }

    #[test]
    fn blurring_softens_an_edge_and_sharpening_hardens_it() {
        let grey = |v: u8| [v, v, v, 255];
        let mut blurred = halves(grey(40), grey(200));
        // Along the edge, so it's blurred by many dabs.
        stroke(Retouch::Blur { strength: 1.0 }, 20.0, &mut blurred, &[(50.0, 5.0), (50.0, 35.0)]);
        let (left, right) = (blurred.pixel(49, 20)[0], blurred.pixel(50, 20)[0]);
        assert!(left > 40 && right < 200, "the edge softened: {left} {right}");

        // A soft, S-shaped edge from dark to light: sharpening steepens it,
        // darkening its darker side and lightening its lighter side.
        let mut soft = halves(grey(40), grey(200));
        for (x, v) in [(48, 60), (49, 120), (50, 180)] {
            soft.write_region(x, 0, 1, 40, &grey(v).repeat(40));
        }
        stroke(Retouch::Sharpen { strength: 1.0 }, 20.0, &mut soft, &[(49.0, 5.0), (49.0, 35.0)]);
        let (dark, light) = (soft.pixel(48, 20)[0], soft.pixel(50, 20)[0]);
        assert!(dark < 60 && light > 180, "the edge hardened: {dark} {light}");
    }

    #[test]
    fn dodge_lightens_and_burn_darkens() {
        let mut canvas = halves([128, 128, 128, 255], [128, 128, 128, 255]);
        let range = ToneRange::Midtones;
        stroke(Retouch::Dodge { exposure: 0.5, range }, 20.0, &mut canvas, &[(25.0, 20.0)]);
        stroke(Retouch::Burn { exposure: 0.5, range }, 20.0, &mut canvas, &[(75.0, 20.0)]);
        assert!(canvas.pixel(25, 20)[0] > 170, "{:?}", canvas.pixel(25, 20));
        assert!(canvas.pixel(75, 20)[0] < 90, "{:?}", canvas.pixel(75, 20));
        assert_eq!(canvas.pixel(25, 20)[3], 255, "alpha is untouched");
    }

    #[test]
    fn the_clone_stamp_copies_from_its_source() {
        let mut canvas = halves([0, 0, 0, 0], [0, 90, 0, 255]);
        // Paint on the left with what's 50 pixels to the right.
        let kind = Retouch::Clone { offset: Point::new(50.0, 0.0), opacity: 1.0 };
        stroke(kind, 10.0, &mut canvas, &[(20.0, 20.0)]);
        assert_eq!(canvas.pixel(20, 20), [0, 90, 0, 255]);
        assert_eq!(canvas.pixel(5, 20), [0; 4], "only under the dab");
    }

    #[test]
    fn retouching_empty_canvas_makes_no_tiles() {
        let mut canvas = Canvas::new(100, 100);
        stroke(Retouch::Blur { strength: 1.0 }, 30.0, &mut canvas, &[(10.0, 10.0), (90.0, 90.0)]);
        assert!(canvas.is_empty());
    }
}
