//! The value graph: a property's value plotted over time, like After
//! Effects' graph editor or Animate's Motion Editor.
//!
//! It shares the timeline's frame spacing and scrolling, so frame 12 on the
//! graph sits right under frame 12 on the ruler. Up is a bigger value.
//! Position and scale have an x and a y, drawn as two curves.
//!
//! Everything shown here is the same data the rest of the editor uses:
//!
//! * the dots are keyframes: drag one up or down to change its value, or
//!   sideways to move it to another frame;
//! * the handles are the tween's easing (a CSS-style cubic Bézier, see
//!   `inkframe_core::Easing`), drawn in real values instead of the 0–1
//!   square the properties panel's curve editor uses. Dragging one changes
//!   that tween's easing.
//!
//! A tween's easing is shared by its x and y, so with both curves moving,
//! either curve's handles can shape it.

use crate::i18n::tr;
use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Stroke, pos2};
use inkframe_core::{Easing, Layer, Point, Property, Track};

use crate::editor::Editor;

/// x and y (or a property's only value) in their usual colors: red and green
/// for the two axes, as in most 3D and motion apps.
const COLORS: [Color32; 2] = [Color32::from_rgb(0xf0, 0x5a, 0x5a), Color32::from_rgb(0x6a, 0xd0, 0x6a)];
const KEY_RADIUS: f32 = 4.0;
const HANDLE_RADIUS: f32 = 3.5;
const GRAB: f32 = 8.0;

/// Maps between frames and screen x, provided by the timeline so the graph
/// lines up with its ruler.
pub struct FrameAxis<'a> {
    pub x_of: &'a dyn Fn(f32) -> f32,
    pub frame_of: &'a dyn Fn(f32) -> f32,
}

#[derive(Default)]
pub struct ValueGraph {
    drag: Option<Drag>,
    /// The value range shown, kept still while dragging so the curve
    /// doesn't rescale under the pointer.
    frozen_range: Option<(f32, f32)>,
}

#[derive(Clone, Copy)]
enum Drag {
    /// A key, by its current frame (it changes as the key is moved), and
    /// which of the property's values was grabbed.
    Key { frame: u32, component: usize },
    /// One easing handle: the tween's starting frame, `out` for the handle
    /// leaving that key (false: arriving at the next), and the component
    /// whose curve it was grabbed on.
    Handle { from: u32, out: bool, component: usize },
}

impl ValueGraph {
    pub fn is_busy(&self) -> bool {
        self.drag.is_some()
    }

