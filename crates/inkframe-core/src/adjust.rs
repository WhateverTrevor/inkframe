//! Image adjustments: Brightness/Contrast, Hue/Saturation, Levels and
//! Curves, applied to a paint layer's canvas (within the selection, if
//! there is one).
//!
//! Each works on straight (not premultiplied) colour and leaves alpha
//! alone. All but Hue/Saturation change each channel on its own, so they're
//! worked out once as a 256-entry table and then looked up per pixel.
//!
//! [`adjust`] returns a new canvas rather than changing one in place, so
//! the Adjustments window can preview from the original as sliders move
//! and put it back on Cancel.
//!
//! An *adjustment layer* holds one of these instead of a drawing, and
//! changes everything drawn beneath it as the picture is drawn, so the
//! setting can be changed or the layer hidden at any time, as Photoshop's
//! adjustment layers are. The renderer applies it with
//! [`Adjustment::apply_premultiplied`].

use serde::{Deserialize, Serialize};

use crate::document::{Document, LayerId};
use crate::paint::{Canvas, premultiply, unpremultiply};
use crate::selection::Selection;

/// An adjustment and its settings. The defaults change nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Adjustment {
    /// Both −1 to 1.
    BrightnessContrast { brightness: f32, contrast: f32 },
    /// Hue in degrees, −180 to 180; saturation and lightness −1 to 1.
    HueSaturation { hue: f32, saturation: f32, lightness: f32 },
    /// Input levels: values at or below `black` become the darkest output,
    /// at or above `white` the lightest, and `gamma` bends the middle
    /// (above 1 lightens). Output levels squeeze the result into
    /// `out_black` to `out_white`.
    Levels { black: u8, gamma: f32, white: u8, out_black: u8, out_white: u8 },
    /// A curve through `points` (input, output; both 0 to 1), sorted by
    /// input. With fewer than two points it's a straight line.
    Curves { points: Vec<(f32, f32)> },
}

impl Adjustment {
    pub fn brightness_contrast() -> Self {
        Adjustment::BrightnessContrast { brightness: 0.0, contrast: 0.0 }
    }

    pub fn hue_saturation() -> Self {
        Adjustment::HueSaturation { hue: 0.0, saturation: 0.0, lightness: 0.0 }
    }

    pub fn levels() -> Self {
        Adjustment::Levels { black: 0, gamma: 1.0, white: 255, out_black: 0, out_white: 255 }
    }

    pub fn curves() -> Self {
        Adjustment::Curves { points: vec![(0.0, 0.0), (1.0, 1.0)] }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Adjustment::BrightnessContrast { .. } => crate::i18n::tr_later("Brightness/Contrast"),
            Adjustment::HueSaturation { .. } => crate::i18n::tr_later("Hue/Saturation"),
            Adjustment::Levels { .. } => crate::i18n::tr_later("Levels"),
            Adjustment::Curves { .. } => crate::i18n::tr_later("Curves"),
        }
    }

    /// For the adjustments that treat each channel alike, what each value
    /// 0–255 becomes.
    pub fn table(&self) -> Option<[u8; 256]> {
        let map: Box<dyn Fn(f32) -> f32> = match self {
            Adjustment::BrightnessContrast { brightness, contrast } => {
                // Contrast stretches or squeezes around the middle grey;
                // at +1 it would be infinite, so it stops just short.
                let factor = if *contrast >= 0.0 { 1.0 / (1.0 - contrast.min(0.99)) } else { 1.0 + contrast };
                let shift = brightness * 0.5;
                Box::new(move |v| (v + shift - 0.5) * factor + 0.5)
            }
            Adjustment::Levels { black, gamma, white, out_black, out_white } => {
                let (black, white) =
                    (*black as f32 / 255.0, (*white).max(black.saturating_add(1)) as f32 / 255.0);
                let (low, high) = (*out_black as f32 / 255.0, *out_white as f32 / 255.0);
                let gamma = gamma.clamp(0.1, 10.0);
                Box::new(move |v| {
                    low + ((v - black) / (white - black)).clamp(0.0, 1.0).powf(1.0 / gamma) * (high - low)
                })
            }
            Adjustment::Curves { points } => {
                let curve = MonotoneCurve::new(points);
                Box::new(move |v| curve.at(v))
            }
            Adjustment::HueSaturation { .. } => return None,
        };
        Some(std::array::from_fn(|i| (map(i as f32 / 255.0).clamp(0.0, 1.0) * 255.0).round() as u8))
    }

    /// What a premultiplied pixel becomes, its alpha kept. `table` is
    /// [`Adjustment::table`], worked out once for a whole picture.
    pub fn apply_premultiplied(&self, table: Option<&[u8; 256]>, pixel: [u8; 4]) -> [u8; 4] {
        if pixel[3] == 0 {
            return pixel;
        }
        let [r, g, b, a] = unpremultiply(&pixel);
        let [r, g, b] = self.apply(table, [r, g, b]);
        premultiply(&[r, g, b, a])
    }

    /// What a straight colour becomes.
    fn apply(&self, table: Option<&[u8; 256]>, rgb: [u8; 3]) -> [u8; 3] {
        if let Some(table) = table {
            return rgb.map(|v| table[v as usize]);
        }
        let Adjustment::HueSaturation { hue, saturation, lightness } = *self else { return rgb };
        let (h, s, l) = to_hsl(rgb);
        let h = (h + hue / 360.0).rem_euclid(1.0);
        // Up moves toward full saturation (or white); down toward none (or
        // black), so ±1 always reaches the extreme.
        let s = if saturation >= 0.0 { s + (1.0 - s) * saturation } else { s * (1.0 + saturation) };
        let l = if lightness >= 0.0 { l + (1.0 - l) * lightness } else { l * (1.0 + lightness) };
        from_hsl(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
    }
}

