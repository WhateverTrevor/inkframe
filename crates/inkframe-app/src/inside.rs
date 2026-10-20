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

use inkframe_core::{
    Affine, CopiedShapes, Element, Layer, LayerId, Point, Rect, SymbolId, TimelineId, Transform,
};

use crate::editor::Editor;

/// One shape in the drawing showing: in its line art, or in the colour art
/// beneath, and its place there. Ordered with the line art first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShapeRef {
    pub colour_art: bool,
    pub index: usize,
}

impl ShapeRef {
    pub fn line(index: usize) -> Self {
        Self { colour_art: false, index }
    }

    pub fn colour(index: usize) -> Self {
        Self { colour_art: true, index }
    }
}

/// The layer being worked inside, and which of its shapes are picked.
#[derive(Debug, Clone, PartialEq)]
pub struct Inside {
    pub layer: LayerId,
    /// The timeline the layer is in, so opening or leaving a symbol leaves
    /// the layer too.
    timeline: TimelineId,
    pub shapes: Vec<ShapeRef>,
    frame: u32,
    /// How many shapes the drawing's line art and colour art had when they
    /// were picked. If that changes behind our back (undo, a script), the
    /// places may point at other shapes, so they're dropped.
    count: (usize, usize),
}

/// The line art and colour art showing on `frame`.
fn parts(layer: &Layer, frame: u32) -> (&[Element], &[Element]) {
    match layer.drawings.get(layer.drawing_index_at(frame)) {
        Some(drawing) => (&drawing.elements, &drawing.colour_art),
        None => (&[], &[]),
    }
}

fn part_mut(layer: &mut Layer, frame: u32, colour_art: bool) -> &mut Vec<Element> {
    let index = layer.drawing_index_at(frame);
    let drawing = &mut layer.drawings[index];
    if colour_art { &mut drawing.colour_art } else { &mut drawing.elements }
}

