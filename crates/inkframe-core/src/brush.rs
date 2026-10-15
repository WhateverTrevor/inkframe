//! The brush engine: turning a pen's path into paint on a canvas.
//!
//! A stroke is drawn as a row of overlapping round **dabs**, one every few
//! pixels along the path (the `spacing`), each sized and weighted by the
//! pen's pressure at that point. Photoshop's brush works this way, and its
//! settings mean the same here:
//!
//! * **Hardness:** how soft a dab's edge is. 100% is a crisp disc; 0% fades
//!   from the middle out.
//! * **Flow:** how much each dab adds. Low flow builds up gradually where
//!   dabs overlap, like an airbrush.
//! * **Opacity:** the most the whole stroke can cover. However many dabs
//!   overlap, one stroke never gets more opaque than this, so a stroke at
//!   50% stays an even 50% even where it crosses itself.
//!
//! That last rule is why a stroke isn't painted straight onto the canvas.
//! Dabs pile up in the stroke's own coverage mask (0 to 1 per pixel), and
//! the canvas is redrawn from the tiles as they were before the stroke,
//! with the mask's colour on top. The eraser is the same stroke, taking
//! away instead of adding.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::color::Color;
use crate::geometry::Point;
use crate::paint::{Canvas, TILE};
use crate::selection::Selection;

/// How a brush paints. Sizes are in canvas pixels; the rest are 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushSettings {
    /// The dab's diameter at full pressure.
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
    pub flow: f32,
    /// Distance between dabs, as a fraction of their diameter.
    pub spacing: f32,
    /// Whether pen pressure changes the size (from `min_size` of it, at the
    /// lightest touch, up to all of it).
    pub pressure_size: bool,
    pub min_size: f32,
    /// Whether pen pressure changes the flow.
    pub pressure_flow: bool,
    /// How much a paper texture breaks up the paint, 0 to 1: 0 is smooth,
    /// higher catches only the paper's raised grain, like chalk or a dry
    /// brush. The texture stays put on the canvas, as paper would.
    pub grain: f32,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            size: 24.0,
            hardness: 0.8,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.12,
            pressure_size: true,
            min_size: 0.15,
            pressure_flow: false,
            grain: 0.0,
        }
    }
}

