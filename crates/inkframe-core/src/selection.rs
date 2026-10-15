//! Pixel selections: which part of a canvas the painting tools may change.
//!
//! A selection is a mask over a canvas, one value per pixel from 0 (not
//! selected) to 255 (selected), so its edges can be soft: an ellipse or a
//! lasso is smooth rather than stair-stepped, and painting fades out across
//! the edge. Every painting tool respects it (see [`Selection::limit`]).
//!
//! Selections are made with the marquee (a rectangle or ellipse), the
//! lasso (any outline), or the magic wand (an area of similar colour), and
//! combined as Photoshop combines them: replace, add, subtract or keep
//! only the overlap. They belong to the editing session rather than to the
//! document, as in Photoshop, so they aren't saved.

use std::sync::Arc;

use crate::geometry::Point;
use crate::paint::{Canvas, TILE};

/// A mask over a `width`×`height` canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub width: u32,
    pub height: u32,
    /// Row by row. Shared, since it's copied around but rarely changed.
    mask: Arc<Vec<u8>>,
}

/// How a new selection combines with the one there already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Combine {
    #[default]
    Replace,
    /// Shift: add to it.
    Add,
    /// Alt/Option: take away from it.
    Subtract,
    /// Shift and Alt together: keep only where both are.
    Intersect,
}

impl Selection {
    /// Nothing selected.
    pub fn none(width: u32, height: u32) -> Self {
        Self { width, height, mask: Arc::new(vec![0; (width * height) as usize]) }
    }

    /// The whole canvas.
    pub fn all(width: u32, height: u32) -> Self {
        Self { width, height, mask: Arc::new(vec![255; (width * height) as usize]) }
    }

