//! The retouching tools: Smudge, Blur, Sharpen, Dodge, Burn, Clone Stamp
//! and the Mixer Brush.
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
//! * **Mixer Brush** paints wet, as Photoshop's does: the brush carries a
//!   reservoir of paint that the canvas's colours mix into as it goes
//!   (*Mix*), smears what it passes over (*Wet*), and runs dry over the
//!   stroke unless it's loaded with plenty (*Load*).
//!
//! Each dab works on a small square of pixels around itself, read from the
//! canvas, changed, and written back. Pixels are premultiplied, which is
//! the right form for mixing (a transparent pixel adds no colour).

use crate::brush::{BrushSettings, DabPath};
use crate::color::Color;
use crate::geometry::Point;
use crate::paint::Canvas;
use crate::parallel::in_bands;
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
            ToneRange::Shadows => crate::i18n::tr_later("Shadows"),
            ToneRange::Midtones => crate::i18n::tr_later("Midtones"),
            ToneRange::Highlights => crate::i18n::tr_later("Highlights"),
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
    /// Paints `color`, wet: each 0 to 1.
    Mixer {
        color: Color,
        /// How much of the canvas the brush picks up and smears.
        wet: f32,
        /// How much paint the brush holds: little runs dry soon.
        load: f32,
        /// How much the picked-up colour mixes into the brush's paint.
        mix: f32,
    },
}

/// Pixels picked up by the smudge: a square of premultiplied RGBA, and
/// where its centre was.
struct Patch {
    pixels: Vec<u8>,
    size: u32,
    centre: (i64, i64),
}

/// One retouching stroke, from pen down to pen up.
pub struct RetouchStroke {
    settings: BrushSettings,
    kind: Retouch,
    path: DabPath,
    smudge: Option<Patch>,
    /// Working space for a dab's pixels, kept from dab to dab: a big
    /// brush's square is a hundred megabytes of colours, slow to allocate
    /// afresh every time.
    scratch: Scratch,
    /// The Mixer Brush's paint (premultiplied, 0 to 255) and how much of
    /// it is left (0 to 1).
    reservoir: Option<([f32; 4], f32)>,
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
            scratch: Scratch::default(),
            reservoir: None,
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
        let s = &self.settings;
        let radius = s.radius(pressure);
        let flow = s.flow_at(pressure);
        // Blur and Sharpen look a few pixels past the dab's edge.
        let blur = blur_radius(radius);
        let margin = match self.kind {
            Retouch::Blur { .. } | Retouch::Sharpen { .. } => blur as i64,
            _ => 0,
        };
        let reach = radius.ceil() as i64 + 1 + margin;
        let (cx, cy) = (at.x.floor() as i64, at.y.floor() as i64);
        let (x0, y0, size) = (cx - reach, cy - reach, (reach * 2 + 1) as u32);
        let row = size as usize;
        let scratch = &mut self.scratch;
        canvas.read_region_into(x0, y0, size, size, &mut scratch.original);
        scratch.changed.clone_from(&scratch.original);
        // How much the dab covers the pixel at index `i` of the square.
        let cover = |i: usize| {
            let (x, y) = (x0 + (i % row) as i64, y0 + (i / row) as i64);
            let d = Point::new(x as f32 + 0.5, y as f32 + 0.5).distance(at);
            s.coverage(d, radius) * flow
        };
        // Each pixel of the square is changed on its own (reading only
        // copies), so the square goes in bands across the cores. `change`
        // gets a pixel's index and colour and gives its new colour, if any.
        let each = |bytes: &mut [u8], change: &(dyn Fn(usize, [f32; 4]) -> Option<[f32; 4]> + Sync)| {
            in_bands(bytes, row * 4, |first, band| {
                for (k, pixel) in band.chunks_exact_mut(4).enumerate() {
                    if let Some(new) = change(first * row + k, colour(pixel)) {
                        store(pixel, new);
                    }
                }
            });
        };

