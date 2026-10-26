use serde::{Deserialize, Serialize};

/// How a value accelerates between two keyframes.
///
/// An easing function maps linear progress `t` (0 → 1, the fraction of time
/// elapsed between two keyframes) to eased progress, which is then used to
/// blend the two keyframes' values.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// No tween: the value jumps to the next keyframe when its frame arrives.
    /// This is what traditional frame-by-frame animators call a "hold".
    Hold,
    /// A CSS-style `cubic-bezier(x1, y1, x2, y2)` timing curve. The curve
    /// always starts at (0, 0) and ends at (1, 1); the two control points
    /// shape it. The y values may go outside 0–1 to overshoot or anticipate.
    CubicBezier {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
}

impl Easing {
    /// The same curve run backwards, for a tween that now plays the other
    /// way: easing in becomes easing out. A hold stays a hold.
    pub fn reversed(self) -> Easing {
        match self {
            Easing::EaseIn => Easing::EaseOut,
            Easing::EaseOut => Easing::EaseIn,
            Easing::CubicBezier { x1, y1, x2, y2 } => {
                Easing::CubicBezier { x1: 1.0 - x2, y1: 1.0 - y2, x2: 1.0 - x1, y2: 1.0 - y1 }
            }
            other => other,
        }
    }

    /// The named presets, in the order they're shown in the UI.
    pub const PRESETS: [Easing; 5] =
        [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut, Easing::Hold];

    pub fn label(&self) -> &'static str {
        match self {
            Easing::Linear => crate::i18n::tr_later("Linear"),
            Easing::EaseIn => crate::i18n::tr_later("Ease In"),
            Easing::EaseOut => crate::i18n::tr_later("Ease Out"),
            Easing::EaseInOut => crate::i18n::tr_later("Ease In-Out"),
            Easing::Hold => crate::i18n::tr_later("Hold"),
            Easing::CubicBezier { .. } => crate::i18n::tr_later("Custom"),
        }
    }

    /// The Bézier control points that describe this curve, or `None` for
    /// `Hold`, which can't be drawn as a smooth curve.
    ///
    /// The preset values are the same as CSS's `ease-in`, `ease-out` and
    /// `ease-in-out`, so they'll feel familiar to anyone who has done web
    /// animation.
    pub fn control_points(&self) -> Option<[f32; 4]> {
        match *self {
            Easing::Linear => Some([0.0, 0.0, 1.0, 1.0]),
            Easing::EaseIn => Some([0.42, 0.0, 1.0, 1.0]),
            Easing::EaseOut => Some([0.0, 0.0, 0.58, 1.0]),
            Easing::EaseInOut => Some([0.42, 0.0, 0.58, 1.0]),
            Easing::Hold => None,
            Easing::CubicBezier { x1, y1, x2, y2 } => Some([x1, y1, x2, y2]),
        }
    }

    /// Maps linear progress `t` to eased progress.
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            // Stay on the starting value for the whole span. The track takes
            // care of switching to the next keyframe exactly on its frame.
            Easing::Hold => 0.0,
            _ => {
                let [x1, y1, x2, y2] = self.control_points().expect("non-hold easing has a curve");
                cubic_bezier(x1, y1, x2, y2, t)
            }
        }
    }
}

/// Evaluates a CSS-style timing curve at horizontal position `x`.
///
/// A cubic Bézier is defined parametrically — both x and y are functions of a
/// hidden parameter `s` — so we can't just plug in `x`. Instead we solve
/// `bezier_x(s) = x` for `s`, then return `bezier_y(s)`.
///
/// Clamping the control points' x values to 0–1 keeps `bezier_x` monotonic,
/// which guarantees exactly one solution. Newton's method finds it in a few
/// steps for almost every curve; bisection is the slow-but-certain fallback
/// for the rare flat spots where Newton's derivative is near zero.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);

    // One axis of a cubic Bézier whose endpoints are fixed at 0 and 1.
    // Expanded form: 3(1-s)²s·p1 + 3(1-s)s²·p2 + s³
    let curve = |p1: f32, p2: f32, s: f32| {
        let inv = 1.0 - s;
        3.0 * inv * inv * s * p1 + 3.0 * inv * s * s * p2 + s * s * s
    };
    let slope = |p1: f32, p2: f32, s: f32| {
        let inv = 1.0 - s;
        3.0 * inv * inv * p1 + 6.0 * inv * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
    };

    const EPSILON: f32 = 1e-6;

    let mut s = x;
    for _ in 0..8 {
        let error = curve(x1, x2, s) - x;
        if error.abs() < EPSILON {
            return curve(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < EPSILON {
            break;
        }
        s -= error / d;
    }

    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    s = x;
    for _ in 0..32 {
        let value = curve(x1, x2, s);
        if (value - x).abs() < EPSILON {
            break;
        }
        if value < x {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    curve(y1, y2, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_hits_its_endpoints() {
        for easing in [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            assert!(easing.apply(0.0).abs() < 1e-4, "{easing:?} at 0");
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-4, "{easing:?} at 1");
        }
    }

    #[test]
    fn ease_in_starts_slow_and_ease_out_starts_fast() {
        assert!(Easing::EaseIn.apply(0.25) < 0.25);
        assert!(Easing::EaseOut.apply(0.25) > 0.25);
    }

    #[test]
    fn ease_in_out_is_symmetric() {
        let a = Easing::EaseInOut.apply(0.3);
        let b = Easing::EaseInOut.apply(0.7);
        assert!((a + b - 1.0).abs() < 1e-3);
    }

    #[test]
    fn linear_bezier_is_identity() {
        let easing = Easing::CubicBezier { x1: 0.0, y1: 0.0, x2: 1.0, y2: 1.0 };
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            assert!((easing.apply(t) - t).abs() < 1e-3);
        }
    }

    #[test]
    fn overshoot_curves_can_leave_unit_range() {
        // A "back out" curve should overshoot past 1.0 before settling.
        let easing = Easing::CubicBezier { x1: 0.34, y1: 1.56, x2: 0.64, y2: 1.0 };
        assert!((0..100).map(|i| easing.apply(i as f32 / 100.0)).any(|v| v > 1.0));
    }

    #[test]
    fn hold_never_moves() {
        assert_eq!(Easing::Hold.apply(0.99), 0.0);
    }
}
