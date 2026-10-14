//! The app icon, drawn in code.
//!
//! Keeping the icon as code rather than a PNG made in a paint program means
//! anyone can tweak it with a text editor, and every size is rendered
//! straight from the vector shapes instead of being scaled down from one
//! big bitmap (which turns thin lines to mush at 16×16).
//!
//! The picture is the classic first animation exercise: a bouncing ball,
//! with onion-skinned ghosts of the earlier frames and a squash where it
//! hits the floor.
//!
//! Everything below is laid out on a 1024×1024 canvas and scaled to the
//! requested size.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, RadialGradient, Rect,
    SpreadMode, Stroke, StrokeDash, Transform,
};

const DESIGN_SIZE: f32 = 1024.0;

/// The rounded square every macOS icon sits on. Apple's template puts an
/// 824-unit body in the middle of the 1024 canvas, leaving room for the
/// shadow, so the icon lines up with others in the Dock.
const BODY: (f32, f32, f32, f32) = (100.0, 100.0, 924.0, 924.0);
const CORNER_RADIUS: f32 = 185.0;

/// The body's gradient, lighter at the top like a lit surface.
const BODY_TOP: (u8, u8, u8) = (52, 58, 112);
const BODY_BOTTOM: (u8, u8, u8) = (20, 23, 52);

const BALL_RADIUS: f32 = 58.0;
/// How much the ball stretches sideways and flattens on impact.
const SQUASH: (f32, f32) = (1.38, 0.72);
const FLOOR_Y: f32 = 780.0;
const CORAL: (u8, u8, u8) = (255, 112, 80);

/// Renders the icon at `size`×`size` pixels.
pub fn render(size: u32) -> Pixmap {
    let mut pixmap = Pixmap::new(size, size).expect("icon size is non-zero");
    let scale = size as f32 / DESIGN_SIZE;
    let to_pixels = Transform::from_scale(scale, scale);

    draw_shadow(&mut pixmap, to_pixels);
    draw_body(&mut pixmap, to_pixels);
    draw_bounce(&mut pixmap, to_pixels);
    pixmap
}

/// A soft shadow under the body. tiny-skia has no blur, so the shadow is a
/// stack of slightly larger, nearly transparent copies of the body: where
/// they overlap the shadow is darkest, and it fades toward the outside.
fn draw_shadow(pixmap: &mut Pixmap, transform: Transform) {
    let (left, top, right, bottom) = BODY;
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    paint.set_color_rgba8(0, 0, 0, 6);
    for step in 0..8 {
        let grow = step as f32 * 2.0;
        let rect = Rect::from_ltrb(left - grow, top - grow + 10.0, right + grow, bottom + grow + 10.0)
            .expect("shadow rect is valid");
        let path = rounded_rect(rect, CORNER_RADIUS + grow);
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
    }
}

