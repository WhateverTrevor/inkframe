use super::*;

#[test]
fn ghosts_are_painted_from_coverage_in_their_tint() {
    let mut pixmap = tiny_skia::Pixmap::new(3, 1).unwrap();
    pixmap.fill(tiny_skia::Color::WHITE);
    let tint = inkframe_core::Color::rgb(0, 0, 255);
    // Nothing, fully covered, half covered; at full strength.
    paint_coverage(&mut pixmap, &[0, 255, 128], tint, 1.0);
    fn px(pixmap: &tiny_skia::Pixmap, i: usize) -> (u8, u8, u8, u8) {
        let p = pixmap.pixels()[i];
        (p.red(), p.green(), p.blue(), p.alpha())
    }
    assert_eq!(px(&pixmap, 0), (255, 255, 255, 255));
    assert_eq!(px(&pixmap, 1), (0, 0, 255, 255));
    let (r, _, b, a) = px(&pixmap, 2);
    assert!((r as i32 - 127).abs() <= 1 && b == 255 && a == 255, "{:?}", px(&pixmap, 2));
    // Coverage of the wrong size (a texture resized since) paints nothing.
    paint_coverage(&mut pixmap, &[255], tint, 1.0);
    assert_eq!(px(&pixmap, 0), (255, 255, 255, 255));
}

#[test]
fn view_round_trips() {
    for rotation in [0.0, 0.7, -2.0] {
        let view = View::stage(pos2(100.0, 50.0), vec2(640.0, 480.0), 0.5, rotation);
        let p = Point::new(300.0, 200.0);
        assert!(view.to_local(view.to_screen(p)).distance(p) < 1e-3);
    }
}

#[test]
fn square_constraint_keeps_direction() {
    let end = constrain_square(Point::ZERO, Point::new(-10.0, 4.0));
    assert_eq!(end, Point::new(-10.0, 10.0));
}

// ---- Interaction tests: drive the real stage with simulated mouse input.

use egui_kittest::Harness;
use inkframe_core::{Color, Stroke};

use crate::inside::ShapeRef;

type State = (StageView, Editor);

fn harness(editor: Editor) -> Harness<'static, State> {
    // Step at 60 fps rather than the harness's default quarter-second, so
    // timing-based input like double-clicks behaves as it does for real.
    let mut harness =
        Harness::builder().with_size(vec2(800.0, 600.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, (stage, editor): &mut State| stage.show(ui, editor),
            (StageView::default(), editor),
        );
    harness.run();
    harness
}

/// Screen position of a stage point, using the stage's current view.
fn screen(harness: &Harness<'_, State>, x: f32, y: f32) -> Pos2 {
    let (stage, editor) = harness.state();
    stage.view(stage.last_area, &editor.doc).to_screen(Point::new(x, y))
}

/// Presses at `from`, moves through `path`, and releases at the last point.
fn drag(harness: &mut Harness<'_, State>, from: Pos2, path: &[Pos2]) {
    harness.drag_at(from);
    harness.step();
    for &p in path {
        harness.hover_at(p);
        harness.step();
    }
    harness.drop_at(*path.last().unwrap());
    harness.run();
}

#[test]
fn rectangle_tool_draws_a_new_layer() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Rectangle;
    let mut h = harness(editor);

    let (from, to) = (screen(&h, 100.0, 100.0), screen(&h, 500.0, 300.0));
    drag(&mut h, from, &[to]);

    let editor = &h.state().1;
    assert_eq!(editor.doc.layers.len(), 1);
    let layer = &editor.doc.layers[0];
    let Shape::Rect { width, height, .. } = layer.drawings[0].elements[0].shape else { panic!("not a rect") };
    // Allow a pixel of slack for screen-to-stage rounding.
    assert!((width - 400.0).abs() < 3.0 && (height - 200.0).abs() < 3.0, "{width}×{height}");
    assert!(layer.transform_at(0).position.distance(Point::new(300.0, 200.0)) < 3.0);
    assert_eq!(editor.selection, Some(layer.id));
}

#[test]
fn the_camera_tool_adds_a_camera_and_pans_it_so_the_scene_follows() {
    let mut doc = Document::default();
    let dot = doc.add_layer(
        "Dot",
        vec![Element {
            shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }],
        0,
        Transform::at(Point::new(500.0, 500.0)),
    );
    let mut editor = Editor::new(doc);
    editor.tool = Tool::Camera;
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 960.0, 540.0), screen(&h, 1060.0, 540.0));
    drag(&mut h, from, &[to]);

    let doc = &h.state().1.doc;
    assert!(doc.camera().is_some(), "added");
    // Dragged 100 right: the dot appears 100 further right.
    let layer = doc.layer(dot).unwrap();
    let seen = doc.layer_matrix(layer, 0).apply(Point::ZERO);
    assert!(seen.distance(Point::new(600.0, 500.0)) < 3.0, "{seen:?}");
}

#[test]
fn the_paint_bucket_fills_between_a_vector_layers_lines() {
    // An inked square: four strokes in one drawing, with no fill.
    let mut doc = Document::default();
    let line = |a: Point, b: Point| Element {
        shape: Shape::Path(Path {
            start: a,
            segments: vec![inkframe_core::Segment::Line { to: b }],
            closed: false,
        }),
        style: Style { fill: None, stroke: Some(Stroke::new(Color::BLACK, 3.0)), gradient: None },
    };
    let p = Point::new;
    let lines = vec![
        line(p(-100.0, -100.0), p(100.0, -100.0)),
        line(p(100.0, -100.0), p(100.0, 100.0)),
        line(p(100.0, 100.0), p(-100.0, 100.0)),
        line(p(-100.0, 100.0), p(-100.0, -100.0)),
    ];
    let id = doc.add_layer("Ink", lines, 0, Transform::at(p(500.0, 400.0)));
    let mut editor = Editor::new(doc);
    editor.select(Some(id));
    editor.tool = Tool::PaintBucket;
    editor.paint_color = Color::rgb(255, 0, 0);
    let mut h = harness(editor);
    let inside = screen(&h, 520.0, 410.0);
    drag(&mut h, inside, &[inside]);

    let layer = h.state().1.doc.layer(id).unwrap();
    assert_eq!(layer.drawings[0].colour_art.len(), 1, "filled");
    assert_eq!(layer.drawings[0].colour_art[0].style.fill, Some(Color::rgb(255, 0, 0)));
    assert!(h.state().1.paint_target().is_none(), "no paint layer was made");
}

#[test]
fn the_pencil_draws_a_line_that_tapers_and_the_width_tool_widens_it() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Pencil;
    editor.pencil_stroke.width = 8.0;
    let mut h = harness(editor);
    let points: Vec<Pos2> = (0..=20).map(|i| screen(&h, 200.0 + i as f32 * 20.0, 300.0)).collect();
    drag(&mut h, points[0], &points[1..]);

    let (widths, anchors, id) = {
        let editor = &h.state().1;
        let layer = editor.doc.layers.last().unwrap();
        let Shape::Line { path, widths } = &layer.drawings[0].elements[0].shape else {
            panic!("a pencil line")
        };
        let to_stage = editor.doc.layer_matrix(layer, 0);
        let anchors: Vec<Point> =
            inkframe_core::centreline_anchors(path).into_iter().map(|a| to_stage.apply(a)).collect();
        (widths.clone(), anchors, layer.id)
    };
    // Thin at the ends, the pencil's width in the middle (a mouse
    // presses fully).
    let middle = widths[widths.len() / 2];
    assert!(widths[0] < middle && widths[widths.len() - 1] < middle, "{widths:?}");
    assert!((middle - 8.0).abs() < 1e-3, "{widths:?}");

    // The Width tool: grab the middle point and pull 60 off the line.
    h.state_mut().1.tool = Tool::Width;
    h.run();
    let at = anchors[anchors.len() / 2];
    let path = [screen(&h, at.x, at.y + 30.0), screen(&h, at.x, at.y + 60.0)];
    let from = screen(&h, at.x, at.y);
    drag(&mut h, from, &path);
    let layer = h.state().1.doc.layer(id).unwrap();
    let Shape::Line { widths, .. } = &layer.drawings[0].elements[0].shape else { panic!("still a line") };
    assert!((widths[widths.len() / 2] - 120.0).abs() < 4.0, "{widths:?}");
}

fn text_content(h: &Harness<'_, State>, id: LayerId) -> String {
    match &h.state().1.doc.layer(id).unwrap().drawings[0].elements[0].shape {
        Shape::Text(text) => text.content.clone(),
        _ => panic!("not text"),
    }
}

#[test]
fn text_tool_types_straight_onto_the_stage() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Text;
    let mut h = harness(editor);

    let at = screen(&h, 300.0, 400.0);
    click(&mut h, at);
    let id = h.state().1.selection.expect("the new text is selected");
    // The first line's baseline starts where the click was.
    let position = h.state().1.doc.layer(id).unwrap().transform_at(0).position;
    assert!(position.distance(Point::new(300.0, 400.0)) < 3.0);

    h.event(egui::Event::Text("Hello".into()));
    h.run();
    h.key_press(egui::Key::Backspace);
    h.run();
    h.key_press(egui::Key::Enter);
    h.event(egui::Event::Text("there".into()));
    h.run();
    assert_eq!(text_content(&h, id), "Hell\nthere");
    // The app does this every frame; the stage alone doesn't.
    h.state_mut().1.remeasure_text();
    let Shape::Text(text) = &h.state().1.doc.layer(id).unwrap().drawings[0].elements[0].shape else {
        unreachable!()
    };
    assert!(text.measured.width() > 0.0, "it was measured, so it can be selected");

    // Typing keys that are also shortcuts (V is the Selection tool)
    // types them instead of switching tools.
    h.event(egui::Event::Text("v".into()));
    h.run();
    assert_eq!(h.state().1.tool, Tool::Text);
    assert!(text_content(&h, id).ends_with('v'));

    // Escape stops editing; the text stays.
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().1.doc.layers.len(), 1);
}

#[test]
fn command_z_while_typing_undoes_the_typing() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Text;
    let mut h = harness(editor);
    let at = screen(&h, 300.0, 400.0);
    click(&mut h, at);
    let id = h.state().1.selection.expect("the new text is selected");

    for t in ["Hi", " ", "there"] {
        h.event(egui::Event::Text(t.into()));
        h.run();
    }
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(text_content(&h, id), "Hi ", "the last word comes off");
    h.key_press_modifiers(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::Z);
    h.run();
    assert_eq!(text_content(&h, id), "Hi there", "and goes back on");
    assert!(h.state().0.text_editing.is_some(), "still typing");
}

#[test]
fn typing_on_the_stage_accepts_input_methods() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Text;
    let mut h = harness(editor);
    let at = screen(&h, 300.0, 400.0);
    click(&mut h, at);
    h.run();
    // The stage tells the system where the cursor is, which turns input
    // methods on and puts their candidate window there.
    let ime = h.output().platform_output.ime.expect("input methods enabled while typing");
    assert!(ime.rect.center().distance(at) < 40.0, "near the click: {:?}", ime.rect);

    let id = h.state().1.selection.unwrap();
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit { text: "nihao".into(), active_range_chars: None }));
    h.run();
    assert_eq!(text_content(&h, id), "nihao");
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("你好".into())));
    h.run();
    assert_eq!(text_content(&h, id), "你好");
}