    /// A rectangle, between two corners (canvas pixels).
    pub fn rectangle(width: u32, height: u32, a: Point, b: Point) -> Self {
        let (min, max) = (Point::new(a.x.min(b.x), a.y.min(b.y)), Point::new(a.x.max(b.x), a.y.max(b.y)));
        Self::polygon(width, height, &[min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)])
    }

    /// The ellipse filling the box between two corners.
    pub fn ellipse(width: u32, height: u32, a: Point, b: Point) -> Self {
        let center = a.lerp(b, 0.5);
        let (rx, ry) = ((b.x - a.x).abs() / 2.0, (b.y - a.y).abs() / 2.0);
        // Enough corners that the outline looks round at any size.
        let sides = ((rx.max(ry) * 0.75) as usize).clamp(24, 720);
        let outline: Vec<Point> = (0..sides)
            .map(|i| {
                let t = i as f32 / sides as f32 * std::f32::consts::TAU;
                Point::new(center.x + t.cos() * rx, center.y + t.sin() * ry)
            })
            .collect();
        Self::polygon(width, height, &outline)
    }

    /// Everything inside an outline (the lasso's), closed from its last
    /// point back to its first. Edges are anti-aliased: each pixel is
    /// covered as much as the outline covers it, measured over a 4×4 grid
    /// of points within it.
    pub fn polygon(width: u32, height: u32, outline: &[Point]) -> Self {
        let mut mask = vec![0u32; (width * height) as usize];
        if outline.len() >= 3 {
            const SUB: usize = 4;
            let edges: Vec<(Point, Point)> =
                outline.iter().zip(outline.iter().cycle().skip(1)).map(|(a, b)| (*a, *b)).collect();
            let top = outline.iter().map(|p| p.y).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
            let bottom =
                (outline.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max).ceil() as u32).min(height);
            let mut crossings = Vec::new();
            for y in top..bottom {
                for sub in 0..SUB {
                    // Where the outline crosses this sub-row: inside is
                    // between pairs of crossings (even-odd).
                    let sy = y as f32 + (sub as f32 + 0.5) / SUB as f32;
                    crossings.clear();
                    for &(a, b) in &edges {
                        if (a.y <= sy) != (b.y <= sy) {
                            crossings.push(a.x + (sy - a.y) / (b.y - a.y) * (b.x - a.x));
                        }
                    }
                    crossings.sort_by(f32::total_cmp);
                    for span in crossings.chunks_exact(2) {
                        let (start, end) = (span[0].max(0.0), span[1].min(width as f32));
                        if start >= end {
                            continue;
                        }
                        // Each pixel gets the part of the span it holds, in
                        // `SUB` steps.
                        let (first, last) = (start.floor() as u32, (end.ceil() as u32).min(width));
                        for x in first..last {
                            let covered = (end.min(x as f32 + 1.0) - start.max(x as f32)).clamp(0.0, 1.0);
                            mask[(y * width + x) as usize] += (covered * 255.0) as u32;
                        }
                    }
                }
            }
        }
        // Each pixel added up to 255 per sub-row; average them.
        let mask = mask.into_iter().map(|v| (v / 4).min(255) as u8).collect();
        Self { width, height, mask: Arc::new(mask) }
    }

    /// A selection from a ready-made mask, `width`×`height`, row by row.
    pub fn from_mask(width: u32, height: u32, mask: Vec<u8>) -> Self {
        debug_assert_eq!(mask.len(), (width * height) as usize);
        Self { width, height, mask: Arc::new(mask) }
    }

    /// The magic wand: the pixels of similar colour to (`x`, `y`) on
    /// `canvas`, joined to it (`contiguous`) or anywhere.
    pub fn similar(canvas: &Canvas, x: i64, y: i64, tolerance: u8, contiguous: bool) -> Self {
        let mut selection = Self::none(canvas.width, canvas.height);
        if let Some(found) = crate::fill::similar_area(canvas, x, y, tolerance, contiguous) {
            selection.mask = Arc::new(found.into_iter().map(|f| if f { 255 } else { 0 }).collect());
        }
        selection
    }

    /// Combines `new` with this selection.
    pub fn combine(&self, new: &Selection, how: Combine) -> Selection {
        let pairs = self.mask.iter().zip(new.mask.iter());
        let mask = match how {
            Combine::Replace => return new.clone(),
            Combine::Add => pairs.map(|(a, b)| *a.max(b)).collect(),
            Combine::Subtract => pairs.map(|(a, b)| ((*a as u32 * (255 - *b as u32)) / 255) as u8).collect(),
            Combine::Intersect => pairs.map(|(a, b)| ((*a as u32 * *b as u32) / 255) as u8).collect(),
        };
        Selection { width: self.width, height: self.height, mask: Arc::new(mask) }
    }

    /// Everything that wasn't selected.
    pub fn inverted(&self) -> Selection {
        Selection {
            width: self.width,
            height: self.height,
            mask: Arc::new(self.mask.iter().map(|v| 255 - v).collect()),
        }
    }

    /// Identifies this selection's mask: copies share it, and any change
    /// makes a new one. For caching things worked out from it.
    pub fn identity(&self) -> usize {
        Arc::as_ptr(&self.mask) as usize
    }

    /// How selected pixel (`x`, `y`) is, 0 to 255; 0 off the canvas.
    pub fn at(&self, x: i64, y: i64) -> u8 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return 0;
        }
        self.mask[(y as u32 * self.width + x as u32) as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|&v| v == 0)
    }

    /// The selection's mask over a rectangle of the canvas, row by row: 0
    /// to 1 per pixel, matching what [`Canvas::read_region`] returns.
    pub fn region(&self, x: i64, y: i64, width: u32, height: u32) -> Vec<f32> {
        (0..height as i64)
            .flat_map(|row| (0..width as i64).map(move |col| (col, row)))
            .map(|(col, row)| self.at(x + col, y + row) as f32 / 255.0)
            .collect()
    }

    /// Keeps a change to a rectangle of pixels, `width` wide, only where
    /// it's selected:
    /// `after` is blended back toward `before` by how unselected each pixel
    /// is. Both are premultiplied RGBA, as `Canvas::read_region` gives.
    pub fn limit(&self, x: i64, y: i64, width: u32, before: &[u8], after: &mut [u8]) {
        for (i, (old, new)) in before.chunks_exact(4).zip(after.chunks_exact_mut(4)).enumerate() {
            let (col, row) = (i as i64 % width as i64, i as i64 / width as i64);
            let keep = self.at(x + col, y + row) as u32;
            if keep == 255 {
                continue;
            }
            for c in 0..4 {
                new[c] = ((new[c] as u32 * keep + old[c] as u32 * (255 - keep) + 127) / 255) as u8;
            }
        }
    }

    /// Like [`Selection::limit`], for a whole canvas changed from `before`:
    /// only the tiles that differ are looked at.
    pub fn limit_canvas(&self, before: &Canvas, after: &mut Canvas) {
        let (columns, rows) = after.tile_counts();
        for ty in 0..rows {
            for tx in 0..columns {
                let (old, new) = (before.tile((tx, ty)), after.tile((tx, ty)));
                if old == new {
                    continue;
                }
                let (x, y) = ((tx * TILE) as i64, (ty * TILE) as i64);
                let old = before.read_region(x, y, TILE, TILE);
                let mut new = after.read_region(x, y, TILE, TILE);
                self.limit(x, y, TILE, &old, &mut new);
                after.write_region(x, y, TILE, TILE, &new);
            }
        }
    }

    /// `canvas` with only the selected pixels kept (soft edges partly).
    pub fn keep(&self, canvas: &Canvas) -> Canvas {
        self.scale_alpha(canvas, |selected| selected)
    }

    /// `canvas` with the selected pixels taken away.
    pub fn clear(&self, canvas: &Canvas) -> Canvas {
        self.scale_alpha(canvas, |selected| 255 - selected)
    }

    /// Scales each pixel (all four premultiplied channels) by `amount` of
    /// how selected it is, out of 255.
    fn scale_alpha(&self, canvas: &Canvas, amount: impl Fn(u32) -> u32) -> Canvas {
        let mut result = canvas.clone();
        for ((tx, ty), tile) in canvas.tiles() {
            let (x0, y0) = ((tx * TILE) as i64, (ty * TILE) as i64);
            let pixels = tile
                .chunks_exact(4)
                .enumerate()
                .flat_map(|(i, p)| {
                    let (x, y) = (x0 + (i as u32 % TILE) as i64, y0 + (i as u32 / TILE) as i64);
                    let keep = amount(self.at(x, y) as u32);
                    p.iter().map(move |&c| ((c as u32 * keep + 127) / 255) as u8).collect::<Vec<_>>()
                })
                .collect();
            result.set_tile((tx, ty), pixels);
        }
        result
    }

    /// The selection moved by whole pixels.
    pub fn shifted(&self, dx: i64, dy: i64) -> Selection {
        let mask = (0..self.height as i64)
            .flat_map(|y| (0..self.width as i64).map(move |x| (x, y)))
            .map(|(x, y)| self.at(x - dx, y - dy))
            .collect();
        Selection { width: self.width, height: self.height, mask: Arc::new(mask) }
    }

    /// The selection's edge, as short horizontal and vertical segments
    /// between selected and unselected pixels (canvas coordinates), for
    /// drawing "marching ants".
    pub fn outline(&self) -> Vec<(Point, Point)> {
        let inside = |x: i64, y: i64| self.at(x, y) >= 128;
        let mut segments = Vec::new();
        for y in 0..=self.height as i64 {
            for x in 0..=self.width as i64 {
                let here = inside(x, y);
                if here != inside(x - 1, y) {
                    segments.push((Point::new(x as f32, y as f32), Point::new(x as f32, y as f32 + 1.0)));
                }
                if here != inside(x, y - 1) {
                    segments.push((Point::new(x as f32, y as f32), Point::new(x as f32 + 1.0, y as f32)));
                }
            }
        }
        merge_runs(segments)
    }
}

