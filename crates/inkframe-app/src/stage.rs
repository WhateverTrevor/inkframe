//! The stage: the canvas where the animation is displayed and drawn on.
//!
//! The document is rendered by `inkframe-render` into a texture, so the stage
//! shows exactly what an export would produce. Editing feedback (selection
//! outlines, shapes being dragged out) is painted on top with egui, because
//! it's interface rather than artwork.

mod anchors;
mod bones;
mod decorations;
mod gradient_handles;
mod hints;
mod inside;
mod lattice;
mod motion;
mod painting;
mod pen_tool;
mod perspective;
mod rigs;
mod rulers;
mod selecting;
mod text_editing;
mod transform_handles;
mod width_tool;

use std::collections::HashMap;

use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{
    Affine, Anchor, Bend, Canvas, CanvasId, Document, Element, LayerId, Path, Point, Rect as StageRect,
    Selection, Shape, Style, TimelineId, Transform, fit_freehand_path, path_from_anchors,
};
use inkframe_render::{RenderOptions, output_size, render_timeline_into};

use crate::convert::{to_color32, to_point};
use crate::editor::{Editor, OnionSkin, Surrounding, Tool};
use crate::handles::{RotateDrag, ScaleHandle, ScaleOptions, scale_pose, skew_pose};
use crate::i18n::tr;
use crate::pen::PenPath;
use crate::subselect::{AnchorPart, move_part, pull_handles, remove_anchor};
use crate::text_edit::TypingHistory;
use crate::{library, motion_path, tablet};
use anchors::{AnchorRef, convert_point, insert_point};
use decorations::{
    draw_camera_frame, draw_checkerboard, draw_emitters, draw_empty_stage_hint, draw_guides,
    draw_onion_skins, draw_registration_point, draw_stage_shadow,
};
use hints::{draw_hints, hint_at};
use motion::PathHit;
use painting::{Painting, canvas_point, canvas_to_screen, draw_brush_outline, draw_symmetry};
use rigs::{deformer_joint_at, draw_deformers, draw_skeleton, draw_warps, warp_point_at};
use selecting::Selecting;
use text_editing::{TextEditing, text_element, text_of};
use transform_handles::{HandleHit, Handles};
use width_tool::{LinePoint, draw_line_widths, line_point_at, line_shape_mut};

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
    Move {
        layers: Vec<(LayerId, Point, Affine)>,
        grab: Point,
        multi: Option<crate::frame_commands::MultiFrameMove>,
        /// The box round what's moving when the drag began, for snapping
        /// to guides.
        bounds: Option<StageRect>,
    },
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
    /// Dragging an edge of the selection box to slant the layer (or, with
    /// `group`, the selected layers together). `grab` is where the drag
    /// started, in the space `start` is in.
    Skew {
        layer: LayerId,
        edge: ScaleHandle,
        start: Transform,
        bounds: StageRect,
        grab: Point,
        group: Vec<GroupMember>,
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
    /// tool (or the Pen, between paths). `start` holds the shape's anchors
    /// from when the drag began, and the pointer is converted into the
    /// layer's local space with `to_local`.
    EditAnchor {
        target: AnchorRef,
        part: AnchorPart,
        /// Pulling new handles out of the point (Alt-dragging it) rather
        /// than moving it.
        pull: bool,
        start: Vec<Anchor>,
        closed: bool,
        /// The points of other shapes welded to a point being moved (see
        /// `weld.rs` in the core), which go with it: each shape's place, the
        /// point's place in it, and the shape's points and closedness as
        /// they were when the drag began.
        welds: Vec<(usize, usize, Vec<Anchor>, bool)>,
        /// Points of other shapes the dragged point snaps onto, welding to
        /// them, in local coordinates.
        snaps: Vec<Point>,
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

/// Everything besides the document that affects the rendered stage image
/// (the document is compared in place, and only copied when the stage is
/// redrawn). If none of it changed since last time, we can skip
/// re-rendering and reuse the texture.
#[derive(PartialEq)]
struct RenderKey {
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
    /// Ctrl+Option-dragging (Alt+right-dragging on Windows) the brush's
    /// size and hardness on the canvas, as in Photoshop: where it began, and
    /// the size and hardness then.
    sizing_brush: Option<(Pos2, f32, f32)>,
    /// Dragging symmetry's centre.
    moving_symmetry: bool,
    /// A selection being drawn with the marquee or lasso.
    selecting: Option<Selecting>,
    /// The selection's outline (marching ants), worked out once per
    /// selection: which selection, and its edges in canvas pixels.
    ants: Option<(usize, Vec<(Point, Point)>)>,
    texture: Option<egui::TextureHandle>,
    rendered: Option<RenderKey>,
    /// The document the texture was drawn from.
    rendered_doc: Option<Document>,
    /// Onion skin ghosts drawn so far (see `GhostCache`).
    ghosts: GhostCache,
    /// The viewport from the last frame, for zoom commands that come from menus.
    last_area: Rect,
    /// A drag inside a layer (see `inside.rs`).
    shape_drag: Option<inside::ShapeDrag>,
    /// Undo for the text being typed into.
    typing: TypingHistory,
    /// A gradient's end being dragged (see `gradient_handles.rs`).
    gradient_drag: Option<gradient_handles::GradientDrag>,
    /// A guide being dragged (see `rulers.rs`).
    guide_drag: Option<rulers::GuideDrag>,
    /// The Distort or Envelope handle being dragged (see `lattice.rs`).
    lattice_drag: Option<usize>,
    /// A skeleton's bone being drawn, moved or posed (see `bones.rs`).
    bone_drag: Option<bones::BoneDrag>,
    /// A perspective guide's vanishing point being dragged (see
    /// `perspective.rs`).
    perspective_drag: Option<usize>,
    /// ⌘ was held as the current drag began: a welded point comes away
    /// from the shapes it's welded to.
    unweld: bool,
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
            sizing_brush: None,
            moving_symmetry: false,
            selecting: None,
            ants: None,
            texture: None,
            rendered: None,
            rendered_doc: None,
            ghosts: GhostCache::default(),
            last_area: Rect::NOTHING,
            shape_drag: None,
            typing: TypingHistory::default(),
            gradient_drag: None,
            guide_drag: None,
            lattice_drag: None,
            bone_drag: None,
            unweld: false,
            perspective_drag: None,
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
        // Skeletons show at rest in rig setup, and while bones are being
        // added (which are drawn at rest).
        editor.doc.rest_pose.0 = editor.rig_setup || editor.tool == Tool::AddBone;
        editor.check_preview();
        editor.check_correction();

        self.handle_navigation(ui, &response, &editor.doc);
        let stage_view = self.view(area, &editor.doc);
        let corners = stage_view.stage_corners();
        let stage_size = stage_view.size * stage_view.stage_zoom;
        // Everything below works in the coordinates of the timeline being
        // edited: the stage's on the main timeline, a symbol's inside one.
        let view = stage_view.with_timeline(editor.timeline_transform());

        painter.rect_filled(area, 0.0, crate::theme::PASTEBOARD);
        draw_stage_shadow(&painter.with_clip_rect(area), &corners);
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
        painter.add(egui::Shape::closed_line(corners.to_vec(), (1.0, Color32::from_black_alpha(90))));

        // Switching away from the pen (or anything else that changes the tool)
        // finishes the path in progress rather than silently dropping it.
        if editor.tool != Tool::Pen && self.pen.is_some() {
            self.finish_path(editor, false);
        }
        self.validate_anchor_selection(editor);

        // Typing into text takes the keyboard, and the mouse when it's on
        // the text; the tools get everything else.
        let pointer_on_text = self.handle_text_editing(ui, &response, editor, view);
        // Rulers and guides first: they're over everything, at the edges.
        let on_guide = !pointer_on_text
            && (self.handle_rulers(ui, &response, editor, stage_view)
                || self.handle_perspective(ui, &response, editor, stage_view));
        let pointer_on_text = pointer_on_text || on_guide;
        // A Distort or Envelope's handles, then a gradient's: they sit on
        // top of the shapes.
        let on_lattice = !pointer_on_text && self.handle_lattice(ui, &response, editor, view);
        // A skeleton's bones, over the drawing they bend.
        let on_bone = !pointer_on_text && !on_lattice && self.handle_bones(ui, &response, editor, view);
        let pointer_on_text = pointer_on_text || on_lattice || on_bone;
        let on_gradient = !pointer_on_text && self.handle_gradient(ui, &response, editor, view);
        let inside_layer =
            !pointer_on_text && !on_gradient && self.handle_inside(ui, &response, editor, view);
        if !pointer_on_text && !inside_layer && !on_gradient {
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
        // Nor while inside a layer: the picked shapes have handles of their own.
        // Nor under a Distort or Envelope's.
        let handles =
            if self.text_editing.is_some() || editor.inside_layer().is_some() || editor.lattice.is_some() {
                None
            } else {
                Handles::for_selection(editor, view)
            };
        let overlay = painter.with_clip_rect(area);
        if editor.timeline() != TimelineId::Main {
            draw_registration_point(&overlay, view.to_screen(Point::ZERO));
        }
        draw_guides(&overlay, editor, view);
        draw_emitters(&overlay, editor, view);
        self.draw_motion_path(&overlay, editor, view);
        self.draw_overlays(&overlay, editor, view, handles.as_ref());
        if editor.tool == Tool::Bone {
            draw_skeleton(&overlay, editor, view);
            draw_deformers(&overlay, editor, view);
            draw_warps(&overlay, editor, view);
        }
        self.draw_text_editing(ui, &overlay, editor, view);
        self.draw_pen_preview(&overlay, view, response.hover_pos());
        // While sizing, the outline stays where the drag began.
        if editor.tool.uses_brush()
            && let Some(pointer) = self.sizing_brush.map(|(origin, ..)| origin).or(response.hover_pos())
        {
            draw_brush_outline(&overlay, editor, view, pointer);
        }
        draw_symmetry(&overlay, editor, view);
        self.draw_painting(&overlay, editor, view);
        self.draw_selection(ui, &overlay, editor, view);
        if editor.layers().is_empty() && editor.timeline() == TimelineId::Main {
            draw_empty_stage_hint(&overlay, &corners, editor.doc.background);
        }
        if let Some(hit) = response.hover_pos().and_then(|p| self.outline_hit(editor, view, p)) {
            // Show where the new point would land: a hollow anchor with a plus.
            overlay.circle(hit.screen, 5.0, Color32::WHITE, (1.0, SELECTION));
            overlay
                .line_segment([hit.screen - vec2(3.0, 0.0), hit.screen + vec2(3.0, 0.0)], (1.0, SELECTION));
            overlay
                .line_segment([hit.screen - vec2(0.0, 3.0), hit.screen + vec2(0.0, 3.0)], (1.0, SELECTION));
        }
        self.update_cursor(ui, &response, editor, view, handles.as_ref());
        if editor.inside_layer().is_some() {
            self.draw_inside(&overlay, editor, view);
            self.inside_cursor(ui, &response, editor, view);
        }
        self.draw_gradient_handles(ui, &overlay, &response, editor, view);
        self.draw_lattice(ui, &overlay, &response, editor, view);
        self.draw_bones(ui, &overlay, &response, editor, view);
        bones::draw_setup_note(&overlay, editor);
        self.draw_perspective(ui, &overlay, &response, editor, stage_view);
        self.draw_rulers(ui, &overlay, editor, stage_view);
        self.guide_cursor(ui, &response, editor, stage_view);
        asset_drop(ui, &overlay, &response, editor, stage_view);
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
            self.unweld = ui.input(|i| i.modifiers.command);
            if let Some(press) = press {
                self.gesture = self.begin_gesture(editor, press, view, space_held, shift, alt);
            }
        }

        if let Some(pointer) = response.interact_pointer_pos() {
            let p = view.to_local(pointer);
            match &mut self.gesture {
                Some(Gesture::Move { layers, grab, multi, bounds }) => {
                    let mut delta = p - *grab;
                    if shift {
                        // Constrain to the dominant axis, like every design tool.
                        if delta.x.abs() > delta.y.abs() { delta.y = 0.0 } else { delta.x = 0.0 }
                    }
                    // An edge or the middle of what's moving lands on a
                    // guide it comes near.
                    if let Some(bounds) = bounds
                        && let Some(guides) = snapping_guides(editor)
                    {
                        let moved = StageRect::from_points(bounds.min + delta, bounds.max + delta);
                        delta = delta + inkframe_core::snap_box(guides, moved, SNAP_REACH / view.zoom);
                    }
                    // Edit Multiple Frames: across the picked frames, not
                    // just this one (see `frame_commands.rs`).
                    if let Some(multi) = multi {
                        editor.apply_multi_frame_move(multi, *grab, delta);
                    } else {
                        for &(id, start, into_space) in layers.iter() {
                            if let Some(layer) = editor.doc.layer(id) {
                                // The same distance on screen, measured in the
                                // parent's space, which may be turned or scaled.
                                let delta = into_space.apply(*grab + delta) - into_space.apply(*grab);
                                let pose = Transform {
                                    position: start + delta,
                                    ..editor.doc.pose(layer, editor.frame)
                                };
                                editor.set_pose(id, pose);
                            }
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
                Some(Gesture::Skew { layer, edge, start, bounds, grab, group, into_space }) => {
                    let pose = skew_pose(start, *bounds, *edge, *grab, into_space.apply(p));
                    if group.is_empty() {
                        editor.set_pose(*layer, pose);
                    } else {
                        transform_group(editor, group, start, &pose);
                    }
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
                Some(Gesture::EditAnchor {
                    target,
                    part,
                    pull,
                    start,
                    closed,
                    welds,
                    snaps,
                    grab,
                    to_local,
                }) => {
                    // Move the part by however far the pointer has moved, so
                    // grabbing slightly off-center doesn't make it jump.
                    let local = to_local.apply(p);
                    let mut goal = part.position(&start[target.index]) + (local - *grab);
                    // A point dragged onto another shape's point lands
                    // exactly on it, welding them.
                    if *part == AnchorPart::Point && !*pull {
                        // The pointer's reach on screen, in the layer's units.
                        let near = ANCHOR_HIT / view.zoom * to_local.average_scale();
                        if let Some(snap) = snaps
                            .iter()
                            .copied()
                            .filter(|s| s.distance(goal) <= near)
                            .min_by(|a, b| a.distance(goal).total_cmp(&b.distance(goal)))
                        {
                            goal = snap;
                        }
                        // Points welded to it go the same way.
                        let delta = goal - start[target.index].point;
                        for (element, index, anchors, closed) in welds.iter() {
                            let moved = move_part(
                                anchors,
                                *index,
                                AnchorPart::Point,
                                anchors[*index].point + delta,
                                false,
                            );
                            if let (Some(shape), Some(path)) = (
                                editor.element_mut(target.layer, *element),
                                path_from_anchors(&moved, *closed),
                            ) {
                                shape.shape = Shape::Path(path);
                            }
                        }
                    }
                    let anchors = if *pull {
                        pull_handles(start, target.index, goal)
                    } else {
                        move_part(start, target.index, *part, goal, alt)
                    };
                    let element = editor.element_mut(target.layer, target.element);
                    if let (Some(element), Some(path)) = (element, path_from_anchors(&anchors, *closed)) {
                        // Rectangles and ovals become paths here, on the first
                        // real edit, rather than as soon as they're clicked.
                        element.shape = Shape::Path(path);
                    }
                }
                Some(Gesture::DrawShape { start, current, .. }) => {
                    let p = snap_to_guides(editor, view, p);
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
        // Double-clicking any other drawn layer steps inside it, to pick
        // out its shapes (see `inside.rs`).
        if response.double_clicked()
            && editor.tool == Tool::Select
            && editor.inside_layer().is_none()
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some(id) = editor.hit_test(view.to_local(pointer), HIT_TOLERANCE / view.zoom)
            && editor.enter_layer(id)
        {
            self.gesture = None;
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
        // The Pen, between paths, picks points the same way (see `handle_pen`
        // for clicks off them).
        let pen_on_anchor = editor.tool == Tool::Pen
            && self.pen.is_none()
            && response.interact_pointer_pos().is_some_and(|p| self.anchor_hit(editor, view, p).is_some());
        if response.clicked() && (editor.tool == Tool::Subselect || pen_on_anchor) {
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
                            let (start, bounds, group) = (handles.pose, handles.bounds, handles.group);
                            return Some(Gesture::Skew {
                                layer,
                                edge,
                                start,
                                bounds,
                                grab: q,
                                group,
                                into_space,
                            });
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
                    .collect::<Vec<_>>();
                let ids: Vec<LayerId> = layers.iter().map(|&(id, _, _)| id).collect();
                let multi = editor.edit_multiple.then(|| editor.begin_multi_frame_move(&ids));
                let bounds = ids
                    .iter()
                    .filter_map(|&id| editor.doc.layer(id))
                    .filter_map(|l| {
                        let to_stage = editor.doc.layer_matrix(l, frame);
                        let local = editor.doc.layer_bounds(l, frame)?;
                        StageRect::bounding(local.corners().map(|c| to_stage.apply(c)))
                    })
                    .reduce(StageRect::union);
                Some(Gesture::Move { layers, grab: p, multi, bounds })
            }
            tool @ (Tool::Rectangle | Tool::Ellipse) => {
                let p = snap_to_guides(editor, view, p);
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
                        multi: None,
                        bounds: None,
                    });
                }
                let local = editor.doc.layer_matrix(layer, editor.frame).inverse()?.apply(p);
                Some(Gesture::Reach { layer: layer.id, local })
            }
            // The pen and the painting tools handle their own presses and
            // drags; see `handle_pen` and `handle_painting`. The Text tool
            // only clicks.
            // Between paths, the Pen drags existing points as the
            // Subselection tool does.
            Tool::Pen if self.pen.is_none() => self.begin_anchor_edit(editor, view, press, p, alt),
            // Add Bone handles its own presses; see `bones.rs`.
            Tool::Pen | Tool::Text | Tool::AddBone => None,
            Tool::Brush
            | Tool::Eraser
            | Tool::Eyedropper
            | Tool::PaintBucket
            | Tool::Gradient
            | Tool::Mixer
            | Tool::Smudge
            | Tool::Blur
            | Tool::Sharpen
            | Tool::Dodge
            | Tool::Burn
            | Tool::CloneStamp
            | Tool::PatternStamp
            | Tool::RectMarquee
            | Tool::EllipseMarquee
            | Tool::Lasso
            | Tool::PolygonLasso
            | Tool::MagicWand => None,
            Tool::Subselect => {
                if self.anchor_hit(editor, view, press).is_none() {
                    // Dragging off the anchors selects a layer, but doesn't move it.
                    let tolerance = HIT_TOLERANCE / view.zoom;
                    editor.selection = editor.hit_test(p, tolerance);
                    self.anchor_selection = None;
                    return None;
                }
                self.begin_anchor_edit(editor, view, press, p, alt)
            }
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
            // Inside a layer, the picked shapes are outlined instead (see
            // `inside.rs`); the layer's own box and pivot would confuse them,
            // as they would a Distort or Envelope's handles.
            let bending = editor.lattice.as_ref().is_some_and(|edit| edit.layer() == layer.id);
            if !layer.is_shown_at(editor.frame)
                || shows_points
                || editor.inside_layer() == Some(layer.id)
                || bending
            {
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
                        let on_anchor = self.pen.is_none()
                            && response
                                .hover_pos()
                                .is_some_and(|p| self.anchor_hit(editor, view, p).is_some());
                        if can_close || on_anchor {
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
                    (None, Tool::AddBone) => CursorIcon::Crosshair,
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
                        | Tool::Mixer
                        | Tool::Smudge
                        | Tool::Blur
                        | Tool::Sharpen
                        | Tool::Dodge
                        | Tool::Burn
                        | Tool::CloneStamp
                        | Tool::PatternStamp
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
        // Inside a layer, the others fade, as with the light table.
        let light_table = editor.inside_layer().or(editor.selection.filter(|_| editor.light_table));
        let key = RenderKey {
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
        // Compared in place: copying the whole document on every update
        // just to compare it costs time on a big scene.
        let same_doc = self.rendered_doc.as_ref() == Some(doc);
        if same_doc && self.rendered.as_ref() == Some(&key) {
            return;
        }
        // Ghosts drawn for another version of the document, or another view
        // of it, are no use.
        let ghost_key = GhostKey { timeline, to_stage, size, images: key.images, fonts: key.fonts };
        if !same_doc || self.ghosts.key.as_ref() != Some(&ghost_key) {
            self.ghosts = GhostCache { key: Some(ghost_key), frames: HashMap::new() };
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
            draw_onion_skins(&mut pixmap, editor, onion, scale, &mut self.ghosts);
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

        // tiny-skia's premultiplied RGBA bytes are egui's Color32s already,
        // so they're copied across in one go rather than pixel by pixel.
        let image = egui::ColorImage::new(
            [size.0 as usize, size.1 as usize],
            bytemuck::cast_slice::<u8, egui::Color32>(pixmap.data()).to_vec(),
        );
        match &mut self.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("stage", image, egui::TextureOptions::LINEAR)),
        }
        self.rendered = Some(key);
        if !same_doc {
            self.rendered_doc = Some(doc.clone());
        }
    }
}

/// Paints `tint` over `pixmap` wherever `coverage` (one alpha a pixel) has
/// something, `opacity` as strong.
fn paint_coverage(pixmap: &mut tiny_skia::Pixmap, coverage: &[u8], tint: inkframe_core::Color, opacity: f32) {
    if coverage.len() != pixmap.pixels().len() {
        return;
    }
    let strength = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    for (pixel, &cover) in pixmap.pixels_mut().iter_mut().zip(coverage) {
        if cover == 0 {
            continue;
        }
        // Premultiplied: the tint's share, and what's left of what was there.
        let a = (cover as u32 * strength + 127) / 255;
        let keep = 255 - a;
        let mix = |src: u8, dst: u8| ((src as u32 * a + dst as u32 * keep + 127) / 255).min(255) as u8;
        let (r, g, b) = (mix(tint.r, pixel.red()), mix(tint.g, pixel.green()), mix(tint.b, pixel.blue()));
        let alpha = (a + pixel.alpha() as u32 * keep / 255).min(255) as u8;
        if let Some(blended) =
            tiny_skia::PremultipliedColorU8::from_rgba(r.min(alpha), g.min(alpha), b.min(alpha), alpha)
        {
            *pixel = blended;
        }
    }
}

/// What onion skin ghosts were drawn for: they hold while these and the
/// document stay the same.
#[derive(Debug, Clone, PartialEq)]
struct GhostKey {
    timeline: TimelineId,
    to_stage: Affine,
    size: (u32, u32),
    images: u64,
    fonts: u64,
}

/// Onion skin ghosts drawn so far: for each frame, how much it covers each
/// pixel of the stage texture.
#[derive(Default)]
struct GhostCache {
    key: Option<GhostKey>,
    frames: HashMap<u32, Vec<u8>>,
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
                Tool::Ellipse => (
                    Shape::Ellipse { radius_x: rect.width() / 2.0, radius_y: rect.height() / 2.0 },
                    tr("Oval"),
                ),
                _ => (
                    Shape::Rect { width: rect.width(), height: rect.height(), corner_radius: 0.0 },
                    tr("Rectangle"),
                ),
            };
            let element = Element { shape, style: editor.draw_style.clone() };
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
                let style = Style { fill: None, stroke: Some(editor.pencil_stroke), gradient: None };
                add_path_layer(editor, tr("Stroke"), path, style);
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
    let style = Style { fill: Some(editor.pencil_stroke.color), stroke: None, gradient: None };
    editor.add_drawn_shape(tr("Line"), Element { shape: Shape::Line { path, widths }, style }, center);
}

fn add_path_layer(editor: &mut Editor, prefix: &str, mut path: Path, style: Style) {
    let Some(bounds) = StageRect::bounding(path.flatten()) else {
        return;
    };
    let center = bounds.center();
    path.translate(-center);
    editor.add_drawn_shape(prefix, Element { shape: Shape::Path(path), style }, center);
}

/// Moves, scales, turns and slants several layers as one, by however the
/// imaginary layer around them changed from `before` to `after`.
///
/// Each layer's position, relative to the group's middle, is scaled and
/// turned the same way the group was, and its own scale and rotation change
/// by the same amounts. That's exact for even scaling, any rotation, and
/// layers that aren't turned.
///
/// Slanting the group, or stretching a turned (or skewed) layer unevenly
/// along the group's axes, slants the layer, which its own scale can't
/// express. For those, the layer's new matrix is worked out directly and
/// turned back into a pose, skew included (`Transform::from_affine`).
///
/// A layer with a parent has its pose in the parent's space, so it always
/// takes the matrix route: the change is carried into that space first.
fn transform_group(editor: &mut Editor, group: &[GroupMember], before: &Transform, after: &Transform) {
    let factor = Point::new(after.scale.x / before.scale.x, after.scale.y / before.scale.y);
    let turn = (after.rotation - before.rotation).to_radians();
    let (sin, cos) = turn.sin_cos();
    let uneven = (factor.x - factor.y).abs() > 1e-4;
    let slanting = after.skew != before.skew;
    let group_change = before.to_affine().inverse().map(|undo| after.to_affine().pre_concat(undo));
    for &GroupMember { id, pose: start, space } in group {
        let slanted = start.rotation % 90.0 != 0.0 || start.skew != Point::ZERO;
        let parented = space != Affine::IDENTITY;
        let in_space = |change: Affine| Some(space.inverse()?.pre_concat(change).pre_concat(space));
        let pose = match group_change.and_then(in_space) {
            Some(change) if parented || slanting || (uneven && slanted) => {
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

/// A card dragged from the Library over the stage: marks where it would
/// land, and places it there when let go.
fn asset_drop(
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &egui::Response,
    editor: &mut Editor,
    view: View,
) {
    if library::dragged_asset(ui.ctx()).is_none() || !response.contains_pointer() {
        return;
    }
    let Some(pointer) = response.hover_pos() else { return };
    let accent = ui.visuals().selection.stroke.color;
    painter.add(egui::Shape::closed_line(view.stage_corners().to_vec(), (2.0, accent)));
    painter.circle_stroke(pointer, 6.0, (1.5, accent));
    painter.line_segment([pointer - vec2(10.0, 0.0), pointer + vec2(10.0, 0.0)], (1.0, accent));
    painter.line_segment([pointer - vec2(0.0, 10.0), pointer + vec2(0.0, 10.0)], (1.0, accent));
    if let Some(asset) = response.dnd_release_payload::<inkframe_core::Asset>() {
        editor.place_asset_at(*asset, view.to_local(pointer));
    }
}

/// How near a guide something must come to snap to it, in screen points.
const SNAP_REACH: f32 = 6.0;

/// The guides to snap to, if snapping's on and they're showing. Only on the
/// main timeline: guides are in stage pixels, and a symbol's coordinates
/// are its own.
fn snapping_guides(editor: &Editor) -> Option<&[inkframe_core::RulerGuide]> {
    (editor.snap_to_guides && editor.show_guides && editor.timeline() == TimelineId::Main)
        .then_some(editor.doc.ruler_guides.as_slice())
        .filter(|g| !g.is_empty())
}

/// A point moved onto any guide it's near (see [`snapping_guides`]).
fn snap_to_guides(editor: &Editor, view: View, p: Point) -> Point {
    match snapping_guides(editor) {
        Some(guides) => inkframe_core::snap_point(guides, p, SNAP_REACH / view.zoom),
        None => p,
    }
}

#[cfg(test)]
mod tests;
