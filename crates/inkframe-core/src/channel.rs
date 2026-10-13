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
pub trait Animatable: Copy + PartialEq {
    /// Blends from `self` (t = 0) to `other` (t = 1). `t` has easing already
    /// applied and may leave 0–1 for overshooting curves.
    fn lerp(self, other: Self, t: f32) -> Self;
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
        Self { keys: vec![Key { frame, value, easing: Easing::default() }] }
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
            return Some(first.value);
        }
        if frame >= last.frame as f32 {
            return Some(last.value);
        }

        // `partition_point` is a binary search for the first key after
        // `frame`; the early returns guarantee it's somewhere in the middle.
        let next = self.keys.partition_point(|k| k.frame as f32 <= frame);
        let (a, b) = (&self.keys[next - 1], &self.keys[next]);
        let progress = (frame - a.frame as f32) / (b.frame - a.frame) as f32;
        Some(a.value.lerp(b.value, a.easing.apply(progress)))
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
                self.keys.insert(i, Key { frame, value, easing });
            }
        }
    }

    /// Rewrites every key's value, keeping frames and easing.
    pub fn map_values(&mut self, f: impl Fn(T) -> T) {
        for key in &mut self.keys {
            key.value = f(key.value);
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
        let key = |frame, value| Key { frame, value, easing: Easing::Linear };
        let channel = Channel::from_keys(vec![key(5, 1.0), key(1, 2.0), key(5, 9.0)]);
        assert_eq!(channel.key_frames(), [1, 5]);
        assert_eq!(channel.key_at(5).unwrap().value, 9.0);
    }

    #[test]
    fn points_tween_on_both_axes() {
        let mut channel = Channel::new(0, Point::new(0.0, 0.0));
        channel.set(4, Point::new(40.0, 80.0));
        assert_eq!(channel.sample(1.0), Some(Point::new(10.0, 20.0)));
    }
}
