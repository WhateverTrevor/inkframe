//! The stage: the canvas where the animation is displayed and drawn on.
//!
//! The document is rendered by `inkframe-render` into a texture, so the stage
//! shows exactly what an export would produce. Editing feedback (selection
//! outlines, shapes being dragged out) is painted on top with egui, because
//! it's interface rather than artwork.

use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{
    Document, Element, LayerId, Point, Rect as StageRect, Shape, Style, Transform, fit_freehand_path,
};
use inkframe_render::{RenderOptions, output_size, render_frame_into};

use crate::convert::{to_color32, to_point};
use crate::editor::{Editor, OnionSkin, Tool};

/// Gap kept around the stage when it's zoomed to fit the window, in points.
const FIT_MARGIN: f32 = 24.0;
/// Largest stage texture we'll render. Beyond this (when zoomed far in) the
/// texture is stretched instead, which looks soft but keeps memory sane.
const MAX_TEXTURE_SIDE: f32 = 4096.0;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;
/// How far from a shape a click still counts as hitting it, in screen points.
const HIT_TOLERANCE: f32 = 4.0;

const PASTEBOARD: Color32 = Color32::from_gray(48);
const SELECTION: Color32 = Color32::from_rgb(0x2f, 0x9b, 0xff);
const ONION_BEFORE: inkframe_core::Color = inkframe_core::Color::rgb(0x30, 0x60, 0xff);
const ONION_AFTER: inkframe_core::Color = inkframe_core::Color::rgb(0x20, 0xb0, 0x40);

/// Maps between stage pixels and screen points for the current zoom and pan.
#[derive(Debug, Clone, Copy)]
struct View {
    /// Screen position of the stage's top-left corner.
    origin: Pos2,
    /// Screen points per stage pixel.
    zoom: f32,
}

impl View {
    fn to_screen(self, p: Point) -> Pos2 {
        self.origin + vec2(p.x, p.y) * self.zoom
    }

    fn to_stage(self, p: Pos2) -> Point {
        to_point(Pos2::ZERO + (p - self.origin) / self.zoom)
    }
}

/// An in-progress mouse interaction on the stage.
enum Gesture {
    /// Dragging a layer. Positions are in stage pixels.
    Move { layer: LayerId, grab: Point, start_position: Point },
    /// Dragging out a new rectangle or ellipse.
    DrawShape { tool: Tool, start: Point, current: Point },
    /// Drawing a freehand stroke.
    Pencil { points: Vec<Point> },
    /// Space + drag scrolls the view, like the hand tool in Animate.
    Pan,
}

/// Everything that affects the rendered stage image. If none of it changed
/// since last time, we can skip re-rendering and reuse the texture.
#[derive(PartialEq)]
struct RenderKey {
    doc: Document,
    frame: u32,
    size: (u32, u32),
    onion: Option<OnionSkin>,
}

pub struct StageView {
    /// `None` means "fit the stage to the window", which follows window resizes.
    zoom: Option<f32>,
    /// Offset of the stage's center from the viewport's center, in points.
    pan: Vec2,
    gesture: Option<Gesture>,
    texture: Option<egui::TextureHandle>,
    rendered: Option<RenderKey>,
    /// The viewport from the last frame, for zoom commands that come from menus.
    last_area: Rect,
}

impl Default for StageView {
    fn default() -> Self {
        Self {
            zoom: None,
            pan: Vec2::ZERO,
            gesture: None,
            texture: None,
            rendered: None,
            last_area: Rect::NOTHING,
        }
    }
}

