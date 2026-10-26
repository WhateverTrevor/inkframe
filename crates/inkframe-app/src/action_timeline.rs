//! A smart bone's action timeline, as Moho's: the bone's turns laid out
//! along a strip, with a lane for each thing it does as it turns.
//!
//! Moho edits a smart bone's actions on a timeline of their own, whose
//! frames are the bone's angles. Here the strip runs from one end of the
//! bone's turns to the other (its limits, or far enough either way to show
//! every action), with rest, 0°, marked. Dragging along it scrubs: the
//! bone is shown at that turn (a preview, see `Editor::preview_turn`) with
//! everything its actions do, and nothing is keyed. In the lanes below:
//!
//! * a reshape is a band from rest to the turn it's reached at, and a
//!   switch a mark where it flips, with the turns beyond shaded; drag their
//!   handles to change the turn;
//! * a correction's keys are diamonds (rest, hollow, at 0°): drag one to
//!   key it at another turn, double-click it to redraw the shapes at its
//!   turn, or right-click it for a menu.
//!
//! The strip only reads and changes the bone's actions (passed in, to be
//! written back by the Properties section) and says what's asked for; the
//! section does the asking, so corrections start the same way from here as
//! from its buttons.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use inkframe_core::{RotationLimits, SmartAction};

use crate::i18n::{tr, tr_args};
use crate::theme;

/// What the person asked of the strip this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Asked {
    /// Show the bone at this turn.
    Preview(f32),
    /// Show it as animated again.
    EndPreview,
    /// Redraw correction `action`'s shapes at `turn` (shown there first).
    Edit { action: usize, turn: f32 },
}

const RULER: f32 = 18.0;
const LANE: f32 = 18.0;
/// Room each side of the strip, so a handle at the very end can be grabbed.
const INSET: f32 = 8.0;

/// The turns the strip spans: the bone's limits, or far enough either way
/// to show every action and where the bone is now, in steps of 45°.
pub fn span(actions: &[SmartAction], limits: Option<RotationLimits>, now: f32) -> (f32, f32) {
    if let Some(limits) = limits {
        return (limits.min.min(0.0), limits.max.max(0.0));
    }
    let furthest = actions
        .iter()
        .flat_map(|action| match action {
            SmartAction::Morph { turn, .. } | SmartAction::Switch { turn, .. } => vec![*turn],
            SmartAction::Correct { keys, .. } => keys.iter().map(|k| k.turn).collect(),
        })
        .chain([now])
        .fold(0.0_f32, |far, turn| far.max(turn.abs()));
    let reach = ((furthest + 20.0) / 45.0).ceil().max(2.0) * 45.0;
    (-reach.min(180.0), reach.min(180.0))
}

