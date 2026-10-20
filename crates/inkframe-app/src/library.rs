//! The Library panel: everything the project can place again and again,
//! as cards with a picture of each. Symbols (made with Modify › Convert to
//! Symbol, F8), imported images, and sounds, filed in folders.
//!
//! Drag a card onto the stage to place it where it's dropped, or onto the
//! timeline to place it from that frame, above that layer. Drag it onto a
//! folder (or a card in one) to file it there. Image and sound files
//! dropped onto the Library from Finder or Explorer are added without
//! being placed (see `InkframeApp::import_dropped_files`).

use std::collections::{HashMap, HashSet};

use egui::{Align2, Color32, CursorIcon, FontId, Id, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{Asset, Symbol};

use crate::editor::Editor;
use crate::icons::{self, Icon, IconButton};

const SMALLEST_CARD: f32 = 56.0;
const LARGEST_CARD: f32 = 160.0;
/// Room under a card's picture for its name.
const LABEL_HEIGHT: f32 = 18.0;
const GAP: f32 = 6.0;

/// A rendered picture of an asset, and what it was rendered from, so we
/// can tell when it's out of date.
struct Thumbnail {
    /// The symbol as it was, for a symbol.
    symbol: Option<Symbol>,
    /// The image library's and fonts' generations when rendered, so a
    /// picture redraws once a file inside it has loaded.
    generations: (u64, u64),
    pixels: u32,
    texture: Option<egui::TextureHandle>,
}

/// A name being typed in place of a card's or folder's label.
enum Renaming {
    Asset(Asset, String),
    Folder(String, String),
}

/// What a card or folder asked for, carried out after drawing so the
/// document doesn't change halfway through a frame.
enum Action {
    Place(Asset),
    Edit(Asset),
    Delete(Asset),
    MoveTo(Asset, String),
    RenameAsset(Asset, String),
    NewFolder,
    RenameFolder(String, String),
    DeleteFolder(String),
}

pub struct LibraryView {
    thumbnails: HashMap<Asset, Thumbnail>,
    search: String,
    card_size: f32,
    /// The card last clicked.
    pub selected: Option<Asset>,
    renaming: Option<Renaming>,
    /// Folders shown closed.
    closed: HashSet<String>,
}

impl Default for LibraryView {
    fn default() -> Self {
        Self {
            thumbnails: HashMap::new(),
            search: String::new(),
            card_size: 84.0,
            selected: None,
            renaming: None,
            closed: HashSet::new(),
        }
    }
}

/// The asset being dragged out of the Library, if any. The stage and the
/// timeline ask this to show where it would land.
pub fn dragged_asset(ctx: &egui::Context) -> Option<Asset> {
    egui::DragAndDrop::payload::<Asset>(ctx).map(|asset| *asset)
}

impl LibraryView {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let assets = editor.doc.library_assets();
        self.thumbnails.retain(|asset, _| assets.contains(asset));
        if self.selected.is_some_and(|s| !assets.contains(&s)) {
            self.selected = None;
        }

        let mut actions = Vec::new();
        self.toolbar(ui, &mut actions);
        ui.add_space(2.0);

        // Dropping a card on the panel's empty space takes it out of its
        // folder. Registered before the cards and folders so they sit on
        // top of it, and checked after them, so it only gets drops they
        // didn't take.
        let background =
            ui.interact(ui.available_rect_before_wrap(), Id::new("library-background"), Sense::hover());

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if assets.is_empty() {
                ui.add_space(8.0);
                ui.weak(
                    "Nothing here yet. Select layers and press F8 to make a symbol, or drop image and sound \
                     files here.",
                );
                return;
            }
            let search = self.search.trim().to_lowercase();
            let doc = &editor.doc;
            let matches = |asset: &Asset| {
                search.is_empty()
                    || doc.asset_name(*asset).is_some_and(|name| name.to_lowercase().contains(&search))
            };
            let in_folder = |folder: &str| -> Vec<Asset> {
                assets
                    .iter()
                    .copied()
                    .filter(|a| doc.asset_folder(*a) == Some(folder))
                    .filter(matches)
                    .collect()
            };
            let top = in_folder("");
            let folders: Vec<(String, Vec<Asset>)> =
                doc.library_folder_names().into_iter().map(|f| (f.clone(), in_folder(&f))).collect();
            let found_any = assets.iter().any(matches);

            self.grid(ui, editor, &top, &mut actions);
            for (folder, contents) in folders {
                // While searching, only folders with something found show,
                // and they open to show it.
                if !search.is_empty() && contents.is_empty() {
                    continue;
                }
                ui.add_space(4.0);
                if self.folder_header(ui, &folder, contents.len(), !search.is_empty(), &mut actions) {
                    ui.indent(("library-folder", &folder), |ui| {
                        if contents.is_empty() {
                            ui.weak("Empty: drag cards here.");
                        }
                        self.grid(ui, editor, &contents, &mut actions);
                    });
                }
            }
            if !found_any {
                ui.weak(format!("Nothing called \"{}\".", self.search.trim()));
            }
        });

        if let Some(asset) = background.dnd_release_payload::<Asset>() {
            actions.push(Action::MoveTo(*asset, String::new()));
        }
        paint_dragged(ui, self, editor);
        for action in actions {
            self.apply(action, editor);
        }
    }

    /// New Folder, card size and search.
    fn toolbar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if ui.add(IconButton::new(Icon::Folder, "New folder")).clicked() {
                actions.push(Action::NewFolder);
            }
            ui.spacing_mut().slider_width = 60.0;
            ui.add(egui::Slider::new(&mut self.card_size, SMALLEST_CARD..=LARGEST_CARD).show_value(false))
                .on_hover_text("Card size");
            ui.add(
                egui::TextEdit::singleline(&mut self.search).hint_text("Search").desired_width(f32::INFINITY),
            );
        });
    }

    /// A folder's row: click to open or close it, drop a card on it to file
    /// the card there, right-click to rename or delete it. Returns whether
    /// it's open.
    fn folder_header(
        &mut self,
        ui: &mut egui::Ui,
        folder: &str,
        count: usize,
        searching: bool,
        actions: &mut Vec<Action>,
    ) -> bool {
        let open = searching || !self.closed.contains(folder);
        if let Some(Renaming::Folder(old, name)) = &mut self.renaming
            && old == folder
        {
            let edit = ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
            edit.request_focus();
            if edit.lost_focus() {
                let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if let Some(Renaming::Folder(old, name)) = self.renaming.take()
                    && !cancelled
                {
                    actions.push(Action::RenameFolder(old, name));
                }
            }
            return open;
        }

        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Folder {folder}"))
        });
        let painter = ui.painter();
        if response.dnd_hover_payload::<Asset>().is_some() {
            painter.rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.6));
        } else if response.hovered() {
            painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let color = ui.visuals().text_color();
        let arrow = Rect::from_center_size(pos2(rect.left() + 9.0, rect.center().y), Vec2::splat(10.0));
        let points = if open {
            vec![arrow.left_top(), arrow.right_top(), arrow.center_bottom()]
        } else {
            vec![arrow.left_top(), arrow.right_center(), arrow.left_bottom()]
        };
        painter.add(egui::Shape::convex_polygon(points, ui.visuals().weak_text_color(), Stroke::NONE));
        let icon = Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), Vec2::splat(15.0));
        icons::paint(painter, if open { Icon::FolderOpen } else { Icon::Folder }, icon, color);
        painter.text(
            pos2(rect.left() + 42.0, rect.center().y),
            Align2::LEFT_CENTER,
            folder,
            FontId::proportional(13.0),
            color,
        );
        painter.text(
            pos2(rect.right() - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(11.0),
            ui.visuals().weak_text_color(),
        );

        if response.clicked() && !searching && !self.closed.remove(folder) {
            self.closed.insert(folder.to_owned());
        }
        if let Some(asset) = response.dnd_release_payload::<Asset>() {
            actions.push(Action::MoveTo(*asset, folder.to_owned()));
        }
        response.context_menu(|ui| {
            if ui.button("Rename").clicked() {
                self.renaming = Some(Renaming::Folder(folder.to_owned(), folder.to_owned()));
            }
            if ui.button("Delete Folder").on_hover_text("What's in it moves to the top level").clicked() {
                actions.push(Action::DeleteFolder(folder.to_owned()));
            }
        });
        open
    }

    /// The cards of one folder (or the top level), wrapping to the panel's
    /// width.
    fn grid(&mut self, ui: &mut egui::Ui, editor: &Editor, assets: &[Asset], actions: &mut Vec<Action>) {
        if assets.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(GAP);
            for &asset in assets {
                self.card(ui, editor, asset, actions);
            }
        });
    }

    /// One card: its picture, its name, and a badge for what kind it is.
    /// Click to select, double-click to edit a symbol or place anything
    /// else, drag to place or file it, right-click for the rest.
    fn card(&mut self, ui: &mut egui::Ui, editor: &Editor, asset: Asset, actions: &mut Vec<Action>) {
        let side = self.card_size;
        let (rect, response) =
            ui.allocate_exact_size(vec2(side, side + LABEL_HEIGHT), Sense::click_and_drag());
        let name = editor.doc.asset_name(asset).unwrap_or_default();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        let picture = Rect::from_min_size(rect.min, Vec2::splat(side));
        let visuals = ui.visuals();
        let selected = self.selected == Some(asset);
        let dropping_here = response.dnd_hover_payload::<Asset>().is_some_and(|dragged| *dragged != asset);

        let painter = ui.painter();
        let fill =
            if response.hovered() { visuals.widgets.hovered.weak_bg_fill } else { visuals.extreme_bg_color };
        painter.rect_filled(picture, 6.0, fill);
        self.paint_picture(ui, editor, asset, picture.shrink(5.0));
        let painter = ui.painter();
        let outline = if selected || dropping_here {
            Stroke::new(2.0, visuals.selection.stroke.color)
        } else {
            Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
        };
        painter.rect_stroke(picture, 6.0, outline, StrokeKind::Inside);
        paint_badge(painter, asset, picture, visuals);

        // The name, or a box to type a new one in.
        let label =
            Rect::from_min_size(pos2(rect.left(), picture.bottom() + 2.0), vec2(side, LABEL_HEIGHT - 2.0));
        if let Some(Renaming::Asset(renamed, name)) = &mut self.renaming
            && *renamed == asset
        {
            let edit = ui.put(label, egui::TextEdit::singleline(name).font(egui::TextStyle::Small));
            edit.request_focus();
            if edit.lost_focus() {
                let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if let Some(Renaming::Asset(asset, name)) = self.renaming.take()
                    && !cancelled
                {
                    actions.push(Action::RenameAsset(asset, name));
                }
            }
        } else {
            let name = editor.doc.asset_name(asset).unwrap_or_default();
            let color = if selected { visuals.strong_text_color() } else { visuals.text_color() };
            let galley = ui.painter().layout(name.to_owned(), FontId::proportional(11.0), color, side);
            // One line, cut short with an ellipsis if it doesn't fit.
            let text = if galley.rows.len() > 1 { elide(ui, name, side) } else { name.to_owned() };
            ui.painter().text(
                label.center_top(),
                Align2::CENTER_TOP,
                text,
                FontId::proportional(11.0),
                color,
            );
        }

        if response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), asset);
            self.selected = Some(asset);
        }
        if response.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
        if response.clicked() {
            self.selected = Some(asset);
        }
        if response.double_clicked() {
            let edits = matches!(asset, Asset::Symbol(_));
            actions.push(if edits { Action::Edit(asset) } else { Action::Place(asset) });
        }
        // Dropped on another card, it joins that card's folder. Dropped
        // back on itself, nothing happens, but the drop is still taken here
        // so the panel behind doesn't move it to the top level.
        if let Some(dragged) = response.dnd_release_payload::<Asset>()
            && *dragged != asset
        {
            let folder = editor.doc.asset_folder(asset).unwrap_or_default().to_owned();
            actions.push(Action::MoveTo(*dragged, folder));
        }
        let response = response.on_hover_ui(|ui| describe(ui, editor, asset));
        response.context_menu(|ui| self.card_menu(ui, editor, asset, actions));
    }

    fn card_menu(&mut self, ui: &mut egui::Ui, editor: &Editor, asset: Asset, actions: &mut Vec<Action>) {
        let doc = &editor.doc;
        let placeable = match asset {
            Asset::Symbol(symbol) => editor.can_place(symbol),
            _ => true,
        };
        let place = if matches!(asset, Asset::Sound(_)) { "Add to Timeline" } else { "Place on Stage" };
        if ui
            .add_enabled(placeable, egui::Button::new(place))
            .on_disabled_hover_text("A symbol can't go inside itself")
            .clicked()
        {
            actions.push(Action::Place(asset));
        }
        if matches!(asset, Asset::Symbol(_)) && ui.button("Edit Symbol").clicked() {
            actions.push(Action::Edit(asset));
        }
        if ui.button("Rename").clicked() {
            let name = doc.asset_name(asset).unwrap_or_default().to_owned();
            self.renaming = Some(Renaming::Asset(asset, name));
        }
        let current = doc.asset_folder(asset).unwrap_or_default().to_owned();
        ui.menu_button("Move to", |ui| {
            if ui.add_enabled(!current.is_empty(), egui::Button::new("Top Level")).clicked() {
                actions.push(Action::MoveTo(asset, String::new()));
            }
            for folder in doc.library_folder_names() {
                if ui.add_enabled(folder != current, egui::Button::new(&folder)).clicked() {
                    actions.push(Action::MoveTo(asset, folder));
                }
            }
        });
        ui.separator();
        let why_not = match asset {
            Asset::Symbol(_) => "Still used on the stage",
            _ => "Still on the stage",
        };
        if ui
            .add_enabled(doc.can_remove_asset(asset), egui::Button::new("Delete"))
            .on_disabled_hover_text(why_not)
            .clicked()
        {
            actions.push(Action::Delete(asset));
        }
    }

    /// Draws an asset's picture into `rect`: a rendering for a symbol or an
    /// image, re-made only when it's out of date, or a speaker for a sound.
    fn paint_picture(&mut self, ui: &egui::Ui, editor: &Editor, asset: Asset, rect: Rect) {
        if let Asset::Sound(_) = asset {
            let icon = Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.45));
            icons::paint(ui.painter(), Icon::Speaker, icon, ui.visuals().weak_text_color());
            return;
        }
        let generations = (editor.images.generation(), inkframe_render::fonts_generation());
        // Render at the display's pixel density so pictures are sharp.
        let pixels = (rect.width() * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
        let symbol = match asset {
            Asset::Symbol(id) => editor.doc.symbol(id),
            _ => None,
        };
        let stale = self.thumbnails.get(&asset).is_none_or(|t| {
            t.symbol.as_ref() != symbol || t.generations != generations || t.pixels != pixels
        });
        if stale {
            let pixmap = match asset {
                Asset::Symbol(id) => {
                    inkframe_render::render_symbol_thumbnail(&editor.doc, Some(&editor.images), id, 0, pixels)
                }
                Asset::Image(id) => inkframe_render::render_image_thumbnail(&editor.images, id, pixels),
                Asset::Sound(_) => None,
            };
            let texture = pixmap.map(|pixmap| {
                let size = [pixmap.width() as usize, pixmap.height() as usize];
                let image = egui::ColorImage::from_rgba_premultiplied(size, pixmap.data());
                ui.ctx().load_texture(format!("library-{asset:?}"), image, egui::TextureOptions::LINEAR)
            });
            self.thumbnails
                .insert(asset, Thumbnail { symbol: symbol.cloned(), generations, pixels, texture });
        }
        match self.thumbnails.get(&asset).and_then(|t| t.texture.as_ref()) {
            Some(texture) => {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
            }
            None => {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "empty",
                    FontId::proportional(10.0),
                    ui.visuals().weak_text_color(),
                );
            }
        }
    }

    fn apply(&mut self, action: Action, editor: &mut Editor) {
        match action {
            Action::Place(asset) => {
                editor.place_asset_at(asset, editor.doc.stage_rect().center());
            }
            Action::Edit(Asset::Symbol(symbol)) => editor.enter_symbol(symbol, 0),
            Action::Edit(_) => {}
            Action::Delete(asset) => {
                editor.doc.remove_asset(asset);
            }
            Action::MoveTo(asset, folder) => editor.doc.move_asset(asset, &folder),
            Action::RenameAsset(asset, name) => editor.doc.rename_asset(asset, &name),
            Action::NewFolder => {
                let folder = editor.doc.add_library_folder();
                self.renaming = Some(Renaming::Folder(folder.clone(), folder));
            }
            Action::RenameFolder(old, new) => {
                // Keep a closed folder closed under its new name.
                if editor.doc.rename_library_folder(&old, &new) && self.closed.remove(&old) {
                    self.closed.insert(new.trim().to_owned());
                }
            }
            Action::DeleteFolder(folder) => {
                editor.doc.remove_library_folder(&folder);
                self.closed.remove(&folder);
            }
        }
    }
}

