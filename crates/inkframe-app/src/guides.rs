//! Framing guides drawn over the stage: safe areas, and a field guide as
//! traditional animators and Harmony use.
//!
//! The stage is the camera's frame, so both are laid out on the stage
//! rectangle. They're only drawn in the editor, never exported.
//!
//! This file works out the lines and labels in stage pixels; `stage.rs`
//! puts them on screen (turning them with the view).

use egui::Align2;
use inkframe_core::{Point, Rect};

/// Which safe areas to show, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SafeAreas {
    #[default]
    Off,
    /// Action safe 93% and title safe 90% of the frame, as SMPTE
    /// ST 2046-1 sets them for HD.
    Current,
    /// Action safe 90% and title safe 80%, the older figures from when TV
    /// sets cropped more. Many studios still frame to them.
    Classic,
}

impl SafeAreas {
    pub const ALL: [SafeAreas; 3] = [SafeAreas::Off, SafeAreas::Current, SafeAreas::Classic];

    pub fn label(self) -> &'static str {
        match self {
            SafeAreas::Off => "Off",
            SafeAreas::Current => "Current (93% / 90%)",
            SafeAreas::Classic => "Classic (90% / 80%)",
        }
    }

    /// The action-safe and title-safe fractions of the frame's width and
    /// height.
    fn fractions(self) -> Option<(f32, f32)> {
        match self {
            SafeAreas::Off => None,
            SafeAreas::Current => Some((0.93, 0.90)),
            SafeAreas::Classic => Some((0.90, 0.80)),
        }
    }
}

/// The editor's framing guide settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Guides {
    pub safe_areas: SafeAreas,
    pub field_guide: bool,
}

/// How strongly a guide line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Faint,
    Normal,
    Strong,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuideLine {
    pub from: Point,
    pub to: Point,
    pub weight: Weight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuideLabel {
    pub at: Point,
    pub text: String,
    /// Which side of the text sits at `at`.
    pub anchor: Align2,
}

/// The safe area rectangles within `frame`, outermost first, with their
/// names.
pub fn safe_areas(frame: Rect, kind: SafeAreas) -> Vec<(Rect, &'static str)> {
    let Some((action, title)) = kind.fractions() else { return Vec::new() };
    vec![(shrunk(frame, action), "Action safe"), (shrunk(frame, title), "Title safe")]
}

/// `frame` scaled by `fraction` about its middle.
fn shrunk(frame: Rect, fraction: f32) -> Rect {
    let centre = frame.center();
    let half = Point::new(frame.width() * fraction / 2.0, frame.height() * fraction / 2.0);
    Rect::from_points(centre - half, centre + half)
}

/// A 12-field chart filling `frame`: the 12-field is the whole frame and
/// each smaller field (11F, 10F … 1F) is that many twelfths of it, all
/// centred, with a grid line at every field step, the centre axes, and
/// diagonals from the middle to the corners. Fields are drawn at the
/// frame's own proportions, so the chart matches the camera.
pub fn field_guide(frame: Rect) -> (Vec<GuideLine>, Vec<GuideLabel>) {
    const FIELDS: u32 = 12;
    let centre = frame.center();
    // The distance between neighbouring fields' edges.
    let step = Point::new(frame.width() / (2 * FIELDS) as f32, frame.height() / (2 * FIELDS) as f32);
    let field = |n: u32| {
        let half = Point::new(step.x * n as f32, step.y * n as f32);
        Rect::from_points(centre - half, centre + half)
    };
    let mut lines = Vec::new();
    let mut line = |from: Point, to: Point, weight| lines.push(GuideLine { from, to, weight });

    // The grid, across the whole chart.
    for k in 1..FIELDS {
        for sign in [-1.0, 1.0] {
            let x = centre.x + sign * step.x * k as f32;
            let y = centre.y + sign * step.y * k as f32;
            line(Point::new(x, frame.min.y), Point::new(x, frame.max.y), Weight::Faint);
            line(Point::new(frame.min.x, y), Point::new(frame.max.x, y), Weight::Faint);
        }
    }
    for corner in frame.corners() {
        line(centre, corner, Weight::Faint);
    }
    // The fields themselves, every other one heavier, as on a printed chart.
    for n in 1..=FIELDS {
        let weight = if n % 2 == 0 { Weight::Normal } else { Weight::Faint };
        let [a, b, c, d] = field(n).corners();
        for (from, to) in [(a, b), (b, c), (c, d), (d, a)] {
            line(from, to, weight);
        }
    }
    line(Point::new(centre.x, frame.min.y), Point::new(centre.x, frame.max.y), Weight::Strong);
    line(Point::new(frame.min.x, centre.y), Point::new(frame.max.x, centre.y), Weight::Strong);

    let mut labels: Vec<GuideLabel> = (1..=FIELDS)
        .map(|n| GuideLabel { at: field(n).min, text: format!("{n}F"), anchor: Align2::LEFT_TOP })
        .collect();
    for (at, text, anchor) in [
        (Point::new(centre.x, frame.min.y), "N", Align2::CENTER_TOP),
        (Point::new(frame.max.x, centre.y), "E", Align2::RIGHT_CENTER),
        (Point::new(centre.x, frame.max.y), "S", Align2::CENTER_BOTTOM),
        (Point::new(frame.min.x, centre.y), "W", Align2::LEFT_CENTER),
    ] {
        labels.push(GuideLabel { at, text: text.to_owned(), anchor });
    }
    (lines, labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hd() -> Rect {
        Rect::from_points(Point::ZERO, Point::new(1920.0, 1080.0))
    }

    #[test]
    fn current_safe_areas_are_93_and_90_percent() {
        let areas = safe_areas(hd(), SafeAreas::Current);
        let (action, title) = (areas[0].0, areas[1].0);
        assert!((action.width() - 1785.6).abs() < 0.01 && (action.height() - 1004.4).abs() < 0.01);
        assert!((title.width() - 1728.0).abs() < 0.01 && (title.height() - 972.0).abs() < 0.01);
        assert!(action.center().distance(hd().center()) < 0.01);
        assert!(safe_areas(hd(), SafeAreas::Off).is_empty());
    }

    #[test]
    fn classic_title_safe_is_80_percent() {
        let title = safe_areas(hd(), SafeAreas::Classic)[1].0;
        assert!((title.min.x - 192.0).abs() < 0.01 && (title.min.y - 108.0).abs() < 0.01);
    }

    #[test]
    fn the_12_field_is_the_frame_and_6_field_is_half_of_it() {
        let (_, labels) = field_guide(hd());
        let at = |text: &str| labels.iter().find(|l| l.text == text).unwrap().at;
        assert_eq!(at("12F"), Point::ZERO);
        assert_eq!(at("6F"), Point::new(480.0, 270.0));
        // A twelfth of the frame: 160 × 90 about the middle.
        assert_eq!(at("1F"), Point::new(880.0, 495.0));
        assert!(labels.iter().any(|l| l.text == "N"));
    }

    #[test]
    fn the_field_guide_stays_inside_the_frame() {
        let (lines, _) = field_guide(hd());
        let inside = |p: Point| (0.0..=1920.0).contains(&p.x) && (0.0..=1080.0).contains(&p.y);
        assert!(lines.iter().all(|l| inside(l.from) && inside(l.to)));
    }
}
