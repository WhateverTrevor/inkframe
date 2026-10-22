//! Skeletons on the stage (see `crate::skeleton`): drawing bones with the
//! Add Bone tool and posing them with the Bone tool.
//!
//! With Add Bone, dragging on the stage draws a bone, hanging from the
//! picked one; starting the drag on the picked bone's end joins it there.
//! Dragging a bone moves it at rest (rigging, not animating), and dragging
//! its end moves just that end. Clicking empty stage lets go of the picked
//! bone, so the next one starts a new root.
//!
//! With the Bone tool, dragging a bone turns it and the bones above it so
//! its end follows the pointer, keying them; Alt turns just that bone, and
//! Shift stretches it instead. Dragging the start of a root bone moves the
//! whole character.

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Stroke, vec2};
use inkframe_core::{Affine, LayerId, Point, RestPart};

use super::{SELECTION, StageView, View};
use crate::editor::{Editor, Tool};

/// How near the pointer must be to grab a bone or its end, on screen.
const GRAB: f32 = 7.0;
/// The bones' colour; the picked one is drawn in the selection colour.
const BONE: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x3c);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum BoneDrag {
    /// Bone tool: turning a bone (and those above, unless `alone`).
    Pose { skeleton: LayerId, bone: usize, alone: bool },
    /// Bone tool: stretching a bone toward the pointer (Shift).
    Stretch { skeleton: LayerId, bone: usize },
    /// Bone tool: moving a root, `offset` from the pointer to its start.
    Move { skeleton: LayerId, bone: usize, offset: Point },
    /// Add Bone: a new bone being drawn, in the bones' coordinates.
    New { skeleton: LayerId, start: Point, end: Point },
    /// Add Bone: moving a bone's rest pose, from where the pointer `last` was.
    Rest { skeleton: LayerId, bone: usize, part: RestPart, last: Point },
}

/// One bone as drawn: its skeleton, its place, and its ends in the bones'
/// coordinates and on screen.
struct Drawn {
    skeleton: LayerId,
    bone: usize,
    ends: (Point, Point),
    screen: (Pos2, Pos2),
    root: bool,
}

/// The bones' coordinates to the timeline's, for skeleton `id`.
fn bone_space(editor: &Editor, id: LayerId) -> Option<Affine> {
    Some(editor.doc.bone_space(editor.doc.layer(id)?, editor.frame))
}

/// Every bone of every skeleton showing in the timeline being edited:
/// posed as on this frame for the Bone tool, at rest for Add Bone.
fn drawn_bones(editor: &Editor, view: View) -> Vec<Drawn> {
    let at_rest = editor.doc.rest_pose.0;
    let mut drawn = Vec::new();
    for layer in editor.layers().iter().filter(|l| l.visible && l.skeleton.is_some()) {
        let (Some(skeleton), Some(space)) = (&layer.skeleton, bone_space(editor, layer.id)) else { continue };
        let pose = skeleton.pose(editor.frame);
        for (bone, b) in skeleton.bones.iter().enumerate() {
            let ends = if at_rest {
                (b.start, b.end)
            } else {
                let m = pose.moves[bone];
                (m.apply(b.start), m.apply(b.end))
            };
            let screen = (view.to_screen(space.apply(ends.0)), view.to_screen(space.apply(ends.1)));
            drawn.push(Drawn { skeleton: layer.id, bone, ends, screen, root: b.parent.is_none() });
        }
    }
    drawn
}

/// What's under the pointer: a bone, and whether the pointer is on its
/// start or its end rather than along it.
fn bone_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(Drawn, Option<bool>)> {
    let to = |p: Pos2| Point::new(p.x, p.y);
    drawn_bones(editor, view)
        .into_iter()
        .rev()
        .map(|d| {
            let distance = to(pointer).distance_to_segment(to(d.screen.0), to(d.screen.1));
            (d, distance)
        })
        .filter(|(_, distance)| *distance <= GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(d, _)| {
            let end = if d.screen.1.distance(pointer) <= GRAB {
                Some(true)
            } else if d.screen.0.distance(pointer) <= GRAB {
                Some(false)
            } else {
                None
            };
            (d, end)
        })
}

