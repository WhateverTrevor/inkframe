//! Builds a small sample animation in code and saves it to disk.
//!
//! Run with: `cargo run -p inkframe-core --example bouncing_ball -- examples/bouncing-ball.inkf`
//!
//! It's also a compact tour of the document API: layers, shapes, keyframes
//! and easing.

use inkframe_core::{Color, Document, Easing, Element, Point, Shape, Stroke, Style, Transform};

const FLOOR_Y: f32 = 880.0;
const TOP_Y: f32 = 260.0;
const BALL_RADIUS: f32 = 70.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "bouncing-ball.inkf".to_owned());

    let mut doc = Document::default();
    doc.background = Color::rgb(0xf4, 0xf1, 0xea);

    let floor = Element {
        shape: Shape::Rect { width: 1920.0, height: 200.0, corner_radius: 0.0 },
        style: Style { fill: Some(Color::rgb(0x3b, 0x3a, 0x48)), stroke: None },
    };
    doc.add_layer("Floor", vec![floor], 0, Transform::at(Point::new(960.0, FLOOR_Y + BALL_RADIUS + 100.0)));

    let ball = Element {
        shape: Shape::Ellipse { radius_x: BALL_RADIUS, radius_y: BALL_RADIUS },
        style: Style {
            fill: Some(Color::rgb(0xe8, 0x5d, 0x3a)),
            stroke: Some(Stroke { color: Color::rgb(0x2a, 0x1a, 0x14), width: 4.0 }),
        },
    };
    let id = doc.add_layer("Ball", vec![ball], 0, Transform::at(Point::new(960.0, TOP_Y)));
    let track = &mut doc.layer_mut(id).expect("just added").track;

    // Classic bounce timing: accelerate into the floor (ease in), squash on
    // contact, decelerate on the way up (ease out).
    let up = Transform::at(Point::new(960.0, TOP_Y));
    let squash = Transform {
        // Lower the center so the squashed ball still touches the floor.
        position: Point::new(960.0, FLOOR_Y + BALL_RADIUS * 0.2),
        scale: Point::new(1.3, 0.75),
        ..Transform::default()
    };
    for (frame, pose, easing) in [
        (0, up, Easing::EaseIn),
        (12, squash, Easing::EaseOut),
        (24, up, Easing::EaseIn),
        (36, squash, Easing::EaseOut),
        (47, up, Easing::Linear),
    ] {
        track.set(frame, pose);
        track.key_at_mut(frame).expect("just set").easing = easing;
    }

    doc.save(&path)?;
    println!("Wrote {path}");
    Ok(())
}
