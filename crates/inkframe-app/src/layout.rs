//! The window's layout: areas you can split, join, resize and swap, the
//! way Blender's work.
//!
//! The window is divided into rectangular **areas** with no gaps between
//! them. Each shows one kind of editor (the stage, the timeline, the
//! properties…), picked from the menu in its header, and any kind can be in
//! any area, more than once (two stages at different zooms, say).
//!
//! * **Resize** by dragging the border between two areas.
//! * **Split** by dragging from an area's corner into it: across makes two
//!   areas side by side, down makes one above the other.
//! * **Join** by dragging from a corner into the neighbouring area, which
//!   is taken over.
//! * **Swap** two areas' contents by Ctrl-dragging from a corner onto the
//!   other.
//! * **Maximise** the area under the pointer with Ctrl+Space, and again to
//!   go back.
//!
//! The header's right-click menu does the same things. Underneath it's a
//! binary tree: every split divides one rectangle in two at a fraction,
//! and the leaves are areas. That's why joining needs the two areas to be
//! the halves of one split, as in Blender, where they must share a whole
//! edge.
//!
//! The layout is the person's, not the document's, so it's kept in a file
//! of its own in the settings folder and comes back next time.

use std::path::PathBuf;

use egui::{Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::icons::{self, Icon, IconButton};
use crate::theme;

/// The kinds of editor an area can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Stage,
    Timeline,
    Properties,
    Tools,
    Library,
    ExposureSheet,
    /// The timeline's value graph on its own.
    GraphEditor,
    /// The current tool's options (the brush, the selection), which the
    /// Properties area also shows.
    ToolSettings,
    Audio,
    Document,
    Swatches,
    Align,
    History,
}

impl Kind {
    /// Every kind, grouped as the area menu lists them, Blender-style.
    pub const GROUPS: [(&'static str, &'static [Kind]); 4] = [
        ("General", &[Kind::Stage, Kind::Tools, Kind::ToolSettings, Kind::Properties]),
        ("Animation", &[Kind::Timeline, Kind::GraphEditor, Kind::ExposureSheet]),
        ("Design", &[Kind::Swatches, Kind::Align]),
        ("Project", &[Kind::Library, Kind::Audio, Kind::Document, Kind::History]),
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Stage => "Stage",
            Kind::Timeline => "Timeline",
            Kind::Properties => "Properties",
            Kind::Tools => "Tools",
            Kind::Library => "Library",
            Kind::ExposureSheet => "Exposure Sheet",
            Kind::GraphEditor => "Graph Editor",
            Kind::ToolSettings => "Tool Settings",
            Kind::Audio => "Audio",
            Kind::Document => "Document",
            Kind::Swatches => "Swatches",
            Kind::Align => "Align",
            Kind::History => "History",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Kind::Stage => Icon::Stage,
            Kind::Timeline => Icon::Timeline,
            Kind::Properties => Icon::Properties,
            Kind::Tools => Icon::Tools,
            Kind::Library => Icon::Library,
            Kind::ExposureSheet => Icon::ExposureSheet,
            Kind::GraphEditor => Icon::Graph,
            Kind::ToolSettings => Icon::ToolSettings,
            Kind::Audio => Icon::Speaker,
            Kind::Document => Icon::Document,
            Kind::Swatches => Icon::Swatches,
            Kind::Align => Icon::AlignLeft,
            Kind::History => Icon::History,
        }
    }

    /// Whether the area holds a column of settings, which wants a margin,
    /// rather than an editor drawn edge to edge (the stage, the timeline).
    pub fn is_panel(self) -> bool {
        matches!(
            self,
            Kind::Properties
                | Kind::ToolSettings
                | Kind::Library
                | Kind::Audio
                | Kind::Document
                | Kind::Swatches
                | Kind::Align
                | Kind::History
        )
    }

    /// The timeline and the graph editor are one editor in two modes, so
    /// an area switches between them and keeps its scroll and zoom.
    pub fn is_timeline(self) -> bool {
        matches!(self, Kind::Timeline | Kind::GraphEditor)
    }
}

/// Identifies an area for as long as it exists, so the editors in it can
/// keep their own state (a stage's zoom, a timeline's scroll).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AreaId(u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Node {
    /// Two parts, side by side (`across`) or one above the other, the first
    /// taking `fraction` of the space.
    Split { across: bool, fraction: f32, first: Box<Node>, second: Box<Node> },
    /// An area. `kind` is the tab showing; `tabs` lists them all in order
    /// (empty in layouts saved before areas had tabs, meaning just `kind`).
    /// An area never holds two tabs of one kind, so each kind's editor in
    /// it keeps its own state while another tab is showing.
    Area {
        id: AreaId,
        kind: Kind,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tabs: Vec<Kind>,
    },
}

/// Which way a split runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Two areas side by side, a vertical border between.
    Across,
    /// One above the other.
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    root: Node,
    next_id: u64,
    /// The workspace this layout started as.
    #[serde(default)]
    workspace: Workspace,
    /// The area filling the window, if one is maximised.
    #[serde(skip)]
    maximized: Option<AreaId>,
    #[serde(skip)]
    gesture: Option<CornerDrag>,
}

/// Dragging from an area's corner.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CornerDrag {
    area: AreaId,
    corner: Pos2,
    /// Ctrl was held: swapping rather than splitting or joining.
    swap: bool,
}

/// What releasing a corner drag here would do.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CornerAction {
    Split { area: AreaId, direction: Direction, fraction: f32, new_first: bool },
    Join { keep: AreaId, remove: AreaId },
    Swap { a: AreaId, b: AreaId },
}

/// How tall an area's header is.
const HEADER: f32 = 22.0;
/// How thick the grab zone of a border between areas is.
const BORDER: f32 = 6.0;
/// The size of the corner zone that starts a split or join.
const CORNER: f32 = 12.0;
/// The smallest an area can be made, in points.
const MIN_SIZE: f32 = 60.0;

/// Ready-made arrangements of areas, as Animate's workspaces. Picking one
/// replaces the layout; the name stays with it as it's changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Workspace {
    /// Tools, stage and properties, with the timeline across the bottom.
    #[default]
    Essentials,
    /// A tall timeline with the graph editor and exposure sheet as tabs.
    Animator,
    /// Swatches and Align beside the properties, and a short timeline.
    Designer,
    /// Tool settings and swatches up front, and the history, for painting.
    Painter,
}

