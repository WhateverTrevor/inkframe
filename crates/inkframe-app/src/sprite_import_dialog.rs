//! File › Import Sprite Sheet…: a sprite sheet (from Inkframe, Aseprite,
//! TexturePacker or a game's art) brought in as a paint layer to edit, a
//! drawing per frame (see `sprite_sheet.rs` in the core).
//!
//! Picking the data file, or a picture with one beside it of the same
//! name, takes the frames and their timing from the data, every sheet of a
//! multipack set included. A picture on its own is cut into a grid, set in
//! the window over a preview of the sheet.

use std::path::{Path, PathBuf};

use egui::{Color32, Rect, Stroke, TextureOptions, Vec2, pos2, vec2};
use inkframe_core::{
    FrameRate, SheetData, SheetImage, SpriteFrame, grid_frames, guess_grid, merge_sheets, read_sheet_data,
};

use crate::editor::Editor;
use crate::i18n::{tr, tr_args};

/// The preview's longest side, in points.
const PREVIEW: f32 = 360.0;

#[derive(Default)]
pub struct SpriteImportDialog {
    open: Option<Open>,
}

struct Open {
    name: String,
    sheets: Vec<SheetImage>,
    /// The data file's frames, if there was one.
    data: Option<SheetData>,
    /// For a picture on its own: its grid.
    columns: u32,
    rows: u32,
    skip_empty: bool,
    /// Every frame held this long, rather than the data's timing.
    hold: Option<u32>,
    /// Make the stage the frames' size, and the data's frame rate.
    fit_stage: bool,
    preview: Option<egui::TextureHandle>,
}

impl SpriteImportDialog {
    /// Opens the window for a sheet's picture or data file, or says why it
    /// can't be read.
    pub fn open(&mut self, path: &Path, editor: &Editor) -> Result<(), String> {
        let (data, pictures) = read_files(path)?;
        let sheets = pictures
            .iter()
            .map(|picture| {
                let (width, height, pixels) = inkframe_render::read_rgba(picture)?;
                Ok(SheetImage { width, height, pixels })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let first = sheets.first().ok_or_else(|| tr("The sprite sheet has no picture.").to_owned())?;
        let (columns, rows) = guess_grid(first.width, first.height);
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        // Names like `walk-0` belong to a multipack set; the set is `walk`.
        let name = match stem.rsplit_once('-') {
            Some((set, n)) if data.is_some() && n.chars().all(|c| c.is_ascii_digit()) => set.to_owned(),
            _ => stem,
        };
        self.open = Some(Open {
            name,
            sheets,
            data,
            columns,
            rows,
            skip_empty: true,
            hold: None,
            // Into an empty project, a sheet sets the stage, as opening it would.
            fit_stage: editor.doc.layers.is_empty(),
            preview: None,
        });
        Ok(())
    }

    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        let Some(open) = &mut self.open else { return };
        let mut keep = true;
        let mut import = false;
        egui::Window::new(tr("Import Sprite Sheet"))
            .open(&mut keep)
            .collapsible(false)
            .resizable(false)
            .default_width(PREVIEW + 20.0)
            .show(ctx, |ui| import = open.contents(ui, editor));
        if import {
            open.import(editor);
            keep = false;
        }
        if !keep {
            self.open = None;
        }
    }
}

/// The data file and the pictures for a picked file: a data file names
/// its picture (and its set's other files); a picture may have a data file
/// beside it of the same name.
fn read_files(path: &Path) -> Result<(Option<SheetData>, Vec<PathBuf>), String> {
    let is_json = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
    let json_path = if is_json {
        Some(path.to_path_buf())
    } else {
        Some(path.with_extension("json")).filter(|p| p.is_file())
    };
    let Some(json_path) = json_path else { return Ok((None, vec![path.to_path_buf()])) };
    let dir = json_path.parent().unwrap_or(Path::new("."));
    let read = |file: &Path, sheet: usize| -> Result<SheetData, String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        read_sheet_data(&text, sheet).map_err(|e| format!("{}: {e}", file.display()))
    };
    let first = read(&json_path, 0)?;
    // The set's files, this one first, each with its picture.
    let mut files = vec![json_path.clone()];
    files.extend(first.related.iter().map(|name| dir.join(name)));
    let mut sheets = Vec::new();
    let mut pictures = Vec::new();
    for (i, file) in files.iter().enumerate() {
        let data = if i == 0 { first.clone() } else { read(file, i)? };
        let picture =
            data.image.as_ref().map(|image| dir.join(image)).unwrap_or_else(|| file.with_extension("png"));
        pictures.push(picture);
        sheets.push(data);
    }
    // A set's frames are numbered through the whole animation; read the
    // sheets in the order their frames play, whichever was picked.
    Ok((Some(merge_sheets(sheets)), pictures))
}

impl Open {
    /// The frames to import, in order.
    fn frames(&self) -> Vec<SpriteFrame> {
        match &self.data {
            Some(data) => data.frames.clone(),
            None => {
                let sheet = &self.sheets[0];
                let mut frames = grid_frames(sheet.width, sheet.height, self.columns, self.rows);
                if self.skip_empty {
                    frames.retain(|frame| !sheet.is_empty(frame));
                }
                frames
            }
        }
    }

