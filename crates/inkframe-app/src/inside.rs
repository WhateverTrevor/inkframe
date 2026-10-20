//! Working inside one layer: picking out single shapes from everything
//! drawn on it, to move, scale, delete or make a symbol of just those.
//!
//! A click on the stage still picks the whole layer, so dragging it on
//! another frame animates it as before. Double-clicking a layer steps
//! inside it, as double-clicking a symbol does: the other layers fade, and
//! clicks pick the shapes under the pointer instead. Escape, or
//! double-clicking empty stage, steps back out.
//!
//! Shapes are known by their place in the drawing showing on the frame
//! they were picked on. Changing frame, undoing, or anything else that
//! could shuffle them clears the picked shapes (see [`Editor::check_inside`]).

use inkframe_core::{Affine, Element, LayerId, Point, Rect, SymbolId, TimelineId, Transform};

use crate::editor::Editor;

/// The layer being worked inside, and which of its shapes are picked.
#[derive(Debug, Clone, PartialEq)]
pub struct Inside {
    pub layer: LayerId,
    /// The timeline the layer is in, so opening or leaving a symbol leaves
    /// the layer too.
    timeline: TimelineId,
    /// Places in the drawing showing on `frame`, lowest first.
    pub shapes: Vec<usize>,
    frame: u32,
    /// How many shapes the drawing had when they were picked. If that
    /// changes behind our back (undo, a script), the places may point at
    /// other shapes, so they're dropped.
    count: usize,
}

impl Editor {
    /// Steps inside `layer` to pick out its shapes, selecting it. Only a
    /// vector layer that isn't locked can be stepped into.
    pub fn enter_layer(&mut self, layer: LayerId) -> bool {
        let Some(found) = self.doc.layer(layer) else { return false };
        if !found.draws_vectors() || found.locked || found.instance.is_some() {
            return false;
        }
        let count = found.elements_at(self.frame).len();
        self.select(Some(layer));
        self.inside =
            Some(Inside { layer, timeline: self.timeline(), shapes: Vec::new(), frame: self.frame, count });
        true
    }

    pub fn exit_layer(&mut self) {
        self.inside = None;
    }

    /// The layer being worked inside, if any.
    pub fn inside_layer(&self) -> Option<LayerId> {
        self.inside.as_ref().map(|inside| inside.layer)
    }

    /// The picked shapes' places in the layer's drawing.
    pub fn picked_shapes(&self) -> &[usize] {
        self.inside.as_ref().map_or(&[], |inside| &inside.shapes)
    }

    /// Steps out if the layer has gone, been deselected, or is no longer in
    /// the timeline being edited; drops the picked shapes if the frame or
    /// the drawing has changed under them. Called once a frame.
    pub fn check_inside(&mut self) {
        let Some(inside) = &self.inside else { return };
        let layer = self.doc.layer(inside.layer);
        let still_there = layer.is_some_and(|l| !l.locked)
            && self.selection == Some(inside.layer)
            && self.timeline() == inside.timeline;
        if !still_there {
            self.inside = None;
            return;
        }
        let count = layer.map_or(0, |l| l.elements_at(self.frame).len());
        let frame = self.frame;
        if let Some(inside) = &mut self.inside
            && (inside.frame != frame || inside.count != count)
        {
            inside.shapes.clear();
            inside.frame = frame;
            inside.count = count;
        }
    }

    /// Picks exactly these shapes.
    pub fn pick_shapes(&mut self, shapes: Vec<usize>) {
        let (frame, count) = (self.frame, self.inside_shapes().len());
        if let Some(inside) = &mut self.inside {
            inside.shapes = shapes.into_iter().filter(|&i| i < count).collect();
            inside.shapes.sort_unstable();
            inside.shapes.dedup();
            inside.frame = frame;
            inside.count = count;
        }
    }

    /// Picks every shape that can be picked: Select All, inside a layer.
    pub fn pick_all_shapes(&mut self) {
        let all = self.inside_shapes().iter().enumerate().filter(|(_, s)| s.is_reshapable()).map(|(i, _)| i);
        self.pick_shapes(all.collect());
    }

    /// Adds a shape to the picked ones, or takes it out: Shift-click.
    pub fn toggle_shape(&mut self, shape: usize) {
        let mut shapes = self.picked_shapes().to_vec();
        match shapes.iter().position(|&s| s == shape) {
            Some(i) => {
                shapes.remove(i);
            }
            None => shapes.push(shape),
        }
        self.pick_shapes(shapes);
    }

    /// The shapes of the layer being worked inside, on this frame.
    fn inside_shapes(&self) -> &[Element] {
        self.inside_layer().and_then(|id| self.doc.layer(id)).map_or(&[], |l| l.elements_at(self.frame))
    }

