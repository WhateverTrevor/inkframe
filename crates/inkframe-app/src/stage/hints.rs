//! Shape hints on the stage: the lettered markers that steer a shape tween,
//! finding the one under the pointer and drawing them.

use egui::{Color32, Pos2};
use inkframe_core::LayerId;

use super::View;
use crate::editor::Editor;

/// How near a shape hint the pointer must be to grab it, in points.
pub(super) const HINT_RADIUS: f32 = 8.0;

/// The shape hints to show and edit on this frame: the selected layer's,
/// when it's on the first frame of a shape tween (the drawing it morphs
/// from) or on the swap it ends at (the one it morphs into). Returns the
/// layer, the drawing, and whether that's the end.
pub(super) fn shown_hints(editor: &Editor) -> Option<(LayerId, usize, bool)> {
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
pub(super) fn hint_at(editor: &Editor, view: View, pointer: Pos2) -> Option<(LayerId, usize, usize)> {
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
pub(super) fn draw_hints(painter: &egui::Painter, editor: &Editor, view: View) {
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