#[test]
fn empty_text_is_removed_when_editing_stops() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Text;
    let mut h = harness(editor);
    let at = screen(&h, 300.0, 400.0);
    click(&mut h, at);
    assert_eq!(h.state().1.doc.layers.len(), 1);
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.state().1.doc.layers.is_empty(), "nothing was typed, so nothing is left behind");
}

#[test]
fn clicking_inside_text_moves_the_cursor_there() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Text;
    let mut h = harness(editor);
    let at = screen(&h, 300.0, 400.0);
    click(&mut h, at);
    let id = h.state().1.selection.unwrap();
    h.event(egui::Event::Text("ab".into()));
    h.run();

    // Wait out the double-click window (a quick second click would
    // select the whole word), then click just inside the text's start,
    // which puts the cursor before the "a", and type.
    for _ in 0..40 {
        h.step();
    }
    let at = screen(&h, 301.0, 390.0);
    click(&mut h, at);
    h.event(egui::Event::Text("X".into()));
    h.run();
    assert_eq!(text_content(&h, id), "Xab");
}

#[test]
fn new_drawings_start_on_the_current_frame() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Rectangle;
    editor.set_frame(10);
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 100.0, 100.0), screen(&h, 300.0, 300.0));
    drag(&mut h, from, &[to]);

    let layer = &h.state().1.doc.layers[0];
    assert_eq!(layer.start, 10);
    assert!(!layer.is_on(9) && layer.is_on(10));
}

#[test]
fn dragging_a_layer_on_a_later_frame_creates_a_tween() {
    let mut editor = Editor::new(Document::default());
    let square = Element {
        shape: Shape::Rect { width: 200.0, height: 200.0, corner_radius: 0.0 },
        style: editor.draw_style.clone(),
    };
    let id = editor.doc.add_layer("Box", vec![square], 0, Transform::at(Point::new(400.0, 400.0)));
    editor.set_frame(24);
    let mut h = harness(editor);

    let grab = screen(&h, 400.0, 400.0);
    let release = screen(&h, 1000.0, 400.0);
    drag(&mut h, grab, &[grab + vec2(20.0, 0.0), release]);

    let track = &h.state().1.doc.layer(id).unwrap().track;
    assert_eq!(track.key_frames(), [0, 24], "a keyframe should be added on frame 24");
    let moved = track.position.key_at(24).unwrap().value;
    assert!(moved.distance(Point::new(1000.0, 400.0)) < 3.0, "moved to {moved:?}");
    // Frame 0 is untouched, so the layer now tweens between the two.
    assert_eq!(track.position.key_at(0).unwrap().value, Point::new(400.0, 400.0));
    // Only position changed, so only position was keyed.
    assert!(track.rotation.key_at(24).is_none() && track.opacity.key_at(24).is_none());
}

#[test]
fn pencil_draws_a_centered_stroke() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Pencil;
    let mut h = harness(editor);

    let path: Vec<Pos2> = (1..=20)
        .map(|i| {
            let x = 200.0 + i as f32 * 40.0;
            let y = 500.0 + (i as f32 * 0.5).sin() * 150.0;
            screen(&h, x, y)
        })
        .collect();
    let start = screen(&h, 200.0, 500.0);
    drag(&mut h, start, &path);

    let layer = &h.state().1.doc.layers[0];
    let Shape::Line { path: stroke, .. } = &layer.drawings[0].elements[0].shape else { panic!("not a line") };
    assert!(stroke.segments.len() >= 2, "a wavy stroke should keep several segments");
    // The path was recentered, so its local bounds straddle the origin.
    let bounds = h.state().1.doc.layer_bounds(layer, 0).unwrap();
    assert!(bounds.center().distance(Point::ZERO) < 30.0);

    // With pressure and tapering off, it's a plain stroke.
    h.state_mut().1.pencil_pressure = false;
    h.state_mut().1.pencil_taper = false;
    // Deselected, so the second stroke starts a layer of its own.
    h.state_mut().1.select(None);
    h.run();
    drag(&mut h, start, &path);
    let layer = h.state().1.doc.layers.last().unwrap();
    assert!(matches!(layer.drawings[0].elements[0].shape, Shape::Path(_)));
}

/// An editor with a selected 200×200 box centered at (400, 400). It has
/// no stroke, so its selection bounds are exactly the shape's edges.
fn editor_with_selected_box() -> (Editor, LayerId) {
    let mut editor = Editor::new(Document::default());
    let square = Element {
        shape: Shape::Rect { width: 200.0, height: 200.0, corner_radius: 0.0 },
        style: Style { stroke: None, ..editor.draw_style.clone() },
    };
    let id = editor.doc.add_layer("Box", vec![square], 0, Transform::at(Point::new(400.0, 400.0)));
    editor.selection = Some(id);
    (editor, id)
}

fn pose(harness: &Harness<'_, State>, id: LayerId) -> Transform {
    let editor = &harness.state().1;
    editor.doc.layer(id).unwrap().transform_at(editor.frame)
}

#[test]
fn corner_handle_scales_with_the_opposite_corner_pinned() {
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);

    let corner = screen(&h, 500.0, 500.0);
    let target = screen(&h, 700.0, 700.0);
    drag(&mut h, corner, &[corner + vec2(10.0, 10.0), target]);

    let pose = pose(&h, id);
    assert!(pose.scale.distance(Point::new(2.0, 2.0)) < 0.02, "scale {:?}", pose.scale);
    // The top-left corner stayed at (300, 300), so the center moved to (500, 500).
    assert!(pose.position.distance(Point::new(500.0, 500.0)) < 3.0, "position {:?}", pose.position);
}

#[test]
fn handles_sit_outside_a_thick_stroke() {
    // A 40-unit stroke paints 20 units past the box's edges, so the
    // bottom-right handle belongs at (520, 520), not on the outline at
    // (500, 500), which is now inside the painted stroke.
    let (mut editor, id) = editor_with_selected_box();
    editor.doc.layer_mut(id).unwrap().drawings[0].elements[0].style.stroke =
        Some(Stroke::new(Color::BLACK, 40.0));
    let mut h = harness(editor);

    let corner = screen(&h, 520.0, 520.0);
    // Dragging that corner to (760, 760) doubles the 240-unit box, with
    // the opposite corner at (280, 280) pinned.
    let target = screen(&h, 760.0, 760.0);
    drag(&mut h, corner, &[corner + vec2(10.0, 10.0), target]);

    let pose = pose(&h, id);
    assert!(pose.scale.distance(Point::new(2.0, 2.0)) < 0.02, "scale {:?}", pose.scale);
    assert!(pose.position.distance(Point::new(520.0, 520.0)) < 3.0, "position {:?}", pose.position);
}

#[test]
fn dragging_the_motion_path_curves_the_motion() {
    // The box moves from (400, 400) on frame 1 to (800, 400) on frame 11.
    let (mut editor, id) = editor_with_selected_box();
    editor.doc.layer_mut(id).unwrap().track.position.set(10, Point::new(800.0, 400.0));
    let mut h = harness(editor);

    // Grab the path halfway, clear of the box, and pull it up.
    let from = screen(&h, 650.0, 400.0);
    let to = screen(&h, 650.0, 250.0);
    drag(&mut h, from, &[from + vec2(0.0, -10.0), to]);

    let editor = &h.state().1;
    let track = &editor.doc.layer(id).unwrap().track;
    let midway = track.position.sample(5.0).unwrap();
    assert!(midway.y < 330.0, "the middle of the motion should arc upward: {midway:?}");
    // The keys stay where they were, and the box didn't move.
    assert_eq!(track.position.sample(0.0), Some(Point::new(400.0, 400.0)));
    assert_eq!(track.position.sample(10.0), Some(Point::new(800.0, 400.0)));
}

/// Two 100×100 boxes, at (200, 200) and (600, 200), nothing selected.
fn two_boxes() -> (Editor, LayerId, LayerId) {
    let mut editor = Editor::new(Document::default());
    let square = Element {
        shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
        style: Style { fill: Some(inkframe_core::Color::BLACK), stroke: None, gradient: None },
    };
    let a = editor.doc.add_layer("A", vec![square.clone()], 0, Transform::at(Point::new(200.0, 200.0)));
    let b = editor.doc.add_layer("B", vec![square], 0, Transform::at(Point::new(600.0, 200.0)));
    (editor, a, b)
}

#[test]
fn dragging_a_box_over_empty_stage_selects_what_it_touches() {
    let (editor, a, b) = two_boxes();
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 100.0, 400.0), screen(&h, 700.0, 180.0));
    drag(&mut h, from, &[from + vec2(10.0, -10.0), to]);
    let mut selected = h.state().1.selected_ids();
    selected.sort();
    assert_eq!(selected, [a, b]);
}

#[test]
fn shift_click_adds_and_dragging_moves_every_selected_layer() {
    let (editor, a, b) = two_boxes();
    let mut h = harness(editor);
    let on_a = screen(&h, 200.0, 200.0);
    click(&mut h, on_a);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    let on_b = screen(&h, 600.0, 200.0);
    click(&mut h, on_b);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    assert_eq!(h.state().1.selected_ids().len(), 2);

    // Drag A: B comes along.
    let to = screen(&h, 250.0, 300.0);
    for _ in 0..30 {
        h.step(); // past the double-click window
    }
    drag(&mut h, on_a, &[on_a + vec2(10.0, 10.0), to]);
    let editor = &h.state().1;
    let at = |id| editor.doc.layer(id).unwrap().transform_at(0).position;
    assert!(at(a).distance(Point::new(250.0, 300.0)) < 3.0, "{:?}", at(a));
    assert!(at(b).distance(Point::new(650.0, 300.0)) < 3.0, "{:?}", at(b));
}

#[test]
fn several_layers_scale_together_from_the_group_corner() {
    // The boxes span (150, 150) to (650, 250) together. Drag the group's
    // bottom-right corner to (1150, 350): twice as big, top-left pinned.
    let (mut editor, a, b) = two_boxes();
    editor.select_many(&[a, b]);
    let mut h = harness(editor);
    let corner = screen(&h, 650.0, 250.0);
    let target = screen(&h, 1150.0, 350.0);
    drag(&mut h, corner, &[corner + vec2(10.0, 10.0), target]);

    let editor = &h.state().1;
    let pose = |id| editor.doc.layer(id).unwrap().transform_at(0);
    assert!(pose(a).position.distance(Point::new(250.0, 250.0)) < 4.0, "{:?}", pose(a).position);
    assert!(pose(b).position.distance(Point::new(1050.0, 250.0)) < 4.0, "{:?}", pose(b).position);
    assert!(pose(a).scale.distance(Point::new(2.0, 2.0)) < 0.03, "{:?}", pose(a).scale);
}

