//! Modify › Transform › Distort and Envelope: a lattice of handles over the
//! picked shapes that bends them as it's dragged (the geometry is in the
//! core's `envelope.rs`; the handles in `stage/lattice.rs`).
//!
//! It works on shapes picked inside a layer, or with none picked, on
//! everything drawn in the layer (without stepping inside it, so the other
//! layers stay as they are to bend it against). Every drag
//! bends the shapes as they were when the edit began, so going back and
//! forth doesn't wear them down; Enter, Escape, or clicking away from the
//! handles leaves them as they are.

use inkframe_core::{Element, Lattice, LayerId, Point, Rect};

use crate::editor::{Editor, Tool};
use crate::inside::ShapeRef;

/// Which lattice: four corners, or sixteen handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatticeKind {
    Distort,
    Envelope,
}

impl LatticeKind {
    pub fn name(self) -> &'static str {
        match self {
            LatticeKind::Distort => crate::i18n::tr_later("Distort"),
            LatticeKind::Envelope => crate::i18n::tr_later("Envelope"),
        }
    }
}

/// A Distort or Envelope under way.
#[derive(Debug, Clone)]
pub struct LatticeEdit {
    pub kind: LatticeKind,
    /// The handles, in the layer's own coordinates.
    pub lattice: Lattice,
    layer: LayerId,
    frame: u32,
    /// Bending shapes picked inside the layer, rather than its whole
    /// drawing.
    picked: bool,
    /// How many shapes the drawing had, so it ends if one's added or taken
    /// away and the places below no longer mean the same shapes.
    count: usize,
    /// The shapes as they were when the edit began.
    originals: Vec<(ShapeRef, Element)>,
}

impl LatticeEdit {
    pub fn layer(&self) -> LayerId {
        self.layer
    }
}

/// The box the lattice goes over. A flat one (a straight line across)
/// gets some depth, or its handles would sit on top of each other and
/// there'd be nothing to pull.
fn lattice_bounds(shapes: &[(ShapeRef, Element)]) -> Option<Rect> {
    let bounds = shapes.iter().map(|(_, e)| e.shape.bounds()).reduce(Rect::union)?;
    let least = (bounds.width().max(bounds.height()) * 0.2).max(10.0);
    let grow = |span: f32| ((least - span) / 2.0).max(0.0);
    let (dx, dy) = (grow(bounds.width()), grow(bounds.height()));
    Some(Rect::from_points(
        Point::new(bounds.min.x - dx, bounds.min.y - dy),
        Point::new(bounds.max.x + dx, bounds.max.y + dy),
    ))
}

impl Editor {
    /// Puts a Distort or Envelope over the picked shapes, or with none
    /// picked, over everything drawn in the layer worked inside or the
    /// selected layer.
    pub fn start_lattice(&mut self, kind: LatticeKind) -> Result<(), String> {
        let picked = !self.picked_shapes().is_empty();
        let layer =
            self.inside_layer().or(self.selection).ok_or("Select a layer with shapes in it first.")?;
        let bendable = self.doc.layer(layer).is_some_and(|l| {
            l.draws_vectors() && !l.locked && l.instance.is_none() && !l.drawings.is_empty()
        });
        if !bendable {
            return Err("Only shapes can be bent: pick a vector layer that isn't locked.".into());
        }
        let (all, count) = self.layer_shapes(layer);
        let originals: Vec<(ShapeRef, Element)> = if picked {
            all.into_iter().filter(|(r, _)| self.picked_shapes().contains(r)).collect()
        } else {
            all
        };
        let bounds = lattice_bounds(&originals).ok_or("There are no shapes here to bend.")?;
        let lattice = match kind {
            LatticeKind::Distort => Lattice::distort(bounds),
            LatticeKind::Envelope => Lattice::envelope(bounds),
        };
        self.tool = Tool::Select;
        self.lattice =
            Some(LatticeEdit { kind, lattice, layer, frame: self.frame, picked, count, originals });
        Ok(())
    }

    /// Moves handle `index` to `to` (in the layer's coordinates) and bends
    /// the shapes to match. `smooth` keeps the handles of a point in the
    /// middle of a side in line (see [`Lattice::move_point`]).
    pub fn move_lattice_point(&mut self, index: usize, to: Point, smooth: bool) {
        let Some(edit) = &mut self.lattice else { return };
        edit.lattice.move_point(index, to, smooth);
        let (layer, lattice, originals) = (edit.layer, edit.lattice.clone(), edit.originals.clone());
        self.reshape_layer_shapes(layer, &originals, |shape| lattice.bend(shape));
    }

    /// Leaves the shapes as the lattice has bent them.
    pub fn finish_lattice(&mut self) {
        self.lattice = None;
    }

