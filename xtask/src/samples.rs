//! `cargo xtask samples`: builds the sample projects in `examples/`.
//!
//! The samples are made here in code rather than saved by hand, so they
//! can be rebuilt in the current file format whenever it changes, and so
//! what's in each is easy to read. Each shows off one of Inkframe's bigger
//! features and is small enough to take apart.

use std::f32::consts::{PI, TAU};
use std::path::Path;

use inkframe_core::{
    BrushMode, BrushPreset, BrushSettings, BrushStroke, CanvasId, Color, Document, Easing, Element, KAPPA,
    Path as Outline, Point, Preset, Segment, Shape, Stroke, Style, TimelineId, Transform,
};

/// Makes one sample project.
type Builder = fn() -> Document;

pub fn write_all(dir: &Path) -> Result<(), String> {
    let samples: [(&str, Builder); 4] = [
        ("multiplane", multiplane),
        ("particles", particles),
        ("ink-and-paint", ink_and_paint),
        ("painted-landscape", painted_landscape),
    ];
    for (name, build) in samples {
        let path = dir.join(format!("{name}.inkf"));
        build().save(&path).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn filled(shape: Shape, color: Color) -> Element {
    Element { shape, style: Style { fill: Some(color), stroke: None, gradient: None } }
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
    let ink = Style { fill: None, stroke: Some(Stroke::new(Color::rgb(40, 34, 40), 5.0)), gradient: None };
    let line = |outline: Outline| Element { shape: Shape::Path(outline), style: ink.clone() };

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

/// A sunset over mountains, painted with the ready-made brushes on paint
/// layers: a wash of sky, clouds that drift, two mountain ranges, hills,
/// trees and grass. Each part is its own layer, so it can be hidden to see
/// what's under it, or painted over.
fn painted_landscape() -> Document {
    let mut doc = Document::default();
    doc.duration = 96;
    doc.background = Color::rgb(250, 190, 150);
    let mut brushes = Brushes::new();
    let (width, height) = (doc.width as f32, doc.height as f32);
    let along = |y: &dyn Fn(f32) -> f32| -> Vec<Point> {
        (0..=120).map(|i| i as f32 * (width + 200.0) / 120.0 - 100.0).map(|x| Point::new(x, y(x))).collect()
    };

    // The sky, darkening upwards from a warm horizon, in overlapping bands
    // of a big soft brush so they blend; then the sun and its glow.
    let sky = paint_layer(&mut doc, "Sky");
    let (horizon, zenith) = (Color::rgb(255, 196, 150), Color::rgb(52, 70, 128));
    for row in 0..=24 {
        let t = row as f32 / 24.0;
        let color = mix(zenith, horizon, t);
        brushes.paint(&mut doc, sky, "Soft Round", 260.0, color, &along(&|_| t * 760.0 - 40.0));
    }
    // The airbrush builds up as it goes round, as one held in place would.
    let sun = Point::new(1240.0, 480.0);
    let ring = |r: f32| -> Vec<Point> {
        (0..=24).map(|i| i as f32 * TAU / 24.0).map(|a| sun + Point::new(a.cos(), a.sin()) * r).collect()
    };
    for (r, size) in [(160.0, 360.0), (60.0, 260.0)] {
        brushes.paint(&mut doc, sky, "Airbrush", size, Color::rgb(255, 226, 170), &ring(r));
    }
    brushes.paint(
        &mut doc,
        sky,
        "Hard Round",
        120.0,
        Color::rgb(255, 244, 214),
        &[sun, sun + Point::new(1.0, 0.0)],
    );

    // Clouds across the sky, lit pink from below, drifting right.
    let clouds = paint_layer(&mut doc, "Clouds");
    for (y, from, to, size) in
        [(170.0, 160.0, 760.0, 150.0), (300.0, 980.0, 1700.0, 130.0), (430.0, 300.0, 900.0, 100.0)]
    {
        let wave = |x: f32| y + (x * 0.012).sin() * 24.0;
        let points: Vec<Point> =
            (0..=24).map(|i| from + (to - from) * i as f32 / 24.0).map(|x| Point::new(x, wave(x))).collect();
        brushes.paint(&mut doc, clouds, "Clouds", size, Color::rgb(232, 168, 176), &points);
        let lit: Vec<Point> = points.iter().map(|p| *p + Point::new(0.0, size * 0.3)).collect();
        brushes.paint(&mut doc, clouds, "Clouds", size * 0.6, Color::rgb(255, 214, 190), &lit);
    }
    if let Some(layer) = doc.layer_mut(clouds.0) {
        let centre = Point::new(width / 2.0, height / 2.0);
        layer.track.set_pose(95, Transform::at(centre + Point::new(90.0, 0.0)));
    }

    // Two ranges, the far one paler, each painted in rows that follow its
    // ridge and lit from the top down by the low sun. A peak is where
    // `1 - |sin|` comes to a point.
    let mountains = paint_layer(&mut doc, "Mountains");
    let peaks = |x: f32, every: f32, phase: f32| 1.0 - (x * every + phase).sin().abs();
    let far = |x: f32| 560.0 - 150.0 * peaks(x, 0.0045, 0.3) - 40.0 * peaks(x, 0.013, 1.0);
    let near = |x: f32| 690.0 - 110.0 * peaks(x, 0.006, 2.0) - 30.0 * peaks(x, 0.019, 0.0);
    for (ridge, lit, shade) in [
        (&far as &dyn Fn(f32) -> f32, Color::rgb(214, 170, 186), Color::rgb(150, 132, 170)),
        (&near, Color::rgb(150, 116, 146), Color::rgb(84, 82, 122)),
    ] {
        for row in 0..=14 {
            let t = row as f32 / 14.0;
            let y = 380.0 + t * 460.0;
            let color = mix(lit, shade, t.sqrt());
            brushes.paint(
                &mut doc,
                mountains,
                "Hard Round",
                60.0,
                color,
                &along(&|x| y.max(ridge(x) + 30.0)),
            );
        }
    }

    // Rolling hills in front: a soft base in rows that follow their top,
    // lighter where the sun reaches, then bristly strokes along the slope.
    let hills = paint_layer(&mut doc, "Hills");
    let hill = |x: f32| 760.0 + (x * 0.003 + 0.5).sin() * 50.0 + (x * 0.009).sin() * 18.0;
    for row in 0..=12 {
        let t = row as f32 / 12.0;
        let y = 740.0 + t * 380.0;
        let color = mix(Color::rgb(126, 150, 82), Color::rgb(40, 82, 52), t);
        brushes.paint(&mut doc, hills, "Soft Round", 140.0, color, &along(&|x| y.max(hill(x) + 70.0)));
    }
    for (below, color) in
        [(30.0, Color::rgb(150, 168, 92)), (110.0, Color::rgb(98, 132, 72)), (200.0, Color::rgb(70, 110, 62))]
    {
        brushes.paint(&mut doc, hills, "Dry Brush", 50.0, color, &along(&|x| hill(x) + below));
    }

    // Pines along the hills, each a zigzag of foliage narrowing to its top.
    let trees = paint_layer(&mut doc, "Trees");
    for (x, tall) in
        [(180.0, 250.0), (260.0, 190.0), (420.0, 300.0), (1560.0, 270.0), (1680.0, 210.0), (1790.0, 320.0)]
    {
        let foot = Point::new(x, hill(x) + 40.0);
        brushes.paint(
            &mut doc,
            trees,
            "Hard Round",
            12.0,
            Color::rgb(60, 40, 36),
            &[foot, foot - Point::new(0.0, tall * 0.4)],
        );
        let zigzag: Vec<Point> = (0..=14)
            .map(|i| {
                let t = i as f32 / 14.0;
                let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                foot + Point::new(side * tall * 0.24 * (1.0 - t), -tall * (0.15 + 0.85 * t))
            })
            .collect();
        brushes.paint(&mut doc, trees, "Foliage", 38.0, Color::rgb(28, 62, 46), &zigzag);
    }

    // Grass and flowers along the bottom.
    let foreground = paint_layer(&mut doc, "Grass");
    for (y, color, size) in [
        (1000.0, Color::rgb(62, 110, 58), 80.0),
        (1040.0, Color::rgb(44, 92, 50), 90.0),
        (1080.0, Color::rgb(30, 70, 42), 100.0),
    ] {
        brushes.paint(&mut doc, foreground, "Grass", size, color, &along(&|x| y + (x * 0.02).sin() * 10.0));
    }
    brushes.paint(
        &mut doc,
        foreground,
        "Stars and Snow",
        14.0,
        Color::rgb(255, 226, 130),
        &along(&|x| 990.0 + (x * 0.05).sin() * 30.0),
    );
    doc
}

/// A new paint layer on the main timeline, with its canvas.
fn paint_layer(doc: &mut Document, name: &str) -> (inkframe_core::LayerId, CanvasId) {
    let layer = doc.add_paint_layer(TimelineId::Main, name, 0).expect("the main timeline is there");
    let (canvas, ..) = doc.paint_canvas_at(layer, 0).expect("a paint layer has a canvas");
    (layer, canvas)
}

/// The ready-made brushes, by name, and a count of strokes so each one's
/// jitter is seeded differently (and the same on every rebuild).
struct Brushes {
    presets: Vec<BrushPreset>,
    strokes: u64,
}

impl Brushes {
    fn new() -> Self {
        Self { presets: BrushSettings::presets(), strokes: 0 }
    }

    /// Paints along `points` with the named brush at `size`, a dab at a time
    /// as a pen would, at full pressure.
    fn paint(
        &mut self,
        doc: &mut Document,
        (_, canvas): (inkframe_core::LayerId, CanvasId),
        brush: &str,
        size: f32,
        color: Color,
        points: &[Point],
    ) {
        let preset =
            self.presets.iter().find(|p| p.name == brush).unwrap_or_else(|| panic!("no brush {brush:?}"));
        self.strokes += 1;
        let settings = BrushSettings { size, ..preset.settings.clone() };
        let mut stroke = BrushStroke::new(settings, BrushMode::Paint(color)).with_seed(self.strokes);
        let canvas = doc.canvas_mut(canvas).expect("the layer's canvas");
        for pair in points.windows(2) {
            // In short steps, as a pen's samples come, so the stroke follows
            // the line rather than cutting corners.
            let steps = (pair[0].distance(pair[1]) / 8.0).ceil().max(1.0) as usize;
            for step in 0..steps {
                let t = step as f32 / steps as f32;
                stroke.add_point(canvas, pair[0] + (pair[1] - pair[0]) * t, 1.0);
            }
        }
        if let Some(last) = points.last() {
            stroke.add_point(canvas, *last, 1.0);
        }
    }
}

/// `a` blended towards `b` by `t`.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color::rgb(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}
