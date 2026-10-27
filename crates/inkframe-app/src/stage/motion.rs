//! A layer's motion path on the stage: finding the selected layer's path,
//! hitting its points and handles, and drawing it.

use egui::{Color32, Pos2, Rect, StrokeKind, Vec2};
use inkframe_core::{MotionCurve, Point};

use super::{ANCHOR_HIT, HIT_TOLERANCE, SELECTION, StageView, View, follows_guide, into_parent_space};
use crate::editor::{Editor, Tool};
use crate::motion_path;

impl StageView {
    /// The layer whose motion path is shown: the selected layer, if it
    /// moves (has two or more position keys), while the Selection tool is
    /// in use and nothing's playing or being typed.
    pub(super) fn motion_path_layer<'a>(&self, editor: &'a Editor) -> Option<&'a inkframe_core::Layer> {
        if editor.tool != Tool::Select || editor.playback.playing || self.text_editing.is_some() {
            return None;
        }
        if editor.selected_ids().len() > 1 {
            return None;
        }
        editor.selected_layer().filter(|l| l.track.position.keys().len() >= 2 && !l.locked)
    }

    /// The motion path handle or tween under `pointer`, if any.
    pub(super) fn motion_path_hit(&self, editor: &Editor, view: View, pointer: Pos2) -> Option<PathHit> {
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
    pub(super) fn draw_motion_path(&self, painter: &egui::Painter, editor: &Editor, view: View) {
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
}

/// Something on a motion path under the pointer.
pub(super) enum PathHit {
    Handle { frame: u32, out: bool },
    Segment { segment: motion_path::Segment, s: f32 },
}
