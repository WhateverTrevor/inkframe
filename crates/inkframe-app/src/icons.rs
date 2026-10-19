//! The interface's small icons, drawn as lines rather than taken from an
//! emoji font, so they share one weight and colour and follow the theme.
//! (The toolbar's tool icons are drawn the same way, in `toolbar.rs`.)
//!
//! Each icon is laid out on a unit square: `at(x, y)` maps 0–1 across and
//! down to a point in the icon's rectangle.

use std::f32::consts::{PI, TAU};

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget, pos2, vec2};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // Layers.
    Eye,
    EyeClosed,
    Lock,
    Unlock,
    Up,
    Down,
    Duplicate,
    Trash,
    // Playback.
    First,
    Previous,
    Play,
    Pause,
    Next,
    Last,
    Loop,
    Onion,
    Speaker,
    SpeakerMuted,
    // Area kinds.
    Stage,
    Timeline,
    Properties,
    Tools,
    Library,
    ExposureSheet,
    Graph,
    ToolSettings,
    Document,
    Folder,
    FolderOpen,
    Swatches,
    History,
    Camera,
    LightTable,
    /// A dial: master controllers.
    Dial,
    Plus,
    // The Align panel.
    AlignLeft,
    AlignCenterX,
    AlignRight,
    AlignTop,
    AlignCenterY,
    AlignBottom,
    DistributeX,
    DistributeY,
    MatchWidth,
    MatchHeight,
    /// A circle struck through: no fill or no stroke.
    NoColor,
    /// The little downward arrow that marks a menu.
    Chevron,
}

