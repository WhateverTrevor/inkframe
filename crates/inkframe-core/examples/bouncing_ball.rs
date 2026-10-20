//! Builds a small sample animation in code and saves it to disk.
//!
//! Run with: `cargo run -p inkframe-core --example bouncing_ball -- examples/bouncing-ball.inkf`
//!
//! It's also a compact tour of the document API: layers, shapes, and
//! per-property keyframes with easing.

use inkframe_core::{AnyChannel, Color, Document, Easing, Element, Point, Shape, Stroke, Style, Transform};

const FLOOR_Y: f32 = 880.0;
const TOP_Y: f32 = 260.0;
const BALL_RADIUS: f32 = 70.0;
const CENTER_X: f32 = 960.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "bouncing-ball.inkf".to_owned());

    let mut doc = Document::default();
    doc.background = Color::rgb(0xf4, 0xf1, 0xea);

    let floor = Element {
        shape: Shape::Rect { width: 1920.0, height: 200.0, corner_radius: 0.0 },
        style: Style { fill: Some(Color::rgb(0x3b, 0x3a, 0x48)), stroke: None, gradient: None },
    };
    doc.add_layer(
        "Floor",
        vec![floor],
        0,
        Transform::at(Point::new(CENTER_X, FLOOR_Y + BALL_RADIUS + 100.0)),
    );

    let ball = Element {
        shape: Shape::Ellipse { radius_x: BALL_RADIUS, radius_y: BALL_RADIUS },
        style: Style {
            fill: Some(Color::rgb(0xe8, 0x5d, 0x3a)),
            stroke: Some(Stroke::new(Color::rgb(0x2a, 0x1a, 0x14), 4.0)),
            gradient: None,
        },
    };
    let id = doc.add_layer("Ball", vec![ball], 0, Transform::at(Point::new(CENTER_X, TOP_Y)));
    let track = &mut doc.layer_mut(id).expect("just added").track;

    // Height follows classic bounce timing: accelerate into the floor (ease
    // in), then decelerate on the way back up (ease out). When squashed to
    // 75% height the center drops so the ball still touches the floor.
    let up = Point::new(CENTER_X, TOP_Y);
    let down = Point::new(CENTER_X, FLOOR_Y + BALL_RADIUS * 0.25);
    let bounce = [
        (0, up, Easing::EaseIn),
        (12, down, Easing::EaseOut),
        (24, up, Easing::EaseIn),
        (36, down, Easing::EaseOut),
        (47, up, Easing::Linear),
    ];
    for (frame, position, _) in bounce {
        track.position.set(frame, position);
    }
    // Easing belongs to a tween, which only exists once the key after it
    // does, so set it in a second pass. (The last key has no tween after it.)
    for (frame, _, easing) in bounce {
        if let Some(e) = track.position.tween_easing_mut(frame) {
            *e = easing;
        }
    }

    // Squash and stretch lives on its own channel and only happens around
    // each impact: stretched while falling fast, squashed on contact, then
    // back to round. With whole-transform keyframes this would have pinned
    // the height on those frames too.
    let round = Point::new(1.0, 1.0);
    let stretch = Point::new(0.88, 1.15);
    let squash = Point::new(1.3, 0.75);
    for impact in [12, 36] {
        track.scale.set(impact - 2, stretch);
        track.scale.set(impact, squash);
        track.scale.set(impact + 3, round);
    }
    track.scale.set(0, round);

    doc.save(&path)?;
    println!("Wrote {path}");
    Ok(())
}
