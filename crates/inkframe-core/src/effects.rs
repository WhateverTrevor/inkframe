//! Colour effects and filters on a layer, as Animate's Properties panel
//! offers for symbols: brightness, tint or finer colour changes, and drop
//! shadows, blurs, glows and colour adjustment.
//!
//! The renderer draws a layer with any of these into a buffer of its own
//! first (see the render crate), then hands the pixels here: premultiplied
//! RGBA, four bytes a pixel, row by row. The colour effect goes first, then
//! the filters in their order, each working on what the last produced, so
//! a blur after a drop shadow blurs the shadow too.
//!
//! Sizes (blur radius, shadow distance) are in output pixels at 100%, so
//! they're multiplied by the render scale: a 2× export gets a 2× blur.

use serde::{Deserialize, Serialize};

use crate::color::Color;

/// A layer's colour effect.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ColorEffect {
    #[default]
    None,
    /// Towards white (positive, up to 1) or black (negative, down to −1).
    Brightness { amount: f32 },
    /// Towards `color` by `amount` (0 to 1), keeping the shading.
    Tint { color: Color, amount: f32 },
    /// Each channel times its multiplier plus its offset (offsets in 0–255
    /// units), red, green, blue and alpha, as Animate's Advanced effect.
    Advanced { multiply: [f32; 4], add: [f32; 4] },
}

impl ColorEffect {
    pub fn is_none(&self) -> bool {
        *self == ColorEffect::None
    }

    pub fn label(&self) -> &'static str {
        match self {
            ColorEffect::None => "None",
            ColorEffect::Brightness { .. } => "Brightness",
            ColorEffect::Tint { .. } => "Tint",
            ColorEffect::Advanced { .. } => "Advanced",
        }
    }
}

/// One filter in a layer's list, which can be switched off without losing
/// its settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default = "on")]
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: FilterKind,
}

fn on() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilterKind {
    /// A blurred copy of the layer's silhouette in `color`, `distance`
    /// pixels away at `angle` degrees (clockwise from the right), under it.
    DropShadow { distance: f32, angle: f32, blur: f32, color: Color, strength: f32 },
    /// Blurs the layer, `x` and `y` pixels across and down.
    Blur { x: f32, y: f32 },
    /// A blurred silhouette in `color` around the layer's edges, outside
    /// it or (`inner`) just inside.
    Glow { blur: f32, color: Color, strength: f32, inner: bool },
    /// Brightness and contrast (−1 to 1), saturation (−1 grey to 1 vivid)
    /// and hue (degrees round the colour wheel).
    AdjustColor { brightness: f32, contrast: f32, saturation: f32, hue: f32 },
}

impl FilterKind {
    /// Every filter, as the Add menu lists them, with Animate's defaults.
    pub const ALL: [FilterKind; 4] = [
        FilterKind::DropShadow {
            distance: 5.0,
            angle: 45.0,
            blur: 5.0,
            color: Color::rgba(0, 0, 0, 255),
            strength: 1.0,
        },
        FilterKind::Blur { x: 5.0, y: 5.0 },
        FilterKind::Glow { blur: 5.0, color: Color::rgba(255, 0, 0, 255), strength: 1.0, inner: false },
        FilterKind::AdjustColor { brightness: 0.0, contrast: 0.0, saturation: 0.0, hue: 0.0 },
    ];

    pub fn label(&self) -> &'static str {
        match self {
            FilterKind::DropShadow { .. } => "Drop Shadow",
            FilterKind::Blur { .. } => "Blur",
            FilterKind::Glow { .. } => "Glow",
            FilterKind::AdjustColor { .. } => "Adjust Color",
        }
    }

    /// How far past the layer this filter can paint, in pixels at 100%.
    fn reach(&self) -> f32 {
        match *self {
            FilterKind::DropShadow { distance, blur, .. } => distance.abs() + 2.0 * blur,
            FilterKind::Blur { x, y } => 2.0 * x.max(y),
            FilterKind::Glow { blur, inner, .. } => {
                if inner {
                    0.0
                } else {
                    2.0 * blur
                }
            }
            FilterKind::AdjustColor { .. } => 0.0,
        }
    }
}

/// How far past a layer its filters can paint, in pixels at `scale`: the
/// room its buffer needs around it.
pub fn filter_margin(filters: &[Filter], scale: f32) -> f32 {
    filters.iter().filter(|f| f.enabled).map(|f| f.kind.reach() * scale).sum::<f32>().ceil()
}

