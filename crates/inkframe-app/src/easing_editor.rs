//! The interactive easing curve in the properties panel.
//!
//! The graph shows one tween's timing: time runs left to right from the
//! tween's first keyframe to its last, and progress runs bottom to top from
//! the first keyframe's value to the next one's. A straight diagonal is a
//! constant speed. Wherever the curve is steep the value changes quickly,
//! and wherever it's flat the value barely moves.
//!
//! Every curve except Hold is a cubic Bézier from (0, 0) to (1, 1) with two
//! control points, the same model as CSS's `cubic-bezier()`. Those points are
//! drawn as handles that can be dragged. Dragging a preset's handle turns it
//! into a Custom curve that starts from the preset's shape.

use crate::i18n::tr;
use egui::{Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use inkframe_core::Easing;

const CURVE: Color32 = Color32::from_rgb(0x8a, 0x7c, 0xd8);
const HANDLE: Color32 = Color32::from_rgb(0xff, 0xb4, 0x4c);
const HANDLE_RADIUS: f32 = 5.0;
/// How far from a handle a press still grabs it, in screen points.
const GRAB_DISTANCE: f32 = 12.0;

/// Handles may be dragged this far above or below the graph's 0–1 range,
/// which already allows strong overshoot and anticipation.
const HANDLE_Y_LIMIT: (f32, f32) = (-1.0, 2.0);

/// One of the curve's two control points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Leaves the first keyframe: shapes how the tween starts.
    Out,
    /// Arrives at the second keyframe: shapes how the tween ends.
    In,
}

/// Draws the curve editor for one tween and lets its handles be dragged.
///
/// `progress` is how far the playhead is through the tween (0–1), shown as a
/// dot on the curve, or `None` when the playhead is elsewhere. Returns true
/// if the easing was changed.
pub fn show(ui: &mut egui::Ui, easing: &mut Easing, progress: Option<f32>) -> bool {
    let (rect, response) = ui.allocate_exact_size(graph_size(ui.available_width()), Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

    // The visible progress range grows to fit overshooting curves. While a
    // handle is being dragged it stays as it was when the drag started;
    // otherwise the graph would rescale under the pointer, and the handle
    // would slide away from it.
    let range_id = response.id.with("range");
    let range = match ui.data(|d| d.get_temp::<(f32, f32)>(range_id)) {
        Some(frozen) if response.dragged() => frozen,
        _ => visible_range(*easing),
    };
    let plot = Plot::inside(rect, range);
    let mut changed = false;

    // Grab the handle nearest the press, and keep it until release. egui
    // only reports a drag once the pointer has moved a few points, so look
    // up where the press began rather than where the pointer is now.
    let held_id = response.id.with("held");
    if response.drag_started()
        && let Some(pointer) = ui.input(|i| i.pointer.press_origin())
    {
        ui.data_mut(|d| {
            d.insert_temp(range_id, range);
            match nearest_handle(&plot, *easing, pointer) {
                Some(handle) => {
                    d.insert_temp(held_id, handle);
                }
                None => d.remove::<Handle>(held_id),
            }
        });
    }
    let held = ui.data(|d| d.get_temp::<Handle>(held_id));
    if response.dragged()
        && let (Some(handle), Some(pointer)) = (held, response.interact_pointer_pos())
    {
        let moved = with_handle(*easing, handle, plot.to_curve(pointer));
        changed = moved != *easing;
        *easing = moved;
    }
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<Handle>(held_id));
    }

    draw(ui, &painter, &plot, *easing, progress, held);
    let hovered = response.hover_pos().and_then(|p| nearest_handle(&plot, *easing, p));
    if hovered.is_some() || held.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    response.on_hover_text(tr("Drag a handle to reshape the curve"));
    changed
}

