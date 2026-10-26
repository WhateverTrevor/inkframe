//! The Templates panel: the template library (see `template.rs` in the
//! core), as Harmony's. Templates are files in the Templates folder beside
//! Inkframe's settings, shared by every project. The panel saves the
//! selected layers as one and inserts one at the playhead.

use std::path::{Path, PathBuf};

use egui::{Sense, TextureHandle, TextureOptions, Vec2, vec2};
use inkframe_core::{Document, TEMPLATE_EXTENSION, Template};
use inkframe_render::RenderOptions;

use crate::editor::Editor;
use crate::i18n::tr;
use crate::icons::{Icon, IconButton};
use crate::theme;

/// The size of a template's thumbnail, in points.
const THUMBNAIL: Vec2 = vec2(150.0, 84.0);

#[derive(Default)]
pub struct TemplateLibrary {
    /// Where templates live; the settings folder's unless a test says.
    folder: Option<PathBuf>,
    /// What's in the folder, read when the panel first shows (and again
    /// after a save or delete, or on Refresh).
    entries: Option<Vec<Entry>>,
    /// The name for the next template saved.
    name: String,
    /// What went wrong last, to show.
    problem: Option<String>,
}

struct Entry {
    path: PathBuf,
    template: Template,
    /// Drawn the first time it's shown.
    thumbnail: Option<TextureHandle>,
}

impl TemplateLibrary {
    /// Keeps templates in `folder` rather than the settings folder's (for
    /// screenshots, which mustn't touch the real library).
    pub fn use_folder(&mut self, folder: PathBuf) {
        self.folder = Some(folder);
        self.entries = None;
    }

    /// Saves the selected layers as a template (for screenshots).
    pub fn save_selected(&mut self, editor: &Editor, name: &str) {
        self.problem = self.save(editor, name).err();
    }

    fn folder(&self) -> Option<PathBuf> {
        self.folder.clone().or_else(|| crate::layout::settings_dir().map(|dir| dir.join("Templates")))
    }

    /// Reads every template in the folder, alphabetically. Files that
    /// can't be read are left out.
    fn scan(&mut self) {
        let mut entries: Vec<Entry> = self
            .folder()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                (path.extension()? == TEMPLATE_EXTENSION).then_some(path)
            })
            .filter_map(|path| {
                let template = Template::from_json(&std::fs::read_to_string(&path).ok()?).ok()?;
                Some(Entry { path, template, thumbnail: None })
            })
            .collect();
        entries.sort_by_key(|e| e.template.name.to_lowercase());
        self.entries = Some(entries);
    }

    /// Saves the selected layers as a template named `name`.
    fn save(&mut self, editor: &Editor, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(tr("Give the template a name first.").to_owned());
        }
        let ids = editor.selected_with_folders();
        let template = editor
            .doc
            .make_template(name, &ids, editor.project_dir().as_deref())
            .ok_or(tr("Select the layers to save first."))?;
        let folder = self.folder().ok_or(tr("There's no settings folder to save templates in."))?;
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = free_path(&folder, name);
        let json = template.to_json().map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        self.scan();
        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        if self.entries.is_none() {
            self.scan();
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text(tr("Template name"))
                    .desired_width(110.0),
            );
            let can_save = !editor.selected_ids().is_empty();
            let save = ui.add_enabled(can_save, egui::Button::new(tr("Save Selected"))).on_hover_text(tr(
                tr("Save the selected layers, with their animation, as a template for any project"),
            ));
            if save.clicked() {
                let name = self.name.clone();
                self.problem = self.save(editor, &name).err();
                if self.problem.is_none() {
                    self.name.clear();
                }
            }
            if ui.add(IconButton::new(Icon::History, tr("Read the Templates folder again"))).clicked() {
                self.scan();
            }
        });
        if let Some(problem) = &self.problem {
            ui.colored_label(ui.visuals().warn_fg_color, problem);
        }
        ui.add_space(6.0);

        let mut insert = None;
        let mut delete = None;
        let entries = self.entries.as_mut().expect("scanned above");
        if entries.is_empty() {
            ui.weak(tr(
                "No templates yet. Select a rig, a cycle or a prop and save it here to use in any project.",
            ));
        }
        // Cards in rows as wide as the panel allows.
        let gap = ui.spacing().item_spacing.x;
        let per_row = (((ui.available_width() + gap) / (THUMBNAIL.x + gap)).floor() as usize).max(1);
        for (row, chunk) in entries.chunks_mut(per_row).enumerate() {
            ui.horizontal_top(|ui| {
                for (j, entry) in chunk.iter_mut().enumerate() {
                    let i = row * per_row + j;
                    ui.vertical(|ui| {
                        ui.set_width(THUMBNAIL.x);
                        let (rect, response) = ui.allocate_exact_size(THUMBNAIL, Sense::click());
                        ui.painter().rect_filled(rect, 4.0, theme::SUNKEN);
                        let texture = entry
                            .thumbnail
                            .get_or_insert_with(|| thumbnail(ui.ctx(), editor, &entry.template));
                        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                        ui.painter().image(texture.id(), rect.shrink(2.0), uv, egui::Color32::WHITE);
                        if response.hovered() {
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                (1.5, theme::ACCENT),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let response =
                            response.on_hover_text(tr("Double-click (or Insert) to add it at the playhead"));
                        if response.double_clicked() {
                            insert = Some(i);
                        }
                        response.context_menu(|ui| {
                            if ui.button(tr("Insert at Playhead")).clicked() {
                                insert = Some(i);
                                ui.close();
                            }
                            if ui.button(tr("Delete Template")).clicked() {
                                delete = Some(i);
                                ui.close();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(&entry.template.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button(tr("Insert")).clicked() {
                                    insert = Some(i);
                                }
                            });
                        });
                    });
                }
            });
        }
        if let Some(i) = insert {
            let template = entries[i].template.clone();
            editor.insert_template(&template);
        }
        if let Some(i) = delete {
            if let Err(problem) = std::fs::remove_file(&entries[i].path) {
                self.problem = Some(problem.to_string());
            }
            self.scan();
        }
    }
}