#[test]
fn several_layers_slant_together_from_the_group_edge() {
    // The boxes span (150, 150) to (650, 250) together. Slide the
    // group's top edge 100 px right: both lean 45° over its 100 px
    // height, and the bottom stays put.
    let (mut editor, a, b) = two_boxes();
    editor.select_many(&[a, b]);
    let mut h = harness(editor);
    let edge = screen(&h, 250.0, 150.0);
    let target = screen(&h, 350.0, 150.0);
    drag(&mut h, edge, &[edge + vec2(5.0, 0.0), target]);

    let editor = &h.state().1;
    let pose = |id| editor.doc.layer(id).unwrap().transform_at(0);
    for id in [a, b] {
        assert!((pose(id).skew.x - 45.0).abs() < 2.0, "{:?}", pose(id).skew);
        // Each box's bottom middle (50 px below its centre) is where it was.
        let bottom = pose(id).to_affine().apply(Point::new(0.0, 50.0));
        assert!((bottom.y - 250.0).abs() < 2.0, "{bottom:?}");
    }
    assert!((pose(b).position.x - pose(a).position.x - 400.0).abs() < 2.0, "still 400 px apart");
}

#[test]
fn several_layers_turn_together_around_their_middle() {
    let (mut editor, a, b) = two_boxes();
    let group: Vec<GroupMember> = [a, b]
        .iter()
        .map(|&id| GroupMember {
            id,
            pose: editor.doc.layer(id).unwrap().transform_at(0),
            space: Affine::IDENTITY,
        })
        .collect();
    let before = Transform::at(Point::new(400.0, 200.0));
    transform_group(&mut editor, &group, &before, &Transform { rotation: 90.0, ..before });

    let pose = |id| editor.doc.layer(id).unwrap().transform_at(0);
    // A was 200 px left of the middle; a quarter turn clockwise puts it
    // 200 px above, turned 90° itself.
    assert!(pose(a).position.distance(Point::new(400.0, 0.0)) < 1e-3, "{:?}", pose(a).position);
    assert!(pose(b).position.distance(Point::new(400.0, 400.0)) < 1e-3);
    assert_eq!(pose(a).rotation, 90.0);
}

#[test]
fn dragging_the_top_edge_skews_the_layer() {
    // The box spans 300–500 both ways. Grab the top edge away from its
    // middle handle and slide it 100 px right.
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);
    let edge = screen(&h, 350.0, 300.0);
    let target = screen(&h, 450.0, 300.0);
    drag(&mut h, edge, &[edge + vec2(5.0, 0.0), target]);

    let pose = pose(&h, id);
    assert!((pose.skew.x - 26.565).abs() < 1.0, "atan(100 / 200): {}", pose.skew.x);
    // The bottom edge's middle didn't move.
    let bottom = pose.to_affine().apply(Point::new(0.0, 100.0));
    assert!(bottom.distance(Point::new(400.0, 500.0)) < 2.0, "{bottom:?}");
}

#[test]
fn rotation_knob_turns_the_layer() {
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);

    let knob = {
        let (stage, editor) = h.state();
        let view = stage.view(stage.last_area, &editor.doc);
        Handles::for_selection(editor, view).unwrap().knob
    };
    // Swing the knob from straight above the pivot round to its right:
    // a quarter turn clockwise.
    let pivot = screen(&h, 400.0, 400.0);
    let radius = pivot.distance(knob);
    let path = [pivot + vec2(radius, -radius).normalized() * radius, pivot + vec2(radius, 0.0)];
    drag(&mut h, knob, &path);

    let rotation = pose(&h, id).rotation;
    assert!((rotation - 90.0).abs() < 1.0, "rotation {rotation}");
}

/// Where a point on a layer's drawing is on the stage.
fn on_stage(harness: &Harness<'_, State>, id: LayerId, local: Point) -> Point {
    let editor = &harness.state().1;
    let layer = editor.doc.layer(id).unwrap();
    editor.doc.layer_matrix(layer, editor.frame).apply(local)
}

#[test]
fn dragging_the_pivot_moves_it_but_not_the_layer() {
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);

    // The pivot starts in the middle; Option-drag it up and to the
    // right, short of the corner, which would snap it.
    let from = screen(&h, 400.0, 400.0);
    let to = screen(&h, 470.0, 340.0);
    hold_alt(&mut h, true);
    drag(&mut h, from, &[from + vec2(10.0, -10.0), to]);

    let pivot = h.state().1.doc.layer(id).unwrap().pivot;
    assert!(pivot.distance(Point::new(70.0, -60.0)) < 1.0, "pivot {pivot:?}");
    // The box itself stayed put.
    let corner = on_stage(&h, id, Point::new(-100.0, -100.0));
    assert!(corner.distance(Point::new(300.0, 300.0)) < 0.1, "corner {corner:?}");
}

#[test]
fn a_plain_drag_on_the_pivot_moves_the_layer() {
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 400.0, 400.0), screen(&h, 450.0, 400.0));
    drag(&mut h, from, &[from + vec2(10.0, 0.0), to]);
    assert_eq!(h.state().1.doc.layer(id).unwrap().pivot, Point::ZERO);
    assert!(pose(&h, id).position.distance(Point::new(450.0, 400.0)) < 1.0);
}

fn hold_alt(harness: &mut Harness<'_, State>, held: bool) {
    let modifiers = egui::Modifiers { alt: held, ..egui::Modifiers::NONE };
    harness.event(egui::Event::ModifiersChanged(modifiers));
    harness.step();
}

#[test]
fn the_pivot_snaps_to_a_corner() {
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);
    let from = screen(&h, 400.0, 400.0);
    let to = screen(&h, 300.0, 500.0) + vec2(3.0, -2.0);
    hold_alt(&mut h, true);
    drag(&mut h, from, &[from + vec2(-10.0, 10.0), to]);
    assert_eq!(h.state().1.doc.layer(id).unwrap().pivot, Point::new(-100.0, 100.0));
}

#[test]
fn rotating_swings_the_layer_around_its_pivot() {
    let (mut editor, id) = editor_with_selected_box();
    // The pivot on the middle of the left edge, like an elbow.
    editor.doc.set_pivot(id, Point::new(-100.0, 0.0));
    let mut h = harness(editor);

    let knob = {
        let (stage, editor) = h.state();
        let view = stage.view(stage.last_area, &editor.doc);
        Handles::for_selection(editor, view).unwrap().knob
    };
    let pivot = screen(&h, 300.0, 400.0);
    // The knob stays above the box's middle; swing it a quarter turn
    // clockwise around the elbow.
    let turned = |degrees: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let v = knob - pivot;
        pivot + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
    };
    let path = [turned(30.0), turned(60.0), turned(90.0)];
    drag(&mut h, knob, &path);

    assert!((pose(&h, id).rotation - 90.0).abs() < 1.0, "rotation {}", pose(&h, id).rotation);
    // The elbow stayed where it was; the far edge swung down.
    let elbow = on_stage(&h, id, Point::new(-100.0, 0.0));
    assert!(elbow.distance(Point::new(300.0, 400.0)) < 0.5, "elbow {elbow:?}");
    let hand = on_stage(&h, id, Point::new(100.0, 0.0));
    assert!(hand.distance(Point::new(300.0, 600.0)) < 4.0, "hand {hand:?}");
}

/// A box at (400, 400) turned a quarter turn, with a smaller box at
/// (700, 400) attached to it and selected.
fn parent_and_child() -> (Editor, LayerId, LayerId) {
    let (mut editor, parent) = editor_with_selected_box();
    editor.set_pose(parent, Transform { rotation: 90.0, ..Transform::at(Point::new(400.0, 400.0)) });
    let small = Element {
        shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
        style: Style { stroke: None, ..editor.draw_style.clone() },
    };
    let child = editor.doc.add_layer("Hand", vec![small], 0, Transform::at(Point::new(700.0, 400.0)));
    assert!(editor.doc.set_parent(child, Some(parent), 0));
    editor.selection = Some(child);
    (editor, parent, child)
}

#[test]
fn a_child_drags_where_the_pointer_goes_inside_a_turned_parent() {
    let (editor, _, child) = parent_and_child();
    let mut h = harness(editor);

    let handles_at = {
        let (stage, editor) = h.state();
        let view = stage.view(stage.last_area, &editor.doc);
        Handles::for_selection(editor, view).unwrap().center
    };
    assert!(handles_at.distance(screen(&h, 700.0, 400.0)) < 0.5, "the handles go around the child");

    let from = screen(&h, 700.0, 400.0);
    let to = screen(&h, 800.0, 400.0);
    drag(&mut h, from, &[from + vec2(10.0, 0.0), to]);

    // Rightward on the stage is "up" in the turned parent's space, so the
    // child's own key moved along y, and on the stage it went right.
    let at = on_stage(&h, child, Point::ZERO);
    assert!(at.distance(Point::new(800.0, 400.0)) < 1.0, "child at {at:?}");
    let key = pose(&h, child).position;
    assert!((key.y - (-400.0)).abs() < 1.0, "key {key:?}");
}

#[test]
fn dragging_a_parent_and_child_together_moves_the_child_once() {
    let (mut editor, parent, child) = parent_and_child();
    editor.select_many(&[parent, child]);
    let mut h = harness(editor);
    let from = screen(&h, 400.0, 400.0);
    let to = screen(&h, 400.0, 500.0);
    drag(&mut h, from, &[from + vec2(0.0, 10.0), to]);
    let at = on_stage(&h, child, Point::ZERO);
    assert!(at.distance(Point::new(700.0, 500.0)) < 1.0, "child at {at:?}");
}

/// A torso box at (300, 400), and an arm hanging from it: an upper arm
/// and forearm, each 200 long and pivoting at their left ends, with the
/// shoulder at (400, 400) and the elbow at (600, 400). The Bone tool is
/// picked.
fn bone_rig() -> (Editor, LayerId, LayerId, LayerId) {
    let mut editor = Editor::new(Document::default());
    let style = Style { stroke: None, ..editor.draw_style.clone() };
    let rect = |width, height| Element {
        shape: Shape::Rect { width, height, corner_radius: 0.0 },
        style: style.clone(),
    };
    let doc = &mut editor.doc;
    let torso = doc.add_layer("Torso", vec![rect(100.0, 100.0)], 0, Transform::at(Point::new(300.0, 400.0)));
    let upper = doc.add_layer("Upper", vec![rect(200.0, 40.0)], 0, Transform::at(Point::new(500.0, 400.0)));
    let fore = doc.add_layer("Fore", vec![rect(200.0, 40.0)], 0, Transform::at(Point::new(700.0, 400.0)));
    for (layer, parent) in [(upper, torso), (fore, upper)] {
        doc.set_pivot(layer, Point::new(-100.0, 0.0));
        doc.set_parent(layer, Some(parent), 0);
    }
    editor.tool = Tool::Bone;
    (editor, torso, upper, fore)
}

