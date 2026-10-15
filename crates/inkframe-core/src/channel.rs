//! Keyframes for a single animated property.
//!
//! A [`Channel`] is the timeline of one value, such as a layer's position or
//! its opacity. Each property having its own channel is what lets a layer
//! ease its movement while fading at a steady rate, or rotate on different
//! beats from the ones it moves on.

use serde::{Deserialize, Serialize};

use crate::easing::Easing;
use crate::geometry::Point;

/// A value that can be tweened: anything we can blend between two keyframes.
pub trait Animatable: Clone + PartialEq {
    /// Blends from `self` (t = 0) to `other` (t = 1). `t` has easing already
    /// applied and may leave 0–1 for overshooting curves.
    fn lerp(self, other: Self, t: f32) -> Self;

    /// The value `t` of the way through the tween from `from` to `to`.
    /// Usually a straight blend; positions override this to follow a curved
    /// motion path (see [`Bend`]).
    fn tween(from: &Key<Self>, to: &Key<Self>, t: f32) -> Self {
        from.value.clone().lerp(to.value.clone(), t)
    }

    /// Called when a key is inserted at `t` (by time) through the tween from
    /// `from` to `to`, to keep the tween's shape. Only positions have a
    /// shape to keep.
    fn split(_from: &mut Key<Self>, _to: &mut Key<Self>, _inserted: &mut Key<Self>, _t: f32) {}
}

impl Animatable for f32 {
    fn lerp(self, other: f32, t: f32) -> f32 {
        // Numbers, rotation included, are blended as plain values. For
        // rotation that means 0° → 720° spins twice rather than taking the
        // shortest way round, which matches Animate and After Effects.
        self + (other - self) * t
    }
}

impl Animatable for Point {
    fn lerp(self, other: Point, t: f32) -> Point {
        Point::lerp(self, other, t)
    }

    fn tween(from: &Key<Point>, to: &Key<Point>, t: f32) -> Point {
        match MotionCurve::between(from, to) {
            Some(curve) => curve.at_distance(t),
            None => from.value.lerp(to.value, t),
        }
    }

    fn split(from: &mut Key<Point>, to: &mut Key<Point>, inserted: &mut Key<Point>, t: f32) {
        let Some(curve) = MotionCurve::between(from, to) else { return };
        // Split the curve where the new key lands on it (de Casteljau's
        // construction), so the two halves trace exactly the old path.
        let s = curve.parameter_at_distance(t);
        let [p0, p1, p2, p3] = curve.points;
        let (a, b, c) = (p0.lerp(p1, s), p1.lerp(p2, s), p2.lerp(p3, s));
        let (d, e) = (a.lerp(b, s), b.lerp(c, s));
        let middle = d.lerp(e, s);
        from.bend.get_or_insert_default().out = a - p0;
        to.bend.get_or_insert_default().into = c - p3;
        inserted.value = middle;
        inserted.bend = Some(Bend { into: d - middle, out: e - middle });
    }
}

impl Channel<Point> {
    /// Sets the curve handles on the key at `frame`. Returns false if
    /// there's no key there.
    pub fn set_bend(&mut self, frame: u32, bend: Option<Bend>) -> bool {
        match self.index_of(frame) {
            Ok(i) => {
                // All-zero handles are the same as none; store them as none
                // so the file stays tidy.
                self.keys[i].bend = bend.filter(|b| *b != Bend::default());
                true
            }
            Err(_) => false,
        }
    }

    /// Straightens every tween: removes all curve handles.
    pub fn straighten(&mut self) {
        for key in &mut self.keys {
            key.bend = None;
        }
    }
}

/// Curve handles on a position key, which bend the motion path through it,
/// as in Animate's motion paths and After Effects' spatial keyframes.
///
/// Like the handles on a pen point: `into` shapes the path arriving at the
/// key and `out` the path leaving it. Both are offsets from the key's
/// position, so moving the key carries the curve's shape along with it. A
/// zero handle means the path leaves or arrives straight.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Bend {
    pub into: Point,
    pub out: Point,
}

/// The cubic Bézier a bent tween travels along.
pub struct MotionCurve {
    pub points: [Point; 4],
}

