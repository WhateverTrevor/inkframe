//! Ink and paint: filling the areas a drawing's lines enclose, as Harmony's
//! Paint tool does.
//!
//! A drawing has line art (its `elements`, the inked lines) and colour art
//! (`colour_art`, the fills), and the colour art is drawn beneath. Clicking
//! inside an area of the line art makes a fill there, shaped to the lines:
//!
//! 1. The line art is drawn into a grid of blocked and open cells, each
//!    line thickened by the gap-closing distance so that small gaps where
//!    lines don't quite meet count as closed.
//! 2. The open cells reachable from the click are flooded. Reaching the
//!    grid's edge means the area isn't enclosed, and nothing is filled.
//! 3. The flooded area is grown back by that thickening, so the fill runs
//!    under the lines rather than stopping short and leaving a sliver.
//! 4. Its outline is traced and simplified into a closed path.
//!
//! Filling an area that already has a fill recolours it instead. Areas
//! inside the filled one (an eye inside a face) are covered too, under
//! their own lines; filling them afterwards puts their own fill on top.

use std::collections::VecDeque;

use crate::color::Color;
use crate::document::{Element, Layer};
use crate::geometry::{Point, Rect, simplify};
use crate::shape::{Path, Segment, Shape, Style};

/// What a fill did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillResult {
    /// A new fill was added to the colour art.
    Filled,
    /// An existing fill there took the colour.
    Recoloured,
    /// The point isn't inside an area the lines enclose.
    NotEnclosed,
}

/// The most grid cells along a side: fine enough to follow the lines,
/// small enough to fill in a blink.
const MAX_CELLS: f32 = 1200.0;

impl Layer {
    /// Whether the layer's drawings are vector shapes to draw and fill in:
    /// not a symbol instance, a paint layer, a folder or the camera.
    pub fn draws_vectors(&self) -> bool {
        self.instance.is_none()
            && !self.folder
            && !self.camera
            && self.adjustment.is_none()
            && !self.all_elements().any(|e| matches!(e.shape, Shape::Paint { .. }))
    }

    /// Fills the area of the line art around `point` (in the layer's own
    /// coordinates) in the drawing showing on `frame`, closing gaps up to
    /// `gap` units wide; or recolours the fill already there.
    pub fn fill_enclosed(&mut self, frame: u32, point: Point, color: Color, gap: f32) -> FillResult {
        let index = self.drawing_index_at(frame);
        let drawing = &mut self.drawings[index];
        let hit = drawing.colour_art.iter().rposition(|e| e.shape.hit_test(point, &e.style, 0.0));
        let Some(path) = enclosed_area(&drawing.elements, point, gap) else {
            // Not in a closed area of the lines, but maybe on a fill (one
            // whose lines have since been changed): recolour that.
            return match hit {
                Some(i) => {
                    drawing.colour_art[i].style.fill = Some(color);
                    FillResult::Recoloured
                }
                None => FillResult::NotEnclosed,
            };
        };
        // A fill already covering just this area takes the new colour. One
        // covering a bigger area (a face, around an eye being filled) stays,
        // with the new fill over it.
        let same = |e: &Element| {
            let (a, b) = (e.shape.bounds(), Shape::Path(path.clone()).bounds());
            a.min.distance(b.min) + a.max.distance(b.max) < 2.0 + 0.02 * (b.width() + b.height())
        };
        if let Some(i) = hit.filter(|&i| same(&drawing.colour_art[i])) {
            drawing.colour_art[i].style.fill = Some(color);
            return FillResult::Recoloured;
        }
        drawing.colour_art.push(Element {
            shape: Shape::Path(path),
            style: Style { fill: Some(color), stroke: None, gradient: None },
        });
        FillResult::Filled
    }
}

