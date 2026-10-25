//! Find a Command (⌘K): every command in one searchable list, with where
//! it lives in the menus and its keys, as Blender's F3 search and the
//! command palettes of code editors. Type a few letters of what you're
//! after ("envel", "skeleton", "inbetween") and press Enter.
//!
//! The list is written out here rather than gathered from the menus, as
//! menus are only drawn while open. Each entry says which menu it's under,
//! so the finder also teaches where things are.

use egui::{Align2, Key, KeyboardShortcut, RichText, Sense, vec2};

use crate::app::Command;
use crate::editor::{Editor, Tool};
use crate::envelope::LatticeKind;
use crate::icons::{self, Icon};
use crate::shortcuts;

/// One command the finder lists.
pub(crate) struct Entry {
    pub label: &'static str,
    /// Where it is: a menu and submenu, or a panel.
    pub place: &'static str,
    pub command: Command,
    pub icon: Option<Icon>,
    pub keys: Option<KeyboardShortcut>,
    /// Other words to find it by.
    pub also: &'static str,
}

fn entry(label: &'static str, place: &'static str, command: Command) -> Entry {
    Entry { label, place, command, icon: None, keys: None, also: "" }
}

impl Entry {
    fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    fn keys(mut self, keys: KeyboardShortcut) -> Self {
        self.keys = Some(keys);
        self
    }

    fn also(mut self, words: &'static str) -> Self {
        self.also = words;
        self
    }

    /// How well the entry matches `query` (higher is better), or `None` if
    /// it doesn't: every word of the query must be found in its name, its
    /// place or its other words, and a name that starts with the query
    /// comes first.
    fn score(&self, query: &str) -> Option<u32> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Some(0);
        }
        let label = self.label.to_lowercase();
        let haystack = format!("{label} {} {}", self.place.to_lowercase(), self.also);
        if !query.split_whitespace().all(|word| haystack.contains(word)) {
            return None;
        }
        Some(if label.starts_with(&query) {
            3
        } else if label.contains(&query) {
            2
        } else {
            1
        })
    }
}