impl Editor {
    /// Steps inside `layer` to pick out its shapes, selecting it. Only a
    /// vector layer that isn't locked can be stepped into.
    pub fn enter_layer(&mut self, layer: LayerId) -> bool {
        let Some(found) = self.doc.layer(layer) else { return false };
        if !found.draws_vectors() || found.locked || found.instance.is_some() || found.drawings.is_empty() {
            return false;
        }
        let (lines, colour) = parts(found, self.frame);
        let count = (lines.len(), colour.len());
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

    /// The picked shapes, line art first, each part in drawing order.
    pub fn picked_shapes(&self) -> &[ShapeRef] {
        self.inside.as_ref().map_or(&[], |inside| &inside.shapes)
    }

    /// How many shapes the layer being worked inside shows now, in its
    /// line art and colour art.
    fn inside_count(&self) -> (usize, usize) {
        let (lines, colour) = self.inside_parts();
        (lines.len(), colour.len())
    }

    /// Steps out if the layer has gone, been deselected, or is no longer in
    /// the timeline being edited; drops the picked shapes if the frame or
    /// the drawing has changed under them. Called once a frame.
    pub fn check_inside(&mut self) {
        let Some(inside) = &self.inside else { return };
        let still_there = self.doc.layer(inside.layer).is_some_and(|l| !l.locked)
            && self.selection == Some(inside.layer)
            && self.timeline() == inside.timeline;
        if !still_there {
            self.inside = None;
            return;
        }
        let (count, frame) = (self.inside_count(), self.frame);
        if let Some(inside) = &mut self.inside
            && (inside.frame != frame || inside.count != count)
        {
            inside.shapes.clear();
            inside.frame = frame;
            inside.count = count;
        }
    }

    /// Picks exactly these shapes.
    pub fn pick_shapes(&mut self, shapes: Vec<ShapeRef>) {
        let (count, frame) = (self.inside_count(), self.frame);
        if let Some(inside) = &mut self.inside {
            let exists = |s: &ShapeRef| s.index < if s.colour_art { count.1 } else { count.0 };
            inside.shapes = shapes.into_iter().filter(exists).collect();
            inside.shapes.sort_unstable();
            inside.shapes.dedup();
            inside.frame = frame;
            inside.count = count;
        }
    }

    /// Picks every shape that can be picked: Select All, inside a layer.
    pub fn pick_all_shapes(&mut self) {
        let all = self.inside_refs().into_iter().filter(|(_, s)| s.is_reshapable()).map(|(r, _)| r);
        self.pick_shapes(all.collect());
    }

    /// Adds a shape to the picked ones, or takes it out: Shift-click.
    pub fn toggle_shape(&mut self, shape: ShapeRef) {
        let mut shapes = self.picked_shapes().to_vec();
        match shapes.iter().position(|&s| s == shape) {
            Some(i) => {
                shapes.remove(i);
            }
            None => shapes.push(shape),
        }
        self.pick_shapes(shapes);
    }

    /// The line art and colour art of the layer being worked inside, on
    /// this frame.
    fn inside_parts(&self) -> (&[Element], &[Element]) {
        match self.inside_layer().and_then(|id| self.doc.layer(id)) {
            Some(layer) => parts(layer, self.frame),
            None => (&[], &[]),
        }
    }

    /// Every shape of the layer being worked inside, bottom first: the
    /// colour art, then the line art over it.
    fn inside_refs(&self) -> Vec<(ShapeRef, &Element)> {
        let (lines, colour) = self.inside_parts();
        let colour = colour.iter().enumerate().map(|(i, e)| (ShapeRef::colour(i), e));
        colour.chain(lines.iter().enumerate().map(|(i, e)| (ShapeRef::line(i), e))).collect()
    }

    fn inside_shape(&self, shape: ShapeRef) -> Option<&Element> {
        let (lines, colour) = self.inside_parts();
        if shape.colour_art { colour.get(shape.index) } else { lines.get(shape.index) }
    }

    /// From the layer's own coordinates to the timeline's (the stage's, or
    /// a symbol's being edited).
    fn inside_matrix(&self) -> Option<Affine> {
        let layer = self.doc.layer(self.inside_layer()?)?;
        Some(self.doc.layer_matrix(layer, self.frame))
    }

    /// The topmost shape that can be picked under `point` (in the
    /// timeline's coordinates): a line before the fill beneath it. Pictures
    /// and text can't be picked: they move with their layer.
    pub fn shape_at(&self, point: Point, tolerance: f32) -> Option<ShapeRef> {
        let to_timeline = self.inside_matrix()?;
        let local = to_timeline.inverse()?.apply(point);
        // The tolerance is in the timeline's units; the layer may be scaled.
        let tolerance = tolerance / to_timeline.average_scale().max(1e-3);
        self.inside_refs()
            .into_iter()
            .rev()
            .find(|(_, shape)| shape.is_reshapable() && shape.shape.hit_test(local, &shape.style, tolerance))
            .map(|(r, _)| r)
    }

    /// The shapes that can be picked which a box (corners in the
    /// timeline's coordinates) touches.
    pub fn shapes_in(&self, a: Point, b: Point) -> Vec<ShapeRef> {
        let Some(to_timeline) = self.inside_matrix() else { return Vec::new() };
        let area = Rect::from_points(a, b);
        let touches = |shape: &Element| {
            Rect::bounding(shape.bounds().corners().map(|c| to_timeline.apply(c))).is_some_and(|b| {
                b.min.x <= area.max.x
                    && area.min.x <= b.max.x
                    && b.min.y <= area.max.y
                    && area.min.y <= b.max.y
            })
        };
        let mut found: Vec<ShapeRef> = self
            .inside_refs()
            .into_iter()
            .filter(|(_, shape)| shape.is_reshapable() && touches(shape))
            .map(|(r, _)| r)
            .collect();
        found.sort_unstable();
        found
    }

    /// The box around the picked shapes, in the layer's own coordinates.
    pub fn picked_bounds(&self) -> Option<Rect> {
        self.picked_shapes()
            .iter()
            .filter_map(|&s| self.inside_shape(s))
            .map(Element::bounds)
            .reduce(Rect::union)
    }

    /// The picked shapes, as they are now, to transform from (see
    /// [`Editor::transform_shapes`]).
    pub fn picked_originals(&self) -> Vec<(ShapeRef, Element)> {
        self.picked_shapes().iter().filter_map(|&s| Some((s, self.inside_shape(s)?.clone()))).collect()
    }

    /// Puts each of `originals` back where it was, mapped by `m` (in the
    /// layer's coordinates). Dragging calls this with the shapes as they
    /// were when the drag began, so nothing drifts.
    pub fn transform_shapes(&mut self, originals: &[(ShapeRef, Element)], m: Affine) {
        let Some(id) = self.inside_layer() else { return };
        let frame = self.frame;
        let Some(layer) = self.doc.layer_mut(id) else { return };
        for (shape, original) in originals {
            let part = part_mut(layer, frame, shape.colour_art);
            if let (Some(slot), Some(moved)) = (part.get_mut(shape.index), original.transformed(m)) {
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
            // From the back, so removing one doesn't move the next.
            for shape in picked.iter().rev() {
                let part = part_mut(layer, frame, shape.colour_art);
                if shape.index < part.len() {
                    part.remove(shape.index);
                }
            }
        }
        self.pick_shapes(Vec::new());
        true
    }

    /// The picked shapes for the clipboard, in the timeline's coordinates,
    /// so they can be pasted into a layer that's placed differently.
    pub fn copy_shapes(&self) -> Option<CopiedShapes> {
        let to_timeline = self.inside_matrix()?;
        let picked = self.picked_originals();
        if picked.is_empty() {
            return None;
        }
        let mut copied = CopiedShapes::default();
        for (shape, element) in picked {
            let Some(element) = element.transformed(to_timeline) else { continue };
            if shape.colour_art { copied.colour.push(element) } else { copied.lines.push(element) }
        }
        Some(copied)
    }

    /// Pastes shapes into the layer being worked inside, or steps into the
    /// selected layer (or a new one) first. With `centered_on`, they land
    /// centered there; without, where they were copied from ("Paste in
    /// Place"). The pasted shapes are left picked.
    pub fn paste_shapes(&mut self, copied: &CopiedShapes, centered_on: Option<Point>) {
        if self.inside_layer().is_none() {
            let into = self.selection.filter(|&id| self.enter_layer(id));
            if into.is_none() {
                let Some(new) = self.add_empty_layer() else { return };
                self.enter_layer(new);
            }
        }
        let Some(id) = self.inside_layer() else { return };
        let Some(to_local) = self.inside_matrix().and_then(|m| m.inverse()) else { return };
        let all = copied.lines.iter().chain(&copied.colour);
        let offset = match (centered_on, all.map(Element::bounds).reduce(Rect::union)) {
            (Some(center), Some(bounds)) => center - bounds.center(),
            _ => Point::ZERO,
        };
        let place = to_local.pre_concat(Affine::translate(offset));
        let frame = self.frame;
        let (lines_before, colour_before) = self.inside_count();
        if let Some(layer) = self.doc.layer_mut(id) {
            for (colour_art, list) in [(false, &copied.lines), (true, &copied.colour)] {
                let part = part_mut(layer, frame, colour_art);
                part.extend(list.iter().filter_map(|e| e.transformed(place)));
            }
        }
        let (lines_after, colour_after) = self.inside_count();
        let pasted = (lines_before..lines_after)
            .map(ShapeRef::line)
            .chain((colour_before..colour_after).map(ShapeRef::colour))
            .collect();
        self.pick_shapes(pasted);
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
        let lines: Vec<Element> =
            picked.iter().filter(|(s, _)| !s.colour_art).map(|(_, e)| e.clone()).collect();
        let colour: Vec<Element> =
            picked.iter().filter(|(s, _)| s.colour_art).map(|(_, e)| e.clone()).collect();
        let new = self
            .doc
            .add_layer_to(timeline, name, lines, frame, Transform::default())
            .ok_or("Couldn't add a layer")?;
        if let Some(layer) = self.doc.layer_mut(new) {
            layer.drawings[0].colour_art = colour;
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
                style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
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
        assert_eq!(editor.shape_at(Point::new(150.0, 100.0), 1.0), Some(ShapeRef::line(1)));
        assert_eq!(editor.shape_at(Point::new(125.0, 100.0), 1.0), None, "between them");
        assert_eq!(
            editor.shapes_in(Point::new(140.0, 80.0), Point::new(260.0, 120.0)),
            vec![ShapeRef::line(1), ShapeRef::line(2)]
        );

        editor.pick_shapes(vec![ShapeRef::line(2), ShapeRef::line(0)]);
        assert_eq!(editor.picked_shapes(), &[ShapeRef::line(0), ShapeRef::line(2)]);
        editor.toggle_shape(ShapeRef::line(0));
        assert_eq!(editor.picked_shapes(), &[ShapeRef::line(2)]);
    }

    #[test]
    fn moving_picked_shapes_leaves_the_rest_and_the_layer_alone() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![ShapeRef::line(1)]);
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
        editor.pick_shapes(vec![ShapeRef::line(0), ShapeRef::line(2)]);
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
        editor.pick_shapes(vec![ShapeRef::line(1)]);
        editor.set_frame(5);
        editor.check_inside();
        assert_eq!(editor.inside_layer(), Some(id), "still inside");
        assert!(editor.picked_shapes().is_empty());
    }

    #[test]
    fn picked_shapes_become_a_symbol_in_place() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![ShapeRef::line(1), ShapeRef::line(2)]);
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