impl Workspace {
    pub const ALL: [Workspace; 4] =
        [Workspace::Essentials, Workspace::Animator, Workspace::Designer, Workspace::Painter];

    pub fn name(self) -> &'static str {
        match self {
            Workspace::Essentials => "Essentials",
            Workspace::Animator => "Animator",
            Workspace::Designer => "Designer",
            Workspace::Painter => "Painter",
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::workspace(Workspace::Essentials)
    }
}

/// Builds layout trees with fresh area ids.
struct Builder {
    next_id: u64,
}

impl Builder {
    /// An area with these tabs, the first showing.
    fn area(&mut self, tabs: &[Kind]) -> Box<Node> {
        self.next_id += 1;
        // One tab is stored as none: just the area's kind.
        let kind = tabs[0];
        let tabs = if tabs.len() > 1 { tabs.to_vec() } else { Vec::new() };
        Box::new(Node::Area { id: AreaId(self.next_id), kind, tabs })
    }

    fn single(&mut self, kind: Kind) -> Box<Node> {
        self.area(&[kind])
    }
}

fn across(fraction: f32, first: Box<Node>, second: Box<Node>) -> Box<Node> {
    Box::new(Node::Split { across: true, fraction, first, second })
}

fn down(fraction: f32, first: Box<Node>, second: Box<Node>) -> Box<Node> {
    Box::new(Node::Split { across: false, fraction, first, second })
}

impl Layout {
    /// A workspace's arrangement.
    pub fn workspace(workspace: Workspace) -> Self {
        let mut b = Builder { next_id: 0 };
        // Room for the tools in two columns on a laptop-sized window.
        const TOOLS: f32 = 0.075;
        let root = match workspace {
            Workspace::Essentials => {
                let (tools, stage) = (b.single(Kind::Tools), b.single(Kind::Stage));
                let right = b.area(&[Kind::Properties, Kind::Library]);
                let timeline = b.single(Kind::Timeline);
                down(0.72, across(TOOLS, tools, across(0.76, stage, right)), timeline)
            }
            Workspace::Animator => {
                let (tools, stage) = (b.single(Kind::Tools), b.single(Kind::Stage));
                let right = b.area(&[Kind::Properties, Kind::Library, Kind::Swatches]);
                let timeline = b.area(&[Kind::Timeline, Kind::GraphEditor, Kind::ExposureSheet]);
                down(0.6, across(TOOLS, tools, across(0.76, stage, right)), timeline)
            }
            Workspace::Designer => {
                let (tools, stage) = (b.single(Kind::Tools), b.single(Kind::Stage));
                let upper = b.area(&[Kind::Properties, Kind::Library]);
                let lower = b.area(&[Kind::Swatches, Kind::Align]);
                let timeline = b.single(Kind::Timeline);
                down(0.8, across(TOOLS, tools, across(0.74, stage, down(0.6, upper, lower))), timeline)
            }
            Workspace::Painter => {
                let (tools, stage) = (b.single(Kind::Tools), b.single(Kind::Stage));
                let upper = b.area(&[Kind::ToolSettings, Kind::Swatches]);
                let lower = b.area(&[Kind::Properties, Kind::History, Kind::Library]);
                let timeline = b.single(Kind::Timeline);
                down(0.82, across(TOOLS, tools, across(0.74, stage, down(0.5, upper, lower))), timeline)
            }
        };
        Self { root: *root, next_id: b.next_id, workspace, maximized: None, gesture: None }
    }

    pub fn workspace_name(&self) -> Workspace {
        self.workspace
    }
}

impl Layout {
    /// Every area and where it is, laid out in `rect` (just the maximised
    /// one, if there is one).
    pub fn areas(&self, rect: Rect) -> Vec<(AreaId, Kind, Rect)> {
        if let Some(max) = self.maximized
            && let Some(kind) = self.kind_of(max)
        {
            return vec![(max, kind, rect)];
        }
        let mut areas = Vec::new();
        collect_areas(&self.root, rect, &mut areas);
        areas
    }

    pub fn kind_of(&self, area: AreaId) -> Option<Kind> {
        self.areas_unmaximized().into_iter().find(|(id, _)| *id == area).map(|(_, kind)| kind)
    }

