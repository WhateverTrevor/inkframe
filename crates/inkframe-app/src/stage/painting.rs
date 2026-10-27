//! Painting on a paint layer from the stage: strokes with the Brush, Eraser,
//! Pattern Stamp and retouching tools (with symmetry, and the brush's
//! on-canvas sizing and quick menu), the bucket and gradient, the
//! eyedropper, and drawing the brush's outline and the stroke under way.

use egui::{Color32, Pos2, vec2};
use inkframe_core::{
    BrushMode, BrushStroke, Canvas, CanvasId, Color, LayerId, Pen, Point, Retouch, RetouchStroke, Shape, Tip,
};

use super::{HIT_TOLERANCE, StageView, View};
use crate::editor::{Editor, Tool};
use crate::tablet;

impl StageView {
    /// The Brush, Eraser and Eyedropper. Painting starts the moment the pen
    /// touches (not after it has moved a little, as dragging does), so a
    /// tap leaves a dot, and paints through every pointer event, not just
    /// one per frame, so quick strokes stay smooth.
    pub(super) fn handle_painting(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) {
        if editor.tool.uses_brush() && self.painting.is_none() && self.size_brush(ui, response, editor, view)
        {
            return;
        }
        if self.painting.is_none() && self.move_symmetry_centre(ui, response, editor, view) {
            return;
        }
        let (pressed, down, alt, space_held) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.modifiers.alt,
                i.key_down(egui::Key::Space),
            )
        });
        let press = ui.input(|i| i.pointer.press_origin());
        if pressed
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(press) = press
        {
            match editor.tool {
                // Alt picks a colour with the brush, as in Photoshop.
                Tool::Eyedropper => return pick_color(editor, view.to_local(press)),
                Tool::Brush | Tool::Mixer if alt => return pick_color(editor, view.to_local(press)),
                // Alt-click sets where the Clone Stamp copies from.
                Tool::CloneStamp if alt => return set_clone_source(editor, view, press),
                Tool::PaintBucket => return fill_at(editor, view, press),
                _ => {}
            }
            self.painting = start_painting(editor, view, press);
            if let Some(painting) = &mut self.painting {
                let pen = tablet::Sample::now(press);
                painting.smoothed = press;
                painting.pen = pen;
                paint_to(painting, editor, view, pen);
            }
        }
        let Some(painting) = &mut self.painting else { return };
        if down {
            let samples = ui.input(tablet::samples);
            if samples.is_empty() {
                // Resting: Build-up keeps the paint flowing.
                let dt = ui.input(|i| i.stable_dt).min(0.1);
                if let (PaintWork::Brush(stroke), Some(canvas)) =
                    (&mut painting.work, editor.doc.canvas_mut(painting.canvas))
                {
                    stroke.rest(canvas, dt);
                }
                if editor.brush_for(editor.tool).is_some_and(|b| b.build_up) {
                    ui.ctx().request_repaint();
                }
            }
            for sample in samples {
                // Smoothing: each point moves the stroke only part of the
                // way toward the pen.
                let follow = 1.0 - editor.smoothing.clamp(0.0, 0.95);
                painting.smoothed += (sample.pos - painting.smoothed) * follow;
                painting.pen = sample;
                paint_to(painting, editor, view, tablet::Sample { pos: painting.smoothed, ..sample });
            }
        } else {
            // Lifted: finish where the pen actually is.
            let pen = painting.pen;
            paint_to(painting, editor, view, pen);
            if let PaintWork::Gradient { from } = painting.work {
                lay_gradient(painting, editor, view, from, pen.pos);
            }
            self.painting = None;
        }
    }

    /// Dragging symmetry's centre by its handle, rather than painting.
    /// Returns whether it's being dragged.
    pub(super) fn move_symmetry_centre(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        let (pressed, down, pointer) =
            ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.interact_pos()));
        if !editor.paints_symmetrically() {
            self.moving_symmetry = false;
            return false;
        }
        let Some((layer, _, width, height)) = editor.paint_target() else { return false };
        if pressed
            && response.is_pointer_button_down_on()
            && let (Some(p), Some(handle)) =
                (pointer, canvas_to_screen(editor, view, layer, editor.symmetry_point(width, height)))
        {
            self.moving_symmetry = p.distance(handle) <= SYMMETRY_HANDLE + 3.0;
        }
        if !self.moving_symmetry {
            return false;
        }
        if !down {
            self.moving_symmetry = false;
            return true;
        }
        if let Some(at) = pointer.and_then(|p| canvas_point(editor, view, layer, (width, height), p)) {
            editor.symmetry_centre = Some((at.x, at.y));
        }
        true
    }

    /// The brush's quick menu and its on-canvas sizing, as in Photoshop:
    /// right-click for the quick menu; Ctrl+Option-drag (Alt+right-drag on
    /// Windows) left and right for the size, up and down for the hardness.
    /// Returns whether it's sizing, so nothing paints meanwhile.
    pub(super) fn size_brush(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        let (primary, secondary, modifiers, down, pointer) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.modifiers,
                i.pointer.any_down(),
                i.pointer.interact_pos(),
            )
        });
        if let Some((origin, size, hardness)) = self.sizing_brush {
            if !down {
                self.sizing_brush = None;
                return true;
            }
            if let (Some(p), Some(brush)) = (pointer, editor.brush_of(editor.tool)) {
                let moved = p - origin;
                brush.size = (size + moved.x * 2.0 / view.zoom).clamp(1.0, 2500.0);
                brush.hardness = (hardness - moved.y / 200.0).clamp(0.0, 1.0);
            }
            return true;
        }
        let starts = response.is_pointer_button_down_on()
            && ((primary && modifiers.ctrl && modifiers.alt) || (secondary && modifiers.alt));
        if starts && let (Some(origin), Some(brush)) = (pointer, editor.brush_for(editor.tool)) {
            self.sizing_brush = Some((origin, brush.size, brush.hardness));
            editor.brush_quick_menu = None;
            return true;
        }
        if response.secondary_clicked()
            && !modifiers.alt
            && let Some(p) = response.interact_pointer_pos()
        {
            editor.brush_quick_menu = Some((p.x, p.y));
        }
        false
    }

    /// While a gradient is being dragged out: the line it runs along.
    pub(super) fn draw_painting(&self, painter: &egui::Painter, editor: &Editor, view: View) {
        if let Some(Painting { work: PaintWork::Gradient { from }, pen, .. }) = &self.painting {
            painter.line_segment([*from, pen.pos], (3.0, Color32::from_black_alpha(140)));
            painter.line_segment([*from, pen.pos], (1.5, Color32::WHITE));
            for end in [*from, pen.pos] {
                painter.circle(end, 4.0, Color32::WHITE, (1.5, Color32::from_black_alpha(160)));
            }
        }
        // The Clone Stamp's source: where it is, or, mid-stroke, where it's
        // copying from right now.
        if editor.tool == Tool::CloneStamp
            && let Some((layer, source)) = editor.clone_source
        {
            let at = match (&self.painting, editor.clone_offset) {
                (Some(painting), Some(offset)) => {
                    canvas_point(editor, view, painting.layer, painting.size, painting.pen.pos)
                        .map(|p| p + offset)
                }
                _ => Some(source),
            };
            if let Some(screen) = at.and_then(|p| canvas_to_screen(editor, view, layer, p)) {
                let arm = vec2(7.0, 0.0);
                for stroke in [(3.0, Color32::from_black_alpha(140)), (1.0, Color32::WHITE)] {
                    painter.line_segment([screen - arm, screen + arm], stroke);
                    painter.line_segment([screen - arm.rot90(), screen + arm.rot90()], stroke);
                    painter.circle_stroke(screen, 5.0, stroke);
                }
            }
        }
    }
}