/// Every command the finder knows, roughly in menu order.
pub(crate) fn entries() -> Vec<Entry> {
    use inkframe_core::{PerspectiveKind, ShapeOp};
    vec![
        entry("New", "File", Command::New).keys(shortcuts::NEW).also("document project"),
        entry("Open…", "File", Command::Open).keys(shortcuts::OPEN),
        entry("Open Example…", "File", Command::OpenExample).also("sample tutorial"),
        entry("Import to Stage…", "File", Command::ImportImage)
            .keys(shortcuts::IMPORT)
            .also("picture image photo psd"),
        entry("Import Audio…", "File", Command::ImportAudio).icon(Icon::Speaker).also("sound music voice"),
        entry("Save", "File", Command::Save).keys(shortcuts::SAVE),
        entry("Save As…", "File", Command::SaveAs).keys(shortcuts::SAVE_AS),
        entry("Export…", "File", Command::Export)
            .keys(shortcuts::EXPORT)
            .also("video mp4 gif svg lottie html web page render"),
        entry("Batch Export…", "File", Command::BatchExport).also("render several"),
        entry("Undo", "Edit", Command::Undo).keys(shortcuts::UNDO),
        entry("Redo", "Edit", Command::Redo).keys(shortcuts::REDO),
        entry("Cut", "Edit", Command::Cut).keys(shortcuts::CUT),
        entry("Copy", "Edit", Command::Copy).keys(shortcuts::COPY),
        entry("Paste in Center", "Edit", Command::Paste).keys(shortcuts::PASTE),
        entry("Paste in Place", "Edit", Command::PasteInPlace).keys(shortcuts::PASTE_IN_PLACE),
        entry("Duplicate Layer", "Edit", Command::DuplicateLayer)
            .icon(Icon::Duplicate)
            .keys(shortcuts::DUPLICATE),
        entry("Delete Layer", "Edit", Command::DeleteLayer).icon(Icon::Trash),
        entry("Select All", "Edit", Command::SelectAll).keys(shortcuts::SELECT_ALL),
        entry("Convert to Symbol", "Modify", Command::ConvertToSymbol)
            .icon(Icon::Symbol)
            .keys(shortcuts::CONVERT_TO_SYMBOL)
            .also("reuse library instance"),
        entry("Union", "Modify › Combine Shapes", Command::CombineShapes(ShapeOp::Union))
            .icon(Icon::Combine)
            .also("merge join"),
        entry("Intersect", "Modify › Combine Shapes", Command::CombineShapes(ShapeOp::Intersect))
            .icon(Icon::Combine),
        entry("Punch", "Modify › Combine Shapes", Command::CombineShapes(ShapeOp::Punch))
            .icon(Icon::Combine)
            .also("subtract cut hole"),
        entry("Crop", "Modify › Combine Shapes", Command::CombineShapes(ShapeOp::Crop)).icon(Icon::Combine),
        entry("Break Apart", "Modify", Command::BreakApart)
            .icon(Icon::BreakApart)
            .keys(shortcuts::BREAK_APART)
            .also("text letters outline"),
        entry("Distort", "Modify › Transform", Command::StartLattice(LatticeKind::Distort))
            .icon(Icon::Distort)
            .also("perspective corners skew lattice"),
        entry("Envelope", "Modify › Transform", Command::StartLattice(LatticeKind::Envelope))
            .icon(Icon::Envelope)
            .also("warp bend bulge lattice"),
        entry("Edit Symbol", "Modify", Command::EditSymbol).icon(Icon::Symbol),
        entry("Swap Symbol…", "Modify", Command::SwapSymbol).icon(Icon::Symbol).also("replace"),
        entry("Exit Symbol", "Modify", Command::ExitSymbol).icon(Icon::StepOut),
        entry("Mask", "Modify", Command::MaskLayerBelow).also("clip window"),
        entry("Vectorise…", "Modify", Command::Vectorise).also("trace bitmap paint to shapes"),
        entry("New Layer", "Modify", Command::NewLayer).icon(Icon::Plus).keys(shortcuts::NEW_LAYER),
        entry("New Paint Layer", "Modify", Command::NewPaintLayer).also("raster brush canvas"),
        entry("New Folder", "Modify", Command::NewFolder).icon(Icon::Folder).also("group"),
        entry("New Skeleton", "Modify", Command::NewSkeleton)
            .icon(Icon::Bones)
            .also("rig bones moho character"),
        entry("New Switch", "Modify", Command::NewSwitch).icon(Icon::Folder).also("mouth hand poses swap"),
        entry("Add Shape Hint", "Modify", Command::AddShapeHint)
            .keys(shortcuts::ADD_SHAPE_HINT)
            .also("morph tween"),
        entry("Merge Down", "Modify", Command::MergeDown).also("paint layers flatten"),
        entry("New Drawing", "Modify", Command::NewDrawing).also("substitution frame by frame"),
        entry("Duplicate Drawing", "Modify", Command::DuplicateDrawing),
        entry("Next Drawing", "Modify", Command::NextDrawing).keys(shortcuts::NEXT_DRAWING),
        entry("Previous Drawing", "Modify", Command::PreviousDrawing).keys(shortcuts::PREVIOUS_DRAWING),
        entry("Lip Sync…", "Modify", Command::LipSync).also("mouth audio phonemes"),
        entry("Insert Keyframe", "Timeline", Command::InsertKeyframe)
            .keys(shortcuts::INSERT_KEYFRAME)
            .also("key f6"),
        entry("Clear Keyframe", "Timeline", Command::RemoveKeyframe).keys(shortcuts::REMOVE_KEYFRAME),
        entry("Insert Frames", "Timeline", Command::InsertFrames)
            .keys(shortcuts::INSERT_FRAMES)
            .also("f5 hold"),
        entry("Remove Frames", "Timeline", Command::RemoveFrames).keys(shortcuts::REMOVE_FRAMES),
        entry("Insert Blank Keyframe", "Timeline", Command::InsertBlankKeyframe)
            .keys(shortcuts::INSERT_BLANK_KEYFRAME),
        entry("Reverse Frames", "Timeline", Command::ReverseFrames),
        entry("Create Shape Tween", "Timeline", Command::CreateShapeTween).also("morph"),
        entry("Draw Inbetweens", "Timeline", Command::MakeInbetweens)
            .icon(Icon::Inbetween)
            .also("tween frames assistant"),
        entry("Add Camera", "Timeline", Command::AddCamera).icon(Icon::Camera),
        entry("Play / Pause", "Timeline", Command::PlayPause).icon(Icon::Play).keys(shortcuts::PLAY),
        entry("Zoom In", "View", Command::ZoomIn).keys(shortcuts::ZOOM_IN),
        entry("Zoom Out", "View", Command::ZoomOut).keys(shortcuts::ZOOM_OUT),
        entry("Fit Stage", "View", Command::ZoomFit).keys(shortcuts::ZOOM_FIT),
        entry("Rulers", "View", Command::ToggleRulers)
            .icon(Icon::Ruler)
            .keys(shortcuts::RULERS)
            .also("guides"),
        entry("Rig Setup (Skeletons at Rest)", "View", Command::ToggleRigSetup)
            .icon(Icon::RigSetup)
            .keys(shortcuts::RIG_SETUP)
            .also("bones pose"),
        entry(
            "One-Point Perspective",
            "View › Perspective Guide",
            Command::SetPerspective(Some(PerspectiveKind::OnePoint)),
        )
        .icon(Icon::Perspective)
        .also("vanishing"),
        entry(
            "Two-Point Perspective",
            "View › Perspective Guide",
            Command::SetPerspective(Some(PerspectiveKind::TwoPoint)),
        )
        .icon(Icon::Perspective)
        .also("vanishing"),
        entry(
            "Three-Point Perspective",
            "View › Perspective Guide",
            Command::SetPerspective(Some(PerspectiveKind::ThreePoint)),
        )
        .icon(Icon::Perspective)
        .also("vanishing"),
        entry("No Perspective Guide", "View › Perspective Guide", Command::SetPerspective(None))
            .icon(Icon::Perspective),
        entry("Show Guides", "View › Guides", Command::ToggleGuides).keys(shortcuts::SHOW_GUIDES),
        entry("Snap to Guides", "View › Guides", Command::ToggleSnapToGuides).keys(shortcuts::SNAP_TO_GUIDES),
        // Tools, by the slot they're in.
        tool_entry(Tool::Select),
        tool_entry(Tool::Subselect),
        tool_entry(Tool::Width),
        tool_entry(Tool::Pen),
        tool_entry(Tool::Pencil),
        tool_entry(Tool::Bone),
        tool_entry(Tool::AddBone),
        tool_entry(Tool::Camera),
        tool_entry(Tool::Brush),
        tool_entry(Tool::PaintBucket),
        tool_entry(Tool::Text),
    ]
}