#[test]
fn the_bone_tool_pulls_the_arm_after_the_pointer() {
    let (editor, torso, upper, fore) = bone_rig();
    let mut h = harness(editor);
    let from = screen(&h, 750.0, 400.0);
    let to = screen(&h, 650.0, 500.0);
    drag(&mut h, from, &[from + vec2(-10.0, 10.0), to]);

    // The spot grabbed, halfway along the forearm, is under the pointer.
    let grabbed = on_stage(&h, fore, Point::new(50.0, 0.0));
    assert!(grabbed.distance(Point::new(650.0, 500.0)) < 2.0, "grabbed at {grabbed:?}");
    // The shoulder stayed, and the torso neither moved nor turned.
    assert!(on_stage(&h, upper, Point::new(-100.0, 0.0)).distance(Point::new(400.0, 400.0)) < 0.01);
    assert_eq!(pose(&h, torso), Transform::at(Point::new(300.0, 400.0)));
    assert_eq!(h.state().1.selection, Some(fore));
}

#[test]
fn the_bone_tool_moves_a_whole_rig_by_its_root() {
    let (editor, torso, _, fore) = bone_rig();
    let mut h = harness(editor);
    let from = screen(&h, 300.0, 400.0);
    let to = screen(&h, 300.0, 300.0);
    drag(&mut h, from, &[from + vec2(0.0, -10.0), to]);
    assert!(pose(&h, torso).position.distance(Point::new(300.0, 300.0)) < 1.0);
    let hand = on_stage(&h, fore, Point::new(100.0, 0.0));
    assert!(hand.distance(Point::new(800.0, 300.0)) < 1.0, "the arm came along: {hand:?}");
}

#[test]
fn dragging_a_warp_point_bulges_the_drawing() {
    let mut editor = Editor::new(Document::default());
    let square = Element {
        shape: Shape::Rect { width: 200.0, height: 200.0, corner_radius: 0.0 },
        style: Style { stroke: None, ..editor.draw_style.clone() },
    };
    let id = editor.doc.add_layer("Blob", vec![square], 0, Transform::at(Point::new(400.0, 400.0)));
    let bounds = inkframe_core::Rect::from_center_size(Point::ZERO, 200.0, 200.0);
    editor.doc.layer_mut(id).unwrap().warp = Some(inkframe_core::Warp::over(bounds, 3, 3, 0));
    editor.tool = Tool::Bone;
    let mut h = harness(editor);

    // Pull the middle of the right edge, at (500, 400), 60 further out.
    let from = screen(&h, 500.0, 400.0);
    let to = screen(&h, 560.0, 400.0);
    drag(&mut h, from, &[from + vec2(5.0, 0.0), to]);

    let editor = &h.state().1;
    let warp = editor.doc.layer(id).unwrap().warp.as_ref().unwrap();
    let moved = warp.points_at(0)[5];
    assert!(moved.distance(Point::new(160.0, 0.0)) < 3.0, "{moved:?}");
    // The drawing bulges out there, and the layer itself didn't move.
    assert!(editor.hit_test(Point::new(530.0, 400.0), 1.0) == Some(id));
    assert_eq!(pose(&h, id), Transform::at(Point::new(400.0, 400.0)));
}

#[test]
fn dragging_a_deformer_joint_bends_the_drawing() {
    let mut editor = Editor::new(Document::default());
    let bar = Element {
        shape: Shape::Rect { width: 200.0, height: 40.0, corner_radius: 0.0 },
        style: Style { stroke: None, ..editor.draw_style.clone() },
    };
    let id = editor.doc.add_layer("Tail", vec![bar], 0, Transform::at(Point::new(400.0, 400.0)));
    let bounds = inkframe_core::Rect::from_center_size(Point::ZERO, 200.0, 40.0);
    editor.doc.layer_mut(id).unwrap().deformer = Some(inkframe_core::Deformer::along(bounds, 2, 0));
    editor.tool = Tool::Bone;
    let mut h = harness(editor);

    // Swing the tip joint, at (500, 400), round to straight below the
    // middle joint.
    let from = screen(&h, 500.0, 400.0);
    let to = screen(&h, 400.0, 500.0);
    drag(&mut h, from, &[from + vec2(-5.0, 10.0), to]);

    let editor = &h.state().1;
    let layer = editor.doc.layer(id).unwrap();
    let bends = layer.deformer.as_ref().unwrap().bends_at(0);
    assert!((bends[1] - 90.0).abs() < 1.0, "{bends:?}");
    // The layer itself didn't move, and clicking the bent end hits it.
    assert_eq!(pose(&h, id), Transform::at(Point::new(400.0, 400.0)));
    assert_eq!(editor.hit_test(Point::new(400.0, 480.0), 1.0), Some(id));
    assert_eq!(editor.hit_test(Point::new(480.0, 400.0), 1.0), None, "nothing left where the end was");
}

/// The colour of stage pixel (`x`, `y`) on the selected paint layer.
fn painted_at(harness: &Harness<'_, State>, x: i64, y: i64) -> [u8; 4] {
    let editor = &harness.state().1;
    let (_, canvas, ..) = editor.paint_target().expect("a paint layer is selected");
    editor.doc.canvas(canvas).unwrap().pixel(x, y)
}

#[test]
fn the_brush_paints_a_new_paint_layer_along_the_stroke() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Brush;
    editor.smoothing = 0.0;
    editor.paint_color = Color::rgb(200, 0, 0);
    let mut h = harness(editor);
    let (from, middle, to) = (screen(&h, 300.0, 500.0), screen(&h, 600.0, 500.0), screen(&h, 900.0, 500.0));
    drag(&mut h, from, &[middle, to]);

    assert_eq!(h.state().1.layers().len(), 1, "a paint layer was made for the stroke");
    for x in [300, 450, 600, 750, 900] {
        assert_eq!(painted_at(&h, x, 500), [200, 0, 0, 255], "at x = {x}");
    }
    assert_eq!(painted_at(&h, 600, 560), [0; 4], "the stroke is only as wide as the brush");
}

#[test]
fn a_tap_with_the_brush_leaves_a_dot() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Brush;
    let mut h = harness(editor);
    let at = screen(&h, 400.0, 300.0);
    click(&mut h, at);
    assert_eq!(painted_at(&h, 400, 300)[3], 255);
}

#[test]
fn the_eraser_takes_paint_away() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Brush;
    editor.smoothing = 0.0;
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 300.0, 500.0), screen(&h, 900.0, 500.0));
    drag(&mut h, from, &[to]);
    h.state_mut().1.tool = Tool::Eraser;
    let (from, to) = (screen(&h, 600.0, 300.0), screen(&h, 600.0, 700.0));
    drag(&mut h, from, &[to]);
    assert_eq!(painted_at(&h, 600, 500), [0; 4]);
    assert_eq!(painted_at(&h, 400, 500)[3], 255);
}

#[test]
fn the_eyedropper_picks_up_a_colour_from_the_stage() {
    let (mut editor, _) = editor_with_selected_box();
    editor.tool = Tool::Eyedropper;
    let fill = editor.draw_style.fill.unwrap();
    let mut h = harness(editor);
    let at = screen(&h, 400.0, 400.0);
    click(&mut h, at);
    assert_eq!(h.state().1.paint_color, fill);
}

#[test]
fn a_new_drawing_on_a_paint_layer_is_a_blank_canvas_of_its_own() {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Brush;
    let mut h = harness(editor);
    let at = screen(&h, 400.0, 300.0);
    click(&mut h, at);
    let editor = &mut h.state_mut().1;
    let (_, first, ..) = editor.paint_target().unwrap();
    editor.set_frame(5);
    editor.new_drawing(false);
    let (_, second, ..) = editor.paint_target().unwrap();
    assert_ne!(first, second);
    assert!(editor.doc.canvas(second).unwrap().is_empty());
    assert!(!editor.doc.canvas(first).unwrap().is_empty());
}

#[test]
fn the_bucket_fills_an_empty_paint_layer() {
    let mut editor = Editor::new(Document::default());
    editor.use_tool(Tool::PaintBucket);
    editor.paint_color = Color::rgb(0, 120, 0);
    let mut h = harness(editor);
    let at = screen(&h, 500.0, 500.0);
    click(&mut h, at);
    assert_eq!(painted_at(&h, 10, 10), [0, 120, 0, 255]);
    assert_eq!(painted_at(&h, 1900, 1070), [0, 120, 0, 255]);
}

#[test]
fn dragging_the_gradient_blends_between_the_colours() {
    let mut editor = Editor::new(Document::default());
    editor.use_tool(Tool::Gradient);
    editor.paint_color = Color::rgb(0, 0, 0);
    editor.second_color = Color::rgb(255, 255, 255);
    let mut h = harness(editor);
    let (from, middle, to) = (screen(&h, 400.0, 500.0), screen(&h, 900.0, 500.0), screen(&h, 1400.0, 500.0));
    drag(&mut h, from, &[middle, to]);
    assert_eq!(painted_at(&h, 300, 100), [0, 0, 0, 255], "black before the start");
    assert_eq!(painted_at(&h, 1500, 900), [255, 255, 255, 255], "white past the end");
    let grey = painted_at(&h, 900, 700)[0];
    assert!((110..=145).contains(&grey), "grey in the middle: {grey}");
}

#[test]
fn the_clone_stamp_copies_from_where_it_was_alt_clicked() {
    let mut editor = Editor::new(Document::default());
    editor.use_tool(Tool::Brush);
    editor.smoothing = 0.0;
    editor.paint_color = Color::rgb(0, 0, 200);
    let mut h = harness(editor);
    // A blue dot to copy.
    let dot = screen(&h, 300.0, 300.0);
    click(&mut h, dot);
    h.state_mut().1.use_tool(Tool::CloneStamp);
    h.state_mut().1.clone.brush.hardness = 1.0;
    hold_alt(&mut h, true);
    click(&mut h, dot);
    hold_alt(&mut h, false);
    assert!(h.state().1.clone_source.is_some());
    // Stamping 400 pixels to the right copies the dot there.
    let there = screen(&h, 700.0, 300.0);
    click(&mut h, there);
    assert_eq!(painted_at(&h, 700, 300), [0, 0, 200, 255]);
}

#[test]
fn smudging_pulls_paint_along() {
    let mut editor = Editor::new(Document::default());
    editor.use_tool(Tool::Brush);
    let mut h = harness(editor);
    let dot = screen(&h, 500.0, 500.0);
    click(&mut h, dot);
    h.state_mut().1.use_tool(Tool::Smudge);
    h.state_mut().1.smoothing = 0.0;
    let to = screen(&h, 600.0, 500.0);
    drag(&mut h, dot, &[dot + vec2(10.0, 0.0), to]);
    assert!(painted_at(&h, 540, 500)[3] > 30, "paint dragged out to the right");
}

/// An editor with an empty paint layer selected and `tool` picked.
fn painting_with(tool: Tool) -> Harness<'static, State> {
    let mut editor = Editor::new(Document::default());
    editor.add_paint_layer();
    editor.use_tool(tool);
    editor.smoothing = 0.0;
    harness(editor)
}

/// The selection on the paint layer, at stage pixel (`x`, `y`).
fn selected_at(harness: &Harness<'_, State>, x: i64, y: i64) -> u8 {
    harness.state().1.active_selection().map_or(0, |(_, s)| s.at(x, y))
}