impl StageView {
    /// True while the user is dragging on the stage. Undo steps aren't
    /// recorded mid-gesture, so a drag becomes a single undoable action.
    pub fn is_busy(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn zoom_to_fit(&mut self) {
        self.zoom = None;
        self.pan = Vec2::ZERO;
    }

    pub fn zoom_to_actual_size(&mut self, doc: &Document) {
        self.set_zoom_about(doc, self.last_area.center(), 1.0);
    }

    pub fn zoom_by(&mut self, doc: &Document, factor: f32) {
        let current = self.view(self.last_area, doc).zoom;
        self.set_zoom_about(doc, self.last_area.center(), current * factor);
    }

    /// The zoom level as a percentage, for display.
    pub fn zoom_percent(&self, doc: &Document) -> f32 {
        self.view(self.last_area, doc).zoom * 100.0
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let area = response.rect;
        self.last_area = area;

        self.handle_navigation(ui, &response, &editor.doc);
        let view = self.view(area, &editor.doc);
        let stage_rect = Rect::from_min_size(
            view.origin,
            vec2(editor.doc.width as f32, editor.doc.height as f32) * view.zoom,
        );

        painter.rect_filled(area, 0.0, PASTEBOARD);
        if !editor.doc.background.is_opaque() {
            draw_checkerboard(&painter.with_clip_rect(area), stage_rect);
        }
        self.update_texture(ui.ctx(), editor, stage_rect.size());
        if let Some(texture) = &self.texture {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(texture.id(), stage_rect, uv, Color32::WHITE);
        }
        painter.rect_stroke(stage_rect, 0.0, (1.0, Color32::from_gray(20)), StrokeKind::Outside);

        self.handle_tools(ui, &response, editor, view);
        self.draw_overlays(&painter.with_clip_rect(area), editor, view);
        self.update_cursor(ui, &response, editor.tool);
    }

    fn view(&self, area: Rect, doc: &Document) -> View {
        let stage_size = vec2(doc.width as f32, doc.height as f32);
        let zoom = self.zoom.unwrap_or_else(|| {
            let available = (area.size() - Vec2::splat(FIT_MARGIN * 2.0)).max(Vec2::splat(1.0));
            (available / stage_size).min_elem().clamp(MIN_ZOOM, MAX_ZOOM)
        });
        let center = area.center() + self.pan;
        View { origin: center - stage_size * zoom / 2.0, zoom }
    }

    /// Changes zoom while keeping the stage point under `anchor` fixed on
    /// screen, which is what makes zooming toward the cursor feel natural.
    fn set_zoom_about(&mut self, doc: &Document, anchor: Pos2, zoom: f32) {
        let before = self.view(self.last_area, doc);
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let anchored = before.to_stage(anchor);

        // Solve for the new origin that puts `anchored` back under `anchor`,
        // then express it as a pan offset from the viewport center.
        let origin = anchor - vec2(anchored.x, anchored.y) * zoom;
        let stage_size = vec2(doc.width as f32, doc.height as f32);
        self.pan = origin + stage_size * zoom / 2.0 - self.last_area.center();
        self.zoom = Some(zoom);
    }

    fn handle_navigation(&mut self, ui: &egui::Ui, response: &egui::Response, doc: &Document) {
        if response.dragged_by(PointerButton::Middle) || matches!(self.gesture, Some(Gesture::Pan)) {
            self.pan += response.drag_delta();
        }
        if !response.hovered() {
            return;
        }
        // egui reports pinch gestures and ⌘/Ctrl + scroll as zoom, and plain
        // two-finger scrolling as scroll.
        let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if zoom_delta != 1.0 {
            if let Some(pointer) = response.hover_pos() {
                let zoom = self.view(response.rect, doc).zoom * zoom_delta;
                self.set_zoom_about(doc, pointer, zoom);
            }
        } else if scroll != Vec2::ZERO {
            self.pan += scroll;
        }
    }

    fn handle_tools(&mut self, ui: &egui::Ui, response: &egui::Response, editor: &mut Editor, view: View) {
        let shift = ui.input(|i| i.modifiers.shift);

        if response.drag_started_by(PointerButton::Primary) {
            // Use where the button went *down*, not where the pointer is now:
            // egui only reports a drag once the pointer has moved a few
            // points, and those points matter when drawing precisely.
            let press = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos());
            let space_held = ui.input(|i| i.key_down(egui::Key::Space));
            if let Some(press) = press {
                self.gesture = self.begin_gesture(editor, view.to_stage(press), view, space_held);
            }
        }

        if let Some(pointer) = response.interact_pointer_pos() {
            let p = view.to_stage(pointer);
            match &mut self.gesture {
                Some(Gesture::Move { layer, grab, start_position }) => {
                    let mut delta = p - *grab;
                    if shift {
                        // Constrain to the dominant axis, like every design tool.
                        if delta.x.abs() > delta.y.abs() { delta.y = 0.0 } else { delta.x = 0.0 }
                    }
                    let id = *layer;
                    let position = *start_position + delta;
                    if let Some(layer) = editor.doc.layer(id) {
                        let pose = Transform { position, ..layer.transform_at(editor.frame) };
                        editor.set_pose(id, pose);
                    }
                }
                Some(Gesture::DrawShape { start, current, .. }) => {
                    *current = if shift { constrain_square(*start, p) } else { p };
                }
                Some(Gesture::Pencil { points }) => {
                    // Skip samples closer than half a screen point; they add
                    // nothing but noise for the smoother to clean up.
                    if points.last().is_none_or(|last| last.distance(p) * view.zoom > 0.5) {
                        points.push(p);
                    }
                }
                Some(Gesture::Pan) | None => {}
            }
        }

        if response.drag_stopped() {
            if let Some(gesture) = self.gesture.take() {
                finish_gesture(gesture, editor, view);
            }
        }

        // A click without dragging selects whatever is under the pointer.
        if response.clicked() && editor.tool == Tool::Select {
            if let Some(pointer) = response.interact_pointer_pos() {
                let tolerance = HIT_TOLERANCE / view.zoom;
                editor.selection = editor.doc.hit_test(view.to_stage(pointer), editor.frame, tolerance);
            }
        }
    }

