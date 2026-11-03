//! The point picked on a shape's outline (with the Subselection tool, the
//! Pen with ⌘, or the Pen's slot-mates), in the Properties panel: where it
//! is, what sort of point it is, and its handles, so a curve can be shaped
//! point by point with numbers as well as by dragging.
//!
//! Positions, lengths and angles are measured on the stage, as the point
//! and its handles are seen, whatever the layer's own move, turn or scale.

use egui::DragValue;
use inkframe_core::{Anchor, LayerId};

use super::sections::section;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::icons::Icon;
use crate::subselect::{AnchorPart, PointKind, handle_polar, set_handle, set_point_kind};

pub(super) fn point_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some((_, element, index)) = editor.picked_anchor.filter(|(layer, _, _)| *layer == id) else {
        return;
    };
    let Some(layer) = editor.doc.layer(id) else { return };
    let to_stage = editor.doc.layer_matrix(layer, editor.frame);
    let Some(to_local) = to_stage.inverse() else { return };
    // The point as it shows on this frame, posed if it's animated.
    let Some((anchors, closed)) = editor.posed_element(id, element).map(|e| e.shape.to_anchors()) else {
        return;
    };
    let Some(&anchor) = anchors.get(index) else { return };
    if !section(ui, editor, "point", tr("Point"), Icon::Shapes) {
        return;
    }
    // Points picked with it, which a new Type changes too.
    let others: Vec<(usize, usize)> = editor
        .more_picked_anchors
        .iter()
        .filter(|(layer, ..)| *layer == id)
        .map(|&(_, e, i)| (e, i))
        .collect();
    if !others.is_empty() {
        ui.weak(tr_args(
            "{count} points picked: Type changes them all; the rest is the highlighted point's",
            &[("count", &(others.len() + 1).to_string())],
        ));
    }

    let map = |a: &Anchor, m: &inkframe_core::Affine| Anchor {
        point: m.apply(a.point),
        handle_in: m.apply(a.handle_in),
        handle_out: m.apply(a.handle_out),
    };
    let staged = map(&anchor, &to_stage);
    let kind = PointKind::of(&staged);
    let mut edited = staged;
    let mut new_kind = None;

    egui::Grid::new("point").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Position"));
        ui.horizontal(|ui| {
            let mut point = edited.point;
            let x = ui.add(DragValue::new(&mut point.x).speed(1.0).prefix("x ").max_decimals(1));
            let y = ui.add(DragValue::new(&mut point.y).speed(1.0).prefix("y ").max_decimals(1));
            if x.changed() || y.changed() {
                // The handles come along, as dragging the point brings them.
                let moved = point - edited.point;
                edited = Anchor {
                    point,
                    handle_in: edited.handle_in + moved,
                    handle_out: edited.handle_out + moved,
                };
            }
        });
        ui.end_row();
    });

    // Its own row, the panel's full width, so all three choices show
    // however narrow the panel (or long the words, translated).
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Type"));
        for (option, label, tip) in [
            (PointKind::Corner, tr("Corner"), tr("No handles: the outline turns sharply here")),
            (
                PointKind::Smooth,
                tr("Smooth"),
                tr("Handles in line, so the curve flows through; each its own length"),
            ),
            (PointKind::Symmetric, tr("Symmetric"), tr("Handles in line and the same length")),
        ] {
            if ui.selectable_label(kind == option, label).on_hover_text(tip).clicked() && kind != option {
                new_kind = Some(option);
            }
        }
    });
    if kind == PointKind::Broken {
        ui.weak(tr("Broken: its handles move separately. Alt-drag a handle to break a point."));
    }

    egui::Grid::new("point handles").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for (part, label) in
            [(AnchorPart::HandleIn, tr("Handle in")), (AnchorPart::HandleOut, tr("Handle out"))]
        {
            ui.label(label);
            ui.horizontal(|ui| {
                let (mut length, mut degrees) = handle_polar(&edited, part);
                let l = ui
                    .add(DragValue::new(&mut length).speed(0.5).range(0.0..=100_000.0).max_decimals(1))
                    .on_hover_text(tr("How far the handle reaches: how strongly the curve leans its way"));
                // A handle that isn't out has no direction to change.
                let a = ui.add_enabled(
                    length > 0.0,
                    DragValue::new(&mut degrees).speed(1.0).suffix("°").max_decimals(1),
                );
                if l.changed() || a.changed() {
                    edited = set_handle(&edited, part, length, degrees);
                }
            });
            ui.end_row();
        }
    });

    // A new Type for the other points picked, shape by shape (the key
    // point's own shape is done below).
    if let Some(kind) = new_kind {
        for &(other, other_index) in others.iter().filter(|(e, _)| *e != element) {
            if let Some(shape) = editor.posed_element(id, other) {
                let (anchors, closed) = shape.shape.to_anchors();
                let changed = set_point_kind(&anchors, other_index, closed, kind);
                editor.reshape(id, other, &changed, closed);
            }
        }
    }

    // Back into the layer's coordinates, where its shapes are kept.
    let local = if let Some(kind) = new_kind {
        // The new kind is worked out among each point's neighbours, where a
        // corner's new handles take their direction from.
        let same_shape = others.iter().filter(|(e, _)| *e == element).map(|&(_, i)| i);
        Some(
            std::iter::once(index)
                .chain(same_shape)
                .fold(anchors.clone(), |anchors, i| set_point_kind(&anchors, i, closed, kind)),
        )
    } else if edited != staged {
        let mut anchors = anchors.clone();
        anchors[index] = map(&edited, &to_local);
        Some(anchors)
    } else {
        None
    };
    if let Some(anchors) = local {
        // Rectangles and ovals become paths, as on their first point dragged.
        editor.reshape(id, element, &anchors, closed);
    }

    // Its animation, if it's keyed (see Animate Points in the tool's bar).
    let frame = editor.frame;
    let drawing = editor.doc.layer_mut(id).and_then(|layer| {
        let index = layer.drawing_index_at(frame);
        layer.drawings.get_mut(index)
    });
    if let Some(drawing) = drawing {
        let keys = drawing.point_key_frames(element, index);
        if !keys.is_empty() {
            ui.horizontal_wrapped(|ui| {
                let frames = keys.iter().map(|f| (f + 1).to_string()).collect::<Vec<_>>().join(", ");
                ui.weak(tr_args("Animated, keyed on frames {frames}", &[("frames", &frames)]));
                if ui
                    .button(tr("Remove Animation"))
                    .on_hover_text(tr("The point stays where it's drawn on every frame"))
                    .clicked()
                {
                    drawing.clear_point_keys(element, index);
                }
            });
        }
    }
    ui.add_space(8.0);
}
