//! Combining picked shapes (Modify › Combine Shapes: Union, Intersect,
//! Punch, Crop) and Break Apart, on the layer being worked inside. The
//! geometry is in the core (`boolean.rs`); this is which shapes, and where
//! the results go.

use inkframe_core::{Element, Point, Shape, ShapeOp, area_of, break_apart, combine};

use crate::editor::Editor;
use crate::inside::ShapeRef;

impl Editor {
    /// Combines the picked shapes, which must be at least two in one part
    /// of the drawing (line art or colour art). The results take the lowest
    /// one's place in the stack and are left picked.
    pub fn combine_shapes(&mut self, how: ShapeOp) -> Result<(), String> {
        let picked = self.picked_originals();
        if picked.len() < 2 {
            return Err(
                "Pick two or more shapes first: double-click a layer, then click or drag over them.".into()
            );
        }
        let colour_art = picked[0].0.colour_art;
        if picked.iter().any(|(r, _)| r.colour_art != colour_art) {
            return Err("Pick lines or fills, not both: they're separate parts of the drawing.".into());
        }
        let shapes: Vec<Element> = picked.iter().map(|(_, e)| e.clone()).collect();
        let results = combine(how, &shapes).map_err(|e| e.to_string())?;
        let at = picked[0].0.index;
        self.delete_shapes();
        let count = self.replace_in_drawing(colour_art, at, results);
        self.pick_shapes((at..at + count).map(|i| ShapeRef { colour_art, index: i }).collect());
        Ok(())
    }

    /// Animate's Break Apart: text becomes a shape for each letter (with
    /// its holes, as in an O), and a pencil line becomes the filled shape
    /// it draws. Acts on the picked shapes, or with none picked, everything
    /// in the selected layer's drawing. Returns how many were broken apart.
    pub fn break_apart(&mut self) -> usize {
        let Some(layer) = self.inside_layer().or(self.selection) else { return 0 };
        let frame = self.frame;
        let Some(found) = self.doc.layer(layer) else { return 0 };
        if found.locked || !found.draws_vectors() {
            return 0;
        }
        let drawing = found.drawing_index_at(frame);
        let targets: Vec<usize> = if self.inside_layer() == Some(layer) && !self.picked_shapes().is_empty() {
            self.picked_shapes().iter().filter(|r| !r.colour_art).map(|r| r.index).collect()
        } else {
            (0..found.drawings[drawing].elements.len()).collect()
        };
        let mut broken = 0;
        // From the top down, so replacing one doesn't move the next.
        for &index in targets.iter().rev() {
            let Some(element) =
                self.doc.layer(layer).and_then(|l| l.drawings[drawing].elements.get(index)).cloned()
            else {
                continue;
            };
            let pieces = match &element.shape {
                Shape::Text(_) => {
                    let Some(outline) = inkframe_render::shape_outline(&element.shape) else { continue };
                    break_apart(&rings(&outline), &element.style)
                }
                // A pencil line's outline, filled in the line's colour.
                Shape::Line { .. } => {
                    let Some(area) = area_of(&element) else { continue };
                    let color = element.style.stroke.map(|s| s.color).or(element.style.fill);
                    let style = inkframe_core::Style { fill: color, stroke: None, gradient: None };
                    break_apart(&area, &style)
                }
                _ => continue,
            };
            if let Some(layer) = self.doc.layer_mut(layer) {
                let count = pieces.len();
                let drawing = &mut layer.drawings[drawing];
                drawing.elements.splice(index..=index, pieces);
                // Bindings to the shape broken apart go; later ones move down.
                drawing.shape_removed(false, index);
                drawing.shapes_inserted(false, index, count);
            }
            broken += 1;
        }
        if broken > 0 && self.inside_layer() == Some(layer) {
            self.pick_shapes(Vec::new());
        }
        broken
    }

    /// Puts `shapes` into the drawing being worked inside at `at`, in its
    /// line art or colour art. Returns how many went in.
    fn replace_in_drawing(&mut self, colour_art: bool, at: usize, shapes: Vec<Element>) -> usize {
        let (Some(id), frame) = (self.inside_layer(), self.frame) else { return 0 };
        let Some(layer) = self.doc.layer_mut(id) else { return 0 };
        let index = layer.drawing_index_at(frame);
        let part = if colour_art {
            &mut layer.drawings[index].colour_art
        } else {
            &mut layer.drawings[index].elements
        };
        let at = at.min(part.len());
        let count = shapes.len();
        part.splice(at..at, shapes);
        layer.drawings[index].shapes_inserted(colour_art, at, count);
        count
    }
}