impl StageView {
    /// Handles the pointer for the Add Bone tool, and for the Bone tool
    /// when it's on a skeleton's bone. Returns whether it did, in which
    /// case the other tools leave the pointer alone.
    pub(super) fn handle_bones(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) -> bool {
        editor.check_bone();
        if !matches!(editor.tool, Tool::Bone | Tool::AddBone) {
            self.bone_drag = None;
            return false;
        }
        let (alt, shift) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift));
        let adding = editor.tool == Tool::AddBone;
        // In rig setup the Bone tool moves bones at rest too, as Add Bone
        // does, rather than posing them.
        let rigging = adding || editor.doc.rest_pose.0;
        let to_bones = |editor: &Editor, skeleton: LayerId, p: Point| {
            bone_space(editor, skeleton).and_then(|m| m.inverse()).map_or(p, |m| m.apply(p))
        };

        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(press) = ui.input(|i| i.pointer.press_origin())
        {
            let p = view.to_local(press);
            let hit = bone_at(editor, view, press)
                .filter(|(d, _)| !adding || Some(d.skeleton) == editor.active_skeleton());
            // With Add Bone, a drag from the picked bone's end draws the next
            // bone from there, rather than moving that end.
            let joined = editor
                .active_skeleton()
                .filter(|_| adding)
                .and_then(|skeleton| picked_end(editor, view, skeleton).map(|end| (skeleton, end)))
                .filter(|(_, end)| end.1.distance(press) <= GRAB * 1.5);
            self.bone_drag = match (rigging, hit) {
                _ if joined.is_some() => {
                    joined.map(|(skeleton, (start, _))| BoneDrag::New { skeleton, start, end: start })
                }
                (false, Some((d, end))) => {
                    pick(editor, d.skeleton, d.bone);
                    if shift {
                        Some(BoneDrag::Stretch { skeleton: d.skeleton, bone: d.bone })
                    } else if d.root && end == Some(false) {
                        let offset = d.ends.0 - to_bones(editor, d.skeleton, p);
                        Some(BoneDrag::Move { skeleton: d.skeleton, bone: d.bone, offset })
                    } else {
                        Some(BoneDrag::Pose { skeleton: d.skeleton, bone: d.bone, alone: alt })
                    }
                }
                (false, None) => None,
                (true, Some((d, end))) => {
                    editor.selected_bone = Some(d.bone);
                    let part = if end == Some(true) { RestPart::End } else { RestPart::Whole };
                    let last = to_bones(editor, d.skeleton, p);
                    Some(BoneDrag::Rest { skeleton: d.skeleton, bone: d.bone, part, last })
                }
                (true, None) if !adding => None,
                (true, None) => editor.active_skeleton().map(|skeleton| {
                    let start = to_bones(editor, skeleton, p);
                    BoneDrag::New { skeleton, start, end: start }
                }),
            };
        }

        if let Some(pointer) = response.interact_pointer_pos() {
            let p = view.to_local(pointer);
            match &mut self.bone_drag {
                Some(BoneDrag::Pose { skeleton, bone, alone }) => {
                    let target = to_bones(editor, *skeleton, p);
                    editor.pose_bone(*skeleton, *bone, target, *alone);
                }
                Some(BoneDrag::Stretch { skeleton, bone }) => {
                    // As long as the pointer is far along the bone from its
                    // start, against how long it is at rest.
                    let (skeleton, bone) = (*skeleton, *bone);
                    let to = to_bones(editor, skeleton, p);
                    let frame = editor.frame;
                    let factor = editor.skeleton(skeleton).and_then(|s| {
                        let rest = s.bones.get(bone)?.length();
                        let (start, end) = s.posed_ends(bone, frame)?;
                        // Measured along the posed bone's own direction.
                        let unit = (end - start) * (1.0 / (end - start).length().max(1e-4));
                        Some((to - start).dot(unit) / rest.max(1e-3))
                    });
                    if let Some(factor) = factor {
                        editor.stretch_bone(skeleton, bone, factor);
                    }
                }
                Some(BoneDrag::Move { skeleton, bone, offset }) => {
                    let to = to_bones(editor, *skeleton, p) + *offset;
                    editor.move_bone(*skeleton, *bone, to);
                }
                Some(BoneDrag::New { skeleton, end, .. }) => *end = to_bones(editor, *skeleton, p),
                Some(BoneDrag::Rest { skeleton, bone, part, last }) => {
                    let now = to_bones(editor, *skeleton, p);
                    let (skeleton, bone, part, delta) = (*skeleton, *bone, *part, now - *last);
                    *last = now;
                    editor.shift_bone_rest(skeleton, bone, delta, part);
                }
                None => {}
            }
        }

        if response.drag_stopped() {
            if let Some(BoneDrag::New { skeleton, start, end }) = self.bone_drag {
                // A drag too short to see is a slip, not a bone.
                let space = bone_space(editor, skeleton).unwrap_or(Affine::IDENTITY);
                let length = view.to_screen(space.apply(start)).distance(view.to_screen(space.apply(end)));
                if length > 4.0 {
                    editor.add_bone(skeleton, start, end);
                }
            }
            self.bone_drag = None;
        }

        // A click picks the bone under it; with Add Bone, a click on empty
        // stage lets go, so the next bone starts a new root.
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            match bone_at(editor, view, pointer) {
                Some((d, _)) => {
                    pick(editor, d.skeleton, d.bone);
                    return true;
                }
                None if adding => editor.selected_bone = None,
                None => {}
            }
        }
        adding || self.bone_drag.is_some()
    }

    /// The skeletons' bones over the drawing, the picked one's reach while
    /// adding bones, the bone being drawn, and a note saying what to do.
    pub(super) fn draw_bones(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        response: &egui::Response,
        editor: &Editor,
        view: View,
    ) {
        if !matches!(editor.tool, Tool::Bone | Tool::AddBone) {
            return;
        }
        let adding = editor.tool == Tool::AddBone;
        let active = editor.active_skeleton();
        let drawn = drawn_bones(editor, view);

        // How far the picked bone reaches into a drawing bound by region.
        if adding
            && let (Some(id), Some(bone)) = (active, editor.selected_bone)
            && let Some(d) = drawn.iter().find(|d| d.skeleton == id && d.bone == bone)
            && let (Some(skeleton), Some(space)) = (editor.skeleton(id), bone_space(editor, id))
        {
            let reach = skeleton.bones[bone].strength * space.average_scale() * view.zoom;
            let (a, b) = d.screen;
            let along = (b - a).normalized();
            let across = vec2(-along.y, along.x);
            let fill = BONE.gamma_multiply(0.12);
            if (b - a).length() > 1e-3 {
                let quad =
                    vec![a + across * reach, b + across * reach, b - across * reach, a - across * reach];
                painter.add(egui::Shape::convex_polygon(quad, fill, Stroke::NONE));
            }
            for end in [a, b] {
                painter.circle_filled(end, reach, fill);
            }
        }

        if adding && let (Some(id), Some(bone)) = (active, editor.selected_bone) {
            draw_pins(painter, editor, view, id, bone);
        }
        if !adding && let (Some(id), Some(bone)) = (active, editor.selected_bone) {
            draw_limits(painter, editor, view, id, bone);
        }
        // A bone reaching for a target: a dotted line from its end to the
        // target's start.
        for d in &drawn {
            let target = editor.skeleton(d.skeleton).and_then(|s| s.bones.get(d.bone)).and_then(|b| b.target);
            if let Some(t) = drawn.iter().find(|t| t.skeleton == d.skeleton && Some(t.bone) == target) {
                let dots =
                    egui::Shape::dashed_line(&[d.screen.1, t.screen.0], Stroke::new(1.5, BONE), 4.0, 4.0);
                painter.extend(dots);
            }
        }
        for d in &drawn {
            let picked = Some(d.skeleton) == active && editor.selected_bone == Some(d.bone);
            let colour = if picked { SELECTION } else { BONE };
            draw_bone(painter, d.screen.0, d.screen.1, colour, d.root);
        }
        if let Some(BoneDrag::New { skeleton, start, end }) = self.bone_drag
            && let Some(space) = bone_space(editor, skeleton)
        {
            let (a, b) = (view.to_screen(space.apply(start)), view.to_screen(space.apply(end)));
            draw_bone(painter, a, b, SELECTION.gamma_multiply(0.7), false);
        }

        if adding {
            let note = match active {
                None => "Add Bone · select a skeleton first: Modify › New Skeleton".to_owned(),
                Some(_) if editor.selected_bone.is_some() => {
                    "Add Bone · drag to add a bone hanging from the picked one · click away to start a new root"
                        .to_owned()
                }
                Some(_) => "Add Bone · drag to draw a bone · drag a bone or its end to move it".to_owned(),
            };
            let area = painter.clip_rect();
            let galley = painter.layout_no_wrap(note, FontId::proportional(12.0), Color32::WHITE);
            let pill =
                Rect::from_min_size(area.left_top() + vec2(10.0, 10.0), galley.size() + vec2(20.0, 10.0));
            painter.rect_filled(pill, pill.height() / 2.0, BONE.gamma_multiply(0.9));
            painter.galley(pill.min + vec2(10.0, 5.0), galley, Color32::WHITE);
        }

        let cursor = match self.bone_drag {
            Some(BoneDrag::New { .. }) => Some(CursorIcon::Crosshair),
            Some(_) => Some(CursorIcon::Grabbing),
            None => response.hover_pos().map(|p| match bone_at(editor, view, p) {
                Some((d, Some(false))) if d.root && !adding => CursorIcon::Move,
                Some(_) => CursorIcon::Grab,
                None if adding => CursorIcon::Crosshair,
                None => CursorIcon::Default,
            }),
        };
        if let Some(cursor) = cursor
            && (adding || self.bone_drag.is_some() || cursor != CursorIcon::Default)
        {
            ui.ctx().set_cursor_icon(cursor);
        }
    }
}