impl BrushSettings {
    /// Ready-made brushes, by name: a starting point to adjust from.
    pub fn presets() -> [(&'static str, BrushSettings); 6] {
        let base = BrushSettings::default();
        [
            ("Hard Round", BrushSettings { hardness: 1.0, spacing: 0.1, ..base }),
            ("Soft Round", BrushSettings { size: 60.0, hardness: 0.0, spacing: 0.12, ..base }),
            (
                "Airbrush",
                BrushSettings {
                    size: 90.0,
                    hardness: 0.0,
                    flow: 0.08,
                    spacing: 0.08,
                    pressure_size: false,
                    pressure_flow: true,
                    ..base
                },
            ),
            (
                "Inking Pen",
                BrushSettings { size: 8.0, hardness: 0.95, spacing: 0.05, min_size: 0.05, ..base },
            ),
            ("Chalk", BrushSettings { size: 30.0, hardness: 0.7, spacing: 0.15, grain: 0.45, ..base }),
            ("Dry Brush", BrushSettings { size: 50.0, hardness: 0.5, flow: 0.8, grain: 0.6, ..base }),
        ]
    }

    /// The dab's radius at `pressure`, never under half a pixel.
    pub fn radius(&self, pressure: f32) -> f32 {
        let scale = if self.pressure_size { self.min_size + (1.0 - self.min_size) * pressure } else { 1.0 };
        (self.size * scale / 2.0).max(0.5)
    }

    /// Distance to the next dab: `spacing` of the dab's size, but never
    /// under half a pixel, or tiny brushes would stamp thousands of dabs.
    pub fn step(&self, pressure: f32) -> f32 {
        (self.radius(pressure) * 2.0 * self.spacing).max(0.5)
    }

    /// How much a dab of `radius` covers a pixel whose centre is `distance`
    /// from the dab's: fully inside `radius - soft`, fading to nothing half
    /// a pixel past `radius`. The soft part is at least a pixel, so a hard
    /// brush still gets a smooth (anti-aliased) edge, and a 0% one fades all
    /// the way from its centre.
    pub fn coverage(&self, distance: f32, radius: f32) -> f32 {
        let soft = ((1.0 - self.hardness) * radius).max(1.0);
        let t = ((radius + 0.5 - distance) / (soft + 0.5)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// How much paint the paper's grain lets through at pixel (`x`, `y`).
    pub fn grain_at(&self, x: u32, y: u32) -> f32 {
        if self.grain <= 0.0 {
            return 1.0;
        }
        // Paint catches where the paper is higher than the grain setting.
        let height = paper(x, y);
        ((height - self.grain) / 0.25 + 0.5).clamp(0.0, 1.0)
    }
}

/// A paper texture: smooth random bumps a few pixels across, the same at
/// the same spot every time, 0 (low) to 1 (high).
fn paper(x: u32, y: u32) -> f32 {
    // Value noise: random heights on a 3-pixel grid, blended between.
    const CELL: f32 = 3.0;
    let (fx, fy) = (x as f32 / CELL, y as f32 / CELL);
    let (cx, cy) = (fx.floor() as u32, fy.floor() as u32);
    let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(tx), smooth(ty));
    let h = |x: u32, y: u32| {
        // A small integer hash: stable, and good enough to look random.
        let mut n = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
        n ^= n >> 15;
        n = n.wrapping_mul(0x2C1B_3C6D);
        n ^= n >> 12;
        (n & 0xFFFF) as f32 / 65535.0
    };
    let top = h(cx, cy) + (h(cx + 1, cy) - h(cx, cy)) * sx;
    let bottom = h(cx, cy + 1) + (h(cx + 1, cy + 1) - h(cx, cy + 1)) * sx;
    top + (bottom - top) * sy
}

/// Where a stroke's dabs go: one at the start, then one every `step` along
/// the pen's path, with the pressure in between found by blending. Every
/// stroke tool (brush, eraser, smudge, clone stamp…) places dabs this way.
#[derive(Debug, Clone, Copy, Default)]
pub struct DabPath {
    last: Option<(Point, f32)>,
    until_next: f32,
}

impl DabPath {
    /// Carries the path on to `at` with `pressure`, calling `dab` with the
    /// centre and pressure of each dab due on the way.
    pub fn advance(
        &mut self,
        settings: &BrushSettings,
        at: Point,
        pressure: f32,
        mut dab: impl FnMut(Point, f32),
    ) {
        let pressure = pressure.clamp(0.0, 1.0);
        match self.last {
            None => {
                dab(at, pressure);
                self.until_next = settings.step(pressure);
            }
            Some((from, from_pressure)) => {
                let length = from.distance(at);
                let mut travelled = self.until_next;
                while travelled <= length {
                    let t = if length > 0.0 { travelled / length } else { 1.0 };
                    let p = from_pressure + (pressure - from_pressure) * t;
                    dab(from.lerp(at, t), p);
                    travelled += settings.step(p);
                }
                self.until_next = travelled - length;
            }
        }
        self.last = Some((at, pressure));
    }
}

/// What a stroke does to the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrushMode {
    Paint(Color),
    Erase,
}

/// One stroke, from pen down to pen up.
pub struct BrushStroke {
    settings: BrushSettings,
    mode: BrushMode,
    /// Each touched tile as it was before the stroke (`None`: it was empty).
    before: BTreeMap<(u32, u32), Option<Arc<Vec<u8>>>>,
    /// How much of the stroke covers each pixel of each touched tile.
    coverage: BTreeMap<(u32, u32), Vec<f32>>,
    /// Tiles whose coverage changed since the canvas was last redrawn.
    dirty: BTreeSet<(u32, u32)>,
    path: DabPath,
    /// Where it may paint, if not everywhere.
    selection: Option<Selection>,
}

impl BrushStroke {
    pub fn new(settings: BrushSettings, mode: BrushMode) -> Self {
        Self {
            settings,
            mode,
            before: BTreeMap::new(),
            coverage: BTreeMap::new(),
            dirty: BTreeSet::new(),
            path: DabPath::default(),
            selection: None,
        }
    }

    /// Paints only within `selection` (soft edges painting partly).
    pub fn within(mut self, selection: Option<Selection>) -> Self {
        self.selection = selection;
        self
    }

    /// Carries the stroke on to `at` (canvas pixels) with `pressure` (0 to
    /// 1; 1 for a mouse), laying dabs along the way, and updates `canvas`.
    pub fn add_point(&mut self, canvas: &mut Canvas, at: Point, pressure: f32) {
        let mut dabs = Vec::new();
        self.path.advance(&self.settings, at, pressure, |at, pressure| dabs.push((at, pressure)));
        for (at, pressure) in dabs {
            self.dab(canvas, at, pressure);
        }
        self.redraw(canvas);
    }

