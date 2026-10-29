//! Opening, saving and importing: documents and examples, pictures, fonts,
//! sound and dropped files, the PSD export, recovering after a crash, and
//! asking before unsaved changes are lost.

use std::path::{Path, PathBuf};

use inkframe_core::Document;
use inkframe_render::IMAGE_EXTENSIONS;

use super::{FILE_EXTENSION, InkframeApp, file_name, show_error};
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::layout::Kind;
use crate::prefs::Prefs;

impl InkframeApp {
    pub(super) fn current_prefs(&self) -> Prefs {
        Prefs {
            shape_per_layer: !self.editor.draw_into_layer,
            show_rulers: self.editor.show_rulers,
            hide_guides: !self.editor.show_guides,
            lock_guides: self.editor.lock_guides,
            no_guide_snapping: !self.editor.snap_to_guides,
            toured: self.toured,
            seen_version: self.seen_version.clone(),
            language: self.language.clone(),
            brush: Some(self.editor.brush.clone()),
            eraser: Some(self.editor.eraser.clone()),
            brush_name: self.editor.brushes.picked.clone(),
            symmetry: self.editor.symmetry,
            smoothing: Some(self.editor.smoothing),
            pressure_feel: self.editor.pressure_feel,
            recent_colors: crate::color_picker::recent(),
            no_update_check: !self.update_prefs.automatic,
            update_checked_at: self.update_prefs.checked_at,
            skipped_version: self.update_prefs.skipped.clone(),
            folded_sections: self.editor.folded_sections.iter().cloned().collect(),
            ..Prefs::gather(self.editor.guides, self.batch.jobs(), self.batch.chosen_folder())
        }
    }

    pub(super) fn replace_editor(&mut self, editor: Editor) {
        // The tool and guides are the person's, not the document's.
        let (tool, guides) = (self.editor.tool, self.editor.guides);
        self.editor = editor;
        self.editor.tool = tool;
        self.editor.guides = guides;
        self.views.stage().zoom_to_fit();
    }

    pub(super) fn open(&mut self) {
        if !self.confirm_discard_changes() {
            return;
        }
        let Some(path) =
            rfd::FileDialog::new().add_filter(tr("Inkframe document"), &[FILE_EXTENSION]).pick_file()
        else {
            return;
        };
        match Editor::open(&path) {
            Ok(editor) => self.replace_editor(editor),
            Err(err) => {
                show_error(&tr_args("Couldn't open {file}", &[("file", &file_name(&path))]), &err.to_string())
            }
        }
    }

