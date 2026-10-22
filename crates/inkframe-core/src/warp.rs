//! The free-form deformer: a grid of control points over a drawing that
//! bends it smoothly when they're dragged, as Harmony's free-form deformer
//! and Photoshop's warp do.
//!
//! The grid covers a rectangle of the drawing at rest. Each control point
//! can be moved off its rest position, and that offset is animated, so the
//! drawing can squash, bulge and sway over time. A point of the drawing
//! moves by a blend of every control point's offset, weighted by how near
//! it is: the blend is a Bézier patch (Bernstein polynomials across and
//! down), which bends the whole drawing without kinks along the grid lines.
//! Moving a corner pulls that corner hardest and the far side not at all.
//! This is the free-form deformation of Sederberg and Parry.
//!
//! It works after the bone deformer (see `deform.rs`), if the layer has one
//! too, so bones can swing a limb and the grid add a bulge.

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, path_from_anchors};
use crate::channel::{AnyChannel, Channel};
use crate::deform::subdivide;
use crate::document::Element;
use crate::geometry::{Point, Rect};
use crate::shape::Shape;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warp {
    /// The area the grid covers at rest, in the layer's own coordinates.
    pub bounds: Rect,
    /// Control points across and down (at least 2 each).
    pub columns: usize,
    pub rows: usize,
    /// Each control point's offset from rest, animated, row by row.
    pub offsets: Vec<Channel<Point>>,
}

impl Warp {
    /// An unbent grid of `columns`×`rows` control points over `bounds`,
    /// keyed at `frame`.
    pub fn over(bounds: Rect, columns: usize, rows: usize, frame: u32) -> Self {
        let (columns, rows) = (columns.clamp(2, 8), rows.clamp(2, 8));
        let offsets = (0..columns * rows).map(|_| Channel::new(frame, Point::ZERO)).collect();
        Self { bounds, columns, rows, offsets }
    }

    /// Control point `(column, row)` at rest.
    pub fn rest_point(&self, column: usize, row: usize) -> Point {
        let u = column as f32 / (self.columns - 1) as f32;
        let v = row as f32 / (self.rows - 1) as f32;
        Point::new(self.bounds.min.x + u * self.bounds.width(), self.bounds.min.y + v * self.bounds.height())
    }

    /// Every control point where it is on `frame`, row by row.
    pub fn points_at(&self, frame: u32) -> Vec<Point> {
        let offsets = self.offsets_at(frame);
        (0..self.rows)
            .flat_map(|row| (0..self.columns).map(move |column| (column, row)))
            .zip(offsets)
            .map(|((column, row), offset)| self.rest_point(column, row) + offset)
            .collect()
    }

    fn offsets_at(&self, frame: u32) -> Vec<Point> {
        self.offsets.iter().map(|c| c.sample(frame as f32).unwrap_or(Point::ZERO)).collect()
    }

    /// Whether every control point is at rest on `frame`.
    pub fn is_at_rest(&self, frame: u32) -> bool {
        self.offsets_at(frame).iter().all(|o| o.length() < 1e-4)
    }

    /// Moves control point `index` to `to` (in the layer's coordinates) on
    /// `frame`, keying it there.
    pub fn move_point(&mut self, index: usize, to: Point, frame: u32) {
        let (column, row) = (index % self.columns, index / self.columns);
        let rest = self.rest_point(column, row);
        if let Some(channel) = self.offsets.get_mut(index) {
            channel.set(frame, to - rest);
        }
    }

    /// Where the drawing's point `p` goes, given the control points'
    /// offsets.
    fn warp_point(&self, offsets: &[Point], p: Point) -> Point {
        let u = ((p.x - self.bounds.min.x) / self.bounds.width().max(1e-3)).clamp(0.0, 1.0);
        let v = ((p.y - self.bounds.min.y) / self.bounds.height().max(1e-3)).clamp(0.0, 1.0);
        let across = bernstein(self.columns - 1, u);
        let down = bernstein(self.rows - 1, v);
        let mut moved = Point::ZERO;
        for (row, wv) in down.iter().enumerate() {
            for (column, wu) in across.iter().enumerate() {
                moved = moved + offsets[row * self.columns + column] * (wu * wv);
            }
        }
        p + moved
    }

