//! The Motion Presets panel, as Animate's: movements to give layers. The
//! built-in ones (a fade, a pop, a bounce) and your own, saved from a
//! layer's animation as files in the Motion Presets folder beside
//! Inkframe's settings, so every project has them. Each shows a little
//! square doing its movement. See `preset.rs` in the core for how a preset
//! is kept relative to where a layer starts.

use std::path::PathBuf;

use egui::{Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::{Document, Layer, LayerId, MotionPreset, PRESET_EXTENSION, Point, Property, Transform};

use crate::editor::Editor;
use crate::icons::{Icon, IconButton};

const PREVIEW: Vec2 = vec2(64.0, 40.0);

pub struct PresetLibrary {
    /// Where your presets live; the settings folder's unless a test or a
    /// screenshot says.
    folder: Option<PathBuf>,
    /// Yours, read when the panel first shows and after a save or delete.
    saved: Option<Vec<(PathBuf, MotionPreset)>>,
    built_in: Vec<MotionPreset>,
    /// The one picked: built in, or yours, by name.
    chosen: Option<(bool, String)>,
    /// The name for the next preset saved.
    name: String,
    problem: Option<String>,
    /// A layer each preset has been applied to, to show it moving.
    previews: std::collections::HashMap<String, Layer>,
}

impl Default for PresetLibrary {
    fn default() -> Self {
        Self {
            folder: None,
            saved: None,
            built_in: MotionPreset::built_in(),
            chosen: None,
            name: "My Movement".to_owned(),
            problem: None,
            previews: Default::default(),
        }
    }
}

impl PresetLibrary {
    /// Keeps presets in `folder` rather than the settings folder's.
    pub fn use_folder(&mut self, folder: PathBuf) {
        self.folder = Some(folder);
        self.saved = None;
    }

    fn folder(&self) -> Option<PathBuf> {
        self.folder.clone().or_else(|| crate::layout::settings_dir().map(|dir| dir.join("Motion Presets")))
    }

    fn scan(&mut self) {
        let mut saved: Vec<(PathBuf, MotionPreset)> = self
            .folder()
            .and_then(|folder| std::fs::read_dir(folder).ok())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|e| e == PRESET_EXTENSION))
            .filter_map(|path| {
                let preset = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
                Some((path, preset))
            })
            .collect();
        saved.sort_by_key(|(_, preset)| preset.name.to_lowercase());
        self.saved = Some(saved);
    }

    /// Saves `preset` as a file of its own, named after it.
    pub fn save(&mut self, preset: &MotionPreset) -> Result<(), String> {
        let folder = self.folder().ok_or("There's no settings folder to save presets in.")?;
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let stem: String = preset.name.chars().filter(|c| !"/\\:*?\"<>|".contains(*c)).collect();
        let mut path = folder.join(format!("{stem}.{PRESET_EXTENSION}"));
        let mut n = 2;
        while path.exists() {
            path = folder.join(format!("{stem} {n}.{PRESET_EXTENSION}"));
            n += 1;
        }
        let json = serde_json::to_vec_pretty(preset).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        self.saved = None;
        self.chosen = Some((false, preset.name.clone()));
        Ok(())
    }

    /// For screenshots: the selected layer's movement saved (replacing what
    /// was saved before), and Bounce In picked.
    pub fn screenshot_setup(&mut self, editor: &Editor) {
        if let Some(folder) = self.folder() {
            let _ = std::fs::remove_dir_all(folder);
        }
        if let Some(layer) = editor.selected_layer()
            && let Some((first, last)) = save_range(editor, layer.id)
            && let Some(preset) =
                MotionPreset::from_layer(format!("{} movement", layer.name), layer, first, last)
        {
            let _ = self.save(&preset);
        }
        self.chosen = Some((true, "Bounce In".to_owned()));
    }

    fn chosen_preset(&self) -> Option<&MotionPreset> {
        let (built_in, name) = self.chosen.as_ref()?;
        if *built_in {
            self.built_in.iter().find(|p| &p.name == name)
        } else {
            self.saved.as_ref()?.iter().map(|(_, p)| p).find(|p| &p.name == name)
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        if self.saved.is_none() {
            self.scan();
        }
        let targets = targets(editor);

        // Apply, at the top where it's easy to reach.
        ui.horizontal(|ui| {
            let chosen = self.chosen_preset().cloned();
            let can = chosen.is_some() && !targets.is_empty();
            let plural = if targets.len() == 1 { "" } else { "s" };
            let label = if targets.is_empty() {
                "Apply".to_owned()
            } else {
                format!("Apply to {} layer{plural}", targets.len())
            };
            if ui
                .add_enabled(can, egui::Button::new(label))
                .on_hover_text("From the playhead, starting from where each layer is now")
                .on_disabled_hover_text("Pick a preset below and select the layers to move")
                .clicked()
                && let Some(preset) = chosen
            {
                apply(editor, &preset, &targets);
            }
        });
        ui.add_space(4.0);

        // The time drives the previews, a frame at the document's rate.
        let fps = editor.doc.frame_rate.fps();
        let time = ui.input(|i| i.time);
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(1.0 / fps));

        ui.label(crate::inspector::section_text("Built in"));
        let built_in = self.built_in.clone();
        for preset in &built_in {
            self.row(ui, preset, true, time * fps, false);
        }
        ui.add_space(6.0);
        ui.label(crate::inspector::section_text("Yours"));
        let saved = self.saved.clone().unwrap_or_default();
        if saved.is_empty() {
            ui.weak("None yet. Animate a layer, then save its movement here.");
        }
        let mut delete = None;
        for (path, preset) in &saved {
            if self.row(ui, preset, false, time * fps, true) {
                delete = Some(path.clone());
            }
        }
        if let Some(path) = delete {
            let _ = std::fs::remove_file(path);
            self.saved = None;
        }

        // Saving the selected layer's movement.
        ui.add_space(6.0);
        ui.separator();
        let source = editor.selected_layer().map(|l| l.id);
        let range = source.and_then(|id| save_range(editor, id));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(140.0));
            let tip = match range {
                Some((first, last)) => {
                    format!("The selected layer's movement, frames {} to {}", first + 1, last + 1)
                }
                None => "Select a layer that moves (or pick its frames in the timeline)".to_owned(),
            };
            if ui
                .add_enabled(range.is_some(), egui::Button::new("Save Movement"))
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
                && let (Some(id), Some((first, last))) = (source, range)
                && let Some(layer) = editor.doc.layer(id)
            {
                self.problem = match MotionPreset::from_layer(self.name.trim(), layer, first, last) {
                    Some(preset) => self.save(&preset).err(),
                    None => Some("That layer doesn't move over those frames.".to_owned()),
                };
            }
        });
        if let Some(problem) = &self.problem {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
    }

    /// One preset: its preview and name, picked by a click. With
    /// `deletable`, a delete button; returns whether it was pressed.
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        preset: &MotionPreset,
        built_in: bool,
        frames: f64,
        deletable: bool,
    ) -> bool {
        let chosen = self.chosen.as_ref().is_some_and(|(b, n)| *b == built_in && *n == preset.name);
        let mut deleted = false;
        let response = ui
            .horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(PREVIEW, Sense::hover());
                let layer = self
                    .previews
                    .entry(format!("{built_in}{}", preset.name))
                    .or_insert_with(|| preview_layer(preset));
                // Plays through, then holds a moment before going again.
                let cycle = preset.length as f64 + 12.0;
                let frame = (frames % cycle).min(preset.length as f64) as f32;
                paint_preview(ui, rect, layer, preset, frame);
                let text = egui::RichText::new(&preset.name);
                let label = ui.selectable_label(chosen, if chosen { text.strong() } else { text });
                ui.weak(format!("{} frames", preset.length));
                if deletable && ui.add(IconButton::new(Icon::Trash, "Delete this preset")).clicked() {
                    deleted = true;
                }
                label
            })
            .inner;
        if response.clicked() {
            self.chosen = Some((built_in, preset.name.clone()));
        }
        deleted
    }
}

