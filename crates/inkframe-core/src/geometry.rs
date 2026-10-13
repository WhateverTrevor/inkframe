//! Small 2D math types.
//!
//! We deliberately avoid pulling in a big linear-algebra crate: the editor
//! only needs points, axis-aligned rectangles and 2D affine transforms.
//! Coordinates follow screen conventions — x grows right, y grows *down* —
//! which matches tiny-skia, egui and every video format we export to.

use std::ops::{Add, Mul, Neg, Sub};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn distance(self, other: Point) -> f32 {
        (self - other).length()
    }

    pub fn dot(self, other: Point) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn lerp(self, other: Point, t: f32) -> Point {
        self + (other - self) * t
    }

    /// Shortest distance from `self` to the line segment `a`–`b`.
    pub fn distance_to_segment(self, a: Point, b: Point) -> f32 {
        let ab = b - a;
        let len_sq = ab.dot(ab);
        if len_sq == 0.0 {
            return self.distance(a);
        }
        // Project onto the infinite line, then clamp to the segment's ends.
        let t = ((self - a).dot(ab) / len_sq).clamp(0.0, 1.0);
        self.distance(a + ab * t)
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Point {
    type Output = Point;
    fn mul(self, rhs: f32) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

impl Neg for Point {
    type Output = Point;
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    /// The rectangle spanned by two corners, in any order.
    pub fn from_points(a: Point, b: Point) -> Self {
        Self { min: Point::new(a.x.min(b.x), a.y.min(b.y)), max: Point::new(a.x.max(b.x), a.y.max(b.y)) }
    }

    pub fn from_center_size(center: Point, width: f32, height: f32) -> Self {
        let half = Point::new(width / 2.0, height / 2.0);
        Self { min: center - half, max: center + half }
    }

    /// The smallest rectangle containing every point, or `None` if empty.
    pub fn bounding(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        Some(points.fold(Rect::from_points(first, first), |r, p| r.including(p)))
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Point {
        self.min.lerp(self.max, 0.5)
    }

    pub fn including(self, p: Point) -> Rect {
        Rect {
            min: Point::new(self.min.x.min(p.x), self.min.y.min(p.y)),
            max: Point::new(self.max.x.max(p.x), self.max.y.max(p.y)),
        }
    }

    pub fn union(self, other: Rect) -> Rect {
        self.including(other.min).including(other.max)
    }

    pub fn expand(self, amount: f32) -> Rect {
        let d = Point::new(amount, amount);
        Rect { min: self.min - d, max: self.max + d }
    }

    pub fn contains(&self, p: Point) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }

    /// Corners in clockwise order starting at the top-left.
    pub fn corners(&self) -> [Point; 4] {
        [self.min, Point::new(self.max.x, self.min.y), self.max, Point::new(self.min.x, self.max.y)]
    }
}

/// A 2D affine transform (translation, rotation, scale, skew).
///
/// It maps a point like this:
///
/// ```text
/// x' = a·x + c·y + e
/// y' = b·x + d·y + f
/// ```
///
/// The field order matches tiny-skia's `Transform::from_row(sx, ky, kx, sy, tx, ty)`,
/// so converting between the two is a straight copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(offset: Point) -> Self {
        Affine { e: offset.x, f: offset.y, ..Self::IDENTITY }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Affine { a: sx, d: sy, ..Self::IDENTITY }
    }

    /// Rotation by `radians`. Because y points down, positive angles turn
    /// clockwise on screen.
    pub fn rotate(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Affine { a: cos, b: sin, c: -sin, d: cos, e: 0.0, f: 0.0 }
    }

    /// Composes two transforms: the result applies `inner` first, then `self`.
    ///
    /// Reading `outer.pre_concat(inner)` left to right matches how the math
    /// is usually written: `outer · inner · point`.
    pub fn pre_concat(self, inner: Affine) -> Affine {
        let o = self;
        let i = inner;
        Affine {
            a: o.a * i.a + o.c * i.b,
            b: o.b * i.a + o.d * i.b,
            c: o.a * i.c + o.c * i.d,
            d: o.b * i.c + o.d * i.d,
            e: o.a * i.e + o.c * i.f + o.e,
            f: o.b * i.e + o.d * i.f + o.f,
        }
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    /// The transform that undoes this one, or `None` if it collapses space
    /// (for example a scale of zero), in which case nothing can be mapped back.
    pub fn inverse(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Affine {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            e: (self.c * self.f - self.d * self.e) * inv,
            f: (self.b * self.e - self.a * self.f) * inv,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-4
    }

    #[test]
    fn rotate_is_clockwise_on_screen() {
        // Rotating the "right" vector by 90° should point it down (+y).
        let p = Affine::rotate(90f32.to_radians()).apply(Point::new(1.0, 0.0));
        assert!(approx(p, Point::new(0.0, 1.0)));
    }

    #[test]
    fn pre_concat_applies_inner_first() {
        let m = Affine::translate(Point::new(10.0, 0.0)).pre_concat(Affine::scale(2.0, 2.0));
        assert!(approx(m.apply(Point::new(1.0, 1.0)), Point::new(12.0, 2.0)));
    }

    #[test]
    fn inverse_round_trips() {
        let m = Affine::translate(Point::new(5.0, -3.0))
            .pre_concat(Affine::rotate(0.7))
            .pre_concat(Affine::scale(2.0, 0.5));
        let p = Point::new(3.0, 4.0);
        assert!(approx(m.inverse().unwrap().apply(m.apply(p)), p));
    }

    #[test]
    fn zero_scale_has_no_inverse() {
        assert!(Affine::scale(0.0, 1.0).inverse().is_none());
    }

    #[test]
    fn segment_distance_clamps_to_ends() {
        let d = Point::new(-3.0, 4.0).distance_to_segment(Point::ZERO, Point::new(10.0, 0.0));
        assert!((d - 5.0).abs() < 1e-6);
    }
}
