//! The grain of a layer's line texture (see `LineTexture` in the core):
//! a tile of how much ink each spot of paper takes, repeated over the
//! layer, which lines are then painted through.

use std::sync::OnceLock;

use inkframe_core::{Color, LineTexture, TextureKind};
use tiny_skia::{FilterQuality, Paint, Pattern, Pixmap, SpreadMode, Transform};

/// The tile's size, in the layer's units. Large enough that the repeat
/// doesn't show at a glance.
const TILE: u32 = 96;

/// A whole number from a spot and a seed, scrambled so neighbours aren't
/// alike (a small integer hash; no randomness, so a frame always draws the
/// same).
fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Smooth noise: the hash on a grid `cell` across and `cell * stretch`
/// down, blended between grid points, wrapping at the tile's edge.
fn value_noise(x: u32, y: u32, cell: f32, stretch: f32, seed: u32) -> f32 {
    let (gx, gy) = (x as f32 / cell, y as f32 / (cell * stretch));
    let (cx, cy) =
        ((TILE as f32 / cell).round() as u32, (TILE as f32 / (cell * stretch)).round().max(1.0) as u32);
    let (x0, y0) = (gx.floor() as u32, gy.floor() as u32);
    let (fx, fy) = (gx.fract(), gy.fract());
    let at = |i: u32, j: u32| hash(i % cx.max(1), j % cy, seed);
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * fx;
    let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * fx;
    top + (bottom - top) * fy
}

/// How much ink each spot of the tile takes, 0 to 1, for a kind of grain.
fn grain(kind: TextureKind) -> &'static [f32] {
    static TILES: [OnceLock<Vec<f32>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let index = TextureKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    TILES[index].get_or_init(|| {
        (0..TILE * TILE)
            .map(|i| {
                let (x, y) = (i % TILE, i / TILE);
                match kind {
                    // Fine tooth: every spot a little different.
                    TextureKind::Pencil => 0.55 + 0.45 * hash(x, y, 1).powf(0.6),
                    // Streaks dragged across, with coarse blotches.
                    TextureKind::Charcoal => {
                        let streak = value_noise(x, y, 3.0, 0.25, 2);
                        let blotch = value_noise(x, y, 12.0, 1.0, 3);
                        (0.25 + 0.75 * streak * (0.5 + blotch)).min(1.0)
                    }
                    // Speckles of bare paper in two-unit flecks.
                    TextureKind::Chalk => {
                        if hash(x / 2, y / 2, 4) > 0.62 {
                            0.1
                        } else {
                            0.85 + 0.15 * hash(x, y, 5)
                        }
                    }
                }
            })
            .collect()
    })
}

/// A tile of `color` showing `texture`'s grain, to paint lines through
/// with [`paint`].
pub(crate) fn tile(color: Color, texture: LineTexture) -> Option<Pixmap> {
    let amount = texture.amount.clamp(0.0, 1.0);
    let mut tile = Pixmap::new(TILE, TILE)?;
    for (pixel, ink) in tile.data_mut().chunks_exact_mut(4).zip(grain(texture.kind)) {
        // Full ink where the paper takes it; less as the grain shows more.
        let a = (color.a as f32 * (1.0 - amount * (1.0 - ink))).round() as u32;
        let premultiply = |c: u8| (c as u32 * a / 255) as u8;
        pixel.copy_from_slice(&[premultiply(color.r), premultiply(color.g), premultiply(color.b), a as u8]);
    }
    Some(tile)
}

/// A paint laying `tile` in the layer's own units (the coordinates its
/// shapes are drawn in), so the grain stays put on the drawing.
pub(crate) fn paint(tile: &Pixmap) -> Paint<'_> {
    let shader =
        Pattern::new(tile.as_ref(), SpreadMode::Repeat, FilterQuality::Nearest, 1.0, Transform::identity());
    Paint { shader, anti_alias: true, ..Paint::default() }
}