/// A brush or eraser stroke being painted.
pub(super) struct Painting {
    layer: LayerId,
    canvas: CanvasId,
    /// The canvas's size, to find its pixels from the layer's coordinates.
    size: (u32, u32),
    work: PaintWork,
    /// Where the smoothed stroke has got to, on screen. It trails the pen
    /// by `Editor::smoothing`, which steadies a shaky line.
    smoothed: Pos2,
    /// Where the pen really is, and how hard it's pressed, for the stroke
    /// to catch up to when it lifts.
    pen: tablet::Sample,
}

/// What a painting gesture is doing.
pub(super) enum PaintWork {
    /// The brush or eraser.
    Brush(BrushStroke),
    /// Smudge, Blur, Sharpen, Dodge, Burn or the Clone Stamp.
    Retouch(RetouchStroke),
    /// Dragging out a gradient, from where the drag began; it's laid down
    /// when the pen lifts.
    Gradient { from: Pos2 },
}

/// A circle the size the brush (or eraser) paints at full pressure, around
/// the pointer, scaled like the canvas it would paint on.
/// Symmetry's lines through the paint layer's middle, faintly, while the
/// Brush or Eraser paints with it: one or two mirror lines, or a line out
/// to each segment's edge.
pub(super) fn draw_symmetry(painter: &egui::Painter, editor: &Editor, view: View) {
    use inkframe_core::Symmetry;
    if !editor.paints_symmetrically() {
        return;
    }
    let Some((layer, _, width, height)) = editor.paint_target() else { return };
    let (w, h) = (width as f32, height as f32);
    let middle = editor.symmetry_point(width, height);
    let reach = w.max(h) * 2.0;
    let mut lines = Vec::new();
    match editor.symmetry {
        Symmetry::Vertical => lines.push((Point::new(middle.x, 0.0), Point::new(middle.x, h))),
        Symmetry::Horizontal => lines.push((Point::new(0.0, middle.y), Point::new(w, middle.y))),
        Symmetry::Both => {
            lines.push((Point::new(middle.x, 0.0), Point::new(middle.x, h)));
            lines.push((Point::new(0.0, middle.y), Point::new(w, middle.y)));
        }
        Symmetry::Radial(n) | Symmetry::Mandala(n) => {
            for i in 0..n {
                let turn = std::f32::consts::TAU * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
                lines
                    .push((middle, Point::new(middle.x + turn.cos() * reach, middle.y + turn.sin() * reach)));
            }
        }
        Symmetry::Off => {}
    }
    let colour = Color32::from_rgba_unmultiplied(120, 170, 255, 140);
    for (a, b) in lines {
        if let (Some(a), Some(b)) =
            (canvas_to_screen(editor, view, layer, a), canvas_to_screen(editor, view, layer, b))
        {
            painter.add(egui::Shape::dashed_line(&[a, b], (1.0, colour), 6.0, 4.0));
        }
    }
    // The centre's handle, to drag it somewhere else.
    if let Some(at) = canvas_to_screen(editor, view, layer, middle) {
        painter.circle(at, SYMMETRY_HANDLE, Color32::from_black_alpha(90), (1.5, Color32::WHITE));
        painter.circle_filled(at, 2.0, Color32::WHITE);
    }
}

