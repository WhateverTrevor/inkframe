//! Editing a path's anchors with the Subselect tool: picking anchors and
//! handles, adding and converting points, and drawing them over the path.

use egui::{Color32, Pos2, Rect, StrokeKind, Vec2};
use inkframe_core::{Anchor, LayerId, Point, Shape, insert_anchor, nearest_on_anchors, path_from_anchors};

use super::{ANCHOR_HIT, Gesture, SELECTION, StageView, View};
use crate::editor::{Editor, Tool};
use crate::subselect::{AnchorPart, toggle_smooth};

impl StageView {
    /// Picks point `index` of shape `element` in the selected layer, as a
    /// click on it would. For the screenshot hook.
    pub fn pick_anchor(&mut self, editor: &Editor, element: usize, index: usize) {
        if let Some(layer) = editor.selection {
            self.pick_only(AnchorRef { layer, element, index });
        }
    }

    /// The picked points: the key one last, so a list taken from here and
    /// given back to [`Self::set_picked`] keeps it the key.
    pub(super) fn picked_points(&self) -> Vec<AnchorRef> {
        self.more_anchors.iter().copied().chain(self.anchor_selection).collect()
    }

    pub(super) fn is_picked(&self, target: AnchorRef) -> bool {
        self.anchor_selection == Some(target) || self.more_anchors.contains(&target)
    }

    /// Picks `points`, the last of them the key point.
    pub(super) fn set_picked(&mut self, mut points: Vec<AnchorRef>) {
        self.anchor_selection = points.pop();
        self.more_anchors = points;
    }

    pub(super) fn pick_only(&mut self, target: AnchorRef) {
        self.set_picked(vec![target]);
    }

    pub(super) fn clear_picked(&mut self) {
        self.set_picked(Vec::new());
    }

    /// Shift-click: a point picked is let go, and one that isn't is added,
    /// becoming the key point (the one whose handles show).
    pub(super) fn toggle_picked(&mut self, target: AnchorRef) {
        let mut points = self.picked_points();
        if let Some(at) = points.iter().position(|p| *p == target) {
            points.remove(at);
        } else {
            points.push(target);
        }
        self.set_picked(points);
    }

    /// Makes a picked point the key one, keeping the rest; picks only it if
    /// it wasn't picked. A press on a point does this, so dragging one of
    /// several picked points drags them all.
    pub(super) fn make_key(&mut self, target: AnchorRef) {
        if !self.is_picked(target) {
            return self.pick_only(target);
        }
        let mut points: Vec<AnchorRef> = self.picked_points().into_iter().filter(|p| *p != target).collect();
        points.push(target);
        self.set_picked(points);
    }