/// A path for a new template called `name` in `folder`, numbered if one
/// of that name is there already.
fn free_path(folder: &Path, name: &str) -> PathBuf {
    // Keep the name readable but safe as a file name everywhere.
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .collect();
    let path = |n: usize| {
        let stem = if n == 1 { stem.clone() } else { format!("{stem} {n}") };
        folder.join(format!("{stem}.{TEMPLATE_EXTENSION}"))
    };
    (1..).map(path).find(|p| !p.exists()).expect("some number is free")
}

/// A template drawn at its first frame, framed to fit the thumbnail, on
/// the open document's background.
fn thumbnail(ctx: &egui::Context, editor: &Editor, template: &Template) -> TextureHandle {
    let mut doc = Document::default();
    (doc.width, doc.height, doc.background) = (editor.doc.width, editor.doc.height, editor.doc.background);
    doc.duration = editor.doc.duration.max(template.first_frame + 1);
    let frame = template.first_frame;
    let ids = doc.insert_template(template, inkframe_core::TimelineId::Main, frame, None);
    let size = THUMBNAIL * ctx.pixels_per_point();
    let (width, height) = (size.x.round() as u32, size.y.round() as u32);
    let image = tiny_skia::Pixmap::new(width, height)
        .map(|mut pixmap| {
            pixmap.fill(tiny_skia::Color::from_rgba8(
                doc.background.r,
                doc.background.g,
                doc.background.b,
                255,
            ));
            // Its drawing's bounds, with a margin, fitted and centred.
            let bounds =
                ids.iter().filter_map(|&id| doc.stage_bounds(id, frame)).reduce(inkframe_core::Rect::union);
            if let Some(bounds) = bounds {
                let scale = 0.85
                    * (width as f32 / bounds.width().max(1.0)).min(height as f32 / bounds.height().max(1.0));
                let centre = bounds.center();
                let fit = inkframe_core::Affine::translate(inkframe_core::Point::new(
                    width as f32 / 2.0,
                    height as f32 / 2.0,
                ))
                .pre_concat(inkframe_core::Affine::scale(scale, scale))
                .pre_concat(inkframe_core::Affine::translate(inkframe_core::Point::ZERO - centre));
                let options = RenderOptions { draw_background: false, ..RenderOptions::default() };
                inkframe_render::render_timeline_into(
                    &mut pixmap,
                    &doc,
                    inkframe_core::TimelineId::Main,
                    frame,
                    fit,
                    None,
                    &options,
                );
            }
            egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], pixmap.data())
        })
        .unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]));
    ctx.load_texture(format!("template-{}", template.name), image, TextureOptions::LINEAR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Point, Transform};

    #[test]
    fn saving_then_inserting_brings_the_layers_back_at_the_playhead() {
        let folder = std::env::temp_dir().join(format!("inkframe-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut library = TemplateLibrary { folder: Some(folder.clone()), ..Default::default() };

        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("Prop", Vec::new(), 0, Transform::at(Point::new(10.0, 10.0)));
        editor.select(Some(id));
        assert!(library.save(&editor, "").is_err(), "needs a name");
        library.save(&editor, "Prop: v1").unwrap();
        library.save(&editor, "Prop: v1").unwrap();
        let names: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "the second is numbered, not overwritten: {names:?}");

        let template = library.entries.as_ref().unwrap()[0].template.clone();
        editor.set_frame(5);
        editor.insert_template(&template);
        let inserted = editor.selected_layer().unwrap();
        assert_ne!(inserted.id, id);
        assert_eq!(inserted.start, 5);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
