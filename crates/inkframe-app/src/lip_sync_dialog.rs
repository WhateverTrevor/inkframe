//! The Lip Sync window: fills a mouth layer's drawing swaps from a line of
//! dialogue (see `inkframe_core::lip_sync`).
//!
//! It works on the layer selected when the window opened, so clicking
//! around the stage to check the result doesn't change its target. The
//! mouth shapes come from the dialogue's loudness, or from a Rhubarb or
//! Papagayo timing file for real sounds.

use std::path::{Path, PathBuf};

use inkframe_core::{
    AudioId, LayerId, MouthCue, MouthMap, MouthShape, cues_from_loudness, parse_papagayo, parse_rhubarb,
};

use crate::audio_engine::AudioEngine;
use crate::editor::Editor;

/// Timing files the window can read: Rhubarb's tab-separated output, and
/// Papagayo/Moho switch files.
const TIMING_EXTENSIONS: [&str; 3] = ["tsv", "txt", "dat"];

/// Where the mouth shapes come from.
enum Timing {
    Loudness,
    /// A timing file, read when chosen.
    File {
        path: PathBuf,
        cues: Vec<MouthCue>,
    },
}

#[derive(Default)]
pub struct LipSyncDialog {
    pub open: bool,
    layer: Option<LayerId>,
    audio: Option<AudioId>,
    timing: Option<Timing>,
    map: MouthMap,
    /// The result of the last Apply, or why it couldn't.
    message: Option<String>,
}

impl LipSyncDialog {
    /// Presses Apply, unless it has been pressed already. For the
    /// screenshot hook.
    pub fn apply_once(&mut self, editor: &mut Editor, audio: &mut AudioEngine) {
        if self.message.is_none() {
            self.message = Some(match self.apply(editor, audio) {
                Ok(summary) | Err(summary) => summary,
            });
        }
    }
}

