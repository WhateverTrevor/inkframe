//! Rigs drawn over the stage for the Bone tool: layer skeletons, deformer
//! bones and warp grids, and finding the joint or grid point under the pointer.

use egui::{Color32, Pos2, Rect, StrokeKind, Vec2};
use inkframe_core::LayerId;

use super::{ANCHOR_HIT, View};
use crate::editor::Editor;

/// The rigs in the timeline being edited, as the Bone tool shows them: a
/// bone from each parent's joint (its pivot) to each child's, so the
/// skeleton the layers hang from can be seen and grabbed. Roots are
/// squares, since dragging one moves its whole rig.
pub(super) fn draw_skeleton(painter: &egui::Painter, editor: &Editor, view: View) {
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

/// Every deformer's bones in the timeline being edited, as they're bent on
/// this frame, in screen coordinates: `(layer, joints)`.
pub(super) fn deformer_chains(editor: &Editor, view: View) -> Vec<(LayerId, Vec<Pos2>)> {
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
pub(super) fn deformer_joint_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, usize)> {
    deformer_chains(editor, view).into_iter().rev().find_map(|(layer, joints)| {
        let bone = joints.iter().skip(1).position(|j| j.distance(pointer) <= ANCHOR_HIT)?;
        Some((layer, bone))
    })
}

/// Every warp grid in the timeline being edited, its points on screen,
/// row by row, with its size.
pub(super) fn warp_grids(editor: &Editor, view: View) -> Vec<(LayerId, usize, Vec<Pos2>)> {
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
pub(super) fn warp_point_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, usize)> {
    warp_grids(editor, view).into_iter().rev().find_map(|(layer, _, points)| {
        let point = points.iter().position(|p| p.distance(pointer) <= ANCHOR_HIT)?;
        Some((layer, point))
    })
}

/// Warp grids: the lines between their points, and the points as small
/// squares to drag, in the deformers' green.
pub(super) fn draw_warps(painter: &egui::Painter, editor: &Editor, view: View) {
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
pub(super) fn draw_deformers(painter: &egui::Painter, editor: &Editor, view: View) {
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

pub(super) const DEFORMER: Color32 = Color32::from_rgb(0x4c, 0xc2, 0x8a);

/// The skeleton's color: warm, so it stands apart from the blue selection.
pub(super) const BONE: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x3c);