    fn areas_unmaximized(&self) -> Vec<(AreaId, Kind)> {
        let mut areas = Vec::new();
        collect_areas(&self.root, Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.0)), &mut areas);
        areas.into_iter().map(|(id, kind, _)| (id, kind)).collect()
    }

    /// Turns every timeline into a graph editor (for screenshots).
    pub fn timelines_to_graphs(&mut self) {
        for (id, kind) in self.areas_unmaximized() {
            if kind == Kind::Timeline {
                self.set_kind(id, Kind::GraphEditor);
            }
        }
    }

    /// Changes what the showing tab of an area shows. If the area has a tab
    /// of that kind already, it shows that tab instead.
    pub fn set_kind(&mut self, area: AreaId, kind: Kind) {
        if let Some(Node::Area { kind: k, tabs, .. }) = find_mut(&mut self.root, area) {
            if tabs.contains(&kind) {
                *k = kind;
                return;
            }
            if let Some(tab) = tabs.iter_mut().find(|t| **t == *k) {
                *tab = kind;
            }
            *k = kind;
        }
    }

    /// An area's tabs, in order.
    pub fn tabs_of(&self, area: AreaId) -> Vec<Kind> {
        match find(&self.root, area) {
            Some(Node::Area { kind, tabs, .. }) if tabs.is_empty() => vec![*kind],
            Some(Node::Area { tabs, .. }) => tabs.clone(),
            _ => Vec::new(),
        }
    }

    /// Every area's every tab: the editors that need keeping, showing or not.
    pub fn all_tabs(&self) -> Vec<(AreaId, Kind)> {
        self.areas_unmaximized()
            .into_iter()
            .flat_map(|(id, _)| self.tabs_of(id).into_iter().map(move |kind| (id, kind)))
            .collect()
    }

    /// Adds a tab to an area and shows it (or shows the one it has).
    pub fn add_tab(&mut self, area: AreaId, kind: Kind) {
        let mut tabs = self.tabs_of(area);
        if !tabs.contains(&kind) {
            tabs.push(kind);
        }
        if let Some(Node::Area { kind: k, tabs: t, .. }) = find_mut(&mut self.root, area) {
            *t = tabs;
            *k = kind;
        }
    }

    /// Shows one of an area's tabs.
    pub fn show_tab(&mut self, area: AreaId, kind: Kind) {
        if self.tabs_of(area).contains(&kind) {
            self.add_tab(area, kind);
        }
    }

    /// Closes a tab, showing the one beside it. An area's last tab stays.
    pub fn close_tab(&mut self, area: AreaId, kind: Kind) -> bool {
        let mut tabs = self.tabs_of(area);
        let Some(at) = tabs.iter().position(|&t| t == kind).filter(|_| tabs.len() > 1) else { return false };
        tabs.remove(at);
        let showing = self.kind_of(area);
        if let Some(Node::Area { kind: k, tabs: t, .. }) = find_mut(&mut self.root, area) {
            if showing == Some(kind) {
                *k = tabs[at.min(tabs.len() - 1)];
            }
            *t = tabs;
        }
        true
    }

    /// Moves a tab from one area to another, as dragging it there does.
    /// Moving an area's last tab closes that area, if it can be closed;
    /// otherwise nothing moves. Returns whether it moved.
    pub fn move_tab(&mut self, from: AreaId, kind: Kind, to: AreaId) -> bool {
        if from == to || !self.tabs_of(from).contains(&kind) || self.tabs_of(to).is_empty() {
            return false;
        }
        if self.tabs_of(from).len() == 1 {
            if !self.close(from) {
                return false;
            }
        } else {
            self.close_tab(from, kind);
        }
        self.add_tab(to, kind);
        true
    }

    /// Splits an area in two, the new half showing the same editor, and
    /// returns the new area. `fraction` is how much of the space goes to the
    /// first part (left or top); `new_first` puts the new area there.
    pub fn split(
        &mut self,
        area: AreaId,
        direction: Direction,
        fraction: f32,
        new_first: bool,
    ) -> Option<AreaId> {
        let new = AreaId(self.next_id + 1);
        let node = find_mut(&mut self.root, area)?;
        let Node::Area { kind, .. } = *node else { return None };
        self.next_id += 1;
        let fresh = Node::Area { id: new, kind, tabs: Vec::new() };
        let (old, fresh) = (Box::new(node.clone()), Box::new(fresh));
        let (first, second) = if new_first { (fresh, old) } else { (old, fresh) };
        *node = Node::Split {
            across: direction == Direction::Across,
            fraction: fraction.clamp(0.05, 0.95),
            first,
            second,
        };
        Some(new)
    }

    /// The area sharing a split with `area`, if the other half is a single
    /// area: the one it can join with.
    pub fn sibling(&self, area: AreaId) -> Option<AreaId> {
        fn search(node: &Node, area: AreaId) -> Option<AreaId> {
            let Node::Split { first, second, .. } = node else { return None };
            match (first.as_ref(), second.as_ref()) {
                (Node::Area { id: a, .. }, Node::Area { id: b, .. }) if *a == area => Some(*b),
                (Node::Area { id: a, .. }, Node::Area { id: b, .. }) if *b == area => Some(*a),
                _ => search(first, area).or_else(|| search(second, area)),
            }
        }
        search(&self.root, area)
    }

    /// `keep` takes over `remove`'s space, if they're the two halves of a
    /// split. Returns whether it did.
    pub fn join(&mut self, keep: AreaId, remove: AreaId) -> bool {
        if self.sibling(keep) != Some(remove) {
            return false;
        }
        replace_split_with(&mut self.root, keep);
        if self.maximized == Some(remove) {
            self.maximized = None;
        }
        true
    }

    /// Closes an area, whatever shares its split taking its space (one
    /// area, or several). The last area can't be closed.
    pub fn close(&mut self, area: AreaId) -> bool {
        fn remove(node: &mut Node, area: AreaId) -> bool {
            let Node::Split { first, second, .. } = node else { return false };
            let is = |n: &Node| matches!(n, Node::Area { id, .. } if *id == area);
            let other = if is(first) {
                second.as_ref().clone()
            } else if is(second) {
                first.as_ref().clone()
            } else {
                return remove(first, area) || remove(second, area);
            };
            *node = other;
            true
        }
        let closed = remove(&mut self.root, area);
        if closed && self.maximized == Some(area) {
            self.maximized = None;
        }
        closed
    }

    /// Swaps what two areas show, tabs and all.
    pub fn swap(&mut self, a: AreaId, b: AreaId) {
        let take = |layout: &mut Self, id| match find_mut(&mut layout.root, id) {
            Some(Node::Area { kind, tabs, .. }) => Some((*kind, std::mem::take(tabs))),
            _ => None,
        };
        let (Some(ta), Some(tb)) = (take(self, a), take(self, b)) else { return };
        for (id, (kind, tabs)) in [(a, tb), (b, ta)] {
            if let Some(Node::Area { kind: k, tabs: t, .. }) = find_mut(&mut self.root, id) {
                (*k, *t) = (kind, tabs);
            }
        }
    }

    /// Two stages side by side, and an exposure sheet beside the timeline:
    /// a rearranged layout for the screenshot hook.
    pub fn arrange_for_screenshot(&mut self) {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0));
        let find =
            |layout: &Layout, kind| layout.areas(window).into_iter().find(|a| a.1 == kind).map(|a| a.0);
        if let Some(stage) = find(self, Kind::Stage) {
            self.split(stage, Direction::Across, 0.5, false);
        }
        if let Some(timeline) = find(self, Kind::Timeline)
            && let Some(sheet) = self.split(timeline, Direction::Across, 0.72, false)
        {
            self.set_kind(sheet, Kind::ExposureSheet);
        }
        if let Some(timeline) = find(self, Kind::Timeline)
            && let Some(graph) = self.split(timeline, Direction::Across, 0.5, false)
        {
            self.set_kind(graph, Kind::GraphEditor);
        }
        if let Some(properties) = find(self, Kind::Properties)
            && let Some(settings) = self.split(properties, Direction::Down, 0.4, true)
        {
            self.set_kind(settings, Kind::ToolSettings);
        }
    }

    pub fn toggle_maximized(&mut self, area: AreaId) {
        self.maximized = if self.maximized == Some(area) { None } else { Some(area) };
    }

    pub fn is_maximized(&self) -> bool {
        self.maximized.is_some()
    }

    /// Draws the borders, headers and corner zones, and handles resizing,
    /// splitting, joining and swapping. `content` fills each area below its
    /// header. Returns the area under the pointer, if any.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        mut content: impl FnMut(&mut egui::Ui, AreaId, Kind),
    ) -> Option<AreaId> {
        let rect = ui.available_rect_before_wrap();
        let areas = self.areas(rect);
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let hovered = pointer.and_then(|p| areas.iter().find(|(_, _, r)| r.contains(p)).map(|(id, ..)| *id));

        for &(id, kind, area_rect) in &areas {
            let header = Rect::from_min_size(area_rect.min, vec2(area_rect.width(), HEADER));
            let body = Rect::from_min_max(pos2(area_rect.left(), header.bottom()), area_rect.max).shrink(1.0);
            // Each area on the usual panel colour, as the fixed panels were.
            ui.painter().rect_filled(area_rect, 0.0, ui.visuals().panel_fill);
            let body = if kind.is_panel() { body.shrink2(vec2(10.0, 6.0)) } else { body };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt(("area", id.0)));
            child.set_clip_rect(body);
            content(&mut child, id, kind);
            self.header(ui, id, kind, header);
        }
        if !self.is_maximized() {
            self.borders(ui, rect);
            self.corners(ui, &areas);
        }
        // Lines between areas.
        let line = Stroke::new(1.0, ui.visuals().window_stroke.color);
        for (_, _, area_rect) in &areas {
            ui.painter().rect_stroke(*area_rect, 0.0, line, egui::StrokeKind::Inside);
        }
        hovered
    }

    /// An area's header: which editor it shows (a menu to change it) and
    /// the area menu on right-click.
    fn header(&mut self, ui: &mut egui::Ui, id: AreaId, kind: Kind, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
        // The right-click target goes in first, so the editor menu drawn
        // over it gets its clicks: in egui the widget added last is on top.
        let response = ui.interact(rect, ui.id().with(("header_menu", id.0)), Sense::click());
        let mut header = ui.new_child(
            egui::UiBuilder::new().max_rect(rect.shrink2(vec2(6.0, 1.0))).id_salt(("header", id.0)),
        );
        let tabs = self.tabs_of(id);
        // When the tabs don't all fit with their names, the ones not
        // showing shrink to their icons.
        let font = egui::TextStyle::Button.resolve(ui.style());
        let full: f32 =
            tabs.iter().map(|&tab| tab_width(ui, tab, tab == kind, false, &font) + 2.0).sum::<f32>() + 30.0;
        let compact = full > rect.width() - 12.0;
        header.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for &tab in &tabs {
                let showing = tab == kind;
                let button = tab_button(ui, tab, showing, compact && !showing);
                // Tabs can be dragged to another area's header.
                button.dnd_set_drag_payload(TabDrag { area: id, kind: tab });
                if showing {
                    // The showing tab's menu changes what it shows. It has an
                    // id of its own: the default one is shared with the
                    // right-click menu, which closes on a left click.
                    egui::Popup::menu(&button).id(editors_menu_id(&button)).show(|ui| {
                        if let Some(chosen) = kind_menu(ui, kind) {
                            self.set_kind(id, chosen);
                            ui.close();
                        }
                    });
                } else if button.clicked() {
                    self.show_tab(id, tab);
                }
                if button.middle_clicked() {
                    self.close_tab(id, tab);
                }
                button.context_menu(|ui| {
                    if ui.add_enabled(tabs.len() > 1, egui::Button::new("Close Tab")).clicked() {
                        self.close_tab(id, tab);
                        ui.close();
                    }
                });
            }
            let add = ui.add(IconButton::new(Icon::Plus, "Add a tab"));
            egui::Popup::menu(&add).show(|ui| {
                if let Some(chosen) = kind_menu(ui, kind) {
                    self.add_tab(id, chosen);
                    ui.close();
                }
            });
        });
        // A tab dragged here from another area moves here when let go.
        let pointer = ui.input(|i| i.pointer.hover_pos());
        if let Some(dragged) = egui::DragAndDrop::payload::<TabDrag>(ui.ctx())
            && dragged.area != id
            && pointer.is_some_and(|p| rect.contains(p))
        {
            ui.painter().rect_stroke(rect.shrink(1.0), 2.0, (2.0, theme::ACCENT), egui::StrokeKind::Inside);
            if ui.input(|i| i.pointer.any_released())
                && let Some(dragged) = egui::DragAndDrop::take_payload::<TabDrag>(ui.ctx())
            {
                self.move_tab(dragged.area, dragged.kind, id);
            }
        }
        response.on_hover_text("Right-click for the area menu").context_menu(|ui| {
            let half = 0.5;
            if ui.button("Split Side by Side").clicked() {
                self.split(id, Direction::Across, half, false);
                ui.close();
            }
            if ui.button("Split Top and Bottom").clicked() {
                self.split(id, Direction::Down, half, false);
                ui.close();
            }
            let sibling = self.sibling(id);
            if ui.add_enabled(sibling.is_some(), egui::Button::new("Join with Neighbour")).clicked()
                && let Some(other) = sibling
            {
                self.join(id, other);
                ui.close();
            }
            let alone = matches!(self.root, Node::Area { .. });
            if ui.add_enabled(!alone, egui::Button::new("Close Area")).clicked() {
                self.close(id);
                ui.close();
            }
            ui.separator();
            let label = if self.maximized == Some(id) { "Back to Layout" } else { "Maximise Area" };
            if ui.add(egui::Button::new(label).shortcut_text("Ctrl+Space")).clicked() {
                self.toggle_maximized(id);
                ui.close();
            }
        });
    }

    /// The borders between areas, draggable to resize.
    fn borders(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let mut handles = Vec::new();
        collect_borders(&self.root, rect, Vec::new(), &mut handles);
        for (path, across, line, parent) in handles {
            let response = ui.interact(line, ui.id().with(("border", path.clone())), Sense::drag());
            let cursor = if across { CursorIcon::ResizeHorizontal } else { CursorIcon::ResizeVertical };
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(cursor);
                ui.painter().rect_filled(line, 0.0, ui.visuals().selection.bg_fill.gamma_multiply(0.6));
            }
            if response.dragged()
                && let Some(p) = response.interact_pointer_pos()
                && let Some(Node::Split { fraction, .. }) = node_at_mut(&mut self.root, &path)
            {
                let (start, length, at) = if across {
                    (parent.left(), parent.width(), p.x)
                } else {
                    (parent.top(), parent.height(), p.y)
                };
                // Neither side smaller than an area can usefully be.
                let min = (MIN_SIZE / length).min(0.45);
                *fraction = ((at - start) / length).clamp(min, 1.0 - min);
            }
        }
    }

    /// The corner zones: drag into the area to split it, into a neighbour
    /// to join, or with Ctrl onto any area to swap.
    fn corners(&mut self, ui: &mut egui::Ui, areas: &[(AreaId, Kind, Rect)]) {
        let accent = ui.visuals().selection.stroke.color;
        for &(id, _, rect) in areas {
            for corner in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()] {
                let inward = vec2(
                    if corner.x == rect.left() { 1.0 } else { -1.0 },
                    if corner.y == rect.top() { 1.0 } else { -1.0 },
                );
                let zone = Rect::from_two_pos(corner, corner + inward * CORNER);
                let response = ui.interact(
                    zone,
                    ui.id().with(("corner", id.0, corner.x as i32, corner.y as i32)),
                    Sense::drag(),
                );
                if response.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                    let triangle = vec![
                        corner,
                        corner + vec2(inward.x * CORNER, 0.0),
                        corner + vec2(0.0, inward.y * CORNER),
                    ];
                    ui.painter().add(egui::Shape::convex_polygon(
                        triangle,
                        accent.gamma_multiply(0.5),
                        Stroke::NONE,
                    ));
                }
                if response.drag_started() {
                    let swap = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                    self.gesture = Some(CornerDrag { area: id, corner, swap });
                }
            }
        }

        let Some(drag) = self.gesture else { return };
        let pointer = ui.input(|i| i.pointer.latest_pos());
        let sibling = self.sibling(drag.area);
        let action = pointer.and_then(|p| corner_action(drag, p, areas, sibling));
        // Show what letting go would do.
        let painter = ui.painter();
        match action {
            Some(CornerAction::Split { area, direction, fraction, .. }) => {
                if let Some((.., rect)) = areas.iter().find(|(id, ..)| *id == area) {
                    let line = match direction {
                        Direction::Across => {
                            let x = rect.left() + rect.width() * fraction;
                            [pos2(x, rect.top()), pos2(x, rect.bottom())]
                        }
                        Direction::Down => {
                            let y = rect.top() + rect.height() * fraction;
                            [pos2(rect.left(), y), pos2(rect.right(), y)]
                        }
                    };
                    painter.line_segment(line, Stroke::new(2.0, accent));
                }
            }
            Some(CornerAction::Join { remove, .. }) => {
                if let Some((.., rect)) = areas.iter().find(|(id, ..)| *id == remove) {
                    painter.rect_filled(*rect, 0.0, Color32::from_black_alpha(110));
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Join",
                        FontId::proportional(18.0),
                        Color32::WHITE,
                    );
                }
            }
            Some(CornerAction::Swap { b, .. }) => {
                if let Some((.., rect)) = areas.iter().find(|(id, ..)| *id == b) {
                    painter.rect_stroke(*rect, 0.0, Stroke::new(2.0, accent), egui::StrokeKind::Inside);
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Swap",
                        FontId::proportional(18.0),
                        accent,
                    );
                }
            }
            None => {}
        }
        if ui.input(|i| i.pointer.any_released()) {
            self.gesture = None;
            match action {
                Some(CornerAction::Split { area, direction, fraction, new_first }) => {
                    self.split(area, direction, fraction, new_first);
                }
                Some(CornerAction::Join { keep, remove }) => {
                    self.join(keep, remove);
                }
                Some(CornerAction::Swap { a, b }) => self.swap(a, b),
                None => {}
            }
        }
    }

    /// Where the layout is kept between runs.
    fn file() -> Option<PathBuf> {
        let base = if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        };
        base.map(|dir| dir.join("Inkframe").join("layout.json"))
    }

    /// The layout saved last time, or the usual one.
    pub fn load() -> Self {
        Self::file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str::<Layout>(&json).ok())
            .map(Layout::upgraded)
            .unwrap_or_default()
    }

    /// A layout saved by an older version, brought up to date: the Library
    /// used to be part of the Properties panel, so a layout without one
    /// gets it as a tab beside Properties.
    fn upgraded(mut self) -> Self {
        let tabs = self.all_tabs();
        if !tabs.iter().any(|(_, kind)| *kind == Kind::Library)
            && let Some(&(area, _)) = tabs.iter().find(|(_, kind)| *kind == Kind::Properties)
        {
            let showing = self.kind_of(area).unwrap_or(Kind::Properties);
            self.add_tab(area, Kind::Library);
            self.show_tab(area, showing);
        }
        self
    }

    /// Keeps the layout for next time. Failing is harmless: next time just
    /// starts with the usual layout.
    pub fn save(&self) {
        let Some(path) = Self::file() else { return };
        if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_string_pretty(self)) {
            let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&path, json));
        }
    }
}