    /// Draws the graph of the selected layer's focused property in `area`,
    /// and handles dragging its keys and handles.
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        response: &Response,
        area: Rect,
        editor: &mut Editor,
        axis: FrameAxis,
    ) {
        let painter = ui.painter_at(area);
        let property = editor.focused_property;
        let Some(layer) = editor.selected_layer() else {
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                tr("Select a layer to see its animation as a graph"),
                FontId::proportional(13.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };
        let curves = Curves::of(layer, property);

        // The value range: everything the curves and handles reach in the
        // visible frames, with some room, unless frozen by a drag.
        let first_frame = (axis.frame_of)(area.left()).floor().max(0.0);
        let last_frame = (axis.frame_of)(area.right()).ceil();
        let range = match (self.frozen_range, self.drag) {
            (Some(range), Some(_)) => range,
            _ => curves.range(first_frame, last_frame),
        };
        let plot = Plot { area: area.shrink2(egui::vec2(0.0, 14.0)), range, axis: &axis };

        self.handle_input(ui, response, editor, &plot, &curves, range);
        let Some(layer) = editor.selected_layer() else { return };
        let curves = Curves::of(layer, property);
        draw(ui, &painter, &plot, &curves, editor.frame, first_frame, last_frame);
    }

    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &Response,
        editor: &mut Editor,
        plot: &Plot,
        curves: &Curves,
        range: (f32, f32),
    ) {
        if response.drag_started()
            && let Some(press) = ui.input(|i| i.pointer.press_origin())
            && plot.area.expand(GRAB).contains(press)
        {
            self.drag = hit(plot, curves, press);
            self.frozen_range = self.drag.map(|_| range);
        }
        if response.drag_stopped() {
            self.drag = None;
            self.frozen_range = None;
        }
        let (Some(drag), Some(pointer)) = (self.drag, response.interact_pointer_pos()) else { return };
        let property = editor.focused_property;
        let Some(id) = editor.selection else { return };
        let Some(layer) = editor.doc.layer_mut(id) else { return };
        let track = &mut layer.track;

        match drag {
            Drag::Key { frame, component } => {
                // Sideways: move the key to the frame under the pointer,
                // if that frame's free. Up and down: change its value.
                let to = (plot.axis.frame_of)(pointer.x).round().max(0.0) as u32;
                let frame =
                    if to != frame && track.channel_mut(property).move_key(frame, to) { to } else { frame };
                set_component(track, property, frame, component, plot.value_at(pointer.y));
                self.drag = Some(Drag::Key { frame, component });
            }
            Drag::Handle { from, out, component } => {
                let Some(tween) = track.channel(property).tween_at(from) else { return };
                let (start, end) = (
                    curves_value(track, property, tween.start, component),
                    curves_value(track, property, tween.end, component),
                );
                let span = (tween.end - tween.start) as f32;
                // The handle's spot as a fraction of the tween: across in
                // time, up in value (0 at the first key's value, 1 at the
                // next's). That's exactly an easing curve's control point.
                let t = ((plot.axis.frame_of)(pointer.x) - tween.start as f32) / span;
                let v = (plot.value_at(pointer.y) - start) / (end - start);
                let [mut x1, mut y1, mut x2, mut y2] =
                    shown_points(tween.easing).unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let (t, v) = (t.clamp(0.0, 1.0), v.clamp(-1.0, 2.0));
                if out {
                    (x1, y1) = (t, v)
                } else {
                    (x2, y2) = (t, v)
                }
                if let Some(easing) = track.channel_mut(property).tween_easing_mut(from) {
                    *easing = Easing::CubicBezier { x1, y1, x2, y2 };
                }
            }
        }
    }
}

/// Where things are on screen: frames across (from the timeline), values up.
struct Plot<'a> {
    area: Rect,
    range: (f32, f32),
    axis: &'a FrameAxis<'a>,
}

impl Plot<'_> {
    fn y_of(&self, value: f32) -> f32 {
        let (low, high) = self.range;
        self.area.bottom() - (value - low) / (high - low) * self.area.height()
    }

    fn value_at(&self, y: f32) -> f32 {
        let (low, high) = self.range;
        low + (self.area.bottom() - y) / self.area.height() * (high - low)
    }

    fn at(&self, frame: f32, value: f32) -> Pos2 {
        pos2((self.axis.x_of)(frame), self.y_of(value))
    }
}

/// A property's curves: its keys and how to sample it, as plain numbers.
/// Scale and opacity are shown as percentages, as in the properties panel.
struct Curves {
    names: &'static [&'static str],
    /// Each key's frame, values, and the easing to the next key.
    keys: Vec<(u32, Vec<f32>, Easing)>,
    samples: Box<dyn Fn(f32) -> Vec<f32>>,
}

impl Curves {
    fn of(layer: &Layer, property: Property) -> Self {
        let track = layer.track.clone();
        let channel = layer.track.channel(property);
        let keys = channel
            .key_frames()
            .into_iter()
            .map(|frame| {
                let easing =
                    channel.tween_at(frame).filter(|t| t.start == frame).map_or(Easing::Linear, |t| t.easing);
                (frame, values(&track, property, frame as f32), easing)
            })
            .collect();
        let names: &'static [&'static str] = match property {
            Property::Position => &["x", "y"],
            Property::Rotation => &["°"],
            Property::Scale => &["x %", "y %"],
            Property::Skew => &["x °", "y °"],
            Property::Opacity => &["%"],
            // Effects are settings, not numbers along a curve.
            Property::Effects => &[],
            Property::Depth => &["px"],
        };
        Self { names, keys, samples: Box::new(move |frame| values(&track, property, frame)) }
    }

    /// The smallest and largest value shown between two frames, with a
    /// little room above and below (and some height for a flat line).
    fn range(&self, first: f32, last: f32) -> (f32, f32) {
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        let mut take = |v: f32| {
            low = low.min(v);
            high = high.max(v);
        };
        let mut frame = first;
        while frame <= last {
            (self.samples)(frame).into_iter().for_each(&mut take);
            frame += 1.0;
        }
        for (_, values, _) in &self.keys {
            values.iter().copied().for_each(&mut take);
        }
        if low > high {
            return (0.0, 1.0);
        }
        let room = ((high - low) * 0.1).max(1.0);
        (low - room, high + room)
    }
}