/// Draws `icon` filling `rect` (best square) in `color`.
pub fn paint(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let side = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), Vec2::splat(side));
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let stroke = Stroke::new((side / 10.0).clamp(1.2, 1.8), color);
    let line = |points: &[(f32, f32)]| {
        painter.add(Shape::line(points.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
    };
    let fill = |points: &[(f32, f32)]| {
        painter.add(Shape::convex_polygon(
            points.iter().map(|&(x, y)| at(x, y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    // Points along an arc of the circle at (cx, cy), radius `radius`, from
    // angle `from` to `to` (radians, clockwise from the right, y down).
    let arc = |cx: f32, cy: f32, radius: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=24)
            .map(|i| {
                let a = from + (to - from) * i as f32 / 24.0;
                at(cx + radius * a.cos(), cy + radius * a.sin())
            })
            .collect()
    };

    match icon {
        Icon::Eye | Icon::EyeClosed => {
            // An almond of two arcs meeting at the corners (0.05, 0.5) and
            // (0.95, 0.5), each bulging 0.28 from the middle line, with the
            // pupil. Closed, just the lower lid and lashes.
            const CORNER: f32 = 0.4577; // radians from level to a corner
            let upper = arc(0.5, 0.72, 0.5, -PI + CORNER, -CORNER);
            let lower = arc(0.5, 0.28, 0.5, CORNER, PI - CORNER);
            if icon == Icon::Eye {
                let mut outline = upper;
                outline.extend(lower);
                painter.add(Shape::closed_line(outline, stroke));
                painter.circle_filled(at(0.5, 0.5), side * 0.14, color);
            } else {
                painter.add(Shape::line(lower, stroke));
                for (x0, x1) in [(0.3, 0.22), (0.5, 0.5), (0.7, 0.78)] {
                    line(&[(x0, 0.74), (x1, 0.92)]);
                }
            }
        }
        Icon::Lock | Icon::Unlock => {
            let body = Rect::from_min_max(at(0.2, 0.45), at(0.8, 0.92));
            painter.rect_filled(body, 2.0, color);
            // The shackle: down into the body on both sides, or lifted
            // with its right leg free.
            let (top, right_end) = if icon == Icon::Lock { (0.3, 0.45) } else { (0.18, 0.25) };
            let mut shackle = vec![at(0.32, 0.45)];
            shackle.extend(arc(0.5, top, 0.18, PI, TAU));
            shackle.push(at(0.68, right_end));
            painter.add(Shape::line(shackle, stroke));
        }
        Icon::Up => {
            line(&[(0.5, 0.85), (0.5, 0.18)]);
            line(&[(0.22, 0.45), (0.5, 0.17), (0.78, 0.45)]);
        }
        Icon::Down => {
            line(&[(0.5, 0.15), (0.5, 0.82)]);
            line(&[(0.22, 0.55), (0.5, 0.83), (0.78, 0.55)]);
        }
        Icon::Duplicate => {
            painter.rect_stroke(
                Rect::from_min_max(at(0.1, 0.3), at(0.7, 0.9)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            line(&[(0.3, 0.3), (0.3, 0.1), (0.9, 0.1), (0.9, 0.7), (0.7, 0.7)]);
        }
        Icon::Trash => {
            line(&[(0.12, 0.25), (0.88, 0.25)]);
            line(&[(0.38, 0.25), (0.4, 0.1), (0.6, 0.1), (0.62, 0.25)]);
            line(&[(0.22, 0.25), (0.28, 0.92), (0.72, 0.92), (0.78, 0.25)]);
            line(&[(0.42, 0.42), (0.43, 0.77)]);
            line(&[(0.58, 0.42), (0.57, 0.77)]);
        }
        Icon::First => {
            line(&[(0.2, 0.2), (0.2, 0.8)]);
            fill(&[(0.82, 0.18), (0.82, 0.82), (0.3, 0.5)]);
        }
        Icon::Previous => {
            fill(&[(0.78, 0.14), (0.78, 0.86), (0.2, 0.5)]);
        }
        Icon::Play => {
            fill(&[(0.25, 0.12), (0.88, 0.5), (0.25, 0.88)]);
        }
        Icon::Pause => {
            for x in [0.25, 0.6] {
                painter.rect_filled(Rect::from_min_max(at(x, 0.15), at(x + 0.17, 0.85)), 1.0, color);
            }
        }
        Icon::Next => {
            fill(&[(0.22, 0.14), (0.8, 0.5), (0.22, 0.86)]);
        }
        Icon::Last => {
            fill(&[(0.18, 0.18), (0.7, 0.5), (0.18, 0.82)]);
            line(&[(0.8, 0.2), (0.8, 0.8)]);
        }
        Icon::Loop => {
            // Two arrows chasing round a circle.
            painter.add(Shape::line(arc(0.5, 0.5, 0.33, PI * 1.15, PI * 1.85 + PI * 0.15), stroke));
            painter.add(Shape::line(arc(0.5, 0.5, 0.33, PI * 0.15, PI * 0.85 + PI * 0.15), stroke));
            fill(&[(0.86, 0.25), (0.98, 0.52), (0.7, 0.48)]);
            fill(&[(0.14, 0.75), (0.02, 0.48), (0.3, 0.52)]);
        }
        Icon::Onion => {
            // Three overlapping frames, fading back: onion skin.
            for (i, alpha) in [(0.0, 0.35), (0.12, 0.6), (0.24, 1.0)] {
                let frame = Rect::from_min_size(at(0.08 + i, 0.12 + i), r.size() * 0.6);
                painter.rect_stroke(
                    frame,
                    2.0,
                    Stroke::new(stroke.width, color.gamma_multiply(alpha)),
                    egui::StrokeKind::Middle,
                );
            }
        }
        Icon::Speaker | Icon::SpeakerMuted => {
            fill(&[(0.08, 0.36), (0.3, 0.36), (0.3, 0.64), (0.08, 0.64)]);
            fill(&[(0.3, 0.36), (0.55, 0.12), (0.55, 0.88), (0.3, 0.64)]);
            if icon == Icon::Speaker {
                painter.add(Shape::line(arc(0.55, 0.5, 0.17, -PI * 0.3, PI * 0.3), stroke));
                painter.add(Shape::line(arc(0.55, 0.5, 0.33, -PI * 0.3, PI * 0.3), stroke));
            } else {
                line(&[(0.68, 0.36), (0.92, 0.64)]);
                line(&[(0.92, 0.36), (0.68, 0.64)]);
            }
        }
        Icon::Stage => {
            // A picture: frame, hills and the sun.
            painter.rect_stroke(
                Rect::from_min_max(at(0.05, 0.15), at(0.95, 0.85)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            line(&[(0.12, 0.78), (0.4, 0.45), (0.58, 0.65), (0.7, 0.53), (0.88, 0.78)]);
            painter.circle_filled(at(0.7, 0.33), side * 0.07, color);
        }
        Icon::Timeline => {
            // Two tracks with a keyframe diamond each, and the playhead
            // crossing them.
            for (y, key) in [(0.42, 0.24), (0.8, 0.74)] {
                line(&[(0.04, y), (0.96, y)]);
                fill(&[(key, y - 0.14), (key + 0.14, y), (key, y + 0.14), (key - 0.14, y)]);
            }
            line(&[(0.5, 0.18), (0.5, 0.98)]);
            fill(&[(0.36, 0.02), (0.64, 0.02), (0.5, 0.2)]);
        }
        Icon::Properties => {
            // Sliders.
            for (y, knob) in [(0.22, 0.65), (0.5, 0.3), (0.78, 0.55)] {
                line(&[(0.08, y), (0.92, y)]);
                painter.circle_filled(at(knob, y), side * 0.1, color);
            }
        }
        Icon::Tools => {
            // The selection arrow.
            let arrow = [
                (0.25, 0.08),
                (0.25, 0.8),
                (0.42, 0.64),
                (0.55, 0.92),
                (0.66, 0.87),
                (0.53, 0.6),
                (0.76, 0.6),
            ];
            painter.add(Shape::closed_line(arrow.iter().map(|&(x, y)| at(x, y)).collect(), stroke));
        }
        Icon::Library => {
            // Books on a shelf, one leaning.
            for x in [0.12, 0.32] {
                painter.rect_stroke(
                    Rect::from_min_max(at(x, 0.15), at(x + 0.15, 0.88)),
                    1.0,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            line(&[(0.55, 0.22), (0.7, 0.17), (0.9, 0.84), (0.75, 0.88), (0.55, 0.22)]);
        }
        Icon::ExposureSheet => {
            painter.rect_stroke(
                Rect::from_min_max(at(0.15, 0.05), at(0.85, 0.95)),
                1.5,
                stroke,
                egui::StrokeKind::Middle,
            );
            line(&[(0.38, 0.05), (0.38, 0.95)]);
            for y in [0.28, 0.5, 0.72] {
                line(&[(0.15, y), (0.85, y)]);
            }
        }
        Icon::Graph => {
            line(&[(0.1, 0.05), (0.1, 0.9), (0.95, 0.9)]);
            let curve: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let t = i as f32 / 20.0;
                    // An ease-in-out from bottom left to top right.
                    let eased = t * t * (3.0 - 2.0 * t);
                    at(0.18 + t * 0.74, 0.8 - eased * 0.66)
                })
                .collect();
            painter.add(Shape::line(curve, stroke));
        }
        Icon::ToolSettings => {
            // A brush: handle, ferrule and a filled tip.
            line(&[(0.88, 0.08), (0.5, 0.48)]);
            line(&[(0.58, 0.4), (0.44, 0.54)]);
            fill(&[(0.42, 0.5), (0.52, 0.6), (0.3, 0.85), (0.08, 0.92), (0.16, 0.7)]);
        }
        Icon::Document => {
            line(&[(0.2, 0.05), (0.62, 0.05), (0.82, 0.25), (0.82, 0.95), (0.2, 0.95), (0.2, 0.05)]);
            line(&[(0.62, 0.05), (0.62, 0.25), (0.82, 0.25)]);
            for y in [0.48, 0.64, 0.8] {
                line(&[(0.32, y), (0.7, y)]);
            }
        }
        Icon::Folder | Icon::FolderOpen => {
            // A tabbed folder; open, its front leaf tips forward.
            let back = [(0.05, 0.2), (0.38, 0.2), (0.48, 0.32), (0.95, 0.32), (0.95, 0.85), (0.05, 0.85)];
            if icon == Icon::Folder {
                painter.add(Shape::convex_polygon(
                    back.iter().map(|&(x, y)| at(x, y)).collect(),
                    color.gamma_multiply(0.25),
                    stroke,
                ));
            } else {
                line(&[(0.05, 0.85), (0.05, 0.2), (0.38, 0.2), (0.48, 0.32), (0.85, 0.32), (0.85, 0.45)]);
                painter.add(Shape::convex_polygon(
                    [(0.05, 0.85), (0.2, 0.45), (1.0, 0.45), (0.85, 0.85)]
                        .iter()
                        .map(|&(x, y)| at(x, y))
                        .collect(),
                    color.gamma_multiply(0.25),
                    stroke,
                ));
            }
        }
        Icon::Swatches => {
            // Four chips, in fading shades.
            for (i, (x, y)) in
                [(0.05, 0.05), (0.55, 0.05), (0.05, 0.55), (0.55, 0.55)].into_iter().enumerate()
            {
                let chip = Rect::from_min_max(at(x, y), at(x + 0.4, y + 0.4));
                painter.rect_filled(chip, 1.5, color.gamma_multiply(1.0 - i as f32 * 0.22));
            }
        }
        Icon::History => {
            // A clock face with an arrow running back round it.
            painter.add(Shape::line(arc(0.5, 0.5, 0.4, -PI * 0.75, PI * 1.0), stroke));
            fill(&[(0.02, 0.18), (0.28, 0.2), (0.12, 0.4)]);
            line(&[(0.5, 0.28), (0.5, 0.52), (0.68, 0.62)]);
        }
        Icon::Camera => {
            // A film camera from the side, as on the toolbar.
            painter.rect_stroke(
                Rect::from_min_max(at(0.0, 0.42), at(0.7, 0.92)),
                1.5,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.add(Shape::closed_line(
                vec![at(0.7, 0.55), at(1.0, 0.42), at(1.0, 0.92), at(0.7, 0.8)],
                stroke,
            ));
            painter.circle_stroke(at(0.18, 0.2), side * 0.16, stroke);
            painter.circle_stroke(at(0.52, 0.2), side * 0.16, stroke);
        }
        Icon::LightTable => {
            // A lamp over a sheet of paper.
            painter.add(Shape::closed_line(
                vec![at(0.3, 0.1), at(0.7, 0.1), at(0.85, 0.42), at(0.15, 0.42)],
                stroke,
            ));
            for (x0, x1) in [(0.35, 0.25), (0.5, 0.5), (0.65, 0.75)] {
                line(&[(x0, 0.5), (x1, 0.66)]);
            }
            painter.rect_stroke(
                Rect::from_min_max(at(0.05, 0.74), at(0.95, 0.94)),
                1.5,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Dial => {
            painter.circle_stroke(r.center(), side * 0.4, stroke);
            line(&[(0.5, 0.5), (0.74, 0.26)]);
            painter.circle_filled(at(0.5, 0.5), side * 0.08, color);
        }
        Icon::Plus => {
            line(&[(0.5, 0.15), (0.5, 0.85)]);
            line(&[(0.15, 0.5), (0.85, 0.5)]);
        }
        Icon::AlignLeft | Icon::AlignCenterX | Icon::AlignRight => {
            // A guide line and two bars of different lengths against it.
            let x = match icon {
                Icon::AlignLeft => 0.08,
                Icon::AlignCenterX => 0.5,
                _ => 0.92,
            };
            line(&[(x, 0.02), (x, 0.98)]);
            for (y, length) in [(0.22, 0.7), (0.62, 0.45)] {
                let left = match icon {
                    Icon::AlignLeft => 0.16,
                    Icon::AlignCenterX => 0.5 - length / 2.0,
                    _ => 0.84 - length,
                };
                painter.rect_filled(Rect::from_min_max(at(left, y), at(left + length, y + 0.22)), 1.0, color);
            }
        }
        Icon::AlignTop | Icon::AlignCenterY | Icon::AlignBottom => {
            let y = match icon {
                Icon::AlignTop => 0.08,
                Icon::AlignCenterY => 0.5,
                _ => 0.92,
            };
            line(&[(0.02, y), (0.98, y)]);
            for (x, length) in [(0.22, 0.7), (0.58, 0.45)] {
                let top = match icon {
                    Icon::AlignTop => 0.16,
                    Icon::AlignCenterY => 0.5 - length / 2.0,
                    _ => 0.84 - length,
                };
                painter.rect_filled(Rect::from_min_max(at(x, top), at(x + 0.22, top + length)), 1.0, color);
            }
        }
        Icon::DistributeX => {
            // Three blocks with even gaps, marked by their centres.
            for x in [0.08, 0.4, 0.72] {
                painter.rect_filled(Rect::from_min_max(at(x, 0.3), at(x + 0.2, 0.7)), 1.0, color);
            }
            line(&[(0.02, 0.88), (0.98, 0.88)]);
        }
        Icon::DistributeY => {
            for y in [0.08, 0.4, 0.72] {
                painter.rect_filled(Rect::from_min_max(at(0.3, y), at(0.7, y + 0.2)), 1.0, color);
            }
            line(&[(0.88, 0.02), (0.88, 0.98)]);
        }
        Icon::MatchWidth => {
            // Two bars made the same width, between end marks.
            line(&[(0.05, 0.1), (0.05, 0.9)]);
            line(&[(0.95, 0.1), (0.95, 0.9)]);
            for y in [0.22, 0.58] {
                painter.rect_filled(Rect::from_min_max(at(0.12, y), at(0.88, y + 0.2)), 1.0, color);
            }
        }
        Icon::MatchHeight => {
            line(&[(0.1, 0.05), (0.9, 0.05)]);
            line(&[(0.1, 0.95), (0.9, 0.95)]);
            for x in [0.22, 0.58] {
                painter.rect_filled(Rect::from_min_max(at(x, 0.12), at(x + 0.2, 0.88)), 1.0, color);
            }
        }
        Icon::NoColor => {
            painter.circle_stroke(r.center(), side * 0.42, stroke);
            line(&[(0.2, 0.8), (0.8, 0.2)]);
        }
        Icon::Chevron => {
            line(&[(0.22, 0.38), (0.5, 0.66), (0.78, 0.38)]);
        }
    }
}

/// A small square button showing an icon, borderless until hovered, as in
/// Animate's panels. `selected` gives it the accent fill, for toggles.
pub struct IconButton<'a> {
    icon: Icon,
    hover: &'a str,
    selected: bool,
    size: f32,
}

impl<'a> IconButton<'a> {
    pub fn new(icon: Icon, hover: &'a str) -> Self {
        Self { icon, hover, selected: false, size: 24.0 }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for IconButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(vec2(self.size, self.size - 2.0), Sense::click());
        let enabled = ui.is_enabled();
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let fill = if self.selected {
                Some(theme::ACCENT_DIM)
            } else if response.is_pointer_button_down_on() {
                Some(theme::CONTROL_HOVER)
            } else if response.hovered() && enabled {
                Some(theme::CONTROL)
            } else {
                None
            };
            if let Some(fill) = fill {
                painter.rect_filled(rect, 4.0, fill);
            }
            let color = if !enabled {
                theme::TEXT_WEAK.gamma_multiply(0.5)
            } else if self.selected || response.hovered() {
                Color32::WHITE
            } else {
                theme::TEXT
            };
            paint(painter, self.icon, rect.shrink(self.size * 0.2), color);
        }
        response.on_hover_text(self.hover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_draws_inside_its_rectangle() {
        use Icon::*;
        let all = [
            Eye,
            EyeClosed,
            Lock,
            Unlock,
            Up,
            Down,
            Duplicate,
            Trash,
            First,
            Previous,
            Play,
            Pause,
            Next,
            Last,
            Loop,
            Onion,
            Speaker,
            SpeakerMuted,
            Stage,
            Timeline,
            Properties,
            Tools,
            Library,
            ExposureSheet,
            Graph,
            ToolSettings,
            Document,
            Folder,
            FolderOpen,
            Swatches,
            History,
            Camera,
            LightTable,
            Dial,
            Plus,
            AlignLeft,
            AlignCenterX,
            AlignRight,
            AlignTop,
            AlignCenterY,
            AlignBottom,
            DistributeX,
            DistributeY,
            MatchWidth,
            MatchHeight,
            NoColor,
            Chevron,
        ];
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(pos2(100.0, 100.0), Vec2::splat(16.0));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for icon in all {
                paint(ui.painter(), icon, rect, Color32::WHITE);
            }
        });
        output.textures_delta.clear();
        let shapes: Vec<_> = output.shapes.into_iter().map(|clipped| clipped.shape).collect();
        assert!(shapes.len() >= all.len());
        // Strokes reach a little past the outline, but nothing strays.
        for shape in shapes {
            assert!(rect.expand(3.0).contains_rect(shape.visual_bounding_rect()), "{shape:?}");
        }
    }
}
