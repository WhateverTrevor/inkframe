use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::geometry::{Point, Rect};

/// Geometry of a drawable shape, in the owning layer's local coordinates.
///
/// Rectangles and ellipses are centered on the local origin, so the layer's
/// position is their center and they rotate around it. Paths are stored with
/// absolute local coordinates, and the tools that create them recenter the
/// points around the origin for the same reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    Rect {
        width: f32,
        height: f32,
        #[serde(default)]
        corner_radius: f32,
    },
    Ellipse {
        radius_x: f32,
        radius_y: f32,
    },
    Path(Path),
}

/// A sequence of connected line and curve segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub start: Point,
    pub segments: Vec<Segment>,
    /// Closed paths connect the last point back to `start` and can be filled.
    #[serde(default)]
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Segment {
    Line {
        to: Point,
    },
    /// A cubic Bézier curve. The control points pull the curve toward them
    /// without (usually) passing through them.
    Cubic {
        ctrl1: Point,
        ctrl2: Point,
        to: Point,
    },
}

impl Segment {
    pub fn end(&self) -> Point {
        match *self {
            Segment::Line { to } | Segment::Cubic { to, .. } => to,
        }
    }
}

/// How a shape is painted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

impl Path {
    /// Approximates the path with straight lines, for hit-testing.
    ///
    /// Each curve is split into a fixed number of pieces. That's cruder than
    /// adaptive subdivision but plenty accurate for deciding what the user
    /// clicked on, which is all we use it for.
    pub fn flatten(&self) -> Vec<Point> {
        const STEPS_PER_CURVE: usize = 16;

        let mut points = vec![self.start];
        let mut current = self.start;
        for segment in &self.segments {
            match *segment {
                Segment::Line { to } => points.push(to),
                Segment::Cubic { ctrl1, ctrl2, to } => {
                    for step in 1..=STEPS_PER_CURVE {
                        let t = step as f32 / STEPS_PER_CURVE as f32;
                        points.push(cubic_point(current, ctrl1, ctrl2, to, t));
                    }
                }
            }
            current = segment.end();
        }
        if self.closed {
            points.push(self.start);
        }
        points
    }

    /// Every point that defines the path, including Bézier control points.
    fn control_hull(&self) -> impl Iterator<Item = Point> + '_ {
        std::iter::once(self.start).chain(self.segments.iter().flat_map(|s| match *s {
            Segment::Line { to } => vec![to],
            Segment::Cubic { ctrl1, ctrl2, to } => vec![ctrl1, ctrl2, to],
        }))
    }

    /// Moves every point by `offset`.
    pub fn translate(&mut self, offset: Point) {
        self.start = self.start + offset;
        for segment in &mut self.segments {
            match segment {
                Segment::Line { to } => *to = *to + offset,
                Segment::Cubic { ctrl1, ctrl2, to } => {
                    *ctrl1 = *ctrl1 + offset;
                    *ctrl2 = *ctrl2 + offset;
                    *to = *to + offset;
                }
            }
        }
    }
}

impl Shape {
    /// A rectangle containing the whole shape (not including stroke width).
    ///
    /// For paths this uses the Bézier control points, which always enclose
    /// the curve but may be a little looser than the true bounds.
    pub fn bounds(&self) -> Rect {
        match self {
            Shape::Rect { width, height, .. } => Rect::from_center_size(Point::ZERO, *width, *height),
            Shape::Ellipse { radius_x, radius_y } => {
                Rect::from_center_size(Point::ZERO, radius_x * 2.0, radius_y * 2.0)
            }
            Shape::Path(path) => {
                Rect::bounding(path.control_hull()).unwrap_or(Rect::from_points(Point::ZERO, Point::ZERO))
            }
        }
    }