impl Document {
    /// Adds an adjustment layer to `timeline` from `frame` on, on top of
    /// its stack. Returns it.
    pub fn add_adjustment_layer(
        &mut self,
        timeline: crate::TimelineId,
        name: impl Into<String>,
        adjustment: Adjustment,
        frame: u32,
    ) -> Option<LayerId> {
        let centre = crate::Point::new(self.width as f32 / 2.0, self.height as f32 / 2.0);
        let id = self.add_layer_to(timeline, name, Vec::new(), frame, crate::Transform::at(centre))?;
        let layer = self.layer_mut(id)?;
        layer.start = frame;
        layer.adjustment = Some(adjustment);
        Some(id)
    }
}

/// `canvas` with `adjustment` applied, within `selection` if given.
pub fn adjust(canvas: &Canvas, adjustment: &Adjustment, selection: Option<&Selection>) -> Canvas {
    let table = adjustment.table();
    let mut adjusted = canvas.clone();
    for (at, tile) in canvas.tiles() {
        let pixels = tile
            .chunks_exact(4)
            .flat_map(|p| {
                if p[3] == 0 {
                    return [0; 4];
                }
                let [r, g, b, a] = unpremultiply(p);
                let [r, g, b] = adjustment.apply(table.as_ref(), [r, g, b]);
                premultiply(&[r, g, b, a])
            })
            .collect();
        adjusted.set_tile(at, pixels);
    }
    if let Some(selection) = selection {
        selection.limit_canvas(canvas, &mut adjusted);
    }
    adjusted
}

/// A smooth curve through points that never overshoots between them
/// (Fritsch–Carlson monotone cubic), as curves editors draw: dragging one
/// point up doesn't make the curve dip somewhere else.
struct MonotoneCurve {
    xs: Vec<f32>,
    ys: Vec<f32>,
    slopes: Vec<f32>,
}