#[test]
fn symmetrys_centre_drags_by_its_handle_and_strokes_mirror_round_it() {
    let mut h = painting_with(Tool::Brush);
    h.state_mut().1.symmetry = inkframe_core::Symmetry::Vertical;
    let (w, height) = (h.state().1.doc.width as f32, h.state().1.doc.height as f32);
    let (handle, to) = (screen(&h, w / 2.0, height / 2.0), screen(&h, w / 2.0 + 200.0, height / 2.0));
    drag(&mut h, handle, &[handle + vec2(5.0, 0.0), to]);
    let (x, _) = h.state().1.symmetry_centre.expect("moved");
    assert!((x - (w / 2.0 + 200.0)).abs() < 3.0, "{x}");
    assert_eq!(
        painted_at(&h, (w / 2.0 + 100.0) as i64, (height / 2.0) as i64),
        [0; 4],
        "dragging the handle paints nothing"
    );

    // A dot 150 px left of the centre is mirrored 150 px right of it.
    let centre = w / 2.0 + 200.0;
    let dot = screen(&h, centre - 150.0, 200.0);
    click(&mut h, dot);
    assert_eq!(painted_at(&h, (centre + 150.0) as i64, 200)[3], 255);
}

#[test]
fn a_marquee_selection_keeps_painting_inside_it() {
    let mut h = painting_with(Tool::RectMarquee);
    let (from, to) = (screen(&h, 200.0, 200.0), screen(&h, 600.0, 600.0));
    drag(&mut h, from, &[from + vec2(10.0, 10.0), to]);
    assert_eq!(selected_at(&h, 400, 400), 255);
    assert_eq!(selected_at(&h, 700, 400), 0);

    h.state_mut().1.use_tool(Tool::Brush);
    let (from, to) = (screen(&h, 100.0, 400.0), screen(&h, 900.0, 400.0));
    drag(&mut h, from, &[from + vec2(10.0, 0.0), to]);
    assert_eq!(painted_at(&h, 400, 400)[3], 255, "painted inside");
    assert_eq!(painted_at(&h, 800, 400), [0; 4], "not outside");
}

#[test]
fn shift_adds_to_a_selection() {
    let mut h = painting_with(Tool::RectMarquee);
    let (a, b) = (screen(&h, 100.0, 100.0), screen(&h, 200.0, 200.0));
    drag(&mut h, a, &[a + vec2(5.0, 5.0), b]);
    hold_shift(&mut h, true);
    let (c, d) = (screen(&h, 300.0, 100.0), screen(&h, 400.0, 200.0));
    drag(&mut h, c, &[c + vec2(5.0, 5.0), d]);
    hold_shift(&mut h, false);
    assert_eq!(selected_at(&h, 150, 150), 255);
    assert_eq!(selected_at(&h, 350, 150), 255);
    assert_eq!(selected_at(&h, 250, 150), 0);
}

#[test]
fn the_polygonal_lasso_closes_on_its_first_corner() {
    let mut h = painting_with(Tool::PolygonLasso);
    let corners = [(200.0, 200.0), (600.0, 200.0), (400.0, 600.0), (200.0, 200.0)];
    for (x, y) in corners {
        let at = screen(&h, x, y);
        click(&mut h, at);
        // Far enough apart in time not to count as a double-click.
        for _ in 0..30 {
            h.step();
        }
    }
    assert_eq!(selected_at(&h, 400, 300), 255);
    assert_eq!(selected_at(&h, 250, 500), 0, "outside the triangle");
}

#[test]
fn the_magic_wand_selects_an_area_of_colour() {
    let mut h = painting_with(Tool::Brush);
    let dot = screen(&h, 500.0, 500.0);
    click(&mut h, dot);
    h.state_mut().1.use_tool(Tool::MagicWand);
    click(&mut h, dot);
    assert_eq!(selected_at(&h, 500, 500), 255);
    assert_eq!(selected_at(&h, 900, 500), 0);
}

#[test]
fn dragging_inside_a_selection_moves_its_pixels() {
    let mut h = painting_with(Tool::Brush);
    let dot = screen(&h, 500.0, 500.0);
    click(&mut h, dot);
    h.state_mut().1.use_tool(Tool::RectMarquee);
    let (a, b) = (screen(&h, 450.0, 450.0), screen(&h, 550.0, 550.0));
    drag(&mut h, a, &[a + vec2(5.0, 5.0), b]);
    h.state_mut().1.use_tool(Tool::Select);
    let to = screen(&h, 700.0, 500.0);
    drag(&mut h, dot, &[dot + vec2(10.0, 0.0), to]);
    assert_eq!(painted_at(&h, 500, 500), [0; 4], "lifted from where it was");
    assert_eq!(painted_at(&h, 700, 500)[3], 255, "put down where it was dragged");
    assert_eq!(selected_at(&h, 700, 500), 255, "the selection came too");
}

#[test]
fn delete_clears_the_selected_pixels() {
    let mut h = painting_with(Tool::Brush);
    let (from, to) = (screen(&h, 100.0, 400.0), screen(&h, 900.0, 400.0));
    drag(&mut h, from, &[from + vec2(10.0, 0.0), to]);
    let editor = &mut h.state_mut().1;
    let (_, canvas, w, hgt) = editor.paint_target().unwrap();
    let selection = Selection::rectangle(w, hgt, Point::new(0.0, 0.0), Point::new(500.0, 1080.0));
    editor.pixel_selection = Some((canvas, selection));
    assert!(editor.clear_selected_pixels());
    assert_eq!(painted_at(&h, 300, 400), [0; 4]);
    assert_eq!(painted_at(&h, 700, 400)[3], 255);
    assert_eq!(h.state().1.layers().len(), 1, "the layer stays");
}

fn hold_shift(harness: &mut Harness<'_, State>, held: bool) {
    let modifiers = egui::Modifiers { shift: held, ..egui::Modifiers::NONE };
    harness.event(egui::Event::ModifiersChanged(modifiers));
    harness.step();
}

#[test]
fn painting_on_a_turned_view_lands_where_the_pen_is() {
    let mut h = painting_with(Tool::Brush);
    h.state_mut().0.rotate_view(90.0);
    h.run();
    let (from, to) = (screen(&h, 300.0, 500.0), screen(&h, 900.0, 500.0));
    // The stage's x axis now runs down the screen.
    assert!((from.x - to.x).abs() < 0.5 && to.y > from.y, "{from:?} {to:?}");
    drag(&mut h, from, &[from + vec2(0.0, 10.0), to]);
    for x in [300, 600, 900] {
        assert_eq!(painted_at(&h, x, 500)[3], 255, "at x = {x}");
    }
    h.state_mut().0.reset_rotation();
    h.run();
    let upright = screen(&h, 300.0, 500.0);
    assert!(upright.y < screen(&h, 300.0, 600.0).y);
}

#[test]
fn turning_the_parent_carries_the_child() {
    let (mut editor, parent, child) = parent_and_child();
    editor.set_pose(parent, Transform { rotation: 180.0, ..Transform::at(Point::new(400.0, 400.0)) });
    let h = harness(editor);
    // A further quarter turn about (400, 400) takes the hand from the
    // right of the box to below it.
    let at = on_stage(&h, child, Point::ZERO);
    assert!(at.distance(Point::new(400.0, 700.0)) < 0.5, "child at {at:?}");
}

#[test]
fn clicking_a_handle_keeps_the_selection() {
    let (editor, id) = editor_with_selected_box();
    let mut h = harness(editor);

    // The edge handle sits on the outline; clicking slightly outside the
    // shape would normally deselect.
    let handle = screen(&h, 500.0, 400.0) + vec2(3.0, 0.0);
    h.drag_at(handle);
    h.step();
    h.drop_at(handle);
    h.run();

    assert_eq!(h.state().1.selection, Some(id));
}

fn click(harness: &mut Harness<'_, State>, at: Pos2) {
    harness.drag_at(at);
    harness.step();
    harness.drop_at(at);
    harness.run();
}

fn pen_harness() -> Harness<'static, State> {
    let mut editor = Editor::new(Document::default());
    editor.tool = Tool::Pen;
    harness(editor)
}

#[test]
fn pen_clicks_and_closing_make_a_filled_shape() {
    let mut h = pen_harness();
    let corners = [(200.0, 200.0), (600.0, 200.0), (400.0, 500.0)];
    for (x, y) in corners {
        let at = screen(&h, x, y);
        click(&mut h, at);
    }
    assert!(h.state().0.is_drawing_path());
    let first = screen(&h, 200.0, 200.0);
    click(&mut h, first);

    let (stage, editor) = h.state();
    assert!(!stage.is_drawing_path());
    let element = &editor.doc.layers[0].drawings[0].elements[0];
    let Shape::Path(path) = &element.shape else { panic!("not a path") };
    assert!(path.closed);
    assert_eq!(path.segments.len(), 3);
    assert!(element.style.fill.is_some(), "closed shapes are filled");
}

#[test]
fn pen_drag_makes_a_curve() {
    let mut h = pen_harness();
    let start = screen(&h, 200.0, 400.0);
    click(&mut h, start);

    // Press at the second point and drag out a handle before releasing.
    let second = screen(&h, 600.0, 400.0);
    h.drag_at(second);
    h.step();
    h.hover_at(second + vec2(60.0, -60.0));
    h.step();
    h.drop_at(second + vec2(60.0, -60.0));
    h.run();

    let (stage, editor) = h.state_mut();
    stage.finish_path(editor, false);
    let element = &editor.doc.layers[0].drawings[0].elements[0];
    let Shape::Path(path) = &element.shape else { panic!("not a path") };
    assert!(matches!(path.segments[0], inkframe_core::Segment::Cubic { .. }));
    assert!(element.style.fill.is_none(), "open paths are only stroked");
}

#[test]
fn switching_tools_finishes_the_path() {
    let mut h = pen_harness();
    for x in [200.0, 600.0] {
        let at = screen(&h, x, 300.0);
        click(&mut h, at);
    }
    h.state_mut().1.tool = Tool::Select;
    h.run();

    let (stage, editor) = h.state();
    assert!(!stage.is_drawing_path());
    assert_eq!(editor.doc.layers.len(), 1);
}

/// The selected box from `editor_with_selected_box`, with the
/// Subselection tool active.
fn subselect_harness() -> (Harness<'static, State>, LayerId) {
    let (mut editor, id) = editor_with_selected_box();
    editor.tool = Tool::Subselect;
    (harness(editor), id)
}

fn shape(harness: &Harness<'_, State>, id: LayerId) -> Shape {
    harness.state().1.doc.layer(id).unwrap().drawings[0].elements[0].shape.clone()
}

#[test]
fn clicking_an_anchor_selects_it_without_changing_the_shape() {
    let (mut h, id) = subselect_harness();
    let corner = screen(&h, 500.0, 500.0);
    click(&mut h, corner);

    assert!(h.state().0.has_anchor_selection());
    assert!(matches!(shape(&h, id), Shape::Rect { .. }), "a click must not convert the rectangle");
}

