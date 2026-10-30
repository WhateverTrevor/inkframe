//! Find a Command (⌘K): every command in one searchable list, with where
//! it lives in the menus and its keys, as Blender's F3 search and the
//! command palettes of code editors. Type a few letters of what you're
//! after ("envel", "skeleton", "inbetween") and press Enter.
//!
//! The list is written out here rather than gathered from the menus, as
//! menus are only drawn while open. Each entry says which menu it's under,
//! so the finder also teaches where things are.

use egui::{Align2, Key, KeyboardShortcut, RichText, Sense, vec2};

use crate::adjust_dialog::AdjustmentKind;
use crate::app::Command;
use crate::editor::{Editor, Tool};
use crate::envelope::LatticeKind;
use crate::i18n::{tr, tr_args, tr_later};
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
        // Found by the words shown, and by the English ones too, which
        // people often know from tutorials.
        let label = tr(self.label).to_lowercase();
        let english = self.label.to_lowercase();
        let haystack = format!(
            "{label} {english} {} {} {} {}",
            tr(self.place).to_lowercase(),
            self.place.to_lowercase(),
            tr(self.also).to_lowercase(),
            self.also.to_lowercase()
        );
        if !query.split_whitespace().all(|word| haystack.contains(word)) {
            return None;
        }
        Some(if label.starts_with(&query) || english.starts_with(&query) {
            3
        } else if label.contains(&query) || english.contains(&query) {
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
        entry(tr_later("New"), tr_later("File"), Command::New)
            .keys(shortcuts::NEW)
            .also(tr_later("document project")),
        entry(tr_later("Open…"), tr_later("File"), Command::Open).keys(shortcuts::OPEN),
        entry(tr_later("Open Example…"), tr_later("File"), Command::OpenExample)
            .also(tr_later("sample tutorial")),
        entry(tr_later("Import to Stage…"), tr_later("File"), Command::ImportImage)
            .keys(shortcuts::IMPORT)
            .also(tr_later("picture image photo psd")),
        entry(tr_later("Check for Updates…"), tr_later("Help"), Command::CheckForUpdates)
            .also(tr_later("new version upgrade download")),
        entry(tr_later("Import Sprite Sheet…"), tr_later("File"), Command::ImportSpriteSheet)
            .also(tr_later("sprites game frames texturepacker aseprite json")),
        entry(tr_later("Import Audio…"), tr_later("File"), Command::ImportAudio)
            .icon(Icon::Speaker)
            .also(tr_later("sound music voice")),
        entry(tr_later("Save"), tr_later("File"), Command::Save).keys(shortcuts::SAVE),
        entry(tr_later("Save As…"), tr_later("File"), Command::SaveAs).keys(shortcuts::SAVE_AS),
        entry(tr_later("Export…"), tr_later("File"), Command::Export)
            .keys(shortcuts::EXPORT)
            .also(tr_later("video mp4 gif svg lottie html web page render")),
        entry(tr_later("Batch Export…"), tr_later("File"), Command::BatchExport)
            .also(tr_later("render several")),
        entry(tr_later("Undo"), tr_later("Edit"), Command::Undo).keys(shortcuts::UNDO),
        entry(tr_later("Redo"), tr_later("Edit"), Command::Redo).keys(shortcuts::REDO),
        entry(tr_later("Cut"), tr_later("Edit"), Command::Cut).keys(shortcuts::CUT),
        entry(tr_later("Copy"), tr_later("Edit"), Command::Copy).keys(shortcuts::COPY),
        entry(tr_later("Paste in Center"), tr_later("Edit"), Command::Paste).keys(shortcuts::PASTE),
        entry(tr_later("Paste in Place"), tr_later("Edit"), Command::PasteInPlace)
            .keys(shortcuts::PASTE_IN_PLACE),
        entry(tr_later("Duplicate Layer"), tr_later("Edit"), Command::DuplicateLayer)
            .icon(Icon::Duplicate)
            .keys(shortcuts::DUPLICATE),
        entry(tr_later("Delete Layer"), tr_later("Edit"), Command::DeleteLayer).icon(Icon::Trash),
        entry(tr_later("Select All"), tr_later("Edit"), Command::SelectAll).keys(shortcuts::SELECT_ALL),
        entry(tr_later("Define Brush Preset…"), tr_later("Edit"), Command::DefineBrush)
            .also(tr_later("brush tip from selection custom stamp photoshop")),
        entry(tr_later("Define Pattern…"), tr_later("Edit"), Command::DefinePattern)
            .also(tr_later("texture pattern from selection paper photoshop")),
        entry(tr_later("Brushes"), tr_later("Edit"), Command::ShowBrushes)
            .icon(Icon::Brushes)
            .also(tr_later("brush presets panel import abr export")),
        entry(tr_later("Brush Settings"), tr_later("Edit"), Command::ShowBrushSettings)
            .icon(Icon::BrushSettings)
            .also(tr_later("brush engine dynamics scattering texture dual jitter photoshop")),
        entry(tr_later("Convert to Symbol"), tr_later("Modify"), Command::ConvertToSymbol)
            .icon(Icon::Symbol)
            .keys(shortcuts::CONVERT_TO_SYMBOL)
            .also(tr_later("reuse library instance")),
        entry(tr_later("Union"), tr_later("Modify › Combine Shapes"), Command::CombineShapes(ShapeOp::Union))
            .icon(Icon::Combine)
            .also(tr_later("merge join")),
        entry(
            tr_later("Intersect"),
            tr_later("Modify › Combine Shapes"),
            Command::CombineShapes(ShapeOp::Intersect),
        )
        .icon(Icon::Combine),
        entry(tr_later("Punch"), tr_later("Modify › Combine Shapes"), Command::CombineShapes(ShapeOp::Punch))
            .icon(Icon::Combine)
            .also(tr_later("subtract cut hole")),
        entry(tr_later("Crop"), tr_later("Modify › Combine Shapes"), Command::CombineShapes(ShapeOp::Crop))
            .icon(Icon::Combine),
        entry(tr_later("Break Apart"), tr_later("Modify"), Command::BreakApart)
            .icon(Icon::BreakApart)
            .keys(shortcuts::BREAK_APART)
            .also(tr_later("text letters outline")),
        entry(
            tr_later("Distort"),
            tr_later("Modify › Transform"),
            Command::StartLattice(LatticeKind::Distort),
        )
        .icon(Icon::Distort)
        .also(tr_later("perspective corners skew lattice")),
        entry(
            tr_later("Envelope"),
            tr_later("Modify › Transform"),
            Command::StartLattice(LatticeKind::Envelope),
        )
        .icon(Icon::Envelope)
        .also(tr_later("warp bend bulge lattice")),
        entry(tr_later("Edit Symbol"), tr_later("Modify"), Command::EditSymbol).icon(Icon::Symbol),
        entry(tr_later("Swap Symbol…"), tr_later("Modify"), Command::SwapSymbol)
            .icon(Icon::Symbol)
            .also(tr_later("replace")),
        entry(tr_later("Exit Symbol"), tr_later("Modify"), Command::ExitSymbol).icon(Icon::StepOut),
        entry(tr_later("Mask"), tr_later("Modify"), Command::MaskLayerBelow).also(tr_later("clip window")),
        entry(tr_later("Create Clipping Mask"), tr_later("Modify"), Command::ToggleClipping)
            .keys(shortcuts::CLIPPING_MASK)
            .also(tr_later("clip to layer below release photoshop")),
        entry(tr_later("Add Layer Mask"), tr_later("Modify"), Command::AddLayerMask)
            .also(tr_later("hide reveal paint mask photoshop")),
        entry(tr_later("Paint the Mask or Layer"), tr_later("Modify"), Command::EditLayerMask)
            .also(tr_later("layer mask edit")),
        entry(tr_later("Turn Layer Mask Off or On"), tr_later("Modify"), Command::ToggleLayerMask)
            .also(tr_later("disable enable layer mask")),
        entry(tr_later("Apply Layer Mask"), tr_later("Modify"), Command::ApplyLayerMask)
            .also(tr_later("bake layer mask")),
        entry(tr_later("Delete Layer Mask"), tr_later("Modify"), Command::DeleteLayerMask)
            .also(tr_later("remove layer mask")),
        entry(tr_later("Vectorise…"), tr_later("Modify"), Command::Vectorise)
            .also(tr_later("trace bitmap paint to shapes")),
        entry(tr_later("New Layer"), tr_later("Modify"), Command::NewLayer)
            .icon(Icon::Plus)
            .keys(shortcuts::NEW_LAYER),
        entry(tr_later("New Paint Layer"), tr_later("Modify"), Command::NewPaintLayer)
            .also(tr_later("raster brush canvas")),
        entry(tr_later("New Folder"), tr_later("Modify"), Command::NewFolder)
            .icon(Icon::Folder)
            .also(tr_later("group")),
        entry(tr_later("New Skeleton"), tr_later("Modify"), Command::NewSkeleton)
            .icon(Icon::Bones)
            .also(tr_later("rig bones moho character")),
        entry(tr_later("New Switch"), tr_later("Modify"), Command::NewSwitch)
            .icon(Icon::Folder)
            .also(tr_later("mouth hand poses swap")),
        entry(tr_later("Add Shape Hint"), tr_later("Modify"), Command::AddShapeHint)
            .keys(shortcuts::ADD_SHAPE_HINT)
            .also(tr_later("morph tween")),
        entry(
            tr_later("New Adjustment Layer: Curves"),
            tr_later("Modify"),
            Command::NewAdjustmentLayer(AdjustmentKind::Curves),
        )
        .also(tr_later("grade colour tone non-destructive")),
        entry(
            tr_later("New Adjustment Layer: Levels"),
            tr_later("Modify"),
            Command::NewAdjustmentLayer(AdjustmentKind::Levels),
        )
        .also(tr_later("grade tone non-destructive")),
        entry(
            tr_later("New Adjustment Layer: Hue/Saturation"),
            tr_later("Modify"),
            Command::NewAdjustmentLayer(AdjustmentKind::HueSaturation),
        )
        .also(tr_later("grade colour non-destructive")),
        entry(
            tr_later("New Adjustment Layer: Brightness/Contrast"),
            tr_later("Modify"),
            Command::NewAdjustmentLayer(AdjustmentKind::BrightnessContrast),
        )
        .also(tr_later("grade tone non-destructive")),
        entry(tr_later("Merge Down"), tr_later("Modify"), Command::MergeDown)
            .also(tr_later("paint layers flatten")),
        entry(tr_later("New Drawing"), tr_later("Modify"), Command::NewDrawing)
            .also(tr_later("substitution frame by frame")),
        entry(tr_later("Duplicate Drawing"), tr_later("Modify"), Command::DuplicateDrawing),
        entry(tr_later("Next Drawing"), tr_later("Modify"), Command::NextDrawing)
            .keys(shortcuts::NEXT_DRAWING),
        entry(tr_later("Previous Drawing"), tr_later("Modify"), Command::PreviousDrawing)
            .keys(shortcuts::PREVIOUS_DRAWING),
        entry(tr_later("Lip Sync…"), tr_later("Modify"), Command::LipSync)
            .also(tr_later("mouth audio phonemes")),
        entry(tr_later("Insert Keyframe"), tr_later("Timeline"), Command::InsertKeyframe)
            .keys(shortcuts::INSERT_KEYFRAME)
            .also(tr_later("key f6")),
        entry(tr_later("Clear Keyframe"), tr_later("Timeline"), Command::RemoveKeyframe)
            .keys(shortcuts::REMOVE_KEYFRAME),
        entry(tr_later("Insert Frames"), tr_later("Timeline"), Command::InsertFrames)
            .keys(shortcuts::INSERT_FRAMES)
            .also(tr_later("f5 hold")),
        entry(tr_later("Remove Frames"), tr_later("Timeline"), Command::RemoveFrames)
            .keys(shortcuts::REMOVE_FRAMES),
        entry(tr_later("Insert Blank Keyframe"), tr_later("Timeline"), Command::InsertBlankKeyframe)
            .keys(shortcuts::INSERT_BLANK_KEYFRAME),
        entry(tr_later("Reverse Frames"), tr_later("Timeline"), Command::ReverseFrames),
        entry(tr_later("Cut Frames"), tr_later("Timeline"), Command::CutFrames)
            .also(tr_later("keyframes keys move")),
        entry(tr_later("Copy Frames"), tr_later("Timeline"), Command::CopyFrames)
            .also(tr_later("keyframes keys duplicate")),
        entry(tr_later("Paste Frames"), tr_later("Timeline"), Command::PasteFrames)
            .also(tr_later("keyframes keys")),
        entry(tr_later("Create Shape Tween"), tr_later("Timeline"), Command::CreateShapeTween)
            .also(tr_later("morph")),
        entry(tr_later("Draw Inbetweens"), tr_later("Timeline"), Command::MakeInbetweens)
            .icon(Icon::Inbetween)
            .also(tr_later("tween frames assistant")),
        entry(tr_later("Add Camera"), tr_later("Timeline"), Command::AddCamera).icon(Icon::Camera),
        entry(tr_later("Play / Pause"), tr_later("Timeline"), Command::PlayPause)
            .icon(Icon::Play)
            .keys(shortcuts::PLAY),
        entry(tr_later("Zoom In"), tr_later("View"), Command::ZoomIn).keys(shortcuts::ZOOM_IN),
        entry(tr_later("Zoom Out"), tr_later("View"), Command::ZoomOut).keys(shortcuts::ZOOM_OUT),
        entry(tr_later("Fit Stage"), tr_later("View"), Command::ZoomFit).keys(shortcuts::ZOOM_FIT),
        entry(tr_later("Flip View Horizontally"), tr_later("View"), Command::FlipView)
            .also(tr_later("mirror canvas check proportions h")),
        entry(tr_later("Rulers"), tr_later("View"), Command::ToggleRulers)
            .icon(Icon::Ruler)
            .keys(shortcuts::RULERS)
            .also(tr_later("guides")),
        entry(tr_later("Rig Setup (Skeletons at Rest)"), tr_later("View"), Command::ToggleRigSetup)
            .icon(Icon::RigSetup)
            .keys(shortcuts::RIG_SETUP)
            .also(tr_later("bones pose")),
        entry(
            tr_later("One-Point Perspective"),
            tr_later("View › Perspective Guide"),
            Command::SetPerspective(Some(PerspectiveKind::OnePoint)),
        )
        .icon(Icon::Perspective)
        .also(tr_later("vanishing")),
        entry(
            tr_later("Two-Point Perspective"),
            tr_later("View › Perspective Guide"),
            Command::SetPerspective(Some(PerspectiveKind::TwoPoint)),
        )
        .icon(Icon::Perspective)
        .also(tr_later("vanishing")),
        entry(
            tr_later("Three-Point Perspective"),
            tr_later("View › Perspective Guide"),
            Command::SetPerspective(Some(PerspectiveKind::ThreePoint)),
        )
        .icon(Icon::Perspective)
        .also(tr_later("vanishing")),
        entry(
            tr_later("No Perspective Guide"),
            tr_later("View › Perspective Guide"),
            Command::SetPerspective(None),
        )
        .icon(Icon::Perspective),
        entry(tr_later("Show Guides"), tr_later("View › Guides"), Command::ToggleGuides)
            .keys(shortcuts::SHOW_GUIDES),
        entry(tr_later("Snap to Guides"), tr_later("View › Guides"), Command::ToggleSnapToGuides)
            .keys(shortcuts::SNAP_TO_GUIDES),
        entry(tr_later("What's New"), tr_later("Help"), Command::WhatsNew)
            .also(tr_later("changes release notes update version")),
        entry(tr_later("Take the Tour"), tr_later("Help"), Command::StartTour)
            .also(tr_later("welcome introduction tutorial walkthrough")),
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
        tool_entry(Tool::Mixer),
        tool_entry(Tool::PaintBucket),
        tool_entry(Tool::Text),
    ]
}

/// Picking a tool, as an entry.
fn tool_entry(tool: Tool) -> Entry {
    Entry {
        label: tool.name(),
        place: tr_later("Tools"),
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
        egui::Window::new(tr("Find a Command"))
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
                            .hint_text(tr("Find a command or tool: envelope, skeleton, inbetween…"))
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
                        ui.weak(tr("Nothing matches. Try another word."));
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
                ui.weak(tr("↑↓ to choose · Enter to run · Esc to close"));
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
    let label = if current {
        tr_args("{tool}  (current tool)", &[("tool", tr(e.label))])
    } else {
        tr(e.label).to_owned()
    };
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
        tr(e.place),
        egui::FontId::proportional(12.0),
        weak,
    );
    // Not every entry has other words (and tr("") isn't "" in the
    // pseudo-language).
    if e.also.is_empty() { response } else { response.on_hover_text(RichText::new(tr(e.also)).weak()) }
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