        match self.kind {
            Retouch::Smudge { strength } => {
                if let Some(patch) = &self.smudge {
                    each(&mut scratch.changed, &|i, pixel| {
                        let amount = cover(i) * strength;
                        let (x, y) = (x0 + (i % row) as i64, y0 + (i / row) as i64);
                        let carried = patch.at(x - cx, y - cy).filter(|_| amount > 0.0)?;
                        Some(mix(pixel, carried, amount))
                    });
                }
                // Pick up what's here now, to put down at the next dab (in
                // the last patch's room).
                let mut carried = self.smudge.take().map(|p| p.pixels).unwrap_or_default();
                carried.clone_from(&scratch.changed);
                self.smudge = Some(Patch { pixels: carried, size, centre: (cx - x0, cy - y0) });
            }
            Retouch::Blur { strength } | Retouch::Sharpen { strength } => {
                blur_across(&scratch.original, row, blur as usize, &mut scratch.across);
                let across = &scratch.across;
                let sharpen = matches!(self.kind, Retouch::Sharpen { .. });
                // Overlapping dabs sharpen the same pixels again and again,
                // and sharpening feeds on itself, so each dab does only a
                // little; at full strength it would soon overshoot into
                // noise.
                let strength = if sharpen { strength * SHARPEN_PER_DAB } else { strength };
                // Each band blurs its rows down as it goes and uses them
                // straight away, so the blurred square is never kept whole.
                in_bands(&mut scratch.changed, row * 4, |first, band| {
                    blur_down(
                        across,
                        row,
                        blur as usize,
                        first..first + band.len() / (row * 4),
                        |y, blurred| {
                            let line = &mut band[(y - first) * row * 4..][..row * 4];
                            for (x, pixel) in line.chunks_exact_mut(4).enumerate() {
                                let amount = cover(y * row + x) * strength;
                                if amount > 0.0 {
                                    let old = colour(pixel);
                                    let target = if sharpen { away(old, blurred[x]) } else { blurred[x] };
                                    store(pixel, clamp_premultiplied(mix(old, target, amount)));
                                }
                            }
                        },
                    );
                });
            }
            Retouch::Dodge { exposure, range } | Retouch::Burn { exposure, range } => {
                let dodge = matches!(self.kind, Retouch::Dodge { .. });
                each(&mut scratch.changed, &|i, mut pixel| {
                    let a = pixel[3];
                    let covered = if a > 0.0 { cover(i) } else { 0.0 };
                    if covered <= 0.0 {
                        return None;
                    }
                    // Straight (not premultiplied) colour, 0–255.
                    let straight = [0, 1, 2].map(|c| pixel[c] * 255.0 / a);
                    let lightness = (straight[0] * 0.299 + straight[1] * 0.587 + straight[2] * 0.114) / 255.0;
                    let amount = covered * exposure * range.weight(lightness.clamp(0.0, 1.0));
                    for c in 0..3 {
                        let v = straight[c];
                        let changed = if dodge { v + (255.0 - v) * amount } else { v * (1.0 - amount) };
                        pixel[c] = changed.clamp(0.0, 255.0) * a / 255.0;
                    }
                    Some(pixel)
                });
            }
            Retouch::Clone { offset, opacity } => {
                let (sx, sy) = (x0 + offset.x.round() as i64, y0 + offset.y.round() as i64);
                self.source.read_region_into(sx, sy, size, size, &mut scratch.source);
                let from = &scratch.source;
                each(&mut scratch.changed, &|i, pixel| {
                    let amount = cover(i) * opacity;
                    (amount > 0.0).then(|| mix(pixel, colour(&from[i * 4..]), amount))
                });
            }
            Retouch::Mixer { color, wet, load, mix: mixing } => {
                let alpha = color.a as f32 / 255.0;
                let loaded = [color.r, color.g, color.b].map(|v| v as f32 * alpha);
                let (paint, left) =
                    self.reservoir.get_or_insert(([loaded[0], loaded[1], loaded[2], color.a as f32], 1.0));
                // The canvas's colour under the dab mixes into the brush's
                // paint, as much as Mix says and Wet lets it pick up.
                if let Some(under) = average_under(&scratch.original, &cover) {
                    *paint = mix(*paint, under, wet * mixing * MIX_PER_DAB);
                }
                // The paint runs out over the stroke, sooner with less
                // loaded; a wet brush still smears when it's dry.
                *left *= 1.0 - (1.0 - load) * DRY_PER_DAB;
                let (paint, strength) = (*paint, left.max(wet));
                let carried = self.smudge.as_ref();
                each(&mut scratch.changed, &|i, pixel| {
                    let amount = cover(i) * strength;
                    if amount <= 0.0 {
                        return None;
                    }
                    // Wet, the brush lays down what it picked up last dab
                    // as much as its own paint, so colours smear.
                    let (x, y) = (x0 + (i % row) as i64, y0 + (i / row) as i64);
                    let picked = carried.and_then(|p| p.at(x - cx, y - cy)).unwrap_or(paint);
                    Some(mix(pixel, mix(paint, picked, wet), amount.min(1.0)))
                });
                let mut carried = self.smudge.take().map(|p| p.pixels).unwrap_or_default();
                carried.clone_from(&scratch.changed);
                self.smudge = Some(Patch { pixels: carried, size, centre: (cx - x0, cy - y0) });
            }
        }
        if let Some(selection) = &self.selection {
            selection.limit(x0, y0, size, &scratch.original, &mut scratch.changed);
        }
        canvas.write_region(x0, y0, size, size, &scratch.changed);
    }
}