    /// The window's contents. Returns true when Import is pressed.
    fn contents(&mut self, ui: &mut egui::Ui, editor: &Editor) -> bool {
        self.show_preview(ui);
        let frames = self.frames();
        ui.add_space(6.0);
        egui::Grid::new("sprite import").num_columns(2).show(ui, |ui| {
            ui.label(tr("Layer name"));
            ui.text_edit_singleline(&mut self.name);
            ui.end_row();
            if self.data.is_none() {
                ui.label(tr("Grid"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.columns).range(1..=256).suffix(tr(" across")));
                    ui.add(egui::DragValue::new(&mut self.rows).range(1..=256).suffix(tr(" down")));
                });
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.skip_empty, tr("Leave out empty cells"));
                ui.end_row();
            }
            ui.label(tr("Timing"));
            ui.horizontal(|ui| {
                let timed =
                    self.data.as_ref().is_some_and(|d| d.frames.iter().any(|f| f.duration_ms.is_some()));
                let mut fixed = self.hold.is_some() || !timed;
                if timed {
                    ui.radio_value(&mut fixed, false, tr("As the sheet says"));
                }
                ui.radio_value(&mut fixed, true, tr("Each frame for"));
                if fixed {
                    let mut hold = self.hold.unwrap_or(1);
                    ui.add(egui::DragValue::new(&mut hold).range(1..=24).suffix(tr(" frames")));
                    self.hold = Some(hold);
                } else {
                    self.hold = None;
                }
            });
            ui.end_row();
            ui.label("");
            let size = frames.first().map_or((0, 0), |f| f.source);
            ui.checkbox(
                &mut self.fit_stage,
                tr_args(
                    "Make the stage {w} × {h}",
                    &[("w", &size.0.to_string()), ("h", &size.1.to_string())],
                ),
            )
            .on_hover_text(tr("The stage the size of a frame, and the sheet's frame rate if it gives one"));
            ui.end_row();
        });
        let lasts = holds(&frames, self.hold, self.fps(editor)).iter().sum::<u32>();
        ui.weak(tr_args(
            "{frames} frames, lasting {length} frames on the timeline",
            &[("frames", &frames.len().to_string()), ("length", &lasts.to_string())],
        ));
        ui.add_space(6.0);
        ui.horizontal(|ui| ui.add_enabled(!frames.is_empty(), egui::Button::new(tr("Import"))).clicked())
            .inner
    }

    /// The first sheet, with the frames outlined on it.
    fn show_preview(&mut self, ui: &mut egui::Ui) {
        let sheet = &self.sheets[0];
        let texture = self.preview.get_or_insert_with(|| {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [sheet.width as usize, sheet.height as usize],
                &sheet.pixels,
            );
            ui.ctx().load_texture("sprite sheet preview", image, TextureOptions::NEAREST)
        });
        let scale = (PREVIEW / sheet.width.max(sheet.height) as f32).min(4.0);
        let size = vec2(sheet.width as f32, sheet.height as f32) * scale;
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(60));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
        let outline = Stroke::new(1.0, crate::theme::ACCENT);
        for frame in self.frames().iter().filter(|f| f.sheet == 0) {
            let (w, h) =
                if frame.rotated { (frame.height, frame.width) } else { (frame.width, frame.height) };
            let min = rect.min + vec2(frame.x as f32, frame.y as f32) * scale;
            ui.painter().rect_stroke(
                Rect::from_min_size(min, Vec2::new(w as f32, h as f32) * scale),
                0.0,
                outline,
                egui::StrokeKind::Inside,
            );
        }
    }

    /// The frame rate the frames will play at: the sheet's, if the stage
    /// is to take it, otherwise the project's.
    fn fps(&self, editor: &Editor) -> f64 {
        let sheet = self.data.as_ref().and_then(|d| d.frame_rate).filter(|_| self.fit_stage);
        sheet.unwrap_or_else(|| editor.doc.frame_rate.fps())
    }

    fn import(&self, editor: &mut Editor) {
        let frames = self.frames();
        let Some(first) = frames.first() else { return };
        let fps = self.fps(editor);
        if self.fit_stage {
            (editor.doc.width, editor.doc.height) = first.source;
            if let Some(rate) = self.data.as_ref().and_then(|d| d.frame_rate) {
                editor.doc.frame_rate = FrameRate::new(rate.round().max(1.0) as u32, 1);
            }
        }
        let holds = holds(&frames, self.hold, fps);
        let (timeline, frame) = (editor.timeline(), editor.frame);
        if let Some(id) =
            editor.doc.add_sprite_layer(timeline, &self.name, frame, &self.sheets, &frames, &holds)
        {
            editor.select(Some(id));
        }
    }
}

/// How many timeline frames each frame shows for: `hold` each, or as long
/// as the data says at `fps` (a frame at least).
fn holds(frames: &[SpriteFrame], hold: Option<u32>, fps: f64) -> Vec<u32> {
    frames
        .iter()
        .map(|frame| match (hold, frame.duration_ms) {
            (Some(hold), _) => hold,
            (None, Some(ms)) => ((ms as f64 / 1000.0 * fps).round() as u32).max(1),
            (None, None) => 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_last_as_the_sheet_says_at_the_projects_rate() {
        let frame = |ms| SpriteFrame {
            sheet: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            rotated: false,
            offset: (0, 0),
            source: (1, 1),
            duration_ms: ms,
        };
        let frames = [frame(Some(125.0)), frame(Some(250.0)), frame(Some(10.0)), frame(None)];
        assert_eq!(holds(&frames, None, 24.0), [3, 6, 1, 1]);
        assert_eq!(holds(&frames, Some(2), 24.0), [2, 2, 2, 2]);
    }
}