/// A small mark in a card's corner saying what kind of asset it is.
fn paint_badge(painter: &egui::Painter, asset: Asset, picture: Rect, visuals: &egui::Visuals) {
    let icon = match asset {
        Asset::Symbol(_) => Icon::Library,
        Asset::Image(_) => Icon::Stage,
        Asset::Sound(_) => return,
    };
    let badge = Rect::from_min_size(picture.left_top() + vec2(4.0, 4.0), Vec2::splat(16.0));
    painter.rect_filled(badge, 3.0, visuals.panel_fill.gamma_multiply(0.85));
    icons::paint(painter, icon, badge.shrink(2.5), visuals.weak_text_color());
}

/// `name` cut short with an ellipsis to fit `width`.
fn elide(ui: &egui::Ui, name: &str, width: f32) -> String {
    let fits = |text: &str| {
        ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(11.0), Color32::WHITE).size().x
            <= width
    };
    let chars: Vec<char> = name.chars().collect();
    (1..chars.len())
        .rev()
        .map(|n| format!("{}…", chars[..n].iter().collect::<String>().trim_end()))
        .find(|text| fits(text))
        .unwrap_or_else(|| "…".to_owned())
}

/// The tooltip for a card: its name, what it is, and how it's used.
fn describe(ui: &mut egui::Ui, editor: &Editor, asset: Asset) {
    let doc = &editor.doc;
    ui.strong(doc.asset_name(asset).unwrap_or_default());
    match asset {
        Asset::Symbol(id) => {
            let uses = doc.symbol_use_count(id);
            let frames = doc.symbol(id).map_or(0, |s| s.duration);
            let plural = if uses == 1 { "" } else { "s" };
            ui.label(format!("Symbol · {frames} frames · used {uses} time{plural}"));
            ui.weak("Drag onto the stage or timeline to place it. Double-click to edit it.");
        }
        Asset::Image(id) => {
            if let Some(image) = doc.image(id) {
                ui.label(format!("Image · {} × {} px", image.width, image.height));
            }
            if let Some(problem) = editor.images.problem(id) {
                ui.colored_label(ui.visuals().error_fg_color, problem);
            }
            ui.weak("Drag onto the stage or timeline to place it.");
        }
        Asset::Sound(id) => {
            if let Some(track) = doc.audio_track(id) {
                ui.label(format!("Sound · starts on frame {}", track.start + 1));
            }
            ui.weak("Drag onto the timeline to add another copy from that frame.");
        }
    }
}