/// For a picked bone with limits, the wedge it may turn in, about its
/// start: from its turn's lowest to highest, as posed now.
fn draw_limits(painter: &egui::Painter, editor: &Editor, view: View, id: LayerId, bone: usize) {
    let Some(skeleton) = editor.skeleton(id) else { return };
    let Some(b) = skeleton.bones.get(bone) else { return };
    let (Some(limits), Some(space)) = (b.limits, bone_space(editor, id)) else { return };
    let Some((start, end)) = skeleton.posed_ends(bone, editor.frame) else { return };
    // The bone's direction now, less its turn now, is where a turn of 0
    // points; the wedge spans the turns from there.
    let turn = b.turn.sample(editor.frame as f32).map_or(0.0, |t| limits.clamp(t));
    let along = end - start;
    let zero = along.y.atan2(along.x) - turn.to_radians();
    let radius = b.length() * 0.6;
    let (low, high) = (limits.min.min(limits.max), limits.max.max(limits.min));
    let steps = (((high - low).abs() / 5.0).ceil() as usize).clamp(2, 72);
    let mut wedge = vec![view.to_screen(space.apply(start))];
    for k in 0..=steps {
        let angle = zero + (low + (high - low) * k as f32 / steps as f32).to_radians();
        let p = start + Point::new(angle.cos(), angle.sin()) * radius;
        wedge.push(view.to_screen(space.apply(p)));
    }
    // Filled only while it's a wedge, not most of a circle (which a convex
    // fill can't draw).
    if high - low <= 180.0 {
        painter.add(egui::Shape::convex_polygon(wedge.clone(), BONE.gamma_multiply(0.15), Stroke::NONE));
    }
    painter.add(egui::Shape::closed_line(wedge, Stroke::new(1.0, BONE.gamma_multiply(0.6))));
}