impl MotionCurve {
    /// The curve from `from` to `to`, or `None` if the tween runs straight
    /// (neither end has a handle on that side).
    pub fn between(from: &Key<Point>, to: &Key<Point>) -> Option<Self> {
        let out = from.bend.map_or(Point::ZERO, |b| b.out);
        let into = to.bend.map_or(Point::ZERO, |b| b.into);
        if out == Point::ZERO && into == Point::ZERO {
            return None;
        }
        // A zero handle on one end means "straight out of this end": put
        // that control point a third of the way along, which is what makes
        // a cubic with no bend at all an evenly paced straight line.
        let (start, end) = (from.value, to.value);
        let near = |at: Point, toward: Point, handle: Point| {
            if handle == Point::ZERO { at.lerp(toward, 1.0 / 3.0) } else { at + handle }
        };
        Some(Self { points: [start, near(start, end, out), near(end, start, into), end] })
    }

    /// The point at parameter `s` (0 at the start, 1 at the end). Equal
    /// steps of `s` aren't equal distances along the curve; see
    /// [`MotionCurve::at_distance`].
    pub fn at(&self, s: f32) -> Point {
        let [p0, p1, p2, p3] = self.points;
        crate::shape::cubic_point(p0, p1, p2, p3, s)
    }

    /// The point a fraction `t` of the way along the curve's length.
    ///
    /// A Bézier's own parameter speeds up and slows down along the curve
    /// (it bunches up where the curve bends tightly). Animators expect a
    /// linear tween to travel at an even speed, with easing the only thing
    /// changing it, so positions are spaced by distance instead.
    pub fn at_distance(&self, t: f32) -> Point {
        self.at(self.parameter_at_distance(t))
    }

    /// The parameter that's a fraction `t` of the way along the curve.
    ///
    /// Measured by adding up the lengths of short straight pieces, which is
    /// plenty accurate for placing a layer. Outside 0–1 (an overshooting
    /// ease), the parameter simply runs on past the ends.
    pub fn parameter_at_distance(&self, t: f32) -> f32 {
        if !(0.0..=1.0).contains(&t) {
            return t;
        }
        const STEPS: usize = 64;
        let mut lengths = [0.0f32; STEPS + 1];
        let mut previous = self.points[0];
        for (i, length) in lengths.iter_mut().enumerate().skip(1) {
            let point = self.at(i as f32 / STEPS as f32);
            *length = previous.distance(point);
            previous = point;
        }
        let total: f32 = lengths.iter().sum();
        if total <= f32::EPSILON {
            return t;
        }
        let target = t * total;
        let mut covered = 0.0;
        for (i, &length) in lengths.iter().enumerate().skip(1) {
            if covered + length >= target {
                let within = if length > 0.0 { (target - covered) / length } else { 0.0 };
                return ((i - 1) as f32 + within) / STEPS as f32;
            }
            covered += length;
        }
        1.0
    }
}

/// A value pinned to a frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Key<T> {
    /// Zero-based frame index. The UI shows frames starting at 1, like
    /// Animate, but everything under the hood counts from 0.
    pub frame: u32,
    pub value: T,
    /// How to travel from this key to the *next* one. The last key's easing
    /// has no effect, but we keep it so moving keys around never silently
    /// loses a setting.
    #[serde(default)]
    pub easing: Easing,
    /// For positions only: curve handles that bend the motion path through
    /// this key. `None` means straight lines either side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend: Option<Bend>,
}

/// One property's keys, always sorted by frame with at most one key per
/// frame, and never empty once constructed.
///
/// The field is private so those rules can't be broken from outside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Channel<T> {
    keys: Vec<Key<T>>,
}

impl<T: Animatable> Channel<T> {
    /// A channel holding `value` still, from a single key.
    pub fn new(frame: u32, value: T) -> Self {
        Self { keys: vec![Key { frame, value, easing: Easing::default(), bend: None }] }
    }

    pub(crate) fn from_keys(keys: Vec<Key<T>>) -> Self {
        let mut channel = Self { keys };
        channel.normalize();
        channel
    }

    pub fn keys(&self) -> &[Key<T>] {
        &self.keys
    }

    pub fn key_at(&self, frame: u32) -> Option<&Key<T>> {
        self.index_of(frame).ok().map(|i| &self.keys[i])
    }

