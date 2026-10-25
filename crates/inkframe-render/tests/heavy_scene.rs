//! How long a heavy scene takes to draw: 24 characters rigged on skeletons
//! with target feet, walking, 400 moving shapes with gradients, and two
//! particle layers. Run it by hand, in release, to check the renderer stays
//! quick enough to scrub and play:
//!
//! ```sh
//! cargo test --release -p inkframe-render --test heavy_scene -- --ignored --nocapture
//! ```

use std::time::Instant;

use inkframe_core::{
    BoneBinding, Color, Document, Element, Point, Preset, RotationLimits, Shape, ShapeGradient, Skeleton,
    Style, TimelineId, Transform,
};
use inkframe_render::{RenderOptions, render_frame};

/// The scene, as in the guidebook's walk project, many times over.
pub fn heavy_scene() -> Document {
    let mut doc = Document::default();
    doc.duration = 24;
    let fill = |c: Color| Style { fill: Some(c), stroke: None, gradient: None };

    // Shapes drifting across the stage behind the walkers.
    for i in 0..400 {
        let (x, y) = ((i % 40) as f32 * 48.0 + 24.0, (i / 40) as f32 * 100.0 + 60.0);
        let mut style = fill(Color::rgb(80 + (i % 7) as u8 * 20, 120, 200));
        if i % 3 == 0 {
            style.gradient = Some(ShapeGradient::radial(Color::rgb(255, 230, 160), Color::rgb(230, 120, 80)));
        }
        let shape = if i % 2 == 0 {
            Shape::Ellipse { radius_x: 18.0, radius_y: 18.0 }
        } else {
            Shape::Rect { width: 30.0, height: 30.0, corner_radius: 6.0 }
        };
        let id = doc.add_layer(
            format!("Shape {i}"),
            vec![Element { shape, style }],
            0,
            Transform::at(Point::new(x, y)),
        );
        doc.layer_mut(id).unwrap().track.set_pose(23, Transform::at(Point::new(x + 40.0, y + 10.0)));
    }

    // Walkers in a grid: legs on target feet, each leg following its limb.
    for n in 0..24 {
        let (cx, ground) = ((n % 8) as f32 * 230.0 + 150.0, (n / 8) as f32 * 340.0 + 330.0);
        let hip = ground - 170.0;
        let leg = |dx: f32, c: Color| {
            Element { shape: Shape::Rect { width: 28.0, height: 180.0, corner_radius: 14.0 }, style: fill(c) }
                .transformed(inkframe_core::Affine::translate(Point::new(cx + dx, hip + 85.0)))
                .unwrap()
        };
        let far = doc.add_layer("Far leg", vec![leg(1.0, Color::rgb(70, 115, 190))], 0, Transform::default());
        let body = doc.add_layer(
            "Body",
            vec![
                Element {
                    shape: Shape::Rect { width: 66.0, height: 105.0, corner_radius: 25.0 },
                    style: fill(Color::rgb(240, 160, 70)),
                }
                .transformed(inkframe_core::Affine::translate(Point::new(cx, hip - 25.0)))
                .unwrap(),
            ],
            0,
            Transform::default(),
        );
        let near =
            doc.add_layer("Near leg", vec![leg(-1.0, Color::rgb(90, 150, 230))], 0, Transform::default());
        let folder = doc.add_folder(TimelineId::Main, &[near, body, far]).unwrap();
        let mut skeleton = Skeleton::default();
        let hips = skeleton.add_bone(None, Point::new(cx, hip - 25.0), Point::new(cx, hip), 0);
        let mut limbs = Vec::new();
        for (side, dx) in [(0, -1.0), (1, 1.0)] {
            let thigh =
                skeleton.add_bone(Some(hips), Point::new(cx + dx, hip), Point::new(cx + dx, hip + 85.0), 0);
            let shin = skeleton.add_bone(
                Some(thigh),
                Point::new(cx + dx, hip + 85.0),
                Point::new(cx + dx, ground),
                0,
            );
            let foot =
                skeleton.add_bone(None, Point::new(cx + dx, ground), Point::new(cx + dx + 25.0, ground), 0);
            skeleton.bones[shin].target = Some(foot);
            skeleton.bones[shin].limits = Some(RotationLimits { min: 0.0, max: 150.0 });
            for (frame, step, lift) in [(0, 45.0, 0.0), (12, -45.0, 0.0), (18, 0.0, -24.0), (24, 45.0, 0.0)] {
                let frame = (frame + side * 12) % 25;
                skeleton.move_bone(foot, Point::new(cx + dx + step, ground + lift), frame);
            }
            limbs.push(thigh);
        }
        let doc_layer = doc.layer_mut(folder).unwrap();
        doc_layer.skeleton = Some(skeleton);
        for (layer, binding) in [
            (near, BoneBinding::Limb(limbs[0])),
            (far, BoneBinding::Limb(limbs[1])),
            (body, BoneBinding::Bone(hips)),
        ] {
            doc.layer_mut(layer).unwrap().bone_binding = binding;
        }
    }

    doc.add_particle_layer(Preset::Fire, 0, Point::new(960.0, 1000.0));
    doc.add_particle_layer(Preset::Snow, 0, Point::new(960.0, 0.0));
    doc
}

#[test]
#[ignore = "a timing check to run by hand, in release"]
fn a_heavy_scene_draws_quickly_enough_to_scrub() {
    let doc = heavy_scene();
    let time = |scale: f32| {
        let options = RenderOptions { scale, ..RenderOptions::default() };
        // Once to warm up (fonts, caches), then every frame.
        render_frame(&doc, 0, &options);
        let start = Instant::now();
        for frame in 0..doc.duration {
            assert!(render_frame(&doc, frame, &options).is_some());
        }
        start.elapsed().as_secs_f64() * 1000.0 / doc.duration as f64
    };
    // To look at it: HEAVY_SCENE_PNG=frame.png saves frame 6, and
    // HEAVY_SCENE_SAVE=heavy.inkf the project, to time in the app.
    if let Some(path) = std::env::var_os("HEAVY_SCENE_PNG") {
        render_frame(&doc, 6, &RenderOptions::default()).unwrap().save_png(path).unwrap();
    }
    if let Some(path) = std::env::var_os("HEAVY_SCENE_SAVE") {
        doc.save(std::path::Path::new(&path)).unwrap();
    }
    let layers = doc.all_layers().count();
    let (preview, full) = (time(0.5), time(1.0));
    println!(
        "{layers} layers: {preview:.1} ms a frame at half size (the stage), {full:.1} ms at full size (export)"
    );
}
