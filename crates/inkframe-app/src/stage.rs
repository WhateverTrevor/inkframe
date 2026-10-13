//! The stage: the canvas where the animation is displayed and drawn on.
//!
//! The document is rendered by `inkframe-render` into a texture, so the stage
//! shows exactly what an export would produce. Editing feedback (selection
//! outlines, shapes being dragged out) is painted on top with egui, because
//! it's interface rather than artwork.

use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{
    Affine, Anchor, Document, Element, LayerId, Path, Point, Rect as StageRect, Shape, Style, Transform,
    fit_freehand_path, insert_anchor, nearest_on_anchors, path_from_anchors,
};
use inkframe_render::{RenderOptions, output_size, render_frame_into};

use crate::convert::{to_color32, to_point};
use crate::editor::{Editor, OnionSkin, Tool};
use crate::handles::{RotateDrag, ScaleHandle, ScaleOptions, scale_pose};
use crate::pen::PenPath;
use crate::subselect::{AnchorPart, move_part, remove_anchor, toggle_smooth};

/// Gap kept around the stage when it's zoomed to fit the window, in points.
const FIT_MARGIN: f32 = 24.0;
/// Largest stage texture we'll render. Beyond this (when zoomed far in) the
/// texture is stretched instead, which looks soft but keeps memory sane.
const MAX_TEXTURE_SIDE: f32 = 4096.0;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;
/// How far from a shape a click still counts as hitting it, in screen points.
const HIT_TOLERANCE: f32 = 4.0;
/// Side length of the square scale handles, in screen points.
const HANDLE_SIZE: f32 = 8.0;
/// How far the rotation knob sits above the top edge, in screen points.
const ROTATE_KNOB_DISTANCE: f32 = 24.0;
/// Just outside each corner, within this many points, dragging rotates
/// instead of scaling, as in Animate's Free Transform tool.
const ROTATE_ZONE: f32 = 18.0;
/// How close to an existing pen anchor a click must land to hit it, in screen points.
const ANCHOR_HIT: f32 = 7.0;
/// Pen drags shorter than this make a corner rather than a curve, in screen points.
const MIN_HANDLE_DRAG: f32 = 2.0;

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
    /// Dragging a scale handle. `start` is the pose when the drag began;
    /// every update is computed from it, so errors never accumulate.
    Scale { layer: LayerId, handle: ScaleHandle, start: Transform, bounds: StageRect },
    /// Dragging the rotation knob or a corner's rotate zone.
    Rotate { layer: LayerId, drag: RotateDrag },
    /// Dragging an anchor point or one of its handles with the Subselection
    /// tool. `start` holds the shape's anchors from when the drag began, and
    /// the pointer is converted into the layer's local space with `to_local`.
    EditAnchor {
        target: AnchorRef,
        part: AnchorPart,
        start: Vec<Anchor>,
        closed: bool,
        /// Where the pointer grabbed, in local coordinates.
        grab: Point,
        to_local: Affine,
    },
    /// Space + drag scrolls the view, like the hand tool in Animate.
    Pan,
}

/// One anchor of one shape in a layer, as picked with the Subselection tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRef {
    layer: LayerId,
    element: usize,
    index: usize,
}

/// A spot on the selected shape's outline where the Pen tool would add a point.
#[derive(Debug, Clone, Copy)]
struct OutlineHit {
    layer: LayerId,
    element: usize,
    segment: usize,
    /// How far along the segment, 0–1.
    t: f32,
    /// Where on screen the new point would go.
    screen: Pos2,
}

/// Which transform handle the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HandleHit {
    Scale(ScaleHandle),
    Rotate,
}

/// Screen positions of the transform handles around the selected layer.
///
/// Rebuilt every UI frame from the layer's current pose, so the handles
/// follow the layer through tweens, undo and edits in the inspector.
struct Handles {
    layer: LayerId,
    pose: Transform,
    /// The layer's bounds in its own local coordinates.
    bounds: StageRect,
    scale: [(ScaleHandle, Pos2); 8],
    center: Pos2,
    top: Pos2,
    knob: Pos2,
}

