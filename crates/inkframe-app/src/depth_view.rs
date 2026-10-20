//! The Depth View: the scene from above, as Harmony's Top view shows a
//! multiplane set-up.
//!
//! The camera sits at the bottom looking up into the scene, its field of
//! view drawn as two lines. Each layer at the top of the main timeline is
//! a line at its depth, as wide as it reaches across the scene. Drag a
//! layer up or down to set its depth, and the camera up to dolly in or
//! sideways to pan; both key the current frame like any other edit.
//! Scroll to zoom, drag empty space to look around, and Fit puts the
//! whole set-up back in view.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};
use inkframe_core::{FOCAL_LENGTH, LayerId, Point, TimelineId};

use crate::editor::Editor;
use crate::theme;

const CAMERA: Color32 = Color32::from_rgb(0xff, 0xa0, 0x3c);
/// How close, in points, the pointer must be to pick up a layer's line.
const REACH: f32 = 6.0;

/// One area's view: where it looks, how close, and what's being dragged.
#[derive(Default)]
pub struct DepthView {
    /// `None` fits everything in, following changes; set once the view is
    /// zoomed or moved by hand.
    view: Option<View>,
    /// What's being dragged, with the view and the pointer when it began,
    /// so the drag is measured in the scene as it was then.
    dragging: Option<(Drag, View, Pos2)>,
    /// Where it was last drawn, and how, for finding things on screen.
    drawn: Option<(Rect, View)>,
}

/// The middle of the view, in scene units (x across, depth up), and
/// points per unit.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    centre: Point,
    scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    /// A layer's line: its depth when picked up.
    Layer { id: LayerId, depth: f32 },
    /// The camera: its dolly and look-at point when picked up.
    Camera { id: LayerId, dolly: f32, look_at: Point },
    /// Empty space: the view's centre when picked up.
    Pan { centre: Point },
}

/// Something in the view that can be picked up.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Layer(LayerId),
    /// The camera, with its dolly and look-at point.
    Camera(LayerId, f32, Point),
}

/// A layer as the view shows it.
struct Plane {
    id: LayerId,
    name: String,
    depth: f32,
    /// How far across the scene it reaches.
    left: f32,
    right: f32,
    color: Color32,
    /// Hidden, or faded by the camera passing.
    faint: bool,
}

impl View {
    fn to_screen(self, rect: Rect, x: f32, depth: f32) -> Pos2 {
        rect.center() + vec2((x - self.centre.x) * self.scale, -(depth - self.centre.y) * self.scale)
    }

    fn to_scene(self, rect: Rect, p: Pos2) -> Point {
        let d = p - rect.center();
        Point::new(self.centre.x + d.x / self.scale, self.centre.y - d.y / self.scale)
    }
}