#[test]
fn dragging_a_rect_corner_turns_it_into_a_path() {
    let (mut h, id) = subselect_harness();
    let corner = screen(&h, 500.0, 500.0);
    let target = screen(&h, 560.0, 530.0);
    drag(&mut h, corner, &[corner + vec2(5.0, 5.0), target]);

    let Shape::Path(path) = shape(&h, id) else { panic!("rectangle should now be a path") };
    let (anchors, closed) = Shape::Path(path).to_anchors();
    assert!(closed);
    assert_eq!(anchors.len(), 4);
    // The box is centered on (400, 400), so the corner's local position
    // moved from (100, 100) to about (160, 130). The others stay put.
    assert!(anchors[2].point.distance(Point::new(160.0, 130.0)) < 3.0, "{:?}", anchors[2].point);
    assert_eq!(anchors[0].point, Point::new(-100.0, -100.0));
}

#[test]
fn dragging_a_handle_reshapes_the_curve_smoothly() {
    let (mut editor, id) = editor_with_selected_box();
    editor.doc.layer_mut(id).unwrap().drawings[0].elements[0].shape =
        Shape::Ellipse { radius_x: 100.0, radius_y: 100.0 };
    editor.tool = Tool::Subselect;
    let mut h = harness(editor);

    // Select the top anchor so its handles appear, then drag the
    // outgoing handle (which starts at local (55.2, -100)) further right.
    let top = screen(&h, 400.0, 300.0);
    click(&mut h, top);
    let handle = screen(&h, 400.0 + 100.0 * inkframe_core::KAPPA, 300.0);
    let target = screen(&h, 520.0, 260.0);
    drag(&mut h, handle, &[handle + vec2(5.0, 0.0), target]);

    let (anchors, _) = shape(&h, id).to_anchors();
    let top = anchors[0];
    assert!(top.handle_out.distance(Point::new(120.0, -140.0)) < 3.0, "{:?}", top.handle_out);
    // The incoming handle swung round to stay opposite.
    let (into, out) = (top.handle_in - top.point, top.handle_out - top.point);
    assert!(into.dot(out) / (into.length() * out.length()) < -0.999);
}

#[test]
fn delete_removes_the_selected_anchor() {
    let (mut h, id) = subselect_harness();
    let corner = screen(&h, 500.0, 500.0);
    click(&mut h, corner);
    let (stage, editor) = h.state_mut();
    stage.delete_selected_anchor(editor);

    assert_eq!(shape(&h, id).to_anchors().0.len(), 3);
    assert!(!h.state().0.has_anchor_selection());
}

/// The selected box from `editor_with_selected_box`, with the Pen tool.
fn pen_on_box_harness() -> (Harness<'static, State>, LayerId) {
    let (mut editor, id) = editor_with_selected_box();
    editor.tool = Tool::Pen;
    (harness(editor), id)
}

#[test]
fn pen_click_on_the_outline_adds_a_point() {
    let (mut h, id) = pen_on_box_harness();
    // Middle of the top edge: on the outline, away from the corners.
    let edge = screen(&h, 400.0, 300.0);
    click(&mut h, edge);

    let (stage, editor) = h.state();
    assert!(!stage.is_drawing_path(), "the click shouldn't start a new path");
    assert_eq!(editor.doc.layers.len(), 1);
    let (anchors, closed) = shape(&h, id).to_anchors();
    assert!(closed);
    assert_eq!(anchors.len(), 5);
    assert!(anchors[1].point.distance(Point::new(0.0, -100.0)) < 2.0, "{:?}", anchors[1].point);
}

#[test]
fn pen_click_away_from_the_outline_starts_a_new_path() {
    let (mut h, id) = pen_on_box_harness();
    let elsewhere = screen(&h, 1200.0, 700.0);
    click(&mut h, elsewhere);

    assert!(h.state().0.is_drawing_path());
    assert!(matches!(shape(&h, id), Shape::Rect { .. }));
}

#[test]
fn pen_click_on_an_existing_point_does_not_add_another() {
    let (mut h, id) = pen_on_box_harness();
    let corner = screen(&h, 300.0, 300.0);
    click(&mut h, corner);

    assert!(matches!(shape(&h, id), Shape::Rect { .. }), "no point should be added on top of a corner");
    let (stage, _) = h.state();
    assert!(!stage.is_drawing_path(), "nor a new path started there");
    assert!(stage.has_anchor_selection(), "the point is picked, showing its handles");
}

/// Drags with Alt/Option held the whole way.
fn alt_drag(harness: &mut Harness<'_, State>, from: Pos2, path: &[Pos2]) {
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::ALT));
    drag(harness, from, path);
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run();
}

#[test]
fn the_pen_drags_existing_points_between_paths() {
    let (mut h, id) = pen_on_box_harness();
    let corner = screen(&h, 500.0, 500.0);
    let target = screen(&h, 540.0, 560.0);
    drag(&mut h, corner, &[corner + vec2(5.0, 5.0), target]);

    assert!(!h.state().0.is_drawing_path());
    let (anchors, _) = shape(&h, id).to_anchors();
    assert!(anchors[2].point.distance(Point::new(140.0, 160.0)) < 3.0, "{:?}", anchors[2].point);
}

#[test]
fn alt_dragging_a_corner_with_the_pen_pulls_out_curve_handles() {
    let (mut h, id) = pen_on_box_harness();
    let corner = screen(&h, 500.0, 500.0);
    let target = screen(&h, 560.0, 470.0);
    alt_drag(&mut h, corner, &[corner + vec2(5.0, -2.0), target]);

    let (anchors, _) = shape(&h, id).to_anchors();
    let a = anchors[2];
    assert!(a.point.distance(Point::new(100.0, 100.0)) < 1e-3, "the point stays put: {a:?}");
    assert!(a.handle_out.distance(Point::new(160.0, 70.0)) < 3.0, "{a:?}");
    assert!(a.handle_in.distance(Point::new(40.0, 130.0)) < 3.0, "mirrored: {a:?}");
    assert!(h.state().0.has_anchor_selection());
}

#[test]
fn alt_dragging_a_point_with_the_subselection_tool_pulls_handles_too() {
    let (mut h, id) = subselect_harness();
    let corner = screen(&h, 300.0, 300.0);
    let target = screen(&h, 300.0, 240.0);
    alt_drag(&mut h, corner, &[corner + vec2(0.0, -5.0), target]);

    let (anchors, _) = shape(&h, id).to_anchors();
    let a = anchors[0];
    assert!(a.point.distance(Point::new(-100.0, -100.0)) < 1e-3);
    assert!(a.handle_out.distance(Point::new(-100.0, -160.0)) < 3.0, "{a:?}");
    assert!(a.handle_in.distance(Point::new(-100.0, -40.0)) < 3.0, "{a:?}");
}

#[test]
fn with_a_point_picked_the_pen_drags_its_handles() {
    let (mut editor, id) = editor_with_selected_box();
    editor.doc.layer_mut(id).unwrap().drawings[0].elements[0].shape =
        Shape::Ellipse { radius_x: 100.0, radius_y: 100.0 };
    editor.tool = Tool::Pen;
    let mut h = harness(editor);

    let top = screen(&h, 400.0, 300.0);
    click(&mut h, top);
    let handle = screen(&h, 400.0 + 100.0 * inkframe_core::KAPPA, 300.0);
    let target = screen(&h, 520.0, 260.0);
    drag(&mut h, handle, &[handle + vec2(5.0, 0.0), target]);

    assert!(!h.state().0.is_drawing_path());
    let (anchors, _) = shape(&h, id).to_anchors();
    assert!(anchors[0].handle_out.distance(Point::new(120.0, -140.0)) < 3.0, "{:?}", anchors[0].handle_out);
}

#[test]
fn starting_a_new_path_lets_go_of_the_picked_point() {
    let (mut h, _) = pen_on_box_harness();
    let (corner, elsewhere) = (screen(&h, 300.0, 300.0), screen(&h, 1200.0, 700.0));
    click(&mut h, corner);
    assert!(h.state().0.has_anchor_selection());
    click(&mut h, elsewhere);
    let (stage, _) = h.state();
    assert!(stage.is_drawing_path());
    assert!(!stage.has_anchor_selection());
}

/// Clicks with Alt/Option held for the whole click.
fn alt_click(harness: &mut Harness<'_, State>, at: Pos2) {
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::ALT));
    click(harness, at);
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run();
}

#[test]
fn alt_click_toggles_a_point_between_corner_and_smooth() {
    let (mut h, id) = subselect_harness();
    let corner = screen(&h, 500.0, 500.0);

    alt_click(&mut h, corner);
    let (anchors, _) = shape(&h, id).to_anchors();
    let a = anchors[2];
    assert!(a.handle_in != a.point && a.handle_out != a.point, "should now be smooth: {a:?}");

    alt_click(&mut h, corner);
    let (anchors, _) = shape(&h, id).to_anchors();
    assert_eq!(anchors[2], Anchor::corner(Point::new(100.0, 100.0)));
}

#[test]
fn plain_click_does_not_convert() {
    let (mut h, id) = subselect_harness();
    let corner = screen(&h, 500.0, 500.0);
    click(&mut h, corner);
    assert!(matches!(shape(&h, id), Shape::Rect { .. }));
}

/// Two quick clicks in place. Unlike `click`, the pointer stays over the
/// stage in between, as it does for a real double-click.
fn double_click(harness: &mut Harness<'_, State>, at: Pos2) {
    harness.hover_at(at);
    harness.step();
    for pressed in [true, false, true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    harness.run();
}

#[test]
fn double_clicking_an_instance_opens_its_symbol() {
    let (mut editor, id) = editor_with_selected_box();
    let symbol = editor.convert_selected_to_symbol().unwrap();
    let mut h = harness(editor);

    let on_box = screen(&h, 400.0, 400.0);
    double_click(&mut h, on_box);

    let editor = &h.state().1;
    assert_eq!(editor.timeline(), TimelineId::Symbol(symbol));
    assert_ne!(editor.selection, Some(id), "selection moves into the symbol");
}

/// A layer at (400, 400) holding two 80×80 squares, 100 to its left and
/// 100 to its right.
fn editor_with_two_squares() -> (Editor, LayerId) {
    let mut editor = Editor::new(Document::default());
    let square = |x: f32| {
        Element {
            shape: Shape::Rect { width: 80.0, height: 80.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }
        .transformed(Affine::translate(Point::new(x, 0.0)))
        .unwrap()
    };
    let id = editor.doc.add_layer(
        "Shapes",
        vec![square(-100.0), square(100.0)],
        0,
        Transform::at(Point::new(400.0, 400.0)),
    );
    (editor, id)
}

#[test]
fn double_clicking_a_layer_steps_inside_to_move_one_shape() {
    let (editor, id) = editor_with_two_squares();
    let mut h = harness(editor);
    let on_square = screen(&h, 500.0, 400.0);
    double_click(&mut h, on_square);
    assert_eq!(h.state().1.inside_layer(), Some(id));

    // Drag the right-hand square down: only it moves, and the layer
    // itself isn't moved or keyed.
    let (from, to) = (screen(&h, 500.0, 400.0), screen(&h, 500.0, 460.0));
    drag(&mut h, from, &[to]);
    let editor = &h.state().1;
    assert_eq!(editor.picked_shapes(), &[ShapeRef::line(1)]);
    let layer = editor.doc.layer(id).unwrap();
    let shapes = layer.elements_at(0);
    assert!(shapes[1].bounds().center().distance(Point::new(100.0, 60.0)) < 1.5, "{:?}", shapes[1].bounds());
    assert!(shapes[0].bounds().center().distance(Point::new(-100.0, 0.0)) < 1e-3);
    assert_eq!(layer.track.position.keys().len(), 1);
}

#[test]
fn inside_a_layer_a_box_picks_shapes_and_double_clicking_empty_stage_steps_out() {
    let (mut editor, id) = editor_with_two_squares();
    editor.enter_layer(id);
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 250.0, 300.0), screen(&h, 560.0, 500.0));
    drag(&mut h, from, &[to]);
    assert_eq!(h.state().1.picked_shapes(), &[ShapeRef::line(0), ShapeRef::line(1)]);

    let empty = screen(&h, 900.0, 800.0);
    double_click(&mut h, empty);
    assert_eq!(h.state().1.inside_layer(), None);
}

