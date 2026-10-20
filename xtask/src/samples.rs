//! `cargo xtask samples`: builds the sample projects in `examples/`.
//!
//! The samples are made here in code rather than saved by hand, so they
//! can be rebuilt in the current file format whenever it changes, and so
//! what's in each is easy to read. Each shows off one of Inkframe's bigger
//! features and is small enough to take apart.

use std::f32::consts::{PI, TAU};
use std::path::Path;

use inkframe_core::{
    Color, Document, Easing, Element, KAPPA, Path as Outline, Point, Preset, Segment, Shape, Stroke, Style,
    Transform,
};

/// Makes one sample project.
type Builder = fn() -> Document;

pub fn write_all(dir: &Path) -> Result<(), String> {
    let samples: [(&str, Builder); 3] =
        [("multiplane", multiplane), ("particles", particles), ("ink-and-paint", ink_and_paint)];
    for (name, build) in samples {
        let path = dir.join(format!("{name}.inkf"));
        build().save(&path).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn filled(shape: Shape, color: Color) -> Element {
    Element { shape, style: Style { fill: Some(color), stroke: None } }
}

fn block(width: f32, height: f32, corner_radius: f32, color: Color) -> Element {
    filled(Shape::Rect { width, height, corner_radius }, color)
}

fn oval(radius_x: f32, radius_y: f32, color: Color) -> Element {
    filled(Shape::Ellipse { radius_x, radius_y }, color)
}

/// A landscape at several depths, with the camera dollying in and drifting
/// right: near things grow and slide past faster than far ones.
fn multiplane() -> Document {
    let mut doc = Document::default();
    doc.duration = 96;
    doc.background = Color::rgb(150, 200, 235);
    // Far to near: name, drawing, where, depth.
    let scene = [
        ("Sun", oval(120.0, 120.0, Color::rgb(255, 230, 150)), Point::new(1300.0, 300.0), 4000.0),
        // Wide and deep enough that the camera never sees past their edges.
        (
            "Mountains",
            block(6000.0, 900.0, 260.0, Color::rgb(110, 130, 170)),
            Point::new(960.0, 960.0),
            1500.0,
        ),
        ("Hills", block(6000.0, 1000.0, 320.0, Color::rgb(90, 160, 110)), Point::new(960.0, 1200.0), 600.0),
        ("Tree", block(140.0, 380.0, 70.0, Color::rgb(40, 110, 60)), Point::new(620.0, 800.0), 150.0),
        ("Rock", oval(110.0, 70.0, Color::rgb(120, 115, 110)), Point::new(1250.0, 760.0), 0.0),
        ("Bush", oval(260.0, 200.0, Color::rgb(30, 80, 40)), Point::new(250.0, 1020.0), -350.0),
    ];
    for (name, drawing, at, depth) in scene {
        let id = doc.add_layer(name, vec![drawing], 0, Transform::at(at));
        if let Some(layer) = doc.layer_mut(id) {
            layer.track.set_depth(0, depth);
        }
    }
    let camera = doc.add_camera();
    let centre = doc.stage_rect().center();
    if let Some(camera) = doc.layer_mut(camera) {
        camera.track.set_depth(95, 450.0);
        camera.track.set_pose(95, Transform::at(Point::new(centre.x + 160.0, centre.y)));
        camera.track.set_easing(0, Easing::EaseInOut);
    }
    doc
}

/// A campfire at night: fire, smoke and sparks, with snow falling.
fn particles() -> Document {
    let mut doc = Document::default();
    doc.duration = 120;
    doc.background = Color::rgb(22, 26, 44);
    let stage = doc.stage_rect();
    doc.add_layer(
        "Moon",
        vec![oval(70.0, 70.0, Color::rgb(235, 235, 220))],
        0,
        Transform::at(Point::new(1550.0, 220.0)),
    );
    doc.add_layer(
        "Ground",
        vec![block(stage.width() + 200.0, 260.0, 0.0, Color::rgb(46, 52, 70))],
        0,
        Transform::at(Point::new(stage.center().x, stage.max.y - 60.0)),
    );
    let fire_at = Point::new(stage.center().x, stage.max.y - 190.0);
    for (name, turn) in [("Log", 12.0), ("Log", -12.0)] {
        let log = doc.add_layer(
            name,
            vec![block(260.0, 44.0, 22.0, Color::rgb(92, 60, 40))],
            0,
            Transform::at(fire_at),
        );
        if let Some(log) = doc.layer_mut(log) {
            log.track
                .set_pose(0, Transform { rotation: turn, ..Transform::at(fire_at + Point::new(0.0, 20.0)) });
        }
    }
    doc.add_particle_layer(Preset::Smoke, 0, fire_at + Point::new(0.0, -120.0));
    doc.add_particle_layer(Preset::Fire, 0, fire_at);
    doc.add_particle_layer(Preset::Sparks, 0, fire_at);
    doc.add_particle_layer(Preset::Snow, 0, Point::new(stage.center().x, stage.min.y - 40.0));
    doc
}

/// A flower inked as line art and coloured with the paint bucket, as in
/// Harmony: the colours sit in the colour art under the lines, and each is
/// a palette swatch, so changing a swatch recolours the drawing.
fn ink_and_paint() -> Document {
    let mut doc = Document::default();
    doc.background = Color::rgb(245, 240, 228);
    let ink = Style { fill: None, stroke: Some(Stroke { color: Color::rgb(40, 34, 40), width: 5.0 }) };
    let line = |outline: Outline| Element { shape: Shape::Path(outline), style: ink };

    // Drawn around the flower's middle; the stem reaches down to (0, 420).
    let centre = Point::ZERO;
    let mut lines: Vec<Element> =
        (0..6).map(|i| line(petal(centre, i as f32 * TAU / 6.0, 40.0, 170.0, 62.0))).collect();
    lines.push(line(circle(centre, 55.0)));
    lines.push(line(Outline {
        start: Point::new(0.0, 55.0),
        segments: vec![Segment::Cubic {
            ctrl1: Point::new(-20.0, 200.0),
            ctrl2: Point::new(30.0, 320.0),
            to: Point::new(0.0, 420.0),
        }],
        closed: false,
    }));
    let leaf_base = Point::new(-2.0, 290.0);
    lines.push(line(petal(leaf_base, PI * 0.85, 0.0, 150.0, 45.0)));

    let id = doc.add_layer("Flower", lines, 0, Transform::at(Point::new(960.0, 920.0)));
    let (yellow, pink, green) =
        (Color::rgb(250, 205, 70), Color::rgb(240, 120, 150), Color::rgb(90, 170, 90));
    for (name, color) in [("Middle", yellow), ("Petals", pink), ("Leaf", green)] {
        doc.add_swatch(name, color);
    }
    if let Some(flower) = doc.layer_mut(id) {
        // Swaying from the foot of the stem.
        flower.pivot = Point::new(0.0, 420.0);
        flower.track.set_pose(0, Transform::at(Point::new(960.0, 1000.0)));
        flower.track.set_pose(24, Transform { rotation: 6.0, ..Transform::at(Point::new(960.0, 1000.0)) });
        flower.track.set_pose(47, Transform::at(Point::new(960.0, 1000.0)));
        flower.track.set_easing(0, Easing::EaseInOut);
        flower.track.set_easing(24, Easing::EaseInOut);

        flower.fill_enclosed(0, centre, yellow, 2.0);
        for i in 0..6 {
            let angle = i as f32 * TAU / 6.0;
            flower.fill_enclosed(0, centre + Point::new(angle.cos(), angle.sin()) * 110.0, pink, 2.0);
        }
        let leaf_angle: f32 = PI * 0.85;
        flower.fill_enclosed(
            0,
            leaf_base + Point::new(leaf_angle.cos(), leaf_angle.sin()) * 75.0,
            green,
            2.0,
        );
    }
    doc
}

/// A petal (or leaf) pointing `angle` radians out from `base`, starting
/// `inset` out and reaching `length`, `width` across at its widest.
fn petal(base: Point, angle: f32, inset: f32, length: f32, width: f32) -> Outline {
    let out = Point::new(angle.cos(), angle.sin());
    let side = Point::new(-out.y, out.x);
    let root = base + out * inset;
    let tip = base + out * length;
    let swell = (length - inset) * 0.45;
    Outline {
        start: root,
        segments: vec![
            Segment::Cubic {
                ctrl1: root + out * swell + side * width,
                ctrl2: tip - out * swell * 0.4 + side * width * 0.5,
                to: tip,
            },
            Segment::Cubic {
                ctrl1: tip - out * swell * 0.4 - side * width * 0.5,
                ctrl2: root + out * swell - side * width,
                to: root,
            },
        ],
        closed: true,
    }
}

/// A circle as four Bézier quarters.
fn circle(centre: Point, r: f32) -> Outline {
    let k = r * KAPPA;
    let at = |x: f32, y: f32| centre + Point::new(x, y);
    Outline {
        start: at(r, 0.0),
        segments: vec![
            Segment::Cubic { ctrl1: at(r, k), ctrl2: at(k, r), to: at(0.0, r) },
            Segment::Cubic { ctrl1: at(-k, r), ctrl2: at(-r, k), to: at(-r, 0.0) },
            Segment::Cubic { ctrl1: at(-r, -k), ctrl2: at(-k, -r), to: at(0.0, -r) },
            Segment::Cubic { ctrl1: at(k, -r), ctrl2: at(r, -k), to: at(r, 0.0) },
        ],
        closed: true,
    }
}