/// Draws the strip for a bone with `actions` (whose turns it may change),
/// shown at `now` degrees, `previewing` if that's a preview. `names` are
/// the lanes' labels, an action each.
pub fn show(
    ui: &mut egui::Ui,
    actions: &mut [SmartAction],
    names: &[String],
    (low, high): (f32, f32),
    now: f32,
    previewing: bool,
) -> Option<Asked> {
    let width = ui.available_width().max(120.0);
    let height = RULER + LANE * actions.len() as f32 + 4.0;
    let (rect, strip) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let strip = strip.on_hover_text(tr("Drag along to see the bone at any turn, without keying it"));
    let painter = ui.painter_at(rect.expand(2.0));
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + INSET, rect.top()),
        Pos2::new(rect.right() - INSET, rect.bottom()),
    );
    let x_of = |turn: f32| inner.left() + (turn - low) / (high - low) * inner.width();
    let turn_of = |x: f32| (low + (x - inner.left()) / inner.width() * (high - low)).clamp(low, high).round();
    let weak = ui.visuals().weak_text_color();
    let font = FontId::proportional(10.0);
    let mut asked = None;

    painter.rect_filled(rect, 4.0, theme::SUNKEN);
    // The ruler: a tick every 15°, numbered every 45° (or 90°, if that
    // would crowd the numbers), rest marked.
    let numbered = if x_of(45.0) - x_of(0.0) >= 34.0 { 45.0 } else { 90.0 };
    let mut tick = (low / 15.0).ceil() * 15.0;
    while tick <= high + 0.01 {
        let x = x_of(tick);
        let major = (tick % numbered).abs() < 0.01;
        let top = rect.top() + if major { 2.0 } else { 9.0 };
        painter.line_segment(
            [Pos2::new(x, top), Pos2::new(x, rect.top() + RULER - 2.0)],
            Stroke::new(1.0, theme::LINE),
        );
        if major {
            painter.text(
                Pos2::new(x + 2.0, rect.top() + 1.0),
                Align2::LEFT_TOP,
                format!("{tick:.0}°"),
                font.clone(),
                weak,
            );
        }
        tick += 15.0;
    }
    let rest = x_of(0.0);
    painter.line_segment(
        [Pos2::new(rest, rect.top() + RULER), Pos2::new(rest, rect.bottom())],
        Stroke::new(1.0, theme::LINE),
    );

    // Each action's lane. Handles are interacted with before the strip's
    // own drag, which scrubs, so grabbing one doesn't also scrub.
    let accent = theme::ACCENT;
    for (i, action) in actions.iter_mut().enumerate() {
        let top = rect.top() + RULER + i as f32 * LANE;
        let lane = Rect::from_min_max(Pos2::new(rect.left(), top), Pos2::new(rect.right(), top + LANE));
        let middle = lane.center().y;
        let label = names.get(i).map(String::as_str).unwrap_or("");
        let id = ui.id().with(("action lane", i));
        match action {
            SmartAction::Morph { turn, .. } => {
                let (from, to) = (rest.min(x_of(*turn)), rest.max(x_of(*turn)));
                let band = Rect::from_min_max(Pos2::new(from, middle - 4.0), Pos2::new(to, middle + 4.0));
                painter.rect_filled(band, 2.0, accent.gamma_multiply(0.35));
                if let Some(new) =
                    drag_handle(ui, id, Pos2::new(x_of(*turn), middle), &turn_of, accent, Handle::Round)
                {
                    *turn = new;
                }
            }
            SmartAction::Switch { turn, .. } => {
                // Past the turn, the other layer shows.
                let x = x_of(*turn);
                let far = if *turn >= 0.0 { inner.right() } else { inner.left() };
                let beyond = Rect::from_x_y_ranges(x.min(far)..=x.max(far), middle - 4.0..=middle + 4.0);
                painter.rect_filled(beyond, 2.0, accent.gamma_multiply(0.25));
                if let Some(new) = drag_handle(ui, id, Pos2::new(x, middle), &turn_of, accent, Handle::Bar) {
                    *turn = new;
                }
            }
            SmartAction::Correct { keys, .. } => {
                diamond(&painter, Pos2::new(rest, middle), weak, false);
                let mut moved = None;
                let mut removed = None;
                for (k, key) in keys.iter().enumerate() {
                    let at = Pos2::new(x_of(key.turn), middle);
                    let response = ui
                        .interact(
                            Rect::from_center_size(at, Vec2::splat(12.0)),
                            id.with(k),
                            Sense::click_and_drag(),
                        )
                        .on_hover_text(tr_args(
                            "Keyed at {turn}°: drag to move, double-click to redraw",
                            &[("turn", &format!("{:.0}", key.turn))],
                        ));
                    let lit = response.hovered() || response.dragged();
                    if response.dragged()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        moved = Some((k, turn_of(pointer.x)));
                    }
                    if response.double_clicked() {
                        asked = Some(Asked::Edit { action: i, turn: key.turn });
                    }
                    response.context_menu(|ui| {
                        if ui
                            .button(tr_args("Redraw at {turn}°", &[("turn", &format!("{:.0}", key.turn))]))
                            .clicked()
                        {
                            asked = Some(Asked::Edit { action: i, turn: key.turn });
                            ui.close();
                        }
                        if ui.button(tr("Remove Key")).clicked() {
                            removed = Some(k);
                            ui.close();
                        }
                    });
                    diamond(&painter, at, if lit { Color32::WHITE } else { accent }, true);
                }
                // A key can't land on rest, or on another key.
                if let Some((k, turn)) = moved
                    && turn.abs() >= 1.0
                    && keys.iter().enumerate().all(|(j, other)| j == k || (other.turn - turn).abs() >= 1.0)
                {
                    keys[k].turn = turn;
                    keys.sort_by(|a, b| a.turn.total_cmp(&b.turn));
                }
                if let Some(k) = removed {
                    keys.remove(k);
                }
            }
        }
        painter.text(Pos2::new(rect.left() + 4.0, middle), Align2::LEFT_CENTER, label, font.clone(), weak);
    }

    // The playhead: where the bone's shown now.
    let x = x_of(now.clamp(low, high));
    let colour = if previewing { theme::ACCENT } else { Color32::from_rgb(0xe5, 0x48, 0x4d) };
    painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(2.0, colour));
    painter.circle_filled(Pos2::new(x, rect.top() + 3.0), 3.5, colour);

    // Dragging or clicking anywhere else scrubs.
    if (strip.dragged() || strip.clicked())
        && let Some(pointer) = strip.interact_pointer_pos()
    {
        asked = asked.or(Some(Asked::Preview(turn_of(pointer.x))));
    }
    ui.horizontal(|ui| {
        if previewing {
            ui.label(
                egui::RichText::new(tr_args("Previewing {turn}°", &[("turn", &format!("{now:.0}"))]))
                    .color(theme::ACCENT),
            );
            if ui.small_button(tr("Back to the animation")).clicked() {
                asked = Some(Asked::EndPreview);
            }
        } else {
            ui.weak(tr_args("Turned {turn}° on this frame", &[("turn", &format!("{now:.0}"))]));
        }
    });
    asked
}