/// What a corner drag from `drag` would do with the pointer at `p`.
/// Joining is only offered with `sibling`, the area that shares a whole
/// edge with the dragged one.
fn corner_action(
    drag: CornerDrag,
    p: Pos2,
    areas: &[(AreaId, Kind, Rect)],
    sibling: Option<AreaId>,
) -> Option<CornerAction> {
    let (_, _, rect) = *areas.iter().find(|(id, ..)| *id == drag.area)?;
    let target = areas.iter().find(|(_, _, r)| r.contains(p)).map(|(id, ..)| *id)?;
    if drag.swap {
        return (target != drag.area).then_some(CornerAction::Swap { a: drag.area, b: target });
    }
    if target != drag.area {
        return (Some(target) == sibling).then_some(CornerAction::Join { keep: drag.area, remove: target });
    }
    // Into the area itself: split, once it's clearly meant.
    let moved = p - drag.corner;
    if moved.length() < CORNER * 2.0 {
        return None;
    }
    let direction = if moved.x.abs() > moved.y.abs() { Direction::Across } else { Direction::Down };
    let fraction = match direction {
        Direction::Across => (p.x - rect.left()) / rect.width(),
        Direction::Down => (p.y - rect.top()) / rect.height(),
    };
    // The new area opens on the corner's side, between it and the pointer.
    let new_first = match direction {
        Direction::Across => drag.corner.x == rect.left(),
        Direction::Down => drag.corner.y == rect.top(),
    };
    Some(CornerAction::Split { area: drag.area, direction, fraction, new_first })
}