    /// Starts dragging the anchor point or handle under `press`, if any.
    /// Alt-dragging a point pulls new handles out of it, as in Animate.
    /// With `convert` (the Convert Anchor Point tool, or Alt with the Pen),
    /// a point always pulls new handles and a handle always moves alone.
    pub(super) fn begin_anchor_edit(
        &mut self,
        editor: &Editor,
        view: View,
        press: Pos2,
        p: Point,
        alt: bool,
        convert: bool,
    ) -> Option<Gesture> {
        let (target, part) = self.anchor_hit(editor, view, press)?;
        self.make_key(target);
        let layer = editor.doc.layer(target.layer)?;
        let to_local = editor.doc.layer_matrix(layer, editor.frame).inverse()?;
        // Points are edited where they show: posed, if they're animated.
        let elements = layer.posed_elements(editor.frame);
        let (start, closed) = elements.get(target.element)?.shape.to_anchors();
        let pull = (alt || convert) && part == AnchorPart::Point;
        // Moving a point takes the points welded to it along, unless ⌘ was
        // held to part it from them; it can snap onto any other point.
        let welded = inkframe_core::welded_to(&elements, target.element, target.index);
        let welds = if part == AnchorPart::Point && !pull && !self.unweld {
            welded
                .iter()
                .filter_map(|&(element, index)| {
                    let (anchors, closed) = elements.get(element)?.shape.to_anchors();
                    Some((element, index, anchors, closed))
                })
                .collect()
        } else {
            Vec::new()
        };
        let snaps = elements
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                *i != target.element && e.is_reshapable() && !matches!(e.shape, Shape::Line { .. })
            })
            .flat_map(|(_, e)| e.shape.to_anchors().0.into_iter().map(|a| a.point))
            .filter(|p| p.distance(start[target.index].point) > inkframe_core::WELD_DISTANCE)
            .collect();
        // Dragging one of several picked points moves them all.
        let picked = self.picked_points();
        let group = if part == AnchorPart::Point && !pull && picked.len() > 1 {
            by_element(&picked)
                .into_iter()
                .filter_map(|(element, indices)| {
                    let (anchors, closed) = elements.get(element)?.shape.to_anchors();
                    Some((element, indices, anchors, closed))
                })
                .collect()
        } else {
            Vec::new()
        };
        Some(Gesture::EditAnchor {
            target,
            part,
            pull,
            convert,
            start,
            closed,
            welds,
            snaps,
            group,
            reflow: false,
            grab: to_local.apply(p),
            to_local,
        })
    }

    /// Where a Pen click at `pointer` would add a point to the selected
    /// shape, if it's on the outline.
    ///
    /// Only applies when no new path is being drawn, and never right on an
    /// existing anchor: clicking there should do what clicking an anchor
    /// does, not stack a second point on top of it.
    pub(super) fn outline_hit(&self, editor: &Editor, view: View, pointer: Pos2) -> Option<OutlineHit> {
        if !matches!(editor.tool, Tool::Pen | Tool::AddAnchor) || self.pen.is_some() {
            return None;
        }
        let layer = editor.selected_layer().filter(|l| l.is_shown_at(editor.frame) && !l.locked)?;
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let local = to_stage.inverse()?.apply(view.to_local(pointer));
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let mut best: Option<OutlineHit> = None;
        for (element, e) in layer.posed_elements(editor.frame).iter().enumerate() {
            let (anchors, closed) = e.shape.to_anchors();
            let Some(hit) = nearest_on_anchors(&anchors, closed, local) else { continue };
            // Measure on screen, so the target is the same size at any zoom
            // and on layers that are scaled up or down.
            let on_screen = screen(hit.point);
            let distance = on_screen.distance(pointer);
            let on_anchor = anchors.iter().any(|a| screen(a.point).distance(pointer) <= ANCHOR_HIT);
            let closer = best.is_none_or(|b| distance < b.screen.distance(pointer));
            if distance <= ANCHOR_HIT && !on_anchor && closer {
                best = Some(OutlineHit {
                    layer: layer.id,
                    element,
                    segment: hit.segment,
                    t: hit.t,
                    screen: on_screen,
                });
            }
        }
        best
    }

    /// Drops the anchor selection if it no longer points at something that
    /// exists (after undo, deleting points, or selecting another layer), or
    /// if the Subselection tool isn't active.
    pub(super) fn validate_anchor_selection(&mut self, editor: &mut Editor) {
        let picking = editor.tool == Tool::Subselect || (editor.tool.edits_points() && self.pen.is_none());
        let valid = |target: &AnchorRef| {
            picking
                && editor.selection == Some(target.layer)
                && editor
                    .element(target.layer, target.element)
                    .is_some_and(|e| target.index < e.shape.to_anchors().0.len())
        };
        // Without its key point, the pick is over.
        if !self.anchor_selection.as_ref().is_some_and(valid) {
            self.clear_picked();
        }
        self.more_anchors.retain(valid);
        editor.picked_anchor = self.anchor_selection.map(|a| (a.layer, a.element, a.index));
        editor.more_picked_anchors =
            self.more_anchors.iter().map(|a| (a.layer, a.element, a.index)).collect();
    }

    /// The selected layer's anchors and handles as they appear on screen:
    /// `(target, part, screen position)` for everything that can be dragged.
    ///
    /// Every anchor point is listed, but handles only for the selected
    /// anchor (or all, for Convert Anchor Point), matching what's drawn. Handles come first, so when one sits
    /// on top of an anchor, the handle wins.
    pub(super) fn anchor_targets(&self, editor: &Editor, view: View) -> Vec<(AnchorRef, AnchorPart, Pos2)> {
        let Some(layer) = editor.selected_layer().filter(|l| l.is_shown_at(editor.frame) && !l.locked) else {
            return Vec::new();
        };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let mut handles = Vec::new();
        let mut points = Vec::new();
        for (element, e) in layer.posed_elements(editor.frame).iter().enumerate() {
            for (index, anchor) in e.shape.to_anchors().0.iter().enumerate() {
                let target = AnchorRef { layer: layer.id, element, index };
                points.push((target, AnchorPart::Point, screen(anchor.point)));
                // Convert Anchor Point shows every point's handles, since
                // a click on a curve's point (to show them otherwise) would
                // make it a corner.
                if self.anchor_selection == Some(target) || editor.tool == Tool::ConvertAnchor {
                    for part in [AnchorPart::HandleIn, AnchorPart::HandleOut] {
                        if part.position(anchor) != anchor.point {
                            handles.push((target, part, screen(part.position(anchor))));
                        }
                    }
                }
            }
        }
        handles.extend(points);
        handles
    }

    pub(super) fn anchor_hit(
        &self,
        editor: &Editor,
        view: View,
        pointer: Pos2,
    ) -> Option<(AnchorRef, AnchorPart)> {
        self.anchor_targets(editor, view)
            .into_iter()
            .find(|(_, _, pos)| pos.distance(pointer) <= ANCHOR_HIT)
            .map(|(target, part, _)| (target, part))
    }

    /// Draws the selected layer's outlines, anchor squares, and the handles
    /// of the selected anchor.
    pub(super) fn draw_anchor_overlay(&self, painter: &egui::Painter, editor: &Editor, view: View) {
        let Some(layer) = editor.selected_layer().filter(|l| l.is_shown_at(editor.frame)) else { return };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let screen = |p: Point| view.to_screen(to_stage.apply(p));

        let elements = layer.posed_elements(editor.frame);
        let welded = inkframe_core::welded_points(&elements);
        for (element, e) in elements.iter().enumerate() {
            let (anchors, closed) = e.shape.to_anchors();
            if let Some(path) = path_from_anchors(&anchors, closed) {
                painter.line(path.flatten().into_iter().map(screen).collect(), (1.0, SELECTION));
            }
            for (index, anchor) in anchors.iter().enumerate() {
                let target = AnchorRef { layer: layer.id, element, index };
                let selected = self.is_picked(target);
                let point = screen(anchor.point);
                if self.anchor_selection == Some(target) || editor.tool == Tool::ConvertAnchor {
                    // Handles: a line back to the anchor and a round knob.
                    for handle in [anchor.handle_in, anchor.handle_out] {
                        if handle != anchor.point {
                            painter.line_segment([point, screen(handle)], (1.0, SELECTION));
                            painter.circle_filled(screen(handle), 3.5, SELECTION);
                        }
                    }
                }
                let square = Rect::from_center_size(point, Vec2::splat(7.0));
                painter.rect_filled(square, 0.0, if selected { SELECTION } else { Color32::WHITE });
                painter.rect_stroke(square, 0.0, (1.0, SELECTION), StrokeKind::Middle);
                // A welded point: a dot inside, as it's one point of several
                // shapes.
                if welded.get(element).and_then(|w| w.get(index)).copied().unwrap_or(false) {
                    painter.circle_filled(point, 1.8, if selected { Color32::WHITE } else { SELECTION });
                }
            }
        }
    }
}