    /// Adds one dab centred on `at` to the stroke's coverage.
    fn dab(&mut self, canvas: &Canvas, at: Point, pressure: f32) {
        let s = self.settings;
        let radius = s.radius(pressure);
        let flow = s.flow * if s.pressure_flow { pressure } else { 1.0 };
        let reach = radius + 1.0;
        let (x0, y0) = ((at.x - reach).floor().max(0.0) as u32, (at.y - reach).floor().max(0.0) as u32);
        let x1 = ((at.x + reach).ceil() as u32).min(canvas.width);
        let y1 = ((at.y + reach).ceil() as u32).min(canvas.height);
        if at.x + reach < 0.0 || at.y + reach < 0.0 || x0 >= x1 || y0 >= y1 {
            return;
        }
        for ty in y0 / TILE..=(y1 - 1) / TILE {
            for tx in x0 / TILE..=(x1 - 1) / TILE {
                let tile = (tx, ty);
                self.before.entry(tile).or_insert_with(|| canvas.shared_tile(tile));
                let coverage = self.coverage.entry(tile).or_insert_with(|| vec![0.0; (TILE * TILE) as usize]);
                let (left, top) = (tx * TILE, ty * TILE);
                for y in y0.max(top)..y1.min(top + TILE) {
                    for x in x0.max(left)..x1.min(left + TILE) {
                        // Measured to the pixel's centre.
                        let d = Point::new(x as f32 + 0.5, y as f32 + 0.5).distance(at);
                        let cover = s.coverage(d, radius);
                        if cover <= 0.0 {
                            continue;
                        }
                        let selected = self
                            .selection
                            .as_ref()
                            .map_or(1.0, |sel| sel.at(x as i64, y as i64) as f32 / 255.0);
                        let amount = cover * flow * s.grain_at(x, y) * selected;
                        let c = &mut coverage[((y - top) * TILE + x - left) as usize];
                        *c += amount * (1.0 - *c);
                    }
                }
                self.dirty.insert(tile);
            }
        }
    }

    /// Redraws the tiles the stroke changed: as they were before it, with
    /// the stroke on top (or taken out, erasing).
    fn redraw(&mut self, canvas: &mut Canvas) {
        let opacity = self.settings.opacity.clamp(0.0, 1.0);
        for tile in std::mem::take(&mut self.dirty) {
            let coverage = &self.coverage[&tile];
            let mut pixels = match &self.before[&tile] {
                Some(before) => before.as_ref().clone(),
                None => vec![0; (TILE * TILE * 4) as usize],
            };
            for (pixel, &c) in pixels.chunks_exact_mut(4).zip(coverage) {
                let alpha = c * opacity;
                if alpha <= 0.0 {
                    continue;
                }
                match self.mode {
                    BrushMode::Paint(color) => {
                        // Premultiplied "source over": the brush colour at
                        // `alpha`, then what was there, showing through.
                        let a = alpha * color.a as f32 / 255.0;
                        let src = [color.r, color.g, color.b].map(|v| v as f32 * a);
                        for (channel, s) in pixel[..3].iter_mut().zip(src) {
                            *channel = (s + *channel as f32 * (1.0 - a)).round() as u8;
                        }
                        pixel[3] = (a * 255.0 + pixel[3] as f32 * (1.0 - a)).round() as u8;
                    }
                    BrushMode::Erase => {
                        for channel in pixel.iter_mut() {
                            *channel = (*channel as f32 * (1.0 - alpha)).round() as u8;
                        }
                    }
                }
            }
            canvas.set_tile(tile, pixels);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> BrushMode {
        BrushMode::Paint(Color::rgb(255, 0, 0))
    }

    fn line(settings: BrushSettings, mode: BrushMode, canvas: &mut Canvas, from: Point, to: Point) {
        let mut stroke = BrushStroke::new(settings, mode);
        stroke.add_point(canvas, from, 1.0);
        stroke.add_point(canvas, to, 1.0);
    }

    #[test]
    fn a_dab_paints_its_middle_solid_and_fades_at_the_edge() {
        let mut canvas = Canvas::new(100, 100);
        let settings = BrushSettings { size: 20.0, hardness: 0.0, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, red());
        stroke.add_point(&mut canvas, Point::new(50.0, 50.0), 1.0);
        // Nearly solid in the middle (a 0% brush is only completely solid
        // at its exact centre, and this pixel's centre is just off it).
        let [r, g, b, a] = canvas.pixel(50, 50);
        assert!(a > 245 && r == a && g == 0 && b == 0, "{:?}", [r, g, b, a]);
        let edge = canvas.pixel(57, 50)[3];
        assert!(edge > 0 && edge < 200, "soft edge alpha {edge}");
        assert_eq!(canvas.pixel(65, 50), [0; 4]);
    }

    #[test]
    fn a_stroke_is_continuous_between_points() {
        let mut canvas = Canvas::new(300, 100);
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 50.0), Point::new(290.0, 50.0));
        // Unbroken along its length, across the tile boundary at x = 256.
        for x in 10..290 {
            assert!(canvas.pixel(x, 50)[3] > 200, "gap at x = {x}");
        }
    }