    /// From the layer's own coordinates to the timeline's (the stage's, or
    /// a symbol's being edited).
    fn inside_matrix(&self) -> Option<Affine> {
        let layer = self.doc.layer(self.inside_layer()?)?;
        Some(self.doc.layer_matrix(layer, self.frame))
    }

    /// The topmost shape that can be picked under `point` (in the
    /// timeline's coordinates). Pictures and text can't be: they move with
    /// their layer.
    pub fn shape_at(&self, point: Point, tolerance: f32) -> Option<usize> {
        let to_local = self.inside_matrix()?.inverse()?;
        let local = to_local.apply(point);
        // The tolerance is in the timeline's units; the layer may be scaled.
        let tolerance = tolerance / self.inside_matrix()?.average_scale().max(1e-3);
        let shapes = self.inside_shapes();
        (0..shapes.len()).rev().find(|&i| {
            let shape = &shapes[i];
            shape.is_reshapable() && shape.shape.hit_test(local, &shape.style, tolerance)
        })
    }

    /// The shapes that can be picked which a box (corners in the
    /// timeline's coordinates) touches.
    pub fn shapes_in(&self, a: Point, b: Point) -> Vec<usize> {
        let Some(to_timeline) = self.inside_matrix() else { return Vec::new() };
        let area = Rect::from_points(a, b);
        self.inside_shapes()
            .iter()
            .enumerate()
            .filter(|(_, shape)| shape.is_reshapable())
            .filter(|(_, shape)| {
                Rect::bounding(shape.bounds().corners().map(|c| to_timeline.apply(c))).is_some_and(|b| {
                    b.min.x <= area.max.x
                        && area.min.x <= b.max.x
                        && b.min.y <= area.max.y
                        && area.min.y <= b.max.y
                })
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The box around the picked shapes, in the layer's own coordinates.
    pub fn picked_bounds(&self) -> Option<Rect> {
        let shapes = self.inside_shapes();
        self.picked_shapes().iter().filter_map(|&i| shapes.get(i)).map(Element::bounds).reduce(Rect::union)
    }

    /// The picked shapes, as they are now, to transform from (see
    /// [`Editor::transform_shapes`]).
    pub fn picked_originals(&self) -> Vec<(usize, Element)> {
        let shapes = self.inside_shapes();
        self.picked_shapes().iter().filter_map(|&i| Some((i, shapes.get(i)?.clone()))).collect()
    }

    /// Puts each of `originals` back where it was, mapped by `m` (in the
    /// layer's coordinates). Dragging calls this with the shapes as they
    /// were when the drag began, so nothing drifts.
    pub fn transform_shapes(&mut self, originals: &[(usize, Element)], m: Affine) {
        let Some(id) = self.inside_layer() else { return };
        let frame = self.frame;
        let Some(layer) = self.doc.layer_mut(id) else { return };
        let shapes = layer.elements_at_mut(frame);
        for (i, original) in originals {
            if let (Some(slot), Some(moved)) = (shapes.get_mut(*i), original.transformed(m)) {
                *slot = moved;
            }
        }
        // Rectangles and ovals have become paths, but there are as many
        // shapes as before, so the picked places still hold.
    }

    /// Deletes the picked shapes. Returns whether there were any.
    pub fn delete_shapes(&mut self) -> bool {
        let Some(id) = self.inside_layer() else { return false };
        let picked = self.picked_shapes().to_vec();
        if picked.is_empty() {
            return false;
        }
        let frame = self.frame;
        if let Some(layer) = self.doc.layer_mut(id) {
            let shapes = layer.elements_at_mut(frame);
            for &i in picked.iter().rev() {
                if i < shapes.len() {
                    shapes.remove(i);
                }
            }
        }
        self.pick_shapes(Vec::new());
        true
    }

    /// Makes a symbol of the picked shapes, as F8 does in Animate. They
    /// move to a new layer just above, posed and timed exactly as the one
    /// they came from (same animation, same parent), which then becomes the
    /// symbol's instance, so nothing moves on the stage. Steps out of the
    /// layer and selects the instance.
    pub fn shapes_to_symbol(&mut self) -> Result<SymbolId, String> {
        let id = self.inside_layer().ok_or("Not inside a layer")?;
        let picked = self.picked_originals();
        if picked.is_empty() {
            return Err("Pick some shapes first: click them, or drag a box around them.".to_owned());
        }
        let frame = self.frame;
        let timeline = self.timeline();
        let source = self.doc.layer(id).ok_or("The layer has gone")?.clone();
        let name = self.doc.next_layer_name_in(timeline, &format!("{} part", source.name));
        let shapes: Vec<Element> = picked.iter().map(|(_, e)| e.clone()).collect();
        let new = self
            .doc
            .add_layer_to(timeline, name, shapes, frame, Transform::default())
            .ok_or("Couldn't add a layer")?;
        if let Some(layer) = self.doc.layer_mut(new) {
            layer.track = source.track.clone();
            layer.start = source.start;
            layer.end = source.end;
            layer.parent = source.parent;
            layer.pivot = source.pivot;
            layer.in_folder = source.in_folder;
        }
        if let Some(index) = self.layers().iter().position(|l| l.id == id) {
            self.doc.move_layer_to(new, index + 1);
        }
        self.delete_shapes();
        self.exit_layer();
        let symbol = self.doc.convert_layer_to_symbol(new, frame).ok_or("Couldn't make a symbol")?;
        self.select(Some(new));
        Ok(symbol)
    }
}

#[cfg(test)]
mod tests {
    use inkframe_core::{Color, Document, Shape, Style};

    use super::*;

    /// A layer at (100, 100) with three 20×20 squares drawn into it, at 0,
    /// 50 and 100 along its x axis.
    fn editor_with_three_squares() -> (Editor, LayerId) {
        let mut editor = Editor::new(Document::default());
        let square = |x: f32| {
            Element {
                shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 },
                style: Style { fill: Some(Color::BLACK), stroke: None },
            }
            .transformed(Affine::translate(Point::new(x, 0.0)))
            .unwrap()
        };
        let shapes = vec![square(0.0), square(50.0), square(100.0)];
        let id = editor.doc.add_layer("Shapes", shapes, 0, Transform::at(Point::new(100.0, 100.0)));
        (editor, id)
    }

    #[test]
    fn inside_a_layer_shapes_are_picked_by_point_and_box() {
        let (mut editor, id) = editor_with_three_squares();
        assert!(editor.enter_layer(id));
        assert_eq!(editor.selection, Some(id));
        assert_eq!(editor.shape_at(Point::new(150.0, 100.0), 1.0), Some(1));
        assert_eq!(editor.shape_at(Point::new(125.0, 100.0), 1.0), None, "between them");
        assert_eq!(editor.shapes_in(Point::new(140.0, 80.0), Point::new(260.0, 120.0)), vec![1, 2]);

        editor.pick_shapes(vec![2, 0]);
        assert_eq!(editor.picked_shapes(), &[0, 2]);
        editor.toggle_shape(0);
        assert_eq!(editor.picked_shapes(), &[2]);
    }

    #[test]
    fn moving_picked_shapes_leaves_the_rest_and_the_layer_alone() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![1]);
        let originals = editor.picked_originals();
        editor.transform_shapes(&originals, Affine::translate(Point::new(0.0, 30.0)));

        let layer = editor.doc.layer(id).unwrap();
        let shapes = layer.elements_at(0);
        assert!(shapes[1].bounds().center().distance(Point::new(50.0, 30.0)) < 1e-3);
        assert!(shapes[0].bounds().center().distance(Point::ZERO) < 1e-3, "untouched");
        assert!(layer.track.position.keys().len() == 1, "the layer didn't move or key");
    }

