//! Showing each area of the window: the panel it holds, the stage and its
//! breadcrumbs, and the hint over an empty stage while files are dragged in.

use std::collections::BTreeMap;

use inkframe_core::{Clipboard, SymbolId, TimelineId};

use super::InkframeApp;
use crate::editor::Editor;
use crate::i18n::tr;
use crate::layout::{AreaId, Kind, Layout};
use crate::stage::StageView;
use crate::timeline::TimelineView;
use crate::xsheet::ExposureSheet;
use crate::{inspector, panels, toolbar};

impl InkframeApp {
    /// One area's editor.
    pub(super) fn show_area(&mut self, ui: &mut egui::Ui, area: AreaId, kind: Kind) {
        // Where the tour can find this area.
        let targets = &mut self.tour_targets;
        match kind {
            Kind::Tools => crate::tour::Targets::note(&mut targets.tools, ui.max_rect()),
            Kind::Properties => crate::tour::Targets::note(&mut targets.properties, ui.max_rect()),
            Kind::Timeline => crate::tour::Targets::note(&mut targets.timeline, ui.max_rect()),
            _ => {}
        }
        match kind {
            Kind::Stage => {
                // What can be done now, above the stage (see `context_bar.rs`).
                let top = ui.cursor().top();
                let asked = crate::context_bar::show(ui, &mut self.editor);
                let full = ui.max_rect();
                let bar = egui::Rect::from_x_y_ranges(full.x_range(), top..=ui.cursor().top());
                crate::tour::Targets::note(&mut self.tour_targets.context_bar, bar);
                crate::tour::Targets::note(
                    &mut self.tour_targets.stage,
                    egui::Rect::from_x_y_ranges(full.x_range(), bar.max.y..=full.max.y),
                );
                if self.editor.timeline() != TimelineId::Main {
                    breadcrumbs(ui, &mut self.editor);
                }
                if let Some(stage) = self.views.stages.get_mut(&area) {
                    stage.show(ui, &mut self.editor);
                }
                if let Some(command) = asked {
                    self.execute(command);
                }
            }
            Kind::Timeline | Kind::GraphEditor => {
                let frames_copied = matches!(self.clipboard, Some(Clipboard::Frames(_)));
                let mut asked = None;
                if let Some(timeline) = self.views.timelines.get_mut(&area) {
                    // The area's kind says which mode; the Graph button
                    // in the transport bar switches the kind.
                    let graph = kind == Kind::GraphEditor;
                    timeline.set_graph_mode(graph);
                    asked = timeline.show(ui, &mut self.editor, &mut self.audio, frames_copied);
                    if timeline.graph_mode() != graph {
                        let other = if graph { Kind::Timeline } else { Kind::GraphEditor };
                        self.views.retype = Some((area, other));
                    }
                }
                if let Some(command) = asked {
                    self.execute(command);
                }
            }
            Kind::Properties => {
                if let Some(inspector::AudioAction::Import) =
                    inspector::show(ui, &mut self.editor, &mut self.audio)
                {
                    self.import_audio();
                }
            }
            Kind::Tools => {
                egui::ScrollArea::vertical()
                    .id_salt(("tools", area))
                    .show(ui, |ui| toolbar::show(ui, &mut self.editor));
            }
            // The Library scrolls its cards itself, under the preview.
            Kind::Library => self.library.show(ui, &mut self.editor, &mut self.audio),
            Kind::ExposureSheet => {
                if let Some(sheet) = self.views.sheets.get_mut(&area) {
                    sheet.contents(ui, &mut self.editor, &mut self.audio);
                }
            }
            Kind::ToolSettings => {
                egui::ScrollArea::vertical()
                    .id_salt(("tool_settings", area))
                    .show(ui, |ui| inspector::tool_settings(ui, &mut self.editor));
            }
            Kind::Audio => {
                egui::ScrollArea::vertical().id_salt(("audio", area)).show(ui, |ui| {
                    if let Some(inspector::AudioAction::Import) =
                        inspector::audio_section(ui, &mut self.editor, &mut self.audio)
                    {
                        self.import_audio();
                    }
                });
            }
            Kind::Document => {
                egui::ScrollArea::vertical()
                    .id_salt(("document", area))
                    .show(ui, |ui| inspector::document(ui, &mut self.editor));
            }
            Kind::Colour => {
                egui::ScrollArea::vertical()
                    .id_salt(("colour", area))
                    .show(ui, |ui| crate::color_picker::panel(ui, &mut self.editor));
            }
            Kind::Swatches => {
                egui::ScrollArea::vertical()
                    .id_salt(("swatches", area))
                    .show(ui, |ui| panels::swatches(ui, &mut self.editor, &mut self.panels));
            }
            Kind::Align => {
                egui::ScrollArea::vertical()
                    .id_salt(("align", area))
                    .show(ui, |ui| panels::align(ui, &mut self.editor, &mut self.panels));
            }
            Kind::Bones => {
                egui::ScrollArea::vertical()
                    .id_salt(("bones", area))
                    .show(ui, |ui| crate::bones_panel::show(ui, &mut self.editor));
            }
            Kind::MotionPresets => {
                egui::ScrollArea::vertical()
                    .id_salt(("presets", area))
                    .show(ui, |ui| self.presets.show(ui, &mut self.editor));
            }
            Kind::Templates => {
                egui::ScrollArea::vertical()
                    .id_salt(("templates", area))
                    .show(ui, |ui| self.templates.show(ui, &mut self.editor));
            }
            Kind::NodeView => {
                if let Some(view) = self.views.nodes.get_mut(&area) {
                    view.show(ui, &mut self.editor);
                }
            }
            Kind::ScriptConsole => self.scripts.console(ui, &mut self.editor),
            Kind::DepthView => {
                if let Some(view) = self.views.depths.get_mut(&area) {
                    view.show(ui, &mut self.editor);
                }
            }
            Kind::Controllers => {
                egui::ScrollArea::vertical()
                    .id_salt(("controllers", area))
                    .show(ui, |ui| panels::controllers(ui, &mut self.editor));
            }
            Kind::Brushes => {
                egui::ScrollArea::vertical()
                    .id_salt(("brushes", area))
                    .show(ui, |ui| crate::brush_panel::brushes(ui, &mut self.editor, &mut self.brush_panels));
            }
            Kind::BrushSettings => {
                egui::ScrollArea::both().id_salt(("brush_settings", area)).show(ui, |ui| {
                    crate::brush_panel::brush_settings(ui, &mut self.editor, &mut self.brush_panels)
                });
            }
            Kind::History => {
                egui::ScrollArea::vertical()
                    .id_salt(("history", area))
                    .stick_to_bottom(true)
                    .show(ui, |ui| panels::history(ui, &mut self.editor));
            }
        }
    }
}