    /// Whether `p` (in local coordinates) lands on the visible shape.
    ///
    /// `tolerance` widens the target, which makes thin strokes clickable. It
    /// should be a few screen pixels converted into local units.
    pub fn hit_test(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        let half_stroke = style.stroke.map_or(0.0, |s| s.width / 2.0);
        let reach = half_stroke + tolerance;

        match self {
            Shape::Rect { width, height, .. } => {
                // Rounded corners are ignored here; clicking a hair outside a
                // rounded corner still selecting the shape feels fine.
                Rect::from_center_size(Point::ZERO, *width, *height).expand(reach).contains(p)
            }
            Shape::Ellipse { radius_x, radius_y } => {
                let rx = radius_x + reach;
                let ry = radius_y + reach;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                (p.x / rx).powi(2) + (p.y / ry).powi(2) <= 1.0
            }
            Shape::Path(path) => {
                let outline = path.flatten();
                let near_outline = outline.windows(2).any(|w| p.distance_to_segment(w[0], w[1]) <= reach);
                near_outline || (path.closed && style.fill.is_some() && point_in_polygon(p, &outline))
            }
        }
    }
}

/// Evaluates a cubic Bézier at parameter `t` (0 = start, 1 = end).
pub(crate) fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let inv = 1.0 - t;
    p0 * (inv * inv * inv) + p1 * (3.0 * inv * inv * t) + p2 * (3.0 * inv * t * t) + p3 * (t * t * t)
}

/// Even-odd point-in-polygon test: cast a ray to the right of `p` and count
/// how many edges it crosses. An odd count means `p` is inside.
fn point_in_polygon(p: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    for edge in polygon.windows(2) {
        let (a, b) = (edge[0], edge[1]);
        let straddles = (a.y > p.y) != (b.y > p.y);
        if straddles {
            // x coordinate where the edge crosses the ray's horizontal line.
            let cross_x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < cross_x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILLED: Style = Style { fill: Some(Color::BLACK), stroke: None };

    fn triangle() -> Path {
        Path {
            start: Point::new(0.0, 0.0),
            segments: vec![
                Segment::Line { to: Point::new(10.0, 0.0) },
                Segment::Line { to: Point::new(0.0, 10.0) },
            ],
            closed: true,
        }
    }

    #[test]
    fn rect_is_centered_on_origin() {
        let shape = Shape::Rect { width: 20.0, height: 10.0, corner_radius: 0.0 };
        assert!(shape.hit_test(Point::new(-9.0, 4.0), &FILLED, 0.0));
        assert!(!shape.hit_test(Point::new(11.0, 0.0), &FILLED, 0.0));
        assert!(shape.hit_test(Point::new(11.0, 0.0), &FILLED, 2.0));
    }

    #[test]
    fn ellipse_excludes_its_bounding_box_corners() {
        let shape = Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 };
        assert!(shape.hit_test(Point::new(6.0, 6.0), &FILLED, 0.0));
        assert!(!shape.hit_test(Point::new(9.0, 9.0), &FILLED, 0.0));
    }

    #[test]
    fn filled_closed_path_hits_inside() {
        let shape = Shape::Path(triangle());
        assert!(shape.hit_test(Point::new(2.0, 2.0), &FILLED, 0.0));
        assert!(!shape.hit_test(Point::new(8.0, 8.0), &FILLED, 0.0));
    }

    #[test]
    fn unfilled_path_only_hits_near_the_line() {
        let stroked = Style { fill: None, stroke: Some(Stroke { color: Color::BLACK, width: 2.0 }) };
        let shape = Shape::Path(triangle());
        assert!(!shape.hit_test(Point::new(2.0, 2.0), &stroked, 0.0));
        assert!(shape.hit_test(Point::new(5.0, 0.5), &stroked, 0.0));
    }

    #[test]
    fn cubic_point_hits_endpoints() {
        let (a, b, c, d) = (Point::ZERO, Point::new(1.0, 5.0), Point::new(4.0, 5.0), Point::new(5.0, 0.0));
        assert_eq!(cubic_point(a, b, c, d, 0.0), a);
        assert_eq!(cubic_point(a, b, c, d, 1.0), d);
    }
}
