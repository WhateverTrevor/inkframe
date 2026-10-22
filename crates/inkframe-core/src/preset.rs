//! Motion presets, as Animate's: a stretch of animation saved on its own,
//! to give any layer the same movement. A bounce in, a fade, a spin.
//!
//! A preset keeps a layer's keys over a stretch of frames *relative to
//! where the layer was when it started*, so applying it starts from wherever
//! the layer is now:
//!
//! * position and depth as distances moved,
//! * rotation and skew as angles turned,
//! * scale and opacity as how many times bigger, or as opaque.
//!
//! Easing and motion-path curves come along too. Applying one replaces the
//! layer's keys over that stretch, for the properties the preset animates,
//! and leaves the rest of its animation alone.

use serde::{Deserialize, Serialize};

use crate::channel::{AnyChannel, Bend, Channel, Key};
use crate::document::Layer;
use crate::easing::Easing;
use crate::geometry::Point;

/// The extension preset files are saved with.
pub const PRESET_EXTENSION: &str = "inkpreset";

/// One key of a preset: how far into it, the change at that point, and
/// how it travels on to the next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetKey<T> {
    pub frame: u32,
    pub value: T,
    #[serde(default)]
    pub easing: Easing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend: Option<Bend>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotionPreset {
    pub name: String,
    /// How many frames it lasts, from its first key to its last.
    pub length: u32,
    /// Distances moved from the starting position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub position: Vec<PresetKey<Point>>,
    /// Degrees turned from the starting rotation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rotation: Vec<PresetKey<f32>>,
    /// Times the starting scale.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scale: Vec<PresetKey<Point>>,
    /// Degrees of skew added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skew: Vec<PresetKey<Point>>,
    /// Times the starting opacity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opacity: Vec<PresetKey<f32>>,
    /// Distance moved in depth.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depth: Vec<PresetKey<f32>>,
}

impl MotionPreset {
    /// A layer's animation from frame `first` to `last`, as a preset. Only
    /// properties that change there are kept. `None` if nothing does.
    pub fn from_layer(name: impl Into<String>, layer: &Layer, first: u32, last: u32) -> Option<Self> {
        let (first, last) = (first.min(last), first.max(last));
        let track = &layer.track;
        let start = |p: Point, q: Point| p - q;
        let ratio = |a: Point, b: Point| Point::new(divide(a.x, b.x), divide(a.y, b.y));
        let preset = Self {
            name: name.into(),
            length: last - first,
            position: capture(&track.position, first, last, start),
            rotation: capture(&track.rotation, first, last, |a, b| a - b),
            scale: capture(&track.scale, first, last, ratio),
            skew: capture(&track.skew, first, last, start),
            opacity: capture(&track.opacity, first, last, divide),
            depth: capture(&track.depth, first, last, |a, b| a - b),
        };
        let animates = !(preset.position.is_empty()
            && preset.rotation.is_empty()
            && preset.scale.is_empty()
            && preset.skew.is_empty()
            && preset.opacity.is_empty()
            && preset.depth.is_empty());
        animates.then_some(preset)
    }

    /// Gives `layer` this movement from frame `at`, starting from how the
    /// layer is posed there.
    pub fn apply(&self, layer: &mut Layer, at: u32) {
        let end = at + self.length;
        let track = &mut layer.track;
        place(&mut track.position, &self.position, at, end, |base, v| base + v);
        place(&mut track.rotation, &self.rotation, at, end, |base, v| base + v);
        place(&mut track.scale, &self.scale, at, end, |base, v| Point::new(base.x * v.x, base.y * v.y));
        place(&mut track.skew, &self.skew, at, end, |base, v| base + v);
        place(&mut track.opacity, &self.opacity, at, end, |base, v| (base * v).clamp(0.0, 1.0));
        place(&mut track.depth, &self.depth, at, end, |base, v| base + v);
        // A layer that ended before the movement does now lasts through it.
        if layer.end.is_some_and(|e| e < end) {
            layer.end = Some(end);
        }
    }