fn draw(
    ui: &egui::Ui,
    painter: &egui::Painter,
    plot: &Plot,
    easing: Easing,
    progress: Option<f32>,
    held: Option<Handle>,
) {
    let weak = ui.visuals().weak_text_color();
    let guide = Stroke::new(1.0, weak.gamma_multiply(0.35));

    // The 0 and 1 progress lines: the two keyframes' values.
    for v in [0.0, 1.0] {
        painter.line_segment([plot.to_screen(0.0, v), plot.to_screen(1.0, v)], guide);
    }
    // Constant speed, for comparison.
    painter.line_segment([plot.to_screen(0.0, 0.0), plot.to_screen(1.0, 1.0)], guide);

    let curve: Vec<Pos2> = (0..=64)
        .map(|i| {
            let t = i as f32 / 64.0;
            plot.to_screen(t, easing.apply(t))
        })
        .collect();
    painter.line(curve, Stroke::new(2.0, CURVE));

    if let Some([x1, y1, x2, y2]) = easing.control_points() {
        let arms = [
            (plot.to_screen(0.0, 0.0), plot.to_screen(x1, y1), Handle::Out),
            (plot.to_screen(1.0, 1.0), plot.to_screen(x2, y2), Handle::In),
        ];
        for (anchor, handle_at, handle) in arms {
            painter.line_segment([anchor, handle_at], Stroke::new(1.0, HANDLE.gamma_multiply(0.7)));
            let radius = if held == Some(handle) { HANDLE_RADIUS + 1.5 } else { HANDLE_RADIUS };
            painter.circle(handle_at, radius, HANDLE, Stroke::new(1.0, Color32::BLACK));
        }
    }
    for corner in [plot.to_screen(0.0, 0.0), plot.to_screen(1.0, 1.0)] {
        painter.circle_filled(corner, 3.0, CURVE);
    }

    // Where the playhead is in this tween.
    if let Some(t) = progress {
        let at = plot.to_screen(t, easing.apply(t));
        painter.line_segment([pos2(at.x, plot.rect.top()), pos2(at.x, plot.rect.bottom())], guide);
        painter.circle(at, 4.0, Color32::WHITE, Stroke::new(1.5, CURVE));
    }
}

/// The graph fills the panel's width, up to a point, at a 4:3 shape.
fn graph_size(available_width: f32) -> egui::Vec2 {
    let width = available_width.min(260.0);
    vec2(width, width * 0.75)
}

/// Maps between the curve's (time, progress) space and the screen.
struct Plot {
    rect: Rect,
    /// The progress values at the bottom and top of `rect`.
    range: (f32, f32),
}

impl Plot {
    /// The plot area inside the graph's background, inset so handles at the
    /// edges aren't cut off.
    fn inside(graph: Rect, range: (f32, f32)) -> Self {
        Self { rect: graph.shrink2(vec2(14.0, 12.0)), range }
    }

    fn to_screen(&self, t: f32, v: f32) -> Pos2 {
        let (low, high) = self.range;
        let y = (v - low) / (high - low);
        pos2(self.rect.left() + t * self.rect.width(), self.rect.bottom() - y * self.rect.height())
    }

    fn to_curve(&self, p: Pos2) -> (f32, f32) {
        let (low, high) = self.range;
        let t = (p.x - self.rect.left()) / self.rect.width();
        let y = (self.rect.bottom() - p.y) / self.rect.height();
        (t, low + y * (high - low))
    }
}

/// The progress range to show: 0–1 with a little room, widened to include
/// anything the curve or its handles reach outside that.
fn visible_range(easing: Easing) -> (f32, f32) {
    let mut low: f32 = 0.0;
    let mut high: f32 = 1.0;
    if let Some([_, y1, _, y2]) = easing.control_points() {
        // The curve never goes beyond its handles, so they bound it.
        low = low.min(y1).min(y2);
        high = high.max(y1).max(y2);
    }
    let margin = 0.12 * (high - low);
    (low - margin, high + margin)
}

/// The handle within grabbing distance of `pointer`, nearest first.
fn nearest_handle(plot: &Plot, easing: Easing, pointer: Pos2) -> Option<Handle> {
    let [x1, y1, x2, y2] = easing.control_points()?;
    [(Handle::Out, plot.to_screen(x1, y1)), (Handle::In, plot.to_screen(x2, y2))]
        .into_iter()
        .map(|(handle, at)| (handle, at.distance(pointer)))
        .filter(|(_, distance)| *distance <= GRAB_DISTANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| handle)
}