    #[test]
    fn colour_art_is_picked_under_the_lines_and_moves_with_them() {
        let (mut editor, id) = editor_with_three_squares();
        // A fill under the middle square, wider than it.
        let fill = Element {
            shape: Shape::Rect { width: 40.0, height: 40.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::WHITE), stroke: None, gradient: None },
        }
        .transformed(Affine::translate(Point::new(50.0, 0.0)))
        .unwrap();
        editor.doc.layer_mut(id).unwrap().drawings[0].colour_art.push(fill);
        editor.enter_layer(id);

        // On the square, the line art wins; beside it, the fill.
        assert_eq!(editor.shape_at(Point::new(150.0, 100.0), 0.5), Some(ShapeRef::line(1)));
        assert_eq!(editor.shape_at(Point::new(165.0, 100.0), 0.5), Some(ShapeRef::colour(0)));

        editor.pick_shapes(vec![ShapeRef::line(1), ShapeRef::colour(0)]);
        let originals = editor.picked_originals();
        editor.transform_shapes(&originals, Affine::translate(Point::new(0.0, 10.0)));
        let drawing = &editor.doc.layer(id).unwrap().drawings[0];
        assert!(drawing.colour_art[0].bounds().center().distance(Point::new(50.0, 10.0)) < 1e-3);
        assert!(drawing.elements[1].bounds().center().distance(Point::new(50.0, 10.0)) < 1e-3);
    }

    #[test]
    fn copied_shapes_paste_into_another_layer_where_they_were() {
        let (mut editor, id) = editor_with_three_squares();
        editor.enter_layer(id);
        editor.pick_shapes(vec![ShapeRef::line(2)]);
        let copied = editor.copy_shapes().unwrap();

        // Another layer, placed elsewhere and scaled: the copy still lands
        // on the stage where the original is.
        let mut other = Transform::at(Point::new(400.0, 300.0));
        other.scale = Point::new(2.0, 2.0);
        let target = editor.doc.add_layer("Target", Vec::new(), 0, other);
        editor.select(Some(target));
        editor.check_inside();
        editor.paste_shapes(&copied, None);

        assert_eq!(editor.inside_layer(), Some(target));
        assert_eq!(editor.picked_shapes(), &[ShapeRef::line(0)]);
        let layer = editor.doc.layer(target).unwrap();
        let to_stage = editor.doc.layer_matrix(layer, 0);
        let center = to_stage.apply(layer.elements_at(0)[0].bounds().center());
        assert!(center.distance(Point::new(200.0, 100.0)) < 1e-2, "{center:?}");

        // Paste in the center of somewhere: centered there.
        editor.paste_shapes(&copied, Some(Point::new(600.0, 500.0)));
        let layer = editor.doc.layer(target).unwrap();
        let center = to_stage.apply(layer.elements_at(0)[1].bounds().center());
        assert!(center.distance(Point::new(600.0, 500.0)) < 1e-2, "{center:?}");
    }
}
