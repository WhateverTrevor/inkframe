//! The Width tool on the stage: the points along a line where its width can
//! be dragged, and drawing them.

use egui::{Color32, Pos2, vec2};
use inkframe_core::{LayerId, Shape};

use super::{SELECTION, View};
use crate::editor::Editor;

/// Where a point of a pencil line is: in a layer's showing drawing, its
/// line art or colour art, which shape, and which of its anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LinePoint {
    colour_art: bool,
    element: usize,
    pub(super) anchor: usize,
}

/// How near a pencil line's point the pointer must be to grab it, in points.
pub(super) const LINE_POINT_GRAB: f32 = 14.0;

/// Every pencil line's points on screen in the timeline being edited, the
/// selected layer's first, with each point's width on screen.
pub(super) fn line_points(editor: &Editor, view: View) -> Vec<(LayerId, LinePoint, Pos2, f32)> {
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
pub(super) fn line_point_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, LinePoint)> {
    line_points(editor, view)
        .into_iter()
        .filter(|(.., at, _)| at.distance(pointer) <= LINE_POINT_GRAB)
        .min_by(|a, b| a.2.distance(pointer).total_cmp(&b.2.distance(pointer)))
        .map(|(layer, place, ..)| (layer, place))
}

/// The pencil line a `LinePoint` is on, to change its widths.
pub(super) fn line_shape_mut(editor: &mut Editor, layer: LayerId, place: LinePoint) -> Option<&mut Shape> {
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
pub(super) fn draw_line_widths(painter: &egui::Painter, editor: &Editor, view: View) {
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