impl Patch {
    /// The picked-up pixel `dx`, `dy` from the patch's centre, if it has one
    /// there.
    fn at(&self, dx: i64, dy: i64) -> Option<[f32; 4]> {
        let (x, y) = (self.centre.0 + dx, self.centre.1 + dy);
        let inside = (0..self.size as i64).contains(&x) && (0..self.size as i64).contains(&y);
        inside.then(|| colour(&self.pixels[(y * self.size as i64 + x) as usize * 4..]))
    }
}

/// How much of the canvas's colour a fully wet, fully mixing Mixer Brush
/// takes into its paint at each dab.
const MIX_PER_DAB: f32 = 0.25;

/// How much of a Mixer Brush's paint an empty load uses up at each dab.
const DRY_PER_DAB: f32 = 0.08;

/// The average colour under a dab, weighting each pixel by how much the dab
/// `cover`s it; `None` where it covers nothing. A sample of the square is
/// plenty for an average, and quick at any size.
fn average_under(bytes: &[u8], cover: &impl Fn(usize) -> f32) -> Option<[f32; 4]> {
    let count = bytes.len() / 4;
    let step = (count / 4096).max(1);
    let mut sum = [0.0f32; 4];
    let mut weight = 0.0;
    for i in (0..count).step_by(step) {
        let w = cover(i);
        if w > 0.0 {
            let c = colour(&bytes[i * 4..]);
            for k in 0..4 {
                sum[k] += c[k] * w;
            }
            weight += w;
        }
    }
    (weight > 0.0).then(|| sum.map(|v| v / weight))
}

/// The share of Sharpen's strength each dab applies.
const SHARPEN_PER_DAB: f32 = 0.2;

/// How far Blur and Sharpen look for neighbours: wider for bigger brushes,
/// so a big soft brush blurs broadly, but never very far, which would be
/// slow and wash colours out across the canvas.
fn blur_radius(brush_radius: f32) -> u32 {
    ((brush_radius * 0.12).round() as u32).clamp(1, 6)
}

