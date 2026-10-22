//! Welded points, as Moho's shared points: where two shapes in a drawing
//! meet at a point (an arm's outline and its sleeve's, a leg drawn up to
//! the body), that point is one point, so editing or bending it keeps the
//! shapes joined.
//!
//! A weld isn't stored: points of different shapes exactly on top of each
//! other are welded, and everything that moves one moves the others the
//! same way, so they stay on top of each other. Dragging a point onto
//! another shape's point welds them; dragging one away with ⌘ held parts
//! them. Pencil lines take no part: their points are their outline's,
//! not the line's.

use crate::anchor::Anchor;
use crate::document::Element;
use crate::geometry::Point;
use crate::shape::Shape;

/// How near two points must be to count as one, in the drawing's units.
pub const WELD_DISTANCE: f32 = 1e-3;

/// A shape's points, if it can be welded: anything but a pencil line,
/// picture, painting or text.
fn weldable(element: &Element) -> Option<Vec<Anchor>> {
    if !element.is_reshapable() || matches!(element.shape, Shape::Line { .. }) {
        return None;
    }
    Some(element.shape.to_anchors().0)
}

/// The points of other shapes in `elements` welded to point `anchor` of
/// shape `element`, as (shape, point) places.
pub fn welded_to(elements: &[Element], element: usize, anchor: usize) -> Vec<(usize, usize)> {
    let Some(at) = elements.get(element).and_then(weldable).and_then(|a| a.get(anchor).map(|a| a.point))
    else {
        return Vec::new();
    };
    welded_at(elements, at, Some(element))
}

/// The points of the shapes in `elements` (other than shape `except`) at
/// `at`, as (shape, point) places.
pub fn welded_at(elements: &[Element], at: Point, except: Option<usize>) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (i, other) in elements.iter().enumerate() {
        if Some(i) == except {
            continue;
        }
        let Some(anchors) = weldable(other) else { continue };
        found.extend(
            anchors
                .iter()
                .enumerate()
                .filter(|(_, a)| a.point.distance(at) <= WELD_DISTANCE)
                .map(|(a, _)| (i, a)),
        );
    }
    found
}

/// For each shape in `elements`, which of its points are welded to another
/// shape's (empty for shapes that can't be welded).
pub fn welded_points(elements: &[Element]) -> Vec<Vec<bool>> {
    elements
        .iter()
        .enumerate()
        .map(|(i, element)| match weldable(element) {
            Some(anchors) => (0..anchors.len()).map(|a| !welded_to(elements, i, a).is_empty()).collect(),
            None => Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Style};

    fn rect(x: f32, width: f32) -> Element {
        let style = Style { fill: Some(Color::BLACK), stroke: None, gradient: None };
        Element { shape: Shape::Rect { width, height: 100.0, corner_radius: 0.0 }, style }
            .transformed(crate::Affine::translate(Point::new(x, 0.0)))
            .unwrap()
    }

    #[test]
    fn shapes_sharing_an_edge_are_welded_at_its_ends() {
        // Two boxes side by side, meeting along x = 50.
        let elements = [rect(0.0, 100.0), rect(100.0, 100.0)];
        // The left box's top right corner (its point 1) is the right box's
        // top left (its point 0).
        assert_eq!(welded_to(&elements, 0, 1), [(1, 0)]);
        assert!(welded_to(&elements, 0, 0).is_empty());
        let welded = welded_points(&elements);
        assert_eq!(welded[0], [false, true, true, false]);
        assert_eq!(welded[1], [true, false, false, true]);
    }

    #[test]
    fn pencil_lines_dont_weld() {
        let line = Element {
            shape: Shape::Line {
                path: crate::Path {
                    start: Point::new(50.0, -50.0),
                    segments: vec![crate::Segment::Line { to: Point::new(200.0, -50.0) }],
                    closed: false,
                },
                widths: vec![4.0, 4.0],
            },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        assert!(welded_at(&[line], Point::new(50.0, -50.0), None).is_empty());
    }
}