#[test]
fn the_knob_above_picked_shapes_turns_them() {
    let (mut editor, id) = editor_with_two_squares();
    editor.enter_layer(id);
    editor.pick_shapes(vec![ShapeRef::line(1)]);
    let mut h = harness(editor);
    // The knob is above the right-hand square's top edge (the square is
    // 360..440 high on the stage); drag it round to its right side, a
    // quarter turn about the square's middle at (500, 400).
    let knob = {
        let view = h.state().0.view(h.state().0.last_area, &h.state().1.doc);
        super::inside::rotate_knob(&h.state().1, view).unwrap().0
    };
    let right = screen(&h, 640.0, 400.0);
    drag(&mut h, knob, &[right]);

    let editor = &h.state().1;
    let square = &editor.doc.layer(id).unwrap().elements_at(0)[1];
    let Shape::Path(path) = &square.shape else { panic!("turned into a path") };
    // A square turned a quarter keeps its box, and its first corner
    // (top-left) has gone round to the top-right.
    let bounds = square.bounds();
    assert!(bounds.center().distance(Point::new(100.0, 0.0)) < 0.5, "{bounds:?}");
    assert!(path.start.distance(Point::new(140.0, -40.0)) < 1.0, "{:?}", path.start);
}

#[test]
fn with_edit_multiple_frames_dragging_moves_the_whole_motion() {
    let (mut editor, id) = editor_with_selected_box();
    // It travels 300 to the right by frame 10.
    editor.doc.layer_mut(id).unwrap().track.position.set(10, Point::new(700.0, 400.0));
    editor.edit_multiple = true;
    let mut h = harness(editor);
    let (from, to) = (screen(&h, 400.0, 400.0), screen(&h, 400.0, 450.0));
    drag(&mut h, from, &[to]);

    let keys: Vec<Point> =
        h.state().1.doc.layer(id).unwrap().track.position.keys().iter().map(|k| k.value).collect();
    assert!(keys[0].distance(Point::new(400.0, 450.0)) < 1.5, "{keys:?}");
    assert!(keys[1].distance(Point::new(700.0, 450.0)) < 1.5, "the end of the move came along: {keys:?}");
}

#[test]
fn a_picked_shapes_gradient_ends_are_dragged_on_the_stage() {
    let (mut editor, id) = editor_with_two_squares();
    // The right square (80×80 around (500, 400) on the stage) fades
    // left to right.
    let gradient = inkframe_core::ShapeGradient::linear(Color::BLACK, Color::WHITE);
    editor.doc.layer_mut(id).unwrap().drawings[0].elements[1].style.gradient = Some(gradient);
    editor.enter_layer(id);
    editor.pick_shapes(vec![ShapeRef::line(1)]);
    let mut h = harness(editor);

    // Its end handle is at the middle of the right side, (540, 400);
    // drag it down to the bottom right corner.
    let (from, to) = (screen(&h, 540.0, 400.0), screen(&h, 540.0, 440.0));
    drag(&mut h, from, &[to]);
    let editor = &h.state().1;
    let square = &editor.doc.layer(id).unwrap().drawings[0].elements[1];
    let gradient = square.style.gradient.as_ref().unwrap();
    assert!(gradient.to.distance(Point::new(1.0, 1.0)) < 0.03, "{:?}", gradient.to);
    assert!(gradient.from.distance(Point::new(0.0, 0.5)) < 1e-6, "the start stays");
    assert!(square.bounds().center().distance(Point::new(100.0, 0.0)) < 1e-3, "the shape didn't move");
}

#[test]
fn a_guide_is_dragged_out_of_the_ruler_and_back_into_it() {
    let mut editor = Editor::new(Document::default());
    editor.show_rulers = true;
    let mut h = harness(editor);
    let area = h.state().0.last_area;
    // From the top ruler down to stage y 300.
    let on_ruler = pos2(screen(&h, 500.0, 0.0).x, area.top() + 8.0);
    let at = screen(&h, 500.0, 300.0);
    drag(&mut h, on_ruler, &[at]);
    let guides = h.state().1.doc.ruler_guides.clone();
    assert_eq!(guides.len(), 1);
    assert!(!guides[0].vertical);
    assert!((guides[0].at - 300.0).abs() <= 1.0, "{}", guides[0].at);

    // Dragged back up onto the ruler, it goes.
    let on_guide = screen(&h, 800.0, guides[0].at);
    drag(&mut h, on_guide, &[on_ruler]);
    assert!(h.state().1.doc.ruler_guides.is_empty());
}

#[test]
fn moving_a_layer_near_a_guide_snaps_it_on() {
    // The box is 200 wide around x 400, so its left edge is at 300.
    let (mut editor, id) = editor_with_selected_box();
    editor.doc.ruler_guides.push(inkframe_core::RulerGuide { vertical: true, at: 250.0 });
    let mut h = harness(editor);
    // 48 left (left edge to 252), close enough to snap the last 2.
    let (from, to) = (screen(&h, 400.0, 400.0), screen(&h, 352.0, 400.0));
    drag(&mut h, from, &[to]);
    let position = h.state().1.doc.layer(id).unwrap().transform_at(0).position;
    assert!((position.x - 350.0).abs() < 0.01, "{position:?}");
}

#[test]
fn drawing_inside_a_symbol_uses_its_coordinates() {
    let (mut editor, _) = editor_with_selected_box();
    let symbol = editor.convert_selected_to_symbol().unwrap();
    editor.enter_symbol(symbol, 0);
    editor.tool = Tool::Rectangle;
    let mut h = harness(editor);

    // `screen` maps stage points, but while editing a symbol the tools
    // work in symbol coordinates, with (0, 0) at the stage's center.
    let center = Document::default().stage_rect().center();
    let (from, to) =
        (screen(&h, center.x - 50.0, center.y - 30.0), screen(&h, center.x + 50.0, center.y + 30.0));
    drag(&mut h, from, &[to]);

    let editor = &h.state().1;
    let layers = editor.doc.layers_of(TimelineId::Symbol(symbol));
    assert_eq!(layers.len(), 2, "the rectangle lands in the symbol");
    let drawn = layers.last().unwrap();
    assert!(
        drawn.transform_at(0).position.distance(Point::ZERO) < 2.0,
        "{:?}",
        drawn.transform_at(0).position
    );
}

#[test]
fn drawing_in_place_inside_a_rotated_instance() {
    let (mut editor, id) = editor_with_selected_box();
    let symbol = editor.convert_selected_to_symbol().unwrap();
    // Turn the instance a quarter turn: the symbol's x axis now points
    // down the screen.
    let pose = Transform { rotation: 90.0, ..editor.doc.layer(id).unwrap().transform_at(0) };
    editor.doc.layer_mut(id).unwrap().track.set_pose(0, pose);
    editor.enter_instance(id);
    editor.tool = Tool::Rectangle;
    let mut h = harness(editor);

    // `screen` takes stage points, which here we work out by hand: drag
    // from the registration point (400, 400) to 30 right and 60 down.
    let to_screen = |h: &Harness<'_, State>, x: f32, y: f32| {
        let (stage, editor) = h.state();
        stage.view(stage.last_area, &editor.doc).to_screen(Point::new(x, y))
    };
    let (from, to) = (to_screen(&h, 400.0, 400.0), to_screen(&h, 430.0, 460.0));
    drag(&mut h, from, &[to]);

    // Undoing the quarter turn, that drag runs 60 along the symbol's x
    // axis and 30 against its y axis: a 60×30 box centered at (30, -15).
    let layers = h.state().1.doc.layers_of(TimelineId::Symbol(symbol));
    let drawn = layers.last().unwrap();
    let Shape::Rect { width, height, .. } = drawn.drawings[0].elements[0].shape else { panic!("not a rect") };
    assert!((width - 60.0).abs() < 1.5 && (height - 30.0).abs() < 1.5, "{width}×{height}");
    let center = drawn.transform_at(0).position;
    assert!(center.distance(Point::new(30.0, -15.0)) < 1.5, "{center:?}");
}

#[test]
fn zooming_keeps_the_anchor_fixed() {
    let doc = Document::default();
    let mut stage =
        StageView { last_area: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)), ..Default::default() };
    let anchor = pos2(200.0, 150.0);
    let before = stage.view(stage.last_area, &doc).to_local(anchor);
    stage.set_zoom_about(&doc, anchor, 2.0);
    let after = stage.view(stage.last_area, &doc).to_local(anchor);
    assert!(before.distance(after) < 1e-3);
}

/// A box layer in a new, empty skeleton, the skeleton selected and the
/// Add Bone tool picked.
fn skeleton_harness() -> (Harness<'static, State>, LayerId, LayerId) {
    let (mut editor, layer) = editor_with_selected_box();
    editor.new_skeleton();
    let skeleton = editor.active_skeleton().unwrap();
    (harness(editor), skeleton, layer)
}

fn bones(h: &Harness<'_, State>, skeleton: LayerId) -> Vec<inkframe_core::Bone> {
    h.state().1.skeleton(skeleton).unwrap().bones.clone()
}

#[test]
fn add_bone_drags_draw_a_chain() {
    let (mut h, skeleton, _) = skeleton_harness();
    let (a, b, c) = (screen(&h, 300.0, 400.0), screen(&h, 400.0, 400.0), screen(&h, 500.0, 400.0));
    drag(&mut h, a, &[a + vec2(5.0, 0.0), b]);
    // Starting on the new bone's end joins the next one there.
    drag(&mut h, b + vec2(2.0, 1.0), &[b + vec2(10.0, 0.0), c]);

    let bones = bones(&h, skeleton);
    assert_eq!(bones.len(), 2);
    assert!(bones[0].start.distance(Point::new(300.0, 400.0)) < 2.0, "{:?}", bones[0].start);
    assert_eq!(bones[1].parent, Some(0));
    assert_eq!(bones[1].start, bones[0].end, "joined at the elbow");
    assert!(bones[1].end.distance(Point::new(500.0, 400.0)) < 2.0);
}