/// How wide a tab is: its icon, its name unless `icon_only`, and room for
/// the chevron on the showing one.
fn tab_width(ui: &egui::Ui, kind: Kind, showing: bool, icon_only: bool, font: &egui::FontId) -> f32 {
    if icon_only {
        return 28.0;
    }
    let text = ui.fonts_mut(|f| f.layout_no_wrap(kind.name().to_owned(), font.clone(), theme::TEXT).size().x);
    text + 32.0 + if showing { 18.0 } else { 6.0 }
}

/// The id of a tab's menu of editors.
fn editors_menu_id(tab: &egui::Response) -> egui::Id {
    tab.id.with("editors")
}

/// A tab being dragged to another area's header.
#[derive(Debug, Clone, Copy)]
struct TabDrag {
    area: AreaId,
    kind: Kind,
}

/// One tab in an area's header: the kind's icon and name. The showing tab
/// is raised, with a chevron for its menu of editors; the others are flat
/// until hovered.
fn tab_button(ui: &mut egui::Ui, kind: Kind, showing: bool, icon_only: bool) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(kind.name().to_owned(), font.clone(), theme::TEXT);
    let size = vec2(tab_width(ui, kind, showing, icon_only, &font), HEADER - 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let popup = editors_menu_id(&response);
    if showing
        && kind == Kind::Stage
        && crate::screenshot::AutoScreenshot::area_menu()
        && ui.ctx().cumulative_frame_nr() == 2
    {
        egui::Popup::open_id(ui.ctx(), popup);
    }
    let open = egui::Popup::is_id_open(ui.ctx(), popup);
    let fill = if open {
        Some(theme::CONTROL_HOVER)
    } else if showing {
        Some(theme::CONTROL)
    } else if response.hovered() {
        Some(theme::RAISED.gamma_multiply(1.6))
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    let color = if showing { theme::TEXT } else { theme::TEXT_WEAK };
    let painter = ui.painter();
    let icon = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(14.0));
    icons::paint(painter, kind.icon(), icon, color);
    if !icon_only {
        painter.galley(pos2(icon.right() + 7.0, rect.center().y - galley.size().y / 2.0), galley, color);
    }
    if showing {
        let chevron = Rect::from_center_size(pos2(rect.right() - 10.0, rect.center().y), Vec2::splat(9.0));
        icons::paint(painter, Icon::Chevron, chevron, theme::TEXT_WEAK);
    }
    let hint = if icon_only {
        kind.name()
    } else if showing {
        "Click to change what this tab shows; drag it to another area's header to move it"
    } else {
        "Click to show; drag to another area's header to move it; middle-click to close"
    };
    response.on_hover_text(hint)
}