/// A property's values at a time: [x, y] for position and scale, one value
/// otherwise. Scale and opacity are percentages.
fn values(track: &Track, property: Property, frame: f32) -> Vec<f32> {
    let pose = track.sample(frame);
    match property {
        Property::Position => vec![pose.position.x, pose.position.y],
        Property::Rotation => vec![track.rotation.sample(frame).unwrap_or_default()],
        Property::Scale => vec![pose.scale.x * 100.0, pose.scale.y * 100.0],
        Property::Skew => vec![pose.skew.x, pose.skew.y],
        Property::Opacity => vec![pose.opacity * 100.0],
        Property::Effects => Vec::new(),
        Property::Depth => vec![track.depth_at(frame)],
    }
}

fn curves_value(track: &Track, property: Property, frame: u32, component: usize) -> f32 {
    values(track, property, frame as f32).get(component).copied().unwrap_or_default()
}

/// Sets one value of the key at `frame`, as shown on the graph.
fn set_component(track: &mut Track, property: Property, frame: u32, component: usize, value: f32) {
    match property {
        Property::Position => {
            let mut p = track.position.sample(frame as f32).unwrap_or(Point::ZERO);
            if component == 0 {
                p.x = value
            } else {
                p.y = value
            }
            track.position.set(frame, p);
        }
        Property::Rotation => track.rotation.set(frame, value),
        Property::Scale => {
            let mut s = track.scale.sample(frame as f32).unwrap_or(Point::new(1.0, 1.0));
            if component == 0 {
                s.x = value / 100.0
            } else {
                s.y = value / 100.0
            }
            track.scale.set(frame, s);
        }
        Property::Skew => {
            let mut k = track.skew.sample(frame as f32).unwrap_or(Point::ZERO);
            let value = value.clamp(-89.0, 89.0);
            if component == 0 {
                k.x = value
            } else {
                k.y = value
            }
            track.skew.set(frame, k);
        }
        Property::Opacity => track.opacity.set(frame, (value / 100.0).clamp(0.0, 1.0)),
        Property::Effects => {}
        Property::Depth => track.depth.set(frame, value),
    }
}

/// An easing's control points as the graph shows them.
///
/// Linear's own points, (0, 0) and (1, 1), sit right on the keys, where they
/// couldn't be told apart from them or grabbed. (⅓, ⅓) and (⅔, ⅔) describe
/// exactly the same straight line, so linear tweens show those instead.
fn shown_points(easing: Easing) -> Option<[f32; 4]> {
    match easing {
        Easing::Linear => Some([1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0]),
        other => other.control_points(),
    }
}

/// The easing handles of the tween from key `i` to key `i + 1`, on one
/// component's curve, as screen points (leaving, arriving). `None` if the
/// tween holds, or that value doesn't change during it (a flat line has no
/// height to put a handle at).
fn handles(plot: &Plot, curves: &Curves, i: usize, component: usize) -> Option<(Pos2, Pos2)> {
    let (from, start, easing) = &curves.keys[i];
    let (to, end, _) = curves.keys.get(i + 1)?;
    let [x1, y1, x2, y2] = shown_points(*easing)?;
    let (a, b) = (start[component], end[component]);
    if (b - a).abs() < 1e-4 {
        return None;
    }
    let span = (to - from) as f32;
    let at = |x: f32, y: f32| plot.at(*from as f32 + x * span, a + y * (b - a));
    Some((at(x1, y1), at(x2, y2)))
}

