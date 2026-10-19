//! Master controllers, as Harmony's: a slider that poses several layers at
//! once, between poses recorded along it.
//!
//! A controller drives a set of layers. Each pose it holds is those layers'
//! transforms and drawings, recorded at a point along the slider ("Neutral"
//! at 0, "Smile" at 1, say). Setting the slider blends the transforms of
//! the two poses either side of it and shows the drawings of the nearer
//! one, then keys the layers on the current frame, as any edit would. So
//! the controller is a quick way to pose, and what it poses animates like
//! everything else: set it on one frame, set it again on another, and the
//! layers tween between.

use serde::{Deserialize, Serialize};

use crate::document::{Document, LayerId};
use crate::geometry::Point;
use crate::transform::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Controller {
    pub name: String,
    /// The layers it poses.
    pub layers: Vec<LayerId>,
    /// Its poses, in slider order.
    pub poses: Vec<ControllerPose>,
    /// Where the slider was last set, to show it there again.
    #[serde(default)]
    pub value: f32,
}

/// The driven layers' transforms and drawings at one point on the slider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerPose {
    pub name: String,
    /// Where on the slider, from 0 to 1.
    pub at: f32,
    /// One per driven layer, in the controller's order.
    pub transforms: Vec<Transform>,
    pub drawings: Vec<usize>,
}

impl Document {
    /// Adds a controller over `layers` and returns its index.
    pub fn add_controller(&mut self, name: impl Into<String>, layers: &[LayerId]) -> usize {
        self.controllers.push(Controller {
            name: name.into(),
            layers: layers.to_vec(),
            poses: Vec::new(),
            value: 0.0,
        });
        self.controllers.len() - 1
    }

    /// Records the driven layers as they are on `frame` as the pose at
    /// `at` on the slider (replacing one already there).
    pub fn record_pose(&mut self, controller: usize, name: impl Into<String>, at: f32, frame: u32) {
        let Some(c) = self.controllers.get(controller) else { return };
        let at = at.clamp(0.0, 1.0);
        let layers: Vec<_> = c.layers.iter().filter_map(|&id| self.layer(id)).collect();
        let transforms = layers.iter().map(|l| l.track.sample(frame as f32)).collect();
        let drawings = layers.iter().map(|l| l.drawing_index_at(frame)).collect();
        let pose = ControllerPose { name: name.into(), at, transforms, drawings };
        let c = &mut self.controllers[controller];
        c.poses.retain(|p| (p.at - at).abs() > 1e-3);
        c.poses.push(pose);
        c.poses.sort_by(|a, b| a.at.total_cmp(&b.at));
        c.value = at;
    }

    /// Sets a controller's slider to `value`, posing its layers on `frame`
    /// between the recorded poses either side.
    pub fn set_controller(&mut self, controller: usize, value: f32, frame: u32) {
        let Some(c) = self.controllers.get_mut(controller) else { return };
        let value = value.clamp(0.0, 1.0);
        c.value = value;
        let Some((before, after, t)) = bracket(&c.poses, value) else { return };
        let (before, after) = (c.poses[before].clone(), c.poses[after].clone());
        let layers = c.layers.clone();
        let nearer = if t < 0.5 { &before } else { &after };
        for (i, id) in layers.into_iter().enumerate() {
            let (Some(a), Some(b)) = (before.transforms.get(i), after.transforms.get(i)) else { continue };
            let pose = blend(a, b, t);
            if let Some(layer) = self.layer_mut(id) {
                layer.track.set_pose(frame, pose);
                if let Some(&drawing) = nearer.drawings.get(i)
                    && layer.drawings.len() > 1
                    && layer.drawing_index_at(frame) != drawing
                {
                    layer.expose(frame, drawing);
                }
            }
        }
    }