/// Applies a colour effect to premultiplied RGBA pixels.
pub fn apply_color_effect(pixels: &mut [u8], effect: &ColorEffect) {
    match *effect {
        ColorEffect::None => {}
        ColorEffect::Brightness { amount } => {
            let amount = amount.clamp(-1.0, 1.0);
            for px in pixels.chunks_exact_mut(4) {
                let alpha = px[3] as f32;
                for c in &mut px[..3] {
                    // Premultiplied white is the alpha itself.
                    let v = *c as f32;
                    let v = if amount >= 0.0 { v + (alpha - v) * amount } else { v * (1.0 + amount) };
                    *c = v.round().clamp(0.0, alpha) as u8;
                }
            }
        }
        ColorEffect::Tint { color, amount } => {
            let amount = amount.clamp(0.0, 1.0);
            let tint = [color.r, color.g, color.b];
            for px in pixels.chunks_exact_mut(4) {
                let alpha = px[3] as f32 / 255.0;
                for (c, t) in px[..3].iter_mut().zip(tint) {
                    let v = *c as f32 * (1.0 - amount) + t as f32 * alpha * amount;
                    *c = v.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        ColorEffect::Advanced { multiply, add } => {
            for px in pixels.chunks_exact_mut(4) {
                let mut rgba = unpremultiply(px);
                for i in 0..4 {
                    rgba[i] = rgba[i] * multiply[i] + add[i];
                }
                premultiply(px, rgba);
            }
        }
    }
}

/// Applies the enabled filters, in order, to a `width`×`height` buffer of
/// premultiplied RGBA. Shadows and glows need room around the layer: the
/// buffer should have [`filter_margin`] pixels of space on each side.
pub fn apply_filters(pixels: &mut [u8], width: usize, height: usize, filters: &[Filter], scale: f32) {
    for filter in filters.iter().filter(|f| f.enabled) {
        match filter.kind {
            FilterKind::Blur { x, y } => blur_rgba(pixels, width, height, x * scale, y * scale),
            FilterKind::DropShadow { distance, angle, blur, color, strength } => {
                let (dx, dy) = (
                    angle.to_radians().cos() * distance * scale,
                    angle.to_radians().sin() * distance * scale,
                );
                let mut shadow = alpha_of(pixels);
                blur_channel(&mut shadow, width, height, blur * scale, blur * scale);
                let shadow = shift(&shadow, width, height, dx.round() as isize, dy.round() as isize);
                under(pixels, &shadow, color, strength);
            }
            FilterKind::Glow { blur, color, strength, inner } => {
                let alpha = alpha_of(pixels);
                if inner {
                    // The silhouette's outside, blurred in over its edges,
                    // and kept to the inside.
                    let mut outside: Vec<u8> = alpha.iter().map(|a| 255 - a).collect();
                    blur_channel(&mut outside, width, height, blur * scale, blur * scale);
                    let glow: Vec<u8> = outside
                        .iter()
                        .zip(&alpha)
                        .map(|(&o, &a)| ((o as u32 * a as u32) / 255) as u8)
                        .collect();
                    over_inside(pixels, &glow, color, strength);
                } else {
                    let mut glow = alpha;
                    blur_channel(&mut glow, width, height, blur * scale, blur * scale);
                    under(pixels, &glow, color, strength);
                }
            }
            FilterKind::AdjustColor { brightness, contrast, saturation, hue } => {
                adjust_color(pixels, brightness, contrast, saturation, hue)
            }
        }
    }
}

fn unpremultiply(px: &[u8]) -> [f32; 4] {
    let a = px[3] as f32;
    if a == 0.0 {
        return [0.0; 4];
    }
    [px[0] as f32 * 255.0 / a, px[1] as f32 * 255.0 / a, px[2] as f32 * 255.0 / a, a]
}

fn premultiply(px: &mut [u8], rgba: [f32; 4]) {
    let a = rgba[3].clamp(0.0, 255.0);
    for i in 0..3 {
        px[i] = (rgba[i].clamp(0.0, 255.0) * a / 255.0).round() as u8;
    }
    px[3] = a.round() as u8;
}

fn alpha_of(pixels: &[u8]) -> Vec<u8> {
    pixels.chunks_exact(4).map(|px| px[3]).collect()
}

/// A copy of a one-channel image moved by `(dx, dy)` pixels, empty where
/// nothing moved in.
fn shift(channel: &[u8], width: usize, height: usize, dx: isize, dy: isize) -> Vec<u8> {
    let mut moved = vec![0; channel.len()];
    for y in 0..height {
        let Some(from_y) = (y as isize).checked_sub(dy).filter(|&v| v >= 0 && (v as usize) < height) else {
            continue;
        };
        for x in 0..width {
            let from_x = x as isize - dx;
            if from_x >= 0 && (from_x as usize) < width {
                moved[y * width + x] = channel[from_y as usize * width + from_x as usize];
            }
        }
    }
    moved
}

/// Paints `color`, as much as `coverage` says (times `strength`), under
/// the pixels: a shadow or glow behind the layer.
fn under(pixels: &mut [u8], coverage: &[u8], color: Color, strength: f32) {
    for (px, &cover) in pixels.chunks_exact_mut(4).zip(coverage) {
        let shade = (cover as f32 / 255.0 * strength).clamp(0.0, 1.0) * color.a as f32 / 255.0;
        let rest = 1.0 - px[3] as f32 / 255.0;
        let tint = [color.r, color.g, color.b];
        for i in 0..3 {
            px[i] = (px[i] as f32 + tint[i] as f32 * shade * rest).round().min(255.0) as u8;
        }
        px[3] = (px[3] as f32 + 255.0 * shade * rest).round().min(255.0) as u8;
    }
}

/// Paints `color` over the pixels where `coverage` says, without spreading
/// past them: an inner glow.
fn over_inside(pixels: &mut [u8], coverage: &[u8], color: Color, strength: f32) {
    for (px, &cover) in pixels.chunks_exact_mut(4).zip(coverage) {
        let amount = (cover as f32 / 255.0 * strength).clamp(0.0, 1.0) * color.a as f32 / 255.0;
        let alpha = px[3] as f32 / 255.0;
        let tint = [color.r, color.g, color.b];
        for i in 0..3 {
            px[i] = (px[i] as f32 * (1.0 - amount) + tint[i] as f32 * alpha * amount).round() as u8;
        }
    }
}

fn adjust_color(pixels: &mut [u8], brightness: f32, contrast: f32, saturation: f32, hue: f32) {
    // Hue turns colours round the grey axis, using the usual luminance
    // weights so it doesn't change how light they look.
    let (cos, sin) = (hue.to_radians().cos(), hue.to_radians().sin());
    let (lr, lg, lb) = (0.213, 0.715, 0.072);
    let hue_matrix = [
        [lr + cos * (1.0 - lr) - sin * lr, lg - cos * lg - sin * lg, lb - cos * lb + sin * (1.0 - lb)],
        [lr - cos * lr + sin * 0.143, lg + cos * (1.0 - lg) + sin * 0.140, lb - cos * lb - sin * 0.283],
        [lr - cos * lr - sin * (1.0 - lr), lg - cos * lg + sin * lg, lb + cos * (1.0 - lb) + sin * lb],
    ];
    let saturation = 1.0 + saturation.clamp(-1.0, 1.0);
    let contrast = 1.0 + contrast.clamp(-1.0, 1.0);
    let brightness = brightness.clamp(-1.0, 1.0) * 255.0;
    for px in pixels.chunks_exact_mut(4) {
        if px[3] == 0 {
            continue;
        }
        let [r, g, b, a] = unpremultiply(px);
        let turned: [f32; 3] = hue_matrix.map(|row| row[0] * r + row[1] * g + row[2] * b);
        let grey = lr * turned[0] + lg * turned[1] + lb * turned[2];
        let adjusted =
            turned.map(|c| ((grey + (c - grey) * saturation) - 128.0) * contrast + 128.0 + brightness);
        premultiply(px, [adjusted[0], adjusted[1], adjusted[2], a]);
    }
}

/// Blurs premultiplied RGBA, `rx` across and `ry` down (radii in pixels).
fn blur_rgba(pixels: &mut [u8], width: usize, height: usize, rx: f32, ry: f32) {
    for channel in 0..4 {
        let mut plane: Vec<u8> = pixels.chunks_exact(4).map(|px| px[channel]).collect();
        blur_channel(&mut plane, width, height, rx, ry);
        for (px, v) in pixels.chunks_exact_mut(4).zip(plane) {
            px[channel] = v;
        }
    }
    // Rounding the channels separately can leave colour a hair above its
    // alpha, which premultiplied pixels can't have.
    for px in pixels.chunks_exact_mut(4) {
        for i in 0..3 {
            px[i] = px[i].min(px[3]);
        }
    }
}

/// A close stand-in for a Gaussian blur of one channel: three box blurs
/// across, then three down, sized so together they spread like a Gaussian
/// whose radius (about two standard deviations) is `rx` or `ry`.
fn blur_channel(plane: &mut [u8], width: usize, height: usize, rx: f32, ry: f32) {
    let mut values: Vec<f32> = plane.iter().map(|&v| v as f32).collect();
    let mut scratch = vec![0.0; values.len()];
    for radius in boxes(rx / 2.0) {
        box_pass(&values, &mut scratch, width, height, radius, true);
        std::mem::swap(&mut values, &mut scratch);
    }
    for radius in boxes(ry / 2.0) {
        box_pass(&values, &mut scratch, width, height, radius, false);
        std::mem::swap(&mut values, &mut scratch);
    }
    for (out, v) in plane.iter_mut().zip(values) {
        *out = v.round().clamp(0.0, 255.0) as u8;
    }
}

/// The radii of three box blurs that together approximate a Gaussian with
/// standard deviation `sigma` (the standard construction: boxes of two
/// neighbouring odd widths, as many of each as gets the variance right).
fn boxes(sigma: f32) -> [usize; 3] {
    if sigma < 0.5 {
        return [0; 3];
    }
    let n = 3.0;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut lower = ideal.floor() as i32;
    if lower % 2 == 0 {
        lower -= 1;
    }
    let upper = lower + 2;
    let ideal_m = (12.0 * sigma * sigma - n * (lower * lower) as f32 - 4.0 * n * lower as f32 - 3.0 * n)
        / (-4.0 * lower as f32 - 4.0);
    let m = ideal_m.round() as i32;
    let width = |i: i32| if i < m { lower } else { upper };
    [0, 1, 2].map(|i| ((width(i) - 1) / 2).max(0) as usize)
}

/// One box blur of radius `r`, along rows (`across`) or columns, treating
/// everything past the edge as empty.
fn box_pass(from: &[f32], to: &mut [f32], width: usize, height: usize, r: usize, across: bool) {
    if r == 0 {
        to.copy_from_slice(from);
        return;
    }
    let (lines, length) = if across { (height, width) } else { (width, height) };
    let at = |line: usize, i: usize| if across { line * width + i } else { i * width + line };
    let span = (2 * r + 1) as f32;
    for line in 0..lines {
        // A running sum over the window, adding the pixel entering it and
        // taking away the one leaving.
        let mut sum: f32 = (0..=r.min(length - 1)).map(|i| from[at(line, i)]).sum();
        for i in 0..length {
            to[at(line, i)] = sum / span;
            if i + r + 1 < length {
                sum += from[at(line, i + r + 1)];
            }
            if i >= r {
                sum -= from[at(line, i - r)];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `size`×`size` transparent buffer with an opaque square of `color`
    /// in the middle, `inner` pixels across.
    fn square(size: usize, inner: usize, color: [u8; 3]) -> Vec<u8> {
        let mut pixels = vec![0; size * size * 4];
        let start = (size - inner) / 2;
        for y in start..start + inner {
            for x in start..start + inner {
                pixels[(y * size + x) * 4..][..4].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
        pixels
    }

    fn pixel(pixels: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        pixels[(y * size + x) * 4..][..4].try_into().unwrap()
    }

    fn filter(kind: FilterKind) -> Filter {
        Filter { enabled: true, kind }
    }

    #[test]
    fn brightness_and_tint_change_colour_but_not_alpha() {
        let mut pixels = vec![100, 50, 0, 200];
        apply_color_effect(&mut pixels, &ColorEffect::Brightness { amount: 1.0 });
        assert_eq!(pixels, [200, 200, 200, 200], "white, at this alpha");
        let mut pixels = vec![100, 50, 0, 200];
        apply_color_effect(&mut pixels, &ColorEffect::Brightness { amount: -1.0 });
        assert_eq!(pixels, [0, 0, 0, 200]);
        let mut pixels = vec![0, 0, 0, 255];
        apply_color_effect(&mut pixels, &ColorEffect::Tint { color: Color::rgb(255, 0, 0), amount: 0.5 });
        assert_eq!(pixels, [128, 0, 0, 255]);
    }

    #[test]
    fn advanced_multiplies_and_offsets_each_channel() {
        let mut pixels = vec![100, 100, 100, 255];
        let effect = ColorEffect::Advanced { multiply: [0.5, 1.0, 1.0, 1.0], add: [0.0, 50.0, 0.0, 0.0] };
        apply_color_effect(&mut pixels, &effect);
        assert_eq!(pixels, [50, 150, 100, 255]);
    }

    #[test]
    fn a_blur_softens_edges_and_keeps_the_total() {
        let size = 40;
        let mut pixels = square(size, 10, [255, 255, 255]);
        let before: u32 = alpha_of(&pixels).iter().map(|&a| a as u32).sum();
        apply_filters(&mut pixels, size, size, &[filter(FilterKind::Blur { x: 4.0, y: 4.0 })], 1.0);
        let after: u32 = alpha_of(&pixels).iter().map(|&a| a as u32).sum();
        assert!((before as f32 - after as f32).abs() / (before as f32) < 0.02, "{before} {after}");
        // Just outside the square is now partly covered, the middle still full.
        assert!(pixel(&pixels, size, 14, 20)[3] > 0);
        assert!(pixel(&pixels, size, 20, 20)[3] > 240);
    }

    #[test]
    fn a_drop_shadow_falls_down_and_right_beneath_the_layer() {
        let size = 40;
        let mut pixels = square(size, 10, [255, 255, 255]);
        let shadow = FilterKind::DropShadow {
            distance: 6.0,
            angle: 45.0,
            blur: 0.0,
            color: Color::rgb(0, 0, 0),
            strength: 1.0,
        };
        apply_filters(&mut pixels, size, size, &[filter(shadow)], 1.0);
        // The square itself is untouched; below-right of it is black shadow.
        assert_eq!(pixel(&pixels, size, 20, 20), [255, 255, 255, 255]);
        assert_eq!(pixel(&pixels, size, 28, 28), [0, 0, 0, 255]);
        assert_eq!(pixel(&pixels, size, 12, 12)[3], 0, "nothing up and left");
    }

    #[test]
    fn a_glow_surrounds_the_layer_and_an_inner_one_stays_inside() {
        let size = 40;
        let glow = |inner| FilterKind::Glow { blur: 6.0, color: Color::rgb(255, 0, 0), strength: 1.0, inner };
        let mut outer = square(size, 10, [0, 0, 255]);
        apply_filters(&mut outer, size, size, &[filter(glow(false))], 1.0);
        let around = pixel(&outer, size, 13, 20);
        assert!(around[0] > 0 && around[3] > 0, "red around it: {around:?}");

        let mut inner = square(size, 10, [0, 0, 255]);
        apply_filters(&mut inner, size, size, &[filter(glow(true))], 1.0);
        assert_eq!(pixel(&inner, size, 13, 20)[3], 0, "nothing outside");
        assert!(pixel(&inner, size, 15, 20)[0] > 0, "red just inside the edge");
    }

    #[test]
    fn adjust_color_turns_hue_and_drains_saturation() {
        let mut red = vec![255, 0, 0, 255];
        apply_filters(
            &mut red,
            1,
            1,
            &[filter(FilterKind::AdjustColor { brightness: 0.0, contrast: 0.0, saturation: -1.0, hue: 0.0 })],
            1.0,
        );
        assert!(red[0] == red[1] && red[1] == red[2], "grey: {red:?}");
        let mut red = vec![255, 0, 0, 255];
        apply_filters(
            &mut red,
            1,
            1,
            &[filter(FilterKind::AdjustColor {
                brightness: 0.0,
                contrast: 0.0,
                saturation: 0.0,
                hue: 120.0,
            })],
            1.0,
        );
        assert!(red[1] > red[0] && red[1] > red[2], "greenish: {red:?}");
    }

    #[test]
    fn switched_off_filters_do_nothing_and_need_no_room() {
        let mut pixels = square(20, 4, [255, 255, 255]);
        let before = pixels.clone();
        let off = Filter { enabled: false, kind: FilterKind::Blur { x: 5.0, y: 5.0 } };
        apply_filters(&mut pixels, 20, 20, &[off], 1.0);
        assert_eq!(pixels, before);
        assert_eq!(filter_margin(&[off], 1.0), 0.0);
        assert_eq!(filter_margin(&[filter(FilterKind::Blur { x: 5.0, y: 5.0 })], 2.0), 20.0);
    }
}
