//! Animated values as scripts see them.
//!
//! Positions, scales and skews are `[x, y]` arrays; rotation and skew are
//! in degrees; scale is a factor (1 is the original size); opacity runs
//! from 0 to 1; depth is in stage pixels. Easing is one of `"linear"`,
//! `"ease-in"`, `"ease-out"`, `"ease-in-out"` and `"hold"`, or
//! `{ bezier: [x1, y1, x2, y2] }`, and applies to the tween leaving a key.

use inkframe_core::{Animatable, Channel, Easing, Key, Point, Property, Track};
use serde_json::{Value, json};

pub(crate) fn property(name: &str) -> Result<Property, String> {
    Ok(match name {
        "position" => Property::Position,
        "rotation" => Property::Rotation,
        "scale" => Property::Scale,
        "skew" => Property::Skew,
        "opacity" => Property::Opacity,
        "depth" => Property::Depth,
        _ => {
            return Err(format!(
                "there's no property called {name}: use position, rotation, scale, skew, opacity or depth"
            ));
        }
    })
}

pub(crate) fn point(value: &Value) -> Result<Point, String> {
    match value.as_array().map(Vec::as_slice) {
        Some([x, y]) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => Ok(Point::new(x as f32, y as f32)),
            _ => Err(format!("expected two numbers, like [960, 540], not {value}")),
        },
        _ => Err(format!("expected two numbers, like [960, 540], not {value}")),
    }
}

fn number(value: &Value) -> Result<f32, String> {
    value.as_f64().map(|n| n as f32).ok_or_else(|| format!("expected a number, not {value}"))
}

fn point_value(p: Point) -> Value {
    json!([p.x, p.y])
}

pub(crate) fn easing(value: &Value) -> Result<Easing, String> {
    if let Some(points) = value.get("bezier") {
        return match points.as_array().map(|p| p.iter().filter_map(Value::as_f64).collect::<Vec<_>>()) {
            Some(p) if p.len() == 4 => Ok(Easing::CubicBezier {
                x1: (p[0] as f32).clamp(0.0, 1.0),
                y1: p[1] as f32,
                x2: (p[2] as f32).clamp(0.0, 1.0),
                y2: p[3] as f32,
            }),
            _ => Err("bezier easing takes four numbers: [x1, y1, x2, y2]".to_owned()),
        };
    }
    Ok(match value.as_str() {
        Some("linear") => Easing::Linear,
        Some("ease-in") => Easing::EaseIn,
        Some("ease-out") => Easing::EaseOut,
        Some("ease-in-out") => Easing::EaseInOut,
        Some("hold") => Easing::Hold,
        _ => {
            return Err(format!(
                "{value} isn't an easing: use \"linear\", \"ease-in\", \"ease-out\", \"ease-in-out\", \"hold\" or {{ bezier: [x1, y1, x2, y2] }}"
            ));
        }
    })
}

fn easing_value(easing: Easing) -> Value {
    match easing {
        Easing::Linear => json!("linear"),
        Easing::EaseIn => json!("ease-in"),
        Easing::EaseOut => json!("ease-out"),
        Easing::EaseInOut => json!("ease-in-out"),
        Easing::Hold => json!("hold"),
        Easing::CubicBezier { x1, y1, x2, y2 } => json!({ "bezier": [x1, y1, x2, y2] }),
    }
}

/// `property`'s keys: `{ frame, value, easing }`, frames from 1.
pub(crate) fn keys(track: &Track, name: &str) -> Result<Vec<Value>, String> {
    fn list<T: Animatable>(channel: &Channel<T>, value: impl Fn(&T) -> Value) -> Vec<Value> {
        channel
            .keys()
            .iter()
            .map(|k| json!({ "frame": k.frame + 1, "value": value(&k.value), "easing": easing_value(k.easing) }))
            .collect()
    }
    Ok(match property(name)? {
        Property::Position => list(&track.position, |p| point_value(*p)),
        Property::Rotation => list(&track.rotation, |r| json!(r)),
        Property::Scale => list(&track.scale, |p| point_value(*p)),
        Property::Skew => list(&track.skew, |p| point_value(*p)),
        Property::Opacity => list(&track.opacity, |o| json!(o)),
        Property::Depth => list(&track.depth, |d| json!(d)),
        Property::Effects => unreachable!("not offered to scripts"),
    })
}

/// `property`'s value at `frame` (0-based, and may be between frames).
pub(crate) fn value_at(track: &Track, name: &str, frame: f32) -> Result<Value, String> {
    let pose = track.sample(frame);
    Ok(match property(name)? {
        Property::Position => point_value(pose.position),
        Property::Rotation => json!(pose.rotation),
        Property::Scale => point_value(pose.scale),
        Property::Skew => point_value(pose.skew),
        Property::Opacity => json!(pose.opacity),
        Property::Depth => json!(track.depth_at(frame)),
        Property::Effects => unreachable!("not offered to scripts"),
    })
}

/// Keys `property` at `frame` (0-based), with `easing` for the tween
/// leaving it if given (otherwise a new key takes the easing before it).
pub(crate) fn set_key(
    track: &mut Track,
    name: &str,
    frame: u32,
    value: &Value,
    easing: Option<Easing>,
) -> Result<(), String> {
    /// Sets the key, keeping its curve handles if it had a key already.
    fn key<T: Animatable>(channel: &mut Channel<T>, frame: u32, value: T, easing: Option<Easing>) {
        match easing {
            // The easing goes on the key itself, so it holds even when the
            // next key hasn't been set yet.
            Some(easing) => {
                let bend = channel.key_at(frame).and_then(|k| k.bend);
                channel.put(Key { frame, value, easing, bend });
            }
            None => channel.set(frame, value),
        }
    }
    match property(name)? {
        Property::Position => key(&mut track.position, frame, point(value)?, easing),
        Property::Rotation => key(&mut track.rotation, frame, number(value)?, easing),
        Property::Scale => key(&mut track.scale, frame, point(value)?, easing),
        Property::Skew => {
            let skew = point(value)?;
            key(
                &mut track.skew,
                frame,
                Point::new(skew.x.clamp(-89.0, 89.0), skew.y.clamp(-89.0, 89.0)),
                easing,
            );
        }
        Property::Opacity => key(&mut track.opacity, frame, number(value)?.clamp(0.0, 1.0), easing),
        Property::Depth => key(&mut track.depth, frame, number(value)?, easing),
        Property::Effects => unreachable!("not offered to scripts"),
    }
    Ok(())
}