/// Working space for a dab, reused (see `RetouchStroke::scratch`).
#[derive(Default)]
struct Scratch {
    /// The square under the dab as it was, and as it's changed.
    original: Vec<u8>,
    changed: Vec<u8>,
    /// Clone Stamp's source square.
    source: Vec<u8>,
    /// The blur's first pass.
    across: Vec<[f32; 4]>,
}

/// A box blur's first pass: each pixel of a square `size` across averaged
/// with its neighbours within `radius` along its row, into `across`. A
/// running sum adds the pixel coming into the window and drops the one
/// leaving, so a pixel costs the same however wide the blur. Rows go in
/// bands across the cores.
fn blur_across(pixels: &[u8], size: usize, radius: usize, across: &mut Vec<[f32; 4]>) {
    across.resize(pixels.len() / 4, [0.0; 4]);
    in_bands(across, size, |first, band| {
        for (r, out) in band.chunks_mut(size).enumerate() {
            let source = &pixels[(first + r) * size * 4..][..size * 4];
            let at = |x: usize| colour(&source[x * 4..]);
            let mut sum = [0.0; 4];
            for x in 0..radius.min(size) {
                add(&mut sum, &at(x), 1.0);
            }
            for (x, o) in out.iter_mut().enumerate() {
                if x + radius < size {
                    add(&mut sum, &at(x + radius), 1.0);
                }
                if x > radius {
                    add(&mut sum, &at(x - radius - 1), -1.0);
                }
                let count = (x + radius).min(size - 1) - x.saturating_sub(radius) + 1;
                *o = sum.map(|v| v / count as f32);
            }
        }
    });
}

/// The second pass, for the rows `rows`: each row of `across` averaged with
/// the rows within `radius` of it, handed to `row_done` with its number.
/// A running sum down each column adds the row coming into the window and
/// drops the one leaving.
fn blur_down(
    across: &[[f32; 4]],
    size: usize,
    radius: usize,
    rows: std::ops::Range<usize>,
    mut row_done: impl FnMut(usize, &[[f32; 4]]),
) {
    let source = |y: usize| &across[y * size..][..size];
    let mut sum = vec![[0.0f32; 4]; size];
    let mut blurred = vec![[0.0f32; 4]; size];
    for y in rows.start.saturating_sub(radius)..=(rows.start + radius).min(size - 1) {
        for (s, p) in sum.iter_mut().zip(source(y)) {
            add(s, p, 1.0);
        }
    }
    for y in rows.clone() {
        if y > rows.start {
            if y + radius < size {
                for (s, p) in sum.iter_mut().zip(source(y + radius)) {
                    add(s, p, 1.0);
                }
            }
            if y > radius {
                for (s, p) in sum.iter_mut().zip(source(y - radius - 1)) {
                    add(s, p, -1.0);
                }
            }
        }
        let count = ((y + radius).min(size - 1) - y.saturating_sub(radius) + 1) as f32;
        for (b, s) in blurred.iter_mut().zip(&sum) {
            *b = s.map(|v| v / count);
        }
        row_done(y, &blurred);
    }
}

