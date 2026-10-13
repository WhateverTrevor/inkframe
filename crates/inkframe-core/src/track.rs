//! A layer's animation: one [`Channel`] per animatable property.

use serde::{Deserialize, Serialize};

use crate::channel::{AnyChannel, Channel, Key};
use crate::easing::Easing;
use crate::geometry::Point;
use crate::transform::Transform;

/// The properties a layer can animate, in the order they're listed in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Property {
    Position,
    Rotation,
    Scale,
    Opacity,
}

impl Property {
    pub const ALL: [Property; 4] =
        [Property::Position, Property::Rotation, Property::Scale, Property::Opacity];

    pub fn name(self) -> &'static str {
        match self {
            Property::Position => "Position",
            Property::Rotation => "Rotation",
            Property::Scale => "Scale",
            Property::Opacity => "Opacity",
        }
    }
}

/// Everything that animates a layer.
///
/// Each property has its own keys and easing. Editing the layer on a frame
/// only adds keys for the properties that actually changed (see
/// [`Track::set_pose`]), so dragging a shape keys its position without
/// pinning its rotation or opacity in place.
///
/// Files from format version 1, which stored whole-transform keyframes, are
/// converted on load; see [`TrackFormat`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "TrackFormat")]
pub struct Track {
    pub position: Channel<Point>,
    pub rotation: Channel<f32>,
    pub scale: Channel<Point>,
    pub opacity: Channel<f32>,
}

impl Track {
    /// A track holding `pose` still, with one key per property at `frame`.
    pub fn new(frame: u32, pose: Transform) -> Self {
        Self {
            position: Channel::new(frame, pose.position),
            rotation: Channel::new(frame, pose.rotation),
            scale: Channel::new(frame, pose.scale),
            opacity: Channel::new(frame, pose.opacity),
        }
    }

    pub fn channel(&self, property: Property) -> &dyn AnyChannel {
        match property {
            Property::Position => &self.position,
            Property::Rotation => &self.rotation,
            Property::Scale => &self.scale,
            Property::Opacity => &self.opacity,
        }
    }

    pub fn channel_mut(&mut self, property: Property) -> &mut dyn AnyChannel {
        match property {
            Property::Position => &mut self.position,
            Property::Rotation => &mut self.rotation,
            Property::Scale => &mut self.scale,
            Property::Opacity => &mut self.opacity,
        }
    }

    fn channels_mut(&mut self) -> [&mut dyn AnyChannel; 4] {
        [&mut self.position, &mut self.rotation, &mut self.scale, &mut self.opacity]
    }

    fn channels(&self) -> [&dyn AnyChannel; 4] {
        Property::ALL.map(|p| self.channel(p))
    }

    /// The full transform at any point in time.
    pub fn sample(&self, frame: f32) -> Transform {
        // Channels are never empty, but fall back to the identity pose
        // rather than panicking if that were ever violated.
        let identity = Transform::default();
        Transform {
            position: self.position.sample(frame).unwrap_or(identity.position),
            rotation: self.rotation.sample(frame).unwrap_or(identity.rotation),
            scale: self.scale.sample(frame).unwrap_or(identity.scale),
            opacity: self.opacity.sample(frame).unwrap_or(identity.opacity),
        }
    }

    /// Makes the layer have `pose` at `frame`: "auto-keying".
    ///
    /// A property gets a key here if its value changed, or if it already
    /// had one (which is then updated). Properties that didn't change are
    /// left alone. That's what keeps per-property animation clean: moving a
    /// shape on frame 24 keys its position only, so a fade on the opacity
    /// channel carries on undisturbed.
    pub fn set_pose(&mut self, frame: u32, pose: Transform) {
        let current = self.sample(frame as f32);
        fn update<T: crate::channel::Animatable>(channel: &mut Channel<T>, frame: u32, old: T, new: T) {
            if old != new || channel.key_at(frame).is_some() {
                channel.set(frame, new);
            }
        }
        update(&mut self.position, frame, current.position, pose.position);
        update(&mut self.rotation, frame, current.rotation, pose.rotation);
        update(&mut self.scale, frame, current.scale, pose.scale);
        update(&mut self.opacity, frame, current.opacity, pose.opacity);
    }

