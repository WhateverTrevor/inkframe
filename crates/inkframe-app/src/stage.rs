//! The stage: the canvas where the animation is displayed and drawn on.
//!
//! The document is rendered by `inkframe-render` into a texture, so the stage
//! shows exactly what an export would produce. Editing feedback (selection
//! outlines, shapes being dragged out) is painted on top with egui, because
//! it's interface rather than artwork.

use egui::{Align2, Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{
    Affine, Anchor, Bend, BrushMode, BrushStroke, Canvas, CanvasId, Color, Combine, Document, Element,
    LayerId, MotionCurve, Path, Point, Rect as StageRect, Retouch, RetouchStroke, Selection, Shape, Style,
    Text, TimelineId, Transform, fit_freehand_path, insert_anchor, nearest_on_anchors, path_from_anchors,
};
use inkframe_render::{RenderOptions, output_size, render_timeline_into};

use crate::convert::{to_color32, to_point};
use crate::editor::{Editor, OnionSkin, Surrounding, Tool};
use crate::guides;
use crate::handles::{RotateDrag, ScaleHandle, ScaleOptions, scale_pose, skew_pose};
use crate::motion_path;
use crate::pen::PenPath;
use crate::subselect::{AnchorPart, move_part, remove_anchor, toggle_smooth};
use crate::tablet;
use crate::text_edit::{TextCursor, caret_position, position_at, selection_boxes, word_at};

/// Gap kept around the stage when it's zoomed to fit the window, in points.
const FIT_MARGIN: f32 = 24.0;
/// Largest stage texture we'll render. Beyond this (when zoomed far in) the
/// texture is stretched instead, which looks soft but keeps memory sane.
const MAX_TEXTURE_SIDE: f32 = 4096.0;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 64.0;
/// How close to the pivot a press grabs it, in screen points.
const PIVOT_GRAB: f32 = 6.0;
/// How close to a selection box edge a press skews instead of moving, in
/// screen points.
const SKEW_REACH: f32 = 3.0;
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

const SELECTION: Color32 = crate::theme::ACCENT;
const ONION_BEFORE: inkframe_core::Color = inkframe_core::Color::rgb(0x30, 0x60, 0xff);
const ONION_AFTER: inkframe_core::Color = inkframe_core::Color::rgb(0x20, 0xb0, 0x40);
/// How strongly the rest of the scene shows while a symbol is edited in place.
const SURROUNDINGS_OPACITY: f32 = 0.3;

/// Maps between screen points and the "local" coordinates of the timeline
/// being edited.
///
/// On the main timeline, local coordinates are stage pixels. Inside a
/// symbol they're the symbol's own, which reach the stage through
/// `timeline_to_stage`: a shift to mid-stage for a symbol opened on its own,
/// or the instance's full transform (possibly rotated and scaled) when
/// editing in place. Because every tool works in local coordinates through
/// this one type, none of them need to know whether they're inside a symbol.
#[derive(Debug, Clone, Copy)]
struct View {
    /// Where the middle of the stage is on screen.
    center: Pos2,
    /// The stage's size, in its own pixels.
    size: Vec2,
    /// Screen points per stage pixel.
    stage_zoom: f32,
    /// How far the view is turned, clockwise, in radians: the stage spins
    /// about its middle, as when turning paper to draw a curve
    /// comfortably. The drawing itself isn't changed.
    rotation: f32,
    timeline_to_stage: Affine,
    stage_to_timeline: Affine,
    /// Screen points per local unit, for sizing hit targets and strokes.
    /// For a scaled instance this folds in its (average) scale.
    zoom: f32,
}

impl View {
    /// A view of the main timeline.
    fn stage(center: Pos2, size: Vec2, stage_zoom: f32, rotation: f32) -> Self {
        Self {
            center,
            size,
            stage_zoom,
            rotation,
            timeline_to_stage: Affine::IDENTITY,
            stage_to_timeline: Affine::IDENTITY,
            zoom: stage_zoom,
        }
    }

    /// The same view, for a timeline placed on the stage by `to_stage`.
    /// A transform that can't be inverted (an instance scaled to nothing)
    /// can't be edited through, so the plain stage view is kept instead.
    fn with_timeline(self, to_stage: Affine) -> Self {
        let Some(from_stage) = to_stage.inverse() else {
            return self;
        };
        // The determinant is how much the transform scales area, so its
        // square root is the average scale along each axis.
        let scale = (to_stage.a * to_stage.d - to_stage.b * to_stage.c).abs().sqrt();
        Self {
            timeline_to_stage: to_stage,
            stage_to_timeline: from_stage,
            zoom: self.stage_zoom * scale,
            ..self
        }
    }

    fn to_screen(self, p: Point) -> Pos2 {
        let on_stage = self.timeline_to_stage.apply(p);
        self.center
            + rotate(
                vec2(on_stage.x, on_stage.y) * self.stage_zoom - self.size * self.stage_zoom / 2.0,
                self.rotation,
            )
    }

    fn to_local(self, p: Pos2) -> Point {
        let unturned = rotate(p - self.center, -self.rotation) / self.stage_zoom + self.size / 2.0;
        self.stage_to_timeline.apply(Point::new(unturned.x, unturned.y))
    }

    /// The stage's corners on screen, clockwise from its top left.
    fn stage_corners(self) -> [Pos2; 4] {
        let (w, h) = (self.size.x, self.size.y);
        [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| self.stage_to_screen(Point::new(x, y)))
    }

    /// Where a point on the stage (rather than in the timeline being
    /// edited) is on screen.
    fn stage_to_screen(self, p: Point) -> Pos2 {
        View { timeline_to_stage: Affine::IDENTITY, ..self }.to_screen(p)
    }
}

/// The angle of `p` around `center`, clockwise from pointing right, in
/// degrees.
fn angle_around(center: Pos2, p: Pos2) -> f32 {
    (p.y - center.y).atan2(p.x - center.x).to_degrees()
}

/// An angle in degrees, brought into −180 to 180.
fn normalize_degrees(angle: f32) -> f32 {
    (angle + 180.0).rem_euclid(360.0) - 180.0
}

/// `v` turned clockwise (on screen, where y points down) by `angle` radians.
fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// An in-progress mouse interaction on the stage.
/// What a drag with the Camera tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CameraDrag {
    Pan,
    Turn,
    Zoom,
}

enum Gesture {
    /// Dragging the selected layers, each from where it started. Each
    /// position is in its layer's parent's space, with the map there from
    /// the timeline's coordinates, which `grab` is in.
    Move { layers: Vec<(LayerId, Point, Affine)>, grab: Point },
    /// Dragging a selection box over empty stage. `adding` keeps what was
    /// already selected (Shift held).
    Marquee { start: Point, current: Point, adding: bool },
    /// Dragging out a new rectangle or ellipse.
    DrawShape { tool: Tool, start: Point, current: Point },
    /// Drawing a freehand stroke.
    Pencil { points: Vec<Point>, pressures: Vec<f32> },
    /// Dragging a scale handle. `start` is the pose when the drag began;
    /// every update is computed from it, so errors never accumulate.
    /// With several layers selected, `start` is the imaginary layer around
    /// them and `group` the real ones with their starting poses.
    Scale {
        layer: LayerId,
        handle: ScaleHandle,
        start: Transform,
        bounds: StageRect,
        group: Vec<GroupMember>,
        /// Timeline coordinates to the space `start` is in (the parent's).
        into_space: Affine,
    },
    /// Dragging an edge of the selection box to slant the layer. `grab` is
    /// where the drag started, in the space `start` is in.
    Skew {
        layer: LayerId,
        edge: ScaleHandle,
        start: Transform,
        bounds: StageRect,
        grab: Point,
        into_space: Affine,
    },
    /// Dragging the rotation knob or a corner's rotate zone.
    Rotate { layer: LayerId, drag: RotateDrag, group: Vec<GroupMember>, into_space: Affine },
    /// Dragging a layer's pivot to a new spot on the layer.
    Pivot { layer: LayerId },
    /// Moving the camera with the Camera tool: panning, or with Shift
    /// turning, or with Alt zooming. `grab` is where the drag began, on
    /// screen, and `start` the camera's pose then.
    Camera { camera: LayerId, how: CameraDrag, grab: Pos2, start: Transform },
    /// Dragging shape hint `index` of one of a layer's drawings along its
    /// outline.
    Hint { layer: LayerId, drawing: usize, index: usize },
    /// Posing a rig with the Bone tool: the joints above `layer` turn so
    /// the point grabbed on it (`local`, in its own coordinates) follows
    /// the pointer.
    Reach { layer: LayerId, local: Point },
    /// Bending a deformer with the Bone tool: bone `bone` of `layer`'s
    /// deformer turns to point at the pointer.
    Bend { layer: LayerId, bone: usize },
    /// Widening or narrowing a pencil line at one of its points.
    Width { layer: LayerId, place: LinePoint },
    /// Dragging control point `point` of `layer`'s free-form deformer.
    Warp { layer: LayerId, point: usize },
    /// Dragging selected pixels with the Selection tool. They're lifted off
    /// (`floating`), leaving `base` behind, and put down at the pointer,
    /// moved by whole pixels from `grab` (canvas pixels).
    MovePixels {
        layer: LayerId,
        canvas: CanvasId,
        size: (u32, u32),
        base: Canvas,
        floating: Canvas,
        selection: Selection,
        grab: Point,
    },
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
    /// Dragging a layer's motion path to bend the tween under the pointer.
    /// `segment` is the tween as it was when the drag began.
    BendPath { layer: LayerId, segment: motion_path::Segment, s: f32 },
    /// Dragging one curve handle of a motion path: the one leaving (`out`)
    /// or arriving at the position key on `frame`.
    PathHandle { layer: LayerId, frame: u32, out: bool },
    /// Space + drag scrolls the view, like the hand tool in Animate.
    Pan,
    /// Shift + Space + drag turns the view around the stage's middle, as
    /// Photoshop's Rotate View does: from `from` (the pointer's angle
    /// around it, in degrees, when the drag began) and the rotation then.
    RotateView { from: f32, start: f32 },
}

/// A brush or eraser stroke being painted.
struct Painting {
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

/// A selection being drawn, in screen points, and how it'll combine with
/// the selection already there.
enum Selecting {
    /// A rectangle or ellipse, dragged from `start`.
    Marquee { start: Pos2, current: Pos2, combine: Combine },
    /// The freehand lasso's path, while the button is held.
    Lasso { points: Vec<Pos2>, combine: Combine },
    /// The polygonal lasso's corners, one per click.
    Polygon { points: Vec<Pos2>, combine: Combine },
}

/// What a painting gesture is doing.
enum PaintWork {
    /// The brush or eraser.
    Brush(BrushStroke),
    /// Smudge, Blur, Sharpen, Dodge, Burn or the Clone Stamp.
    Retouch(RetouchStroke),
    /// Dragging out a gradient, from where the drag began; it's laid down
    /// when the pen lifts.
    Gradient { from: Pos2 },
}

/// Something on a motion path under the pointer.
enum PathHit {
    Handle { frame: u32, out: bool },
    Segment { segment: motion_path::Segment, s: f32 },
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
    /// The pivot: the joint the layer turns and scales around.
    Pivot,
    /// An edge of the box, between its handles, which slants the layer.
    /// Named by the edge's middle handle.
    Skew(ScaleHandle),
}

/// Screen positions of the transform handles around the selected layer.
///
/// Rebuilt every UI frame from the layer's current pose, so the handles
/// follow the layer through tweens, undo and edits in the inspector.
struct Handles {
    layer: LayerId,
    /// With several layers selected: each one, its pose on this frame, and
    /// its parents' placement. The handles then go around them all, as an
    /// imaginary layer (`pose`) that the real ones follow; see
    /// `transform_group`.
    group: Vec<GroupMember>,
    pose: Transform,
    /// Maps the space `pose` is in (the layer's parent's) to the timeline's.
    space: Affine,
    /// The layer's bounds, measured from its pivot (the point `pose`
    /// places), which is what the scale and rotate math works in.
    bounds: StageRect,
    pivot: Pos2,
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
        let frame = editor.frame;
        let editable: Vec<&inkframe_core::Layer> = editor
            .selected_ids()
            .into_iter()
            .filter_map(|id| editor.doc.layer(id))
            .filter(|l| l.is_shown_at(frame) && !l.locked)
            .collect();
        // A layer hanging from another selected one already follows it, so
        // transforming it as well would move it twice.
        let ids: Vec<LayerId> = editable.iter().map(|l| l.id).collect();
        let editable: Vec<_> =
            editable.into_iter().filter(|l| !ids.iter().any(|&a| editor.doc.hangs_from(l, a))).collect();
        let (layer, group, bounds, pose, space) = match editable[..] {
            [] => return None,
            [layer] => {
                let bounds = editor.doc.layer_bounds(layer, frame)?;
                let from_pivot = StageRect { min: bounds.min - layer.pivot, max: bounds.max - layer.pivot };
                let space = editor.doc.parent_matrix(layer, frame);
                (layer, Vec::new(), from_pivot, editor.doc.pose(layer, frame), space)
            }
            // Several: an upright box around them all, centered on its
            // own middle, standing in for a layer that holds them.
            [.., last] => {
                let boxes = editable.iter().filter_map(|l| {
                    let to_stage = editor.doc.layer_matrix(l, frame);
                    StageRect::bounding(
                        editor.doc.layer_bounds(l, frame)?.corners().map(|c| to_stage.apply(c)),
                    )
                });
                let area = boxes.reduce(StageRect::union)?;
                let group = editable
                    .iter()
                    .map(|l| GroupMember {
                        id: l.id,
                        pose: editor.doc.pose(l, frame),
                        space: editor.doc.parent_matrix(l, frame),
                    })
                    .collect();
                let bounds = StageRect::from_center_size(Point::ZERO, area.width(), area.height());
                (last, group, bounds, Transform::at(area.center()), Affine::IDENTITY)
            }
        };
        let to_stage = space.pre_concat(pose.to_affine());
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
            pivot: view.to_screen(space.apply(pose.position)),
            group,
            pose,
            space,
            bounds,
            scale: ScaleHandle::ALL.map(|h| (h, screen(h.local_position(bounds)))),
            center,
            top,
            knob: top + up * ROTATE_KNOB_DISTANCE,
        })
    }

    /// `alt` is whether Option/Alt is held, which makes the pivot grabbable.
    fn hit(&self, p: Pos2, view: View, alt: bool) -> Option<HandleHit> {
        // The pivot starts in the middle of the layer, right where people
        // grab it to move it, so it only takes the press with Option/Alt
        // held. Then it comes first, since it's usually inside the layer.
        if alt && self.group.is_empty() && p.distance(self.pivot) <= PIVOT_GRAB {
            return Some(HandleHit::Pivot);
        }
        if p.distance(self.knob) <= HANDLE_SIZE {
            return Some(HandleHit::Rotate);
        }
        // Handles get a slightly bigger target than they look, so they're
        // easy to grab.
        let near = |pos: &Pos2| (*pos - p).abs().max_elem() <= HANDLE_SIZE * 0.75;
        if let Some((handle, _)) = self.scale.iter().find(|(_, pos)| near(pos)) {
            return Some(HandleHit::Scale(*handle));
        }
        // Along an edge between the handles: skew, as in Animate's Free
        // Transform. One layer at a time, like the skew setting itself.
        if self.group.is_empty() {
            let corner = |x, y| self.scale.iter().find(|(h, _)| h.x == x && h.y == y).map(|(_, pos)| *pos);
            let edges = [
                ((0, -1), (-1, -1), (1, -1)),
                ((1, 0), (1, -1), (1, 1)),
                ((0, 1), (1, 1), (-1, 1)),
                ((-1, 0), (-1, 1), (-1, -1)),
            ];
            for ((x, y), from, to) in edges {
                if let (Some(a), Some(b)) = (corner(from.0, from.1), corner(to.0, to.1))
                    && distance_to_segment(p, a, b) <= SKEW_REACH
                {
                    return Some(HandleHit::Skew(ScaleHandle { x, y }));
                }
            }
        }
        let inside = self
            .space
            .pre_concat(self.pose.to_affine())
            .inverse()
            .is_some_and(|to_local| self.bounds.contains(to_local.apply(view.to_local(p))));
        let near_corner = self.scale.iter().any(|(h, pos)| h.is_corner() && pos.distance(p) <= ROTATE_ZONE);
        (!inside && near_corner).then_some(HandleHit::Rotate)
    }

    fn cursor(&self, hit: HandleHit) -> CursorIcon {
        let handle = match hit {
            HandleHit::Scale(handle) => handle,
            HandleHit::Rotate => return CursorIcon::Grab,
            HandleHit::Pivot => return CursorIcon::Crosshair,
            // Top and bottom edges slide sideways; the sides slide up and down.
            HandleHit::Skew(edge) if edge.y != 0 => return CursorIcon::ResizeColumn,
            HandleHit::Skew(_) => return CursorIcon::ResizeRow,
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

/// The safe areas and field guide, if they're on, over the stage (the
/// camera's frame).
fn draw_guides(painter: &egui::Painter, editor: &Editor, view: View) {
    const SAFE: Color32 = Color32::from_rgb(0x4c, 0xc9, 0xf0);
    const FIELD: Color32 = Color32::from_rgb(0x5a, 0x78, 0xc8);
    let frame = editor.doc.stage_rect();
    let at = |p: Point| view.stage_to_screen(p);

    if editor.guides.field_guide {
        let (lines, labels) = guides::field_guide(frame);
        for line in &lines {
            let (width, alpha) = match line.weight {
                guides::Weight::Faint => (1.0, 0.14),
                guides::Weight::Normal => (1.0, 0.45),
                guides::Weight::Strong => (1.5, 0.8),
            };
            painter.line_segment([at(line.from), at(line.to)], (width, FIELD.gamma_multiply(alpha)));
        }
        // The small fields' labels crowd together when zoomed out; each is
        // shown only if there's room before the next one out.
        for (i, label) in labels.iter().enumerate() {
            let next = labels.get(i + 1).filter(|next| next.text.ends_with('F'));
            if next.is_some_and(|next| at(label.at).distance(at(next.at)) < 18.0) {
                continue;
            }
            let anchor = at(label.at) + label.anchor.to_sign() * -3.0;
            guide_label(painter, anchor, label.anchor, &label.text, FIELD);
        }
    }

    // The two safe areas are only a few pixels apart when zoomed out, so
    // their names go in opposite corners: action safe top right, title
    // safe bottom left (clear of the field guide's labels, top left).
    let safe = guides::safe_areas(frame, editor.guides.safe_areas);
    for (i, (rect, name)) in safe.iter().enumerate() {
        let corners = rect.corners().map(at);
        painter.add(egui::Shape::dashed_line(
            &[corners.as_slice(), &[corners[0]]].concat(),
            (1.0, SAFE),
            6.0,
            4.0,
        ));
        let (corner, anchor) =
            if i == 0 { (corners[1], Align2::RIGHT_TOP) } else { (corners[3], Align2::LEFT_BOTTOM) };
        guide_label(painter, corner + anchor.to_sign() * -4.0, anchor, name, SAFE);
    }
}

/// A guide's label, on a soft backing so it reads over the artwork.
fn guide_label(painter: &egui::Painter, at: Pos2, anchor: Align2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), color);
    let rect = anchor.anchor_size(at, galley.size());
    painter.rect_filled(rect.expand(2.0), 3.0, Color32::from_white_alpha(170));
    painter.galley(rect.min, galley, color);
}