    /// The value at any point in time, tweening between keys.
    ///
    /// `frame` is fractional so callers can sample between frames (useful for
    /// motion blur later on). Before the first key and after the last one,
    /// the value holds steady.
    pub fn sample(&self, frame: f32) -> Option<T> {
        let (first, last) = (self.keys.first()?, self.keys.last()?);
        if frame <= first.frame as f32 {
            return Some(first.value.clone());
        }
        if frame >= last.frame as f32 {
            return Some(last.value.clone());
        }

        // `partition_point` is a binary search for the first key after
        // `frame`; the early returns guarantee it's somewhere in the middle.
        let next = self.keys.partition_point(|k| k.frame as f32 <= frame);
        let (a, b) = (&self.keys[next - 1], &self.keys[next]);
        let progress = (frame - a.frame as f32) / (b.frame - a.frame) as f32;
        Some(T::tween(a, b, a.easing.apply(progress)))
    }

    /// Writes `value` at `frame`, creating a key if there isn't one.
    ///
    /// A new key inherits the easing of the key before it, so splitting an
    /// eased tween in two keeps both halves eased.
    pub fn set(&mut self, frame: u32, value: T) {
        match self.index_of(frame) {
            Ok(i) => self.keys[i].value = value,
            Err(i) => {
                let easing = i.checked_sub(1).map_or(Easing::default(), |p| self.keys[p].easing);
                self.keys.insert(i, Key { frame, value, easing, bend: None });
            }
        }
    }

    /// The key at `frame`, or if there isn't one, a key holding the value
    /// there with this channel's default easing: what a "copy keyframe"
    /// should pick up.
    pub fn key_or_sample(&self, frame: u32) -> Option<Key<T>> {
        if let Some(key) = self.key_at(frame) {
            return Some(key.clone());
        }
        let value = self.sample(frame as f32)?;
        Some(Key { frame, value, easing: Easing::default(), bend: None })
    }

    /// Puts a whole key (value, easing and curve) at `key.frame`, replacing
    /// any key already there.
    pub fn put(&mut self, key: Key<T>) {
        match self.index_of(key.frame) {
            Ok(i) => self.keys[i] = key,
            Err(i) => self.keys.insert(i, key),
        }
    }

    /// Rewrites every key's value, keeping frames and easing.
    pub fn map_values(&mut self, f: impl Fn(T) -> T) {
        for key in &mut self.keys {
            key.value = f(key.value.clone());
        }
    }

    fn normalize(&mut self) {
        // Sort, then keep the *last* of any keys sharing a frame, since in a
        // hand-edited file the later entry is most likely the intended one.
        self.keys.sort_by_key(|k| k.frame);
        self.keys.reverse();
        self.keys.dedup_by_key(|k| k.frame);
        self.keys.reverse();
    }

    fn index_of(&self, frame: u32) -> Result<usize, usize> {
        self.keys.binary_search_by_key(&frame, |k| k.frame)
    }

    fn tween_start_index(&self, frame: u32) -> Option<usize> {
        let i = self.keys.partition_point(|k| k.frame <= frame).checked_sub(1)?;
        (i + 1 < self.keys.len()).then_some(i)
    }
}

/// A tween: the stretch between two consecutive keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    pub start: u32,
    pub end: u32,
    pub easing: Easing,
}

/// The operations that don't depend on what kind of value a channel holds.
///
/// The timeline and inspector treat every property the same way (draw its
/// keys, drag them, change their easing). This trait lets them do that
/// through `&dyn AnyChannel` without caring whether it's a position or an
/// opacity underneath.
pub trait AnyChannel {
    fn key_frames(&self) -> Vec<u32>;
    fn has_key(&self, frame: u32) -> bool;
    /// Adds a key at `frame` that freezes the current tweened value, so the
    /// animation looks the same until the user changes it. Returns `false`
    /// if a key was already there.
    fn insert_key(&mut self, frame: u32) -> bool;
    /// Removes the key at `frame`. The last key can't be removed, since a
    /// channel always needs a value, so this returns `false` in that case.
    fn remove_key(&mut self, frame: u32) -> bool;
    /// Moves the key at `from` to `to`. Fails if there's no key at `from`
    /// or another key already occupies `to`.
    fn move_key(&mut self, from: u32, to: u32) -> bool;
    fn tweens(&self) -> Vec<Tween>;
    /// The tween covering `frame`, if any.
    fn tween_at(&self, frame: u32) -> Option<Tween>;
    /// The easing of the tween covering `frame`, for editing.
    fn tween_easing_mut(&mut self, frame: u32) -> Option<&mut Easing>;
}