/// Picking a tool, as an entry.
fn tool_entry(tool: Tool) -> Entry {
    Entry {
        label: tool.name(),
        place: "Tools",
        command: Command::UseTool(tool),
        icon: None,
        keys: None,
        also: tool.description(),
    }
}

#[derive(Default)]
pub(crate) struct CommandFinder {
    open: bool,
    query: String,
    /// The highlighted row, among those that match.
    highlighted: usize,
    /// Focus the search field on the next frame (just opened).
    focus: bool,
}

impl CommandFinder {
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.highlighted = 0;
        self.focus = true;
    }

    /// Opens with `query` typed, for the screenshot hook.
    pub fn open_with(&mut self, query: &str) {
        self.open();
        self.query = query.to_owned();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows the finder if it's open. Returns the command picked, if any.
    pub fn show(&mut self, ctx: &egui::Context, editor: &Editor) -> Option<Command> {
        if !self.open {
            return None;
        }
        let all = entries();
        let mut matches: Vec<(u32, usize)> =
            all.iter().enumerate().filter_map(|(i, e)| e.score(&self.query).map(|s| (s, i))).collect();
        // Best first; equally good, in menu order.
        matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.highlighted = self.highlighted.min(matches.len().saturating_sub(1));

        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            self.open = false;
            return None;
        }
        if down {
            self.highlighted = (self.highlighted + 1).min(matches.len().saturating_sub(1));
        }
        if up {
            self.highlighted = self.highlighted.saturating_sub(1);
        }
        let mut picked = enter.then(|| matches.get(self.highlighted).map(|&(_, i)| all[i].command)).flatten();

        let mut open = true;
        egui::Window::new("Find a Command")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 70.0))
            .fixed_size(vec2(520.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    icons::paint(ui.painter(), Icon::Search, rect, ui.visuals().weak_text_color());
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Find a command or tool: envelope, skeleton, inbetween…")
                            .desired_width(f32::INFINITY)
                            .frame(egui::Frame::NONE)
                            .font(egui::TextStyle::Heading),
                    );
                    if self.focus {
                        field.request_focus();
                        self.focus = false;
                    }
                    if field.changed() {
                        self.highlighted = 0;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    if matches.is_empty() {
                        ui.weak("Nothing matches. Try another word.");
                    }
                    for (row, &(_, i)) in matches.iter().enumerate() {
                        let e = &all[i];
                        let current = matches!(e.command, Command::UseTool(t) if t == editor.tool);
                        let response = entry_row(ui, e, row == self.highlighted, current);
                        if response.clicked() {
                            picked = Some(e.command);
                        }
                        if response.hovered() {
                            self.highlighted = row;
                        }
                        if row == self.highlighted && (up || down) {
                            response.scroll_to_me(None);
                        }
                    }
                });
                ui.add_space(2.0);
                ui.weak("↑↓ to choose · Enter to run · Esc to close");
            });
        if !open || picked.is_some() {
            self.open = false;
        }
        picked
    }
}

