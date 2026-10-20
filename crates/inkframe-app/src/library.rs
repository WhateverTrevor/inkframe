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

use crate::audio_engine::AudioEngine;
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
    /// Size in pixels.
    size: (u32, u32),
    /// The symbol frame shown.
    frame: u32,
    /// The box around everything the symbol shows over its whole length,
    /// which every frame is fitted to, so playing it doesn't jitter.
    extent: Option<inkframe_core::Rect>,
    texture: Option<egui::TextureHandle>,
}

impl Thumbnail {
    /// A picture of `asset` on `frame` at `size` pixels, reusing `old` if
    /// it's still right (and its extent, if only the frame or size moved).
    fn render(
        old: Option<&Thumbnail>,
        ctx: &egui::Context,
        editor: &Editor,
        asset: Asset,
        frame: u32,
        size: (u32, u32),
        name: &str,
    ) -> Option<Thumbnail> {
        let generations = (editor.images.generation(), inkframe_render::fonts_generation());
        let symbol = match asset {
            Asset::Symbol(id) => editor.doc.symbol(id),
            _ => None,
        };
        let same_source = old.is_some_and(|t| t.symbol.as_ref() == symbol && t.generations == generations);
        if same_source && old.is_some_and(|t| t.size == size && t.frame == frame) {
            return None;
        }
        let extent = match (asset, old) {
            (Asset::Symbol(_), Some(old)) if same_source => old.extent,
            (Asset::Symbol(id), _) => inkframe_render::symbol_extent(&editor.doc, id),
            _ => None,
        };
        let pixmap = match asset {
            Asset::Symbol(id) => extent.and_then(|extent| {
                inkframe_render::render_symbol_frame(
                    &editor.doc,
                    Some(&editor.images),
                    id,
                    frame,
                    size,
                    extent,
                )
            }),
            Asset::Image(id) => {
                inkframe_render::render_image_thumbnail(&editor.images, id, size.0.min(size.1))
            }
            Asset::Sound(_) => None,
        };
        let texture = pixmap.map(|pixmap| {
            let pixels = [pixmap.width() as usize, pixmap.height() as usize];
            let image = egui::ColorImage::from_rgba_premultiplied(pixels, pixmap.data());
            ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
        });
        Some(Thumbnail { symbol: symbol.cloned(), generations, size, frame, extent, texture })
    }
}

/// The big picture of the selected card at the top of the Library: a
/// symbol playing in a loop, an image, or a sound's waveform.
struct Preview {
    asset: Option<Asset>,
    picture: Option<Thumbnail>,
    playing: bool,
    /// Where a playing symbol has got to, in frames (fractional, so slow
    /// UI frames don't lose time).
    clock: f64,
}

impl Preview {
    /// The symbol frame showing, given the symbol's length.
    fn frame(&self, duration: u32) -> u32 {
        self.clock as u32 % duration.max(1)
    }
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
    preview: Preview,
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
            preview: Preview { asset: None, picture: None, playing: false, clock: 0.0 },
        }
    }
}

/// The asset being dragged out of the Library, if any. The stage and the
/// timeline ask this to show where it would land.
pub fn dragged_asset(ctx: &egui::Context) -> Option<Asset> {
    egui::DragAndDrop::payload::<Asset>(ctx).map(|asset| *asset)
}