impl<T: Animatable> AnyChannel for Channel<T> {
    fn key_frames(&self) -> Vec<u32> {
        self.keys.iter().map(|k| k.frame).collect()
    }

    fn has_key(&self, frame: u32) -> bool {
        self.index_of(frame).is_ok()
    }

    fn insert_key(&mut self, frame: u32) -> bool {
        if self.has_key(frame) {
            return false;
        }
        let Some(value) = self.sample(frame as f32) else {
            return false;
        };
        self.set(frame, value);
        // A key added partway through a tween splits it in two; give the
        // value a chance to keep the tween's shape (a curved path, say).
        if let Ok(i) = self.index_of(frame)
            && i > 0
            && i + 1 < self.keys.len()
        {
            let (before, rest) = self.keys.split_at_mut(i);
            let (inserted, after) = rest.split_at_mut(1);
            let (from, to) = (&mut before[i - 1], &mut after[0]);
            let t = (frame - from.frame) as f32 / (to.frame - from.frame) as f32;
            T::split(from, to, &mut inserted[0], from.easing.apply(t));
        }
        true
    }

    fn remove_key(&mut self, frame: u32) -> bool {
        match self.index_of(frame) {
            Ok(i) if self.keys.len() > 1 => {
                self.keys.remove(i);
                true
            }
            _ => false,
        }
    }

    fn move_key(&mut self, from: u32, to: u32) -> bool {
        if from == to {
            return self.has_key(from);
        }
        if self.has_key(to) {
            return false;
        }
        let Ok(i) = self.index_of(from) else {
            return false;
        };
        let mut key = self.keys.remove(i);
        key.frame = to;
        let j = self.index_of(to).unwrap_err();
        self.keys.insert(j, key);
        true
    }

    fn tweens(&self) -> Vec<Tween> {
        self.keys
            .windows(2)
            .map(|w| Tween { start: w[0].frame, end: w[1].frame, easing: w[0].easing })
            .collect()
    }

    fn tween_at(&self, frame: u32) -> Option<Tween> {
        let i = self.tween_start_index(frame)?;
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        Some(Tween { start: a.frame, end: b.frame, easing: a.easing })
    }