/// The Camera tool's view of things: the stage's edge, which is the
/// camera's frame, and how far it's zoomed and turned.
fn draw_camera_frame(painter: &egui::Painter, editor: &Editor, view: View) {
    const CAMERA: Color32 = Color32::from_rgb(0xff, 0xa0, 0x3c);
    let corners = editor.doc.stage_rect().corners().map(|c| view.to_screen(c));
    painter.add(egui::Shape::closed_line(corners.to_vec(), (2.0, CAMERA)));
    let label = match editor.doc.camera_layer() {
        Some(camera) if !camera.visible => "Camera off: show its layer to use it".to_owned(),
        Some(camera) => {
            let pose = editor.doc.pose(camera, editor.frame);
            format!("Camera  {:.0}%  {:.0}°", pose.scale.x * 100.0, pose.rotation)
        }
        None => "Drag to add a camera and pan it; Shift turns, Alt zooms".to_owned(),
    };
    let at = corners[0] + vec2(6.0, 6.0);
    let galley = painter.layout_no_wrap(label, egui::FontId::proportional(12.0), Color32::BLACK);
    let back = Rect::from_min_size(at, galley.size()).expand(4.0);
    painter.rect_filled(back, 3.0, CAMERA);
    painter.galley(at, galley, Color32::BLACK);
}

/// Where a point of a pencil line is: in a layer's showing drawing, its
/// line art or colour art, which shape, and which of its anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinePoint {
    colour_art: bool,
    element: usize,
    anchor: usize,
}

/// How near a pencil line's point the pointer must be to grab it, in points.
const LINE_POINT_GRAB: f32 = 14.0;

/// Every pencil line's points on screen in the timeline being edited, the
/// selected layer's first, with each point's width on screen.
fn line_points(editor: &Editor, view: View) -> Vec<(LayerId, LinePoint, Pos2, f32)> {
    let frame = editor.frame;
    let mut layers: Vec<&inkframe_core::Layer> =
        editor.layers().iter().filter(|l| l.is_shown_at(frame) && !l.locked).collect();
    layers.sort_by_key(|l| !editor.is_selected(l.id));
    let mut found = Vec::new();
    for layer in layers {
        let to_stage = editor.doc.layer_matrix(layer, frame);
        let scale = to_stage.average_scale() * view.zoom;
        let drawing = &layer.drawings[layer.drawing_index_at(frame)];
        for (colour_art, list) in [(false, &drawing.elements), (true, &drawing.colour_art)] {
            for (element, e) in list.iter().enumerate() {
                let Shape::Line { path, widths } = &e.shape else { continue };
                for (anchor, at) in inkframe_core::centreline_anchors(path).into_iter().enumerate() {
                    let width = widths.get(anchor).copied().unwrap_or(1.0) * scale;
                    found.push((
                        layer.id,
                        LinePoint { colour_art, element, anchor },
                        view.to_screen(to_stage.apply(at)),
                        width,
                    ));
                }
            }
        }
    }
    found
}

/// The pencil line point under `pointer`: the nearest within reach.
fn line_point_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, LinePoint)> {
    line_points(editor, view)
        .into_iter()
        .filter(|(.., at, _)| at.distance(pointer) <= LINE_POINT_GRAB)
        .min_by(|a, b| a.2.distance(pointer).total_cmp(&b.2.distance(pointer)))
        .map(|(layer, place, ..)| (layer, place))
}

/// The pencil line a `LinePoint` is on, to change its widths.
fn line_shape_mut(editor: &mut Editor, layer: LayerId, place: LinePoint) -> Option<&mut Shape> {
    let frame = editor.frame;
    let layer = editor.doc.layer_mut(layer)?;
    let index = layer.drawing_index_at(frame);
    let drawing = &mut layer.drawings[index];
    let list = if place.colour_art { &mut drawing.colour_art } else { &mut drawing.elements };
    Some(&mut list.get_mut(place.element)?.shape)
}

/// For the Width tool: each pencil line's points as small dots, and at
/// the point under the pointer, a bar across the line as wide as it is
/// there, which is what dragging changes.
fn draw_line_widths(painter: &egui::Painter, editor: &Editor, view: View) {
    let points = line_points(editor, view);
    for (.., at, _) in &points {
        painter.circle_filled(*at, 2.0, SELECTION.gamma_multiply(0.6));
    }
    let pointer = painter.ctx().pointer_hover_pos();
    let Some(i) = pointer.and_then(|pointer| {
        (0..points.len())
            .filter(|&i| points[i].2.distance(pointer) <= LINE_POINT_GRAB)
            .min_by(|&a, &b| points[a].2.distance(pointer).total_cmp(&points[b].2.distance(pointer)))
    }) else {
        return;
    };
    let (layer, place, at, width) = points[i];
    // Square to the line: the way between its neighbours on the same line.
    let same = |j: usize| {
        points
            .get(j)
            .filter(|p| p.0 == layer && p.1.element == place.element && p.1.colour_art == place.colour_art)
    };
    let before = i.checked_sub(1).and_then(same).map_or(at, |p| p.2);
    let after = same(i + 1).map_or(at, |p| p.2);
    let along = after - before;
    let across = if along.length() > 1e-3 { vec2(-along.y, along.x).normalized() } else { vec2(0.0, 1.0) };
    let half = across * (width / 2.0).max(3.0);
    painter.line_segment([at - half, at + half], (2.0, SELECTION));
    for end in [at - half, at + half] {
        painter.circle(end, 3.5, Color32::WHITE, (1.5, SELECTION));
    }
}

/// How near a shape hint the pointer must be to grab it, in points.
const HINT_RADIUS: f32 = 8.0;

/// The shape hints to show and edit on this frame: the selected layer's,
/// when it's on the first frame of a shape tween (the drawing it morphs
/// from) or on the swap it ends at (the one it morphs into). Returns the
/// layer, the drawing, and whether that's the end.
fn shown_hints(editor: &Editor) -> Option<(LayerId, usize, bool)> {
    let (from, to) = editor.shape_tween_drawings()?;
    let layer = editor.selected_layer()?;
    if layer.morph_at(editor.frame).is_some() {
        return None;
    }
    match layer.drawing_index_at(editor.frame) {
        showing if showing == from => Some((layer.id, from, false)),
        showing if showing == to => Some((layer.id, to, true)),
        _ => None,
    }
}

/// The shape hint under the pointer: its layer, drawing and index.
fn hint_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, usize, usize)> {
    let (id, drawing, _) = shown_hints(editor)?;
    let layer = editor.doc.layer(id)?;
    let to_stage = editor.doc.layer_matrix(layer, editor.frame);
    let hints = &layer.drawings.get(drawing)?.hints;
    let index =
        hints.iter().position(|h| view.to_screen(to_stage.apply(*h)).distance(pointer) <= HINT_RADIUS)?;
    Some((id, drawing, index))
}

/// Shape hints as Animate shows them: lettered circles, yellow on the
/// drawing a tween starts from and green on the one it ends at.
fn draw_hints(painter: &egui::Painter, editor: &Editor, view: View) {
    let Some((id, drawing, end)) = shown_hints(editor) else { return };
    let Some(layer) = editor.doc.layer(id) else { return };
    let to_stage = editor.doc.layer_matrix(layer, editor.frame);
    let color = if end { Color32::from_rgb(0x6c, 0xd0, 0x7a) } else { Color32::from_rgb(0xf2, 0xc9, 0x4c) };
    for (i, hint) in layer.drawings[drawing].hints.iter().enumerate() {
        let at = view.to_screen(to_stage.apply(*hint));
        painter.circle(at, HINT_RADIUS, color, (1.0, Color32::from_black_alpha(160)));
        let letter = char::from(b'a' + (i % 26) as u8).to_string();
        painter.text(
            at,
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::proportional(11.0),
            Color32::BLACK,
        );
    }
}

/// Everything that affects the rendered stage image. If none of it changed
/// since last time, we can skip re-rendering and reuse the texture.
#[derive(PartialEq)]
struct RenderKey {
    doc: Document,
    timeline: TimelineId,
    to_stage: Affine,
    surroundings: Vec<Surrounding>,
    frame: u32,
    size: (u32, u32),
    onion: Option<OnionSkin>,
    /// Changes whenever an image finishes loading, so it gets drawn.
    images: u64,
    /// Changes when fonts are added, which can change how text looks.
    fonts: u64,
    /// The mask layer shown faintly because it's selected.
    shown_mask: Option<LayerId>,
    /// The layer the light table leaves unfaded, when it's on.
    light_table: Option<LayerId>,
}

