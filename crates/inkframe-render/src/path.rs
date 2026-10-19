use inkframe_core::{KAPPA, Segment, Shape};
use tiny_skia::{Path, PathBuilder, Rect};

/// Converts an Inkframe shape into a tiny-skia path in local coordinates.
///
/// Returns `None` for shapes with no area to draw, which tiny-skia refuses
/// to build (for example a rectangle that is 0 pixels wide).
pub fn build_path(shape: &Shape) -> Option<Path> {
    match *shape {
        Shape::Rect { width, height, corner_radius } => {
            let rect = Rect::from_xywh(-width / 2.0, -height / 2.0, width, height)?;
            // A radius larger than half the shorter side would make the
            // corners overlap, so clamp it. That turns a square into a circle
            // rather than into a mess.
            let radius = corner_radius.clamp(0.0, width.min(height) / 2.0);
            if radius <= 0.0 { Some(PathBuilder::from_rect(rect)) } else { rounded_rect(rect, radius) }
        }
        Shape::Ellipse { radius_x, radius_y } => {
            PathBuilder::from_oval(Rect::from_xywh(-radius_x, -radius_y, radius_x * 2.0, radius_y * 2.0)?)
        }
        // Images aren't outlines, and text outlines come from its font; the
        // renderer handles both separately.
        Shape::Image { .. } | Shape::Text(_) | Shape::Paint { .. } => None,
        Shape::Path(ref path) => from_path(path),
        // A pressure line is the outline it sweeps out.
        Shape::Line { ref path, ref widths } => from_path(&inkframe_core::line_outline(path, widths)),
    }
}

fn from_path(path: &inkframe_core::Path) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(path.start.x, path.start.y);
    for segment in &path.segments {
        match *segment {
            Segment::Line { to } => pb.line_to(to.x, to.y),
            Segment::Cubic { ctrl1, ctrl2, to } => {
                pb.cubic_to(ctrl1.x, ctrl1.y, ctrl2.x, ctrl2.y, to.x, to.y)
            }
        }
    }
    if path.closed {
        pb.close();
    }
    pb.finish()
}

/// Builds a rectangle with circular corners, clockwise from the top-left.
fn rounded_rect(rect: Rect, r: f32) -> Option<Path> {
    let (l, t, rt, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let k = r * KAPPA;

    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rt - r, t);
    pb.cubic_to(rt - r + k, t, rt, t + r - k, rt, t + r);
    pb.line_to(rt, b - r);
    pb.cubic_to(rt, b - r + k, rt - r + k, b, rt - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + r - k, b, l, b - r + k, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + r - k, l + r - k, t, l + r, t);
    pb.close();
    pb.finish()
}
