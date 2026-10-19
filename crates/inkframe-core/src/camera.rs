//! The camera, as Animate's camera layer: pan, zoom and turn the whole
//! scene, animated like any layer, with layers at different depths moving
//! by different amounts as it pans (parallax).
//!
//! The camera is a layer with `camera` set, always at the top of the main
//! timeline. Its pose is the camera's: `position` is the point of the
//! scene shown at the middle of the stage, `rotation` turns the view, and
//! `scale.x` zooms (2 shows everything twice as big). Being a layer, it
//! gets keyframes, tweens, easing and the graph editor for free; hiding it
//! turns the camera off.
//!
//! Every layer at the top level of the main timeline is placed through the
//! camera, as part of its parent matrix (see `rig.rs`), so drawing, hit
//! testing, bounds and export all see the scene as the camera does. A
//! layer's `depth` sets how far behind the stage it sits: at depth `d` it
//! appears `F / (F + d)` times its size and moves that much less as the
//! camera pans, `F` being [`FOCAL_LENGTH`]. Negative depths come forward.

use crate::document::{Document, Layer, LayerId};
use crate::geometry::{Affine, Point};
use crate::transform::Transform;

/// How far the camera sits in front of the stage, in stage pixels: a layer
/// this deep appears half its size.
pub const FOCAL_LENGTH: f32 = 1000.0;

/// The nearest a layer can come: closer than this it would fill the view.
const NEAREST: f32 = -0.9 * FOCAL_LENGTH;

impl Document {
    /// The camera, if the main timeline has one and it's switched on (not
    /// hidden).
    pub fn camera(&self) -> Option<&Layer> {
        self.layers.last().filter(|l| l.camera && l.visible)
    }

    /// The camera layer, on or off.
    pub fn camera_layer(&self) -> Option<&Layer> {
        self.layers.last().filter(|l| l.camera)
    }

    /// Adds a camera to the main timeline, looking straight at the stage,
    /// or returns the one there is.
    pub fn add_camera(&mut self) -> LayerId {
        if let Some(camera) = self.layers.iter().find(|l| l.camera) {
            return camera.id;
        }
        let centre = self.stage_rect().center();
        let id = self.add_layer("Camera", Vec::new(), 0, Transform::at(centre));
        if let Some(camera) = self.layer_mut(id) {
            camera.camera = true;
        }
        self.keep_camera_on_top();
        id
    }

    /// Maps the scene at `depth` to the stage as the camera sees it on
    /// `frame`; `None` with no camera.
    pub fn camera_matrix(&self, depth: f32, frame: u32) -> Option<Affine> {
        self.camera_matrix_at(depth, frame as f32)
    }

    fn camera_matrix_at(&self, depth: f32, frame: f32) -> Option<Affine> {
        let camera = self.camera()?;
        let pose = self.pose_at(camera, frame);
        let centre = self.stage_rect().center();
        let perspective = FOCAL_LENGTH / (FOCAL_LENGTH + depth.max(NEAREST));
        let zoom = pose.scale.x.max(0.01) * perspective;
        // The point the camera looks at goes to the middle of the stage,
        // turned and zoomed about there.
        Some(
            Affine::translate(centre)
                .pre_concat(Affine::scale(zoom, zoom))
                .pre_concat(Affine::rotate(-pose.rotation.to_radians()))
                .pre_concat(Affine::translate(Point::ZERO - pose.position)),
        )
    }

    /// The camera's part of a top-level layer's placement: the identity
    /// for the camera itself, layers in symbols, or with no camera.
    pub(crate) fn camera_for(&self, root: &Layer, frame: f32) -> Affine {
        if root.camera || !self.in_main_timeline(root) {
            return Affine::IDENTITY;
        }
        self.camera_matrix_at(root.depth, frame).unwrap_or(Affine::IDENTITY)
    }

    /// Whether `layer` is one of the main timeline's (rather than a
    /// symbol's). Checked by address first, which costs nothing for the
    /// many layers asked about every frame; a copy (a thumbnail draws one)
    /// is looked up by its id.
    fn in_main_timeline(&self, layer: &Layer) -> bool {
        let at = |layers: &[Layer]| layers.as_ptr_range().contains(&(layer as *const Layer));
        if at(&self.layers) {
            return true;
        }
        if self.symbols.iter().any(|s| at(&s.layers)) {
            return false;
        }
        self.layers.iter().any(|l| l.id == layer.id)
    }