/// Marks what's pinned to bone `bone` of skeleton `id`: shapes pinned
/// whole get their outline traced, single points a square.
fn draw_pins(painter: &egui::Painter, editor: &Editor, view: View, id: LayerId, bone: usize) {
    for member in editor.skeleton_members(id) {
        let Some(layer) = editor.doc.layer(member) else { continue };
        let Some(drawing) = layer.drawings.get(layer.drawing_index_at(editor.frame)) else { continue };
        let to_screen = |p: Point| view.to_screen(editor.doc.layer_matrix(layer, editor.frame).apply(p));
        for pin in drawing.bone_points.iter().filter(|p| p.bone == bone) {
            let part = if pin.colour_art { &drawing.colour_art } else { &drawing.elements };
            let Some(element) = part.get(pin.element) else { continue };
            let (anchors, closed) = element.shape.to_anchors();
            match &pin.anchors {
                None => {
                    let points: Vec<Pos2> = anchors.iter().map(|a| to_screen(a.point)).collect();
                    let stroke = Stroke::new(2.0, BONE);
                    if closed {
                        painter.add(egui::Shape::closed_line(points, stroke));
                    } else {
                        painter.add(egui::Shape::line(points, stroke));
                    }
                }
                Some(indices) => {
                    for a in indices.iter().filter_map(|&i| anchors.get(i)) {
                        let square = Rect::from_center_size(to_screen(a.point), egui::Vec2::splat(7.0));
                        painter.rect_filled(square, 1.0, BONE);
                        painter.rect_stroke(
                            square,
                            1.0,
                            Stroke::new(1.0, Color32::WHITE),
                            egui::StrokeKind::Outside,
                        );
                    }
                }
            }
        }
    }
}