    fn begin_gesture(&self, editor: &mut Editor, p: Point, view: View, space_held: bool) -> Option<Gesture> {
        if space_held {
            return Some(Gesture::Pan);
        }
        match editor.tool {
            Tool::Select => {
                let tolerance = HIT_TOLERANCE / view.zoom;
                let hit = editor.doc.hit_test(p, editor.frame, tolerance);
                editor.selection = hit;
                let layer = editor.doc.layer(hit?)?;
                Some(Gesture::Move {
                    layer: layer.id,
                    grab: p,
                    start_position: layer.transform_at(editor.frame).position,
                })
            }
            tool @ (Tool::Rectangle | Tool::Ellipse) => {
                Some(Gesture::DrawShape { tool, start: p, current: p })
            }
            Tool::Pencil => Some(Gesture::Pencil { points: vec![p] }),
        }
    }

    fn draw_overlays(&self, painter: &egui::Painter, editor: &Editor, view: View) {
        // Outline the selected layer's bounds, following its rotation and scale.
        if let Some(layer) = editor.selected_layer().filter(|l| l.visible) {
            if let Some(bounds) = layer.local_bounds() {
                let pose = layer.transform_at(editor.frame);
                let to_stage = pose.to_affine();
                let corners = bounds.corners().map(|c| view.to_screen(to_stage.apply(c))).to_vec();
                painter.add(egui::Shape::closed_line(corners, (1.5, SELECTION)));
                // The pivot: the point the layer rotates and scales around.
                let pivot = view.to_screen(pose.position);
                painter.circle(pivot, 3.5, Color32::WHITE, (1.5, SELECTION));
            }
        }

        let preview = (1.0, SELECTION);
        match &self.gesture {
            Some(Gesture::DrawShape { tool, start, current }) => {
                let rect = Rect::from_two_pos(view.to_screen(*start), view.to_screen(*current));
                if *tool == Tool::Ellipse {
                    painter.add(egui::Shape::ellipse_stroke(rect.center(), rect.size() / 2.0, preview));
                } else {
                    painter.rect_stroke(rect, 0.0, preview, StrokeKind::Middle);
                }
            }
            Some(Gesture::Pencil { points }) => {
                let stroke = editor.pencil_stroke;
                let line = points.iter().map(|p| view.to_screen(*p)).collect();
                painter.line(line, (stroke.width * view.zoom, to_color32(stroke.color)));
            }
            _ => {}
        }
    }

    fn update_cursor(&self, ui: &egui::Ui, response: &egui::Response, tool: Tool) {
        if !response.hovered() && self.gesture.is_none() {
            return;
        }
        let space_held = ui.input(|i| i.key_down(egui::Key::Space));
        let icon = match (&self.gesture, tool) {
            (Some(Gesture::Pan), _) => CursorIcon::Grabbing,
            _ if space_held => CursorIcon::Grab,
            (Some(Gesture::Move { .. }), _) => CursorIcon::Move,
            (_, Tool::Select) => CursorIcon::Default,
            (_, Tool::Rectangle | Tool::Ellipse | Tool::Pencil) => CursorIcon::Crosshair,
        };
        ui.ctx().set_cursor_icon(icon);
    }