pub struct StageView {
    /// `None` means "fit the stage to the window", which follows window resizes.
    zoom: Option<f32>,
    /// Offset of the stage's center from the viewport's center, in points.
    pan: Vec2,
    /// How far the view is turned, clockwise, in degrees.
    rotation: f32,
    gesture: Option<Gesture>,
    /// The path being drawn with the Pen tool, which lasts across many clicks.
    pen: Option<PenPath>,
    /// The anchor picked with the Subselection tool, whose handles are shown.
    anchor_selection: Option<AnchorRef>,
    /// Text being typed into on the stage.
    text_editing: Option<TextEditing>,
    /// A brush or eraser stroke in progress.
    painting: Option<Painting>,
    /// A selection being drawn with the marquee or lasso.
    selecting: Option<Selecting>,
    /// The selection's outline (marching ants), worked out once per
    /// selection: which selection, and its edges in canvas pixels.
    ants: Option<(usize, Vec<(Point, Point)>)>,
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
            rotation: 0.0,
            gesture: None,
            pen: None,
            anchor_selection: None,
            text_editing: None,
            painting: None,
            selecting: None,
            ants: None,
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
        let Some(element) = editor.element_mut(target.layer, target.element) else {
            return;
        };
        let (anchors, closed) = element.shape.to_anchors();
        if let Some(path) =
            remove_anchor(&anchors, target.index, closed).and_then(|a| path_from_anchors(&a, closed))
        {
            element.shape = Shape::Path(path);
        }
    }

    /// The middle of the visible part of the stage, in the coordinates of
    /// the timeline being edited: where "Paste in Center" puts things.
    pub fn visible_center(&self, editor: &Editor) -> Point {
        let view = self.view(self.last_area, &editor.doc).with_timeline(editor.timeline_transform());
        if self.last_area.is_positive() {
            view.to_local(self.last_area.center())
        } else {
            editor.doc.stage_rect().center()
        }
    }

    pub fn zoom_to_fit(&mut self) {
        self.zoom = None;
        self.pan = Vec2::ZERO;
    }

    /// Turns the view by `degrees` (clockwise) about the stage's middle.
    pub fn rotate_view(&mut self, degrees: f32) {
        self.rotation = normalize_degrees(self.rotation + degrees);
    }

    /// Turns the view back upright.
    pub fn reset_rotation(&mut self) {
        self.rotation = 0.0;
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
        let stage_view = self.view(area, &editor.doc);
        let corners = stage_view.stage_corners();
        let stage_size = stage_view.size * stage_view.stage_zoom;
        // Everything below works in the coordinates of the timeline being
        // edited: the stage's on the main timeline, a symbol's inside one.
        let view = stage_view.with_timeline(editor.timeline_transform());

        painter.rect_filled(area, 0.0, crate::theme::PASTEBOARD);
        if !editor.doc.background.is_opaque() {
            draw_checkerboard(&painter.with_clip_rect(area), stage_view);
        }
        self.update_texture(ui.ctx(), editor, stage_size);
        if let Some(texture) = &self.texture {
            // The rendered stage, as a quad so it can turn with the view.
            let mut mesh = egui::Mesh::with_texture(texture.id());
            let uvs = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
            for (corner, uv) in corners.into_iter().zip(uvs) {
                mesh.vertices.push(egui::epaint::Vertex { pos: corner, uv, color: Color32::WHITE });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            painter.add(egui::Shape::mesh(mesh));
        }
        painter.add(egui::Shape::closed_line(corners.to_vec(), (1.0, Color32::from_gray(20))));

        // Switching away from the pen (or anything else that changes the tool)
        // finishes the path in progress rather than silently dropping it.
        if editor.tool != Tool::Pen && self.pen.is_some() {
            self.finish_path(editor, false);
        }
        self.validate_anchor_selection(editor);

        // Typing into text takes the keyboard, and the mouse when it's on
        // the text; the tools get everything else.
        let pointer_on_text = self.handle_text_editing(ui, &response, editor, view);
        if !pointer_on_text {
            self.handle_tools(ui, &response, editor, view);
        }
        if editor.tool == Tool::Pen {
            self.handle_pen(ui, &response, editor, view);
        }
        if editor.tool.paints() {
            self.handle_painting(ui, &response, editor, view);
        }
        if editor.tool.selects() {
            self.handle_selecting(ui, &response, editor, view);
        } else {
            self.selecting = None;
        }
        // No transform handles on text being typed into: dragging would
        // fight with selecting.
        let handles = if self.text_editing.is_some() { None } else { Handles::for_selection(editor, view) };
        let overlay = painter.with_clip_rect(area);
        if editor.timeline() != TimelineId::Main {
            draw_registration_point(&overlay, view.to_screen(Point::ZERO));
        }
        draw_guides(&overlay, editor, view);
        self.draw_motion_path(&overlay, editor, view);
        self.draw_overlays(&overlay, editor, view, handles.as_ref());
        if editor.tool == Tool::Bone {
            draw_skeleton(&overlay, editor, view);
            draw_deformers(&overlay, editor, view);
            draw_warps(&overlay, editor, view);
        }
        self.draw_text_editing(ui, &overlay, editor, view);
        self.draw_pen_preview(&overlay, view, response.hover_pos());
        if editor.tool.uses_brush()
            && let Some(pointer) = response.hover_pos()
        {
            draw_brush_outline(&overlay, editor, view, pointer);
        }
        self.draw_painting(&overlay, editor, view);
        self.draw_selection(ui, &overlay, editor, view);
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
        let rotation = self.rotation.to_radians();
        let zoom = self.zoom.unwrap_or_else(|| {
            let available = (area.size() - Vec2::splat(FIT_MARGIN * 2.0)).max(Vec2::splat(1.0));
            // Fit the stage as turned: the box around it grows as it turns.
            let (sin, cos) = (rotation.sin().abs(), rotation.cos().abs());
            let turned =
                vec2(stage_size.x * cos + stage_size.y * sin, stage_size.x * sin + stage_size.y * cos);
            (available / turned).min_elem().clamp(MIN_ZOOM, MAX_ZOOM)
        });
        View::stage(area.center() + self.pan, stage_size, zoom, rotation)
    }

    /// Changes zoom while keeping the stage point under `anchor` fixed on
    /// screen, which is what makes zooming toward the cursor feel natural.
    fn set_zoom_about(&mut self, doc: &Document, anchor: Pos2, zoom: f32) {
        let before = self.view(self.last_area, doc);
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let anchored = before.to_local(anchor);

        // Solve for the stage centre that puts `anchored` back under
        // `anchor`, then express it as a pan offset from the viewport centre.
        let stage_size = vec2(doc.width as f32, doc.height as f32);
        let from_center = rotate((vec2(anchored.x, anchored.y) - stage_size / 2.0) * zoom, before.rotation);
        self.pan = anchor - from_center - self.last_area.center();
        self.zoom = Some(zoom);
    }

    fn handle_navigation(&mut self, ui: &egui::Ui, response: &egui::Response, doc: &Document) {
        if response.dragged_by(PointerButton::Middle) || matches!(self.gesture, Some(Gesture::Pan)) {
            self.pan += response.drag_delta();
        }
        if let (Some(Gesture::RotateView { from, start }), Some(pointer)) =
            (&self.gesture, response.interact_pointer_pos())
        {
            let center = self.view(response.rect, doc).center;
            self.rotation = normalize_degrees(start + angle_around(center, pointer) - from);
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
                self.gesture = self.begin_gesture(editor, press, view, space_held, shift, alt);
            }
        }

        if let Some(pointer) = response.interact_pointer_pos() {
            let p = view.to_local(pointer);
            match &mut self.gesture {
                Some(Gesture::Move { layers, grab }) => {
                    let mut delta = p - *grab;
                    if shift {
                        // Constrain to the dominant axis, like every design tool.
                        if delta.x.abs() > delta.y.abs() { delta.y = 0.0 } else { delta.x = 0.0 }
                    }
                    for &(id, start, into_space) in layers.iter() {
                        if let Some(layer) = editor.doc.layer(id) {
                            // The same distance on screen, measured in the
                            // parent's space, which may be turned or scaled.
                            let delta = into_space.apply(*grab + delta) - into_space.apply(*grab);
                            let pose =
                                Transform { position: start + delta, ..editor.doc.pose(layer, editor.frame) };
                            editor.set_pose(id, pose);
                        }
                    }
                }
                Some(Gesture::Marquee { current, .. }) => *current = p,
                Some(Gesture::Scale { layer, handle, start, bounds, group, into_space }) => {
                    let options = ScaleOptions { proportional: shift, from_pivot: alt };
                    let pose = scale_pose(start, *bounds, *handle, into_space.apply(p), options);
                    if group.is_empty() {
                        editor.set_pose(*layer, pose);
                    } else {
                        transform_group(editor, group, start, &pose);
                    }
                }
                Some(Gesture::Skew { layer, edge, start, bounds, grab, into_space }) => {
                    editor.set_pose(*layer, skew_pose(start, *bounds, *edge, *grab, into_space.apply(p)));
                }
                Some(Gesture::Rotate { layer, drag, group, into_space }) => {
                    let pose = drag.update(into_space.apply(p), shift);
                    if group.is_empty() {
                        editor.set_pose(*layer, pose);
                    } else {
                        transform_group(editor, group, &drag.start(), &pose);
                    }
                }
                Some(Gesture::Reach { layer, local }) => {
                    editor.doc.reach(*layer, *local, p, editor.frame);
                }
                Some(Gesture::MovePixels { layer, canvas, size, base, floating, selection, grab }) => {
                    if let Some(at) = canvas_point(editor, view, *layer, *size, pointer) {
                        let (dx, dy) = ((at.x - grab.x).round() as i64, (at.y - grab.y).round() as i64);
                        let mut moved = base.clone();
                        moved.draw_over(&floating.shifted(dx, dy));
                        if let Some(pixels) = editor.doc.canvas_mut(*canvas) {
                            *pixels = moved;
                        }
                        editor.pixel_selection = Some((*canvas, selection.shifted(dx, dy)));
                    }
                }
                Some(Gesture::Width { layer, place }) => {
                    let frame = editor.frame;
                    let Some(to_local) =
                        editor.doc.layer(*layer).and_then(|l| editor.doc.layer_matrix(l, frame).inverse())
                    else {
                        return;
                    };
                    let local = to_local.apply(p);
                    if let Some(Shape::Line { path, widths }) = line_shape_mut(editor, *layer, *place) {
                        let anchor = inkframe_core::centreline_anchors(path).get(place.anchor).copied();
                        if let (Some(anchor), Some(width)) = (anchor, widths.get_mut(place.anchor)) {
                            // As wide as twice the pointer's distance from the
                            // centreline: the edge follows the pointer.
                            *width = (local.distance(anchor) * 2.0).clamp(0.2, 400.0);
                        }
                    }
                }
                Some(Gesture::Warp { layer, point }) => {
                    let frame = editor.frame;
                    let to_local =
                        editor.doc.layer(*layer).and_then(|l| editor.doc.layer_matrix(l, frame).inverse());
                    if let Some(to_local) = to_local
                        && let Some(warp) = editor.doc.layer_mut(*layer).and_then(|l| l.warp.as_mut())
                    {
                        warp.move_point(*point, to_local.apply(p), frame);
                    }
                }
                Some(Gesture::Bend { layer, bone }) => {
                    let frame = editor.frame;
                    let to_local =
                        editor.doc.layer(*layer).and_then(|l| editor.doc.layer_matrix(l, frame).inverse());
                    if let Some(to_local) = to_local
                        && let Some(deformer) = editor.doc.layer_mut(*layer).and_then(|l| l.deformer.as_mut())
                    {
                        deformer.point_bone_at(*bone, to_local.apply(p), frame);
                    }
                }
                Some(Gesture::Camera { camera, how, grab, start }) => {
                    let pose = match how {
                        // The scene follows the pointer, so the camera moves
                        // the other way, measured in its own turned and
                        // zoomed view.
                        CameraDrag::Pan => {
                            let moved = p - view.to_local(*grab);
                            let turned = Affine::rotate(start.rotation.to_radians()).apply(moved);
                            let zoom = start.scale.x.max(0.01);
                            Transform { position: start.position - turned * (1.0 / zoom), ..*start }
                        }
                        // Round the middle of the stage: the scene turns with
                        // the pointer, so the camera turns the other way.
                        CameraDrag::Turn => {
                            let centre = view.to_screen(editor.doc.stage_rect().center());
                            let turned = angle_around(centre, pointer) - angle_around(centre, *grab);
                            Transform { rotation: start.rotation - turned.to_degrees(), ..*start }
                        }
                        // Up zooms in, down out, by the same factor for the
                        // same distance whatever the zoom.
                        CameraDrag::Zoom => {
                            let factor = ((grab.y - pointer.y) / 200.0).exp();
                            let zoom = (start.scale.x * factor).clamp(0.05, 50.0);
                            Transform { scale: Point::new(zoom, zoom), ..*start }
                        }
                    };
                    editor.doc.set_pose(*camera, editor.frame, pose);
                }
                Some(Gesture::Hint { layer, drawing, index }) => {
                    let frame = editor.frame;
                    let Some(to_local) =
                        editor.doc.layer(*layer).and_then(|l| editor.doc.layer_matrix(l, frame).inverse())
                    else {
                        return;
                    };
                    let local = to_local.apply(p);
                    if let Some(d) = editor.doc.layer_mut(*layer).and_then(|l| l.drawings.get_mut(*drawing)) {
                        // Hints sit on an outline: the nearest one.
                        let snapped = d
                            .elements
                            .iter()
                            .filter_map(|e| {
                                let (anchors, closed) = e.shape.to_anchors();
                                inkframe_core::nearest_on_anchors(&anchors, closed, local)
                            })
                            .min_by(|a, b| a.distance.total_cmp(&b.distance))
                            .map_or(local, |hit| hit.point);
                        if let Some(hint) = d.hints.get_mut(*index) {
                            *hint = snapped;
                        }
                    }
                }
                Some(Gesture::Pivot { layer }) => {
                    let frame = editor.frame;
                    let Some(layer) = editor.doc.layer(*layer) else { return };
                    // Moving the pivot leaves the drawing where it is, so the
                    // layer's map to the timeline is the same before and after.
                    let to_stage = editor.doc.layer_matrix(layer, frame);
                    let Some(local) = to_stage.inverse().map(|m| m.apply(p)) else { return };
                    // The box's corners, edge midpoints and center attract
                    // it, since those are where a joint most often goes.
                    let snap = editor.doc.layer_bounds(layer, frame).and_then(|bounds| {
                        ScaleHandle::ALL
                            .map(|h| h.local_position(bounds))
                            .into_iter()
                            .chain([bounds.center()])
                            .find(|spot| {
                                view.to_screen(to_stage.apply(*spot)).distance(pointer) <= PIVOT_GRAB
                            })
                    });
                    let id = layer.id;
                    editor.doc.set_pivot(id, snap.unwrap_or(local));
                }
                Some(Gesture::EditAnchor { target, part, start, closed, grab, to_local }) => {
                    // Move the part by however far the pointer has moved, so
                    // grabbing slightly off-center doesn't make it jump.
                    let local = to_local.apply(p);
                    let goal = part.position(&start[target.index]) + (local - *grab);
                    let anchors = move_part(start, target.index, *part, goal, alt);
                    let element = editor.element_mut(target.layer, target.element);
                    if let (Some(element), Some(path)) = (element, path_from_anchors(&anchors, *closed)) {
                        // Rectangles and ovals become paths here, on the first
                        // real edit, rather than as soon as they're clicked.
                        element.shape = Shape::Path(path);
                    }
                }
                Some(Gesture::DrawShape { start, current, .. }) => {
                    *current = if shift { constrain_square(*start, p) } else { p };
                }
                Some(Gesture::Pencil { points, pressures }) => {
                    // Skip samples closer than half a screen point; they add
                    // nothing but noise for the smoother to clean up.
                    if points.last().is_none_or(|last| last.distance(p) * view.zoom > 0.5) {
                        points.push(p);
                        pressures.push(pen_pressure());
                    }
                }
                Some(Gesture::BendPath { layer, segment, s }) => {
                    let p = into_parent_space(editor, *layer).apply(p);
                    let (out, into) = motion_path::bend_through(segment, *s, p);
                    if let Some(layer) = editor.doc.layer_mut(*layer) {
                        let position = &mut layer.track.position;
                        // Keep the handles on the far side of each key as they were.
                        let keep = |frame| position.key_at(frame).and_then(|k| k.bend).unwrap_or_default();
                        let (from, to) = (keep(segment.from), keep(segment.to));
                        position.set_bend(segment.from, Some(Bend { out, ..from }));
                        position.set_bend(segment.to, Some(Bend { into, ..to }));
                    }
                }
                Some(Gesture::PathHandle { layer, frame, out }) => {
                    let p = into_parent_space(editor, *layer).apply(p);
                    if let Some(layer) = editor.doc.layer_mut(*layer) {
                        let position = &mut layer.track.position;
                        let key = position.key_at(*frame).map(|k| k.value);
                        if let Some(bend) =
                            key.and_then(|at| motion_path::with_handle(position, *frame, *out, p - at))
                        {
                            position.set_bend(*frame, Some(bend));
                        }
                    }
                }
                Some(Gesture::Pan | Gesture::RotateView { .. }) | None => {}
            }
        }

        if response.drag_stopped() {
            if let Some(gesture) = self.gesture.take() {
                finish_gesture(gesture, editor, view);
            }
        }

        // Double-clicking a symbol instance opens the symbol for editing.
        if response.double_clicked()
            && editor.tool == Tool::Select
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some(id) = editor.hit_test(view.to_local(pointer), HIT_TOLERANCE / view.zoom)
            && editor.doc.layer(id).is_some_and(|l| l.instance.is_some())
        {
            editor.enter_instance(id);
            return;
        }
        // Double-clicking text types into it, as in Animate.
        if response.double_clicked()
            && editor.tool == Tool::Select
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some(id) = editor.hit_test(view.to_local(pointer), HIT_TOLERANCE / view.zoom)
            && let Some(element) = text_element(editor, id)
        {
            self.begin_text_editing(editor, view, id, element, Some(pointer));
            return;
        }

        // The Text tool: click existing text to edit it, or anywhere else to
        // start new text there.
        if response.clicked()
            && editor.tool == Tool::Text
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let p = view.to_local(pointer);
            let hit = editor.hit_test(p, HIT_TOLERANCE / view.zoom);
            match hit.and_then(|id| text_element(editor, id).map(|element| (id, element))) {
                Some((id, element)) => self.begin_text_editing(editor, view, id, element, Some(pointer)),
                None => {
                    if let Some(id) = editor.add_text(p) {
                        self.begin_text_editing(editor, view, id, 0, None);
                    }
                }
            }
            return;
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
                        editor.selection = editor.hit_test(view.to_local(pointer), tolerance);
                        self.anchor_selection = None;
                    }
                }
            }
        }
        // Right-clicking a shape hint removes it from both drawings.
        if response.secondary_clicked()
            && editor.tool == Tool::Select
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some((layer, _, index)) = hint_at(editor, view, pointer)
            && let Some((from, to)) = editor.shape_tween_drawings()
            && let Some(layer) = editor.doc.layer_mut(layer)
        {
            for drawing in [from, to] {
                if index < layer.drawings[drawing].hints.len() {
                    layer.drawings[drawing].hints.remove(index);
                }
            }
        }
        if response.clicked() && editor.tool == Tool::Select {
            let alt = ui.input(|i| i.modifiers.alt);
            let on_handle = |pointer| {
                hint_at(editor, view, pointer).is_some()
                    || Handles::for_selection(editor, view).and_then(|h| h.hit(pointer, view, alt)).is_some()
            };
            if let Some(pointer) = response.interact_pointer_pos().filter(|p| !on_handle(*p)) {
                let tolerance = HIT_TOLERANCE / view.zoom;
                let hit = editor.hit_test(view.to_local(pointer), tolerance);
                // Shift-click adds or removes a layer, as in Animate; a
                // Shift-click on empty stage leaves the selection alone.
                match (hit, shift) {
                    (Some(id), true) => editor.toggle_selected(id),
                    (None, true) => {}
                    (hit, false) => editor.select(hit),
                }
            }
        }
    }

    /// The layer whose motion path is shown: the selected layer, if it
    /// moves (has two or more position keys), while the Selection tool is
    /// in use and nothing's playing or being typed.
    fn motion_path_layer<'a>(&self, editor: &'a Editor) -> Option<&'a inkframe_core::Layer> {
        if editor.tool != Tool::Select || editor.playback.playing || self.text_editing.is_some() {
            return None;
        }
        if editor.selected_ids().len() > 1 {
            return None;
        }
        editor.selected_layer().filter(|l| l.track.position.keys().len() >= 2 && !l.locked)
    }

    /// The motion path handle or tween under `pointer`, if any.
    fn motion_path_hit(&self, editor: &Editor, view: View, pointer: Pos2) -> Option<PathHit> {
        // A layer following a guide takes its path from the guide; there's
        // nothing of its own to bend.
        let layer = self.motion_path_layer(editor).filter(|l| !follows_guide(editor, l))?;
        let segments = motion_path::segments(&layer.track.position);
        // The keys are in the parent's space, wherever the parent is now.
        let space = editor.doc.parent_matrix(layer, editor.frame);
        let near = |at: Point, reach: f32| view.to_screen(space.apply(at)).distance(pointer) <= reach;
        for segment in segments.iter().filter(|s| s.bent) {
            let [p0, p1, p2, p3] = segment.points;
            if near(p1, ANCHOR_HIT) && p1 != p0 {
                return Some(PathHit::Handle { frame: segment.from, out: true });
            }
            if near(p2, ANCHOR_HIT) && p2 != p3 {
                return Some(PathHit::Handle { frame: segment.to, out: false });
            }
        }
        let local = into_parent_space(editor, layer.id).apply(view.to_local(pointer));
        let zoom = view.zoom * space.average_scale();
        segments
            .iter()
            .map(|segment| (segment, motion_path::nearest(segment, local)))
            .filter(|(_, (_, distance))| distance * zoom <= HIT_TOLERANCE + 2.0)
            .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .map(|(segment, (s, _))| PathHit::Segment { segment: *segment, s })
    }

    /// The selected layer's motion path: a dashed line along the tweens,
    /// a dot for each frame (so the spacing shows where it speeds up and
    /// slows down, as in Animate), a square at each key, and the handles
    /// of curved tweens.
    fn draw_motion_path(&self, painter: &egui::Painter, editor: &Editor, view: View) {
        let Some(layer) = self.motion_path_layer(editor) else { return };
        let position = &layer.track.position;
        let path_color = SELECTION.gamma_multiply(0.8);
        // On a guide, the guide itself shows the path; just mark where the
        // layer is on each frame, and at each key.
        if follows_guide(editor, layer) {
            let keys = position.keys();
            for frame in keys[0].frame..=keys[keys.len() - 1].frame {
                let at = view.to_screen(editor.doc.layer_matrix(layer, frame).apply(layer.pivot));
                if position.key_at(frame).is_some() {
                    let square = Rect::from_center_size(at, Vec2::splat(6.0));
                    painter.rect_filled(square, 0.0, Color32::WHITE);
                    painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
                } else {
                    painter.circle_filled(at, 2.0, path_color);
                }
            }
            return;
        }
        // A child's path is in its parent's space, so it rides along with
        // the parent as it is on this frame.
        let space = editor.doc.parent_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(space.apply(p));
        for segment in motion_path::segments(position) {
            let curve = MotionCurve { points: segment.points };
            let points: Vec<Pos2> = (0..=32).map(|i| screen(curve.at(i as f32 / 32.0))).collect();
            painter.extend(egui::Shape::dashed_line(&points, (1.0, path_color), 5.0, 3.0));
        }
        let keys = position.keys();
        let (first, last) = (keys[0].frame, keys[keys.len() - 1].frame);
        for frame in first..=last {
            if let Some(at) = position.sample(frame as f32) {
                painter.circle_filled(screen(at), 2.0, path_color);
            }
        }
        for segment in motion_path::segments(position).iter().filter(|s| s.bent) {
            let [p0, p1, p2, p3] = segment.points.map(screen);
            for (anchor, handle) in [(p0, p1), (p3, p2)] {
                if anchor != handle {
                    painter.line_segment([anchor, handle], (1.0, SELECTION));
                    painter.circle(handle, 3.5, Color32::WHITE, (1.0, SELECTION));
                }
            }
        }
        for key in keys {
            let square = Rect::from_center_size(screen(key.value), Vec2::splat(6.0));
            painter.rect_filled(square, 0.0, Color32::WHITE);
            painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }
    }

    /// Starts typing into the named text layer with all its text selected.
    /// For the screenshot hook; people start typing by clicking.
    pub fn type_into(&mut self, editor: &mut Editor, name: &str) {
        let layer = editor.layers().iter().find(|l| l.name == name).map(|l| l.id);
        let Some((layer, element)) = layer.and_then(|id| text_element(editor, id).map(|e| (id, e))) else {
            eprintln!("No text layer named {name:?} for INKFRAME_SCREENSHOT_TYPE");
            return;
        };
        let length = text_of(editor, layer, element).map_or(0, |t| t.content.len());
        editor.tool = Tool::Text;
        editor.selection = Some(layer);
        let cursor = TextCursor::select(0, length);
        self.text_editing = Some(TextEditing { layer, element, cursor, selecting: false });
    }

    /// Starts typing into a text layer's `element`, with the cursor where
    /// `pointer` is, or at the end of the text.
    fn begin_text_editing(
        &mut self,
        editor: &mut Editor,
        view: View,
        layer: LayerId,
        element: usize,
        pointer: Option<Pos2>,
    ) {
        editor.selection = Some(layer);
        let Some(text) = text_of(editor, layer, element) else { return };
        let layout = inkframe_render::layout_text(text);
        let position = match (pointer, layer_to_local(editor, layer)) {
            (Some(pointer), Some(to_local)) => position_at(&layout, to_local.apply(view.to_local(pointer))),
            _ => text.content.len(),
        };
        self.text_editing =
            Some(TextEditing { layer, element, cursor: TextCursor::at(position), selecting: false });
    }

    /// Stops typing. Text left empty is removed, as in Animate: clicking
    /// with the Text tool and then clicking away shouldn't leave an
    /// invisible layer behind.
    fn finish_text_editing(&mut self, ui: &egui::Ui, id: egui::Id, editor: &mut Editor) {
        let Some(editing) = self.text_editing.take() else { return };
        ui.memory_mut(|m| m.surrender_focus(id));
        let empty =
            text_of(editor, editing.layer, editing.element).is_some_and(|t| t.content.trim().is_empty());
        editor.select(Some(editing.layer));
        if empty {
            editor.delete_selected_layer();
        }
    }

    /// Keyboard and mouse input while typing into text. Returns whether the
    /// mouse was used to place the cursor or select, in which case the tools
    /// shouldn't also act on it.
    fn handle_text_editing(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        let Some(mut editing) = self.text_editing else { return false };
        // Stop if the text went away (undo, say), another tool was picked,
        // or the keyboard went to another widget, like the properties panel.
        let focus_elsewhere = ui.memory(|m| m.focused().is_some_and(|focused| focused != response.id));
        let to_local = layer_to_local(editor, editing.layer);
        if text_of(editor, editing.layer, editing.element).is_none()
            || !matches!(editor.tool, Tool::Select | Tool::Text)
            || focus_elsewhere
            || to_local.is_none()
        {
            self.finish_text_editing(ui, response.id, editor);
            return false;
        }
        let to_local = to_local.expect("checked above");
        let local = |screen: Pos2| to_local.apply(view.to_local(screen));

        // Keep the keyboard, including the keys egui would otherwise use to
        // move between widgets (Tab, arrows, Escape).
        response.request_focus();
        let filter =
            egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
        ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));

        // Mouse: a press on the text places the cursor (Shift extends the
        // selection) and dragging selects. A press anywhere else stops
        // editing, and the tools handle it as usual.
        let (pressed, down, shift) =
            ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers.shift));
        let layout = text_of(editor, editing.layer, editing.element).map(inkframe_render::layout_text);
        let layout = layout.expect("checked above");
        let mut pointer_used = false;
        if pressed && let Some(pointer) = response.hover_pos() {
            let slack = HIT_TOLERANCE / view.zoom;
            if layout.bounds.expand(slack.max(layout.ascent * 0.25)).contains(local(pointer)) {
                editing.cursor.move_to(position_at(&layout, local(pointer)), shift);
                editing.selecting = true;
                pointer_used = true;
            } else {
                self.finish_text_editing(ui, response.id, editor);
                return false;
            }
        }
        if editing.selecting {
            pointer_used = true;
            if let Some(pointer) = response.hover_pos().or(response.interact_pointer_pos()) {
                editing.cursor.cursor = position_at(&layout, local(pointer));
            }
            editing.selecting = down;
        }
        if response.double_clicked() {
            let text = text_of(editor, editing.layer, editing.element).expect("checked above");
            let word = word_at(&text.content, editing.cursor.cursor);
            editing.cursor = TextCursor::select(word.start, word.end);
            pointer_used = true;
        }

        // Keyboard. The layout is redone for each event, since arrow keys
        // need to know where the text is after the previous keystroke.
        if response.has_focus() {
            let events = ui.input(|i| i.events.clone());
            for event in &events {
                let Some(text) = text_of_mut(editor, editing.layer, editing.element) else { break };
                let layout = inkframe_render::layout_text(text);
                let outcome = editing.cursor.handle(&mut text.content, &layout, event);
                if let Some(copied) = outcome.copy {
                    ui.ctx().copy_text(copied);
                }
                if outcome.finish {
                    self.text_editing = Some(editing);
                    self.finish_text_editing(ui, response.id, editor);
                    return pointer_used;
                }
            }
        }
        // Tell the system where the cursor is. That's what lets input
        // methods work here (egui only enables them for a widget that asks),
        // and their candidate window then opens beside the cursor.
        if let Some(text) = text_of(editor, editing.layer, editing.element)
            && let Some(layer) = editor.doc.layer(editing.layer)
        {
            let layout = inkframe_render::layout_text(text);
            let to_stage = editor.doc.layer_matrix(layer, editor.frame);
            let (x, baseline) = caret_position(&layout, editing.cursor.cursor);
            let top = view.to_screen(to_stage.apply(Point::new(x, baseline - layout.ascent)));
            let bottom = view.to_screen(to_stage.apply(Point::new(x, baseline + layout.descent)));
            let caret = Rect::from_two_pos(top, bottom).expand(1.0);
            ui.ctx().output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: caret,
                    cursor_rect: caret,
                    should_interrupt_composition: false,
                });
            });
        }
        self.text_editing = Some(editing);
        pointer_used
    }

    /// The cursor and selection of text being typed into.
    fn draw_text_editing(&self, ui: &egui::Ui, painter: &egui::Painter, editor: &Editor, view: View) {
        let Some(editing) = &self.text_editing else { return };
        let (Some(text), Some(layer)) =
            (text_of(editor, editing.layer, editing.element), editor.doc.layer(editing.layer))
        else {
            return;
        };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(to_stage.apply(p));
        let layout = inkframe_render::layout_text(text);

        // Each box is drawn as four transformed corners, so the highlight
        // follows the text when it's rotated or scaled.
        for selected in selection_boxes(&layout, editing.cursor.selection()) {
            let corners = selected.corners().map(screen).to_vec();
            painter.add(egui::Shape::convex_polygon(
                corners,
                SELECTION.gamma_multiply(0.35),
                egui::Stroke::NONE,
            ));
        }
        // Text being composed with an input method is underlined until
        // it's committed, as in every text field.
        if let Some((start, end)) = editing.cursor.composing {
            for composing in selection_boxes(&layout, start..end) {
                let y = composing.max.y - layout.descent * 0.5;
                let line = [screen(Point::new(composing.min.x, y)), screen(Point::new(composing.max.x, y))];
                painter.line_segment(line, (1.5, SELECTION));
            }
        }
        // The cursor blinks, as text cursors do, but stays solid while
        // selecting so it's easy to follow.
        let time = ui.input(|i| i.time);
        if editing.selecting || (time * 2.0) as i64 % 2 == 0 {
            let (x, baseline) = caret_position(&layout, editing.cursor.cursor);
            let top = screen(Point::new(x, baseline - layout.ascent));
            let bottom = screen(Point::new(x, baseline + layout.descent));
            painter.line_segment([top, bottom], (1.5, SELECTION));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    fn begin_gesture(
        &mut self,
        editor: &mut Editor,
        press: Pos2,
        view: View,
        space_held: bool,
        shift: bool,
        alt: bool,
    ) -> Option<Gesture> {
        if space_held && shift {
            return Some(Gesture::RotateView {
                from: angle_around(view.center, press),
                start: self.rotation,
            });
        }
        if space_held {
            return Some(Gesture::Pan);
        }
        let p = view.to_local(press);
        match editor.tool {
            Tool::Select if let Some((layer, drawing, index)) = hint_at(editor, view, press) => {
                Some(Gesture::Hint { layer, drawing, index })
            }
            Tool::Select => {
                // Inside a pixel selection on the paint layer: move those
                // pixels, as Photoshop's Move tool does.
                if let Some((layer, canvas, width, height)) = editor.paint_target()
                    && let Some(selection) = editor.selection_on(canvas).cloned()
                    && let Some(grab) = canvas_point(editor, view, layer, (width, height), press)
                    && selection.at(grab.x.floor() as i64, grab.y.floor() as i64) >= 128
                    && let Some(pixels) = editor.doc.canvas(canvas)
                {
                    let (base, floating) = (selection.clear(pixels), selection.keep(pixels));
                    return Some(Gesture::MovePixels {
                        layer,
                        canvas,
                        size: (width, height),
                        base,
                        floating,
                        selection,
                        grab,
                    });
                }
                // Handles of the current selection take priority over
                // whatever shape happens to be underneath them.
                if let Some(handles) = Handles::for_selection(editor, view) {
                    let layer = handles.layer;
                    // The handle math works in the space the pose is in: the
                    // layer's parent's, which is the timeline's for a layer
                    // without one.
                    let into_space = handles.space.inverse().unwrap_or(Affine::IDENTITY);
                    let q = into_space.apply(p);
                    match handles.hit(press, view, alt) {
                        Some(HandleHit::Scale(handle)) => {
                            let (start, bounds, group) = (handles.pose, handles.bounds, handles.group);
                            return Some(Gesture::Scale { layer, handle, start, bounds, group, into_space });
                        }
                        Some(HandleHit::Rotate) => {
                            let drag = RotateDrag::new(handles.pose, q);
                            return Some(Gesture::Rotate { layer, drag, group: handles.group, into_space });
                        }
                        Some(HandleHit::Skew(edge)) => {
                            let (start, bounds) = (handles.pose, handles.bounds);
                            return Some(Gesture::Skew { layer, edge, start, bounds, grab: q, into_space });
                        }
                        Some(HandleHit::Pivot) => return Some(Gesture::Pivot { layer }),
                        None => {}
                    }
                }
                // Then the motion path's handles, then the layer itself, then
                // the path, so grabbing the layer where it sits on its own
                // path moves it rather than bending the path.
                let path_hit = self.motion_path_hit(editor, view, press);
                if let (Some(PathHit::Handle { frame, out }), Some(layer)) = (&path_hit, editor.selection) {
                    return Some(Gesture::PathHandle { layer, frame: *frame, out: *out });
                }
                let tolerance = HIT_TOLERANCE / view.zoom;
                let hit = editor.hit_test(p, tolerance);
                if hit.is_none()
                    && let (Some(PathHit::Segment { segment, s }), Some(layer)) = (path_hit, editor.selection)
                {
                    return Some(Gesture::BendPath { layer, segment, s });
                }
                let Some(hit) = hit else {
                    // Empty stage: drag out a box to select what it touches.
                    return Some(Gesture::Marquee { start: p, current: p, adding: shift });
                };
                // Dragging one of several selected layers moves them all;
                // Shift-dragging another adds it first. Otherwise the layer
                // under the pointer becomes the selection.
                if shift && !editor.is_selected(hit) {
                    editor.toggle_selected(hit);
                } else if !editor.is_selected(hit) {
                    editor.select(Some(hit));
                }
                let frame = editor.frame;
                let selected = editor.selected_ids();
                let layers = selected
                    .iter()
                    .filter_map(|&id| editor.doc.layer(id))
                    .filter(|l| !l.locked)
                    // Children of a moving parent already come along.
                    .filter(|l| !selected.iter().any(|&a| editor.doc.hangs_from(l, a)))
                    .map(|l| {
                        let into_space =
                            editor.doc.parent_matrix(l, frame).inverse().unwrap_or(Affine::IDENTITY);
                        (l.id, editor.doc.pose(l, frame).position, into_space)
                    })
                    .collect();
                Some(Gesture::Move { layers, grab: p })
            }
            tool @ (Tool::Rectangle | Tool::Ellipse) => {
                Some(Gesture::DrawShape { tool, start: p, current: p })
            }
            Tool::Pencil => Some(Gesture::Pencil { points: vec![p], pressures: vec![pen_pressure()] }),
            Tool::Width => {
                let (layer, place) = line_point_at(editor, view, press)?;
                editor.select(Some(layer));
                Some(Gesture::Width { layer, place })
            }
            Tool::Camera => {
                // The first drag adds a camera, if there isn't one.
                let camera = editor.doc.add_camera();
                editor.select(Some(camera));
                let start = editor.doc.layer(camera).map(|l| editor.doc.pose(l, editor.frame))?;
                let how = if shift {
                    CameraDrag::Turn
                } else if alt {
                    CameraDrag::Zoom
                } else {
                    CameraDrag::Pan
                };
                Some(Gesture::Camera { camera, how, grab: press, start })
            }
            Tool::Bone => {
                // A warp grid's points and a deformer's joints first: they
                // sit on top of the drawing.
                if let Some((layer, point)) = warp_point_at(editor, view, press) {
                    editor.select(Some(layer));
                    return Some(Gesture::Warp { layer, point });
                }
                if let Some((layer, bone)) = deformer_joint_at(editor, view, press) {
                    editor.select(Some(layer));
                    return Some(Gesture::Bend { layer, bone });
                }
                let tolerance = HIT_TOLERANCE / view.zoom;
                editor.select(editor.hit_test(p, tolerance));
                let layer = editor.selected_layer()?;
                // A rig's root has nothing to pull against, so dragging it
                // moves the whole rig instead.
                if editor.doc.ik_chain(layer.id).is_empty() {
                    let start = editor.doc.pose(layer, editor.frame).position;
                    return Some(Gesture::Move {
                        layers: vec![(layer.id, start, Affine::IDENTITY)],
                        grab: p,
                    });
                }
                let local = editor.doc.layer_matrix(layer, editor.frame).inverse()?.apply(p);
                Some(Gesture::Reach { layer: layer.id, local })
            }
            // The pen and the painting tools handle their own presses and
            // drags; see `handle_pen` and `handle_painting`. The Text tool
            // only clicks.
            Tool::Pen | Tool::Text => None,
            Tool::Brush
            | Tool::Eraser
            | Tool::Eyedropper
            | Tool::PaintBucket
            | Tool::Gradient
            | Tool::Smudge
            | Tool::Blur
            | Tool::Sharpen
            | Tool::Dodge
            | Tool::Burn
            | Tool::CloneStamp
            | Tool::RectMarquee
            | Tool::EllipseMarquee
            | Tool::Lasso
            | Tool::PolygonLasso
            | Tool::MagicWand => None,
            Tool::Subselect => {
                let Some((target, part)) = self.anchor_hit(editor, view, press) else {
                    // Dragging off the anchors selects a layer, but doesn't move it.
                    let tolerance = HIT_TOLERANCE / view.zoom;
                    editor.selection = editor.hit_test(p, tolerance);
                    self.anchor_selection = None;
                    return None;
                };
                self.anchor_selection = Some(target);
                let layer = editor.doc.layer(target.layer)?;
                let to_local = editor.doc.layer_matrix(layer, editor.frame).inverse()?;
                let (start, closed) = layer.elements_at(editor.frame).get(target.element)?.shape.to_anchors();
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
        let layer = editor.selected_layer().filter(|l| l.is_shown_at(editor.frame) && !l.locked)?;
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let local = to_stage.inverse()?.apply(view.to_local(pointer));
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let mut best: Option<OutlineHit> = None;
        for (element, e) in layer.elements_at(editor.frame).iter().enumerate() {
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
                .element(target.layer, target.element)
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
        let Some(layer) = editor.selected_layer().filter(|l| l.is_shown_at(editor.frame) && !l.locked) else {
            return Vec::new();
        };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let mut handles = Vec::new();
        let mut points = Vec::new();
        for (element, e) in layer.elements_at(editor.frame).iter().enumerate() {
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
        let Some(layer) = editor.selected_layer().filter(|l| l.is_shown_at(editor.frame)) else { return };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        for (element, e) in layer.elements_at(editor.frame).iter().enumerate() {
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
    /// The Brush, Eraser and Eyedropper. Painting starts the moment the pen
    /// touches (not after it has moved a little, as dragging does), so a
    /// tap leaves a dot, and paints through every pointer event, not just
    /// one per frame, so quick strokes stay smooth.
    fn handle_painting(&mut self, ui: &egui::Ui, response: &egui::Response, editor: &mut Editor, view: View) {
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
                Tool::Brush if alt => return pick_color(editor, view.to_local(press)),
                // Alt-click sets where the Clone Stamp copies from.
                Tool::CloneStamp if alt => return set_clone_source(editor, view, press),
                Tool::PaintBucket => return fill_at(editor, view, press),
                _ => {}
            }
            self.painting = start_painting(editor, view, press);
            if let Some(painting) = &mut self.painting {
                let pressure = tablet::current().map_or(1.0, |pen| pen.pressure);
                painting.smoothed = press;
                painting.pen = tablet::Sample { pos: press, pressure };
                paint_to(painting, editor, view, press, pressure);
            }
        }
        let Some(painting) = &mut self.painting else { return };
        if down {
            let samples = ui.input(tablet::samples);
            for sample in samples {
                // Smoothing: each point moves the stroke only part of the
                // way toward the pen.
                let follow = 1.0 - editor.smoothing.clamp(0.0, 0.95);
                painting.smoothed += (sample.pos - painting.smoothed) * follow;
                painting.pen = sample;
                paint_to(painting, editor, view, painting.smoothed, sample.pressure);
            }
        } else {
            // Lifted: finish where the pen actually is.
            let pen = painting.pen;
            paint_to(painting, editor, view, pen.pos, pen.pressure);
            if let PaintWork::Gradient { from } = painting.work {
                lay_gradient(painting, editor, view, from, pen.pos);
            }
            self.painting = None;
        }
    }

    /// The selection tools. Each press's modifiers say how the new
    /// selection combines with the one there: Shift adds, Alt subtracts,
    /// both keep only the overlap.
    fn handle_selecting(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) {
        let (pressed, down, double, enter, escape, modifiers) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.button_double_clicked(PointerButton::Primary),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.modifiers,
            )
        });
        let combine = match (modifiers.shift, modifiers.alt) {
            (true, true) => Combine::Intersect,
            (true, false) => Combine::Add,
            (false, true) => Combine::Subtract,
            (false, false) => Combine::Replace,
        };
        let press = ui
            .input(|i| i.pointer.press_origin())
            .filter(|_| pressed && response.is_pointer_button_down_on());
        let pointer = response.interact_pointer_pos().or(response.hover_pos());

        // The polygonal lasso carries on between clicks.
        if let Some(Selecting::Polygon { points, combine }) = &mut self.selecting {
            let closes = |p: Pos2| points.first().is_some_and(|first| first.distance(p) <= POLYGON_CLOSE);
            if escape {
                self.selecting = None;
            } else if enter || double || press.is_some_and(closes) {
                let (points, combine) = (std::mem::take(points), *combine);
                self.selecting = None;
                finish_selection(editor, view, combine, |w, h, to_canvas| {
                    Selection::polygon(w, h, &points.iter().map(|p| to_canvas(*p)).collect::<Vec<_>>())
                });
            } else if let Some(press) = press {
                points.push(press);
            }
            return;
        }

        if let Some(press) = press {
            if editor.paint_target().is_none() {
                editor.add_paint_layer();
            }
            match editor.tool {
                Tool::RectMarquee | Tool::EllipseMarquee => {
                    self.selecting = Some(Selecting::Marquee { start: press, current: press, combine });
                }
                Tool::Lasso => self.selecting = Some(Selecting::Lasso { points: vec![press], combine }),
                Tool::PolygonLasso => {
                    self.selecting = Some(Selecting::Polygon { points: vec![press], combine })
                }
                Tool::MagicWand => {
                    let (tolerance, contiguous) = (editor.wand_tolerance, editor.wand_contiguous);
                    let Some((_, canvas, ..)) = editor.paint_target() else { return };
                    let Some(pixels) = editor.doc.canvas(canvas).cloned() else { return };
                    finish_selection(editor, view, combine, |_, _, to_canvas| {
                        let at = to_canvas(press);
                        Selection::similar(
                            &pixels,
                            at.x.floor() as i64,
                            at.y.floor() as i64,
                            tolerance,
                            contiguous,
                        )
                    });
                }
                _ => {}
            }
            return;
        }

        match &mut self.selecting {
            Some(Selecting::Marquee { current, .. }) if down => {
                if let Some(pointer) = pointer {
                    *current = pointer;
                }
            }
            Some(Selecting::Lasso { points, .. }) if down => {
                points.extend(ui.input(tablet::samples).into_iter().map(|s| s.pos));
            }
            Some(Selecting::Marquee { start, current, combine }) => {
                let (start, current, combine) = (*start, *current, *combine);
                let ellipse = editor.tool == Tool::EllipseMarquee;
                self.selecting = None;
                // A click without a drag, as in Photoshop, just deselects.
                if start.distance(current) < 2.0 {
                    if combine == Combine::Replace {
                        editor.deselect_pixels();
                    }
                    return;
                }
                finish_selection(editor, view, combine, |w, h, to_canvas| {
                    let (a, b) = (to_canvas(start), to_canvas(current));
                    if ellipse { Selection::ellipse(w, h, a, b) } else { Selection::rectangle(w, h, a, b) }
                });
            }
            Some(Selecting::Lasso { points, combine }) => {
                let (points, combine) = (std::mem::take(points), *combine);
                self.selecting = None;
                finish_selection(editor, view, combine, |w, h, to_canvas| {
                    Selection::polygon(w, h, &points.iter().map(|p| to_canvas(*p)).collect::<Vec<_>>())
                });
            }
            _ => {}
        }
    }

    /// The selection being drawn, and the active selection's marching ants.
    fn draw_selection(&mut self, ui: &egui::Ui, painter: &egui::Painter, editor: &Editor, view: View) {
        let dark = (2.5, Color32::from_black_alpha(160));
        let light = (1.0, Color32::WHITE);
        match &self.selecting {
            Some(Selecting::Marquee { start, current, .. }) => {
                // Drawn in the canvas's own directions, which is how the
                // selection will lie, however the layer or view is turned.
                let Some((layer, _, w, h)) = editor.paint_target() else { return };
                let to_canvas = |p: Pos2| canvas_point(editor, view, layer, (w, h), p).unwrap_or(Point::ZERO);
                let (a, b) = (to_canvas(*start), to_canvas(*current));
                let (min, max) =
                    (Point::new(a.x.min(b.x), a.y.min(b.y)), Point::new(a.x.max(b.x), a.y.max(b.y)));
                let in_canvas: Vec<Point> = if editor.tool == Tool::EllipseMarquee {
                    let center = min.lerp(max, 0.5);
                    let radius = Point::new((max.x - min.x) / 2.0, (max.y - min.y) / 2.0);
                    (0..=48)
                        .map(|i| {
                            let t = i as f32 / 48.0 * std::f32::consts::TAU;
                            Point::new(center.x + t.cos() * radius.x, center.y + t.sin() * radius.y)
                        })
                        .collect()
                } else {
                    vec![min, Point::new(max.x, min.y), max, Point::new(min.x, max.y), min]
                };
                let outline: Vec<Pos2> =
                    in_canvas.into_iter().filter_map(|p| canvas_to_screen(editor, view, layer, p)).collect();
                for stroke in [dark, light] {
                    painter.add(egui::Shape::line(outline.clone(), stroke));
                }
            }
            Some(Selecting::Lasso { points, .. } | Selecting::Polygon { points, .. }) => {
                let mut line = points.clone();
                if matches!(self.selecting, Some(Selecting::Polygon { .. }))
                    && let Some(hover) = ui.input(|i| i.pointer.hover_pos())
                {
                    line.push(hover);
                }
                for stroke in [dark, light] {
                    painter.add(egui::Shape::line(line.clone(), stroke));
                }
                if let (Some(Selecting::Polygon { .. }), Some(first)) = (&self.selecting, points.first()) {
                    painter.circle_stroke(*first, POLYGON_CLOSE, light);
                }
            }
            None => {}
        }

        // Marching ants around the active selection.
        let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
        let Some(selection) = editor.selection_on(canvas) else { return };
        if self.ants.as_ref().is_none_or(|(id, _)| *id != selection.identity()) {
            self.ants = Some((selection.identity(), selection.outline()));
        }
        let Some(layer) = editor.doc.layer(layer) else { return };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let half = Point::new(width as f32 / 2.0, height as f32 / 2.0);
        let screen = |p: Point| view.to_screen(to_stage.apply(p - half));
        // The dashes march along every tenth of a second.
        let time = ui.input(|i| i.time);
        let phase = (time * 10.0) as usize;
        let edges = &self.ants.as_ref().expect("just made").1;
        for &(a, b) in edges {
            let (a, b) = (screen(a), screen(b));
            painter.line_segment([a, b], (1.0, Color32::BLACK));
            // White dashes 4 points long.
            let length = a.distance(b);
            let mut along = ((a.x + a.y) as usize + phase) as f32 % 8.0 - 8.0;
            while along < length {
                let (from, to) = (along.max(0.0), (along + 4.0).min(length));
                if to > from {
                    painter.line_segment([a + (b - a) * (from / length), a + (b - a) * (to / length)], light);
                }
                along += 8.0;
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }

    /// While a gradient is being dragged out: the line it runs along.
    fn draw_painting(&self, painter: &egui::Painter, editor: &Editor, view: View) {
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

    fn handle_pen(&mut self, ui: &egui::Ui, response: &egui::Response, editor: &mut Editor, view: View) {
        let (pressed, button_down, space_held, shift) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.key_down(egui::Key::Space),
                i.modifiers.shift,
            )
        });
        let pointer = response.interact_pointer_pos().map(|p| view.to_local(p));

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
        let hover_stage = hover.map(|p| view.to_local(p));
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
        if editor.tool == Tool::Width {
            draw_line_widths(painter, editor, view);
            return;
        }
        if editor.tool == Tool::Camera {
            draw_camera_frame(painter, editor, view);
            return;
        }
        // Painting needs a clear view of the picture, not a box around it.
        if editor.tool.on_pixels() {
            return;
        }
        // The path tools show the selected shape's own outline and points;
        // the others outline the layer's bounds, following its rotation and scale.
        let shows_points = matches!(editor.tool, Tool::Subselect | Tool::Pen);
        if shows_points {
            self.draw_anchor_overlay(painter, editor, view);
        }
        let selected = editor.selected_ids();
        for layer in selected.iter().filter_map(|id| editor.doc.layer(*id)) {
            if !layer.is_shown_at(editor.frame) || shows_points {
                continue;
            }
            if let Some(bounds) = editor.doc.layer_bounds(layer, editor.frame) {
                let to_stage = editor.doc.layer_matrix(layer, editor.frame);
                let corners = bounds.corners().map(|c| view.to_screen(to_stage.apply(c))).to_vec();
                // In the layer's own colour, as in Animate, so it's plain
                // which layer the selection belongs to.
                let color = crate::convert::to_color32(layer.tag_color());
                painter.add(egui::Shape::closed_line(corners, (1.5, color)));
                // The pivot (the point the layer rotates and scales around)
                // matters only when there's one layer to rotate or scale.
                // It's drawn as a ring with a cross, so it reads as something
                // to grab rather than a dot of the drawing.
                if selected.len() == 1 {
                    let pivot = view.to_screen(to_stage.apply(layer.pivot));
                    painter.circle(pivot, 4.5, Color32::WHITE, (1.5, color));
                    let arm = vec2(3.0, 0.0);
                    painter.line_segment([pivot - arm, pivot + arm], (1.0, color));
                    painter.line_segment([pivot - arm.rot90(), pivot + arm.rot90()], (1.0, color));
                }
            }
        }
        if let Some(handles) = handles {
            // Around a group, a faint box joins the handles, so it's clear
            // they belong to the selection as a whole.
            if !handles.group.is_empty() {
                let corner = |x, y| handles.scale.iter().find(|(h, _)| h.x == x && h.y == y).map(|(_, p)| *p);
                let corners: Vec<Pos2> = [(-1, -1), (1, -1), (1, 1), (-1, 1)]
                    .into_iter()
                    .filter_map(|(x, y)| corner(x, y))
                    .collect();
                painter.add(egui::Shape::closed_line(corners, (1.0, SELECTION.gamma_multiply(0.6))));
            }
            handles.draw(painter);
        }
        // Over the handles: a hint is grabbed before a handle beneath it.
        if editor.tool == Tool::Select {
            draw_hints(painter, editor, view);
        }

        let preview = (1.0, SELECTION);
        if let Some(Gesture::Marquee { start, current, .. }) = &self.gesture {
            let area = Rect::from_two_pos(view.to_screen(*start), view.to_screen(*current));
            painter.rect_filled(area, 0.0, SELECTION.gamma_multiply(0.12));
            painter.rect_stroke(area, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }
        match &self.gesture {
            Some(Gesture::DrawShape { tool, start, current }) => {
                // Built from local points, so the preview turns and stretches
                // with an instance being edited in place.
                let rect = StageRect::from_points(*start, *current);
                let outline: Vec<Pos2> = if *tool == Tool::Ellipse {
                    let (center, rx, ry) = (rect.center(), rect.width() / 2.0, rect.height() / 2.0);
                    (0..48)
                        .map(|i| {
                            let a = i as f32 / 48.0 * std::f32::consts::TAU;
                            view.to_screen(center + Point::new(rx * a.cos(), ry * a.sin()))
                        })
                        .collect()
                } else {
                    rect.corners().iter().map(|c| view.to_screen(*c)).collect()
                };
                painter.add(egui::Shape::closed_line(outline, preview));
            }
            Some(Gesture::Pencil { points, pressures }) => {
                // The line as it's drawn, each piece as wide as the pen
                // pressed there.
                let stroke = editor.pencil_stroke;
                let color = to_color32(stroke.color);
                let pressure = |i: usize| {
                    if editor.pencil_pressure { pressures.get(i).copied().unwrap_or(1.0) } else { 1.0 }
                };
                for (i, pair) in points.windows(2).enumerate() {
                    let width = (stroke.width * pressure(i + 1) * view.zoom).max(0.5);
                    painter.line_segment([view.to_screen(pair[0]), view.to_screen(pair[1])], (width, color));
                    painter.circle_filled(view.to_screen(pair[1]), width / 2.0, color);
                }
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
        // An I-beam over text being typed into, where clicking places the cursor.
        if let Some(editing) = &self.text_editing
            && let (Some(pointer), Some(to_local)) =
                (response.hover_pos(), layer_to_local(editor, editing.layer))
            && let Some(text) = text_of(editor, editing.layer, editing.element)
            && inkframe_render::layout_text(text).bounds.contains(to_local.apply(view.to_local(pointer)))
        {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
            return;
        }
        let space_held = ui.input(|i| i.key_down(egui::Key::Space));
        let handle_cursor = |hit| handles.map_or(CursorIcon::Default, |h| h.cursor(hit));

        let icon = match &self.gesture {
            Some(Gesture::Pan | Gesture::RotateView { .. }) => CursorIcon::Grabbing,
            _ if space_held => CursorIcon::Grab,
            Some(Gesture::Move { .. } | Gesture::MovePixels { .. }) => CursorIcon::Move,
            Some(Gesture::Marquee { .. }) => CursorIcon::Crosshair,
            Some(Gesture::Rotate { .. }) => CursorIcon::Grabbing,
            Some(Gesture::Pivot { .. } | Gesture::Hint { .. }) => CursorIcon::Crosshair,
            Some(Gesture::Camera { .. }) => CursorIcon::Grabbing,
            Some(
                Gesture::Reach { .. } | Gesture::Bend { .. } | Gesture::Warp { .. } | Gesture::Width { .. },
            ) => CursorIcon::Grabbing,
            Some(Gesture::EditAnchor { .. }) => CursorIcon::Move,
            Some(Gesture::BendPath { .. } | Gesture::PathHandle { .. }) => CursorIcon::Grabbing,
            Some(Gesture::Scale { handle, .. }) => handle_cursor(HandleHit::Scale(*handle)),
            Some(Gesture::Skew { edge, .. }) => handle_cursor(HandleHit::Skew(*edge)),
            Some(Gesture::DrawShape { .. } | Gesture::Pencil { .. }) => CursorIcon::Crosshair,
            None => {
                let alt = ui.input(|i| i.modifiers.alt);
                let hovered = handles.zip(response.hover_pos()).and_then(|(h, p)| h.hit(p, view, alt));
                match (hovered, editor.tool) {
                    (Some(hit), _) => handle_cursor(hit),
                    // A hand over the motion path says it can be dragged.
                    (None, Tool::Select)
                        if response.hover_pos().is_some_and(|p| {
                            editor.hit_test(view.to_local(p), HIT_TOLERANCE / view.zoom).is_none()
                                && self.motion_path_hit(editor, view, p).is_some()
                        }) =>
                    {
                        CursorIcon::Grab
                    }
                    (None, Tool::Select) => CursorIcon::Default,
                    (None, Tool::Width) => {
                        if response.hover_pos().is_some_and(|p| line_point_at(editor, view, p).is_some()) {
                            CursorIcon::ResizeHorizontal
                        } else {
                            CursorIcon::Default
                        }
                    }
                    (None, Tool::Camera) => CursorIcon::Move,
                    (None, Tool::Subselect) => {
                        let on_anchor =
                            response.hover_pos().is_some_and(|p| self.anchor_hit(editor, view, p).is_some());
                        if on_anchor { CursorIcon::PointingHand } else { CursorIcon::Default }
                    }
                    (None, Tool::Pen) => {
                        let can_close =
                            self.pen.as_ref().zip(response.hover_pos()).is_some_and(|(pen, p)| {
                                pen.can_close_at(view.to_local(p), ANCHOR_HIT / view.zoom)
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
                    (None, Tool::Text) => CursorIcon::Text,
                    // The brush's outline shows its size (`draw_brush_outline`);
                    // the crosshair marks its centre.
                    (
                        None,
                        Tool::Brush
                        | Tool::Eraser
                        | Tool::Eyedropper
                        | Tool::PaintBucket
                        | Tool::Gradient
                        | Tool::Smudge
                        | Tool::Blur
                        | Tool::Sharpen
                        | Tool::Dodge
                        | Tool::Burn
                        | Tool::CloneStamp
                        | Tool::RectMarquee
                        | Tool::EllipseMarquee
                        | Tool::Lasso
                        | Tool::PolygonLasso
                        | Tool::MagicWand,
                    ) => CursorIcon::Crosshair,
                    (None, Tool::Bone)
                        if response.hover_pos().is_some_and(|p| {
                            deformer_joint_at(editor, view, p).is_some()
                                || warp_point_at(editor, view, p).is_some()
                        }) =>
                    {
                        CursorIcon::Grab
                    }
                    (None, Tool::Bone) => {
                        let tolerance = HIT_TOLERANCE / view.zoom;
                        let over =
                            response.hover_pos().and_then(|p| editor.hit_test(view.to_local(p), tolerance));
                        if over.is_some() { CursorIcon::Grab } else { CursorIcon::Default }
                    }
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
        let timeline = editor.timeline();
        let to_stage = editor.timeline_transform();
        let surroundings = editor.surroundings();
        // A selected mask is drawn faintly so it can be seen while moving it.
        let shown_mask = editor.selected_layer().filter(|l| l.mask).map(|l| l.id);
        let light_table = editor.selection.filter(|_| editor.light_table);
        let key = RenderKey {
            doc: doc.clone(),
            timeline,
            frame: editor.frame,
            to_stage,
            surroundings: surroundings.clone(),
            size,
            onion,
            images: editor.images.generation(),
            fonts: inkframe_render::fonts_generation(),
            shown_mask,
            light_table,
        };
        if self.rendered.as_ref() == Some(&key) {
            return;
        }

        let Some(mut pixmap) = tiny_skia::Pixmap::new(size.0, size.1) else {
            return;
        };
        let bg = doc.background;
        pixmap.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));

        // Editing in place: the scene around the symbol, faded so the
        // symbol itself stands out, as in Animate.
        let images = Some(&editor.images);
        let faded = RenderOptions {
            scale,
            draw_background: false,
            opacity: SURROUNDINGS_OPACITY,
            tint: None,
            images,
            show_guides: true,
            show_mask: None,
            light_table: None,
        };
        for level in &surroundings {
            render_timeline_into(
                &mut pixmap,
                doc,
                level.timeline,
                level.frame,
                level.to_stage,
                Some(level.skip),
                &faded,
            );
        }

        if let Some(onion) = onion {
            draw_onion_skins(&mut pixmap, editor, onion, scale);
        }
        let options = RenderOptions {
            scale,
            draw_background: false,
            images,
            show_guides: true,
            show_mask: shown_mask,
            light_table,
            ..RenderOptions::default()
        };
        render_timeline_into(&mut pixmap, doc, timeline, editor.frame, to_stage, None, &options);

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
fn draw_onion_skins(pixmap: &mut tiny_skia::Pixmap, editor: &Editor, onion: OnionSkin, scale: f32) {
    let (doc, frame, timeline, to_stage) =
        (&editor.doc, editor.frame, editor.timeline(), editor.timeline_transform());
    let mut ghost_at = |f: u32, options: &RenderOptions| {
        render_timeline_into(pixmap, doc, timeline, f, to_stage, None, options);
    };
    let ghost = |distance: u32, count: u32, tint| RenderOptions {
        scale,
        draw_background: false,
        tint: Some(tint),
        images: Some(&editor.images),
        // A guide doesn't move with the animation, so ghosts of it would
        // only clutter.
        show_guides: false,
        show_mask: None,
        light_table: None,
        opacity: 0.45 * (count - distance + 1) as f32 / count as f32,
    };
    // Farthest first, so nearer ghosts paint on top.
    for distance in (1..=onion.before).rev() {
        if let Some(f) = frame.checked_sub(distance) {
            ghost_at(f, &ghost(distance, onion.before, ONION_BEFORE));
        }
    }
    for distance in (1..=onion.after).rev() {
        let f = frame + distance;
        if f <= editor.last_frame() {
            ghost_at(f, &ghost(distance, onion.after, ONION_AFTER));
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
            editor.add_drawn_shape(name, element, rect.center());
        }
        Gesture::Pencil { points, pressures } => {
            // Simplify relative to the screen, so a stroke drawn while zoomed
            // in keeps proportionally more detail.
            let tolerance = 1.5 / view.zoom;
            let Some(path) = fit_freehand_path(&points, tolerance) else {
                return;
            };
            if editor.pencil_pressure || editor.pencil_taper {
                // Anchors along the whole line, so its width can change
                // along it (a straight stroke simplifies to just its ends).
                let length: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
                let path = inkframe_core::with_anchors_every(&path, (length / 40.0).max(6.0 / view.zoom));
                let widths = line_widths(editor, &path, &points, &pressures);
                add_line_layer(editor, path, widths);
            } else {
                let style = Style { fill: None, stroke: Some(editor.pencil_stroke) };
                add_path_layer(editor, "Stroke", path, style);
            }
        }
        Gesture::Marquee { start, current, adding } => {
            let area = StageRect::from_points(start, current);
            let frame = editor.frame;
            let touched: Vec<LayerId> = editor
                .layers()
                .iter()
                .filter(|l| l.is_shown_at(frame) && !l.locked && !l.mask)
                .filter(|l| {
                    // The layer's box on the stage, from its corners, so rotated
                    // layers are judged by where they actually are.
                    let to_stage = editor.doc.layer_matrix(l, frame);
                    editor
                        .doc
                        .layer_bounds(l, frame)
                        .and_then(|b| StageRect::bounding(b.corners().map(|c| to_stage.apply(c))))
                        .is_some_and(|b| overlaps(b, area))
                })
                .map(|l| l.id)
                .collect();
            let mut selected = if adding { editor.selected_ids() } else { Vec::new() };
            for id in touched {
                if !selected.contains(&id) {
                    selected.push(id);
                }
            }
            editor.select_many(&selected);
        }
        Gesture::Move { .. }
        | Gesture::Scale { .. }
        | Gesture::Skew { .. }
        | Gesture::Rotate { .. }
        | Gesture::Pivot { .. }
        | Gesture::Hint { .. }
        | Gesture::Camera { .. }
        | Gesture::Reach { .. }
        | Gesture::Bend { .. }
        | Gesture::Warp { .. }
        | Gesture::Width { .. }
        | Gesture::MovePixels { .. }
        | Gesture::EditAnchor { .. }
        | Gesture::BendPath { .. }
        | Gesture::PathHandle { .. }
        | Gesture::Pan
        | Gesture::RotateView { .. } => {}
    }
}

/// The rigs in the timeline being edited, as the Bone tool shows them: a
/// bone from each parent's joint (its pivot) to each child's, so the
/// skeleton the layers hang from can be seen and grabbed. Roots are
/// squares, since dragging one moves its whole rig.
fn draw_skeleton(painter: &egui::Painter, editor: &Editor, view: View) {
    let frame = editor.frame;
    let joint = |layer: &inkframe_core::Layer| {
        view.to_screen(editor.doc.layer_matrix(layer, frame).apply(layer.pivot))
    };
    let shown = editor.layers().iter().filter(|l| l.is_shown_at(frame));
    let rigged: Vec<_> = shown
        .filter(|l| {
            editor.doc.parent_of(l).is_some() || editor.layers().iter().any(|c| c.parent == Some(l.id))
        })
        .collect();
    for layer in &rigged {
        if let Some(parent) = editor.doc.parent_of(layer) {
            let (from, to) = (joint(parent), joint(layer));
            painter.line_segment([from, to], (5.0, BONE.gamma_multiply(0.35)));
            painter.line_segment([from, to], (1.5, BONE));
        }
    }
    for layer in &rigged {
        let at = joint(layer);
        let selected = editor.is_selected(layer.id);
        let fill = if selected { BONE } else { Color32::WHITE };
        if editor.doc.parent_of(layer).is_none() {
            let square = Rect::from_center_size(at, Vec2::splat(9.0));
            painter.rect_filled(square, 1.0, fill);
            painter.rect_stroke(square, 1.0, (1.5, BONE), StrokeKind::Middle);
        } else {
            painter.circle(at, 4.5, fill, (1.5, BONE));
        }
    }
}

/// A circle the size the brush (or eraser) paints at full pressure, around
/// the pointer, scaled like the canvas it would paint on.
fn draw_brush_outline(painter: &egui::Painter, editor: &Editor, view: View, pointer: Pos2) {
    let Some(settings) = editor.brush_for(editor.tool) else { return };
    let canvas_scale = editor
        .paint_target()
        .and_then(|(layer, ..)| editor.doc.layer(layer))
        .map_or(1.0, |layer| editor.doc.layer_matrix(layer, editor.frame).average_scale());
    let radius = settings.size / 2.0 * canvas_scale * view.zoom;
    // Dark and light rings, so it shows on any colour.
    painter.circle_stroke(pointer, radius, (2.0, Color32::from_black_alpha(140)));
    painter.circle_stroke(pointer, radius, (1.0, Color32::WHITE));
}

/// Gets a stroke ready on the selected paint layer, first adding a paint
/// layer if the selected layer isn't one (as Photoshop would ask for).
fn start_painting(editor: &mut Editor, view: View, press: Pos2) -> Option<Painting> {
    if editor.paint_target().is_none() {
        editor.add_paint_layer();
    }
    let (layer, canvas, width, height) = editor.paint_target()?;
    let size = (width, height);
    // Painting stays inside the selection, if there is one.
    let selection = editor.selection_on(canvas).cloned();
    let work = match editor.tool {
        Tool::Brush => PaintWork::Brush(
            BrushStroke::new(editor.brush, BrushMode::Paint(editor.paint_color)).within(selection.clone()),
        ),
        Tool::Eraser => {
            PaintWork::Brush(BrushStroke::new(editor.eraser, BrushMode::Erase).within(selection.clone()))
        }
        Tool::Gradient => PaintWork::Gradient { from: press },
        tool => {
            let kind = retouch_kind(editor, tool, view, layer, size, press)?;
            let brush = editor.retouch_of(tool)?.brush;
            PaintWork::Retouch(RetouchStroke::new(brush, kind, editor.doc.canvas(canvas)?).within(selection))
        }
    };
    Some(Painting {
        layer,
        canvas,
        size,
        work,
        smoothed: Pos2::ZERO,
        pen: tablet::Sample { pos: Pos2::ZERO, pressure: 1.0 },
    })
}

/// What a retouching tool does, with its settings. The Clone Stamp needs a
/// source on this layer (Alt-click), and keeps the distance to it for the
/// strokes after the first.
fn retouch_kind(
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
fn canvas_point(
    editor: &Editor,
    view: View,
    layer: LayerId,
    size: (u32, u32),
    screen: Pos2,
) -> Option<Point> {
    let layer = editor.doc.layer(layer)?;
    let local = editor.doc.layer_matrix(layer, editor.frame).inverse()?.apply(view.to_local(screen));
    Some(Point::new(local.x + size.0 as f32 / 2.0, local.y + size.1 as f32 / 2.0))
}

/// The opposite of [`canvas_point`]: a canvas pixel on screen.
fn canvas_to_screen(editor: &Editor, view: View, layer: LayerId, at: Point) -> Option<Pos2> {
    let layer = editor.doc.layer(layer)?;
    let size = layer.elements_at(editor.frame).iter().find_map(|e| match e.shape {
        Shape::Paint { width, height, .. } => Some((width, height)),
        _ => None,
    })?;
    let local = Point::new(at.x - size.0 as f32 / 2.0, at.y - size.1 as f32 / 2.0);
    Some(view.to_screen(editor.doc.layer_matrix(layer, editor.frame).apply(local)))
}

/// Carries the stroke (or the gradient's drag) on to `screen`.
fn paint_to(painting: &mut Painting, editor: &mut Editor, view: View, screen: Pos2, pressure: f32) {
    let Some(at) = canvas_point(editor, view, painting.layer, painting.size, screen) else { return };
    let Some(canvas) = editor.doc.canvas_mut(painting.canvas) else { return };
    match &mut painting.work {
        PaintWork::Brush(stroke) => stroke.add_point(canvas, at, pressure),
        PaintWork::Retouch(stroke) => stroke.add_point(canvas, at, pressure),
        PaintWork::Gradient { .. } => {}
    }
}

/// Lays the gradient dragged from `from` to `to` (on screen).
fn lay_gradient(painting: &Painting, editor: &mut Editor, view: View, from: Pos2, to: Pos2) {
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
fn fill_at(editor: &mut Editor, view: View, press: Pos2) {
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

/// How close to the polygonal lasso's first corner a click closes it, in
/// screen points.
const POLYGON_CLOSE: f32 = 8.0;

/// Makes a selection on the selected paint layer's canvas and combines it
/// with the one there. `make` gets the canvas size and a way to turn screen
/// points into canvas pixels.
fn finish_selection(
    editor: &mut Editor,
    view: View,
    combine: Combine,
    make: impl FnOnce(u32, u32, &dyn Fn(Pos2) -> Point) -> Selection,
) {
    let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
    let to_canvas = |p: Pos2| canvas_point(editor, view, layer, (width, height), p).unwrap_or(Point::ZERO);
    let new = make(width, height, &to_canvas);
    let combined = match editor.selection_on(canvas) {
        Some(existing) => existing.combine(&new, combine),
        None if matches!(combine, Combine::Subtract | Combine::Intersect) => return,
        None => new,
    };
    editor.pixel_selection = (!combined.is_empty()).then_some((canvas, combined));
}

/// Alt-click with the Clone Stamp: copy from here on.
fn set_clone_source(editor: &mut Editor, view: View, press: Pos2) {
    let Some((layer, _, width, height)) = editor.paint_target() else { return };
    editor.clone_source = canvas_point(editor, view, layer, (width, height), press).map(|p| (layer, p));
    editor.clone_offset = None;
}

/// Sets the paint colour to what the stage shows at `at` (in the
/// coordinates of the timeline being edited).
fn pick_color(editor: &mut Editor, at: Point) {
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

/// Every deformer's bones in the timeline being edited, as they're bent on
/// this frame, in screen coordinates: `(layer, joints)`.
fn deformer_chains(editor: &Editor, view: View) -> Vec<(LayerId, Vec<Pos2>)> {
    let frame = editor.frame;
    editor
        .layers()
        .iter()
        .filter(|l| l.is_shown_at(frame) && !l.locked)
        .filter_map(|layer| {
            let deformer = layer.deformer.as_ref()?;
            let to_stage = editor.doc.layer_matrix(layer, frame);
            let joints = deformer.posed_joints(frame).into_iter().map(|j| view.to_screen(to_stage.apply(j)));
            Some((layer.id, joints.collect()))
        })
        .collect()
}

/// The deformer joint under `pointer`, as the layer and the bone that
/// ends there (the chain's first joint is its anchor, so it isn't one).
fn deformer_joint_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, usize)> {
    deformer_chains(editor, view).into_iter().rev().find_map(|(layer, joints)| {
        let bone = joints.iter().skip(1).position(|j| j.distance(pointer) <= ANCHOR_HIT)?;
        Some((layer, bone))
    })
}

/// Every warp grid in the timeline being edited, its points on screen,
/// row by row, with its size.
fn warp_grids(editor: &Editor, view: View) -> Vec<(LayerId, usize, Vec<Pos2>)> {
    let frame = editor.frame;
    editor
        .layers()
        .iter()
        .filter(|l| l.is_shown_at(frame) && !l.locked)
        .filter_map(|layer| {
            let warp = layer.warp.as_ref()?;
            let to_stage = editor.doc.layer_matrix(layer, frame);
            let points = warp.points_at(frame).into_iter().map(|p| view.to_screen(to_stage.apply(p)));
            Some((layer.id, warp.columns, points.collect()))
        })
        .collect()
}

/// The warp grid point under `pointer`: its layer and index.
fn warp_point_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, usize)> {
    warp_grids(editor, view).into_iter().rev().find_map(|(layer, _, points)| {
        let point = points.iter().position(|p| p.distance(pointer) <= ANCHOR_HIT)?;
        Some((layer, point))
    })
}

/// Warp grids: the lines between their points, and the points as small
/// squares to drag, in the deformers' green.
fn draw_warps(painter: &egui::Painter, editor: &Editor, view: View) {
    let line = (1.0, DEFORMER.gamma_multiply(0.7));
    for (_, columns, points) in warp_grids(editor, view) {
        let rows = points.len() / columns.max(1);
        for row in 0..rows {
            painter.add(egui::Shape::line(points[row * columns..(row + 1) * columns].to_vec(), line));
        }
        for column in 0..columns {
            painter
                .add(egui::Shape::line((0..rows).map(|row| points[row * columns + column]).collect(), line));
        }
        for &at in &points {
            painter.rect(
                Rect::from_center_size(at, Vec2::splat(7.0)),
                1.0,
                Color32::WHITE,
                (1.5, DEFORMER),
                egui::StrokeKind::Middle,
            );
        }
    }
}

/// Deformer bones, drawn as a chain through the bent drawing: thinner than
/// the skeleton's and green, since they bend a drawing rather than move
/// layers. The anchor joint is a square.
fn draw_deformers(painter: &egui::Painter, editor: &Editor, view: View) {
    for (_, joints) in deformer_chains(editor, view) {
        painter.add(egui::Shape::line(joints.clone(), (1.5, DEFORMER)));
        for (i, &at) in joints.iter().enumerate() {
            if i == 0 {
                let square = Rect::from_center_size(at, Vec2::splat(7.0));
                painter.rect_filled(square, 1.0, DEFORMER);
            } else {
                painter.circle(at, 4.0, Color32::WHITE, (1.5, DEFORMER));
            }
        }
    }
}

const DEFORMER: Color32 = Color32::from_rgb(0x4c, 0xc2, 0x8a);

/// The skeleton's color: warm, so it stands apart from the blue selection.
const BONE: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x3c);

/// A small crosshair marking a symbol's registration point: the origin its
/// instances rotate and scale around.
fn draw_registration_point(painter: &egui::Painter, at: Pos2) {
    let stroke = (1.5, Color32::from_gray(40));
    painter.line_segment([at - vec2(8.0, 0.0), at + vec2(8.0, 0.0)], stroke);
    painter.line_segment([at - vec2(0.0, 8.0), at + vec2(0.0, 8.0)], stroke);
    painter.circle_stroke(at, 4.0, stroke);
}

/// Switches an anchor between corner and smooth. Rectangles and ovals
/// become paths.
fn convert_point(editor: &mut Editor, target: AnchorRef) {
    let Some(element) = editor.element_mut(target.layer, target.element) else {
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
    let Some(element) = editor.element_mut(hit.layer, hit.element) else {
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
/// The pen's pressure now, from 0 to 1; 1 with a mouse.
fn pen_pressure() -> f32 {
    tablet::current().map_or(1.0, |pen| pen.pressure)
}

/// How wide a pencil line is at each anchor of `path`: the pencil's width
/// times the pressure drawn nearest it (if pressure is on), narrowing over
/// the ends (if tapering is on).
fn line_widths(editor: &Editor, path: &Path, points: &[Point], pressures: &[f32]) -> Vec<f32> {
    let anchors = inkframe_core::centreline_anchors(path);
    // How far along the line each anchor is.
    let mut along = vec![0.0];
    for pair in anchors.windows(2) {
        along.push(along.last().copied().unwrap_or(0.0) + pair[0].distance(pair[1]));
    }
    let total = along.last().copied().unwrap_or(0.0);
    let taper = (total * 0.25).clamp(1e-3, 60.0);
    anchors
        .iter()
        .zip(&along)
        .map(|(anchor, &distance)| {
            let nearest = (0..points.len())
                .min_by(|&a, &b| points[a].distance(*anchor).total_cmp(&points[b].distance(*anchor)))
                .unwrap_or(0);
            // Light touches still leave a thin line, as on paper.
            let pressure = if editor.pencil_pressure {
                pressures.get(nearest).copied().unwrap_or(1.0).max(0.1)
            } else {
                1.0
            };
            let end = if editor.pencil_taper {
                (distance.min(total - distance) / taper).clamp(0.15, 1.0)
            } else {
                1.0
            };
            editor.pencil_stroke.width * pressure * end
        })
        .collect()
}

/// Adds a pencil line, centred on its own middle, as `add_path_layer`
/// does for paths.
fn add_line_layer(editor: &mut Editor, mut path: Path, widths: Vec<f32>) {
    let Some(bounds) = StageRect::bounding(path.flatten()) else { return };
    let center = bounds.center();
    path.translate(-center);
    let style = Style { fill: Some(editor.pencil_stroke.color), stroke: None };
    editor.add_drawn_shape("Line", Element { shape: Shape::Line { path, widths }, style }, center);
}

fn add_path_layer(editor: &mut Editor, prefix: &str, mut path: Path, style: Style) {
    let Some(bounds) = StageRect::bounding(path.flatten()) else {
        return;
    };
    let center = bounds.center();
    path.translate(-center);
    editor.add_drawn_shape(prefix, Element { shape: Shape::Path(path), style }, center);
}

/// Text being typed into on the stage.
#[derive(Clone, Copy)]
struct TextEditing {
    layer: LayerId,
    /// Which of the layer's elements is the text.
    element: usize,
    cursor: TextCursor,
    /// While the mouse button that placed the cursor is held, dragging selects.
    selecting: bool,
}

/// Moves, scales and turns several layers as one, by however the imaginary
/// layer around them changed from `before` to `after`.
///
/// Each layer's position, relative to the group's middle, is scaled and
/// turned the same way the group was, and its own scale and rotation change
/// by the same amounts. That's exact for even scaling, any rotation, and
/// layers that aren't turned.
///
/// Stretching a turned (or skewed) layer unevenly along the group's axes
/// slants it, which its own scale can't express. For those, the layer's new
/// matrix is worked out directly and turned back into a pose, skew included
/// (`Transform::from_affine`).
///
/// A layer with a parent has its pose in the parent's space, so it always
/// takes the matrix route: the change is carried into that space first.
fn transform_group(editor: &mut Editor, group: &[GroupMember], before: &Transform, after: &Transform) {
    let factor = Point::new(after.scale.x / before.scale.x, after.scale.y / before.scale.y);
    let turn = (after.rotation - before.rotation).to_radians();
    let (sin, cos) = turn.sin_cos();
    let uneven = (factor.x - factor.y).abs() > 1e-4;
    let group_change = before.to_affine().inverse().map(|undo| after.to_affine().pre_concat(undo));
    for &GroupMember { id, pose: start, space } in group {
        let slanted = start.rotation % 90.0 != 0.0 || start.skew != Point::ZERO;
        let parented = space != Affine::IDENTITY;
        let in_space = |change: Affine| Some(space.inverse()?.pre_concat(change).pre_concat(space));
        let pose = match group_change.and_then(in_space) {
            Some(change) if parented || (uneven && slanted) => {
                let mut pose = start.from_affine(change.pre_concat(start.to_affine()));
                // The matrix only knows the angle modulo a full turn; keep the
                // layer's own count of turns, so a tween through it still spins
                // the way it did.
                let intended = start.rotation + (after.rotation - before.rotation);
                pose.rotation += 360.0 * ((intended - pose.rotation) / 360.0).round();
                pose
            }
            _ => {
                let offset = start.position - before.position;
                let scaled = Point::new(offset.x * factor.x, offset.y * factor.y);
                let turned = Point::new(scaled.x * cos - scaled.y * sin, scaled.x * sin + scaled.y * cos);
                Transform {
                    position: after.position + turned,
                    rotation: start.rotation + (after.rotation - before.rotation),
                    scale: Point::new(start.scale.x * factor.x, start.scale.y * factor.y),
                    ..start
                }
            }
        };
        editor.set_pose(id, pose);
    }
}

/// Maps timeline coordinates into the space a layer's position keys are in:
/// its parent's.
fn into_parent_space(editor: &Editor, layer: LayerId) -> Affine {
    let layer = editor.doc.layer(layer);
    layer.and_then(|l| editor.doc.parent_matrix(l, editor.frame).inverse()).unwrap_or(Affine::IDENTITY)
}

/// One layer of a multi-layer selection, as it was when a gesture began.
#[derive(Debug, Clone, Copy)]
struct GroupMember {
    id: LayerId,
    pose: Transform,
    /// Its parent's placement on the timeline (identity without a parent).
    space: Affine,
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    to_point(p).distance_to_segment(to_point(a), to_point(b))
}

fn overlaps(a: StageRect, b: StageRect) -> bool {
    a.min.x <= b.max.x && a.max.x >= b.min.x && a.min.y <= b.max.y && a.max.y >= b.min.y
}

/// Whether a layer is travelling along a motion guide.
fn follows_guide(editor: &Editor, layer: &inkframe_core::Layer) -> bool {
    layer.follow.is_some_and(|guide| editor.doc.guide_path(guide, editor.frame as f32).is_some())
}

/// The index of the first text element on a layer.
fn text_element(editor: &Editor, layer: LayerId) -> Option<usize> {
    editor.doc.layer(layer)?.elements_at(editor.frame).iter().position(|e| matches!(e.shape, Shape::Text(_)))
}

fn text_of(editor: &Editor, layer: LayerId, element: usize) -> Option<&Text> {
    match &editor.element(layer, element)?.shape {
        Shape::Text(text) => Some(text),
        _ => None,
    }
}

fn text_of_mut(editor: &mut Editor, layer: LayerId, element: usize) -> Option<&mut Text> {
    match &mut editor.element_mut(layer, element)?.shape {
        Shape::Text(text) => Some(text),
        _ => None,
    }
}

/// Maps the timeline's coordinates into a layer's own, where its shapes are.
fn layer_to_local(editor: &Editor, layer: LayerId) -> Option<Affine> {
    editor.doc.layer_matrix(editor.doc.layer(layer)?, editor.frame).inverse()
}

/// Moves `end` so the rectangle from `start` is a square, keeping the drag direction.
fn constrain_square(start: Point, end: Point) -> Point {
    let d = end - start;
    let side = d.x.abs().max(d.y.abs());
    start + Point::new(side.copysign(d.x), side.copysign(d.y))
}

/// The classic grey checkerboard that signals "transparent here".
/// The grey and white squares that show the stage is transparent, turned
/// with the view. Squares are about 10 screen points, but bigger when
/// zoomed far in, so there are never too many to draw.
fn draw_checkerboard(painter: &egui::Painter, view: View) {
    let corners = view.stage_corners();
    painter.add(egui::Shape::convex_polygon(corners.to_vec(), Color32::from_gray(204), egui::Stroke::NONE));
    let mut cell = 10.0 / view.stage_zoom;
    while (view.size.x / cell) * (view.size.y / cell) > 20_000.0 {
        cell *= 2.0;
    }
    let stage = View { timeline_to_stage: Affine::IDENTITY, ..view };
    let clip = painter.clip_rect();
    let (columns, rows) = ((view.size.x / cell).ceil() as i32, (view.size.y / cell).ceil() as i32);
    for row in 0..rows {
        for col in 0..columns {
            if (row + col) % 2 == 0 {
                continue;
            }
            let (x0, y0) = (col as f32 * cell, row as f32 * cell);
            let (x1, y1) = ((x0 + cell).min(view.size.x), (y0 + cell).min(view.size.y));
            let square: Vec<Pos2> = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                .iter()
                .map(|&(x, y)| stage.to_screen(Point::new(x, y)))
                .collect();
            if square.iter().any(|p| clip.contains(*p)) {
                painter.add(egui::Shape::convex_polygon(square, Color32::from_gray(153), egui::Stroke::NONE));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let Shape::Rect { width, height, .. } = layer.drawings[0].elements[0].shape else {
            panic!("not a rect")
        };
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
                style: Style { fill: Some(Color::BLACK), stroke: None },
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
            style: Style { fill: None, stroke: Some(Stroke { color: Color::BLACK, width: 3.0 }) },
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
        let Shape::Line { path: stroke, .. } = &layer.drawings[0].elements[0].shape else {
            panic!("not a line")
        };
        assert!(stroke.segments.len() >= 2, "a wavy stroke should keep several segments");
        // The path was recentered, so its local bounds straddle the origin.
        let bounds = h.state().1.doc.layer_bounds(layer, 0).unwrap();
        assert!(bounds.center().distance(Point::ZERO) < 30.0);

        // With pressure and tapering off, it's a plain stroke.
        h.state_mut().1.pencil_pressure = false;
        h.state_mut().1.pencil_taper = false;
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
            style: Style { stroke: None, ..editor.draw_style },
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
            Some(Stroke { color: Color::BLACK, width: 40.0 });
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
            style: Style { fill: Some(inkframe_core::Color::BLACK), stroke: None },
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
            style: Style { stroke: None, ..editor.draw_style },
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
        let style = Style { stroke: None, ..editor.draw_style };
        let rect =
            |width, height| Element { shape: Shape::Rect { width, height, corner_radius: 0.0 }, style };
        let doc = &mut editor.doc;
        let torso =
            doc.add_layer("Torso", vec![rect(100.0, 100.0)], 0, Transform::at(Point::new(300.0, 400.0)));
        let upper =
            doc.add_layer("Upper", vec![rect(200.0, 40.0)], 0, Transform::at(Point::new(500.0, 400.0)));
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
            style: Style { stroke: None, ..editor.draw_style },
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
            style: Style { stroke: None, ..editor.draw_style },
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
        let (from, middle, to) =
            (screen(&h, 300.0, 500.0), screen(&h, 600.0, 500.0), screen(&h, 900.0, 500.0));
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
        let (from, middle, to) =
            (screen(&h, 400.0, 500.0), screen(&h, 900.0, 500.0), screen(&h, 1400.0, 500.0));
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
        let Shape::Rect { width, height, .. } = drawn.drawings[0].elements[0].shape else {
            panic!("not a rect")
        };
        assert!((width - 60.0).abs() < 1.5 && (height - 30.0).abs() < 1.5, "{width}×{height}");
        let center = drawn.transform_at(0).position;
        assert!(center.distance(Point::new(30.0, -15.0)) < 1.5, "{center:?}");
    }

    #[test]
    fn zooming_keeps_the_anchor_fixed() {
        let doc = Document::default();
        let mut stage = StageView {
            last_area: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            ..Default::default()
        };
        let anchor = pos2(200.0, 150.0);
        let before = stage.view(stage.last_area, &doc).to_local(anchor);
        stage.set_zoom_about(&doc, anchor, 2.0);
        let after = stage.view(stage.last_area, &doc).to_local(anchor);
        assert!(before.distance(after) < 1e-3);
    }
}