/// While in rig setup, a note saying so along the top of the stage (Add
/// Bone has its own), as everything rigged shows at rest.
pub(super) fn draw_setup_note(painter: &egui::Painter, editor: &Editor) {
    // A correction being drawn says so instead.
    if let Some(c) = &editor.correcting {
        let layer = editor.doc.layer(c.layer).map_or("the layer", |l| l.name.as_str());
        let note = format!(
            "Correcting {layer} at {:.0}° · reshape it with any tool · Enter keys it, Esc cancels",
            c.turn
        );
        let area = painter.clip_rect();
        let galley = painter.layout_no_wrap(note, FontId::proportional(12.0), Color32::WHITE);
        let pill = Rect::from_min_size(area.left_top() + vec2(10.0, 10.0), galley.size() + vec2(20.0, 10.0));
        painter.rect_filled(pill, pill.height() / 2.0, BONE.gamma_multiply(0.9));
        painter.galley(pill.min + vec2(10.0, 5.0), galley, Color32::WHITE);
        return;
    }
    let rigged = editor.layers().iter().any(|l| l.skeleton.is_some());
    if !editor.rig_setup || editor.tool == Tool::AddBone || !rigged {
        return;
    }
    let note = "Rig setup · skeletons at rest: draw, paint and rig here · View › Rig Setup to see the poses";
    let area = painter.clip_rect();
    let galley = painter.layout_no_wrap(note.to_owned(), FontId::proportional(12.0), Color32::WHITE);
    let pill = Rect::from_min_size(area.left_top() + vec2(10.0, 10.0), galley.size() + vec2(20.0, 10.0));
    painter.rect_filled(pill, pill.height() / 2.0, BONE.gamma_multiply(0.9));
    painter.galley(pill.min + vec2(10.0, 5.0), galley, Color32::WHITE);
}

/// Picks a bone, selecting its skeleton's folder if another layer was.
fn pick(editor: &mut Editor, skeleton: LayerId, bone: usize) {
    if editor.active_skeleton() != Some(skeleton) {
        editor.select(Some(skeleton));
    }
    editor.selected_bone = Some(bone);
}

/// The picked bone's end at rest, in the bones' coordinates and on screen.
fn picked_end(editor: &Editor, view: View, skeleton: LayerId) -> Option<(Point, Pos2)> {
    let bone = editor.skeleton(skeleton)?.bones.get(editor.selected_bone?)?;
    let space = bone_space(editor, skeleton)?;
    Some((bone.end, view.to_screen(space.apply(bone.end))))
}

/// A bone as Moho draws one: a long diamond, widest a quarter of the way
/// along, from a round joint at its start to a point at its end. A root's
/// start is a square, as it's where the whole character is moved from.
fn draw_bone(painter: &egui::Painter, start: Pos2, end: Pos2, colour: Color32, root: bool) {
    let along = end - start;
    let length = along.length();
    if length < 1e-3 {
        painter.circle_filled(start, 4.0, colour);
        return;
    }
    let side = vec2(-along.y, along.x) / length * (length * 0.1).clamp(3.0, 9.0);
    let widest = start + along * 0.25;
    let outline = vec![start, widest + side, end, widest - side];
    painter.add(egui::Shape::convex_polygon(outline.clone(), colour.gamma_multiply(0.45), Stroke::NONE));
    painter.add(egui::Shape::closed_line(outline, Stroke::new(1.5, colour)));
    if root {
        let square = Rect::from_center_size(start, egui::Vec2::splat(8.0));
        painter.rect_filled(square, 1.0, colour);
    } else {
        painter.circle(start, 3.5, Color32::WHITE, Stroke::new(1.5, colour));
    }
}