/// The "Scene ▸ Symbol ▸ …" bar shown while editing a symbol. Clicking an
/// earlier step goes back out to it.
/// The editors that keep state of their own, one per area showing them, and
/// which stage menu commands act on.
#[derive(Default)]
pub(super) struct Views {
    pub(super) stages: BTreeMap<AreaId, StageView>,
    pub(super) timelines: BTreeMap<AreaId, TimelineView>,
    pub(super) sheets: BTreeMap<AreaId, ExposureSheet>,
    pub(super) nodes: BTreeMap<AreaId, crate::node_view::NodeView>,
    pub(super) depths: BTreeMap<AreaId, crate::depth_view::DepthView>,
    /// The stage last pointed at.
    pub(super) active_stage: Option<AreaId>,
    /// An area to change to another kind once the layout has drawn: a
    /// timeline whose Graph button was pressed, say.
    pub(super) retype: Option<(AreaId, Kind)>,
    /// Stands in when no area shows the stage, so commands still have one.
    pub(super) spare_stage: StageView,
}

impl Views {
    /// Makes views for areas that need one, and forgets those of areas
    /// that have gone or changed kind.
    pub(super) fn prepare(&mut self, layout: &Layout) {
        // Every tab, showing or not, so a hidden tab keeps its view.
        let areas = layout.all_tabs();
        let of = |kind: Kind| areas.iter().filter(move |a| a.1 == kind).map(|a| a.0).collect::<Vec<_>>();
        let (stages, sheets, graphs) = (of(Kind::Stage), of(Kind::ExposureSheet), of(Kind::NodeView));
        let timelines: Vec<AreaId> = areas.iter().filter(|a| a.1.is_timeline()).map(|a| a.0).collect();
        self.stages.retain(|id, _| stages.contains(id));
        self.timelines.retain(|id, _| timelines.contains(id));
        self.sheets.retain(|id, _| sheets.contains(id));
        self.nodes.retain(|id, _| graphs.contains(id));
        let depths = of(Kind::DepthView);
        self.depths.retain(|id, _| depths.contains(id));
        for id in depths {
            self.depths.entry(id).or_default();
        }
        for id in graphs {
            self.nodes.entry(id).or_default();
        }
        for id in stages {
            self.stages.entry(id).or_default();
        }
        for id in timelines {
            self.timelines.entry(id).or_default();
        }
        for id in sheets {
            self.sheets.entry(id).or_default();
        }
        if self.active_stage.is_none_or(|id| !self.stages.contains_key(&id)) {
            self.active_stage = self.stages.keys().next().copied();
        }
    }

    /// The stage menu commands act on.
    pub(super) fn stage(&mut self) -> &mut StageView {
        match self.active_stage.and_then(|id| self.stages.get_mut(&id)) {
            Some(stage) => stage,
            None => &mut self.spare_stage,
        }
    }

    /// Whether a gesture is under way in any stage or timeline, which
    /// holds off recording an undo step.
    pub(super) fn is_busy(&self) -> bool {
        self.stages.values().any(StageView::is_busy) || self.timelines.values().any(TimelineView::is_busy)
    }
}

pub(super) fn breadcrumbs(ui: &mut egui::Ui, editor: &mut Editor) {
    let path: Vec<(SymbolId, String)> = editor
        .open_symbols()
        .map(|id| (id, editor.doc.symbol(id).map_or_else(|| "?".to_owned(), |s| s.name.clone())))
        .collect();
    let mut exit_to = None;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).fill(ui.visuals().panel_fill).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui
                    .link(tr("Scene"))
                    .on_hover_text(tr("Back to the main timeline (Esc steps out one level)"))
                    .clicked()
                {
                    exit_to = Some(0);
                }
                for (depth, (_, name)) in path.iter().enumerate() {
                    ui.weak("›");
                    // The last step is where we are, so it isn't a link.
                    if depth + 1 == path.len() {
                        ui.strong(name);
                    } else if ui.link(name).clicked() {
                        exit_to = Some(depth + 1);
                    }
                }
            });
        },
    );
    if let Some(depth) = exit_to {
        editor.exit_to(depth);
    }
}

/// Shown over the window while files are dragged over it.
pub(super) fn drop_hint(ctx: &egui::Context) {
    let area = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_hint")));
    painter.rect_filled(area, 0.0, egui::Color32::from_black_alpha(140));
    painter.text(
        area.center(),
        egui::Align2::CENTER_CENTER,
        tr("Drop images, sounds or fonts to import them"),
        egui::FontId::proportional(20.0),
        egui::Color32::WHITE,
    );
}