    /// Opens an example as a new project, so saving it asks where to.
    pub(super) fn open_example(&mut self, path: &Path) {
        if !self.confirm_discard_changes() {
            return;
        }
        match Document::load(path) {
            Ok(doc) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.replace_editor(Editor::untitled_copy(doc, name));
            }
            Err(err) => {
                show_error(&tr_args("Couldn't open {file}", &[("file", &file_name(path))]), &err.to_string())
            }
        }
    }

    pub(super) fn import_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Import to stage"))
            .add_filter(tr("Images and Photoshop files"), &[&IMAGE_EXTENSIONS[..], &["psd"]].concat())
            .pick_file()
        else {
            return;
        };
        if let Err(message) = self.editor.import_image(&path) {
            show_error(tr("Couldn't import the image"), &message);
        }
    }

    /// File › Import Sprite Sheet…: its picture or its data file.
    pub(super) fn import_sprite_sheet(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Import a sprite sheet"))
            .add_filter(
                tr("Sprite sheets and their data files"),
                &[&IMAGE_EXTENSIONS[..], &["json"]].concat(),
            )
            .pick_file()
        else {
            return;
        };
        if let Err(message) = self.sprite_import.open(&path, &self.editor) {
            show_error(tr("Couldn't read the sprite sheet"), &message);
        }
    }

    /// File › Export PSD: the current frame, layer by layer.
    pub(super) fn export_psd(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Export Photoshop file"))
            .add_filter(tr("Photoshop file"), &["psd"])
            .set_file_name(format!("{}.psd", self.editor.display_name()))
            .save_file()
        else {
            return;
        };
        if let Err(message) = self.editor.export_psd(&path) {
            show_error(tr("Couldn't export the Photoshop file"), &message);
        }
    }

    /// Imports image and sound files dropped onto the window, the same as
    /// choosing them from the File menu. Images dropped onto the Library go
    /// into it without being placed on the stage.
    pub(super) fn import_dropped_files(&mut self, ctx: &egui::Context) {
        let onto_library =
            self.hovered_area.is_some_and(|area| self.layout.kind_of(area) == Some(Kind::Library));
        // A file dropped without a path on disk can't be linked to, so skip it.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if onto_library && IMAGE_EXTENSIONS.contains(&extension.as_str()) {
                if let Err(message) = self.editor.add_library_image(&path) {
                    show_error(tr("Couldn't import the image"), &message);
                }
            } else if IMAGE_EXTENSIONS.contains(&extension.as_str()) || extension == "psd" {
                if let Err(message) = self.editor.import_image(&path) {
                    show_error(tr("Couldn't import the image"), &message);
                }
            } else if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
                self.editor.import_audio(&path);
            } else if FONT_EXTENSIONS.contains(&extension.as_str()) {
                self.import_font(&path);
            } else {
                show_error(
                    tr("Can't import that file"),
                    &tr_args(
                        "{file} isn't an image, Photoshop, sound or font file Inkframe can read.",
                        &[("file", &file_name(&path))],
                    ),
                );
            }
        }
    }

    /// Several at once, since a family often comes as separate files for
    /// regular, bold and italic.
    pub(super) fn import_fonts(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .set_title(tr("Import fonts"))
            .add_filter(tr("Fonts"), &FONT_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        for path in paths {
            self.import_font(&path);
        }
    }

    pub(super) fn import_font(&mut self, path: &Path) {
        if let Err(message) = self.editor.import_font(path) {
            show_error(tr("Couldn't import the font"), &message);
        }
    }

    pub(super) fn import_audio(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Import audio"))
            .add_filter(tr("Audio"), &AUDIO_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        self.editor.import_audio(&path);
    }

    /// Saves to the current file, asking for a location if there isn't one.
    /// Returns whether the document was saved.
    pub(super) fn save(&mut self) -> bool {
        match self.editor.path.clone() {
            Some(path) => self.save_to(&path),
            None => self.save_as(),
        }
    }

    pub(super) fn save_as(&mut self) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(tr("Inkframe document"), &[FILE_EXTENSION])
            .set_file_name(format!("{}.{FILE_EXTENSION}", self.editor.display_name()))
            .save_file()
        else {
            return false;
        };
        self.save_to(&path)
    }

    pub(super) fn save_to(&mut self, path: &Path) -> bool {
        match self.editor.save_to(path) {
            Ok(()) => {
                // Saved, so there's nothing left to recover.
                if let Some(autosave) = &mut self.autosave {
                    autosave.clear();
                }
                true
            }
            Err(err) => {
                show_error(&tr_args("Couldn't save {file}", &[("file", &file_name(path))]), &err.to_string());
                false
            }
        }
    }

    /// If there are unsaved changes, asks whether to save them. Returns
    /// `true` if it's OK to continue (saved or discarded), `false` to stay put.
    /// Offers back work left behind by an Inkframe that didn't close
    /// properly, newest first.
    pub(super) fn recovery_window(&mut self, ctx: &egui::Context) {
        if self.recoveries.is_empty() {
            return;
        }
        let mut recover = None;
        let mut discard = Vec::new();
        egui::Window::new(tr("Recover Unsaved Work"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(tr(
                    "Inkframe didn't close properly last time. This work was autosaved but never saved:",
                ));
                ui.add_space(6.0);
                egui::Grid::new("recoveries").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                    for (i, found) in self.recoveries.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_min_width(320.0);
                            ui.strong(&found.name);
                            let place = found
                                .original
                                .as_ref()
                                .map_or_else(|| tr("Never saved").to_owned(), |p| p.display().to_string());
                            ui.add(egui::Label::new(egui::RichText::new(place).weak()).truncate());
                            ui.weak(tr_args("Autosaved {when}", &[("when", &found.when())]));
                        });
                        if ui.button(tr("Recover")).clicked() {
                            recover = Some(i);
                        }
                        if ui.button(tr("Discard")).on_hover_text(tr("Delete this autosaved copy")).clicked()
                        {
                            discard.push(i);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                if self.recoveries.len() > 1 && ui.button(tr("Discard All")).clicked() {
                    discard.extend(0..self.recoveries.len());
                }
            });
        if let Some(i) = recover
            && self.confirm_discard_changes()
        {
            match self.recoveries[i].open() {
                Ok(editor) => {
                    self.replace_editor(editor);
                    // It's this session's unsaved work now, autosaved as such.
                    discard.push(i);
                }
                Err(err) => show_error(
                    &tr_args("Couldn't recover {name}", &[("name", &self.recoveries[i].name)]),
                    &err,
                ),
            }
        }
        discard.sort_unstable();
        discard.dedup();
        for i in discard.into_iter().rev() {
            self.recoveries.remove(i).discard();
        }
    }

    pub(super) fn confirm_discard_changes(&mut self) -> bool {
        if !self.editor.has_unsaved_changes() {
            return true;
        }
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(tr("Unsaved changes"))
            .set_description(tr_args(
                "Save changes to “{name}” first?",
                &[("name", &self.editor.display_name())],
            ))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match answer {
            rfd::MessageDialogResult::Yes => self.save(),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    pub(super) fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        // A screenshot run quits without asking: nobody is there to
        // answer, and an unanswered dialog can take its default and save
        // the file that was opened.
        if self.screenshot.is_some() {
            self.allow_close = true;
            return;
        }
        let export_ok = !(self.export.is_running() || self.batch.is_running() || self.scripts.is_running())
            || {
                let answer = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title(tr("Still working"))
                .set_description(
                    tr("Quitting now will stop the export or script, which may leave an incomplete file. Quit anyway?"),
                )
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show();
                answer == rfd::MessageDialogResult::Ok
            };
        if export_ok && self.confirm_discard_changes() {
            self.allow_close = true;
            // A normal quit: whatever wasn't saved was let go on purpose.
            if let Some(autosave) = &mut self.autosave {
                autosave.clear();
            }
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }
}

/// Font files Inkframe can import.
pub(super) const FONT_EXTENSIONS: [&str; 3] = ["ttf", "otf", "ttc"];

/// Sound files Inkframe can import (see `inkframe-audio`).
pub(super) const AUDIO_EXTENSIONS: [&str; 9] =
    ["wav", "mp3", "m4a", "aac", "flac", "ogg", "aif", "aiff", "caf"];