impl Handles {
    /// Handles for the current selection, if it can be transformed.
    fn for_selection(editor: &Editor, view: View) -> Option<Self> {
        if editor.tool != Tool::Select {
            return None;
        }
        let layer = editor.selected_layer().filter(|l| l.visible && !l.locked)?;
        let bounds = layer.local_bounds()?;
        let pose = layer.transform_at(editor.frame);
        let to_stage = pose.to_affine();
        let screen = |local: Point| view.to_screen(to_stage.apply(local));

        let center = screen(bounds.center());
        let top = screen(ScaleHandle::TOP.local_position(bounds));
        // The knob points "up" from the layer's own point of view, so it
        // turns with the layer. A layer with no height has no up direction
        // between center and top edge, so fall back to its rotation.
        let up = match (top - center).normalized() {
            v if v != Vec2::ZERO => v,
            _ => {
                let (sin, cos) = pose.rotation.to_radians().sin_cos();
                vec2(sin, -cos)
            }
        };

        Some(Self {
            layer: layer.id,
            pose,
            bounds,
            scale: ScaleHandle::ALL.map(|h| (h, screen(h.local_position(bounds)))),
            center,
            top,
            knob: top + up * ROTATE_KNOB_DISTANCE,
        })
    }

    fn hit(&self, p: Pos2, view: View) -> Option<HandleHit> {
        if p.distance(self.knob) <= HANDLE_SIZE {
            return Some(HandleHit::Rotate);
        }
        // Handles get a slightly bigger target than they look, so they're
        // easy to grab.
        let near = |pos: &Pos2| (*pos - p).abs().max_elem() <= HANDLE_SIZE * 0.75;
        if let Some((handle, _)) = self.scale.iter().find(|(_, pos)| near(pos)) {
            return Some(HandleHit::Scale(*handle));
        }
        let inside = self
            .pose
            .to_affine()
            .inverse()
            .is_some_and(|to_local| self.bounds.contains(to_local.apply(view.to_stage(p))));
        let near_corner = self.scale.iter().any(|(h, pos)| h.is_corner() && pos.distance(p) <= ROTATE_ZONE);
        (!inside && near_corner).then_some(HandleHit::Rotate)
    }

    fn cursor(&self, hit: HandleHit) -> CursorIcon {
        let HandleHit::Scale(handle) = hit else {
            return CursorIcon::Grab;
        };
        let Some((_, pos)) = self.scale.iter().find(|(h, _)| *h == handle) else {
            return CursorIcon::Default;
        };
        // Pick the resize arrow closest to the handle's direction from the
        // center *on screen*, so the arrows stay right on rotated layers.
        let direction = *pos - self.center;
        let degrees = direction.y.atan2(direction.x).to_degrees().rem_euclid(180.0);
        match degrees {
            d if !(22.5..157.5).contains(&d) => CursorIcon::ResizeHorizontal,
            d if d < 67.5 => CursorIcon::ResizeNwSe,
            d if d < 112.5 => CursorIcon::ResizeVertical,
            _ => CursorIcon::ResizeNeSw,
        }
    }

    fn draw(&self, painter: &egui::Painter) {
        painter.line_segment([self.top, self.knob], (1.0, SELECTION));
        painter.circle(self.knob, 4.5, Color32::WHITE, (1.5, SELECTION));
        for (_, pos) in &self.scale {
            let square = Rect::from_center_size(*pos, Vec2::splat(HANDLE_SIZE));
            painter.rect_filled(square, 0.0, Color32::WHITE);
            painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }
    }
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
    /// The path being drawn with the Pen tool, which lasts across many clicks.
    pen: Option<PenPath>,
    /// The anchor picked with the Subselection tool, whose handles are shown.
    anchor_selection: Option<AnchorRef>,
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
            pen: None,
            anchor_selection: None,
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

    pub fn is_drawing_path(&self) -> bool {
        self.pen.is_some()
    }

    /// Turns the pen path in progress into a new layer. Paths with fewer
    /// than two anchors are discarded.
    pub fn finish_path(&mut self, editor: &mut Editor, closed: bool) {
        let Some(path) = self.pen.take().and_then(|pen| pen.finish(closed)) else {
            return;
        };
        // A closed shape gets the full fill-and-stroke style. An open line
        // can't be filled, so it gets just a stroke, falling back to the
        // pencil's stroke if the shape style has none.
        let style = if closed {
            editor.draw_style
        } else {
            Style { fill: None, stroke: editor.draw_style.stroke.or(Some(editor.pencil_stroke)) }
        };
        add_path_layer(editor, "Path", path, style);
    }