    /// The presets Inkframe comes with.
    pub fn built_in() -> Vec<MotionPreset> {
        let p = Point::new;
        let still = |name: &str, length| MotionPreset {
            name: name.to_owned(),
            length,
            position: Vec::new(),
            rotation: Vec::new(),
            scale: Vec::new(),
            skew: Vec::new(),
            opacity: Vec::new(),
            depth: Vec::new(),
        };
        // Overshoots a little and settles, as things popping in do.
        let pop = Easing::CubicBezier { x1: 0.34, y1: 1.56, x2: 0.64, y2: 1.0 };
        vec![
            MotionPreset {
                opacity: vec![key(0, 0.0, Easing::EaseOut), key(12, 1.0, Easing::Linear)],
                ..still("Fade In", 12)
            },
            MotionPreset {
                opacity: vec![key(0, 1.0, Easing::EaseIn), key(12, 0.0, Easing::Linear)],
                ..still("Fade Out", 12)
            },
            MotionPreset {
                scale: vec![key(0, p(0.0, 0.0), pop), key(12, p(1.0, 1.0), Easing::Linear)],
                opacity: vec![key(0, 0.0, Easing::EaseOut), key(4, 1.0, Easing::Linear)],
                ..still("Pop In", 12)
            },
            MotionPreset {
                position: vec![key(0, p(-400.0, 0.0), Easing::EaseOut), key(16, p(0.0, 0.0), Easing::Linear)],
                opacity: vec![key(0, 0.0, Easing::EaseOut), key(8, 1.0, Easing::Linear)],
                ..still("Slide In from Left", 16)
            },
            MotionPreset {
                position: vec![key(0, p(400.0, 0.0), Easing::EaseOut), key(16, p(0.0, 0.0), Easing::Linear)],
                opacity: vec![key(0, 0.0, Easing::EaseOut), key(8, 1.0, Easing::Linear)],
                ..still("Slide In from Right", 16)
            },
            // Falls in, then two smaller bounces, squashing as it lands.
            MotionPreset {
                position: vec![
                    key(0, p(0.0, -500.0), Easing::EaseIn),
                    key(12, p(0.0, 0.0), Easing::EaseOut),
                    key(18, p(0.0, -120.0), Easing::EaseIn),
                    key(24, p(0.0, 0.0), Easing::EaseOut),
                    key(28, p(0.0, -40.0), Easing::EaseIn),
                    key(32, p(0.0, 0.0), Easing::Linear),
                ],
                scale: vec![
                    key(0, p(0.9, 1.15), Easing::Linear),
                    key(11, p(0.9, 1.15), Easing::EaseOut),
                    key(12, p(1.2, 0.8), Easing::EaseOut),
                    key(15, p(1.0, 1.0), Easing::Linear),
                    key(23, p(1.0, 1.0), Easing::EaseOut),
                    key(24, p(1.1, 0.9), Easing::EaseOut),
                    key(27, p(1.0, 1.0), Easing::Linear),
                ],
                ..still("Bounce In", 32)
            },
            MotionPreset {
                rotation: vec![key(0, 0.0, Easing::EaseInOut), key(24, 360.0, Easing::Linear)],
                ..still("Spin", 24)
            },
            MotionPreset {
                scale: vec![
                    key(0, p(1.0, 1.0), Easing::EaseInOut),
                    key(8, p(1.15, 1.15), Easing::EaseInOut),
                    key(16, p(1.0, 1.0), Easing::Linear),
                ],
                ..still("Pulse", 16)
            },
            MotionPreset {
                position: (0..=8)
                    .map(|i| {
                        let x = if i == 0 || i == 8 {
                            0.0
                        } else if i % 2 == 1 {
                            14.0
                        } else {
                            -14.0
                        };
                        key(i * 2, p(x * (1.0 - i as f32 / 10.0), 0.0), Easing::EaseInOut)
                    })
                    .collect(),
                ..still("Shake", 16)
            },
            MotionPreset {
                position: vec![
                    key(0, p(0.0, 0.0), Easing::EaseInOut),
                    key(24, p(0.0, -30.0), Easing::EaseInOut),
                    key(48, p(0.0, 0.0), Easing::Linear),
                ],
                ..still("Float", 48)
            },
        ]
    }
}

/// A preset key with no motion-path curve.
fn key<T>(frame: u32, value: T, easing: Easing) -> PresetKey<T> {
    PresetKey { frame, value, easing, bend: None }
}

/// `a / b`, or 1 where `b` is 0 (a scale or opacity of nothing has no
/// ratio worth keeping).
fn divide(a: f32, b: f32) -> f32 {
    if b.abs() < 1e-6 { 1.0 } else { a / b }
}