/// The outline of the area of `lines` around `point`, or `None` if it isn't
/// enclosed.
pub fn enclosed_area(lines: &[Element], point: Point, gap: f32) -> Option<Path> {
    let segments = line_segments(lines);
    let bounds = segments
        .iter()
        .flat_map(|s| [s.0, s.1])
        .fold(None::<Rect>, |r, p| Some(r.map_or(Rect::from_points(p, p), |r| r.including(p))))?;
    if !bounds.contains(point) {
        return None;
    }
    let widest = segments.iter().map(|s| s.2).fold(0.0, f32::max);
    // Room round the lines for the flood to go round them, and so reaching
    // the edge means getting out.
    let margin = gap + widest + 2.0;
    let area = bounds.expand(margin);
    let cell = (area.width().max(area.height()) / MAX_CELLS).max(0.5);
    let grid = Grid::new(area, cell);

    // 1. Lines, thickened to close gaps.
    let mut blocked = vec![false; grid.width * grid.height];
    for &(a, b, half_width) in &segments {
        grid.mark_near(&mut blocked, a, b, half_width + gap / 2.0);
    }
    // 2. Flood from the click.
    let start = grid.cell_at(point)?;
    if blocked[start] {
        return None;
    }
    let mut inside = vec![false; blocked.len()];
    let mut queue = VecDeque::from([start]);
    inside[start] = true;
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % grid.width, i / grid.width);
        if x == 0 || y == 0 || x == grid.width - 1 || y == grid.height - 1 {
            return None; // Out past the lines: not enclosed.
        }
        for j in [i - 1, i + 1, i - grid.width, i + grid.width] {
            if !blocked[j] && !inside[j] {
                inside[j] = true;
                queue.push_back(j);
            }
        }
    }
    // 3. Under the lines, as far as they were thickened.
    let reach = ((gap / 2.0 + widest) / cell).ceil() as usize;
    let inside = grid.grow(&inside, reach);
    // 4. The outline.
    let outline = grid.trace(&inside)?;
    let points = simplify(&outline, cell * 0.75);
    if points.len() < 3 {
        return None;
    }
    Some(Path {
        start: points[0],
        segments: points[1..].iter().map(|&to| Segment::Line { to }).collect(),
        closed: true,
    })
}

/// Every line of the line art as straight pieces, each with half its
/// stroke's width. Shapes without a stroke still bound areas along their
/// outline.
fn line_segments(lines: &[Element]) -> Vec<(Point, Point, f32)> {
    let mut segments = Vec::new();
    for element in lines {
        let (anchors, closed) = element.shape.to_anchors();
        let Some(path) = crate::anchor::path_from_anchors(&anchors, closed) else { continue };
        let half = element.style.stroke.map_or(0.5, |s| s.width / 2.0);
        let points = path.flatten();
        segments.extend(points.windows(2).map(|w| (w[0], w[1], half)));
    }
    segments
}

/// A grid of square cells over an area.
struct Grid {
    origin: Point,
    cell: f32,
    width: usize,
    height: usize,
}

impl Grid {
    fn new(area: Rect, cell: f32) -> Self {
        let width = (area.width() / cell).ceil() as usize + 1;
        let height = (area.height() / cell).ceil() as usize + 1;
        Self { origin: area.min, cell, width, height }
    }

    fn cell_at(&self, p: Point) -> Option<usize> {
        let (x, y) = ((p.x - self.origin.x) / self.cell, (p.y - self.origin.y) / self.cell);
        (x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| y as usize * self.width + x as usize)
    }

    fn centre(&self, x: usize, y: usize) -> Point {
        Point::new(self.origin.x + (x as f32 + 0.5) * self.cell, self.origin.y + (y as f32 + 0.5) * self.cell)
    }

