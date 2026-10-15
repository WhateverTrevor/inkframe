//! Lining layers up, as Animate's Align panel does: aligning their edges or
//! centres, spacing them evenly, and making them the same size.
//!
//! Everything works on each layer's bounds on the stage at one frame (its
//! drawing as shown, through its parents), and moves layers by setting
//! their pose on that frame, as dragging them would. With only one layer,
//! or with `to_stage`, the stage itself is what they line up against.

use crate::{Affine, Document, LayerId, Point, Rect, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Document {
    /// A layer's bounds on the stage (in its timeline's space) at `frame`.
    pub fn stage_bounds(&self, id: LayerId, frame: u32) -> Option<Rect> {
        let layer = self.layer(id)?;
        let local = self.layer_bounds(layer, frame)?;
        let to_stage = self.layer_matrix(layer, frame);
        Rect::bounding(local.corners().map(|c| to_stage.apply(c)))
    }

    /// Lines up the layers' edges or centres: with each other, or with the
    /// stage when `to_stage` is set or there's only one.
    pub fn align_layers(&mut self, ids: &[LayerId], frame: u32, how: Align, to_stage: bool) {
        let bounds = self.bounds_of(ids, frame);
        let Some(target) = self.reference(&bounds, to_stage) else { return };
        for (id, rect) in bounds {
            let delta = match how {
                Align::Left => Point::new(target.min.x - rect.min.x, 0.0),
                Align::CenterX => Point::new(target.center().x - rect.center().x, 0.0),
                Align::Right => Point::new(target.max.x - rect.max.x, 0.0),
                Align::Top => Point::new(0.0, target.min.y - rect.min.y),
                Align::CenterY => Point::new(0.0, target.center().y - rect.center().y),
                Align::Bottom => Point::new(0.0, target.max.y - rect.max.y),
            };
            self.shift_on_stage(id, frame, delta);
        }
    }

    /// Spaces the layers' centres evenly along `axis`, between the first
    /// and last of them (or across the stage).
    pub fn distribute_layers(&mut self, ids: &[LayerId], frame: u32, axis: Axis, to_stage: bool) {
        let mut bounds = self.bounds_of(ids, frame);
        let along = |r: &Rect| match axis {
            Axis::Horizontal => r.center().x,
            Axis::Vertical => r.center().y,
        };
        bounds.sort_by(|a, b| along(&a.1).total_cmp(&along(&b.1)));
        let (Some(first), Some(last)) = (bounds.first(), bounds.last()) else { return };
        let (start, end) = if to_stage || bounds.len() < 3 {
            let stage = self.stage_rect();
            match axis {
                Axis::Horizontal => (stage.min.x, stage.max.x),
                Axis::Vertical => (stage.min.y, stage.max.y),
            }
        } else {
            (along(&first.1), along(&last.1))
        };
        // Against the stage, the layers sit in equal shares of it, so one
        // layer lands in the middle; between the end layers, those stay put.
        let n = bounds.len() as f32;
        let at = |i: usize| {
            if to_stage || bounds.len() < 3 {
                start + (end - start) * (i as f32 + 0.5) / n
            } else {
                start + (end - start) * i as f32 / (n - 1.0)
            }
        };
        let moves: Vec<(LayerId, Point)> = bounds
            .iter()
            .enumerate()
            .map(|(i, (id, rect))| {
                let shift = at(i) - along(rect);
                let delta = match axis {
                    Axis::Horizontal => Point::new(shift, 0.0),
                    Axis::Vertical => Point::new(0.0, shift),
                };
                (*id, delta)
            })
            .collect();
        for (id, delta) in moves {
            self.shift_on_stage(id, frame, delta);
        }
    }

    /// Scales the layers so their bounds match the widest (or tallest) of
    /// them, or the stage, along `axis`, each keeping its centre.
    pub fn match_size(&mut self, ids: &[LayerId], frame: u32, axis: Axis, to_stage: bool) {
        let bounds = self.bounds_of(ids, frame);
        let size = |r: &Rect| match axis {
            Axis::Horizontal => r.width(),
            Axis::Vertical => r.height(),
        };
        let target = if to_stage || bounds.len() < 2 {
            size(&self.stage_rect())
        } else {
            bounds.iter().map(|(_, r)| size(r)).fold(0.0, f32::max)
        };
        for (id, rect) in bounds {
            let current = size(&rect);
            if current <= f32::EPSILON {
                continue;
            }
            let factor = target / current;
            let Some(layer) = self.layer(id) else { continue };
            let pose = self.pose(layer, frame);
            let scale = match axis {
                Axis::Horizontal => Point::new(pose.scale.x * factor, pose.scale.y),
                Axis::Vertical => Point::new(pose.scale.x, pose.scale.y * factor),
            };
            self.set_pose(id, frame, Transform { scale, ..pose });
            // Scaling turns around the pivot; put the centre back.
            if let Some(after) = self.stage_bounds(id, frame) {
                self.shift_on_stage(id, frame, rect.center() - after.center());
            }
        }
    }

    fn bounds_of(&self, ids: &[LayerId], frame: u32) -> Vec<(LayerId, Rect)> {
        ids.iter().filter_map(|&id| Some((id, self.stage_bounds(id, frame)?))).collect()
    }

    /// What to line up against: the stage, or the layers' combined bounds.
    fn reference(&self, bounds: &[(LayerId, Rect)], to_stage: bool) -> Option<Rect> {
        if to_stage || bounds.len() < 2 {
            return (!bounds.is_empty()).then(|| self.stage_rect());
        }
        bounds.iter().map(|(_, r)| *r).reduce(Rect::union)
    }

    /// Moves a layer by `delta` on the stage, measured in its parent's
    /// space, which may be turned or scaled.
    fn shift_on_stage(&mut self, id: LayerId, frame: u32, delta: Point) {
        let Some(layer) = self.layer(id) else { return };
        let into_parent = self.parent_matrix(layer, frame).inverse().unwrap_or(Affine::IDENTITY);
        let local = into_parent.apply(delta) - into_parent.apply(Point::ZERO);
        let pose = self.pose(layer, frame);
        self.set_pose(id, frame, Transform { position: pose.position + local, ..pose });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Shape, Style};

    /// Squares of the given sizes centred at the given points.
    fn squares(at: &[(f32, f32, f32)]) -> (Document, Vec<LayerId>) {
        let mut doc = Document::default();
        let ids = at
            .iter()
            .map(|&(x, y, size)| {
                let square = Element {
                    shape: Shape::Rect { width: size, height: size, corner_radius: 0.0 },
                    style: Style { fill: Some(Color::BLACK), stroke: None },
                };
                doc.add_layer("Square", vec![square], 0, Transform::at(Point::new(x, y)))
            })
            .collect();
        (doc, ids)
    }

    fn centre(doc: &Document, id: LayerId) -> Point {
        doc.stage_bounds(id, 0).unwrap().center()
    }

    #[test]
    fn aligning_lines_up_edges_with_each_other() {
        let (mut doc, ids) = squares(&[(100.0, 100.0, 20.0), (300.0, 200.0, 40.0)]);
        doc.align_layers(&ids, 0, Align::Left, false);
        // Both left edges at 90, the first square's.
        assert_eq!(doc.stage_bounds(ids[1], 0).unwrap().min.x, 90.0);
        doc.align_layers(&ids, 0, Align::Bottom, false);
        assert_eq!(doc.stage_bounds(ids[0], 0).unwrap().max.y, 220.0);
    }

    #[test]
    fn one_layer_aligns_with_the_stage() {
        let (mut doc, ids) = squares(&[(100.0, 100.0, 20.0)]);
        doc.align_layers(&ids, 0, Align::CenterX, false);
        doc.align_layers(&ids, 0, Align::CenterY, false);
        assert_eq!(centre(&doc, ids[0]), Point::new(960.0, 540.0));
    }

    #[test]
    fn distributing_spaces_centres_evenly_between_the_ends() {
        let (mut doc, ids) = squares(&[(0.0, 0.0, 10.0), (90.0, 0.0, 10.0), (300.0, 0.0, 10.0)]);
        doc.distribute_layers(&ids, 0, Axis::Horizontal, false);
        assert_eq!(centre(&doc, ids[1]).x, 150.0);
        assert_eq!((centre(&doc, ids[0]).x, centre(&doc, ids[2]).x), (0.0, 300.0));
    }

    #[test]
    fn matching_width_scales_to_the_widest_about_the_centre() {
        let (mut doc, ids) = squares(&[(100.0, 100.0, 20.0), (300.0, 100.0, 40.0)]);
        doc.match_size(&ids, 0, Axis::Horizontal, false);
        let bounds = doc.stage_bounds(ids[0], 0).unwrap();
        assert!((bounds.width() - 40.0).abs() < 1e-3);
        assert!((bounds.center().x - 100.0).abs() < 1e-3);
        assert!((bounds.height() - 20.0).abs() < 1e-3, "height untouched");
    }

    #[test]
    fn a_child_moves_by_the_same_amount_on_stage_whatever_its_parent() {
        let (mut doc, ids) = squares(&[(500.0, 500.0, 20.0), (0.0, 0.0, 20.0)]);
        // The second hangs from the first, which is scaled up 2x.
        doc.layer_mut(ids[1]).unwrap().parent = Some(ids[0]);
        doc.set_pose(
            ids[0],
            0,
            Transform { scale: Point::new(2.0, 2.0), ..Transform::at(Point::new(500.0, 500.0)) },
        );
        doc.align_layers(&ids[1..], 0, Align::Left, true);
        assert!(doc.stage_bounds(ids[1], 0).unwrap().min.x.abs() < 1e-3);
    }
}
