//! Draws shape tweens as filmstrips, the Linear blend above the Natural
//! one, to compare how each handles the cases tweens tend to get wrong.
//!
//! ```sh
//! cargo run --release -p inkframe-render --example morph_filmstrip
//! ```
//!
//! writes `target/morph-filmstrip.png`. Each row runs from the first
//! drawing (left) to the second (right).

use std::f32::consts::PI;

use inkframe_core::{
    Anchor, Color, Document, Element, Point, Shape, ShapeBlend, Stroke, Style, Text, Transform, morph_with,
    path_from_anchors,
};
use inkframe_render::{RenderOptions, render_frame};

const CELL: f32 = 150.0;
const FRAMES: usize = 7;
const LABEL: f32 = 110.0;

fn main() {
    let cases: Vec<(&str, Vec<Element>, Vec<Element>)> = vec![
        ("Square to circle", vec![polygon(&square(50.0), RED)], vec![circle(Point::ZERO, 55.0, BLUE)]),
        ("Raising an arm", vec![polygon(&figure(0.0), BLUE)], vec![polygon(&figure(-1.1), BLUE)]),
        ("Swinging a stick", vec![polygon(&stick(0.3), ORANGE)], vec![polygon(&stick(-1.4), ORANGE)]),
        (
            "Drawn in another order",
            vec![
                circle(Point::new(0.0, -42.0), 18.0, ORANGE),
                polygon(&offset(&box_points(30.0, 32.0), Point::new(0.0, 20.0)), BLUE),
            ],
            vec![
                polygon(&offset(&box_points(32.0, 30.0), Point::new(8.0, 24.0)), BLUE),
                circle(Point::new(10.0, -38.0), 22.0, ORANGE),
            ],
        ),
        ("Star to pentagon", vec![polygon(&star(0.0), RED)], vec![polygon(&pentagon(PI), GREEN)]),
        ("C to S (a line)", vec![line(&c_curve())], vec![line(&s_curve())]),
    ];

    // How long a frame of each takes: a tween works its shapes out again
    // on every frame it plays.
    for (name, from, to) in &cases {
        for blend in [ShapeBlend::Linear, ShapeBlend::Natural] {
            let start = std::time::Instant::now();
            for k in 0..200 {
                std::hint::black_box(morph_with(from, to, (&[], &[]), k as f32 / 200.0, blend));
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / 200.0;
            println!("{name:>24} {blend:?}: {ms:.3} ms a frame");
        }
    }

    let rows = cases.len() * 2;
    let mut doc = Document::default();
    doc.width = (LABEL + CELL * FRAMES as f32) as u32;
    doc.height = (CELL * rows as f32) as u32;
    doc.background = Color::WHITE;
    for (case, (name, from, to)) in cases.iter().enumerate() {
        for (row_in_case, blend) in [ShapeBlend::Linear, ShapeBlend::Natural].into_iter().enumerate() {
            let row = case * 2 + row_in_case;
            let y = CELL * row as f32 + CELL / 2.0;
            let label = if row_in_case == 0 { format!("{name}\nLinear") } else { "Natural".to_owned() };
            doc.add_layer(
                "label",
                vec![Element { shape: Shape::Text(Text::new(label, 13.0)), style: fill(Color::BLACK) }],
                0,
                Transform::at(Point::new(8.0, y - 10.0)),
            );
            for column in 0..FRAMES {
                let t = column as f32 / (FRAMES - 1) as f32;
                let shapes = morph_with(from, to, (&[], &[]), t, blend);
                let x = LABEL + CELL * column as f32 + CELL / 2.0;
                doc.add_layer("frame", shapes, 0, Transform::at(Point::new(x, y)));
            }
        }
    }
    let options = RenderOptions { scale: 1.0, ..RenderOptions::default() };
    let pixmap = render_frame(&doc, 0, &options).expect("a picture");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/morph-filmstrip.png");
    pixmap.save_png(&path).expect("saved");
    println!("wrote {}", path.display());
}

const RED: Color = Color::rgb(220, 60, 60);
const BLUE: Color = Color::rgb(60, 110, 220);
const GREEN: Color = Color::rgb(40, 160, 90);
const ORANGE: Color = Color::rgb(240, 150, 40);

fn fill(color: Color) -> Style {
    Style { fill: Some(color), stroke: None, gradient: None }
}

fn polygon(points: &[Point], color: Color) -> Element {
    let anchors: Vec<Anchor> = points.iter().copied().map(Anchor::corner).collect();
    Element { shape: Shape::Path(path_from_anchors(&anchors, true).unwrap()), style: fill(color) }
}

fn line(anchors: &[Anchor]) -> Element {
    Element {
        shape: Shape::Path(path_from_anchors(anchors, false).unwrap()),
        style: Style { fill: None, stroke: Some(Stroke::new(Color::BLACK, 6.0)), gradient: None },
    }
}

fn circle(centre: Point, radius: f32, color: Color) -> Element {
    let (anchors, _) = Shape::Ellipse { radius_x: radius, radius_y: radius }.to_anchors();
    let moved: Vec<Anchor> = anchors
        .iter()
        .map(|a| Anchor {
            point: a.point + centre,
            handle_in: a.handle_in + centre,
            handle_out: a.handle_out + centre,
        })
        .collect();
    Element { shape: Shape::Path(path_from_anchors(&moved, true).unwrap()), style: fill(color) }
}

fn rotate(p: Point, radians: f32) -> Point {
    let (s, c) = radians.sin_cos();
    Point::new(p.x * c - p.y * s, p.x * s + p.y * c)
}

fn offset(points: &[Point], by: Point) -> Vec<Point> {
    points.iter().map(|&p| p + by).collect()
}

fn box_points(half_w: f32, half_h: f32) -> Vec<Point> {
    vec![
        Point::new(-half_w, -half_h),
        Point::new(half_w, -half_h),
        Point::new(half_w, half_h),
        Point::new(-half_w, half_h),
    ]
}

fn square(half: f32) -> Vec<Point> {
    box_points(half, half)
}

/// A torso with an arm out from its right shoulder at `angle` (radians,
/// 0 pointing right, negative raising it).
fn figure(angle: f32) -> Vec<Point> {
    let shoulder = Point::new(14.0, -30.0);
    let arm = |x: f32, y: f32| shoulder + rotate(Point::new(x, y), angle);
    vec![
        Point::new(-14.0, -45.0),
        Point::new(14.0, -45.0),
        arm(0.0, -5.0),
        arm(52.0, -5.0),
        arm(52.0, 5.0),
        arm(0.0, 5.0),
        Point::new(14.0, 50.0),
        Point::new(-14.0, 50.0),
    ]
}

/// A thin stick hanging from a pin at the top, turned `angle` from straight
/// down.
fn stick(angle: f32) -> Vec<Point> {
    let pin = Point::new(0.0, -50.0);
    [Point::new(-5.0, 0.0), Point::new(5.0, 0.0), Point::new(5.0, 100.0), Point::new(-5.0, 100.0)]
        .map(|p| pin + rotate(p, angle))
        .to_vec()
}

fn star(turn: f32) -> Vec<Point> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { 60.0 } else { 25.0 };
            rotate(Point::new(0.0, -r), turn + i as f32 * PI / 5.0)
        })
        .collect()
}