    /// Forgets layers that no longer exist from every controller.
    pub fn tidy_controllers(&mut self) {
        let exists: Vec<LayerId> = self.all_layers().map(|l| l.id).collect();
        for c in &mut self.controllers {
            let keep: Vec<bool> = c.layers.iter().map(|id| exists.contains(id)).collect();
            let mut i = 0;
            c.layers.retain(|_| {
                i += 1;
                keep[i - 1]
            });
            for pose in &mut c.poses {
                let mut i = 0;
                pose.transforms.retain(|_| {
                    i += 1;
                    keep.get(i - 1).copied().unwrap_or(false)
                });
                let mut i = 0;
                pose.drawings.retain(|_| {
                    i += 1;
                    keep.get(i - 1).copied().unwrap_or(false)
                });
            }
        }
    }
}

/// The poses either side of `value` and how far between them it is.
fn bracket(poses: &[ControllerPose], value: f32) -> Option<(usize, usize, f32)> {
    let last = poses.len().checked_sub(1)?;
    if value <= poses[0].at {
        return Some((0, 0, 0.0));
    }
    if value >= poses[last].at {
        return Some((last, last, 0.0));
    }
    let after = poses.iter().position(|p| p.at > value)?;
    let before = after - 1;
    let span = (poses[after].at - poses[before].at).max(1e-6);
    Some((before, after, (value - poses[before].at) / span))
}

fn blend(a: &Transform, b: &Transform, t: f32) -> Transform {
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let mix2 = |p: Point, q: Point| p.lerp(q, t);
    Transform {
        position: mix2(a.position, b.position),
        rotation: mix(a.rotation, b.rotation),
        scale: mix2(a.scale, b.scale),
        skew: mix2(a.skew, b.skew),
        opacity: mix(a.opacity, b.opacity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Drawing;

    /// An arm and a mouth with two drawings, under a controller with a
    /// "rest" pose at 0 and a "wave" pose at 1.
    fn rigged() -> (Document, LayerId, LayerId) {
        let mut doc = Document::default();
        let arm = doc.add_layer("Arm", Vec::new(), 0, Transform::default());
        let mouth = doc.add_layer("Mouth", Vec::new(), 0, Transform::default());
        doc.layer_mut(mouth).unwrap().add_drawing(Drawing::new("smile", Vec::new()));
        let c = doc.add_controller("Wave", &[arm, mouth]);
        doc.record_pose(c, "Rest", 0.0, 0);
        doc.layer_mut(arm).unwrap().track.set_pose(0, Transform { rotation: 90.0, ..Transform::default() });
        doc.layer_mut(mouth).unwrap().expose(0, 1);
        doc.record_pose(c, "Wave", 1.0, 0);
        (doc, arm, mouth)
    }

    #[test]
    fn the_slider_blends_poses_and_picks_the_nearer_drawing() {
        let (mut doc, arm, mouth) = rigged();
        doc.set_controller(0, 0.25, 10);
        let layer = doc.layer(arm).unwrap();
        assert!((layer.track.sample(10.0).rotation - 22.5).abs() < 1e-3);
        assert_eq!(doc.layer(mouth).unwrap().drawing_index_at(10), 0, "nearer rest");
        doc.set_controller(0, 0.75, 10);
        assert_eq!(doc.layer(mouth).unwrap().drawing_index_at(10), 1, "nearer the wave");
    }

    #[test]
    fn setting_it_keys_the_layers_so_poses_animate() {
        let (mut doc, arm, _) = rigged();
        doc.set_controller(0, 0.0, 10);
        doc.set_controller(0, 1.0, 20);
        let layer = doc.layer(arm).unwrap();
        assert!((layer.track.sample(15.0).rotation - 45.0).abs() < 1e-3, "tweens between the keys");
    }

    #[test]
    fn deleted_layers_drop_out_of_controllers() {
        let (mut doc, arm, mouth) = rigged();
        doc.remove_layer(arm);
        doc.tidy_controllers();
        let c = &doc.controllers[0];
        assert_eq!(c.layers, [mouth]);
        assert!(c.poses.iter().all(|p| p.transforms.len() == 1 && p.drawings.len() == 1));
    }
}