/// The ink-blue background.
fn draw_body(pixmap: &mut Pixmap, transform: Transform) {
    let (left, top, right, bottom) = BODY;
    let body = Rect::from_ltrb(left, top, right, bottom).expect("body rect is valid");
    let shader = LinearGradient::new(
        Point::from_xy(0.0, top),
        Point::from_xy(0.0, bottom),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(BODY_TOP.0, BODY_TOP.1, BODY_TOP.2, 255)),
            GradientStop::new(1.0, Color::from_rgba8(BODY_BOTTOM.0, BODY_BOTTOM.1, BODY_BOTTOM.2, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("gradient is valid");
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    pixmap.fill_path(&rounded_rect(body, CORNER_RADIUS), &paint, FillRule::Winding, transform, None);
}

fn draw_bounce(pixmap: &mut Pixmap, transform: Transform) {
    // The ball's centre follows two parabolas, falling from the top left and
    // rising to the top right. A quadratic Bézier whose control point sits
    // level with the start, halfway across, traces an exact parabola from its
    // apex: constant sideways speed, steadily growing downward speed, just
    // like gravity.
    let left_apex = (220.0, 330.0);
    let contact = (512.0, FLOOR_Y - BALL_RADIUS * SQUASH.1);
    let right_apex = (804.0, 330.0);
    let fall_control = ((left_apex.0 + contact.0) / 2.0, left_apex.1);
    let rise_control = ((contact.0 + right_apex.0) / 2.0, right_apex.1);
    let fall = |t| quad_point(left_apex, fall_control, contact, t);
    let rise = |t| quad_point(contact, rise_control, right_apex, t);

    // The motion path, dotted the way animation tools draw one.
    let mut trail = PathBuilder::new();
    trail.move_to(left_apex.0, left_apex.1);
    trail.quad_to(fall_control.0, fall_control.1, contact.0, contact.1);
    trail.quad_to(rise_control.0, rise_control.1, right_apex.0, right_apex.1);
    let trail = trail.finish().expect("trail has segments");
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    paint.set_color_rgba8(255, 255, 255, 60);
    let stroke = Stroke {
        width: 10.0,
        line_cap: tiny_skia::LineCap::Round,
        dash: StrokeDash::new(vec![1.0, 30.0], 0.0),
        ..Stroke::default()
    };
    pixmap.stroke_path(&trail, &paint, &stroke, transform, None);

    // The floor.
    let mut floor = PathBuilder::new();
    floor.move_to(392.0, FLOOR_Y + 8.0);
    floor.line_to(632.0, FLOOR_Y + 8.0);
    let floor = floor.finish().expect("floor has a segment");
    paint.set_color_rgba8(255, 255, 255, 80);
    let stroke = Stroke { width: 10.0, line_cap: tiny_skia::LineCap::Round, ..Stroke::default() };
    pixmap.stroke_path(&floor, &paint, &stroke, transform, None);

    // Onion skins: earlier frames, faded further into the background the
    // further back they are. The squash on impact is wider and flatter with
    // about the same area, so the ball reads as soft rather than shrinking.
    let frames = [
        (fall(0.0), (1.0, 1.0), 0.78),
        (fall(0.6), (1.0, 1.0), 0.64),
        (contact, SQUASH, 0.5),
        (rise(0.45), (1.0, 1.0), 0.32),
        // The current frame.
        (right_apex, (1.0, 1.0), 0.0),
    ];
    for (center, stretch, fade) in frames {
        draw_ball(pixmap, transform, center, stretch, fade);
    }
}

/// One ball, lit from the top left so it looks round even at small sizes.
///
/// Onion skins are faded by mixing their colours toward the background
/// rather than by making them transparent, so the dotted trail behind them
/// doesn't show through.
fn draw_ball(pixmap: &mut Pixmap, transform: Transform, center: (f32, f32), stretch: (f32, f32), fade: f32) {
    let (rx, ry) = (BALL_RADIUS * stretch.0, BALL_RADIUS * stretch.1);
    let oval =
        Rect::from_ltrb(center.0 - rx, center.1 - ry, center.0 + rx, center.1 + ry).expect("ball is valid");
    let path = PathBuilder::from_oval(oval).expect("ball is valid");

    let background = background_at(center.1);
    let faded = |color| {
        let (r, g, b) = mix(color, background, fade);
        Color::from_rgba8(r, g, b, 255)
    };
    let highlight = Point::from_xy(center.0 - rx * 0.35, center.1 - ry * 0.4);
    let shader = RadialGradient::new(
        highlight,
        0.0,
        highlight,
        rx.max(ry) * 1.4,
        vec![
            GradientStop::new(0.0, faded((255, 196, 168))),
            GradientStop::new(0.55, faded(CORAL)),
            GradientStop::new(1.0, faded((196, 64, 52))),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("gradient is valid");
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
}

/// The body's gradient colour at height `y`, to fade onion skins into.
fn background_at(y: f32) -> (u8, u8, u8) {
    let (_, top, _, bottom) = BODY;
    let t = ((y - top) / (bottom - top)).clamp(0.0, 1.0);
    mix(BODY_TOP, BODY_BOTTOM, t)
}

/// Blends from `a` (at `t` = 0) to `b` (at `t` = 1).
fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

fn quad_point(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1)
}

/// A rectangle with circular corners. Each corner is a cubic Bézier with
/// its handles at 0.5523 of the radius, the standard close fit to a
/// quarter circle.
fn rounded_rect(rect: Rect, radius: f32) -> tiny_skia::Path {
    const K: f32 = 0.5523;
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let rad = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let k = rad * K;

    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(r - rad, t);
    pb.cubic_to(r - rad + k, t, r, t + rad - k, r, t + rad);
    pb.line_to(r, b - rad);
    pb.cubic_to(r, b - rad + k, r - rad + k, b, r - rad, b);
    pb.line_to(l + rad, b);
    pb.cubic_to(l + rad - k, b, l, b - rad + k, l, b - rad);
    pb.line_to(l, t + rad);
    pb.cubic_to(l, t + rad - k, l + rad - k, t, l + rad, t);
    pb.close();
    pb.finish().expect("rounded rect has segments")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_opaque_and_the_corners_are_clear() {
        let icon = render(64);
        let middle = icon.pixel(32, 32).unwrap();
        assert_eq!(middle.alpha(), 255);
        // The very corner is outside the rounded body and the shadow.
        assert_eq!(icon.pixel(0, 0).unwrap().alpha(), 0);
    }
}