fn pentagon(turn: f32) -> Vec<Point> {
    (0..5).map(|i| rotate(Point::new(0.0, -50.0), turn + i as f32 * 2.0 * PI / 5.0)).collect()
}

fn c_curve() -> Vec<Anchor> {
    vec![
        Anchor {
            point: Point::new(35.0, -45.0),
            handle_in: Point::new(35.0, -45.0),
            handle_out: Point::new(-10.0, -70.0),
        },
        Anchor {
            point: Point::new(-40.0, 0.0),
            handle_in: Point::new(-40.0, -40.0),
            handle_out: Point::new(-40.0, 40.0),
        },
        Anchor {
            point: Point::new(35.0, 45.0),
            handle_in: Point::new(-10.0, 70.0),
            handle_out: Point::new(35.0, 45.0),
        },
    ]
}

fn s_curve() -> Vec<Anchor> {
    vec![
        Anchor {
            point: Point::new(35.0, -45.0),
            handle_in: Point::new(35.0, -45.0),
            handle_out: Point::new(-40.0, -60.0),
        },
        Anchor {
            point: Point::new(0.0, 0.0),
            handle_in: Point::new(-40.0, -10.0),
            handle_out: Point::new(40.0, 10.0),
        },
        Anchor {
            point: Point::new(-35.0, 45.0),
            handle_in: Point::new(40.0, 60.0),
            handle_out: Point::new(-35.0, 45.0),
        },
    ]
}
