use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::geometry::{Point, Rect};
use crate::image::ImageId;
use crate::paint::CanvasId;
use crate::text::Text;

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
    /// A bitmap from the document's images, stretched to `width`×`height`
    /// and centered on the origin like a rectangle. Its element's style is
    /// ignored: an image paints its own pixels.
    Image {
        image: ImageId,
        width: f32,
        height: f32,
    },
    /// Lines of text, painted with the element's fill and stroke.
    Text(Text),
    /// A pressure-sensitive line: a centreline with its width at each anchor,
    /// drawn as the outline it sweeps out, filled (see `line.rs`).
    Line {
        path: Path,
        widths: Vec<f32>,
    },
    /// Painted pixels: one of the document's canvases (see `paint.rs`),
    /// one pixel per unit, centered on the origin. Its element's style is
    /// ignored.
    Paint {
        canvas: CanvasId,
        width: u32,
        height: u32,
    },
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// The colour inside. With a gradient, it's what the gradient starts
    /// from, and what everything that wants one colour (the paint bucket,
    /// hit-testing, palettes) goes by.
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    /// A gradient painted inside instead of the flat fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<ShapeGradient>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    /// How open ends finish.
    #[serde(default, skip_serializing_if = "is_default")]
    pub cap: LineCap,
    /// How corners are drawn.
    #[serde(default, skip_serializing_if = "is_default")]
    pub join: LineJoin,
    /// Broken into dashes (or dots, with round caps and a short length).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<Dash>,
    /// Drawn as a symbol's picture laid along it, rather than a plain
    /// line (see `art_brush.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brush: Option<crate::StrokeBrush>,
}

impl Stroke {
    /// A plain line: round ends and corners, not dashed.
    pub fn new(color: Color, width: f32) -> Self {
        Self { color, width, cap: LineCap::default(), join: LineJoin::default(), dash: None, brush: None }
    }
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// How a line's open ends finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCap {
    /// Rounded off past the end, as a pen would. What every line had before
    /// there was a choice.
    #[default]
    Round,
    /// Cut square at the end.
    Butt,
    /// Squared off half the line's width past the end.
    Square,
}

/// How a line turns its corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineJoin {
    #[default]
    Round,
    /// Sharp, as far as a point can reach.
    Miter,
    /// Cut off flat.
    Bevel,
}

/// A dashed line: each dash `length` long, then a `gap`, both in the
/// shape's units. A dash of nearly nothing with round caps makes dots.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Dash {
    pub length: f32,
    pub gap: f32,
}

impl Dash {
    pub fn dashed(width: f32) -> Self {
        Self { length: width * 3.0, gap: width * 2.0 }
    }

    /// Dots one line-width apart, given round caps.
    pub fn dotted(width: f32) -> Self {
        Self { length: 0.0, gap: width * 2.0 }
    }

    pub fn is_dotted(&self) -> bool {
        self.length <= f32::EPSILON
    }
}

/// A fill that blends between colours across a shape, as Animate's linear
/// and radial gradients do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeGradient {
    #[serde(default)]
    pub shape: crate::GradientShape,
    /// The colours, each at a place from 0 (the start) to 1 (the end),
    /// in order.
    pub stops: Vec<GradientStop>,
    /// Where it starts and ends, as fractions of the shape's box: (0, 0) is
    /// its top left and (1, 1) its bottom right. Kept relative so the
    /// gradient follows the shape when it's moved, resized or reshaped. A
    /// radial gradient is centred on `from` and reaches its last colour at
    /// `to`.
    pub from: Point,
    pub to: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub at: f32,
    pub color: Color,
}

impl ShapeGradient {
    /// Left to right across the shape, from `start` to `end`.
    pub fn linear(start: Color, end: Color) -> Self {
        Self {
            shape: crate::GradientShape::Linear,
            stops: vec![GradientStop { at: 0.0, color: start }, GradientStop { at: 1.0, color: end }],
            from: Point::new(0.0, 0.5),
            to: Point::new(1.0, 0.5),
        }
    }

    /// From the middle of the shape out to its sides.
    pub fn radial(start: Color, end: Color) -> Self {
        Self {
            shape: crate::GradientShape::Radial,
            from: Point::new(0.5, 0.5),
            to: Point::new(1.0, 0.5),
            ..Self::linear(start, end)
        }
    }