impl MonotoneCurve {
    fn new(points: &[(f32, f32)]) -> Self {
        let mut points: Vec<(f32, f32)> = points.to_vec();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4);
        if points.len() < 2 {
            points = vec![(0.0, 0.0), (1.0, 1.0)];
        }
        let (xs, ys): (Vec<f32>, Vec<f32>) = points.into_iter().unzip();
        let n = xs.len();
        let secants: Vec<f32> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i])).collect();
        let mut slopes = vec![0.0; n];
        slopes[0] = secants[0];
        slopes[n - 1] = secants[n - 2];
        for i in 1..n - 1 {
            slopes[i] =
                if secants[i - 1] * secants[i] <= 0.0 { 0.0 } else { (secants[i - 1] + secants[i]) / 2.0 };
        }
        // Keep each piece from overshooting.
        for i in 0..n - 1 {
            if secants[i] == 0.0 {
                slopes[i] = 0.0;
                slopes[i + 1] = 0.0;
                continue;
            }
            let (a, b) = (slopes[i] / secants[i], slopes[i + 1] / secants[i]);
            let length = (a * a + b * b).sqrt();
            if length > 3.0 {
                slopes[i] = 3.0 * a / length * secants[i];
                slopes[i + 1] = 3.0 * b / length * secants[i];
            }
        }
        Self { xs, ys, slopes }
    }

    fn at(&self, x: f32) -> f32 {
        let last = self.xs.len() - 1;
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[last] {
            return self.ys[last];
        }
        let i = self.xs.partition_point(|&p| p <= x) - 1;
        let h = self.xs[i + 1] - self.xs[i];
        let t = (x - self.xs[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.ys[i]
            + (t3 - 2.0 * t2 + t) * h * self.slopes[i]
            + (-2.0 * t3 + 3.0 * t2) * self.ys[i + 1]
            + (t3 - t2) * h * self.slopes[i + 1]
    }
}

fn to_hsl([r, g, b]: [u8; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> [u8; 3] {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return [v; 3];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    /// A canvas of a single colour, 4×1.
    fn swatch(rgb: [u8; 3]) -> Canvas {
        let mut canvas = Canvas::new(4, 1);
        canvas.write_region(0, 0, 4, 1, &[rgb[0], rgb[1], rgb[2], 255].repeat(4));
        canvas
    }

    fn colour(canvas: &Canvas) -> [u8; 3] {
        let [r, g, b, _] = unpremultiply(&canvas.pixel(0, 0));
        [r, g, b]
    }

    #[test]
    fn the_defaults_change_nothing() {
        let canvas = swatch([90, 140, 200]);
        for adjustment in [
            Adjustment::brightness_contrast(),
            Adjustment::hue_saturation(),
            Adjustment::levels(),
            Adjustment::curves(),
        ] {
            let after = colour(&adjust(&canvas, &adjustment, None));
            let close = after.iter().zip([90, 140, 200]).all(|(a, b)| a.abs_diff(b) <= 1);
            assert!(close, "{} changed {after:?}", adjustment.name());
        }
    }

    #[test]
    fn brightness_lightens_and_contrast_spreads_from_grey() {
        let brighter = adjust(
            &swatch([100; 3]),
            &Adjustment::BrightnessContrast { brightness: 0.4, contrast: 0.0 },
            None,
        );
        assert!(colour(&brighter)[0] > 140);
        let contrast = Adjustment::BrightnessContrast { brightness: 0.0, contrast: 0.5 };
        assert!(colour(&adjust(&swatch([100; 3]), &contrast, None))[0] < 90);
        assert!(colour(&adjust(&swatch([160; 3]), &contrast, None))[0] > 170);
    }

    #[test]
    fn hue_turns_red_to_green_and_desaturating_makes_grey() {
        let green = adjust(
            &swatch([255, 0, 0]),
            &Adjustment::HueSaturation { hue: 120.0, saturation: 0.0, lightness: 0.0 },
            None,
        );
        assert_eq!(colour(&green), [0, 255, 0]);
        let grey = adjust(
            &swatch([200, 50, 50]),
            &Adjustment::HueSaturation { hue: 0.0, saturation: -1.0, lightness: 0.0 },
            None,
        );
        let [r, g, b] = colour(&grey);
        assert!(r == g && g == b, "{:?}", [r, g, b]);
    }

    #[test]
    fn levels_stretch_the_range_between_black_and_white() {
        let levels = Adjustment::Levels { black: 50, gamma: 1.0, white: 200, out_black: 0, out_white: 255 };
        assert_eq!(colour(&adjust(&swatch([50; 3]), &levels, None)), [0; 3]);
        assert_eq!(colour(&adjust(&swatch([200; 3]), &levels, None)), [255; 3]);
        // Halfway, 127.5, either way.
        assert!((127..=128).contains(&colour(&adjust(&swatch([125; 3]), &levels, None))[0]));
    }

    #[test]
    fn a_curve_through_a_raised_midpoint_lightens_midtones_without_overshooting() {
        let curves = Adjustment::Curves { points: vec![(0.0, 0.0), (0.5, 0.7), (1.0, 1.0)] };
        let table = curves.table().unwrap();
        assert_eq!(table[0], 0);
        assert_eq!(table[255], 255);
        assert!((175..=181).contains(&table[128]), "{}", table[128]);
        assert!(table.windows(2).all(|w| w[0] <= w[1]), "never dips");
    }

    #[test]
    fn adjustments_stay_inside_the_selection() {
        let canvas = swatch([100; 3]);
        let selection = Selection::rectangle(4, 1, Point::new(0.0, 0.0), Point::new(2.0, 1.0));
        let invert = Adjustment::Curves { points: vec![(0.0, 1.0), (1.0, 0.0)] };
        let after = adjust(&canvas, &invert, Some(&selection));
        assert_eq!(after.pixel(0, 0)[0], 155);
        assert_eq!(after.pixel(3, 0)[0], 100);
    }
}