impl DepthView {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let frame = editor.frame;
        let doc = &editor.doc;
        let stage = doc.stage_rect();
        let camera = doc.camera_layer().map(|c| {
            let pose = doc.pose(c, frame);
            (c.id, c.visible, pose.position, pose.scale.x.max(0.01), c.track.depth_at(frame as f32))
        });
        let planes = planes(editor);

        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::PANEL);

        // What's in the scene, for fitting: every layer, and the camera.
        let (dolly, look_at, zoom) = camera.map_or((0.0, stage.center(), 1.0), |c| (c.4, c.2, c.3));
        let eye = dolly - FOCAL_LENGTH;
        let fitted = {
            let lowest = eye - 120.0;
            let highest = planes.iter().map(|p| p.depth).fold(dolly + 400.0, f32::max) + 200.0;
            let left = planes.iter().map(|p| p.left).fold(stage.min.x, f32::min) - 100.0;
            let right = planes.iter().map(|p| p.right).fold(stage.max.x, f32::max) + 100.0;
            let scale = (rect.width() / (right - left)).min(rect.height() / (highest - lowest)).max(1e-4);
            View { centre: Point::new((left + right) / 2.0, (lowest + highest) / 2.0), scale }
        };
        let view = self.view.unwrap_or(fitted);
        self.drawn = Some((rect, view));
        let at = |x: f32, depth: f32| view.to_screen(rect, x, depth);

        // Depth guides every so often, and the stage's own plane.
        let step = nice_step(80.0 / view.scale);
        let (bottom, top) =
            (view.to_scene(rect, rect.left_bottom()).y, view.to_scene(rect, rect.left_top()).y);
        let mut depth = (bottom / step).floor() * step;
        while depth <= top {
            let y = at(0.0, depth).y;
            let strong = depth == 0.0;
            let color = if strong { theme::TEXT_WEAK } else { theme::TEXT_WEAK.gamma_multiply(0.25) };
            painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], (1.0, color));
            let label = if strong { "stage".to_owned() } else { format!("{depth:.0}") };
            painter.text(
                pos2(rect.left() + 4.0, y - 2.0),
                Align2::LEFT_BOTTOM,
                label,
                FontId::proportional(10.0),
                color,
            );
            depth += step;
        }

        // The camera's field of view: from the eye, through the edges of
        // what it shows at its own depth, on into the scene.
        let half = stage.width() / 2.0 / zoom;
        let eye_at = at(look_at.x, eye);
        let reach = (top - eye).max(FOCAL_LENGTH) / FOCAL_LENGTH;
        let camera_on = camera.is_some_and(|c| c.1);
        let fov_color =
            if camera_on { CAMERA.gamma_multiply(0.6) } else { theme::TEXT_WEAK.gamma_multiply(0.4) };
        for side in [-1.0, 1.0] {
            let far = at(look_at.x + side * half * reach, eye + FOCAL_LENGTH * reach);
            painter.line_segment([eye_at, far], (1.0, fov_color));
        }

        // The layers, far ones first.
        let selected = editor.selected_ids();
        let mut lines = Vec::new();
        for plane in &planes {
            let (a, b) = (at(plane.left, plane.depth), at(plane.right, plane.depth));
            let is_selected = selected.contains(&plane.id);
            let mut color = plane.color;
            if plane.faint {
                color = color.gamma_multiply(0.35);
            }
            let width = if is_selected { 4.0 } else { 2.5 };
            painter.line_segment([a, b], (width, color));
            if is_selected {
                painter.line_segment([a, b], (width + 4.0, theme::ACCENT.gamma_multiply(0.35)));
            }
            painter.text(
                pos2(a.x, a.y - 4.0),
                Align2::LEFT_BOTTOM,
                format!("{}  {:.0}", plane.name, plane.depth),
                FontId::proportional(11.0),
                if is_selected { Color32::WHITE } else { theme::TEXT },
            );
            lines.push((plane.id, a, b));
        }

        // The camera: a triangle at the eye, pointing into the scene.
        let mut camera_at = None;
        if let Some((_, on, ..)) = camera {
            let tip = eye_at + vec2(0.0, -14.0);
            let corners = vec![tip, eye_at + vec2(-10.0, 6.0), eye_at + vec2(10.0, 6.0)];
            let color = if on { CAMERA } else { theme::TEXT_WEAK };
            painter.add(egui::Shape::convex_polygon(corners, color, Stroke::NONE));
            painter.text(
                eye_at + vec2(14.0, 0.0),
                Align2::LEFT_CENTER,
                if on { format!("Camera  dolly {dolly:.0}") } else { "Camera off".to_owned() },
                FontId::proportional(11.0),
                color,
            );
            camera_at = Some(eye_at);
        } else {
            painter.text(
                rect.center_bottom() + vec2(0.0, -10.0),
                Align2::CENTER_BOTTOM,
                "No camera: add one with the Camera tool (C) for depth to show",
                FontId::proportional(12.0),
                theme::TEXT_WEAK,
            );
        }

        // Fit, top right.
        let fit = Rect::from_min_size(rect.right_top() + vec2(-44.0, 6.0), vec2(38.0, 20.0));
        let fit_response = ui.put(fit, egui::Button::new("Fit").small());
        if fit_response.clicked() {
            self.view = None;
        }

        // What's under a point: the camera first, then the nearest layer.
        let under = |p: Pos2| -> Option<Target> {
            if camera_at.is_some_and(|c| c.distance(p) < 14.0) {
                return camera.map(|c| Target::Camera(c.0, c.4, c.2));
            }
            lines
                .iter()
                .filter(|(_, a, b)| p.x >= a.x - REACH && p.x <= b.x + REACH && (p.y - a.y).abs() <= REACH)
                .min_by(|x, y| (p.y - x.1.y).abs().total_cmp(&(p.y - y.1.y).abs()))
                .map(|&(id, ..)| Target::Layer(id))
        };
        self.interact(ui, editor, &response, rect, view, under);
    }

    fn interact(
        &mut self,
        ui: &egui::Ui,
        editor: &mut Editor,
        response: &egui::Response,
        rect: Rect,
        view: View,
        under: impl Fn(Pos2) -> Option<Target>,
    ) {
        let hovered = response.hover_pos().and_then(&under);
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 200.0);
            if scroll != 0.0
                && let Some(p) = response.hover_pos()
            {
                // Zoom about the pointer, so what's under it stays put.
                let before = view.to_scene(rect, p);
                let scale = (view.scale * (scroll / 200.0).exp()).clamp(1e-4, 50.0);
                let mut zoomed = View { scale, ..view };
                let after = zoomed.to_scene(rect, p);
                zoomed.centre = zoomed.centre + (before - after);
                self.view = Some(zoomed);
            }
        }
        if hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if response.clicked()
            && let Some(Target::Layer(id) | Target::Camera(id, ..)) = hovered
        {
            editor.select(Some(id));
        }

        // A drag only begins once the pointer has moved, so what it picks
        // up is what was under the pointer when it was pressed.
        let pressed = ui.input(|i| i.pointer.press_origin());
        if response.drag_started()
            && let Some(from) = pressed.or(response.interact_pointer_pos())
        {
            let drag = match under(from) {
                Some(Target::Camera(id, dolly, look_at)) => Drag::Camera { id, dolly, look_at },
                Some(Target::Layer(id)) => {
                    editor.select(Some(id));
                    let depth = editor.doc.layer(id).map_or(0.0, |l| l.track.depth_at(editor.frame as f32));
                    Drag::Layer { id, depth }
                }
                None => Drag::Pan { centre: view.centre },
            };
            // Hold the view still while dragging, or fitting to the change
            // would slide the scene under the pointer.
            self.view = Some(view);
            self.dragging = Some((drag, view, from));
        }
        if let (Some((drag, then, from)), Some(now)) = (self.dragging, response.interact_pointer_pos()) {
            let moved = then.to_scene(rect, now) - then.to_scene(rect, from);
            match drag {
                Drag::Layer { id, depth } => editor.set_depth(id, (depth + moved.y).round()),
                Drag::Camera { id, dolly, look_at } => {
                    editor.set_depth(id, (dolly + moved.y).round());
                    if let Some(camera) = editor.doc.layer(id) {
                        let mut pose = editor.doc.pose(camera, editor.frame);
                        pose.position.x = (look_at.x + moved.x).round();
                        editor.set_pose(id, pose);
                    }
                }
                Drag::Pan { centre } => self.view = Some(View { centre: centre - moved, ..then }),
            }
        }
        if response.drag_stopped() {
            self.dragging = None;
        }
    }
}