/// `easing` with one handle moved to `(t, v)`.
///
/// Time is kept within the tween (0–1). A handle further left or right would
/// make the curve double back on itself in time, meaning two progress values
/// for one frame. Progress may leave 0–1, which is how overshoot works.
/// Hold has no handles, so it's returned unchanged.
pub fn with_handle(easing: Easing, handle: Handle, (t, v): (f32, f32)) -> Easing {
    let Some([mut x1, mut y1, mut x2, mut y2]) = easing.control_points() else { return easing };
    let t = t.clamp(0.0, 1.0);
    let v = v.clamp(HANDLE_Y_LIMIT.0, HANDLE_Y_LIMIT.1);
    match handle {
        Handle::Out => (x1, y1) = (t, v),
        Handle::In => (x2, y2) = (t, v),
    }
    Easing::CubicBezier { x1, y1, x2, y2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Plot {
        Plot { rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0)), range: (-0.5, 1.5) }
    }

    #[test]
    fn plot_round_trips() {
        let plot = plot();
        let (t, v) = plot.to_curve(plot.to_screen(0.25, 1.2));
        assert!((t - 0.25).abs() < 1e-5 && (v - 1.2).abs() < 1e-5);
        // Progress goes up the screen, so 1 is above 0.
        assert!(plot.to_screen(0.0, 1.0).y < plot.to_screen(0.0, 0.0).y);
    }

    #[test]
    fn dragging_a_preset_makes_a_custom_curve_from_it() {
        // Ease In is cubic-bezier(0.42, 0, 1, 1). Moving its first handle
        // keeps the second where the preset had it.
        let moved = with_handle(Easing::EaseIn, Handle::Out, (0.3, 0.1));
        assert_eq!(moved, Easing::CubicBezier { x1: 0.3, y1: 0.1, x2: 1.0, y2: 1.0 });
    }

    #[test]
    fn handles_stay_within_the_tween_in_time_but_can_overshoot() {
        let moved = with_handle(Easing::Linear, Handle::In, (1.4, 1.3));
        assert_eq!(moved, Easing::CubicBezier { x1: 0.0, y1: 0.0, x2: 1.0, y2: 1.3 });
        let moved = with_handle(Easing::Linear, Handle::Out, (-0.2, 9.0));
        assert_eq!(moved, Easing::CubicBezier { x1: 0.0, y1: 2.0, x2: 1.0, y2: 1.0 });
    }

    #[test]
    fn hold_has_no_handles() {
        assert_eq!(with_handle(Easing::Hold, Handle::Out, (0.5, 0.5)), Easing::Hold);
        assert_eq!(nearest_handle(&plot(), Easing::Hold, pos2(0.0, 0.0)), None);
    }

    #[test]
    fn overshooting_curves_widen_the_view() {
        let back_out = Easing::CubicBezier { x1: 0.34, y1: 1.56, x2: 0.64, y2: 1.0 };
        let (low, high) = visible_range(back_out);
        assert!(low < 0.0 && high > 1.56);
    }

    // ---- Interaction: drag a handle with simulated mouse input.

    use egui_kittest::Harness;

    /// The easing being edited, and where the graph was drawn.
    type State = (Easing, Rect);

    fn harness(easing: Easing) -> Harness<'static, State> {
        let mut harness =
            Harness::builder().with_size(vec2(300.0, 300.0)).with_step_dt(1.0 / 60.0).build_ui_state(
                |ui, (easing, graph): &mut State| {
                    *graph = Rect::from_min_size(ui.next_widget_position(), graph_size(ui.available_width()));
                    show(ui, easing, None);
                },
                (easing, Rect::NOTHING),
            );
        harness.run();
        harness
    }

    #[test]
    fn dragging_a_handle_reshapes_the_curve() {
        let mut h = harness(Easing::Linear);
        // Measure the plot before the drag; it's frozen while dragging anyway.
        let plot = Plot::inside(h.state().1, visible_range(Easing::Linear));

        // Linear's first handle sits on the start keyframe, at (0, 0).
        let from = plot.to_screen(0.0, 0.0);
        let to = plot.to_screen(0.5, 0.0);
        h.drag_at(from);
        h.step();
        for step in 1..=4 {
            h.hover_at(from + (to - from) * (step as f32 / 4.0));
            h.step();
        }
        h.drop_at(to);
        h.run();

        let Easing::CubicBezier { x1, y1, x2, y2 } = h.state().0 else { panic!("still a preset") };
        assert!((x1 - 0.5).abs() < 0.02 && y1.abs() < 0.02, "handle at ({x1}, {y1})");
        assert_eq!((x2, y2), (1.0, 1.0), "the other handle shouldn't move");
    }

    #[test]
    fn presses_grab_the_nearest_handle() {
        let plot = plot();
        let easing = Easing::EaseInOut; // handles at (0.42, 0) and (0.58, 1)
        let near_out = plot.to_screen(0.42, 0.0) + vec2(3.0, -2.0);
        assert_eq!(nearest_handle(&plot, easing, near_out), Some(Handle::Out));
        let near_in = plot.to_screen(0.58, 1.0) + vec2(-4.0, 4.0);
        assert_eq!(nearest_handle(&plot, easing, near_in), Some(Handle::In));
        assert_eq!(nearest_handle(&plot, easing, plot.to_screen(0.5, 0.5)), None);
    }
}