/// The radius of symmetry's centre handle, in screen points.
pub(super) const SYMMETRY_HANDLE: f32 = 6.0;

pub(super) fn draw_brush_outline(painter: &egui::Painter, editor: &Editor, view: View, pointer: Pos2) {
    let Some(settings) = editor.brush_for(editor.tool) else { return };
    let canvas_scale = editor
        .paint_target()
        .and_then(|(layer, ..)| editor.doc.layer(layer))
        .map_or(1.0, |layer| editor.doc.layer_matrix(layer, editor.frame).average_scale());
    let radius = settings.size / 2.0 * canvas_scale * view.zoom;
    // The tip's outline: an oval turned to the angle and squashed to the
    // roundness (a sampled tip's, its frame), in dark and light so it
    // shows on any colour.
    let aspect = settings.tip.aspect();
    let (rx, ry) = if aspect >= 1.0 { (radius, radius / aspect) } else { (radius * aspect, radius) };
    let ry = ry * settings.roundness.clamp(0.01, 1.0);
    let turn = settings.angle.to_radians();
    let (along, across) = (vec2(turn.cos(), -turn.sin()), vec2(turn.sin(), turn.cos()));
    let outline: Vec<Pos2> = match settings.tip {
        Tip::Round => (0..64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                pointer + along * a.cos() * rx + across * a.sin() * ry
            })
            .collect(),
        Tip::Sampled(_) => [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y)| pointer + along * x * rx + across * y * ry)
            .to_vec(),
    };
    painter.add(egui::Shape::closed_line(outline.clone(), (2.0, Color32::from_black_alpha(140))));
    painter.add(egui::Shape::closed_line(outline, (1.0, Color32::WHITE)));
    // A soft brush's full-strength core, faintly: inside it the paint goes
    // on at full strength, and it fades out between the two rings.
    if settings.tip == Tip::Round && settings.hardness < 0.95 && rx.min(ry) > 6.0 {
        let core = settings.hardness.max(0.05);
        let inner: Vec<Pos2> = (0..64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                pointer + along * a.cos() * rx * core + across * a.sin() * ry * core
            })
            .collect();
        painter.add(egui::Shape::closed_line(inner, (1.0, Color32::from_white_alpha(110))));
    }
}