    /// Ends the edit if what it was bending has gone from under it: the
    /// layer stepped into or out of, other shapes picked, another layer
    /// selected, a shape added or deleted, another frame, another tool.
    /// Called once a frame.
    pub fn check_lattice(&mut self) {
        let Some(edit) = &self.lattice else { return };
        let same_target = if edit.picked {
            let bent: Vec<ShapeRef> = edit.originals.iter().map(|(r, _)| *r).collect();
            self.inside_layer() == Some(edit.layer)
                && self.picked_shapes().iter().filter(|r| bent.contains(r)).count() == bent.len()
        } else {
            // The whole drawing: from inside the layer, or from outside it
            // with it selected, but not stepping in or out half way.
            let inside = self.inside_layer();
            (inside == Some(edit.layer) || (inside.is_none() && self.selection == Some(edit.layer)))
                && self.picked_shapes().is_empty()
        };
        let still = same_target
            && self.frame == edit.frame
            && self.tool == Tool::Select
            && self.layer_shapes(edit.layer).1 == edit.count;
        if !still {
            self.lattice = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Color, Document, Shape, Style, Transform};

    fn editor_with_square() -> (Editor, LayerId) {
        let mut doc = Document::default();
        let square = Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let id = doc.add_layer("Card", vec![square], 0, Transform::at(Point::new(200.0, 150.0)));
        let mut editor = Editor::new(doc);
        editor.select(Some(id));
        (editor, id)
    }

    fn shape_bounds(editor: &Editor, id: LayerId) -> Rect {
        editor.doc.layer(id).unwrap().elements_at(0)[0].shape.bounds()
    }

    #[test]
    fn distort_bends_a_whole_layer_without_stepping_inside() {
        let (mut editor, id) = editor_with_square();
        editor.start_lattice(LatticeKind::Distort).unwrap();
        assert_eq!(editor.inside_layer(), None);
        // Pull the bottom right corner out and down.
        editor.move_lattice_point(2, Point::new(90.0, 80.0), true);
        let bounds = shape_bounds(&editor, id);
        assert!((bounds.max.x - 90.0).abs() < 1e-2 && (bounds.max.y - 80.0).abs() < 1e-2, "{bounds:?}");
        assert!((bounds.min.x + 50.0).abs() < 1e-2, "the left side stays");
    }

    #[test]
    fn every_drag_starts_from_the_shapes_as_they_were() {
        let (mut editor, id) = editor_with_square();
        editor.start_lattice(LatticeKind::Envelope).unwrap();
        let before = editor.doc.layer(id).unwrap().elements_at(0)[0].clone();
        editor.move_lattice_point(3, Point::new(0.0, -90.0), true);
        // And back where it began: the square again, not a square bent twice.
        editor.move_lattice_point(3, Point::new(0.0, -50.0), true);
        let after = shape_bounds(&editor, id);
        let original = before.shape.bounds();
        assert!(after.min.distance(original.min) < 1e-2 && after.max.distance(original.max) < 1e-2);
    }

    #[test]
    fn inside_a_layer_only_the_picked_shapes_bend() {
        let (mut editor, id) = editor_with_square();
        let copy = editor.doc.layer(id).unwrap().elements_at(0)[0].clone();
        editor.doc.layer_mut(id).unwrap().elements_at_mut(0).push(copy.clone());
        editor.enter_layer(id);
        editor.pick_shapes(vec![ShapeRef::line(1)]);
        editor.start_lattice(LatticeKind::Distort).unwrap();
        editor.move_lattice_point(0, Point::new(-80.0, -80.0), true);
        let shapes = editor.doc.layer(id).unwrap().elements_at(0);
        assert_eq!(shapes[0], copy, "not picked, so not bent");
        assert!(shapes[1].shape.bounds().min.x < -79.0);
    }

    #[test]
    fn stepping_inside_or_adding_a_shape_ends_the_edit() {
        let (mut editor, id) = editor_with_square();
        editor.start_lattice(LatticeKind::Envelope).unwrap();
        editor.enter_layer(id);
        editor.pick_shapes(vec![ShapeRef::line(0)]);
        editor.check_lattice();
        assert!(editor.lattice.is_none(), "picking shapes changes what's bent");

        let (mut editor, id) = editor_with_square();
        editor.start_lattice(LatticeKind::Envelope).unwrap();
        let copy = editor.doc.layer(id).unwrap().elements_at(0)[0].clone();
        editor.doc.layer_mut(id).unwrap().elements_at_mut(0).push(copy);
        editor.check_lattice();
        assert!(editor.lattice.is_none());
    }

    #[test]
    fn the_edit_ends_with_another_frame_or_tool() {
        let (mut editor, _) = editor_with_square();
        editor.start_lattice(LatticeKind::Distort).unwrap();
        editor.check_lattice();
        assert!(editor.lattice.is_some(), "nothing's changed yet");
        editor.tool = Tool::Pencil;
        editor.check_lattice();
        assert!(editor.lattice.is_none());
    }

    #[test]
    fn a_straight_line_gets_a_lattice_with_some_depth() {
        let line = Element {
            shape: Shape::Rect { width: 200.0, height: 0.0, corner_radius: 0.0 },
            style: Style { fill: None, stroke: None, gradient: None },
        };
        let bounds = lattice_bounds(&[(ShapeRef::line(0), line)]).unwrap();
        assert!((bounds.height() - 40.0).abs() < 1e-3, "{bounds:?}");
    }
}