/// One editor in the menu: icon and name, highlighted if it's the
/// area's current one or under the pointer.
fn kind_item(ui: &mut egui::Ui, kind: Kind, current: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(150.0, 24.0), Sense::click());
    let fill = if current {
        Some(theme::ACCENT_DIM)
    } else if response.hovered() {
        Some(theme::CONTROL_HOVER)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    let icon = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(14.0));
    icons::paint(ui.painter(), kind.icon(), icon, theme::TEXT);
    ui.painter().text(
        pos2(icon.right() + 9.0, rect.center().y),
        Align2::LEFT_CENTER,
        kind.name(),
        egui::TextStyle::Button.resolve(ui.style()),
        theme::TEXT,
    );
    response
}

/// The list of editors, a column per group, with the area's current one
/// marked. Returns the one clicked.
fn kind_menu(ui: &mut egui::Ui, current: Kind) -> Option<Kind> {
    let mut chosen = None;
    ui.horizontal_top(|ui| {
        for (group, kinds) in Kind::GROUPS {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(egui::RichText::new(group.to_uppercase()).small().color(theme::TEXT_WEAK));
                for &kind in kinds {
                    if kind_item(ui, kind, kind == current).clicked() {
                        chosen = Some(kind);
                    }
                }
            });
        }
    });
    chosen
}

fn collect_areas(node: &Node, rect: Rect, out: &mut Vec<(AreaId, Kind, Rect)>) {
    match node {
        Node::Area { id, kind, .. } => out.push((*id, *kind, rect)),
        Node::Split { across, fraction, first, second } => {
            let (a, b) = divide(rect, *across, *fraction);
            collect_areas(first, a, out);
            collect_areas(second, b, out);
        }
    }
}

/// Each split's border: its path from the root, which way it runs, the
/// grab zone, and the rectangle it divides.
fn collect_borders(node: &Node, rect: Rect, path: Vec<bool>, out: &mut Vec<(Vec<bool>, bool, Rect, Rect)>) {
    let Node::Split { across, fraction, first, second } = node else { return };
    let (a, b) = divide(rect, *across, *fraction);
    let line = if *across {
        Rect::from_center_size(pos2(a.right(), rect.center().y), vec2(BORDER, rect.height()))
    } else {
        Rect::from_center_size(pos2(rect.center().x, a.bottom()), vec2(rect.width(), BORDER))
    };
    out.push((path.clone(), *across, line, rect));
    let mut left = path.clone();
    left.push(false);
    collect_borders(first, a, left, out);
    let mut right = path;
    right.push(true);
    collect_borders(second, b, right, out);
}

fn divide(rect: Rect, across: bool, fraction: f32) -> (Rect, Rect) {
    if across {
        let x = rect.left() + rect.width() * fraction;
        (
            Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
            Rect::from_min_max(pos2(x, rect.top()), rect.max),
        )
    } else {
        let y = rect.top() + rect.height() * fraction;
        (
            Rect::from_min_max(rect.min, pos2(rect.right(), y)),
            Rect::from_min_max(pos2(rect.left(), y), rect.max),
        )
    }
}

fn find(node: &Node, area: AreaId) -> Option<&Node> {
    match node {
        Node::Area { id, .. } if *id == area => Some(node),
        Node::Area { .. } => None,
        Node::Split { first, second, .. } => find(first, area).or_else(|| find(second, area)),
    }
}

fn find_mut(node: &mut Node, area: AreaId) -> Option<&mut Node> {
    match node {
        Node::Area { id, .. } if *id == area => Some(node),
        Node::Area { .. } => None,
        Node::Split { first, second, .. } => {
            if contains(first, area) {
                find_mut(first, area)
            } else {
                find_mut(second, area)
            }
        }
    }
}