/// One row: icon, name, where it is, and its keys.
fn entry_row(ui: &mut egui::Ui, e: &Entry, highlighted: bool, current: bool) -> egui::Response {
    let height = 30.0;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let painter = ui.painter();
    let visuals = ui.visuals();
    if highlighted {
        painter.rect_filled(rect, 4.0, visuals.selection.bg_fill.gamma_multiply(0.6));
    }
    let text = visuals.text_color();
    let weak = visuals.weak_text_color();
    let icon_rect = egui::Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), vec2(18.0, 18.0));
    match e.icon {
        Some(icon) => icons::paint(painter, icon, icon_rect, text),
        None => {
            painter.circle_filled(icon_rect.center(), 2.5, weak);
        }
    }
    let label = if current { format!("{}  (current tool)", e.label) } else { e.label.to_owned() };
    painter.text(
        rect.left_center() + vec2(34.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        text,
    );
    let mut right = rect.right() - 8.0;
    if let Some(keys) = e.keys {
        let keys = ui.ctx().format_shortcut(&keys);
        let galley = painter.layout_no_wrap(keys, egui::FontId::proportional(12.0), weak);
        let width = galley.size().x;
        painter.galley(egui::pos2(right - width, rect.center().y - galley.size().y / 2.0), galley, weak);
        right -= width + 14.0;
    }
    painter.text(
        egui::pos2(right, rect.center().y),
        Align2::RIGHT_CENTER,
        e.place,
        egui::FontId::proportional(12.0),
        weak,
    );
    response.on_hover_text(RichText::new(e.also).weak())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(query: &str) -> Vec<&'static str> {
        let all = entries();
        let mut found: Vec<(u32, usize)> =
            all.iter().enumerate().filter_map(|(i, e)| e.score(query).map(|s| (s, i))).collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        found.into_iter().map(|(_, i)| all[i].label).collect()
    }

    #[test]
    fn commands_are_found_by_their_names_and_other_words() {
        assert_eq!(find("envel").first(), Some(&"Envelope"));
        assert!(find("rig").contains(&"New Skeleton"));
        assert!(find("inbetween").contains(&"Draw Inbetweens"));
        assert!(find("web page").contains(&"Export…"));
        assert!(find("perspective two").contains(&"Two-Point Perspective"));
        assert!(find("zzzz").is_empty());
    }
}