/// The selected layers a preset can move.
fn targets(editor: &Editor) -> Vec<LayerId> {
    editor
        .selected_ids()
        .into_iter()
        .filter(|&id| editor.doc.layer(id).is_some_and(|l| !l.folder && !l.locked))
        .collect()
}

/// Gives each layer the preset's movement from the playhead.
pub fn apply(editor: &mut Editor, preset: &MotionPreset, layers: &[LayerId]) {
    let frame = editor.frame;
    for &id in layers {
        if let Some(layer) = editor.doc.layer_mut(id) {
            preset.apply(layer, frame);
        }
    }
}

/// The frames to save a layer's movement from: those picked in the
/// timeline, or from its first key to its last.
fn save_range(editor: &Editor, id: LayerId) -> Option<(u32, u32)> {
    if editor.frame_range.is_some() {
        return Some(editor.frame_span());
    }
    let layer = editor.doc.layer(id)?;
    let frames: Vec<u32> =
        [Property::Position, Property::Rotation, Property::Scale, Property::Skew, Property::Opacity]
            .into_iter()
            .flat_map(|p| layer.track.channel(p).key_frames())
            .collect();
    let (first, last) = (*frames.iter().min()?, *frames.iter().max()?);
    (last > first).then_some((first, last))
}

/// A layer at the origin with the preset applied from frame 0, to play.
fn preview_layer(preset: &MotionPreset) -> Layer {
    let mut doc = Document::default();
    let id = doc.add_layer("Preview", Vec::new(), 0, Transform::at(Point::ZERO));
    let mut layer = doc.layer(id).expect("just added").clone();
    preset.apply(&mut layer, 0);
    layer
}

