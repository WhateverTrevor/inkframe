//! The node view, as Harmony's: the timeline being edited drawn as a graph.
//!
//! Each layer is a node. Its transform output (bottom) feeds the Parent
//! input of the layers attached to it; a mask's image output feeds the Mask
//! input of the layers it masks; and each layer's image runs through its
//! colour effect and filters, each a node of its own, into the Composite
//! node, whose inputs are in stacking order (top first), and on to the
//! Display.
//!
//! The graph isn't a second model of the scene: it's built from the layers
//! every frame, and rewiring it changes the same settings the timeline and
//! Properties show (a layer's parent, its mask, its place in the stack, its
//! filters). Only where the nodes sit is the view's own.

use std::collections::HashMap;

use egui::{Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2, pos2, vec2};
use inkframe_core::{Document, Filter, FilterKind, Layer, LayerId, TimelineId};

use crate::convert::to_color32;
use crate::editor::Editor;
use crate::theme;

/// A node, by what it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKey {
    Layer(LayerId),
    /// A layer's colour effect.
    ColorEffect(LayerId),
    /// A layer's filter, by its place in the list.
    Filter(LayerId, usize),
    Composite,
    Display,
}

/// A place a wire starts or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port {
    /// A layer's transform, for the layers attached to it.
    Transform(LayerId),
    /// The end of a layer's image chain: the layer itself, or its last
    /// effect or filter.
    Image(LayerId),
    Parent(LayerId),
    Mask(LayerId),
    /// The Composite's input in place `n` from the top.
    Composite(usize),
}