/// Adds `p` (or takes it away, with a `sign` of −1) to a running `sum`.
fn add(sum: &mut [f32; 4], p: &[f32; 4], sign: f32) {
    for c in 0..4 {
        sum[c] += p[c] * sign;
    }
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

/// A pixel's bytes as a colour to work on: still premultiplied, each 0 to
/// 255.
fn colour(bytes: &[u8]) -> [f32; 4] {
    [bytes[0] as f32, bytes[1] as f32, bytes[2] as f32, bytes[3] as f32]
}

/// A colour back into a pixel's bytes.
fn store(pixel: &mut [u8], value: [f32; 4]) {
    for (byte, v) in pixel.iter_mut().zip(value) {
        *byte = v.round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blur_averages_each_pixels_neighbours() {
        let size = 9;
        let pixels: Vec<u8> =
            (0..size * size).flat_map(|i| [(i * 37 % 255) as u8, (i % 7) as u8, 3, 255]).collect();
        let mut across = Vec::new();
        blur_across(&pixels, size, 2, &mut across);
        let mut blurred = Vec::new();
        blur_down(&across, size, 2, 0..size, |_, row| blurred.extend_from_slice(row));
        // In bands, as the cores share it, it's the same.
        let mut banded = Vec::new();
        for rows in [0..4, 4..size] {
            blur_down(&across, size, 2, rows, |_, row| banded.extend_from_slice(row));
        }
        // (Up to rounding: the sums are added in a different order.)
        assert!(banded.iter().flatten().zip(blurred.iter().flatten()).all(|(a, b)| (a - b).abs() < 1e-3));
        // Straight from the definition: the average over the pixels within
        // 2 across, then over those within 2 down.
        let near = |c: usize| (c.saturating_sub(2))..=(c + 2).min(size - 1);
        for (i, got) in blurred.iter().enumerate() {
            let (x, y) = (i % size, i / size);
            let mut sum = [0.0f32; 4];
            let mut count = 0.0;
            for ny in near(y) {
                let row: Vec<usize> = near(x).collect();
                for c in 0..4 {
                    sum[c] += row.iter().map(|&nx| pixels[(ny * size + nx) * 4 + c] as f32).sum::<f32>()
                        / row.len() as f32;
                }
                count += 1.0;
            }
            for c in 0..4 {
                assert!((got[c] - sum[c] / count).abs() < 1e-3, "pixel {i} channel {c}");
            }
        }
    }

    fn settings(size: f32) -> BrushSettings {
        BrushSettings { size, hardness: 1.0, shape: Default::default(), ..BrushSettings::default() }
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
    fn a_dry_mixer_brush_paints_its_colour() {
        let mut canvas = halves([0; 4], [0; 4]);
        let blue = Color::rgb(0, 0, 255);
        stroke(
            Retouch::Mixer { color: blue, wet: 0.0, load: 1.0, mix: 0.0 },
            16.0,
            &mut canvas,
            &[(20.0, 20.0), (80.0, 20.0)],
        );
        assert_eq!(canvas.pixel(50, 20), [0, 0, 255, 255]);
    }

    #[test]
    fn a_wet_mixer_brush_picks_up_and_carries_the_canvass_colour() {
        // Blue paint dragged from red into empty canvas comes out purple.
        let mut canvas = halves([255, 0, 0, 255], [0; 4]);
        let blue = Color::rgb(0, 0, 255);
        let mixer = Retouch::Mixer { color: blue, wet: 0.8, load: 1.0, mix: 1.0 };
        stroke(mixer, 16.0, &mut canvas, &[(10.0, 20.0), (45.0, 20.0), (90.0, 20.0)]);
        let [r, _, b, a] = canvas.pixel(75, 20);
        // Thinner, too: wet, it smears the empty canvas in.
        assert!(a > 120 && r > 40 && b > 40, "both colours in it: {:?}", canvas.pixel(75, 20));
    }

    #[test]
    fn a_lightly_loaded_mixer_brush_runs_dry() {
        let mut canvas = Canvas::new(400, 40);
        let blue = Color::rgb(0, 0, 255);
        let points: Vec<(f32, f32)> = (0..=38).map(|i| (10.0 + i as f32 * 10.0, 20.0)).collect();
        stroke(Retouch::Mixer { color: blue, wet: 0.0, load: 0.1, mix: 0.0 }, 16.0, &mut canvas, &points);
        let (start, end) = (canvas.pixel(15, 20)[3], canvas.pixel(385, 20)[3]);
        assert!(start > 200 && end < start / 2, "it fades from {start} to {end}");
    }

    #[test]
    fn retouching_empty_canvas_makes_no_tiles() {
        let mut canvas = Canvas::new(100, 100);
        stroke(Retouch::Blur { strength: 1.0 }, 30.0, &mut canvas, &[(10.0, 10.0), (90.0, 90.0)]);
        assert!(canvas.is_empty());
    }
}