    /// Re-renders the stage texture if anything visible has changed.
    fn update_texture(&mut self, ctx: &egui::Context, editor: &Editor, display_size: Vec2) {
        let doc = &editor.doc;

        // Render at the stage's on-screen size in physical pixels, so it's
        // sharp on Retina displays without wasting time on detail that
        // can't be seen when zoomed out.
        let physical_width = display_size.x * ctx.pixels_per_point();
        let largest_side = doc.width.max(doc.height) as f32;
        let scale = (physical_width / doc.width as f32).min(MAX_TEXTURE_SIDE / largest_side);
        let size = output_size(doc, scale);

        // Onion skins are hidden during playback, where they'd just be a blur.
        let onion = (editor.onion.enabled && !editor.playback.playing).then_some(editor.onion);
        let key = RenderKey { doc: doc.clone(), frame: editor.frame, size, onion };
        if self.rendered.as_ref() == Some(&key) {
            return;
        }

        let Some(mut pixmap) = tiny_skia::Pixmap::new(size.0, size.1) else {
            return;
        };
        let bg = doc.background;
        pixmap.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));

        if let Some(onion) = onion {
            draw_onion_skins(&mut pixmap, doc, editor.frame, onion, scale);
        }
        let options = RenderOptions { scale, draw_background: false, ..RenderOptions::default() };
        render_frame_into(&mut pixmap, doc, editor.frame, &options);

        // tiny-skia and egui both use premultiplied alpha, so the bytes can be
        // handed over as they are.
        let image =
            egui::ColorImage::from_rgba_premultiplied([size.0 as usize, size.1 as usize], pixmap.data());
        match &mut self.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("stage", image, egui::TextureOptions::LINEAR)),
        }
        self.rendered = Some(key);
    }
}

/// Draws faint, tinted copies of neighbouring frames: blue for the past,
/// green for the future (the same convention as most 2D animation tools).
/// Nearer frames are stronger so the direction of motion reads clearly.
fn draw_onion_skins(
    pixmap: &mut tiny_skia::Pixmap,
    doc: &Document,
    frame: u32,
    onion: OnionSkin,
    scale: f32,
) {
    let ghost = |distance: u32, count: u32, tint| RenderOptions {
        scale,
        draw_background: false,
        tint: Some(tint),
        opacity: 0.45 * (count - distance + 1) as f32 / count as f32,
    };
    // Farthest first, so nearer ghosts paint on top.
    for distance in (1..=onion.before).rev() {
        if let Some(f) = frame.checked_sub(distance) {
            render_frame_into(pixmap, doc, f, &ghost(distance, onion.before, ONION_BEFORE));
        }
    }
    for distance in (1..=onion.after).rev() {
        let f = frame + distance;
        if f <= doc.last_frame() {
            render_frame_into(pixmap, doc, f, &ghost(distance, onion.after, ONION_AFTER));
        }
    }
}

/// Turns a finished gesture into a document change.
fn finish_gesture(gesture: Gesture, editor: &mut Editor, view: View) {
    match gesture {
        Gesture::DrawShape { tool, start, current } => {
            let rect = StageRect::from_points(start, current);
            // Ignore accidental clicks with a drawing tool.
            if rect.width() * view.zoom < 2.0 || rect.height() * view.zoom < 2.0 {
                return;
            }
            let (shape, name) = match tool {
                Tool::Ellipse => {
                    (Shape::Ellipse { radius_x: rect.width() / 2.0, radius_y: rect.height() / 2.0 }, "Oval")
                }
                _ => (
                    Shape::Rect { width: rect.width(), height: rect.height(), corner_radius: 0.0 },
                    "Rectangle",
                ),
            };
            let element = Element { shape, style: editor.draw_style };
            add_layer(editor, name, element, rect.center());
        }
        Gesture::Pencil { points } => {
            // Simplify relative to the screen, so a stroke drawn while zoomed
            // in keeps proportionally more detail.
            let tolerance = 1.5 / view.zoom;
            let Some(mut path) = fit_freehand_path(&points, tolerance) else {
                return;
            };
            // Recenter the path on its own middle, so the layer's pivot sits
            // in the middle of the stroke rather than at the stage origin.
            let Some(bounds) = StageRect::bounding(path.flatten()) else {
                return;
            };
            let center = bounds.center();
            path.translate(-center);
            let style = Style { fill: None, stroke: Some(editor.pencil_stroke) };
            add_layer(editor, "Stroke", Element { shape: Shape::Path(path), style }, center);
        }
        Gesture::Move { .. } | Gesture::Pan => {}
    }
}

fn add_layer(editor: &mut Editor, prefix: &str, element: Element, position: Point) {
    let name = editor.doc.next_layer_name(prefix);
    let id = editor.doc.add_layer(name, vec![element], editor.frame, Transform::at(position));
    editor.selection = Some(id);
}