/// A tiny-skia outline as loops of points, curves flattened, for the core's
/// shape operations.
fn rings(path: &tiny_skia::Path) -> Vec<Vec<Point>> {
    use tiny_skia::PathSegment;
    const STEPS: usize = 8;
    let mut rings: Vec<Vec<Point>> = Vec::new();
    let mut ring: Vec<Point> = Vec::new();
    let point = |p: tiny_skia::Point| Point::new(p.x, p.y);
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if ring.len() >= 3 {
                    rings.push(std::mem::take(&mut ring));
                }
                ring = vec![point(p)];
            }
            PathSegment::LineTo(p) => ring.push(point(p)),
            PathSegment::QuadTo(q, p) => {
                let from = *ring.last().unwrap_or(&point(p));
                for step in 1..=STEPS {
                    let t = step as f32 / STEPS as f32;
                    let a = from + (point(q) - from) * t;
                    let b = point(q) + (point(p) - point(q)) * t;
                    ring.push(a + (b - a) * t);
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let from = *ring.last().unwrap_or(&point(p));
                let (b, c, d) = (point(c1), point(c2), point(p));
                for step in 1..=STEPS {
                    let t = step as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    ring.push(
                        from * (u * u * u) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * (t * t * t),
                    );
                }
            }
            PathSegment::Close => {
                if ring.len() >= 3 {
                    rings.push(std::mem::take(&mut ring));
                }
            }
        }
    }
    if ring.len() >= 3 {
        rings.push(ring);
    }
    rings
}

#[cfg(test)]
mod tests {
    use inkframe_core::{Affine, Color, Document, Style, Transform};

    use super::*;

    fn editor_with_two_squares() -> (Editor, inkframe_core::LayerId) {
        let mut editor = Editor::new(Document::default());
        let square = |x: f32, c: Color| {
            Element {
                shape: Shape::Rect { width: 20.0, height: 20.0, corner_radius: 0.0 },
                style: Style { fill: Some(c), stroke: None, gradient: None },
            }
            .transformed(Affine::translate(Point::new(x, 0.0)))
            .unwrap()
        };
        let shapes = vec![square(0.0, Color::BLACK), square(10.0, Color::WHITE), square(100.0, Color::BLACK)];
        let id = editor.doc.add_layer("Shapes", shapes, 0, Transform::default());
        editor.enter_layer(id);
        (editor, id)
    }

    #[test]
    fn combining_replaces_the_picked_shapes_in_place() {
        let (mut editor, id) = editor_with_two_squares();
        editor.pick_shapes(vec![ShapeRef::line(0), ShapeRef::line(1)]);
        editor.combine_shapes(ShapeOp::Union).unwrap();
        let shapes = editor.doc.layer(id).unwrap().elements_at(0);
        assert_eq!(shapes.len(), 2, "two became one; the third is untouched");
        assert_eq!(shapes[0].style.fill, Some(Color::WHITE), "the top one's style");
        assert!((shapes[0].bounds().width() - 30.0).abs() < 0.5);
        assert_eq!(editor.picked_shapes(), &[ShapeRef::line(0)]);
    }

    #[test]
    fn combining_needs_two_shapes() {
        let (mut editor, _) = editor_with_two_squares();
        editor.pick_shapes(vec![ShapeRef::line(0)]);
        assert!(editor.combine_shapes(ShapeOp::Punch).is_err());
    }

    #[test]
    fn breaking_apart_text_gives_a_shape_per_letter() {
        let mut editor = Editor::new(Document::default());
        let id = editor.add_text(Point::new(100.0, 100.0)).unwrap();
        if let Some(Shape::Text(text)) =
            editor.doc.layer_mut(id).unwrap().elements_at_mut(0).first_mut().map(|e| &mut e.shape)
        {
            text.content = "Hi".into();
        }
        editor.select(Some(id));
        assert_eq!(editor.break_apart(), 1);
        let shapes = editor.doc.layer(id).unwrap().elements_at(0);
        // H, and i's stem and dot.
        assert_eq!(shapes.len(), 3, "{}", shapes.len());
        assert!(shapes.iter().all(|s| matches!(s.shape, Shape::Path(_))));
    }
}