impl LipSyncDialog {
    /// Opens the window for the selected layer, guessing which of its
    /// drawings is which mouth shape and which sound is its dialogue.
    pub fn open(&mut self, editor: &Editor) {
        let layer = editor.selected_layer();
        self.layer = layer.map(|l| l.id);
        self.map = layer.map(MouthMap::guess).unwrap_or_default();
        if self.audio.is_none_or(|id| editor.doc.audio.iter().all(|a| a.id != id)) {
            self.audio = editor.doc.audio.first().map(|a| a.id);
        }
        self.timing.get_or_insert(Timing::Loudness);
        self.message = None;
        self.open = true;
    }

    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor, audio: &mut AudioEngine) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Lip Sync")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| self.contents(ui, editor, audio));
        self.open = open;
    }

    fn contents(&mut self, ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) {
        let Some(layer) = self.layer.and_then(|id| editor.doc.layer(id)) else {
            ui.label("Select the mouth layer first, then choose Lip Sync again.");
            return;
        };
        if layer.drawings.len() < 2 {
            ui.label(format!(
                "\"{}\" has only one drawing. Add a drawing for each mouth shape first \
                 (Drawings, in the properties panel), named A–H and X, or MBP, AI, O and so on.",
                layer.name
            ));
            return;
        }
        if editor.doc.audio.is_empty() {
            ui.label("Import the dialogue first (File › Import Audio), then choose Lip Sync again.");
            return;
        }
        let drawing_names: Vec<String> = layer.drawings.iter().map(|d| d.name.clone()).collect();
        let layer_name = layer.name.clone();

        egui::Grid::new("lip_sync").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Mouth");
            ui.strong(&layer_name);
            ui.end_row();

            ui.label("Dialogue");
            let current = editor.doc.audio.iter().find(|a| Some(a.id) == self.audio).map_or("", |a| &a.name);
            egui::ComboBox::from_id_salt("lip_sync_audio").selected_text(current).width(240.0).show_ui(
                ui,
                |ui| {
                    for track in &editor.doc.audio {
                        ui.selectable_value(&mut self.audio, Some(track.id), &track.name);
                    }
                },
            );
            ui.end_row();

            ui.label("Shapes from");
            ui.vertical(|ui| self.timing_ui(ui, editor.doc.frame_rate.fps()));
            ui.end_row();
        });

        ui.add_space(6.0);
        ui.strong("Drawing for each shape");
        ui.weak("Guessed from the drawings' names. A shape without one borrows a similar shape's.");
        egui::Grid::new("lip_sync_map").num_columns(3).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
            for shape in MouthShape::ALL {
                ui.strong(shape.letter());
                ui.label(shape.description());
                let mut choice = self.map.get(shape);
                let text = match choice {
                    Some(index) => drawing_names[index].clone(),
                    None => format!("({})", drawing_names[self.map.drawing_for(shape)]),
                };
                egui::ComboBox::from_id_salt(("mouth", shape.letter()))
                    .selected_text(text)
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut choice, None, "Borrow");
                        for (index, name) in drawing_names.iter().enumerate() {
                            ui.selectable_value(&mut choice, Some(index), name);
                        }
                    });
                self.map.set(shape, choice);
                ui.end_row();
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                self.message = Some(match self.apply(editor, audio) {
                    Ok(summary) | Err(summary) => summary,
                });
            }
            if let Some(message) = &self.message {
                ui.label(message);
            }
        });
    }

    fn timing_ui(&mut self, ui: &mut egui::Ui, fps: f64) {
        let from_file = matches!(self.timing, Some(Timing::File { .. }));
        if ui.radio(!from_file, "Loudness: opens wider as the voice gets louder").clicked() {
            self.timing = Some(Timing::Loudness);
        }
        ui.horizontal(|ui| {
            let label = match &self.timing {
                Some(Timing::File { path, .. }) => format!("Timing file: {}", file_name(path)),
                _ => "Timing file from Rhubarb or Papagayo…".to_owned(),
            };
            if ui.radio(from_file, label).clicked() || ui.small_button("Choose…").clicked() {
                self.choose_file(fps);
            }
        });
    }

    fn choose_file(&mut self, fps: f64) {
        let Some(path) = rfd::FileDialog::new().add_filter("Lip sync timing", &TIMING_EXTENSIONS).pick_file()
        else {
            return;
        };
        match read_timing(&path, fps) {
            Ok(cues) => {
                self.message = None;
                self.timing = Some(Timing::File { path, cues });
            }
            Err(err) => self.message = Some(format!("Couldn't read {}: {err}", file_name(&path))),
        }
    }

    /// Writes the mouth shapes into the layer's drawing swaps, across the
    /// dialogue's length. Returns a summary either way.
    fn apply(&self, editor: &mut Editor, audio: &mut AudioEngine) -> Result<String, String> {
        let track =
            editor.doc.audio.iter().find(|a| Some(a.id) == self.audio).ok_or("Choose the dialogue.")?;
        let fps = editor.doc.frame_rate.fps();
        let peaks = audio.peaks(track, editor.project_dir().as_deref(), fps);
        let cues = match (&self.timing, &peaks) {
            (Some(Timing::File { cues, .. }), _) => cues.clone(),
            (_, Some(peaks)) => cues_from_loudness(peaks),
            (_, None) => return Err("The dialogue is still loading; try again in a moment.".into()),
        };
        // Across the whole sound, or, if it hasn't loaded, as far as the
        // timing file goes.
        let length = peaks.map_or_else(|| cues.last().map_or(0, |c| c.frame + 1), |p| p.len() as u32);
        let start = track.start;
        let end = start + length.max(1) - 1;
        let layer = self.layer.and_then(|id| editor.doc.layer_mut(id)).ok_or("The mouth layer is gone.")?;
        layer.apply_lip_sync(&cues, &self.map, start, end);
        Ok(format!("{} mouth changes, frames {}–{}.", cues.len(), start + 1, end + 1))
    }
}

fn read_timing(path: &Path, fps: f64) -> Result<Vec<MouthCue>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    // Papagayo's files say so on their first line; anything else is taken
    // to be Rhubarb's.
    let cues = if text.trim_start().starts_with("MohoSwitch") {
        parse_papagayo(&text)
    } else {
        parse_rhubarb(&text, fps)
    };
    cues.map_err(|e| e.to_string())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Document, Drawing, Point, Transform};

    #[test]
    fn applying_a_timing_file_swaps_the_mouth_under_the_dialogue() {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("Mouth", vec![], 0, Transform::at(Point::ZERO));
        let layer = editor.doc.layer_mut(id).unwrap();
        layer.drawings = ["X", "A", "D"].map(|n| Drawing::new(n, Vec::new())).to_vec();
        editor.selection = Some(id);
        editor.doc.add_audio(Path::new("/nowhere/line.wav"), None, 10);

        let mut dialog = LipSyncDialog::default();
        dialog.open(&editor);
        let cues = parse_rhubarb("0.0 D\n0.25 A\n0.5 X\n", 24.0).unwrap();
        dialog.timing = Some(Timing::File { path: PathBuf::from("line.tsv"), cues });
        // The sound never loads here, so the file's own length is used.
        let summary = dialog.apply(&mut editor, &mut AudioEngine::without_output()).unwrap();

        let layer = editor.doc.layer(id).unwrap();
        assert_eq!([9, 10, 15, 16, 22].map(|f| layer.drawing_index_at(f)), [0, 2, 2, 1, 0]);
        assert_eq!(summary, "3 mouth changes, frames 11–23.");
    }
}