#[test]
fn clicking_away_with_add_bone_starts_a_new_root() {
    let (mut h, skeleton, _) = skeleton_harness();
    let (a, b) = (screen(&h, 300.0, 400.0), screen(&h, 400.0, 400.0));
    drag(&mut h, a, &[a + vec2(5.0, 0.0), b]);
    let away = screen(&h, 700.0, 100.0);
    click(&mut h, away);
    let (c, d) = (screen(&h, 300.0, 200.0), screen(&h, 300.0, 100.0));
    drag(&mut h, c, &[c + vec2(0.0, -5.0), d]);
    assert_eq!(bones(&h, skeleton)[1].parent, None);
}

#[test]
fn the_bone_tool_poses_bones_and_bends_the_box() {
    let (mut h, skeleton, layer) = skeleton_harness();
    let (a, b, c) = (screen(&h, 300.0, 400.0), screen(&h, 400.0, 400.0), screen(&h, 500.0, 400.0));
    drag(&mut h, a, &[a + vec2(5.0, 0.0), b]);
    drag(&mut h, b, &[b + vec2(10.0, 0.0), c]);
    {
        let (_, editor) = h.state_mut();
        editor.use_tool(Tool::Bone);
        editor.frame = 5;
    }
    h.run();
    // Grab the forearm near its end and pull it down below the elbow.
    let grab = screen(&h, 490.0, 400.0);
    let target = screen(&h, 400.0, 500.0);
    drag(&mut h, grab, &[grab + vec2(0.0, 10.0), target]);

    let (_, editor) = h.state();
    let (_, end) = editor.skeleton(skeleton).unwrap().posed_ends(1, 5).unwrap();
    assert!(end.distance(Point::new(400.0, 500.0)) < 5.0, "{end:?}");
    assert!(editor.doc.layer(skeleton).unwrap().key_frames().contains(&5));
    // The box (centred on 400, 400, 200 wide) bent down on its right:
    // its top right corner, mostly on the forearm, swung down most of
    // the way (the upper arm, a little further off, holds it back a
    // little, which is what rounds the bend).
    let layer = editor.doc.layer(layer).unwrap();
    let Shape::Path(path) = &editor.doc.posed_elements(layer, 5)[0].shape else { panic!("bent into a path") };
    let points = path.flatten();
    assert!(!points.iter().any(|p| p.distance(Point::new(100.0, -100.0)) < 8.0), "the corner moved");
    assert!(points.iter().any(|p| p.distance(Point::new(100.0, 60.0)) < 8.0), "down the right side");
}

#[test]
fn dragging_a_roots_start_moves_the_whole_skeleton() {
    let (mut h, skeleton, _) = skeleton_harness();
    let (a, b) = (screen(&h, 300.0, 400.0), screen(&h, 400.0, 400.0));
    drag(&mut h, a, &[a + vec2(5.0, 0.0), b]);
    h.state_mut().1.use_tool(Tool::Bone);
    h.run();
    let target = screen(&h, 350.0, 300.0);
    drag(&mut h, a, &[a + vec2(5.0, -5.0), target]);
    let (start, end) = h.state().1.skeleton(skeleton).unwrap().posed_ends(0, 0).unwrap();
    assert!(start.distance(Point::new(350.0, 300.0)) < 2.0, "{start:?}");
    assert!(end.distance(Point::new(450.0, 300.0)) < 2.0, "turned none: {end:?}");
}

#[test]
fn shift_dragging_a_bone_stretches_it() {
    let (mut h, skeleton, _) = skeleton_harness();
    let (a, b) = (screen(&h, 300.0, 400.0), screen(&h, 400.0, 400.0));
    drag(&mut h, a, &[a + vec2(5.0, 0.0), b]);
    h.state_mut().1.use_tool(Tool::Bone);
    h.run();
    // Grab it near its end with Shift and pull it half as long again.
    let grab = screen(&h, 390.0, 400.0);
    let target = screen(&h, 440.0, 400.0);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    drag(&mut h, grab, &[grab + vec2(5.0, 0.0), target]);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
    let (start, end) = h.state().1.skeleton(skeleton).unwrap().posed_ends(0, 0).unwrap();
    assert!(start.distance(Point::new(300.0, 400.0)) < 2.0, "{start:?}");
    assert!(end.distance(Point::new(440.0, 400.0)) < 3.0, "stretched, not turned: {end:?}");
}

/// The selected box, and a second box in the same drawing sharing its
/// right edge (x 100 in the layer, 500 on the stage), with the
/// Subselection tool.
fn welded_boxes_harness() -> (Harness<'static, State>, LayerId) {
    let (mut editor, id) = editor_with_selected_box();
    let mut other = editor.doc.layer(id).unwrap().elements_at(0)[0].clone();
    other = other.transformed(Affine::translate(Point::new(200.0, 0.0))).unwrap();
    editor.doc.layer_mut(id).unwrap().elements_at_mut(0).push(other);
    editor.tool = Tool::Subselect;
    (harness(editor), id)
}

fn corners(h: &Harness<'_, State>, id: LayerId, element: usize) -> Vec<Point> {
    let shape = &h.state().1.doc.layer(id).unwrap().drawings[0].elements[element].shape;
    shape.to_anchors().0.into_iter().map(|a| a.point).collect()
}

#[test]
fn dragging_a_welded_point_moves_both_shapes() {
    let (mut h, id) = welded_boxes_harness();
    let shared = screen(&h, 500.0, 300.0);
    let target = screen(&h, 530.0, 250.0);
    drag(&mut h, shared, &[shared + vec2(5.0, -5.0), target]);
    let moved = Point::new(130.0, -150.0);
    assert!(corners(&h, id, 0).iter().any(|p| p.distance(moved) < 3.0), "{:?}", corners(&h, id, 0));
    assert!(corners(&h, id, 1).iter().any(|p| p.distance(moved) < 3.0), "{:?}", corners(&h, id, 1));
    // Still exactly together.
    let a = corners(&h, id, 0).into_iter().find(|p| p.distance(moved) < 3.0).unwrap();
    assert!(corners(&h, id, 1).iter().any(|p| p.distance(a) < 1e-3));
}

#[test]
fn command_dragging_a_welded_point_parts_it() {
    let (mut h, id) = welded_boxes_harness();
    let shared = screen(&h, 500.0, 300.0);
    let target = screen(&h, 530.0, 250.0);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    drag(&mut h, shared, &[shared + vec2(5.0, -5.0), target]);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
    let moved = Point::new(130.0, -150.0);
    let both = [corners(&h, id, 0), corners(&h, id, 1)];
    let movers = both.iter().filter(|c| c.iter().any(|p| p.distance(moved) < 3.0)).count();
    assert_eq!(movers, 1, "only one shape's point moved");
}

#[test]
fn dropping_a_point_near_another_welds_them() {
    let (mut editor, id) = editor_with_selected_box();
    // A small box off to the right, its top left corner at (150, -100).
    let mut other = editor.doc.layer(id).unwrap().elements_at(0)[0].clone();
    other = other.transformed(Affine::scale(0.2, 0.2)).unwrap();
    other = other.transformed(Affine::translate(Point::new(170.0, -80.0))).unwrap();
    editor.doc.layer_mut(id).unwrap().elements_at_mut(0).push(other);
    editor.tool = Tool::Subselect;
    let mut h = harness(editor);
    // The big box's top right corner, dropped a little short of it.
    let corner = screen(&h, 500.0, 300.0);
    let near = screen(&h, 549.0, 301.0);
    drag(&mut h, corner, &[corner + vec2(5.0, 0.0), near]);
    assert!(
        corners(&h, id, 0).iter().any(|p| p.distance(Point::new(150.0, -100.0)) < 1e-3),
        "{:?}",
        corners(&h, id, 0)
    );
    assert_eq!(inkframe_core::welded_to(h.state().1.doc.layer(id).unwrap().elements_at(0), 1, 0).len(), 1);
}

/// A paint layer in a skeleton with an upper arm from (300, 500) to
/// (500, 500) and a forearm on to (700, 500), turned 90° so it hangs
/// straight down from the elbow; the Brush picked.
fn posed_paint_harness(rig_setup: bool) -> Harness<'static, State> {
    let mut doc = Document::default();
    let paint = doc.add_paint_layer(TimelineId::Main, "Painted", 0).unwrap();
    let folder = doc.add_folder(TimelineId::Main, &[paint]).unwrap();
    let mut skeleton = inkframe_core::Skeleton::default();
    let upper = skeleton.add_bone(None, Point::new(300.0, 500.0), Point::new(500.0, 500.0), 0);
    let fore = skeleton.add_bone(Some(upper), Point::new(500.0, 500.0), Point::new(700.0, 500.0), 0);
    skeleton.bones[fore].turn.set(0, 90.0);
    doc.layer_mut(folder).unwrap().skeleton = Some(skeleton);
    let mut editor = Editor::new(doc);
    editor.select(Some(paint));
    editor.tool = Tool::Brush;
    editor.rig_setup = rig_setup;
    harness(editor)
}

#[test]
fn painting_on_a_posed_frame_lands_where_its_seen() {
    let mut h = posed_paint_harness(false);
    // On the forearm as it hangs, 150 below the elbow: that's 150 along
    // it at rest, at (650, 500) on the canvas.
    let seen = screen(&h, 500.0, 650.0);
    click(&mut h, seen);
    assert_eq!(painted_at(&h, 650, 500)[3], 255, "painted on the forearm, at rest");
    assert_eq!(painted_at(&h, 500, 650)[3], 0, "not where the pointer was on the canvas");
}

#[test]
fn in_rig_setup_painting_goes_straight_onto_the_rest_pose() {
    let mut h = posed_paint_harness(true);
    assert!(h.state().1.doc.rest_pose.0, "the stage shows skeletons at rest");
    let at = screen(&h, 650.0, 500.0);
    click(&mut h, at);
    assert_eq!(painted_at(&h, 650, 500)[3], 255);
}

#[test]
fn shift_with_the_pen_snaps_to_the_perspective_guide() {
    let mut editor = Editor::new(Document::default());
    editor.doc.perspective =
        Some(inkframe_core::Perspective::new(inkframe_core::PerspectiveKind::OnePoint, 1920.0, 1080.0));
    editor.tool = Tool::Pen;
    let mut h = harness(editor);
    let vp = h.state().1.doc.perspective.as_ref().unwrap().points[0];
    let (first, rough) = (screen(&h, 400.0, 900.0), screen(&h, 700.0, 640.0));
    click(&mut h, first);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    click(&mut h, rough);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
    let anchors = h.state().0.pen.as_ref().expect("drawing a path").anchors().to_vec();
    let (start, next) = (anchors[0].point, anchors[1].point);
    let (a, b) = (next - start, vp - start);
    let sine = (a.x * b.y - a.y * b.x) / (a.length() * b.length());
    assert!(sine.abs() < 1e-3, "toward the vanishing point: {next:?}");
}