/// Gets a stroke ready on the selected paint layer, first adding a paint
/// layer if the selected layer isn't one (as Photoshop would ask for).
pub(super) fn start_painting(editor: &mut Editor, view: View, press: Pos2) -> Option<Painting> {
    if editor.paint_target().is_none() {
        editor.add_paint_layer();
    }
    let (layer, canvas, width, height) = editor.paint_target()?;
    let size = (width, height);
    // Where symmetry mirrors round.
    let middle = editor.symmetry_point(width, height);
    // Painting stays inside the selection, if there is one.
    let selection = editor.selection_on(canvas).cloned();
    let work = match editor.tool {
        Tool::Brush => PaintWork::Brush(
            BrushStroke::new(editor.brush.clone(), BrushMode::Paint(editor.paint_color))
                .with_symmetry(editor.symmetry, middle)
                .with_background(editor.second_color)
                .with_seed(editor.next_stroke_seed())
                .within(selection.clone()),
        ),
        Tool::Eraser => PaintWork::Brush(
            BrushStroke::new(editor.eraser.clone(), BrushMode::Erase)
                .with_symmetry(editor.symmetry, middle)
                .with_seed(editor.next_stroke_seed())
                .within(selection.clone()),
        ),
        Tool::PatternStamp => PaintWork::Brush(
            BrushStroke::new(editor.pattern_stamp.clone(), BrushMode::Paint(editor.paint_color))
                .with_pattern(inkframe_core::PatternPaint {
                    texture: editor.stamp_pattern.clone(),
                    light: editor.second_color,
                })
                .with_symmetry(editor.symmetry, middle)
                .with_seed(editor.next_stroke_seed())
                .within(selection.clone()),
        ),
        Tool::Gradient => PaintWork::Gradient { from: press },
        tool => {
            let kind = retouch_kind(editor, tool, view, layer, size, press)?;
            let brush = editor.retouch_of(tool)?.brush.clone();
            let mut stroke = RetouchStroke::new(brush, kind, editor.doc.canvas(canvas)?).within(selection);
            if tool == Tool::Mixer && editor.mixer_sample_all {
                stroke = stroke.sampling(everything_as_seen(editor, layer, size));
            }
            PaintWork::Retouch(stroke)
        }
    };
    Some(Painting {
        layer,
        canvas,
        size,
        work,
        smoothed: Pos2::ZERO,
        pen: tablet::Sample { pos: Pos2::ZERO, pressure: 1.0, tilt: (0.0, 0.0), stylus: false },
    })
}

/// What a retouching tool does, with its settings. The Clone Stamp needs a
/// source on this layer (Alt-click), and keeps the distance to it for the
/// strokes after the first.
pub(super) fn retouch_kind(
    editor: &mut Editor,
    tool: Tool,
    view: View,
    layer: LayerId,
    size: (u32, u32),
    press: Pos2,
) -> Option<Retouch> {
    let strength = editor.retouch_of(tool)?.strength;
    let range = editor.tone_range;
    Some(match tool {
        Tool::Mixer => Retouch::Mixer {
            color: editor.paint_color,
            wet: strength,
            load: editor.mixer_load,
            mix: editor.mixer_mix,
        },
        Tool::Smudge => Retouch::Smudge { strength },
        Tool::Blur => Retouch::Blur { strength },
        Tool::Sharpen => Retouch::Sharpen { strength },
        Tool::Dodge => Retouch::Dodge { exposure: strength, range },
        Tool::Burn => Retouch::Burn { exposure: strength, range },
        Tool::CloneStamp => {
            let (source_layer, source) = editor.clone_source?;
            if source_layer != layer {
                return None;
            }
            let offset = match editor.clone_offset {
                Some(offset) => offset,
                None => source - canvas_point(editor, view, layer, size, press)?,
            };
            editor.clone_offset = Some(offset);
            Retouch::Clone { offset, opacity: strength }
        }
        _ => return None,
    })
}