/// What's under the pointer. Keys come first: a handle that happens to sit
/// on its key (ease in's arriving handle, say) has no effect anyway.
fn hit(plot: &Plot, curves: &Curves, pointer: Pos2) -> Option<Drag> {
    for component in 0..curves.names.len() {
        for (frame, values, _) in &curves.keys {
            if plot.at(*frame as f32, values[component]).distance(pointer) <= GRAB {
                return Some(Drag::Key { frame: *frame, component });
            }
        }
    }
    for component in 0..curves.names.len() {
        for i in 0..curves.keys.len() {
            if let Some((out, into)) = handles(plot, curves, i, component) {
                let from = curves.keys[i].0;
                if out.distance(pointer) <= GRAB {
                    return Some(Drag::Handle { from, out: true, component });
                }
                if into.distance(pointer) <= GRAB {
                    return Some(Drag::Handle { from, out: false, component });
                }
            }
        }
    }
    None
}

fn draw(
    ui: &egui::Ui,
    painter: &egui::Painter,
    plot: &Plot,
    curves: &Curves,
    playhead: u32,
    first_frame: f32,
    last_frame: f32,
) {
    let weak = ui.visuals().weak_text_color();
    let grid = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6));

    // Value lines at round numbers: a step of 1, 2 or 5 times a power of
    // ten, chosen so there are roughly five of them.
    let (low, high) = plot.range;
    let rough = (high - low) / 5.0;
    let power = 10f32.powf(rough.log10().floor());
    let step =
        [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * power).find(|s| *s >= rough).unwrap_or(power * 10.0);
    let mut value = (low / step).ceil() * step;
    while value <= high {
        let y = plot.y_of(value);
        painter.hline(plot.area.x_range(), y, grid);
        let label = if step >= 1.0 { format!("{value:.0}") } else { format!("{value:.2}") };
        painter.text(
            pos2(plot.area.left() + 4.0, y - 2.0),
            Align2::LEFT_BOTTOM,
            label,
            FontId::proportional(10.0),
            weak,
        );
        value += step;
    }

    // The playhead.
    let x = (plot.axis.x_of)(playhead as f32);
    painter.vline(x, plot.area.y_range(), Stroke::new(1.0, Color32::from_rgb(0xe8, 0x3a, 0x3a)));

    // The curves, sampled a few times per frame so eases look smooth.
    for component in 0..curves.names.len() {
        let color = COLORS[component % COLORS.len()];
        let mut points = Vec::new();
        let mut frame = first_frame;
        while frame <= last_frame {
            let value = (curves.samples)(frame)[component];
            points.push(plot.at(frame, value));
            frame += 0.25;
        }
        painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));

        for i in 0..curves.keys.len() {
            if let Some((out, into)) = handles(plot, curves, i, component) {
                let (from, to) = (&curves.keys[i], &curves.keys[i + 1]);
                let line = Stroke::new(1.0, color.gamma_multiply(0.6));
                let anchors =
                    [plot.at(from.0 as f32, from.1[component]), plot.at(to.0 as f32, to.1[component])];
                for (anchor, handle) in anchors.into_iter().zip([out, into]) {
                    // A handle on top of its key would only hide the key.
                    if anchor.distance(handle) > KEY_RADIUS {
                        painter.line_segment([anchor, handle], line);
                        painter.circle(
                            handle,
                            HANDLE_RADIUS,
                            Color32::from_gray(30),
                            Stroke::new(1.0, color),
                        );
                    }
                }
            }
        }
        for (frame, values, _) in &curves.keys {
            painter.circle(
                plot.at(*frame as f32, values[component]),
                KEY_RADIUS,
                color,
                Stroke::new(1.0, Color32::BLACK),
            );
        }
    }
}