    /// The start and end in the shape's own coordinates, given its box.
    pub fn ends_in(&self, bounds: Rect) -> (Point, Point) {
        let place =
            |p: Point| Point::new(bounds.min.x + p.x * bounds.width(), bounds.min.y + p.y * bounds.height());
        (place(self.from), place(self.to))
    }

    /// The colour at `t` along it (0 to 1), blending between the stops.
    pub fn color_at(&self, t: f32) -> Color {
        let (Some(first), Some(last)) = (self.stops.first(), self.stops.last()) else { return Color::BLACK };
        if t <= first.at {
            return first.color;
        }
        if t >= last.at {
            return last.color;
        }
        let next = self.stops.partition_point(|s| s.at <= t);
        let (a, b) = (self.stops[next - 1], self.stops[next]);
        let f = (t - a.at) / (b.at - a.at).max(1e-6);
        let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
        Color {
            r: mix(a.color.r, b.color.r),
            g: mix(a.color.g, b.color.g),
            b: mix(a.color.b, b.color.b),
            a: mix(a.color.a, b.color.a),
        }
    }

    /// Keeps the stops in order and within 0 to 1, after editing.
    pub fn tidy(&mut self) {
        for stop in &mut self.stops {
            stop.at = stop.at.clamp(0.0, 1.0);
        }
        self.stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    }
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