/// A point on screen, in the pixels of a layer's canvas (which follows the
/// layer wherever it's been moved, turned or scaled).
pub(super) fn canvas_point(
    editor: &Editor,
    view: View,
    layer: LayerId,
    size: (u32, u32),
    screen: Pos2,
) -> Option<Point> {
    let layer = editor.doc.layer(layer)?;
    let mut local = editor.doc.layer_matrix(layer, editor.frame).inverse()?.apply(view.to_local(screen));
    // On a frame where a skeleton bends the painting, the brush lands on
    // the painting where it's seen, not where it was painted at rest.
    if let Some(bend) = editor.doc.layer_bend(layer, editor.frame) {
        let canvas = inkframe_core::Rect::from_center_size(Point::ZERO, size.0 as f32, size.1 as f32);
        local = bend.unbend(local, canvas)?;
    }
    Some(Point::new(local.x + size.0 as f32 / 2.0, local.y + size.1 as f32 / 2.0))
}

/// The opposite of [`canvas_point`]: a canvas pixel on screen.
pub(super) fn canvas_to_screen(editor: &Editor, view: View, layer: LayerId, at: Point) -> Option<Pos2> {
    let layer = editor.doc.layer(layer)?;
    let size = layer.elements_at(editor.frame).iter().find_map(|e| match e.shape {
        Shape::Paint { width, height, .. } => Some((width, height)),
        _ => None,
    })?;
    let mut local = Point::new(at.x - size.0 as f32 / 2.0, at.y - size.1 as f32 / 2.0);
    if let Some(bend) = editor.doc.layer_bend(layer, editor.frame) {
        local = bend.bend(local);
    }
    Some(view.to_screen(editor.doc.layer_matrix(layer, editor.frame).apply(local)))
}

/// Carries the stroke (or the gradient's drag) on to `screen`.
pub(super) fn paint_to(painting: &mut Painting, editor: &mut Editor, view: View, pen: tablet::Sample) {
    let Some(at) = canvas_point(editor, view, painting.layer, painting.size, pen.pos) else { return };
    let Some(canvas) = editor.doc.canvas_mut(painting.canvas) else { return };
    match &mut painting.work {
        PaintWork::Brush(stroke) => {
            stroke.add_pen(canvas, at, Pen { pressure: pen.pressure, tilt: pen.tilt, stylus: pen.stylus })
        }
        PaintWork::Retouch(stroke) => stroke.add_point(canvas, at, pen.pressure),
        PaintWork::Gradient { .. } => {}
    }
}

/// Lays the gradient dragged from `from` to `to` (on screen).
pub(super) fn lay_gradient(painting: &Painting, editor: &mut Editor, view: View, from: Pos2, to: Pos2) {
    let point = |screen| canvas_point(editor, view, painting.layer, painting.size, screen);
    let (Some(from), Some(to)) = (point(from), point(to)) else { return };
    let start = editor.paint_color;
    let end = if editor.gradient_to_clear { Color { a: 0, ..start } } else { editor.second_color };
    let (shape, opacity) = (editor.gradient_shape, editor.gradient_opacity);
    let selection = editor.selection_on(painting.canvas).cloned();
    if let Some(canvas) = editor.doc.canvas_mut(painting.canvas) {
        let gradient = inkframe_core::Gradient { from, to, start, end, shape, opacity };
        inkframe_core::gradient(canvas, &gradient, selection.as_ref());
    }
}