/// The names of the curves a property draws, for the legend.
pub fn curve_names(property: Property) -> &'static [&'static str] {
    match property {
        Property::Position => &["x", "y"],
        Property::Rotation => &["rotation"],
        Property::Scale => &["x scale", "y scale"],
        Property::Skew => &["x skew", "y skew"],
        Property::Opacity => &["opacity"],
        Property::Effects => &[],
        Property::Depth => &["depth"],
    }
}

pub fn curve_color(component: usize) -> Color32 {
    COLORS[component % COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use inkframe_core::{Document, Transform};

    fn moving_layer() -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("A", vec![], 0, Transform::at(Point::new(0.0, 100.0)));
        doc.layer_mut(id).unwrap().track.position.set(10, Point::new(200.0, 100.0));
        doc.layer(id).unwrap().clone()
    }

    #[test]
    fn curves_list_keys_and_sample_values() {
        let curves = Curves::of(&moving_layer(), Property::Position);
        assert_eq!(curves.keys.len(), 2);
        assert_eq!(curves.keys[1].1, [200.0, 100.0]);
        assert_eq!((curves.samples)(5.0), [100.0, 100.0]);
        // The range covers 0–200 with room, and the flat y as well.
        let (low, high) = curves.range(0.0, 10.0);
        assert!(low < 0.0 && high > 200.0);
    }

    #[test]
    fn handles_appear_only_where_the_value_changes() {
        let curves = Curves::of(&moving_layer(), Property::Position);
        let axis = FrameAxis { x_of: &|f| f * 10.0, frame_of: &|x| x / 10.0 };
        let plot = Plot {
            area: Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
            range: (0.0, 200.0),
            axis: &axis,
        };
        assert!(handles(&plot, &curves, 0, 0).is_some(), "x changes, so it has handles");
        assert!(handles(&plot, &curves, 0, 1).is_none(), "y is flat");
    }

    #[test]
    fn dragging_a_key_up_and_down_changes_its_value() {
        use egui_kittest::Harness;
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("A", vec![], 0, Transform::at(Point::new(0.0, 100.0)));
        editor.doc.layer_mut(id).unwrap().track.position.set(10, Point::new(200.0, 100.0));
        editor.selection = Some(id);

        // 20 px per frame, the graph 400×200 at the window's corner.
        let mut h =
            Harness::builder().with_size(egui::vec2(400.0, 200.0)).with_step_dt(1.0 / 60.0).build_ui_state(
                |ui, (graph, editor): &mut (ValueGraph, Editor)| {
                    let area = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
                    let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
                    let axis = FrameAxis { x_of: &|f| f * 20.0 + 10.0, frame_of: &|x| (x - 10.0) / 20.0 };
                    graph.show(ui, &response, area, editor, axis);
                },
                (ValueGraph::default(), editor),
            );
        h.run();

        // x runs 0 → 200 over frames 0–10 and stays there to frame 20, so
        // the range shown is -20 to 220 over the plot's 14–186 px.
        let y_of = |value: f32| 186.0 - (value + 20.0) / 240.0 * 172.0;
        let key = pos2(210.0, y_of(200.0));
        let target = pos2(210.0, y_of(150.0));
        h.drag_at(key);
        h.step();
        h.hover_at(pos2(210.0, y_of(180.0)));
        h.step();
        h.hover_at(target);
        h.step();
        h.drop_at(target);
        h.run();

        let value = h.state().1.doc.layer(id).unwrap().track.position.key_at(10).unwrap().value;
        assert!((value.x - 150.0).abs() < 2.0, "x became {}", value.x);
        assert_eq!(value.y, 100.0, "only the grabbed curve changes");
    }

    #[test]
    fn editing_a_component_keys_just_that_value() {
        let mut layer = moving_layer();
        set_component(&mut layer.track, Property::Position, 10, 1, 300.0);
        assert_eq!(layer.track.position.key_at(10).unwrap().value, Point::new(200.0, 300.0));
        set_component(&mut layer.track, Property::Opacity, 0, 0, 50.0);
        assert_eq!(layer.track.opacity.key_at(0).unwrap().value, 0.5);
    }
}