#[derive(Clone, Copy)]
enum Handle {
    Round,
    Bar,
}

/// A handle at `at` that can be dragged along the strip; returns its new
/// turn while it's dragged.
fn drag_handle(
    ui: &egui::Ui,
    id: egui::Id,
    at: Pos2,
    turn_of: &dyn Fn(f32) -> f32,
    colour: Color32,
    kind: Handle,
) -> Option<f32> {
    let response = ui
        .interact(Rect::from_center_size(at, Vec2::new(12.0, 16.0)), id, Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let lit = response.hovered() || response.dragged();
    let fill = if lit { Color32::WHITE } else { colour };
    let painter = ui.painter();
    match kind {
        Handle::Round => {
            painter.circle(at, 5.0, fill, Stroke::new(1.0, theme::SUNKEN));
        }
        Handle::Bar => {
            let bar = Rect::from_center_size(at, Vec2::new(4.0, 14.0));
            painter.rect(bar, 1.0, fill, Stroke::new(1.0, theme::SUNKEN), StrokeKind::Outside);
        }
    }
    response.dragged().then(|| response.interact_pointer_pos()).flatten().map(|p| turn_of(p.x))
}

fn diamond(painter: &egui::Painter, at: Pos2, colour: Color32, filled: bool) {
    let r = 5.0;
    let points = vec![
        Pos2::new(at.x, at.y - r),
        Pos2::new(at.x + r, at.y),
        Pos2::new(at.x, at.y + r),
        Pos2::new(at.x - r, at.y),
    ];
    let fill = if filled { colour } else { Color32::TRANSPARENT };
    painter.add(Shape::convex_polygon(points, fill, Stroke::new(1.5, colour)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{CorrectionKey, Document, LayerId, Transform};

    fn key(turn: f32) -> CorrectionKey {
        CorrectionKey { turn, colour_art: Vec::new(), line_art: Vec::new() }
    }

    fn a_layer() -> LayerId {
        Document::default().add_layer("Arm", Vec::new(), 0, Transform::default())
    }

    #[test]
    fn the_strip_spans_the_limits_or_every_action() {
        let limits = RotationLimits { min: -100.0, max: 20.0 };
        assert_eq!(span(&[], Some(limits), 0.0), (-100.0, 20.0));
        // At least 90° either way, in steps of 45°, past the furthest turn.
        assert_eq!(span(&[], None, 0.0), (-90.0, 90.0));
        let layer = a_layer();
        let actions = [SmartAction::Correct { layer, drawing: 0, keys: vec![key(-30.0), key(110.0)] }];
        assert_eq!(span(&actions, None, 0.0), (-135.0, 135.0));
        // Never past a whole turn.
        assert_eq!(span(&actions, None, 400.0), (-180.0, 180.0));
    }

    #[test]
    fn dragging_along_the_strip_previews_and_keys_can_be_moved() {
        use egui_kittest::Harness;

        let layer = a_layer();
        type State = (Vec<SmartAction>, Option<Asked>, Pos2);
        let state: State =
            (vec![SmartAction::Correct { layer, drawing: 0, keys: vec![key(45.0)] }], None, Pos2::ZERO);
        let mut h = Harness::new_ui_state(
            |ui, (actions, asked, corner): &mut State| {
                ui.set_width(380.0);
                *corner = ui.cursor().min;
                if let Some(a) = show(ui, actions, &["Correct Arm".into()], (-90.0, 90.0), 0.0, false) {
                    *asked = Some(a);
                }
            },
            state,
        );
        h.set_size(Vec2::new(420.0, 140.0));
        h.run();
        // The strip runs across -90..90°, inset from its sides.
        let corner = h.state().2;
        let (left, right) = (corner.x + INSET, corner.x + 380.0 - INSET);
        let x_of = |turn: f32| left + (turn + 90.0) / 180.0 * (right - left);
        let ruler = corner.y + RULER / 2.0;
        h.drag_at(Pos2::new(x_of(30.0), ruler));
        h.run();
        h.drop_at(Pos2::new(x_of(30.0), ruler));
        h.run();
        match h.state().1 {
            Some(Asked::Preview(turn)) => assert!((turn - 30.0).abs() <= 1.0, "{turn}"),
            other => panic!("{other:?}"),
        }
        // Drag the key from 45° to -60°.
        let lane = corner.y + RULER + LANE / 2.0;
        h.drag_at(Pos2::new(x_of(45.0), lane));
        h.run();
        h.hover_at(Pos2::new(x_of(-60.0), lane));
        h.run();
        h.drop_at(Pos2::new(x_of(-60.0), lane));
        h.run();
        let SmartAction::Correct { keys, .. } = &h.state().0[0] else { unreachable!() };
        assert!((keys[0].turn + 60.0).abs() <= 1.0, "{}", keys[0].turn);
    }
}