    /// Whether any property has a key at `frame`.
    pub fn has_key(&self, frame: u32) -> bool {
        self.channels().iter().any(|c| c.has_key(frame))
    }

    /// Every frame where at least one property has a key, sorted.
    pub fn key_frames(&self) -> Vec<u32> {
        let mut frames: Vec<u32> = self.channels().iter().flat_map(|c| c.key_frames()).collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    /// Keys every property at `frame` with its current value, like F6 in
    /// Animate. Returns whether anything was added.
    pub fn insert_key(&mut self, frame: u32) -> bool {
        // `|` rather than `||`, so every channel gets its key.
        self.channels_mut().into_iter().fold(false, |added, c| c.insert_key(frame) | added)
    }

    /// Removes the keys at `frame` from every property that can spare one.
    pub fn remove_key(&mut self, frame: u32) -> bool {
        self.channels_mut().into_iter().fold(false, |removed, c| c.remove_key(frame) | removed)
    }

    /// Moves every property's key at `from` to `to`.
    ///
    /// All-or-nothing: if any of those properties already has a key at `to`,
    /// nothing moves. Otherwise keys that were together would come apart.
    pub fn move_key(&mut self, from: u32, to: u32) -> bool {
        let moving: Vec<Property> =
            Property::ALL.into_iter().filter(|&p| self.channel(p).has_key(from)).collect();
        if moving.is_empty() || (from != to && moving.iter().any(|&p| self.channel(p).has_key(to))) {
            return false;
        }
        for property in moving {
            self.channel_mut(property).move_key(from, to);
        }
        true
    }

    /// Sets the easing of every property's tween that covers `frame`.
    pub fn set_easing(&mut self, frame: u32, easing: Easing) {
        for channel in self.channels_mut() {
            if let Some(e) = channel.tween_easing_mut(frame) {
                *e = easing;
            }
        }
    }

    /// Whether every property has at least one key. Always true for tracks
    /// built in code; a hand-edited file could break it, so loading checks.
    /// (Loading already sorts and de-duplicates keys, in `Channel::from_keys`.)
    pub(crate) fn has_every_property(&self) -> bool {
        self.channels().iter().all(|c| !c.key_frames().is_empty())
    }
}

/// How a track can appear on disk.
///
/// `untagged` makes serde try each variant in turn. The current format is
/// an object with one array per property; version 1 was a single array of
/// whole-transform keyframes. Accepting both here keeps the migration in one
/// small place instead of needing a parallel copy of the whole document.
#[derive(Deserialize)]
#[serde(untagged)]
enum TrackFormat {
    Current {
        position: Vec<Key<Point>>,
        rotation: Vec<Key<f32>>,
        scale: Vec<Key<Point>>,
        opacity: Vec<Key<f32>>,
    },
    Version1(Vec<KeyframeV1>),
}

/// A keyframe from format version 1: a whole transform plus the easing to
/// the next keyframe.
#[derive(Deserialize)]
struct KeyframeV1 {
    frame: u32,
    transform: Transform,
    #[serde(default)]
    easing: Easing,
}

impl From<TrackFormat> for Track {
    fn from(format: TrackFormat) -> Self {
        match format {
            TrackFormat::Current { position, rotation, scale, opacity } => Track {
                position: Channel::from_keys(position),
                rotation: Channel::from_keys(rotation),
                scale: Channel::from_keys(scale),
                opacity: Channel::from_keys(opacity),
            },
            // Each old keyframe becomes a key on every property, with the
            // same easing, so the animation plays back exactly as before.
            TrackFormat::Version1(keys) => {
                fn channel<T: crate::channel::Animatable>(
                    keys: &[KeyframeV1],
                    value: impl Fn(&Transform) -> T,
                ) -> Channel<T> {
                    Channel::from_keys(
                        keys.iter()
                            .map(|k| Key { frame: k.frame, value: value(&k.transform), easing: k.easing })
                            .collect(),
                    )
                }
                Track {
                    position: channel(&keys, |t| t.position),
                    rotation: channel(&keys, |t| t.rotation),
                    scale: channel(&keys, |t| t.scale),
                    opacity: channel(&keys, |t| t.opacity),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_x(x: f32) -> Transform {
        Transform::at(Point::new(x, 0.0))
    }

    #[test]
    fn set_pose_only_keys_what_changed() {
        let mut track = Track::new(0, at_x(0.0));
        track.set_pose(24, at_x(100.0));

        assert!(track.position.key_at(24).is_some());
        assert!(!track.rotation.has_key(24));
        assert!(!track.opacity.has_key(24));
    }

    #[test]
    fn properties_tween_independently() {
        let mut track = Track::new(0, at_x(0.0));
        track.set_pose(10, at_x(100.0));
        track.set_pose(20, Transform { opacity: 0.0, ..track.sample(20.0) });
        *track.position.tween_easing_mut(0).unwrap() = Easing::EaseIn;

        let mid = track.sample(10.0);
        // Opacity fades linearly from frame 0 to 20, regardless of position's
        // keys and easing.
        assert!((mid.opacity - 0.5).abs() < 1e-4, "{}", mid.opacity);
        assert_eq!(mid.position.x, 100.0);
        // Position eases in: slower than linear at the start.
        assert!(track.sample(5.0).position.x < 50.0);
    }

    #[test]
    fn set_pose_updates_existing_keys_even_if_unchanged_elsewhere() {
        let mut track = Track::new(0, at_x(0.0));
        track.insert_key(10);
        track.set_pose(10, Transform { rotation: 45.0, ..at_x(0.0) });
        assert_eq!(track.rotation.key_at(10).unwrap().value, 45.0);
    }

    #[test]
    fn insert_key_keys_every_property() {
        let mut track = Track::new(0, at_x(0.0));
        assert!(track.insert_key(8));
        assert!(Property::ALL.iter().all(|&p| track.channel(p).has_key(8)));
        assert!(!track.insert_key(8));
    }

    #[test]
    fn move_key_moves_keys_together_or_not_at_all() {
        let mut track = Track::new(0, at_x(0.0));
        track.insert_key(10);
        track.set_pose(20, at_x(50.0)); // Position only.

        // Frame 10's keys can't move onto 20, because position already has one there.
        assert!(!track.move_key(10, 20));
        assert!(track.move_key(10, 15));
        assert!(Property::ALL.iter().all(|&p| track.channel(p).has_key(15)));
        assert_eq!(track.key_frames(), [0, 15, 20]);
    }

    #[test]
    fn set_easing_applies_to_every_covering_tween() {
        let mut track = Track::new(0, at_x(0.0));
        track.insert_key(10);
        track.set_easing(5, Easing::EaseOut);
        assert!(
            Property::ALL.iter().all(|&p| track.channel(p).tween_at(5).unwrap().easing == Easing::EaseOut)
        );
    }

    #[test]
    fn version_1_tracks_load_as_identical_channels() {
        let json = r#"[
            {"frame": 0, "transform": {"position": {"x": 0, "y": 0}, "opacity": 1}, "easing": {"type": "ease_in"}},
            {"frame": 10, "transform": {"position": {"x": 100, "y": 0}, "rotation": 90, "opacity": 0.5}}
        ]"#;
        let track: Track = serde_json::from_str(json).unwrap();

        assert_eq!(track.key_frames(), [0, 10]);
        assert!(
            Property::ALL.iter().all(|&p| track.channel(p).tween_at(0).unwrap().easing == Easing::EaseIn)
        );
        let end = track.sample(10.0);
        assert_eq!((end.position.x, end.rotation, end.opacity), (100.0, 90.0, 0.5));
        // Fields missing from a v1 transform took their defaults.
        assert_eq!(end.scale, Point::new(1.0, 1.0));
    }

    #[test]
    fn current_format_round_trips() {
        let mut track = Track::new(0, at_x(0.0));
        track.set_pose(12, at_x(40.0));
        let json = serde_json::to_string(&track).unwrap();
        assert_eq!(serde_json::from_str::<Track>(&json).unwrap(), track);
    }
}
