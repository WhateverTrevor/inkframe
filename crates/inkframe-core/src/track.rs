use serde::{Deserialize, Serialize};

use crate::easing::Easing;
use crate::transform::Transform;

/// A snapshot of a layer's transform pinned to a specific frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    /// Zero-based frame index. The UI shows frames starting at 1, like Animate,
    /// but everything under the hood counts from 0.
    pub frame: u32,
    pub transform: Transform,
    /// How to travel from this keyframe to the *next* one. The last keyframe's
    /// easing has no effect, but we keep it so moving keys around never
    /// silently loses a setting.
    #[serde(default)]
    pub easing: Easing,
}

/// The keyframes that animate one layer, always kept sorted by frame with at
/// most one keyframe per frame.
///
/// Fields are private so those invariants can't be broken from outside; use
/// the methods to read and edit.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Track {
    keys: Vec<Keyframe>,
}

impl Track {
    /// A track with a single keyframe, which holds the layer still.
    pub fn new(frame: u32, transform: Transform) -> Self {
        Self { keys: vec![Keyframe { frame, transform, easing: Easing::default() }] }
    }

    pub fn keys(&self) -> &[Keyframe] {
        &self.keys
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn key_at(&self, frame: u32) -> Option<&Keyframe> {
        self.index_of(frame).ok().map(|i| &self.keys[i])
    }

    pub fn key_at_mut(&mut self, frame: u32) -> Option<&mut Keyframe> {
        self.index_of(frame).ok().map(|i| &mut self.keys[i])
    }

    /// The nearest keyframe strictly before `frame`.
    pub fn previous_key(&self, frame: u32) -> Option<&Keyframe> {
        self.keys.iter().rev().find(|k| k.frame < frame)
    }

    /// The nearest keyframe strictly after `frame`.
    pub fn next_key(&self, frame: u32) -> Option<&Keyframe> {
        self.keys.iter().find(|k| k.frame > frame)
    }

    /// The keyframe that starts the tween covering `frame`: the last key at or
    /// before `frame`, provided another key follows it. Its easing controls
    /// how that stretch of the animation moves.
    pub fn tween_start(&self, frame: u32) -> Option<&Keyframe> {
        let i = self.tween_start_index(frame)?;
        Some(&self.keys[i])
    }

    pub fn tween_start_mut(&mut self, frame: u32) -> Option<&mut Keyframe> {
        let i = self.tween_start_index(frame)?;
        Some(&mut self.keys[i])
    }

    fn tween_start_index(&self, frame: u32) -> Option<usize> {
        let i = self.keys.partition_point(|k| k.frame <= frame).checked_sub(1)?;
        (i + 1 < self.keys.len()).then_some(i)
    }

    /// Computes the transform at any point in time, tweening between keys.
    ///
    /// `frame` is fractional so callers can sample between frames (useful for
    /// motion blur or sub-frame scrubbing later on). Before the first key and
    /// after the last one, the layer simply holds that key's pose.
    pub fn sample(&self, frame: f32) -> Transform {
        let (first, last) = match (self.keys.first(), self.keys.last()) {
            (Some(first), Some(last)) => (first, last),
            _ => return Transform::default(),
        };
        if frame <= first.frame as f32 {
            return first.transform;
        }
        if frame >= last.frame as f32 {
            return last.transform;
        }

        // Find the pair of keys that bracket `frame`. `partition_point` is a
        // binary search returning the index of the first key after `frame`;
        // the early returns above guarantee it's somewhere in the middle.
        let next = self.keys.partition_point(|k| k.frame as f32 <= frame);
        let a = &self.keys[next - 1];
        let b = &self.keys[next];

        let span = (b.frame - a.frame) as f32;
        let progress = (frame - a.frame as f32) / span;
        a.transform.lerp(&b.transform, a.easing.apply(progress))
    }

    /// Writes `transform` at `frame`, creating a keyframe if there isn't one.
    ///
    /// A new keyframe inherits the easing of the key before it, so splitting
    /// an eased tween in two keeps both halves eased.
    pub fn set(&mut self, frame: u32, transform: Transform) {
        match self.index_of(frame) {
            Ok(i) => self.keys[i].transform = transform,
            Err(i) => {
                let easing = i.checked_sub(1).map_or(Easing::default(), |p| self.keys[p].easing);
                self.keys.insert(i, Keyframe { frame, transform, easing });
            }
        }
    }

    /// Adds a keyframe at `frame` that freezes the current tweened pose, so
    /// the animation looks exactly the same until the user changes it.
    /// Returns `false` if a keyframe was already there.
    pub fn insert_key(&mut self, frame: u32) -> bool {
        if self.key_at(frame).is_some() {
            return false;
        }
        let pose = self.sample(frame as f32);
        self.set(frame, pose);
        true
    }

    /// Removes the keyframe at `frame`.
    ///
    /// The last remaining keyframe can't be removed — a layer always needs at
    /// least one pose — so this returns `None` in that case too.
    pub fn remove_key(&mut self, frame: u32) -> Option<Keyframe> {
        if self.keys.len() <= 1 {
            return None;
        }
        let i = self.index_of(frame).ok()?;
        Some(self.keys.remove(i))
    }

    /// Moves a keyframe to another frame. Fails (returns `false`) if there's
    /// no key at `from` or another key already occupies `to`.
    pub fn move_key(&mut self, from: u32, to: u32) -> bool {
        if from == to {
            return self.key_at(from).is_some();
        }
        if self.key_at(to).is_some() {
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

    /// Restores the sorted/unique invariant on data loaded from disk, which
    /// may have been edited by hand. Later duplicates win.
    pub(crate) fn normalize(&mut self) {
        self.keys.sort_by_key(|k| k.frame);
        self.keys.reverse();
        self.keys.dedup_by_key(|k| k.frame);
        self.keys.reverse();
    }

    fn index_of(&self, frame: u32) -> Result<usize, usize> {
        self.keys.binary_search_by_key(&frame, |k| k.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn at_x(x: f32) -> Transform {
        Transform::at(Point::new(x, 0.0))
    }

    fn two_key_track() -> Track {
        let mut track = Track::new(0, at_x(0.0));
        track.set(10, at_x(100.0));
        track
    }

    #[test]
    fn holds_before_first_and_after_last_key() {
        let track = two_key_track();
        assert_eq!(track.sample(-5.0).position.x, 0.0);
        assert_eq!(track.sample(50.0).position.x, 100.0);
    }

    #[test]
    fn linear_tween_is_proportional() {
        let track = two_key_track();
        assert!((track.sample(2.5).position.x - 25.0).abs() < 1e-4);
    }

    #[test]
    fn hold_jumps_on_the_next_keyframe() {
        let mut track = two_key_track();
        track.key_at_mut(0).unwrap().easing = Easing::Hold;
        assert_eq!(track.sample(9.0).position.x, 0.0);
        assert_eq!(track.sample(10.0).position.x, 100.0);
    }

    #[test]
    fn set_keeps_keys_sorted() {
        let mut track = two_key_track();
        track.set(5, at_x(10.0));
        let frames: Vec<u32> = track.keys().iter().map(|k| k.frame).collect();
        assert_eq!(frames, [0, 5, 10]);
    }

    #[test]
    fn insert_key_preserves_the_pose() {
        let mut track = two_key_track();
        let before = track.sample(4.0);
        assert!(track.insert_key(4));
        assert_eq!(track.key_at(4).unwrap().transform, before);
        assert!(!track.insert_key(4));
    }

    #[test]
    fn new_keys_inherit_easing_from_the_previous_key() {
        let mut track = two_key_track();
        track.key_at_mut(0).unwrap().easing = Easing::EaseIn;
        track.set(5, at_x(10.0));
        assert_eq!(track.key_at(5).unwrap().easing, Easing::EaseIn);
    }

    #[test]
    fn tween_start_finds_the_covering_key() {
        let track = two_key_track();
        assert_eq!(track.tween_start(0).unwrap().frame, 0);
        assert_eq!(track.tween_start(9).unwrap().frame, 0);
        // At or past the last key there's no tween left to ease.
        assert!(track.tween_start(10).is_none());
    }

    #[test]
    fn cannot_remove_the_last_key() {
        let mut track = Track::new(0, at_x(0.0));
        assert!(track.remove_key(0).is_none());
        assert_eq!(track.keys().len(), 1);
    }

    #[test]
    fn move_key_refuses_to_overwrite() {
        let mut track = two_key_track();
        assert!(!track.move_key(0, 10));
        assert!(track.move_key(10, 20));
        assert_eq!(track.keys()[1].frame, 20);
    }

    #[test]
    fn normalize_sorts_and_dedups() {
        let mut track: Track = serde_json::from_str(
            r#"[{"frame":5,"transform":{}},{"frame":1,"transform":{}},{"frame":5,"transform":{"rotation":9}}]"#,
        )
        .unwrap();
        track.normalize();
        let frames: Vec<u32> = track.keys().iter().map(|k| k.frame).collect();
        assert_eq!(frames, [1, 5]);
        assert_eq!(track.key_at(5).unwrap().transform.rotation, 9.0);
    }
}