/// While a card is being dragged, a small copy of its picture follows the
/// pointer, above everything else.
fn paint_dragged(ui: &egui::Ui, view: &LibraryView, editor: &Editor) {
    let Some(asset) = dragged_asset(ui.ctx()) else { return };
    let Some(pointer) = ui.ctx().pointer_latest_pos() else { return };
    let painter =
        ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("library-dragged")));
    let rect = Rect::from_min_size(pointer + vec2(12.0, 12.0), Vec2::splat(56.0));
    painter.rect_filled(rect, 6.0, ui.visuals().extreme_bg_color.gamma_multiply(0.9));
    painter.rect_stroke(rect, 6.0, Stroke::new(1.5, ui.visuals().selection.stroke.color), StrokeKind::Inside);
    match view.thumbnails.get(&asset).and_then(|t| t.texture.as_ref()) {
        Some(texture) => {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(texture.id(), rect.shrink(5.0), uv, Color32::from_white_alpha(220));
        }
        None => icons::paint(&painter, Icon::Speaker, rect.shrink(16.0), ui.visuals().text_color()),
    }
    let name = editor.doc.asset_name(asset).unwrap_or_default();
    painter.text(
        rect.center_bottom() + vec2(0.0, 3.0),
        Align2::CENTER_TOP,
        name,
        FontId::proportional(11.0),
        ui.visuals().text_color(),
    );
}