fn contains(node: &Node, area: AreaId) -> bool {
    match node {
        Node::Area { id, .. } => *id == area,
        Node::Split { first, second, .. } => contains(first, area) || contains(second, area),
    }
}

fn node_at_mut<'a>(node: &'a mut Node, path: &[bool]) -> Option<&'a mut Node> {
    match path.split_first() {
        None => Some(node),
        Some((&second, rest)) => match node {
            Node::Split { first, second: other, .. } => node_at_mut(if second { other } else { first }, rest),
            Node::Area { .. } => None,
        },
    }
}

/// Replaces the split holding `keep` and its sibling area with `keep` alone.
fn replace_split_with(node: &mut Node, keep: AreaId) -> bool {
    let Node::Split { first, second, .. } = node else { return false };
    let is_pair = |a: &Node, b: &Node| {
        matches!((a, b), (Node::Area { id, .. }, Node::Area { .. }) if *id == keep)
            || matches!((a, b), (Node::Area { .. }, Node::Area { id, .. }) if *id == keep)
    };
    if is_pair(first, second) {
        let kept =
            if matches!(first.as_ref(), Node::Area { id, .. } if *id == keep) { first } else { second };
        *node = (**kept).clone();
        return true;
    }
    replace_split_with(first, keep) || replace_split_with(second, keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))
    }

    fn id_of(layout: &Layout, kind: Kind) -> AreaId {
        layout.areas(window()).into_iter().find(|(_, k, _)| *k == kind).unwrap().0
    }

    /// Draws the layout for one frame with `events`, returning where each
    /// piece of text was drawn.
    fn frame(ctx: &egui::Context, layout: &mut Layout, events: Vec<egui::Event>) -> Vec<(String, Rect)> {
        let input = egui::RawInput { screen_rect: Some(window()), events, ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::no_frame().show(ui, |ui| {
                layout.show(ui, |_, _, _| {});
            });
        });
        // No renderer here to upload egui's font textures to.
        output.textures_delta.clear();
        let mut texts = Vec::new();
        for clipped in output.shapes {
            if let egui::Shape::Text(text) = clipped.shape {
                texts.push((text.galley.text().to_owned(), text.visual_bounding_rect()));
            }
        }
        texts
    }

    fn click(at: Pos2) -> Vec<egui::Event> {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![egui::Event::PointerMoved(at), press(true), press(false)]
    }

    /// A click spread over frames, as a real one is: press, then release.
    fn click_slowly(ctx: &egui::Context, layout: &mut Layout, at: Pos2) {
        let mut events = click(at);
        let release = events.pop().unwrap();
        frame(ctx, layout, events);
        frame(ctx, layout, vec![release]);
    }

    #[test]
    fn the_header_menu_changes_what_an_area_shows() {
        let ctx = egui::Context::default();
        let mut layout = Layout::default();
        let stage = id_of(&layout, Kind::Stage);
        let texts = frame(&ctx, &mut layout, Vec::new());
        let find = |texts: &[(String, Rect)], name: &str| {
            texts.iter().find(|(text, _)| text.contains(name)).map(|(_, rect)| rect.center())
        };
        let button = find(&texts, "Stage").expect("the stage's header");
        click_slowly(&ctx, &mut layout, button);
        let texts = frame(&ctx, &mut layout, Vec::new());
        // Every editor is listed.
        for (_, kinds) in Kind::GROUPS {
            for kind in kinds {
                assert!(find(&texts, kind.name()).is_some(), "{} listed", kind.name());
            }
        }
        let item = find(&texts, "Graph Editor").unwrap();
        frame(&ctx, &mut layout, click(item));
        assert_eq!(layout.kind_of(stage), Some(Kind::GraphEditor));
    }

    #[test]
    fn every_kind_is_in_the_menu_once() {
        let listed: Vec<Kind> = Kind::GROUPS.iter().flat_map(|(_, kinds)| kinds.iter().copied()).collect();
        let unique: std::collections::HashSet<Kind> = listed.iter().copied().collect();
        assert_eq!(listed.len(), unique.len());
        assert_eq!(listed.len(), 13);
    }

    #[test]
    fn the_usual_layout_has_one_of_each_main_editor() {
        let layout = Layout::default();
        let kinds: Vec<Kind> = layout.areas(window()).into_iter().map(|(_, k, _)| k).collect();
        assert_eq!(kinds, [Kind::Tools, Kind::Stage, Kind::Properties, Kind::Timeline]);
        let stage = layout.areas(window()).into_iter().find(|(_, k, _)| *k == Kind::Stage).unwrap().2;
        assert!(stage.width() > 600.0 && stage.height() > 500.0);
    }

    #[test]
    fn splitting_makes_a_second_area_of_the_same_kind() {
        let mut layout = Layout::default();
        let stage = id_of(&layout, Kind::Stage);
        let new = layout.split(stage, Direction::Across, 0.5, false).unwrap();
        let areas = layout.areas(window());
        let (old_rect, new_rect) =
            (areas.iter().find(|a| a.0 == stage).unwrap().2, areas.iter().find(|a| a.0 == new).unwrap().2);
        assert_eq!(layout.kind_of(new), Some(Kind::Stage));
        assert!((old_rect.width() - new_rect.width()).abs() < 1.0);
        assert!(new_rect.left() > old_rect.left(), "the new one on the right");
    }

    #[test]
    fn halves_of_a_split_join_back_into_one() {
        let mut layout = Layout::default();
        let stage = id_of(&layout, Kind::Stage);
        let new = layout.split(stage, Direction::Down, 0.5, true).unwrap();
        assert_eq!(layout.sibling(stage), Some(new));
        assert!(layout.join(stage, new));
        assert_eq!(layout.areas(window()).len(), 4);
        // Tools and the timeline aren't the halves of one split.
        let (tools, timeline) = (id_of(&layout, Kind::Tools), id_of(&layout, Kind::Timeline));
        assert!(!layout.join(tools, timeline));
    }

    #[test]
    fn swapping_and_changing_kinds() {
        let mut layout = Layout::default();
        let (stage, timeline) = (id_of(&layout, Kind::Stage), id_of(&layout, Kind::Timeline));
        layout.swap(stage, timeline);
        assert_eq!(
            (layout.kind_of(stage), layout.kind_of(timeline)),
            (Some(Kind::Timeline), Some(Kind::Stage))
        );
        layout.set_kind(stage, Kind::ExposureSheet);
        assert_eq!(layout.kind_of(stage), Some(Kind::ExposureSheet));
    }

    #[test]
    fn a_maximised_area_fills_the_window() {
        let mut layout = Layout::default();
        let timeline = id_of(&layout, Kind::Timeline);
        layout.toggle_maximized(timeline);
        assert_eq!(layout.areas(window()), [(timeline, Kind::Timeline, window())]);
        layout.toggle_maximized(timeline);
        assert_eq!(layout.areas(window()).len(), 4);
    }

    #[test]
    fn corner_drags_split_join_or_swap() {
        let layout = Layout::default();
        let areas = layout.areas(window());
        let (stage, _, rect) = *areas.iter().find(|a| a.1 == Kind::Stage).unwrap();
        let properties = id_of(&layout, Kind::Properties);
        let corner = rect.right_top();
        let drag = CornerDrag { area: stage, corner, swap: false };
        let sibling = layout.sibling(stage);
        // Leftward into the stage: split side by side, the new area on the right.
        let split = corner_action(drag, corner + vec2(-200.0, 20.0), &areas, sibling);
        assert!(matches!(
            split,
            Some(CornerAction::Split { direction: Direction::Across, new_first: false, .. })
        ));
        // Rightward into the properties: join.
        let join = corner_action(drag, corner + vec2(40.0, 20.0), &areas, sibling);
        assert_eq!(join, Some(CornerAction::Join { keep: stage, remove: properties }));
        let swap =
            corner_action(CornerDrag { swap: true, ..drag }, corner + vec2(40.0, 20.0), &areas, sibling);
        assert_eq!(swap, Some(CornerAction::Swap { a: stage, b: properties }));
        // Down into the timeline, which isn't the stage's neighbour in the
        // tree: nothing.
        let bottom = CornerDrag { corner: rect.right_bottom(), ..drag };
        assert_eq!(corner_action(bottom, rect.right_bottom() + vec2(-20.0, 40.0), &areas, sibling), None);
    }

    #[test]
    fn layouts_save_as_json_and_read_back() {
        let mut layout = Layout::default();
        let stage = id_of(&layout, Kind::Stage);
        layout.split(stage, Direction::Down, 0.3, false);
        let json = serde_json::to_string(&layout).unwrap();
        assert_eq!(serde_json::from_str::<Layout>(&json).unwrap(), layout);
    }

    #[test]
    fn tabs_are_added_shown_closed_and_moved() {
        let mut layout = Layout::default();
        let (stage, timeline) = (id_of(&layout, Kind::Stage), id_of(&layout, Kind::Timeline));
        layout.add_tab(timeline, Kind::GraphEditor);
        assert_eq!(layout.tabs_of(timeline), [Kind::Timeline, Kind::GraphEditor]);
        assert_eq!(layout.kind_of(timeline), Some(Kind::GraphEditor));
        // No second tab of a kind: adding it again just shows it.
        layout.add_tab(timeline, Kind::Timeline);
        assert_eq!(layout.tabs_of(timeline).len(), 2);
        assert_eq!(layout.kind_of(timeline), Some(Kind::Timeline));

        assert!(layout.move_tab(timeline, Kind::GraphEditor, stage));
        assert_eq!(layout.tabs_of(stage), [Kind::Stage, Kind::GraphEditor]);
        assert_eq!(layout.tabs_of(timeline), [Kind::Timeline]);
        assert!(layout.close_tab(stage, Kind::GraphEditor));
        assert!(!layout.close_tab(stage, Kind::Stage), "an area's last tab stays");
    }

    #[test]
    fn moving_an_areas_last_tab_away_closes_the_area() {
        let mut layout = Layout::default();
        let (tools, stage) = (id_of(&layout, Kind::Tools), id_of(&layout, Kind::Stage));
        let count = layout.areas(window()).len();
        assert!(layout.move_tab(tools, Kind::Tools, stage));
        assert_eq!(layout.areas(window()).len(), count - 1);
        assert!(layout.tabs_of(stage).contains(&Kind::Tools));
    }

    #[test]
    fn changing_a_tab_to_a_kind_the_area_has_shows_that_tab() {
        let mut layout = Layout::default();
        let right = id_of(&layout, Kind::Properties);
        assert_eq!(layout.tabs_of(right), [Kind::Properties, Kind::Library]);
        layout.set_kind(right, Kind::Library);
        assert_eq!(layout.tabs_of(right), [Kind::Properties, Kind::Library]);
        layout.set_kind(right, Kind::Swatches);
        assert_eq!(layout.tabs_of(right), [Kind::Properties, Kind::Swatches]);
    }

    #[test]
    fn every_workspace_has_a_stage_and_a_timeline_and_round_trips() {
        for workspace in Workspace::ALL {
            let layout = Layout::workspace(workspace);
            let tabs: Vec<Kind> = layout.all_tabs().into_iter().map(|(_, k)| k).collect();
            assert!(tabs.contains(&Kind::Stage) && tabs.contains(&Kind::Timeline), "{workspace:?}");
            let ids: std::collections::HashSet<AreaId> =
                layout.all_tabs().into_iter().map(|(id, _)| id).collect();
            assert_eq!(ids.len(), layout.areas(window()).len(), "{workspace:?}: unique ids");
            let json = serde_json::to_string(&layout).unwrap();
            let back: Layout = serde_json::from_str(&json).unwrap();
            assert_eq!(back, layout);
            assert_eq!(back.workspace_name(), workspace);
        }
    }

    #[test]
    fn layouts_saved_before_tabs_still_load() {
        let json = r#"{"root":{"Area":{"id":1,"kind":"Stage"}},"next_id":1}"#;
        let layout: Layout = serde_json::from_str(json).unwrap();
        assert_eq!(layout.tabs_of(AreaId(1)), [Kind::Stage]);
        assert_eq!(layout.workspace_name(), Workspace::Essentials);

        // One from before the Library left the Properties panel gets it
        // back as a tab there.
        let mut old = Layout::default();
        let right = id_of(&old, Kind::Properties);
        old.close_tab(right, Kind::Library);
        let upgraded = old.upgraded();
        assert_eq!(upgraded.tabs_of(right), [Kind::Properties, Kind::Library]);
        assert_eq!(upgraded.kind_of(right), Some(Kind::Properties));
    }
}
