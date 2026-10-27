//! The box of handles around the selection: scale, rotate, skew and the
//! pivot, what each one under the pointer means, and drawing them.

use egui::{Color32, CursorIcon, Pos2, Rect, StrokeKind, Vec2, vec2};
use inkframe_core::{Affine, LayerId, Point, Rect as StageRect, Transform};

use super::{
    GroupMember, HANDLE_SIZE, PIVOT_GRAB, ROTATE_KNOB_DISTANCE, ROTATE_ZONE, SELECTION, SKEW_REACH, View,
    distance_to_segment,
};
use crate::editor::{Editor, Tool};
use crate::handles::ScaleHandle;

/// Which transform handle the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum HandleHit {
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
pub(super) struct Handles {
    pub(super) layer: LayerId,
    /// With several layers selected: each one, its pose on this frame, and
    /// its parents' placement. The handles then go around them all, as an
    /// imaginary layer (`pose`) that the real ones follow; see
    /// `transform_group`.
    pub(super) group: Vec<GroupMember>,
    pub(super) pose: Transform,
    /// Maps the space `pose` is in (the layer's parent's) to the timeline's.
    pub(super) space: Affine,
    /// The layer's bounds, measured from its pivot (the point `pose`
    /// places), which is what the scale and rotate math works in.
    pub(super) bounds: StageRect,
    pub(super) pivot: Pos2,
    pub(super) scale: [(ScaleHandle, Pos2); 8],
    pub(super) center: Pos2,
    pub(super) top: Pos2,
    pub(super) knob: Pos2,
}

impl Handles {
    /// Handles for the current selection, if it can be transformed.
    pub(super) fn for_selection(editor: &Editor, view: View) -> Option<Self> {
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
    pub(super) fn hit(&self, p: Pos2, view: View, alt: bool) -> Option<HandleHit> {
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
        // Transform (several layers slant together, as one).
        {
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

    pub(super) fn cursor(&self, hit: HandleHit) -> CursorIcon {
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

    pub(super) fn draw(&self, painter: &egui::Painter) {
        painter.line_segment([self.top, self.knob], (1.0, SELECTION));
        painter.circle(self.knob, 4.5, Color32::WHITE, (1.5, SELECTION));
        for (_, pos) in &self.scale {
            let square = Rect::from_center_size(*pos, Vec2::splat(HANDLE_SIZE));
            painter.rect_filled(square, 0.0, Color32::WHITE);
            painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
        }
    }
}