impl Port {
    fn is_output(self) -> bool {
        matches!(self, Port::Transform(_) | Port::Image(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WireKind {
    Image,
    Parent,
    Mask,
}

/// Node sizes, in canvas units.
const LAYER_SIZE: Vec2 = vec2(170.0, 54.0);
const FILTER_SIZE: Vec2 = vec2(130.0, 34.0);
const DISPLAY_SIZE: Vec2 = vec2(110.0, 34.0);
const SLOT: f32 = 20.0;
const PORT_RADIUS: f32 = 5.0;

const IMAGE_WIRE: Color32 = Color32::from_rgb(0xc8, 0xcc, 0xd6);
const PARENT_WIRE: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x3c);
const MASK_WIRE: Color32 = Color32::from_rgb(0xb8, 0x7c, 0xff);

/// The layers that go into the Composite, top first: everything drawn,
/// which leaves out masks (they only cut), guides (scaffolding), folders
/// and the camera.
pub fn composited(layers: &[Layer]) -> Vec<LayerId> {
    layers.iter().rev().filter(|l| !l.mask && !l.guide && !l.folder && !l.camera).map(|l| l.id).collect()
}

/// The layers shown as nodes, top first.
fn shown(layers: &[Layer]) -> Vec<&Layer> {
    layers.iter().rev().filter(|l| !l.folder && !l.camera).collect()
}

/// What connecting `from` (an output) to `to` (an input) does to the
/// document on `frame`. Returns whether it changed anything.
pub fn connect(doc: &mut Document, timeline: TimelineId, from: Port, to: Port, frame: u32) -> bool {
    match (from, to) {
        // Attach: keeps the layer where it is (see `set_parent`).
        (Port::Transform(parent), Port::Parent(child)) => doc.set_parent(child, Some(parent), frame),
        // Any layer wired into a Mask port becomes a mask.
        (Port::Image(mask), Port::Mask(layer)) if mask != layer => {
            let same = doc.timeline_of(mask) == doc.timeline_of(layer);
            if !same {
                return false;
            }
            if let Some(m) = doc.layer_mut(mask) {
                m.mask = true;
            }
            if let Some(l) = doc.layer_mut(layer) {
                l.masked_by = Some(mask);
            }
            true
        }
        // Into the Composite at place `n`: restack it there.
        (Port::Image(layer), Port::Composite(n)) => {
            let others: Vec<LayerId> =
                composited(doc.layers_of(timeline)).into_iter().filter(|&id| id != layer).collect();
            let above_of = others.get(n).copied();
            let folder = above_of.and_then(|id| doc.layer(id)?.in_folder);
            doc.place_layer(layer, above_of, folder)
        }
        _ => false,
    }
}

/// Unplugs an input: a layer's parent or mask.
pub fn disconnect(doc: &mut Document, port: Port, frame: u32) -> bool {
    match port {
        Port::Parent(layer) => doc.set_parent(layer, None, frame),
        Port::Mask(layer) => doc.layer_mut(layer).is_some_and(|l| l.masked_by.take().is_some()),
        _ => false,
    }
}

/// A node as laid out this frame, in canvas units.
struct Node {
    key: NodeKey,
    rect: Rect,
}

/// A wire, from an output's position to an input's.
struct Wire {
    from: Pos2,
    to: Pos2,
    kind: WireKind,
}

/// Everything drawn, laid out in canvas units, with where each port is.
struct Graph {
    nodes: Vec<Node>,
    wires: Vec<Wire>,
    ports: Vec<(Port, Pos2)>,
}

#[derive(Default)]
pub struct NodeView {
    /// Where the canvas origin is, from the area's top-left, in points.
    pan: Vec2,
    zoom: Option<f32>,
    /// Nodes the user has moved. The rest are laid out automatically.
    moved: HashMap<NodeKey, Pos2>,
    drag: Option<Drag>,
}

enum Drag {
    Pan,
    /// Moving a node; `grab` is where it was held, from its corner.
    Node {
        key: NodeKey,
        grab: Vec2,
    },
    /// Pulling a wire out of an output.
    Wire {
        from: Port,
    },
}

impl NodeView {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        ui.horizontal(|ui| {
            if ui.button("Arrange").on_hover_text("Lay the nodes out again").clicked() {
                self.moved.clear();
                self.pan = Vec2::ZERO;
                self.zoom = None;
            }
            ui.weak("Drag from an output to an input to wire it; right-click an input to unplug it.");
        });
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let area = response.rect;
        painter.rect_filled(area, 0.0, theme::SUNKEN);

        let graph = self.layout(editor);
        // Until the view is zoomed or panned by hand, it fits the graph.
        let bounds =
            graph.nodes.iter().map(|n| n.rect).reduce(Rect::union).unwrap_or(Rect::ZERO).expand(40.0);
        let (zoom, origin) = match self.zoom {
            Some(zoom) => (zoom, area.min + self.pan),
            None => {
                let fit =
                    (area.width() / bounds.width()).min(area.height() / bounds.height()).clamp(0.3, 1.0);
                let origin = area.center() - bounds.center().to_vec2() * fit;
                self.pan = origin - area.min;
                (fit, origin)
            }
        };
        let to_screen = |p: Pos2| origin + p.to_vec2() * zoom;
        let to_canvas = |p: Pos2| Pos2::ZERO + (p - origin) / zoom;

        self.handle_input(ui, &response, editor, &graph, to_canvas, zoom);

        // Wires under nodes.
        for wire in &graph.wires {
            let color = match wire.kind {
                WireKind::Image => IMAGE_WIRE,
                WireKind::Parent => PARENT_WIRE,
                WireKind::Mask => MASK_WIRE,
            };
            draw_wire(&painter, to_screen(wire.from), to_screen(wire.to), color, zoom);
        }
        if let Some(Drag::Wire { from }) = self.drag
            && let Some(start) = graph.ports.iter().find(|(p, _)| *p == from).map(|(_, at)| *at)
            && let Some(pointer) = response.hover_pos().or(ui.input(|i| i.pointer.latest_pos()))
        {
            draw_wire(&painter, to_screen(start), pointer, theme::ACCENT, zoom);
        }
        for node in &graph.nodes {
            self.draw_node(&painter, editor, node, to_screen, zoom);
        }
        let font = FontId::proportional(10.0 * zoom);
        for (port, at) in &graph.ports {
            let center = to_screen(*at);
            let fill = if port.is_output() { theme::TEXT } else { theme::CONTROL_HOVER };
            painter.circle(center, PORT_RADIUS * zoom, fill, Stroke::new(1.0, theme::LINE));
            let label = match port {
                Port::Parent(_) => Some(("parent", Align2::RIGHT_CENTER)),
                Port::Mask(_) => Some(("mask", Align2::RIGHT_CENTER)),
                _ => None,
            };
            // Outside the node, left of the port.
            if let Some((text, align)) = label {
                painter.text(center - vec2(8.0 * zoom, 0.0), align, text, font.clone(), theme::TEXT_WEAK);
            }
        }
        self.context_menus(ui, &response, editor, &graph, to_canvas);
    }

    /// Where every node, wire and port goes, in canvas units. Nodes the
    /// user hasn't moved are laid out in rows, one per layer, top layer
    /// first: the layer, then its effects and filters, with the Composite
    /// and Display to the right.
    fn layout(&self, editor: &Editor) -> Graph {
        let layers = shown(editor.layers());
        let doc = &editor.doc;
        let frame = editor.frame;
        let row = |i: usize| 20.0 + i as f32 * (LAYER_SIZE.y + 28.0);
        let place = |key: NodeKey, default: Pos2| self.moved.get(&key).copied().unwrap_or(default);

        let mut nodes = Vec::new();
        let mut wires = Vec::new();
        let mut ports = Vec::new();
        // Where each layer's image comes out, after its effects.
        let mut image_out: HashMap<LayerId, Pos2> = HashMap::new();
        let mut widest_chain = 0usize;
        for (i, layer) in layers.iter().enumerate() {
            let depth = std::iter::successors(doc.parent_of(layer), |l| doc.parent_of(l)).take(8).count();
            // Room on the left for the input ports' labels.
            let at = place(NodeKey::Layer(layer.id), pos2(50.0 + depth as f32 * 30.0, row(i)));
            let rect = Rect::from_min_size(at, LAYER_SIZE);
            nodes.push(Node { key: NodeKey::Layer(layer.id), rect });
            ports.push((Port::Parent(layer.id), pos2(rect.left(), rect.top() + rect.height() * 0.3)));
            ports.push((Port::Mask(layer.id), pos2(rect.left(), rect.top() + rect.height() * 0.72)));
            ports.push((Port::Transform(layer.id), pos2(rect.center().x, rect.bottom())));

            // The image chain: colour effect, then each filter.
            let effects = layer.track.effects_at(frame as f32);
            let mut chain = Vec::new();
            if !effects.color.is_none() {
                chain.push(NodeKey::ColorEffect(layer.id));
            }
            chain.extend((0..effects.filters.len()).map(|j| NodeKey::Filter(layer.id, j)));
            widest_chain = widest_chain.max(chain.len());
            let mut out = pos2(rect.right(), rect.center().y);
            for (j, key) in chain.into_iter().enumerate() {
                let default = pos2(LAYER_SIZE.x + 230.0 + j as f32 * (FILTER_SIZE.x + 40.0), row(i) + 10.0);
                let rect = Rect::from_min_size(place(key, default), FILTER_SIZE);
                wires.push(Wire { from: out, to: pos2(rect.left(), rect.center().y), kind: WireKind::Image });
                out = pos2(rect.right(), rect.center().y);
                nodes.push(Node { key, rect });
            }
            ports.push((Port::Image(layer.id), out));
            image_out.insert(layer.id, out);
        }

        // Parents and masks.
        for layer in &layers {
            if let Some(parent) = doc.parent_of(layer)
                && let (Some(from), Some(to)) =
                    (port_at(&ports, Port::Transform(parent.id)), port_at(&ports, Port::Parent(layer.id)))
            {
                wires.push(Wire { from, to, kind: WireKind::Parent });
            }
            if let Some(mask) = layer.masked_by.and_then(|id| doc.layer(id)).filter(|m| m.mask)
                && let (Some(&from), Some(to)) =
                    (image_out.get(&mask.id), port_at(&ports, Port::Mask(layer.id)))
            {
                wires.push(Wire { from, to, kind: WireKind::Mask });
            }
        }

        // The Composite, one input per drawn layer, top first.
        let stack = composited(editor.layers());
        let rows = layers.len().max(1);
        let composite_default = pos2(
            LAYER_SIZE.x + 300.0 + widest_chain as f32 * (FILTER_SIZE.x + 40.0),
            (row(rows) - row(0)) / 2.0 - stack.len() as f32 * SLOT / 2.0,
        );
        let composite = Rect::from_min_size(
            place(NodeKey::Composite, composite_default),
            vec2(120.0, 30.0 + stack.len().max(1) as f32 * SLOT),
        );
        nodes.push(Node { key: NodeKey::Composite, rect: composite });
        for (n, id) in stack.iter().enumerate() {
            let slot = pos2(composite.left(), composite.top() + 26.0 + (n as f32 + 0.5) * SLOT);
            ports.push((Port::Composite(n), slot));
            if let Some(&from) = image_out.get(id) {
                wires.push(Wire { from, to: slot, kind: WireKind::Image });
            }
        }
        // A spare slot at the bottom, to drop a layer onto the bottom.
        ports.push((
            Port::Composite(stack.len()),
            pos2(composite.left(), composite.top() + 26.0 + (stack.len() as f32 + 0.5) * SLOT),
        ));

        let display_default = pos2(composite.right() + 70.0, composite.center().y - DISPLAY_SIZE.y / 2.0);
        let display = Rect::from_min_size(place(NodeKey::Display, display_default), DISPLAY_SIZE);
        nodes.push(Node { key: NodeKey::Display, rect: display });
        wires.push(Wire {
            from: pos2(composite.right(), composite.center().y),
            to: pos2(display.left(), display.center().y),
            kind: WireKind::Image,
        });
        Graph { nodes, wires, ports }
    }

    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        graph: &Graph,
        to_canvas: impl Fn(Pos2) -> Pos2,
        zoom: f32,
    ) {
        // Scrolling pans, ⌘ or pinch zooms about the pointer.
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if pinch != 1.0 {
                let new = (zoom * pinch).clamp(0.3, 2.5);
                if let Some(p) = response.hover_pos() {
                    let under = to_canvas(p);
                    self.pan += under.to_vec2() * (zoom - new);
                }
                self.zoom = Some(new);
            } else if scroll != Vec2::ZERO {
                self.pan += scroll;
                self.zoom = Some(zoom);
            }
        }
        let port_under = |p: Pos2, outputs: bool| {
            graph
                .ports
                .iter()
                .find(|(port, at)| {
                    port.is_output() == outputs && at.distance(to_canvas(p)) <= PORT_RADIUS * 2.0 + 2.0
                })
                .map(|(port, _)| *port)
        };
        if response.drag_started()
            && let Some(p) = response.interact_pointer_pos()
        {
            self.drag = Some(match port_under(p, true) {
                Some(from) => Drag::Wire { from },
                None => match graph.nodes.iter().rev().find(|n| n.rect.contains(to_canvas(p))) {
                    Some(node) => Drag::Node { key: node.key, grab: to_canvas(p) - node.rect.min },
                    None => Drag::Pan,
                },
            });
        }
        if let (Some(drag), Some(p)) = (&self.drag, response.interact_pointer_pos())
            && response.dragged()
        {
            match drag {
                Drag::Pan => {
                    self.pan += response.drag_delta();
                    self.zoom = Some(zoom);
                }
                Drag::Node { key, grab } => {
                    self.moved.insert(*key, to_canvas(p) - *grab);
                }
                Drag::Wire { .. } => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
            }
        }
        if response.drag_stopped() {
            if let Some(Drag::Wire { from }) = self.drag
                && let Some(p) = response.interact_pointer_pos()
                && let Some(to) = port_under(p, false)
            {
                let (timeline, frame) = (editor.timeline(), editor.frame);
                connect(&mut editor.doc, timeline, from, to, frame);
            }
            self.drag = None;
        }
        // A click on a node selects its layer.
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let hit = graph.nodes.iter().rev().find(|n| n.rect.contains(to_canvas(p)));
            match hit.map(|n| n.key) {
                Some(NodeKey::Layer(id) | NodeKey::ColorEffect(id) | NodeKey::Filter(id, _)) => {
                    editor.select(Some(id));
                }
                Some(_) => {}
                None => editor.select(None),
            }
        }
    }

    fn context_menus(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        graph: &Graph,
        to_canvas: impl Fn(Pos2) -> Pos2,
    ) {
        // What was right-clicked is remembered while its menu is open.
        let id = ui.id().with("node_menu_target");
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let input = graph
                .ports
                .iter()
                .find(|(port, at)| !port.is_output() && at.distance(to_canvas(p)) <= PORT_RADIUS * 2.0 + 2.0)
                .map(|(port, _)| MenuTarget::Port(*port));
            let node = graph
                .nodes
                .iter()
                .rev()
                .find(|n| n.rect.contains(to_canvas(p)))
                .map(|n| MenuTarget::Node(n.key));
            ui.data_mut(|d| d.insert_temp(id, input.or(node)));
        }
        let target = ui.data(|d| d.get_temp::<Option<MenuTarget>>(id)).flatten();
        let frame = editor.frame;
        response.context_menu(|ui| match target {
            Some(MenuTarget::Port(port)) => {
                if ui.button("Unplug").clicked() {
                    disconnect(&mut editor.doc, port, frame);
                    ui.close();
                }
            }
            Some(MenuTarget::Node(NodeKey::Layer(layer))) => {
                ui.menu_button("Add Filter", |ui| {
                    for kind in FilterKind::ALL {
                        if ui.button(kind.label()).clicked() {
                            edit_effects(editor, layer, |e| e.filters.push(Filter { enabled: true, kind }));
                            ui.close();
                        }
                    }
                });
            }
            Some(MenuTarget::Node(NodeKey::Filter(layer, index))) => {
                let enabled = editor
                    .doc
                    .layer(layer)
                    .and_then(|l| l.track.effects_at(frame as f32).filters.get(index).map(|f| f.enabled));
                let label = if enabled == Some(false) { "Switch On" } else { "Switch Off" };
                if ui.button(label).clicked() {
                    edit_effects(editor, layer, |e| {
                        if let Some(f) = e.filters.get_mut(index) {
                            f.enabled = !f.enabled;
                        }
                    });
                    ui.close();
                }
                if ui.button("Remove Filter").clicked() {
                    edit_effects(editor, layer, |e| {
                        if index < e.filters.len() {
                            e.filters.remove(index);
                        }
                    });
                    ui.close();
                }
            }
            Some(MenuTarget::Node(NodeKey::ColorEffect(layer))) => {
                if ui.button("Remove Colour Effect").clicked() {
                    edit_effects(editor, layer, |e| e.color = inkframe_core::ColorEffect::None);
                    ui.close();
                }
            }
            _ => {
                ui.weak("Right-click a layer, a filter or an input");
            }
        });
    }

    fn draw_node(
        &self,
        painter: &egui::Painter,
        editor: &Editor,
        node: &Node,
        to_screen: impl Fn(Pos2) -> Pos2,
        zoom: f32,
    ) {
        let rect = Rect::from_min_max(to_screen(node.rect.min), to_screen(node.rect.max));
        let doc = &editor.doc;
        let frame = editor.frame;
        let title = FontId::proportional(13.0 * zoom);
        let small = FontId::proportional(10.5 * zoom);
        let selected = |id: LayerId| editor.is_selected(id);
        let outline =
            |on: bool| Stroke::new(if on { 2.0 } else { 1.0 }, if on { theme::ACCENT } else { theme::LINE });
        match node.key {
            NodeKey::Layer(id) => {
                let Some(layer) = doc.layer(id) else { return };
                painter.rect(
                    rect,
                    6.0 * zoom,
                    theme::RAISED,
                    outline(selected(id)),
                    egui::StrokeKind::Inside,
                );
                let strip = Rect::from_min_max(rect.min, pos2(rect.left() + 4.0 * zoom, rect.bottom()));
                painter.rect_filled(strip, 2.0, to_color32(layer.tag_color()));
                painter.text(
                    rect.left_top() + vec2(14.0, 9.0) * zoom,
                    Align2::LEFT_TOP,
                    &layer.name,
                    title,
                    theme::TEXT,
                );
                let kind = if layer.mask {
                    "mask"
                } else if layer.guide {
                    "guide"
                } else if layer.instance.is_some() {
                    "symbol"
                } else if !layer.draws_vectors() {
                    "paint"
                } else {
                    "drawing"
                };
                painter.text(
                    rect.left_bottom() + vec2(14.0, -9.0) * zoom,
                    Align2::LEFT_BOTTOM,
                    kind,
                    small,
                    theme::TEXT_WEAK,
                );
            }
            NodeKey::ColorEffect(id) | NodeKey::Filter(id, _) => {
                let effects = doc.layer(id).map(|l| l.track.effects_at(frame as f32)).unwrap_or_default();
                let (label, on) = match node.key {
                    NodeKey::Filter(_, i) => {
                        effects.filters.get(i).map_or(("Filter", false), |f| (f.kind.label(), f.enabled))
                    }
                    _ => (effects.color.label(), true),
                };
                let fill = if on { theme::CONTROL } else { theme::PANEL };
                painter.rect(rect, 6.0 * zoom, fill, outline(selected(id)), egui::StrokeKind::Inside);
                let color = if on { theme::TEXT } else { theme::TEXT_WEAK };
                painter.text(rect.center(), Align2::CENTER_CENTER, label, title, color);
            }
            NodeKey::Composite => {
                painter.rect(rect, 6.0 * zoom, theme::RAISED, outline(false), egui::StrokeKind::Inside);
                painter.text(
                    rect.center_top() + vec2(0.0, 6.0 * zoom),
                    Align2::CENTER_TOP,
                    "Composite",
                    title,
                    theme::TEXT,
                );
                for (n, id) in composited(editor.layers()).iter().enumerate() {
                    let name = doc.layer(*id).map_or("", |l| l.name.as_str());
                    let at = rect.left_top() + vec2(12.0, 26.0 + (n as f32 + 0.5) * SLOT) * zoom;
                    painter.text(at, Align2::LEFT_CENTER, name, small.clone(), theme::TEXT_WEAK);
                }
            }
            NodeKey::Display => {
                painter.rect(rect, 6.0 * zoom, theme::ACCENT_DIM, outline(false), egui::StrokeKind::Inside);
                painter.text(rect.center(), Align2::CENTER_CENTER, "Display", title, theme::TEXT);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MenuTarget {
    Port(Port),
    Node(NodeKey),
}

fn port_at(ports: &[(Port, Pos2)], wanted: Port) -> Option<Pos2> {
    ports.iter().find(|(p, _)| *p == wanted).map(|(_, at)| *at)
}

/// Changes a layer's effects on the current frame, keying them as editing
/// them in Properties does.
fn edit_effects(editor: &mut Editor, layer: LayerId, change: impl FnOnce(&mut inkframe_core::Effects)) {
    let frame = editor.frame;
    if let Some(l) = editor.doc.layer_mut(layer) {
        let mut effects = l.track.effects_at(frame as f32);
        change(&mut effects);
        l.track.set_effects(frame, effects);
    }
}

/// A wire as a curve leaving its output to the right and arriving at its
/// input from the left.
fn draw_wire(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32, zoom: f32) {
    let reach = ((to.x - from.x).abs() / 2.0).max(40.0 * zoom);
    let curve = egui::epaint::CubicBezierShape::from_points_stroke(
        [from, from + vec2(reach, 0.0), to - vec2(reach, 0.0), to],
        false,
        Color32::TRANSPARENT,
        Stroke::new(1.8 * zoom, color),
    );
    painter.add(Shape::CubicBezier(curve));
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Point, Transform};

    /// Layers A (bottom), B, C (top) on the main timeline.
    fn doc() -> (Document, [LayerId; 3]) {
        let mut doc = Document::default();
        let ids =
            ["A", "B", "C"].map(|n| doc.add_layer(n, Vec::new(), 0, Transform::at(Point::new(100.0, 100.0))));
        (doc, ids)
    }

    #[test]
    fn wiring_a_transform_to_a_parent_input_attaches_the_layer() {
        let (mut doc, [a, b, _]) = doc();
        assert!(connect(&mut doc, TimelineId::Main, Port::Transform(a), Port::Parent(b), 0));
        assert_eq!(doc.layer(b).unwrap().parent, Some(a));
        assert!(disconnect(&mut doc, Port::Parent(b), 0));
        assert_eq!(doc.layer(b).unwrap().parent, None);
    }

    #[test]
    fn wiring_an_image_to_a_mask_input_makes_it_a_mask() {
        let (mut doc, [a, b, _]) = doc();
        assert!(connect(&mut doc, TimelineId::Main, Port::Image(b), Port::Mask(a), 0));
        assert!(doc.layer(b).unwrap().mask);
        assert_eq!(doc.layer(a).unwrap().masked_by, Some(b));
        // A mask isn't composited: it only cuts.
        assert_eq!(composited(&doc.layers), [doc.layers[2].id, a]);
    }

    #[test]
    fn wiring_into_the_composite_restacks() {
        let (mut doc, [a, b, c]) = doc();
        assert_eq!(composited(&doc.layers), [c, b, a]);
        // A into the top slot.
        assert!(connect(&mut doc, TimelineId::Main, Port::Image(a), Port::Composite(0), 0));
        assert_eq!(composited(&doc.layers), [a, c, b]);
        // And into the spare slot at the bottom.
        assert!(connect(&mut doc, TimelineId::Main, Port::Image(a), Port::Composite(2), 0));
        assert_eq!(composited(&doc.layers), [c, b, a]);
    }

    #[test]
    fn mismatched_wires_do_nothing() {
        let (mut doc, [a, b, _]) = doc();
        let before = doc.clone();
        assert!(!connect(&mut doc, TimelineId::Main, Port::Transform(a), Port::Mask(b), 0));
        assert!(!connect(&mut doc, TimelineId::Main, Port::Image(a), Port::Parent(b), 0));
        assert_eq!(doc, before);
    }
}