    /// Backspace while drawing: take back the most recent anchor.
    pub fn remove_last_path_point(&mut self) {
        if let Some(pen) = &mut self.pen
            && !pen.remove_last()
        {
            self.pen = None;
        }
    }

    pub fn has_anchor_selection(&self) -> bool {
        self.anchor_selection.is_some()
    }

    /// Deletes the anchor picked with the Subselection tool, if the shape
    /// would still have enough points left.
    pub fn delete_selected_anchor(&mut self, editor: &mut Editor) {
        let Some(target) = self.anchor_selection.take() else { return };
        let Some(element) =
            editor.doc.layer_mut(target.layer).and_then(|l| l.elements.get_mut(target.element))
        else {
            return;
        };
        let (anchors, closed) = element.shape.to_anchors();
        if let Some(path) =
            remove_anchor(&anchors, target.index, closed).and_then(|a| path_from_anchors(&a, closed))
        {
            element.shape = Shape::Path(path);
        }
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

        // Switching away from the pen (or anything else that changes the tool)
        // finishes the path in progress rather than silently dropping it.
        if editor.tool != Tool::Pen && self.pen.is_some() {
            self.finish_path(editor, false);
        }
        self.validate_anchor_selection(editor);

        self.handle_tools(ui, &response, editor, view);
        if editor.tool == Tool::Pen {
            self.handle_pen(ui, &response, editor, view);
        }
        let handles = Handles::for_selection(editor, view);
        let overlay = painter.with_clip_rect(area);
        self.draw_overlays(&overlay, editor, view, handles.as_ref());
        self.draw_pen_preview(&overlay, view, response.hover_pos());
        if let Some(hit) = response.hover_pos().and_then(|p| self.outline_hit(editor, view, p)) {
            // Show where the new point would land: a hollow anchor with a plus.
            overlay.circle(hit.screen, 5.0, Color32::WHITE, (1.0, SELECTION));
            overlay
                .line_segment([hit.screen - vec2(3.0, 0.0), hit.screen + vec2(3.0, 0.0)], (1.0, SELECTION));
            overlay
                .line_segment([hit.screen - vec2(0.0, 3.0), hit.screen + vec2(0.0, 3.0)], (1.0, SELECTION));
        }
        self.update_cursor(ui, &response, editor, view, handles.as_ref());
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
        let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));

        if response.drag_started_by(PointerButton::Primary) {
            // Use where the button went *down*, not where the pointer is now:
            // egui only reports a drag once the pointer has moved a few
            // points, and those points matter when drawing precisely.
            let press = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos());
            let space_held = ui.input(|i| i.key_down(egui::Key::Space));
            if let Some(press) = press {
                self.gesture = self.begin_gesture(editor, press, view, space_held);
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
                Some(Gesture::Scale { layer, handle, start, bounds }) => {
                    let options = ScaleOptions { proportional: shift, from_pivot: alt };
                    editor.set_pose(*layer, scale_pose(start, *bounds, *handle, p, options));
                }
                Some(Gesture::Rotate { layer, drag }) => {
                    editor.set_pose(*layer, drag.update(p, shift));
                }
                Some(Gesture::EditAnchor { target, part, start, closed, grab, to_local }) => {
                    // Move the part by however far the pointer has moved, so
                    // grabbing slightly off-center doesn't make it jump.
                    let local = to_local.apply(p);
                    let goal = part.position(&start[target.index]) + (local - *grab);
                    let anchors = move_part(start, target.index, *part, goal, alt);
                    let element =
                        editor.doc.layer_mut(target.layer).and_then(|l| l.elements.get_mut(target.element));
                    if let (Some(element), Some(path)) = (element, path_from_anchors(&anchors, *closed)) {
                        // Rectangles and ovals become paths here, on the first
                        // real edit, rather than as soon as they're clicked.
                        element.shape = Shape::Path(path);
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
        // Clicks on a handle (which may be outside the shape) keep the selection.
        if response.clicked() && editor.tool == Tool::Subselect {
            if let Some(pointer) = response.interact_pointer_pos() {
                match self.anchor_hit(editor, view, pointer) {
                    Some((target, part)) => {
                        // Alt/Option-click converts between corner and smooth,
                        // as in Animate.
                        if alt && part == AnchorPart::Point {
                            convert_point(editor, target);
                        }
                        self.anchor_selection = Some(target);
                    }
                    None => {
                        let tolerance = HIT_TOLERANCE / view.zoom;
                        editor.selection =
                            editor.doc.hit_test(view.to_stage(pointer), editor.frame, tolerance);
                        self.anchor_selection = None;
                    }
                }
            }
        }
        if response.clicked() && editor.tool == Tool::Select {
            let on_handle =
                |pointer| Handles::for_selection(editor, view).and_then(|h| h.hit(pointer, view)).is_some();
            if let Some(pointer) = response.interact_pointer_pos().filter(|p| !on_handle(*p)) {
                let tolerance = HIT_TOLERANCE / view.zoom;
                editor.selection = editor.doc.hit_test(view.to_stage(pointer), editor.frame, tolerance);
            }
        }
    }

    fn begin_gesture(
        &mut self,
        editor: &mut Editor,
        press: Pos2,
        view: View,
        space_held: bool,
    ) -> Option<Gesture> {
        if space_held {
            return Some(Gesture::Pan);
        }
        let p = view.to_stage(press);
        match editor.tool {
            Tool::Select => {
                // Handles of the current selection take priority over
                // whatever shape happens to be underneath them.
                if let Some(handles) = Handles::for_selection(editor, view) {
                    let layer = handles.layer;
                    match handles.hit(press, view) {
                        Some(HandleHit::Scale(handle)) => {
                            let (start, bounds) = (handles.pose, handles.bounds);
                            return Some(Gesture::Scale { layer, handle, start, bounds });
                        }
                        Some(HandleHit::Rotate) => {
                            return Some(Gesture::Rotate { layer, drag: RotateDrag::new(handles.pose, p) });
                        }
                        None => {}
                    }
                }
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
            // The pen handles its own presses and drags; see `handle_pen`.
            Tool::Pen => None,
            Tool::Subselect => {
                let Some((target, part)) = self.anchor_hit(editor, view, press) else {
                    // Dragging off the anchors selects a layer, but doesn't move it.
                    let tolerance = HIT_TOLERANCE / view.zoom;
                    editor.selection = editor.doc.hit_test(p, editor.frame, tolerance);
                    self.anchor_selection = None;
                    return None;
                };
                self.anchor_selection = Some(target);
                let layer = editor.doc.layer(target.layer)?;
                let to_local = layer.transform_at(editor.frame).to_affine().inverse()?;
                let (start, closed) = layer.elements.get(target.element)?.shape.to_anchors();
                Some(Gesture::EditAnchor { target, part, start, closed, grab: to_local.apply(p), to_local })
            }
        }
    }

    /// Where a Pen click at `pointer` would add a point to the selected
    /// shape, if it's on the outline.
    ///
    /// Only applies when no new path is being drawn, and never right on an
    /// existing anchor: clicking there should do what clicking an anchor
    /// does, not stack a second point on top of it.
    fn outline_hit(&self, editor: &Editor, view: View, pointer: Pos2) -> Option<OutlineHit> {
        if editor.tool != Tool::Pen || self.pen.is_some() {
            return None;
        }
        let layer = editor.selected_layer().filter(|l| l.visible && !l.locked)?;
        let to_stage = layer.transform_at(editor.frame).to_affine();
        let local = to_stage.inverse()?.apply(view.to_stage(pointer));
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let mut best: Option<OutlineHit> = None;
        for (element, e) in layer.elements.iter().enumerate() {
            let (anchors, closed) = e.shape.to_anchors();
            let Some(hit) = nearest_on_anchors(&anchors, closed, local) else { continue };
            // Measure on screen, so the target is the same size at any zoom
            // and on layers that are scaled up or down.
            let on_screen = screen(hit.point);
            let distance = on_screen.distance(pointer);
            let on_anchor = anchors.iter().any(|a| screen(a.point).distance(pointer) <= ANCHOR_HIT);
            let closer = best.is_none_or(|b| distance < b.screen.distance(pointer));
            if distance <= ANCHOR_HIT && !on_anchor && closer {
                best = Some(OutlineHit {
                    layer: layer.id,
                    element,
                    segment: hit.segment,
                    t: hit.t,
                    screen: on_screen,
                });
            }
        }
        best
    }

    /// Drops the anchor selection if it no longer points at something that
    /// exists (after undo, deleting points, or selecting another layer), or
    /// if the Subselection tool isn't active.
    fn validate_anchor_selection(&mut self, editor: &Editor) {
        let Some(target) = self.anchor_selection else { return };
        let still_valid = editor.tool == Tool::Subselect
            && editor.selection == Some(target.layer)
            && editor
                .doc
                .layer(target.layer)
                .and_then(|l| l.elements.get(target.element))
                .is_some_and(|e| target.index < e.shape.to_anchors().0.len());
        if !still_valid {
            self.anchor_selection = None;
        }
    }

    /// The selected layer's anchors and handles as they appear on screen:
    /// `(target, part, screen position)` for everything that can be dragged.
    ///
    /// Every anchor point is listed, but handles only for the selected
    /// anchor, matching what's drawn. Handles come first, so when one sits
    /// on top of an anchor, the handle wins.
    fn anchor_targets(&self, editor: &Editor, view: View) -> Vec<(AnchorRef, AnchorPart, Pos2)> {
        let Some(layer) = editor.selected_layer().filter(|l| l.visible && !l.locked) else {
            return Vec::new();
        };
        let to_stage = layer.transform_at(editor.frame).to_affine();
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let mut handles = Vec::new();
        let mut points = Vec::new();
        for (element, e) in layer.elements.iter().enumerate() {
            for (index, anchor) in e.shape.to_anchors().0.iter().enumerate() {
                let target = AnchorRef { layer: layer.id, element, index };
                points.push((target, AnchorPart::Point, screen(anchor.point)));
                if self.anchor_selection == Some(target) {
                    for part in [AnchorPart::HandleIn, AnchorPart::HandleOut] {
                        if part.position(anchor) != anchor.point {
                            handles.push((target, part, screen(part.position(anchor))));
                        }
                    }
                }
            }
        }
        handles.extend(points);
        handles
    }

    fn anchor_hit(&self, editor: &Editor, view: View, pointer: Pos2) -> Option<(AnchorRef, AnchorPart)> {
        self.anchor_targets(editor, view)
            .into_iter()
            .find(|(_, _, pos)| pos.distance(pointer) <= ANCHOR_HIT)
            .map(|(target, part, _)| (target, part))
    }

    /// Draws the selected layer's outlines, anchor squares, and the handles
    /// of the selected anchor.
    fn draw_anchor_overlay(&self, painter: &egui::Painter, editor: &Editor, view: View) {
        let Some(layer) = editor.selected_layer().filter(|l| l.visible) else { return };
        let to_stage = layer.transform_at(editor.frame).to_affine();
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        for (element, e) in layer.elements.iter().enumerate() {
            let (anchors, closed) = e.shape.to_anchors();
            if let Some(path) = path_from_anchors(&anchors, closed) {
                painter.line(path.flatten().into_iter().map(screen).collect(), (1.0, SELECTION));
            }
            for (index, anchor) in anchors.iter().enumerate() {
                let selected = self.anchor_selection == Some(AnchorRef { layer: layer.id, element, index });
                let point = screen(anchor.point);
                if selected {
                    // Handles: a line back to the anchor and a round knob.
                    for handle in [anchor.handle_in, anchor.handle_out] {
                        if handle != anchor.point {
                            painter.line_segment([point, screen(handle)], (1.0, SELECTION));
                            painter.circle_filled(screen(handle), 3.5, SELECTION);
                        }
                    }
                }
                let square = Rect::from_center_size(point, Vec2::splat(7.0));
                painter.rect_filled(square, 0.0, if selected { SELECTION } else { Color32::WHITE });
                painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
            }
        }
    }

    /// Pen tool input. Each press places an anchor; dragging before release
    /// pulls out its curve handles. Pressing on the first anchor closes the
    /// path, and pressing on the last one again ends it open.
    fn handle_pen(&mut self, ui: &egui::Ui, response: &egui::Response, editor: &mut Editor, view: View) {
        let (pressed, button_down, space_held, shift) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.key_down(egui::Key::Space),
                i.modifiers.shift,
            )
        });
        let pointer = response.interact_pointer_pos().map(|p| view.to_stage(p));

        // Before starting a new path, a click on the selected shape's outline
        // adds a point to it instead, as in Animate and Illustrator.
        if pressed
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(hit) = response.interact_pointer_pos().and_then(|p| self.outline_hit(editor, view, p))
        {
            insert_point(editor, hit);
            return;
        }

        if pressed
            && response.is_pointer_button_down_on()
            && !space_held
            && let Some(p) = pointer
        {
            let tolerance = ANCHOR_HIT / view.zoom;
            match &mut self.pen {
                Some(pen) if pen.can_close_at(p, tolerance) => return self.finish_path(editor, true),
                Some(pen) if pen.is_on_last(p, tolerance) => return self.finish_path(editor, false),
                Some(pen) => pen.add_anchor(p, shift),
                None => self.pen = Some(PenPath::new(p)),
            }
        }

        if let Some(pen) = &mut self.pen
            && pen.is_dragging()
        {
            if let Some(p) = pointer {
                pen.drag_handle(p, MIN_HANDLE_DRAG / view.zoom);
            }
            if !button_down {
                pen.release();
            }
        }
    }

    /// Draws the pen path in progress: the curve so far, a rubber-band line
    /// to the pointer, the anchors, and the handles of the newest anchor.
    fn draw_pen_preview(&self, painter: &egui::Painter, view: View, hover: Option<Pos2>) {
        let Some(pen) = &self.pen else { return };
        let to_screen =
            |path: &Path| path.flatten().into_iter().map(|p| view.to_screen(p)).collect::<Vec<_>>();

        if let Some(path) = pen.finish(false) {
            painter.line(to_screen(&path), (1.5, SELECTION));
        }
        let hover_stage = hover.map(|p| view.to_stage(p));
        let closing = hover_stage.is_some_and(|p| pen.can_close_at(p, ANCHOR_HIT / view.zoom));
        if !pen.is_dragging()
            && let Some(preview) = hover_stage.and_then(|p| pen.preview_to(p))
        {
            painter.line(to_screen(&preview), (1.0, SELECTION.gamma_multiply(0.6)));
        }

        let newest = pen.anchors().last().expect("never empty");
        for handle in [newest.handle_in, newest.handle_out] {
            if handle != newest.point {
                let (a, h) = (view.to_screen(newest.point), view.to_screen(handle));
                painter.line_segment([a, h], (1.0, SELECTION));
                painter.circle_filled(h, 3.0, SELECTION);
            }
        }

        for (i, anchor) in pen.anchors().iter().enumerate() {
            // The first anchor fills in when hovering it would close the path.
            let fill = if i == 0 && closing { SELECTION } else { Color32::WHITE };
            let square = Rect::from_center_size(view.to_screen(anchor.point), Vec2::splat(7.0));
            painter.rect_filled(square, 0.0, fill);
            painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }
    }

    fn draw_overlays(&self, painter: &egui::Painter, editor: &Editor, view: View, handles: Option<&Handles>) {
        // The path tools show the selected shape's own outline and points;
        // the others outline the layer's bounds, following its rotation and scale.
        let shows_points = matches!(editor.tool, Tool::Subselect | Tool::Pen);
        if shows_points {
            self.draw_anchor_overlay(painter, editor, view);
        }
        if let Some(layer) = editor.selected_layer().filter(|l| l.visible && !shows_points) {
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
        if let Some(handles) = handles {
            handles.draw(painter);
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

    fn update_cursor(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &Editor,
        view: View,
        handles: Option<&Handles>,
    ) {
        if !response.hovered() && self.gesture.is_none() {
            return;
        }
        let space_held = ui.input(|i| i.key_down(egui::Key::Space));
        let handle_cursor = |hit| handles.map_or(CursorIcon::Default, |h| h.cursor(hit));

        let icon = match &self.gesture {
            Some(Gesture::Pan) => CursorIcon::Grabbing,
            _ if space_held => CursorIcon::Grab,
            Some(Gesture::Move { .. }) => CursorIcon::Move,
            Some(Gesture::Rotate { .. }) => CursorIcon::Grabbing,
            Some(Gesture::EditAnchor { .. }) => CursorIcon::Move,
            Some(Gesture::Scale { handle, .. }) => handle_cursor(HandleHit::Scale(*handle)),
            Some(Gesture::DrawShape { .. } | Gesture::Pencil { .. }) => CursorIcon::Crosshair,
            None => {
                let hovered = handles.zip(response.hover_pos()).and_then(|(h, p)| h.hit(p, view));
                match (hovered, editor.tool) {
                    (Some(hit), _) => handle_cursor(hit),
                    (None, Tool::Select) => CursorIcon::Default,
                    (None, Tool::Subselect) => {
                        let on_anchor =
                            response.hover_pos().is_some_and(|p| self.anchor_hit(editor, view, p).is_some());
                        if on_anchor { CursorIcon::PointingHand } else { CursorIcon::Default }
                    }
                    (None, Tool::Pen) => {
                        let can_close =
                            self.pen.as_ref().zip(response.hover_pos()).is_some_and(|(pen, p)| {
                                pen.can_close_at(view.to_stage(p), ANCHOR_HIT / view.zoom)
                            });
                        let on_outline =
                            response.hover_pos().is_some_and(|p| self.outline_hit(editor, view, p).is_some());
                        if can_close {
                            CursorIcon::PointingHand
                        } else if on_outline {
                            // The "copy" cursor shows a plus on every
                            // platform, the closest standard match for "add point".
                            CursorIcon::Copy
                        } else {
                            CursorIcon::Crosshair
                        }
                    }
                    (None, Tool::Rectangle | Tool::Ellipse | Tool::Pencil) => CursorIcon::Crosshair,
                }
            }
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
            let Some(path) = fit_freehand_path(&points, tolerance) else {
                return;
            };
            let style = Style { fill: None, stroke: Some(editor.pencil_stroke) };
            add_path_layer(editor, "Stroke", path, style);
        }
        Gesture::Move { .. }
        | Gesture::Scale { .. }
        | Gesture::Rotate { .. }
        | Gesture::EditAnchor { .. }
        | Gesture::Pan => {}
    }
}

fn add_layer(editor: &mut Editor, prefix: &str, element: Element, position: Point) {
    let name = editor.doc.next_layer_name(prefix);
    let id = editor.doc.add_layer(name, vec![element], editor.frame, Transform::at(position));
    editor.selection = Some(id);
}

/// Switches an anchor between corner and smooth. Rectangles and ovals
/// become paths.
fn convert_point(editor: &mut Editor, target: AnchorRef) {
    let Some(element) = editor.doc.layer_mut(target.layer).and_then(|l| l.elements.get_mut(target.element))
    else {
        return;
    };
    let (anchors, closed) = element.shape.to_anchors();
    let anchors = toggle_smooth(&anchors, target.index, closed);
    if let Some(path) = path_from_anchors(&anchors, closed) {
        element.shape = Shape::Path(path);
    }
}

/// Adds a point to a shape's outline. Rectangles and ovals become paths.
fn insert_point(editor: &mut Editor, hit: OutlineHit) {
    let Some(element) = editor.doc.layer_mut(hit.layer).and_then(|l| l.elements.get_mut(hit.element)) else {
        return;
    };
    let (anchors, closed) = element.shape.to_anchors();
    let anchors = insert_anchor(&anchors, closed, hit.segment, hit.t);
    if let Some(path) = path_from_anchors(&anchors, closed) {
        element.shape = Shape::Path(path);
    }
}

/// Adds a layer for a path drawn in stage coordinates.
///
/// The path is recentered on its own middle first, so the layer's pivot
/// (the point it rotates and scales around) sits in the middle of the
/// drawing rather than at the stage's top-left corner.
fn add_path_layer(editor: &mut Editor, prefix: &str, mut path: Path, style: Style) {
    let Some(bounds) = StageRect::bounding(path.flatten()) else {
        return;
    };
    let center = bounds.center();
    path.translate(-center);
    add_layer(editor, prefix, Element { shape: Shape::Path(path), style }, center);
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
        let Shape::Path(stroke) = &layer.elements[0].shape else { panic!("not a path") };
        assert!(stroke.segments.len() >= 2, "a wavy stroke should keep several segments");
        // The path was recentered, so its local bounds straddle the origin.
        let bounds = layer.local_bounds().unwrap();
        assert!(bounds.center().distance(Point::ZERO) < 30.0);
    }

    /// An editor with a selected 200×200 box centered at (400, 400).
    fn editor_with_selected_box() -> (Editor, LayerId) {
        let mut editor = Editor::new(Document::default());
        let square = Element {
            shape: Shape::Rect { width: 200.0, height: 200.0, corner_radius: 0.0 },
            style: editor.draw_style,
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
        let element = &editor.doc.layers[0].elements[0];
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
        let element = &editor.doc.layers[0].elements[0];
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
        harness.state().1.doc.layer(id).unwrap().elements[0].shape.clone()
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
        editor.doc.layer_mut(id).unwrap().elements[0].shape =
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