/// One anchor of one shape in a layer, as picked with the Subselection tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRef {
    pub(super) layer: LayerId,
    pub(super) element: usize,
    pub(super) index: usize,
}

/// A spot on the selected shape's outline where the Pen tool would add a point.
#[derive(Debug, Clone, Copy)]
pub(super) struct OutlineHit {
    layer: LayerId,
    element: usize,
    segment: usize,
    /// How far along the segment, 0–1.
    t: f32,
    /// Where on screen the new point would go.
    pub(super) screen: Pos2,
}

/// Switches an anchor between corner and smooth. Rectangles and ovals
/// become paths.
pub(super) fn convert_point(editor: &mut Editor, target: AnchorRef) {
    let Some(element) = editor.posed_element(target.layer, target.element) else {
        return;
    };
    let (anchors, closed) = element.shape.to_anchors();
    let anchors = toggle_smooth(&anchors, target.index, closed);
    editor.reshape(target.layer, target.element, &anchors, closed);
}

/// Makes a curve's point a corner, its handles taken in: a click with the
/// Convert Anchor Point tool. A corner already is one, and stays.
pub(super) fn make_corner(editor: &mut Editor, target: AnchorRef) {
    let Some(element) = editor.posed_element(target.layer, target.element) else {
        return;
    };
    let (mut anchors, closed) = element.shape.to_anchors();
    let Some(anchor) = anchors.get_mut(target.index) else { return };
    if anchor.handle_in == anchor.point && anchor.handle_out == anchor.point {
        return;
    }
    *anchor = Anchor::corner(anchor.point);
    editor.reshape(target.layer, target.element, &anchors, closed);
}

/// Adds a point to a shape's outline. Rectangles and ovals become paths.
///
/// It goes into the shape as drawn, at the same place along the same side
/// as it was clicked on the shape as posed, and animated points after it
/// keep their animation.
pub(super) fn insert_point(editor: &mut Editor, hit: OutlineHit) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer_mut(hit.layer) else { return };
    let index = layer.drawing_index_at(frame);
    let Some(drawing) = layer.drawings.get_mut(index) else { return };
    let Some(element) = drawing.elements.get_mut(hit.element) else { return };
    let (anchors, closed) = element.shape.to_anchors();
    let anchors = insert_anchor(&anchors, closed, hit.segment, hit.t);
    if let Some(path) = path_from_anchors(&anchors, closed) {
        element.shape = Shape::Path(path);
        drawing.point_inserted(hit.element, hit.segment + 1);
    }
}

/// Picked points grouped by the shape they're on: each shape's place in
/// the layer, and its picked points' places in it.
pub(super) fn by_element(points: &[AnchorRef]) -> Vec<(usize, Vec<usize>)> {
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for point in points {
        match groups.iter_mut().find(|(element, _)| *element == point.element) {
            Some((_, indices)) => indices.push(point.index),
            None => groups.push((point.element, vec![point.index])),
        }
    }
    groups
}