/// Moves `end` so the rectangle from `start` is a square, keeping the drag direction.
fn constrain_square(start: Point, end: Point) -> Point {
    let d = end - start;
    let side = d.x.abs().max(d.y.abs());
    start + Point::new(side.copysign(d.x), side.copysign(d.y))
}

/// The classic grey checkerboard that signals "transparent here".
fn draw_checkerboard(painter: &egui::Painter, stage: Rect) {
    const CELL: f32 = 10.0;
    let visible = stage.intersect(painter.clip_rect());
    if !visible.is_positive() {
        return;
    }
    painter.rect_filled(visible, 0.0, Color32::from_gray(204));

    // Only iterate over cells that are actually on screen.
    let first = ((visible.min - stage.min) / CELL).floor();
    let last = ((visible.max - stage.min) / CELL).ceil();
    for row in first.y as i32..last.y as i32 {
        for col in first.x as i32..last.x as i32 {
            if (row + col) % 2 == 0 {
                continue;
            }
            let min = stage.min + vec2(col as f32, row as f32) * CELL;
            let cell = Rect::from_min_size(min, Vec2::splat(CELL)).intersect(stage);
            painter.rect_filled(cell, 0.0, Color32::from_gray(153));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_round_trips() {
        let view = View { origin: pos2(100.0, 50.0), zoom: 0.5 };
        let p = Point::new(300.0, 200.0);
        assert_eq!(view.to_stage(view.to_screen(p)), p);
    }

    #[test]
    fn square_constraint_keeps_direction() {
        let end = constrain_square(Point::ZERO, Point::new(-10.0, 4.0));
        assert_eq!(end, Point::new(-10.0, 10.0));
    }

    // ---- Interaction tests: drive the real stage with simulated mouse input.

    use egui_kittest::Harness;

    type State = (StageView, Editor);

    fn harness(editor: Editor) -> Harness<'static, State> {
        let mut harness = Harness::new_ui_state(
            |ui, (stage, editor): &mut State| stage.show(ui, editor),
            (StageView::default(), editor),
        );
        harness.set_size(vec2(800.0, 600.0));
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
        let Shape::Rect { width, height, .. } = layer.elements[0].shape else { panic!("not a rect") };
        // Allow a pixel of slack for screen-to-stage rounding.
        assert!((width - 400.0).abs() < 3.0 && (height - 200.0).abs() < 3.0, "{width}×{height}");
        assert!(layer.transform_at(0).position.distance(Point::new(300.0, 200.0)) < 3.0);
        assert_eq!(editor.selection, Some(layer.id));
    }

    #[test]
    fn dragging_a_layer_on_a_later_frame_creates_a_tween() {
        let mut editor = Editor::new(Document::default());
        let square = Element {
            shape: Shape::Rect { width: 200.0, height: 200.0, corner_radius: 0.0 },
            style: editor.draw_style,
        };
        let id = editor.doc.add_layer("Box", vec![square], 0, Transform::at(Point::new(400.0, 400.0)));
        editor.set_frame(24);
        let mut h = harness(editor);

        let grab = screen(&h, 400.0, 400.0);
        let release = screen(&h, 1000.0, 400.0);
        drag(&mut h, grab, &[grab + vec2(20.0, 0.0), release]);

        let track = &h.state().1.doc.layer(id).unwrap().track;
        assert_eq!(track.keys().len(), 2, "a keyframe should be added on frame 24");
        let moved = track.key_at(24).unwrap().transform.position;
        assert!(moved.distance(Point::new(1000.0, 400.0)) < 3.0, "moved to {moved:?}");
        // Frame 0 is untouched, so the layer now tweens between the two.
        assert_eq!(track.key_at(0).unwrap().transform.position, Point::new(400.0, 400.0));
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
        let Shape::Path(stroke) = &layer.elements[0].shape else { panic!("not a path") };
        assert!(stroke.segments.len() >= 2, "a wavy stroke should keep several segments");
        // The path was recentered, so its local bounds straddle the origin.
        let bounds = layer.local_bounds().unwrap();
        assert!(bounds.center().distance(Point::ZERO) < 30.0);
    }

    #[test]
    fn zooming_keeps_the_anchor_fixed() {
        let doc = Document::default();
        let mut stage = StageView {
            last_area: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            ..Default::default()
        };
        let anchor = pos2(200.0, 150.0);
        let before = stage.view(stage.last_area, &doc).to_stage(anchor);
        stage.set_zoom_about(&doc, anchor, 2.0);
        let after = stage.view(stage.last_area, &doc).to_stage(anchor);
        assert!(before.distance(after) < 1e-3);
    }
}
