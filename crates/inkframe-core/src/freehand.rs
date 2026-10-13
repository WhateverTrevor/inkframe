//! Turns raw pointer samples into a smooth vector path.
//!
//! Mouse and tablet input arrives as a dense, jittery polyline — often a
//! point every pixel or two. Storing that directly would make files huge and
//! strokes look shaky. Instead we:
//!
//! 1. **Simplify** with Ramer–Douglas–Peucker, keeping only the points that
//!    matter to the overall shape.
//! 2. **Smooth** by threading a Catmull-Rom spline through the survivors and
//!    converting each span into an equivalent cubic Bézier, which every
//!    renderer understands.

use crate::geometry::Point;
use crate::shape::{Path, Segment};

/// Fits a smooth path through freehand input.
///
/// `tolerance` is the largest distance (in the same units as the points) a
/// simplified stroke may drift from the original. Around 1.5 stage pixels
/// gives clean results without visibly changing the drawing.
///
/// Returns `None` if there aren't enough distinct points to form a stroke.
pub fn fit_freehand_path(points: &[Point], tolerance: f32) -> Option<Path> {
    let mut points: Vec<Point> = points.to_vec();
    points.dedup();
    if points.len() < 2 {
        return None;
    }

    let kept = simplify(&points, tolerance);
    let segments = catmull_rom_to_bezier(&kept);
    Some(Path { start: kept[0], segments, closed: false })
}

/// Ramer–Douglas–Peucker line simplification.
///
/// Draw a straight line from the first point to the last. Find the input
/// point farthest from that line. If it's within `tolerance`, the line is a
/// good enough stand-in for everything between, so drop those points.
/// Otherwise keep the farthest point and repeat on each half.
fn simplify(points: &[Point], tolerance: f32) -> Vec<Point> {
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;

    // An explicit stack instead of recursion, so very long strokes can't
    // overflow the call stack.
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        let (farthest, distance) = (first + 1..last)
            .map(|i| (i, points[i].distance_to_segment(points[first], points[last])))
            .fold((first, 0.0f32), |best, cur| if cur.1 > best.1 { cur } else { best });

        if distance > tolerance {
            keep[farthest] = true;
            stack.push((first, farthest));
            stack.push((farthest, last));
        }
    }

    points.iter().zip(keep).filter_map(|(&p, k)| k.then_some(p)).collect()
}

/// Converts a Catmull-Rom spline through `points` into cubic Bézier segments.
///
/// A Catmull-Rom spline passes through every point, with the tangent at each
/// point parallel to the line joining its two neighbours. For the span from
/// `p1` to `p2` (with neighbours `p0` and `p3`), the matching Bézier control
/// points are:
///
/// ```text
/// ctrl1 = p1 + (p2 - p0) / 6
/// ctrl2 = p2 - (p3 - p1) / 6
/// ```
///
/// At the ends of the stroke there's no outer neighbour, so we reuse the
/// endpoint itself, which makes the curve leave and arrive head-on.
fn catmull_rom_to_bezier(points: &[Point]) -> Vec<Segment> {
    if points.len() == 2 {
        return vec![Segment::Line { to: points[1] }];
    }

    let at = |i: isize| points[i.clamp(0, points.len() as isize - 1) as usize];
    (0..points.len() as isize - 1)
        .map(|i| {
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            Segment::Cubic {
                ctrl1: p1 + (p2 - p0) * (1.0 / 6.0),
                ctrl2: p2 - (p3 - p1) * (1.0 / 6.0),
                to: p2,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line_collapses_to_two_points() {
        let points: Vec<Point> = (0..100).map(|i| Point::new(i as f32, 0.0)).collect();
        let path = fit_freehand_path(&points, 1.0).unwrap();
        assert_eq!(path.segments, vec![Segment::Line { to: Point::new(99.0, 0.0) }]);
    }

    #[test]
    fn corner_survives_simplification() {
        let mut points: Vec<Point> = (0..=50).map(|i| Point::new(i as f32, 0.0)).collect();
        points.extend((1..=50).map(|i| Point::new(50.0, i as f32)));
        let kept = simplify(&points, 1.0);
        assert_eq!(kept, vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0), Point::new(50.0, 50.0)]);
    }

    #[test]
    fn curve_passes_through_kept_points() {
        let kept = [Point::new(0.0, 0.0), Point::new(10.0, 10.0), Point::new(20.0, 0.0)];
        let segments = catmull_rom_to_bezier(&kept);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].end(), kept[1]);
        assert_eq!(segments[1].end(), kept[2]);
    }

    #[test]
    fn single_point_is_not_a_stroke() {
        assert!(fit_freehand_path(&[Point::ZERO, Point::ZERO], 1.0).is_none());
    }
}
