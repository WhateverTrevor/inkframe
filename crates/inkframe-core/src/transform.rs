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
    /// Horizontal and vertical skew in degrees: slanting the layer, as with
    /// Animate's skew. Kept within ±89° when edited, since at 90° a layer
    /// slants into a line.
    pub skew: Point,
    /// 0.0 (invisible) to 1.0 (fully opaque).
    pub opacity: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Point::ZERO,
            rotation: 0.0,
            scale: Point::new(1.0, 1.0),
            skew: Point::ZERO,
            opacity: 1.0,
        }
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
            skew: self.skew.lerp(other.skew, t),
            opacity: mix(self.opacity, other.opacity),
        }
    }

    /// The matrix that maps the layer's local coordinates onto the stage.
    ///
    /// Order matters: scale first, then skew, then rotate, then move into
    /// place. Doing it the other way round would rotate around the stage
    /// origin instead of around the layer's own pivot, and skewing after
    /// rotating would slant along the stage's axes instead of the layer's.
    pub fn to_affine(&self) -> Affine {
        let skew = Point::new(self.skew.x.to_radians(), self.skew.y.to_radians());
        Affine::translate(self.position)
            .pre_concat(Affine::rotate(self.rotation.to_radians()))
            .pre_concat(Affine::skew(skew))
            .pre_concat(Affine::scale(self.scale.x, self.scale.y))
    }

    /// The pose whose matrix is `matrix`, keeping `self`'s opacity.
    ///
    /// Any 2D linear map is a rotation, then a horizontal skew, then a scale
    /// (the QR decomposition, in linear-algebra terms), so this always
    /// exists; the result has no vertical skew. That's what lets a group of
    /// layers be stretched exactly even when some are rotated: the stretched
    /// matrix is worked out, then turned back into a pose.
    pub fn from_affine(&self, matrix: Affine) -> Transform {
        // The first column is where the x axis goes: its angle is the
        // rotation and its length the horizontal scale.
        let rotation = matrix.b.atan2(matrix.a);
        let scale_x = matrix.a.hypot(matrix.b);
        // Undo the rotation on the second column; what's left is the
        // skew (across) and the vertical scale (down).
        let (sin, cos) = rotation.sin_cos();
        let across = cos * matrix.c + sin * matrix.d;
        let down = -sin * matrix.c + cos * matrix.d;
        let skew_x = if down.abs() > 1e-9 { (-across / down).atan() } else { 0.0 };
        Transform {
            position: Point::new(matrix.e, matrix.f),
            rotation: rotation.to_degrees(),
            scale: Point::new(scale_x, down),
            skew: Point::new(skew_x.to_degrees(), 0.0),
            opacity: self.opacity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn skew_slants_around_the_pivot() {
        let skewed = Transform { skew: Point::new(45.0, 0.0), ..Transform::default() }.to_affine();
        // The top of the layer moves right as far as it is above the pivot,
        // the bottom left; the pivot itself stays put.
        assert!(close(skewed.apply(Point::new(0.0, -10.0)), Point::new(10.0, -10.0)));
        assert!(close(skewed.apply(Point::new(0.0, 10.0)), Point::new(-10.0, 10.0)));
        assert!(close(skewed.apply(Point::new(10.0, 0.0)), Point::new(10.0, 0.0)));
    }

    #[test]
    fn poses_survive_a_trip_through_their_matrix() {
        let pose = Transform {
            position: Point::new(30.0, -20.0),
            rotation: 35.0,
            scale: Point::new(2.0, 0.5),
            skew: Point::new(20.0, 0.0),
            opacity: 0.7,
        };
        let back = pose.from_affine(pose.to_affine());
        assert!(close(back.position, pose.position) && (back.rotation - 35.0).abs() < 1e-3);
        assert!(close(back.scale, pose.scale) && close(back.skew, pose.skew));
        assert_eq!(back.opacity, 0.7);
    }

    #[test]
    fn any_matrix_becomes_a_pose_that_draws_the_same() {
        // A rotated layer stretched sideways on the stage: not expressible
        // with rotation and scale alone, but exact with skew.
        let stretch = Affine::scale(3.0, 1.0);
        let rotated = Transform { rotation: 30.0, ..Transform::default() };
        let matrix = stretch.pre_concat(rotated.to_affine());
        let pose = rotated.from_affine(matrix);
        for p in [Point::new(5.0, 0.0), Point::new(0.0, 5.0), Point::new(-3.0, 7.0)] {
            assert!(close(pose.to_affine().apply(p), matrix.apply(p)), "{p:?}");
        }
    }
}