/// A small rounded square moving as the preset moves it, its travel
/// fitted to the preview box.
fn paint_preview(ui: &egui::Ui, rect: Rect, layer: &Layer, preset: &MotionPreset, frame: f32) {
    let visuals = ui.visuals();
    ui.painter().rect_filled(rect, 4.0, visuals.extreme_bg_color);
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    let track = &layer.track;
    let position = track.position.sample(frame).unwrap_or(Point::ZERO);
    let rotation = track.rotation.sample(frame).unwrap_or(0.0).to_radians();
    let scale = track.scale.sample(frame).unwrap_or(Point::new(1.0, 1.0));
    let opacity = track.opacity.sample(frame).unwrap_or(1.0);
    // However far it travels, it stays inside the box.
    let reach = preset.position.iter().fold(1.0f32, |m, k| m.max(k.value.x.abs()).max(k.value.y.abs()));
    let fit = ((rect.width() / 2.0 - 10.0) / reach).min(1.0);
    let centre = rect.center() + vec2(position.x * fit, position.y * fit);
    let half = 7.0;
    let (sin, cos) = rotation.sin_cos();
    let corner = |x: f32, y: f32| -> Pos2 {
        let (x, y) = (x * scale.x, y * scale.y);
        pos2(centre.x + x * cos - y * sin, centre.y + x * sin + y * cos)
    };
    let points = vec![corner(-half, -half), corner(half, -half), corner(half, half), corner(-half, half)];
    let color = visuals.selection.stroke.color.gamma_multiply(opacity.clamp(0.0, 1.0));
    let clip = ui.painter().with_clip_rect(rect.shrink(1.0));
    clip.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::Transform;

    #[test]
    fn a_saved_movement_is_read_back_and_applied_to_the_selection() {
        let folder = std::env::temp_dir().join(format!("inkframe-presets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut library = PresetLibrary::default();
        library.use_folder(folder.clone());

        // A layer that rises 50 over frames 0 to 10.
        let mut editor = Editor::new(Document::default());
        let source = editor.doc.add_layer("Rise", Vec::new(), 0, Transform::at(Point::new(0.0, 100.0)));
        editor.doc.layer_mut(source).unwrap().track.position.set(10, Point::new(0.0, 50.0));
        editor.select(Some(source));
        assert_eq!(save_range(&editor, source), Some((0, 10)));
        let preset = MotionPreset::from_layer("Rise", editor.doc.layer(source).unwrap(), 0, 10).unwrap();
        library.save(&preset).unwrap();
        library.scan();
        assert_eq!(library.saved.as_ref().unwrap().len(), 1);
        assert_eq!(library.chosen_preset().unwrap().name, "Rise");

        // Two other layers, given it from frame 20.
        let a = editor.doc.add_layer("A", Vec::new(), 0, Transform::at(Point::new(300.0, 300.0)));
        let b = editor.doc.add_layer("B", Vec::new(), 0, Transform::at(Point::new(600.0, 200.0)));
        editor.select_many(&[a, b]);
        editor.set_frame(20);
        let chosen = library.chosen_preset().unwrap().clone();
        let layers = targets(&editor);
        apply(&mut editor, &chosen, &layers);
        let at = |id, f: f32| editor.doc.layer(id).unwrap().track.position.sample(f).unwrap();
        assert_eq!(at(a, 30.0), Point::new(300.0, 250.0));
        assert_eq!(at(b, 30.0), Point::new(600.0, 150.0));
        let _ = std::fs::remove_dir_all(folder);
    }
}