/// The Paint Bucket: fills on the selected paint layer (made if needed).
pub(super) fn fill_at(editor: &mut Editor, view: View, press: Pos2) {
    // On a vector layer, ink and paint: fill the area its lines enclose.
    // The selected layer if it's one, or else the one clicked.
    if editor.paint_target().is_none() {
        let point = view.to_local(press);
        let frame = editor.frame;
        let vector = |id: LayerId| {
            editor
                .doc
                .layer(id)
                .filter(|l| l.draws_vectors() && !l.locked && l.is_shown_at(frame))
                .map(|l| l.id)
        };
        let target = editor
            .selection
            .and_then(vector)
            .or_else(|| editor.hit_test(point, HIT_TOLERANCE / view.zoom).and_then(vector));
        if let Some(id) = target {
            editor.select(Some(id));
            editor.fill_vector(id, point);
            return;
        }
        editor.add_paint_layer();
    }
    let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
    let Some(at) = canvas_point(editor, view, layer, (width, height), press) else { return };
    let (color, settings) = (editor.paint_color, editor.bucket);
    let selection = editor.selection_on(canvas).cloned();
    if let Some(canvas) = editor.doc.canvas_mut(canvas) {
        let (x, y) = (at.x.floor() as i64, at.y.floor() as i64);
        inkframe_core::bucket_fill(canvas, x, y, color, settings, selection.as_ref());
    }
}

/// Alt-click with the Clone Stamp: copy from here on.
pub(super) fn set_clone_source(editor: &mut Editor, view: View, press: Pos2) {
    let Some((layer, _, width, height)) = editor.paint_target() else { return };
    editor.clone_source = canvas_point(editor, view, layer, (width, height), press).map(|p| (layer, p));
    editor.clone_offset = None;
}

/// Sets the paint colour to what the stage shows at `at` (in the
/// coordinates of the timeline being edited).
/// Every layer as seen on this frame, laid out in a paint layer's pixels
/// (`size` of them): what the Mixer Brush samples with Sample All Layers.
pub(super) fn everything_as_seen(editor: &Editor, layer: LayerId, size: (u32, u32)) -> Canvas {
    let mut seen = Canvas::new(size.0, size.1);
    let options = inkframe_render::RenderOptions { images: Some(&editor.images), ..Default::default() };
    let (Some(image), Some(found)) =
        (inkframe_render::render_frame(&editor.doc, editor.frame, &options), editor.doc.layer(layer))
    else {
        return seen;
    };
    // Each of the layer's pixels, through the layer's placement (and any
    // bend) to the stage, to the rendered pixel there.
    let to_timeline = editor.doc.layer_matrix(found, editor.frame);
    let bend = editor.doc.layer_bend(found, editor.frame);
    let to_stage = editor.timeline_transform();
    let (w, h) = (size.0 as f32, size.1 as f32);
    let mut pixels = vec![0u8; (size.0 * size.1 * 4) as usize];
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let (x, y) = ((i as u32 % size.0) as f32 + 0.5, (i as u32 / size.0) as f32 + 0.5);
        let mut local = Point::new(x - w / 2.0, y - h / 2.0);
        if let Some(bend) = &bend {
            local = bend.bend(local);
        }
        let at = to_stage.apply(to_timeline.apply(local));
        if at.x >= 0.0
            && at.y >= 0.0
            && let Some(p) = image.pixel(at.x as u32, at.y as u32)
        {
            pixel.copy_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
        }
    }
    seen.write_region(0, 0, size.0, size.1, &pixels);
    seen
}

pub(super) fn pick_color(editor: &mut Editor, at: Point) {
    let stage = editor.timeline_transform().apply(at);
    let options = inkframe_render::RenderOptions { images: Some(&editor.images), ..Default::default() };
    let Some(image) = inkframe_render::render_frame(&editor.doc, editor.frame, &options) else { return };
    let (x, y) = (stage.x.floor(), stage.y.floor());
    if x < 0.0 || y < 0.0 {
        return;
    }
    if let Some(pixel) = image.pixel(x as u32, y as u32) {
        let c = pixel.demultiply();
        editor.paint_color = Color::rgb(c.red(), c.green(), c.blue());
    }
}