    /// The smallest rectangle containing the path's outline.
    ///
    /// The control points of a curve can sit far outside the curve itself
    /// (think of a gentle arc with long handles), so boxing those would leave
    /// a visible gap around the shape. Instead, each curve contributes its
    /// endpoints plus the points where it turns around horizontally or
    /// vertically: its extremes, see [`cubic_extremes`].
    fn tight_bounds(&self) -> Rect {
        let mut bounds = Rect::from_points(self.start, self.start);
        let mut from = self.start;
        for segment in &self.segments {
            match *segment {
                Segment::Line { to } => bounds = bounds.including(to),
                Segment::Cubic { ctrl1, ctrl2, to } => {
                    for t in cubic_extremes(from, ctrl1, ctrl2, to) {
                        bounds = bounds.including(cubic_point(from, ctrl1, ctrl2, to, t));
                    }
                    bounds = bounds.including(to);
                }
            }
            from = segment.end();
        }
        bounds
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
    /// The smallest rectangle containing the shape's outline, not counting
    /// stroke width. For what's actually painted, use [`Element::bounds`].
    ///
    /// [`Element::bounds`]: crate::Element::bounds
    pub fn bounds(&self) -> Rect {
        match self {
            Shape::Rect { width, height, .. } => Rect::from_center_size(Point::ZERO, *width, *height),
            Shape::Ellipse { radius_x, radius_y } => {
                Rect::from_center_size(Point::ZERO, radius_x * 2.0, radius_y * 2.0)
            }
            Shape::Path(path) => path.tight_bounds(),
            Shape::Line { path, widths } => crate::line::line_outline(path, widths).tight_bounds(),
            Shape::Image { width, height, .. } => Rect::from_center_size(Point::ZERO, *width, *height),
            Shape::Text(text) => text.measured,
            Shape::Paint { width, height, .. } => {
                Rect::from_center_size(Point::ZERO, *width as f32, *height as f32)
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
            // Clicking anywhere on an image selects it, transparent parts
            // included. They're usually backgrounds or reference art, where
            // having to find an opaque pixel would just get in the way.
            Shape::Image { width, height, .. } => {
                Rect::from_center_size(Point::ZERO, *width, *height).expand(tolerance).contains(p)
            }
            // The whole canvas. The document checks the actual pixels (see
            // `Document::element_hit`), which a shape can't see.
            Shape::Paint { width, height, .. } => {
                Rect::from_center_size(Point::ZERO, *width as f32, *height as f32).contains(p)
            }
            // Anywhere in the block selects it, gaps between letters
            // included; aiming for the inside of an "o" would be maddening.
            Shape::Text(text) => text.measured.expand(reach).contains(p),
            Shape::Ellipse { radius_x, radius_y } => {
                let rx = radius_x + reach;
                let ry = radius_y + reach;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                (p.x / rx).powi(2) + (p.y / ry).powi(2) <= 1.0
            }
            Shape::Line { path, widths } => {
                let outline = crate::line::line_outline(path, widths).flatten();
                let near_outline = outline.windows(2).any(|w| p.distance_to_segment(w[0], w[1]) <= tolerance);
                near_outline || point_in_polygon(p, &outline)
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

/// The parameters (strictly between 0 and 1) where a cubic Bézier stops
/// moving in x or in y and turns back, which is where it reaches past its
/// endpoints. At most two per axis.
///
/// These are where the curve's derivative is zero. The derivative of a cubic
/// Bézier is a quadratic Bézier on the differences of its points:
///
/// ```text
/// B'(t) / 3 = (1-t)² a + 2(1-t)t b + t² c,   a = p1-p0, b = p2-p1, c = p3-p2
///           = (a - 2b + c) t² + 2(b - a) t + a
/// ```
///
/// so each axis is a quadratic equation, solved with the usual formula.
fn cubic_extremes(p0: Point, p1: Point, p2: Point, p3: Point) -> impl Iterator<Item = f32> {
    let axis = |p0: f32, p1: f32, p2: f32, p3: f32| {
        let (a, b, c) = (p1 - p0, p2 - p1, p3 - p2);
        quadratic_roots(a - 2.0 * b + c, 2.0 * (b - a), a)
    };
    axis(p0.x, p1.x, p2.x, p3.x)
        .into_iter()
        .chain(axis(p0.y, p1.y, p2.y, p3.y))
        .flatten()
        .filter(|t| *t > 0.0 && *t < 1.0)
}

/// Real solutions of `a t² + b t + c = 0`.
fn quadratic_roots(a: f32, b: f32, c: f32) -> [Option<f32>; 2] {
    // When the handles are evenly spaced, the t² term vanishes and the
    // equation is linear (or, if b is zero too, has no single solution).
    if a.abs() < 1e-6 {
        return [(b.abs() > 1e-6).then(|| -c / b), None];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return [None, None];
    }
    let root = discriminant.sqrt();
    [Some((-b + root) / (2.0 * a)), Some((-b - root) / (2.0 * a))]
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

    const FILLED: Style = Style { fill: Some(Color::BLACK), stroke: None, gradient: None };

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
        let stroked = Style { fill: None, stroke: Some(Stroke::new(Color::BLACK, 2.0)), gradient: None };
        let shape = Shape::Path(triangle());
        assert!(!shape.hit_test(Point::new(2.0, 2.0), &stroked, 0.0));
        assert!(shape.hit_test(Point::new(5.0, 0.5), &stroked, 0.0));
    }

    #[test]
    fn curve_bounds_hug_the_curve_not_its_handles() {
        // An arch from (0,0) to (10,0) whose handles reach up to y = -8. The
        // curve itself only rises three quarters of the way: to y = -6.
        let arch = Path {
            start: Point::ZERO,
            segments: vec![Segment::Cubic {
                ctrl1: Point::new(0.0, -8.0),
                ctrl2: Point::new(10.0, -8.0),
                to: Point::new(10.0, 0.0),
            }],
            closed: false,
        };
        let bounds = Shape::Path(arch).bounds();
        assert!((bounds.min.y - -6.0).abs() < 1e-4, "top was {}", bounds.min.y);
        assert_eq!((bounds.min.x, bounds.max.x, bounds.max.y), (0.0, 10.0, 0.0));
    }

    #[test]
    fn curve_bounds_include_overshoot_past_the_endpoints() {
        // An S-curve whose handles swing out sideways: the curve bulges past
        // x = 0 and x = 10 even though both endpoints are inside that range.
        let s_curve = Path {
            start: Point::ZERO,
            segments: vec![Segment::Cubic {
                ctrl1: Point::new(-10.0, 5.0),
                ctrl2: Point::new(20.0, 5.0),
                to: Point::new(10.0, 10.0),
            }],
            closed: false,
        };
        let bounds = Shape::Path(s_curve).bounds();
        assert!(bounds.min.x < -1.0 && bounds.max.x > 11.0);
        // And it never reaches the handles themselves.
        assert!(bounds.min.x > -10.0 && bounds.max.x < 20.0);
    }

    #[test]
    fn cubic_point_hits_endpoints() {
        let (a, b, c, d) = (Point::ZERO, Point::new(1.0, 5.0), Point::new(4.0, 5.0), Point::new(5.0, 0.0));
        assert_eq!(cubic_point(a, b, c, d, 0.0), a);
        assert_eq!(cubic_point(a, b, c, d, 1.0), d);
    }
}