    #[test]
    fn deleting_picked_shapes_and_leaving() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![0, 2]);
        assert!(editor.delete_shapes());
        assert_eq!(editor.doc.layer(id).unwrap().elements_at(0).len(), 1);
        assert!(editor.picked_shapes().is_empty());

        // Selecting another layer steps out.
        let other = editor.doc.add_layer("Other", Vec::new(), 0, Transform::default());
        editor.select(Some(other));
        editor.check_inside();
        assert_eq!(editor.inside_layer(), None);
    }

    #[test]
    fn changing_frame_drops_the_picked_shapes() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![1]);
        editor.set_frame(5);
        editor.check_inside();
        assert_eq!(editor.inside_layer(), Some(id), "still inside");
        assert!(editor.picked_shapes().is_empty());
    }

    #[test]
    fn picked_shapes_become_a_symbol_in_place() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![1, 2]);
        let symbol = editor.shapes_to_symbol().unwrap();

        assert_eq!(editor.doc.layer(id).unwrap().elements_at(0).len(), 1, "one square stays behind");
        let instance = editor.selected_layer().unwrap();
        assert_eq!(instance.instance.map(|i| i.symbol), Some(symbol));
        assert_eq!(editor.inside_layer(), None);
        // Above the layer they came from, and where they were on the stage.
        let order: Vec<_> = editor.layers().iter().map(|l| l.id).collect();
        assert_eq!(order, vec![id, instance.id]);
        let bounds = editor.doc.layer_bounds(instance, 0).unwrap();
        let to_stage = editor.doc.layer_matrix(instance, 0);
        let center = to_stage.apply(bounds.center());
        assert!(center.distance(Point::new(175.0, 100.0)) < 0.5, "{center:?}");
    }
}