    /// Puts the camera back on top of the main timeline, outside any
    /// folder, after layers are added or restacked.
    pub(crate) fn keep_camera_on_top(&mut self) {
        let Some(at) = self.layers.iter().position(|l| l.camera) else { return };
        let mut camera = self.layers.remove(at);
        (camera.in_folder, camera.parent) = (None, None);
        self.layers.push(camera);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Shape, Style};

    fn dot_at(doc: &mut Document, x: f32, y: f32) -> LayerId {
        let dot = Element {
            shape: Shape::Rect { width: 2.0, height: 2.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        doc.add_layer("Dot", vec![dot], 0, Transform::at(Point::new(x, y)))
    }

    /// Where a layer's origin appears on the stage.
    fn seen(doc: &Document, id: LayerId, frame: u32) -> Point {
        let layer = doc.layer(id).unwrap();
        doc.layer_matrix(layer, frame).apply(Point::ZERO)
    }

    fn near(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-2
    }

    #[test]
    fn a_new_camera_changes_nothing_until_it_moves() {
        let mut doc = Document::default();
        let dot = dot_at(&mut doc, 100.0, 200.0);
        doc.add_camera();
        assert!(near(seen(&doc, dot, 0), Point::new(100.0, 200.0)));
    }

    #[test]
    fn panning_zooming_and_turning_move_the_scene() {
        let mut doc = Document::default();
        let dot = dot_at(&mut doc, 1060.0, 540.0);
        let camera = doc.add_camera();
        // Look 100 right of centre: the dot, 100 right of centre, is now
        // in the middle.
        doc.set_pose(camera, 0, Transform::at(Point::new(1060.0, 540.0)));
        assert!(near(seen(&doc, dot, 0), Point::new(960.0, 540.0)));
        // Back at the centre, zoomed 2×: the dot is twice as far out.
        doc.set_pose(
            camera,
            0,
            Transform { scale: Point::new(2.0, 2.0), ..Transform::at(Point::new(960.0, 540.0)) },
        );
        assert!(near(seen(&doc, dot, 0), Point::new(1160.0, 540.0)));
        // Turned 90° clockwise: the scene turns the other way.
        doc.set_pose(camera, 0, Transform { rotation: 90.0, ..Transform::at(Point::new(960.0, 540.0)) });
        assert!(near(seen(&doc, dot, 0), Point::new(960.0, 440.0)));
    }

    #[test]
    fn deeper_layers_move_less_and_look_smaller() {
        let mut doc = Document::default();
        let near_dot = dot_at(&mut doc, 960.0, 540.0);
        let far_dot = dot_at(&mut doc, 960.0, 540.0);
        doc.layer_mut(far_dot).unwrap().depth = FOCAL_LENGTH;
        let camera = doc.add_camera();
        doc.set_pose(camera, 0, Transform::at(Point::new(1060.0, 540.0)));
        assert!(near(seen(&doc, near_dot, 0), Point::new(860.0, 540.0)));
        // Twice as far from the camera: half the movement.
        assert!(near(seen(&doc, far_dot, 0), Point::new(910.0, 540.0)));
    }

    #[test]
    fn hiding_the_camera_turns_it_off() {
        let mut doc = Document::default();
        let dot = dot_at(&mut doc, 100.0, 100.0);
        let camera = doc.add_camera();
        doc.set_pose(camera, 0, Transform::at(Point::new(0.0, 0.0)));
        doc.layer_mut(camera).unwrap().visible = false;
        assert!(near(seen(&doc, dot, 0), Point::new(100.0, 100.0)));
    }

    #[test]
    fn the_camera_stays_on_top() {
        let mut doc = Document::default();
        let camera = doc.add_camera();
        let later = dot_at(&mut doc, 0.0, 0.0);
        assert_eq!(doc.layers.last().unwrap().id, camera);
        doc.move_layer_to(later, 5);
        doc.place_layer(later, None, None);
        assert_eq!(doc.layers.last().unwrap().id, camera);
        assert_eq!(doc.add_camera(), camera, "only ever one");
    }

    #[test]
    fn children_follow_their_parent_through_the_camera_once() {
        let mut doc = Document::default();
        let parent = dot_at(&mut doc, 1000.0, 540.0);
        let child = dot_at(&mut doc, 10.0, 0.0);
        doc.layer_mut(child).unwrap().parent = Some(parent);
        let camera = doc.add_camera();
        doc.set_pose(camera, 0, Transform::at(Point::new(1060.0, 540.0)));
        // The parent is 60 left of where the camera looks; its child 10
        // further right, so 50 left of the middle.
        assert!(near(seen(&doc, child, 0), Point::new(910.0, 540.0)));
    }
}