    /// The drawing's shapes bent as on `frame`. Images and text come back
    /// as they were.
    pub fn deform(&self, elements: &[Element], frame: u32) -> Vec<Element> {
        let offsets = self.offsets_at(frame);
        if offsets.len() != self.columns * self.rows || self.is_at_rest(frame) {
            return elements.to_vec();
        }
        // Pieces short enough to bend smoothly across a grid cell.
        let cell = (self.bounds.width() / self.columns as f32).min(self.bounds.height() / self.rows as f32);
        let piece = (cell / 4.0).max(1.0);
        let bend = |p: Point| self.warp_point(&offsets, p);
        elements
            .iter()
            .map(|element| {
                let (anchors, closed) = element.shape.to_anchors();
                let bent: Vec<Anchor> = subdivide(&anchors, closed, piece)
                    .into_iter()
                    .map(|a| Anchor {
                        point: bend(a.point),
                        handle_in: bend(a.handle_in),
                        handle_out: bend(a.handle_out),
                    })
                    .collect();
                match path_from_anchors(&bent, closed) {
                    Some(path) => Element { shape: Shape::Path(path), style: element.style.clone() },
                    None => element.clone(),
                }
            })
            .collect()
    }

    pub fn key_frames(&self) -> Vec<u32> {
        let mut frames: Vec<u32> = self.offsets.iter().flat_map(|c| c.key_frames()).collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    pub fn insert_key(&mut self, frame: u32) -> bool {
        self.offsets.iter_mut().fold(false, |added, c| c.insert_key(frame) | added)
    }

    pub fn remove_key(&mut self, frame: u32) -> bool {
        self.offsets.iter_mut().fold(false, |removed, c| c.remove_key(frame) | removed)
    }

    pub fn move_key(&mut self, from: u32, to: u32) {
        for channel in &mut self.offsets {
            channel.move_key(from, to);
        }
    }
}

/// The Bernstein polynomials of degree `n` at `t`: how strongly each of
/// `n + 1` control points along a line pulls at `t`. They sum to 1.
pub(crate) fn bernstein(n: usize, t: f32) -> Vec<f32> {
    let choose = |k: usize| (0..k).fold(1.0f32, |c, i| c * (n - i) as f32 / (i + 1) as f32);
    (0..=n).map(|k| choose(k) * t.powi(k as i32) * (1.0 - t).powi((n - k) as i32)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Style};

    fn square() -> Element {
        Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }
    }

    fn bounds() -> Rect {
        Rect::from_center_size(Point::ZERO, 100.0, 100.0)
    }

    #[test]
    fn an_unbent_grid_changes_nothing() {
        let warp = Warp::over(bounds(), 3, 3, 0);
        assert_eq!(warp.deform(&[square()], 0), [square()]);
    }

    #[test]
    fn dragging_a_corner_pulls_that_corner_and_leaves_the_far_one() {
        let mut warp = Warp::over(bounds(), 3, 3, 0);
        // The bottom-right control point, 30 further right.
        warp.move_point(8, Point::new(80.0, 50.0), 0);
        let bent = warp.deform(&[square()], 0);
        let Shape::Path(path) = &bent[0].shape else { panic!("a path") };
        let points = path.flatten();
        assert!(points.iter().any(|p| p.distance(Point::new(80.0, 50.0)) < 1e-3), "corner follows exactly");
        assert!(points.iter().any(|p| p.distance(Point::new(-50.0, -50.0)) < 1e-3), "far corner stays");
    }

    #[test]
    fn warps_animate_between_keys() {
        let mut warp = Warp::over(bounds(), 2, 2, 0);
        warp.move_point(3, Point::new(70.0, 50.0), 10);
        assert!(warp.is_at_rest(0));
        let halfway = warp.points_at(5)[3];
        assert!((halfway.x - 60.0).abs() < 1e-3, "{halfway:?}");
        assert_eq!(warp.key_frames(), [0, 10]);
    }

    #[test]
    fn bernstein_weights_sum_to_one() {
        for n in 1..6 {
            for t in [0.0, 0.3, 0.5, 1.0] {
                let sum: f32 = bernstein(n, t).iter().sum();
                assert!((sum - 1.0).abs() < 1e-5);
            }
        }
    }
}