    /// Marks the cells within `radius` of the segment from `a` to `b`.
    fn mark_near(&self, cells: &mut [bool], a: Point, b: Point, radius: f32) {
        let to_cell = |v: f32, origin: f32, limit: usize| {
            ((v - origin) / self.cell).floor().clamp(0.0, (limit - 1) as f32) as usize
        };
        let (x0, x1) = (
            to_cell(a.x.min(b.x) - radius, self.origin.x, self.width),
            to_cell(a.x.max(b.x) + radius, self.origin.x, self.width),
        );
        let (y0, y1) = (
            to_cell(a.y.min(b.y) - radius, self.origin.y, self.height),
            to_cell(a.y.max(b.y) + radius, self.origin.y, self.height),
        );
        // Half a cell's diagonal more, so a thin line can't slip between
        // cell centres.
        let reach = radius + self.cell * 0.71;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if self.centre(x, y).distance_to_segment(a, b) <= reach {
                    cells[y * self.width + x] = true;
                }
            }
        }
    }

    /// The cells within `steps` cells of any marked one (a square spread,
    /// near enough for growing a fill under a line).
    fn grow(&self, cells: &[bool], steps: usize) -> Vec<bool> {
        let mut grown = cells.to_vec();
        for _ in 0..steps {
            let before = grown.clone();
            for y in 1..self.height - 1 {
                for x in 1..self.width - 1 {
                    let i = y * self.width + x;
                    if !before[i]
                        && (before[i - 1]
                            || before[i + 1]
                            || before[i - self.width]
                            || before[i + self.width])
                    {
                        grown[i] = true;
                    }
                }
            }
        }
        grown
    }

    /// The outer edge of the marked cells, as cell corners in order round
    /// it once.
    ///
    /// The walk goes from corner to corner along cell edges, keeping marked
    /// cells on its right. It starts at the top-left corner of the first
    /// marked cell, heading right along its top edge. At each corner it
    /// looks at the two cells ahead: if the one ahead-left is marked the
    /// edge turns left, if only the one ahead-right is it carries straight
    /// on, and otherwise it turns right.
    fn trace(&self, cells: &[bool]) -> Option<Vec<Point>> {
        let marked = |x: isize, y: isize| {
            x >= 0
                && y >= 0
                && (x as usize) < self.width
                && (y as usize) < self.height
                && cells[y as usize * self.width + x as usize]
        };
        let corner = |x: isize, y: isize| {
            Point::new(self.origin.x + x as f32 * self.cell, self.origin.y + y as f32 * self.cell)
        };
        let first = cells.iter().position(|&c| c)?;
        let start = ((first % self.width) as isize, (first / self.width) as isize);
        // Right, down, left, up (y runs down the screen).
        const STEPS: [(isize, isize); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        let (mut x, mut y) = start;
        let mut heading = 0;
        let mut points = vec![corner(x, y)];
        for _ in 0..cells.len() * 4 {
            x += STEPS[heading].0;
            y += STEPS[heading].1;
            if (x, y) == start {
                return Some(points);
            }
            points.push(corner(x, y));
            // The cells ahead of corner (x, y), left and right of the
            // heading. Corner (x, y) is the top-left of cell (x, y).
            let (left, right) = match heading {
                0 => ((x, y - 1), (x, y)),
                1 => ((x, y), (x - 1, y)),
                2 => ((x - 1, y), (x - 1, y - 1)),
                _ => ((x - 1, y - 1), (x, y - 1)),
            };
            heading = if marked(left.0, left.1) {
                (heading + 3) % 4
            } else if marked(right.0, right.1) {
                heading
            } else {
                (heading + 1) % 4
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Drawing, Stroke, Transform};

    /// A 100×100 square drawn as four lines, with a `gap` between the last
    /// and the first.
    fn square_lines(gap: f32) -> Vec<Element> {
        let line = |a: Point, b: Point| Element {
            shape: Shape::Path(Path { start: a, segments: vec![Segment::Line { to: b }], closed: false }),
            style: Style { fill: None, stroke: Some(Stroke::new(Color::BLACK, 2.0)), gradient: None },
        };
        let p = Point::new;
        vec![
            line(p(0.0, 0.0), p(100.0, 0.0)),
            line(p(100.0, 0.0), p(100.0, 100.0)),
            line(p(100.0, 100.0), p(0.0, 100.0)),
            line(p(0.0, 100.0), p(0.0, gap)),
        ]
    }

    fn layer(lines: Vec<Element>) -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("Ink", Vec::new(), 0, Transform::default());
        let mut layer = doc.layer(id).unwrap().clone();
        layer.drawings[0] = Drawing::new("1", lines);
        layer
    }

    #[test]
    fn filling_inside_closed_lines_makes_a_fill_under_them() {
        let mut layer = layer(square_lines(0.0));
        let red = Color::rgb(255, 0, 0);
        assert_eq!(layer.fill_enclosed(0, Point::new(50.0, 50.0), red, 0.0), FillResult::Filled);
        let fill = &layer.drawings[0].colour_art[0];
        assert_eq!(fill.style.fill, Some(red));
        // It reaches under the lines: about the square, not short of it.
        let bounds = fill.shape.bounds();
        assert!(bounds.min.x < 1.5 && bounds.max.x > 98.5, "{bounds:?}");
        assert!(bounds.min.y < 1.5 && bounds.max.y > 98.5, "{bounds:?}");
        // And it's drawn under the line art.
        assert_eq!(layer.shown_elements(0)[0].style.fill, Some(red));
    }

    #[test]
    fn a_gap_leaks_unless_it_is_closed() {
        let mut leaky = layer(square_lines(10.0));
        assert_eq!(
            leaky.fill_enclosed(0, Point::new(50.0, 50.0), Color::BLACK, 0.0),
            FillResult::NotEnclosed
        );
        assert!(leaky.drawings[0].colour_art.is_empty());
        assert_eq!(leaky.fill_enclosed(0, Point::new(50.0, 50.0), Color::BLACK, 12.0), FillResult::Filled);
    }

    #[test]
    fn outside_the_lines_nothing_is_filled() {
        let mut layer = layer(square_lines(0.0));
        assert_eq!(
            layer.fill_enclosed(0, Point::new(150.0, 50.0), Color::BLACK, 0.0),
            FillResult::NotEnclosed
        );
    }

    #[test]
    fn filling_a_fill_again_recolours_it() {
        let mut layer = layer(square_lines(0.0));
        layer.fill_enclosed(0, Point::new(50.0, 50.0), Color::BLACK, 0.0);
        let blue = Color::rgb(0, 0, 255);
        assert_eq!(layer.fill_enclosed(0, Point::new(50.0, 50.0), blue, 0.0), FillResult::Recoloured);
        assert_eq!(layer.drawings[0].colour_art.len(), 1);
        assert_eq!(layer.drawings[0].colour_art[0].style.fill, Some(blue));
    }

    #[test]
    fn simplifying_keeps_corners_and_drops_points_along_straight_runs() {
        let points: Vec<Point> =
            (0..=10).map(|i| Point::new(i as f32, 0.0)).chain([Point::new(10.0, 10.0)]).collect();
        assert_eq!(
            simplify(&points, 0.1),
            [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)]
        );
    }

    #[test]
    fn an_area_inside_a_filled_one_gets_its_own_fill_on_top() {
        // A big square with a small one inside it.
        let mut lines = square_lines(0.0);
        let small: Vec<Element> = square_lines(0.0)
            .into_iter()
            .map(|mut e| {
                if let Shape::Path(path) = &mut e.shape {
                    let shrink = |p: Point| Point::new(40.0 + p.x * 0.2, 40.0 + p.y * 0.2);
                    path.start = shrink(path.start);
                    for s in &mut path.segments {
                        if let Segment::Line { to } = s {
                            *to = shrink(*to);
                        }
                    }
                }
                e
            })
            .collect();
        lines.extend(small);
        let mut layer = layer(lines);
        let (skin, white) = (Color::rgb(240, 180, 140), Color::WHITE);
        assert_eq!(layer.fill_enclosed(0, Point::new(10.0, 10.0), skin, 0.0), FillResult::Filled);
        assert_eq!(layer.fill_enclosed(0, Point::new(50.0, 50.0), white, 0.0), FillResult::Filled);
        let fills = &layer.drawings[0].colour_art;
        assert_eq!((fills[0].style.fill, fills[1].style.fill), (Some(skin), Some(white)));
        // Filling the small one again recolours just it.
        assert_eq!(layer.fill_enclosed(0, Point::new(50.0, 50.0), skin, 0.0), FillResult::Recoloured);
        assert_eq!(layer.drawings[0].colour_art.len(), 2);
    }
}