    fn tween_easing_mut(&mut self, frame: u32) -> Option<&mut Easing> {
        let i = self.tween_start_index(frame)?;
        Some(&mut self.keys[i].easing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_keys() -> Channel<f32> {
        let mut channel = Channel::new(0, 0.0);
        channel.set(10, 100.0);
        channel
    }

    #[test]
    fn holds_before_first_and_after_last_key() {
        let channel = two_keys();
        assert_eq!(channel.sample(-5.0), Some(0.0));
        assert_eq!(channel.sample(50.0), Some(100.0));
    }

    #[test]
    fn linear_tween_is_proportional() {
        assert!((two_keys().sample(2.5).unwrap() - 25.0).abs() < 1e-4);
    }

    #[test]
    fn hold_jumps_on_the_next_key() {
        let mut channel = two_keys();
        *channel.tween_easing_mut(0).unwrap() = Easing::Hold;
        assert_eq!(channel.sample(9.0), Some(0.0));
        assert_eq!(channel.sample(10.0), Some(100.0));
    }

    #[test]
    fn set_keeps_keys_sorted() {
        let mut channel = two_keys();
        channel.set(5, 10.0);
        assert_eq!(channel.key_frames(), [0, 5, 10]);
    }

    #[test]
    fn insert_key_preserves_the_value() {
        let mut channel = two_keys();
        let before = channel.sample(4.0);
        assert!(channel.insert_key(4));
        assert_eq!(channel.key_at(4).map(|k| k.value), before);
        assert!(!channel.insert_key(4));
    }

    #[test]
    fn new_keys_inherit_easing_from_the_previous_key() {
        let mut channel = two_keys();
        *channel.tween_easing_mut(0).unwrap() = Easing::EaseIn;
        channel.set(5, 10.0);
        assert_eq!(channel.key_at(5).unwrap().easing, Easing::EaseIn);
    }

    #[test]
    fn tween_at_finds_the_covering_tween() {
        let channel = two_keys();
        assert_eq!(channel.tween_at(9).map(|t| (t.start, t.end)), Some((0, 10)));
        // At or past the last key there's no tween left to ease.
        assert!(channel.tween_at(10).is_none());
    }

    #[test]
    fn cannot_remove_the_last_key() {
        let mut channel = Channel::new(0, 1.0);
        assert!(!channel.remove_key(0));
        assert_eq!(channel.keys().len(), 1);
    }

    #[test]
    fn move_key_refuses_to_overwrite() {
        let mut channel = two_keys();
        assert!(!channel.move_key(0, 10));
        assert!(channel.move_key(10, 20));
        assert_eq!(channel.key_frames(), [0, 20]);
    }

    #[test]
    fn from_keys_sorts_and_keeps_the_last_duplicate() {
        let key = |frame, value| Key { frame, value, easing: Easing::Linear, bend: None };
        let channel = Channel::from_keys(vec![key(5, 1.0), key(1, 2.0), key(5, 9.0)]);
        assert_eq!(channel.key_frames(), [1, 5]);
        assert_eq!(channel.key_at(5).unwrap().value, 9.0);
    }

    /// A tween from (0, 0) at frame 0 to (100, 0) at frame 10.
    fn straight_path() -> Channel<Point> {
        let mut channel = Channel::new(0, Point::ZERO);
        channel.set(10, Point::new(100.0, 0.0));
        channel
    }

    #[test]
    fn unbent_paths_are_straight_lines() {
        assert_eq!(straight_path().sample(5.0), Some(Point::new(50.0, 0.0)));
    }

    #[test]
    fn bent_paths_curve_through_their_handles_side() {
        // Both handles pull upward (negative y): the path arcs up.
        let mut channel = straight_path();
        channel.keys[0].bend = Some(Bend { into: Point::ZERO, out: Point::new(30.0, -60.0) });
        channel.keys[1].bend = Some(Bend { into: Point::new(-30.0, -60.0), out: Point::ZERO });
        let middle = channel.sample(5.0).unwrap();
        assert!((middle.x - 50.0).abs() < 1e-3, "symmetric, so halfway is in the middle");
        assert!(middle.y < -30.0, "the arc rises: {middle:?}");
        // The ends are still exactly on the keys.
        assert_eq!(channel.sample(0.0), Some(Point::ZERO));
        assert_eq!(channel.sample(10.0), Some(Point::new(100.0, 0.0)));
    }

    #[test]
    fn bent_paths_are_travelled_at_an_even_speed() {
        // A lopsided curve: one handle long, the other none. By the curve's
        // own parameter the layer would rush along one end; by distance,
        // every frame covers about the same ground.
        let mut channel = straight_path();
        channel.keys[0].bend = Some(Bend { into: Point::ZERO, out: Point::new(0.0, -150.0) });
        let points: Vec<Point> = (0..=10).map(|f| channel.sample(f as f32).unwrap()).collect();
        let steps: Vec<f32> = points.windows(2).map(|w| w[0].distance(w[1])).collect();
        let (shortest, longest) = steps.iter().fold((f32::MAX, 0.0f32), |(s, l), d| (s.min(*d), l.max(*d)));
        assert!(longest / shortest < 1.1, "steps should be even: {steps:?}");
    }

    #[test]
    fn inserting_a_key_keeps_a_curved_path_in_place() {
        let mut channel = straight_path();
        channel.keys[0].bend = Some(Bend { into: Point::ZERO, out: Point::new(20.0, -80.0) });
        channel.keys[1].bend = Some(Bend { into: Point::new(-40.0, -40.0), out: Point::ZERO });
        let before: Vec<Point> = (0..=10).map(|f| channel.sample(f as f32).unwrap()).collect();
        assert!(channel.insert_key(4));
        let after: Vec<Point> = (0..=10).map(|f| channel.sample(f as f32).unwrap()).collect();
        for (frame, (a, b)) in before.iter().zip(&after).enumerate() {
            // The halves are re-spaced by their own lengths, so frames may
            // shift slightly along the path, but the path itself is unchanged.
            assert!(a.distance(*b) < 2.0, "frame {frame}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn points_tween_on_both_axes() {
        let mut channel = Channel::new(0, Point::new(0.0, 0.0));
        channel.set(4, Point::new(40.0, 80.0));
        assert_eq!(channel.sample(1.0), Some(Point::new(10.0, 20.0)));
    }
}