/// A channel's keys from `first` to `last`, each turned into a change from
/// the value at `first` by `relative`, counted from `first`. Empty if the
/// value doesn't change there.
fn capture<T: crate::Animatable + PartialEq>(
    channel: &Channel<T>,
    first: u32,
    last: u32,
    relative: impl Fn(T, T) -> T,
) -> Vec<PresetKey<T>> {
    let Some(base) = channel.sample(first as f32) else { return Vec::new() };
    let mut keys: Vec<Key<T>> =
        channel.keys().iter().filter(|k| (first..=last).contains(&k.frame)).cloned().collect();
    // Keys at both ends, so the stretch is complete even if it starts or
    // ends between keys.
    for end in [first, last] {
        if !keys.iter().any(|k| k.frame == end)
            && let Some(key) = channel.key_or_sample(end)
        {
            keys.push(key);
        }
    }
    keys.sort_by_key(|k| k.frame);
    if keys.iter().all(|k| k.value == base) {
        return Vec::new();
    }
    keys.into_iter()
        .map(|k| PresetKey {
            frame: k.frame - first,
            value: relative(k.value, base.clone()),
            easing: k.easing,
            bend: k.bend,
        })
        .collect()
}

/// Puts a preset's keys for one property into `channel` from frame `at`,
/// made absolute from the value there by `absolute`, in place of whatever
/// keys were between `at` and `end`.
fn place<T: crate::Animatable>(
    channel: &mut Channel<T>,
    keys: &[PresetKey<T>],
    at: u32,
    end: u32,
    absolute: impl Fn(T, T) -> T,
) {
    if keys.is_empty() {
        return;
    }
    let Some(base) = channel.sample(at as f32) else { return };
    channel.retime(&|f| (!(at..=end).contains(&f)).then_some(f), at);
    for key in keys {
        channel.put(Key {
            frame: at + key.frame,
            value: absolute(base.clone(), key.value.clone()),
            easing: key.easing,
            bend: key.bend,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Transform};

    fn layer_at(x: f32, y: f32) -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("Ball", Vec::new(), 0, Transform::at(Point::new(x, y)));
        doc.layer(id).unwrap().clone()
    }

    #[test]
    fn a_preset_is_saved_relative_and_applied_from_wherever_the_layer_is() {
        // Moves 100 right and fades to half over frames 10 to 20.
        let mut source = layer_at(50.0, 50.0);
        source.track.position.set(10, Point::new(50.0, 50.0));
        source.track.position.set(20, Point::new(150.0, 50.0));
        *source.track.position.tween_easing_mut(10).unwrap() = Easing::EaseOut;
        source.track.opacity.set(10, 1.0);
        source.track.opacity.set(20, 0.5);
        let preset = MotionPreset::from_layer("Slide", &source, 10, 20).unwrap();
        assert_eq!(preset.length, 10);
        assert_eq!(preset.position.last().unwrap().value, Point::new(100.0, 0.0));
        assert_eq!(preset.position[0].easing, Easing::EaseOut, "easing comes along");
        assert!(preset.rotation.is_empty(), "rotation never changes");

        // Applied to a layer elsewhere, a third opaque, from frame 30.
        let mut target = layer_at(400.0, 300.0);
        target.track.opacity.set(0, 0.6);
        preset.apply(&mut target, 30);
        let at = |f: f32| target.track.position.sample(f).unwrap();
        assert_eq!(at(30.0), Point::new(400.0, 300.0));
        assert_eq!(at(40.0), Point::new(500.0, 300.0));
        assert!((target.track.opacity.sample(40.0).unwrap() - 0.3).abs() < 1e-5, "half of its own opacity");
        // Before it starts, the layer is as it was.
        assert_eq!(at(0.0), Point::new(400.0, 300.0));
    }

    #[test]
    fn applying_replaces_keys_in_its_stretch_only() {
        let mut target = layer_at(0.0, 0.0);
        target.track.rotation.set(5, 45.0);
        target.track.rotation.set(50, 90.0);
        let spin = MotionPreset::built_in().into_iter().find(|p| p.name == "Spin").unwrap();
        spin.apply(&mut target, 10);
        let frames = target.track.rotation.key_frames();
        assert_eq!(frames, vec![0, 5, 10, 34, 50], "the preset's keys, the others kept");
    }

    #[test]
    fn built_in_presets_round_trip_through_json() {
        for preset in MotionPreset::built_in() {
            let json = serde_json::to_string(&preset).unwrap();
            assert_eq!(serde_json::from_str::<MotionPreset>(&json).unwrap(), preset, "{}", preset.name);
            assert!(preset.length > 0);
        }
    }

    #[test]
    fn a_layer_that_does_not_move_makes_no_preset() {
        assert!(MotionPreset::from_layer("Still", &layer_at(0.0, 0.0), 0, 20).is_none());
    }
}