    #[test]
    fn opacity_caps_a_stroke_that_crosses_itself() {
        let mut canvas = Canvas::new(100, 100);
        let settings = BrushSettings { opacity: 0.5, hardness: 1.0, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, red());
        for p in [(20.0, 50.0), (80.0, 50.0), (20.0, 50.0), (80.0, 50.0)] {
            stroke.add_point(&mut canvas, Point::new(p.0, p.1), 1.0);
        }
        let alpha = canvas.pixel(50, 50)[3];
        assert!((126..=129).contains(&alpha), "alpha {alpha}");
    }

    #[test]
    fn low_flow_builds_up_where_dabs_overlap() {
        let mut canvas = Canvas::new(100, 100);
        let settings =
            BrushSettings { flow: 0.1, hardness: 1.0, pressure_size: false, ..BrushSettings::default() };
        line(settings, red(), &mut canvas, Point::new(50.0, 50.0), Point::new(50.5, 50.0));
        let light = canvas.pixel(50, 50)[3];
        let mut canvas = Canvas::new(100, 100);
        line(settings, red(), &mut canvas, Point::new(40.0, 50.0), Point::new(60.0, 50.0));
        let built_up = canvas.pixel(50, 50)[3];
        assert!(built_up > light * 3, "{light} then {built_up}");
    }

    #[test]
    fn pressure_makes_lighter_touches_thinner() {
        let mut canvas = Canvas::new(200, 100);
        let settings = BrushSettings { size: 40.0, hardness: 1.0, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, red());
        stroke.add_point(&mut canvas, Point::new(20.0, 50.0), 0.1);
        stroke.add_point(&mut canvas, Point::new(180.0, 50.0), 1.0);
        let height_at = |x: i64| (0..100).filter(|&y| canvas.pixel(x, y)[3] > 128).count();
        assert!(height_at(30) < height_at(170) / 2, "{} vs {}", height_at(30), height_at(170));
    }

    #[test]
    fn erasing_takes_paint_away() {
        let mut canvas = Canvas::new(100, 100);
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 50.0), Point::new(90.0, 50.0));
        line(
            BrushSettings::default(),
            BrushMode::Erase,
            &mut canvas,
            Point::new(50.0, 10.0),
            Point::new(50.0, 90.0),
        );
        assert_eq!(canvas.pixel(50, 50), [0; 4]);
        assert_eq!(canvas.pixel(20, 50)[3], 255);
    }

    #[test]
    fn a_stroke_leaves_the_tiles_it_didnt_touch_shared() {
        let mut canvas = Canvas::new(600, 100);
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 50.0), Point::new(20.0, 50.0));
        line(BrushSettings::default(), red(), &mut canvas, Point::new(500.0, 50.0), Point::new(520.0, 50.0));
        let before = canvas.clone();
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 80.0), Point::new(20.0, 80.0));
        // Only tile (0, 0) differs; the right-hand one is still the same.
        assert_ne!(before, canvas);
        assert!(std::ptr::eq(before.tile((1, 0)).unwrap(), canvas.tile((1, 0)).unwrap()));
    }

    #[test]
    fn grain_breaks_paint_up_where_the_paper_is_low() {
        let mut canvas = Canvas::new(200, 100);
        let settings = BrushSettings { size: 60.0, grain: 0.5, hardness: 1.0, ..BrushSettings::default() };
        line(settings, red(), &mut canvas, Point::new(20.0, 50.0), Point::new(180.0, 50.0));
        let row: Vec<u8> = (30..170).map(|x| canvas.pixel(x, 50)[3]).collect();
        assert!(row.iter().any(|&a| a > 200), "the paper's peaks catch paint");
        assert!(row.iter().any(|&a| a < 60), "its hollows don't");
    }

    #[test]
    fn a_selection_keeps_paint_inside_it() {
        let mut canvas = Canvas::new(100, 100);
        let selection = Selection::rectangle(100, 100, Point::new(0.0, 0.0), Point::new(50.0, 100.0));
        let mut stroke = BrushStroke::new(BrushSettings::default(), red()).within(Some(selection));
        stroke.add_point(&mut canvas, Point::new(10.0, 50.0), 1.0);
        stroke.add_point(&mut canvas, Point::new(90.0, 50.0), 1.0);
        assert_eq!(canvas.pixel(40, 50)[3], 255);
        assert_eq!(canvas.pixel(60, 50), [0; 4]);
    }

    #[test]
    fn painting_off_the_canvas_is_harmless() {
        let mut canvas = Canvas::new(50, 50);
        line(
            BrushSettings::default(),
            red(),
            &mut canvas,
            Point::new(-100.0, -100.0),
            Point::new(-80.0, 200.0),
        );
        assert!(canvas.is_empty());
    }
}