#[cfg(test)]
mod tests {
    use egui::Pos2;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use inkframe_core::{Document, TimelineId, Transform};

    use super::*;

    type State = (LibraryView, Editor);

    /// Two symbols, Ball at the top level and Box in the Props folder.
    fn harness() -> (Harness<'static, State>, Asset, Asset) {
        let mut editor = Editor::new(Document::default());
        let ball = editor.doc.add_layer("Ball", vec![], 0, Transform::default());
        let ball = Asset::Symbol(editor.doc.convert_layer_to_symbol(ball, 0).unwrap());
        let box_ = editor.doc.add_layer("Box", vec![], 0, Transform::default());
        let box_ = Asset::Symbol(editor.doc.convert_layer_to_symbol(box_, 0).unwrap());
        editor.doc.rename_asset(ball, "Ball");
        editor.doc.rename_asset(box_, "Box");
        editor.doc.move_asset(box_, "Props");
        // At 60 fps, so two clicks a step apart count as a double-click.
        let mut h = Harness::builder().with_size(vec2(400.0, 500.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, (library, editor): &mut State| library.show(ui, editor),
            (LibraryView::default(), editor),
        );
        h.run();
        (h, ball, box_)
    }

    fn center_of(h: &Harness<'_, State>, label: &str) -> Pos2 {
        h.get_by_label(label).rect().center()
    }

    fn click(h: &mut Harness<'_, State>, at: Pos2) {
        h.drag_at(at);
        h.step();
        h.drop_at(at);
        h.step();
    }

    #[test]
    fn dragging_a_card_onto_a_folder_files_it_there() {
        let (mut h, ball, _) = harness();
        let (from, to) = (center_of(&h, "Ball"), center_of(&h, "Folder Props"));
        h.drag_at(from);
        h.step();
        h.hover_at(from + vec2(10.0, 10.0));
        h.step();
        h.hover_at(to);
        h.step();
        h.drop_at(to);
        h.run();
        assert_eq!(h.state().1.doc.asset_folder(ball), Some("Props"));
    }

    #[test]
    fn double_clicking_a_symbol_opens_it() {
        let (mut h, ball, _) = harness();
        let Asset::Symbol(symbol) = ball else { unreachable!() };
        let card = center_of(&h, "Ball");
        click(&mut h, card);
        click(&mut h, card);
        h.run();
        assert_eq!(h.state().0.selected, Some(ball));
        assert_eq!(h.state().1.timeline(), TimelineId::Symbol(symbol));
    }

    #[test]
    fn searching_shows_only_matching_cards() {
        let (mut h, ..) = harness();
        h.state_mut().0.search = "bo".to_owned();
        h.run();
        assert!(h.query_by_label("Box").is_some(), "found inside its folder");
        assert!(h.query_by_label("Ball").is_none());
    }
}
