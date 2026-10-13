use serde::{Deserialize, Serialize};

use crate::geometry::{Affine, Point};

/// The animatable properties of a layer at one moment in time.
///
/// This is what a keyframe stores and what tweening interpolates. Shapes are
/// drawn around the layer's local origin, which therefore doubles as the
/// pivot point for rotation and scaling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    /// Where the layer's origin sits on the stage, in pixels.
    pub position: Point,
    /// Rotation in degrees, clockwise. Values beyond ±360 are meaningful:
    /// tweening from 0 to 720 spins the layer twice.
    pub rotation: f32,
    /// Horizontal and vertical scale, where 1.0 is the original size.
    pub scale: Point,
    /// 0.0 (invisible) to 1.0 (fully opaque).
    pub opacity: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { position: Point::ZERO, rotation: 0.0, scale: Point::new(1.0, 1.0), opacity: 1.0 }
    }
}

impl Transform {
    pub fn at(position: Point) -> Self {
        Self { position, ..Self::default() }
    }

    /// Linear blend between two transforms. `t` is expected to already have
    /// easing applied; this function only does the straight-line part.
    pub fn lerp(&self, other: &Transform, t: f32) -> Transform {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Transform {
            position: self.position.lerp(other.position, t),
            // Rotation is interpolated as a plain number rather than taking the
            // shortest path around the circle. That matches Animate and After
            // Effects, and it's the only way to express "spin three times".
            rotation: mix(self.rotation, other.rotation),
            scale: self.scale.lerp(other.scale, t),
            opacity: mix(self.opacity, other.opacity),
        }
    }

    /// The matrix that maps the layer's local coordinates onto the stage.
    ///
    /// Order matters: scale first, then rotate, then move into place. Doing
    /// it the other way round would rotate around the stage origin instead of
    /// around the layer's own pivot.
    pub fn to_affine(&self) -> Affine {
        Affine::translate(self.position)
            .pre_concat(Affine::rotate(self.rotation.to_radians()))
            .pre_concat(Affine::scale(self.scale.x, self.scale.y))
    }
}