/// Joins edge segments that continue one another in a straight line, so a
/// rectangle's outline is four segments rather than hundreds.
fn merge_runs(mut segments: Vec<(Point, Point)>) -> Vec<(Point, Point)> {
    let horizontal = |s: &(Point, Point)| s.0.y == s.1.y;
    segments.sort_by(|a, b| {
        let key = |s: &(Point, Point)| if horizontal(s) { (0, s.0.y, s.0.x) } else { (1, s.0.x, s.0.y) };
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(ka.2.total_cmp(&kb.2))
    });
    let mut merged: Vec<(Point, Point)> = Vec::new();
    for segment in segments {
        match merged.last_mut() {
            Some(last) if horizontal(last) == horizontal(&segment) && last.1 == segment.0 => {
                last.1 = segment.1
            }
            _ => merged.push(segment),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_selects_whole_pixels_inside_it() {
        let s = Selection::rectangle(20, 20, Point::new(2.0, 3.0), Point::new(10.0, 8.0));
        assert_eq!(s.at(2, 3), 255);
        assert_eq!(s.at(9, 7), 255);
        assert_eq!(s.at(10, 7), 0);
        assert_eq!(s.at(1, 5), 0);
        // Four straight sides.
        assert_eq!(s.outline().len(), 4);
    }

    #[test]
    fn edges_between_pixels_are_partly_selected() {
        let s = Selection::rectangle(20, 20, Point::new(2.5, 0.0), Point::new(10.0, 10.0));
        assert!((120..=135).contains(&s.at(2, 5)), "half a pixel: {}", s.at(2, 5));
    }

    #[test]
    fn an_ellipse_is_round_with_soft_edges() {
        let s = Selection::ellipse(100, 100, Point::new(10.0, 10.0), Point::new(90.0, 90.0));
        assert_eq!(s.at(50, 50), 255);
        assert_eq!(s.at(12, 12), 0, "the box's corner is outside the ellipse");
        let edge = (0..100).map(|x| s.at(x, 50)).find(|&v| v > 0 && v < 255);
        assert!(edge.is_some(), "the edge is anti-aliased");
    }

    #[test]
    fn selections_combine() {
        let left = Selection::rectangle(10, 1, Point::new(0.0, 0.0), Point::new(6.0, 1.0));
        let right = Selection::rectangle(10, 1, Point::new(4.0, 0.0), Point::new(10.0, 1.0));
        let row = |s: &Selection| (0..10).map(|x| s.at(x, 0) / 255).collect::<Vec<_>>();
        assert_eq!(row(&left.combine(&right, Combine::Add)), [1; 10]);
        assert_eq!(row(&left.combine(&right, Combine::Subtract)), [1, 1, 1, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(row(&left.combine(&right, Combine::Intersect)), [0, 0, 0, 0, 1, 1, 0, 0, 0, 0]);
        assert_eq!(row(&left.inverted()), [0, 0, 0, 0, 0, 0, 1, 1, 1, 1]);
    }

    #[test]
    fn keeping_and_clearing_split_a_canvas_in_two() {
        let mut canvas = Canvas::new(4, 1);
        canvas.write_region(0, 0, 4, 1, &[50, 60, 70, 255].repeat(4));
        let selection = Selection::rectangle(4, 1, Point::new(0.0, 0.0), Point::new(2.0, 1.0));
        let (kept, cleared) = (selection.keep(&canvas), selection.clear(&canvas));
        assert_eq!((kept.pixel(1, 0), kept.pixel(2, 0)), ([50, 60, 70, 255], [0; 4]));
        assert_eq!((cleared.pixel(1, 0), cleared.pixel(2, 0)), ([0; 4], [50, 60, 70, 255]));
        assert_eq!(selection.shifted(1, 0).at(2, 0), 255);
        assert_eq!(selection.shifted(1, 0).at(0, 0), 0);
    }

    #[test]
    fn changes_are_kept_only_where_selected() {
        let selection = Selection::rectangle(4, 1, Point::new(0.0, 0.0), Point::new(2.0, 1.0));
        let before = [0u8; 16];
        let mut after = [200u8; 16];
        selection.limit(0, 0, 4, &before, &mut after);
        assert_eq!(after, [200, 200, 200, 200, 200, 200, 200, 200, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
}