impl LibraryView {
    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) {
        let assets = editor.doc.library_assets();
        self.thumbnails.retain(|asset, _| assets.contains(asset));
        if self.selected.is_some_and(|s| !assets.contains(&s)) {
            self.selected = None;
        }

        let mut actions = Vec::new();
        self.toolbar(ui, &mut actions);
        // Always there (while there's anything to preview), so choosing a
        // card doesn't push the cards down under the pointer.
        if !assets.is_empty() {
            egui::CollapsingHeader::new("Preview").id_salt("library-preview").default_open(true).show(
                ui,
                |ui| {
                    self.preview(ui, editor, audio);
                },
            );
        }
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
        // Hovering an animated symbol's card plays it in the card.
        let frame = match asset {
            Asset::Symbol(id) if response.hovered() => editor.doc.symbol(id).map_or(0, |s| {
                let elapsed = ui.input(|i| i.time) * editor.doc.frame_rate.fps();
                if s.duration > 1 {
                    ui.ctx().request_repaint();
                }
                elapsed as u32 % s.duration.max(1)
            }),
            _ => 0,
        };
        self.paint_picture(ui, editor, asset, frame, picture.shrink(5.0));
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

    /// Draws an asset's picture into `rect`: a rendering of a symbol's
    /// `frame` or of an image, re-made only when it's out of date, or a
    /// speaker for a sound.
    fn paint_picture(&mut self, ui: &egui::Ui, editor: &Editor, asset: Asset, frame: u32, rect: Rect) {
        if let Asset::Sound(_) = asset {
            let icon = Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.45));
            icons::paint(ui.painter(), Icon::Speaker, icon, ui.visuals().weak_text_color());
            return;
        }
        // Render at the display's pixel density so pictures are sharp.
        let pixels = (rect.width() * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
        let name = format!("library-{asset:?}");
        let old = self.thumbnails.get(&asset);
        if let Some(fresh) = Thumbnail::render(old, ui.ctx(), editor, asset, frame, (pixels, pixels), &name) {
            self.thumbnails.insert(asset, fresh);
        }
        paint_texture(ui, self.thumbnails.get(&asset), rect);
    }

    /// The selected card, big: a symbol plays in a loop (with play, pause
    /// and a scrubber), an image shows larger, and a sound shows its
    /// waveform with a button to hear it.
    fn preview(&mut self, ui: &mut egui::Ui, editor: &Editor, audio: &mut AudioEngine) {
        let width = ui.available_width();
        let height = (width * 0.6).clamp(110.0, 260.0);
        let Some(asset) = self.selected else {
            if self.preview.asset.take().is_some() {
                audio.stop_preview();
            }
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            ui.painter().rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Click a card to preview it",
                FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
            ui.add_space(controls_height(ui));
            return;
        };
        if self.preview.asset != Some(asset) {
            // A new card starts from the top, playing if it's animated.
            let animated =
                matches!(asset, Asset::Symbol(id) if editor.doc.symbol(id).is_some_and(|s| s.duration > 1));
            self.preview = Preview { asset: Some(asset), picture: None, playing: animated, clock: 0.0 };
            audio.stop_preview();
        }
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        // The controls and details underneath get the same room for every
        // kind of card, so switching cards never moves the ones below.
        let (below, _) = ui.allocate_exact_size(vec2(width, controls_height(ui)), Sense::hover());
        let mut controls = ui.new_child(
            egui::UiBuilder::new().max_rect(below).layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let background = crate::convert::to_color32(editor.doc.background);
        ui.painter().rect_filled(rect, 6.0, background);
        ui.painter().rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            StrokeKind::Inside,
        );
        let inside = rect.shrink(6.0);
        let doc = &editor.doc;
        let project_dir = editor.project_dir();

        match asset {
            Asset::Symbol(id) => {
                let Some(symbol) = doc.symbol(id) else { return };
                let duration = symbol.duration.max(1);
                if self.preview.playing {
                    let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
                    self.preview.clock = (self.preview.clock + dt * doc.frame_rate.fps()) % duration as f64;
                    ui.ctx().request_repaint();
                }
                let frame = self.preview.frame(duration);
                let pixels = |side: f32| (side * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
                let size = (pixels(inside.width()), pixels(inside.height()));
                let name = format!("library-preview-{asset:?}");
                let old = self.preview.picture.as_ref();
                if let Some(fresh) = Thumbnail::render(old, ui.ctx(), editor, asset, frame, size, &name) {
                    self.preview.picture = Some(fresh);
                }
                paint_texture(ui, self.preview.picture.as_ref(), inside);

                controls.horizontal(|ui| {
                    let (icon, tip) = if self.preview.playing {
                        (Icon::Pause, "Pause preview")
                    } else {
                        (Icon::Play, "Play preview")
                    };
                    if ui.add_enabled(duration > 1, IconButton::new(icon, tip)).clicked() {
                        self.preview.playing = !self.preview.playing;
                    }
                    // Frames count from 1 here, as in the timeline.
                    let mut shown = frame + 1;
                    ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(40.0);
                    let scrub = ui.add_enabled(
                        duration > 1,
                        egui::Slider::new(&mut shown, 1..=duration).show_value(false),
                    );
                    if scrub.changed() {
                        self.preview.clock = (shown - 1) as f64;
                        self.preview.playing = false;
                    }
                    ui.weak(format!("{shown} / {duration}"));
                });
                let uses = doc.symbol_use_count(id);
                let plural = if uses == 1 { "" } else { "s" };
                controls.weak(format!("Symbol · {duration} frames · used {uses} time{plural}"));
            }
            Asset::Image(id) => {
                let side = inside.width().min(inside.height());
                let square = Rect::from_center_size(inside.center(), Vec2::splat(side));
                let pixels = (side * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
                let name = format!("library-preview-{asset:?}");
                let old = self.preview.picture.as_ref();
                if let Some(fresh) =
                    Thumbnail::render(old, ui.ctx(), editor, asset, 0, (pixels, pixels), &name)
                {
                    self.preview.picture = Some(fresh);
                }
                paint_texture(ui, self.preview.picture.as_ref(), square);
                if let Some(image) = doc.image(id) {
                    let placed = if doc.image_in_use(id) { "on the stage" } else { "not on the stage" };
                    controls.weak(format!("Image · {} × {} px · {placed}", image.width, image.height));
                }
            }
            Asset::Sound(id) => {
                let Some(track) = doc.audio_track(id) else { return };
                let fps = doc.frame_rate.fps();
                let peaks = audio.peaks(track, project_dir.as_deref(), fps);
                paint_waveform(ui, peaks.as_deref().map(Vec::as_slice), inside);
                let length = audio.duration(track, project_dir.as_deref());
                let position = audio.preview_position(track, project_dir.as_deref());
                if let (Some(position), Some(length)) = (position, length) {
                    let x = inside.left() + inside.width() * (position / length.max(1e-6)).min(1.0) as f32;
                    ui.painter().vline(
                        x,
                        inside.y_range(),
                        Stroke::new(2.0, ui.visuals().selection.stroke.color),
                    );
                    ui.ctx().request_repaint();
                }
                controls.horizontal(|ui| {
                    let playing = position.is_some();
                    let (icon, tip) =
                        if playing { (Icon::Pause, "Stop sound") } else { (Icon::Play, "Play sound") };
                    if ui.add_enabled(peaks.is_some(), IconButton::new(icon, tip)).clicked() {
                        if playing {
                            audio.stop_preview();
                        } else {
                            audio.preview(track, project_dir.as_deref());
                        }
                    }
                    let seconds = length.map_or_else(|| "loading…".to_owned(), |s| format!("{s:.1} s"));
                    ui.weak(format!("Sound · {seconds} · starts on frame {}", track.start + 1));
                });
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

/// Room under the preview for a row of controls and a line of details.
fn controls_height(ui: &egui::Ui) -> f32 {
    ui.spacing().interact_size.y * 2.0 + ui.spacing().item_spacing.y * 2.0
}

/// Draws a rendered picture into `rect`, or says there's nothing to show.
fn paint_texture(ui: &egui::Ui, picture: Option<&Thumbnail>, rect: Rect) {
    match picture.and_then(|t| t.texture.as_ref()) {
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

/// A sound's loudness across its length, as bars, mirrored about the
/// middle like the timeline's waveforms.
fn paint_waveform(ui: &egui::Ui, peaks: Option<&[f32]>, rect: Rect) {
    let color = ui.visuals().weak_text_color();
    let Some(peaks) = peaks.filter(|p| !p.is_empty()) else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Loading…",
            FontId::proportional(11.0),
            color,
        );
        return;
    };
    // One bar per couple of points, each the loudest of the frames it covers.
    let bars = (rect.width() / 2.0).floor().max(1.0) as usize;
    let middle = rect.center().y;
    for bar in 0..bars {
        let from = bar * peaks.len() / bars;
        let to = ((bar + 1) * peaks.len() / bars).max(from + 1).min(peaks.len());
        let loudest = peaks[from..to].iter().fold(0.0f32, |a, &b| a.max(b)).min(1.0);
        let x = rect.left() + (bar as f32 + 0.5) * rect.width() / bars as f32;
        let half = (loudest * rect.height() / 2.0).max(0.5);
        ui.painter().vline(x, (middle - half)..=(middle + half), Stroke::new(1.0, color));
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

    type State = (LibraryView, Editor, AudioEngine);

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
        let mut h = Harness::builder().with_size(vec2(400.0, 600.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, (library, editor, audio): &mut State| library.show(ui, editor, audio),
            (LibraryView::default(), editor, AudioEngine::without_output()),
        );
        h.run_steps(4);
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
        h.run_steps(4);
        assert_eq!(h.state().1.doc.asset_folder(ball), Some("Props"));
    }

    #[test]
    fn double_clicking_a_symbol_opens_it() {
        let (mut h, ball, _) = harness();
        let Asset::Symbol(symbol) = ball else { unreachable!() };
        let card = center_of(&h, "Ball");
        click(&mut h, card);
        click(&mut h, card);
        h.run_steps(4);
        assert_eq!(h.state().0.selected, Some(ball));
        assert_eq!(h.state().1.timeline(), TimelineId::Symbol(symbol));
    }

    #[test]
    fn the_preview_plays_the_selected_symbol_and_can_be_paused_and_scrubbed() {
        let (mut h, ball, _) = harness();
        let card = center_of(&h, "Ball");
        click(&mut h, card);
        assert_eq!(h.state().0.preview.asset, Some(ball));
        assert!(h.state().0.preview.playing, "an animated symbol starts playing");

        // Half a second at 24 fps is about 12 frames.
        for _ in 0..30 {
            h.step();
        }
        let frame = h.state().0.preview.frame(48);
        assert!((10..=14).contains(&frame), "on frame {frame}");

        h.get_by_label("Pause preview").click();
        h.run_steps(4);
        let paused = h.state().0.preview.frame(48);
        for _ in 0..30 {
            h.step();
        }
        assert_eq!(h.state().0.preview.frame(48), paused, "paused");
        h.get_by_label("Play preview").click();
        h.run_steps(4);
        assert!(h.state().0.preview.playing);

        // Choosing another card starts it from its first frame.
        let other = center_of(&h, "Box");
        click(&mut h, other);
        assert_eq!(h.state().0.preview.frame(48), 0);
    }

    #[test]
    fn searching_shows_only_matching_cards() {
        let (mut h, ..) = harness();
        h.state_mut().0.search = "bo".to_owned();
        h.run_steps(4);
        assert!(h.query_by_label("Box").is_some(), "found inside its folder");
        assert!(h.query_by_label("Ball").is_none());
    }
}