/// The layers the camera places, as lines across the scene.
fn planes(editor: &Editor) -> Vec<Plane> {
    let doc = &editor.doc;
    let frame = editor.frame;
    let mut planes: Vec<Plane> = doc
        .layers_of(TimelineId::Main)
        .iter()
        .filter(|l| !l.camera && !l.folder && l.parent.is_none() && l.is_shown_at(frame))
        .filter_map(|layer| {
            // Across the scene, before the camera: where it is on the stage.
            let bounds = doc.layer_bounds(layer, frame)?;
            let placed = doc.placement_without_camera(layer, frame as f32);
            let xs = bounds.corners().map(|c| placed.apply(c).x);
            let (left, right) = xs.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
            let color = layer.color.map_or(theme::TEXT, crate::convert::to_color32);
            Some(Plane {
                id: layer.id,
                name: layer.name.clone(),
                depth: layer.track.depth_at(frame as f32),
                left,
                right,
                color,
                faint: !layer.visible || doc.camera_fade(layer, frame as f32) < 1.0,
            })
        })
        .collect();
    planes.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    planes
}

/// A round number near `rough`: 1, 2 or 5 times a power of ten.
fn nice_step(rough: f32) -> f32 {
    let power = 10f32.powf(rough.max(1e-3).log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * power).find(|&s| s >= rough).unwrap_or(10.0 * power)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_steps_are_round_numbers() {
        assert_eq!(nice_step(80.0), 100.0);
        assert_eq!(nice_step(130.0), 200.0);
        assert_eq!(nice_step(400.0), 500.0);
    }

    #[test]
    fn the_view_maps_the_scene_and_back() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let view = View { centre: Point::new(960.0, 0.0), scale: 0.2 };
        let p = view.to_screen(rect, 1460.0, 500.0);
        assert_eq!(p, pos2(300.0, 50.0), "right and up of the middle");
        let back = view.to_scene(rect, p);
        assert!((back.x - 1460.0).abs() < 1e-3 && (back.y - 500.0).abs() < 1e-3);
    }

    use egui_kittest::Harness;
    use inkframe_core::{Color, Element, Shape, Style, Transform};

    type State = (DepthView, Editor);

    /// A layer 200 deep, seen by a camera.
    fn harness() -> (Harness<'static, State>, LayerId, LayerId) {
        let mut editor = Editor::new(inkframe_core::Document::default());
        let square = Element {
            shape: Shape::Rect { width: 400.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        let layer = editor.doc.add_layer("Hill", vec![square], 0, Transform::at(Point::new(960.0, 500.0)));
        editor.doc.layer_mut(layer).unwrap().track.set_depth(0, 200.0);
        let camera = editor.doc.add_camera();
        let mut h = Harness::new_ui_state(
            |ui, (view, editor): &mut State| view.show(ui, editor),
            (DepthView::default(), editor),
        );
        h.set_size(vec2(500.0, 600.0));
        h.run();
        (h, layer, camera)
    }

    fn on_screen(h: &Harness<'_, State>, x: f32, depth: f32) -> Pos2 {
        let (rect, view) = h.state().0.drawn.unwrap();
        view.to_screen(rect, x, depth)
    }

    fn drag(h: &mut Harness<'_, State>, from: Pos2, to: Pos2) {
        h.drag_at(from);
        h.step();
        h.hover_at(to);
        h.step();
        h.drop_at(to);
        h.run();
    }

    #[test]
    fn dragging_a_layer_up_sends_it_deeper() {
        let (mut h, layer, _) = harness();
        let from = on_screen(&h, 960.0, 200.0);
        let to = on_screen(&h, 960.0, 700.0);
        drag(&mut h, from, to);
        let depth = h.state().1.doc.layer(layer).unwrap().track.depth_at(0.0);
        assert!((depth - 700.0).abs() <= 2.0, "{depth}");
        assert_eq!(h.state().1.selection, Some(layer));
    }

    #[test]
    fn dragging_the_camera_up_dollies_it_in() {
        let (mut h, _, camera) = harness();
        let from = on_screen(&h, 960.0, -FOCAL_LENGTH);
        let to = on_screen(&h, 960.0, -FOCAL_LENGTH + 300.0);
        drag(&mut h, from, to);
        let dolly = h.state().1.doc.layer(camera).unwrap().track.depth_at(0.0);
        assert!((dolly - 300.0).abs() <= 2.0, "{dolly}");
    }

    #[test]
    fn layers_are_listed_far_first_with_their_depth() {
        use inkframe_core::{Color, Element, Shape, Style, Transform};
        let mut editor = Editor::new(inkframe_core::Document::default());
        let square = Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        let near =
            editor.doc.add_layer("Near", vec![square.clone()], 0, Transform::at(Point::new(500.0, 500.0)));
        let far = editor.doc.add_layer("Far", vec![square], 0, Transform::at(Point::new(800.0, 500.0)));
        editor.doc.layer_mut(far).unwrap().track.set_depth(0, 2000.0);
        let planes = planes(&editor);
        assert_eq!(planes.iter().map(|p| p.id).collect::<Vec<_>>(), [far, near]);
        assert_eq!((planes[1].left, planes[1].right), (450.0, 550.0));
    }
}
