//! The exposure sheet: drawing swaps laid out frame by frame, the way
//! traditional animators time them on paper (and Harmony and OpenToonz on
//! screen).
//!
//! Time runs down the page, one row per frame. Each layer that swaps
//! drawings gets a column showing which drawing is exposed: its name where
//! it starts, then a line down for as long as it's held. Sounds get a
//! column too, with their loudness, so mouths can be timed to dialogue.
//!
//! Typing a drawing's name into a cell exposes it from that frame and
//! moves down a row, so a run of timing can be keyed in quickly. Typing a
//! name the layer hasn't got makes a new, blank drawing by that name, as
//! in Harmony.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use inkframe_core::{Drawing, Layer, LayerId};

use crate::audio_engine::AudioEngine;
use crate::editor::Editor;

const ROW: f32 = 18.0;
const FRAME_COLUMN: f32 = 44.0;
const COLUMN: f32 = 76.0;
const SOUND_COLUMN: f32 = 56.0;

/// What one cell of a layer's column shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// The layer isn't on stage on this frame.
    Outside,
    /// The named drawing is exposed from this frame.
    Starts(String),
    /// The drawing above is still exposed.
    Holds,
}

/// A layer's column for frames `0..frames`.
pub fn column(layer: &Layer, frames: u32, last_frame: u32) -> Vec<Cell> {
    let mut shown_before = None;
    (0..frames)
        .map(|frame| {
            if frame < layer.start || frame > layer.last_frame(last_frame) {
                shown_before = None;
                return Cell::Outside;
            }
            let drawing = layer.drawing_index_at(frame);
            if shown_before == Some(drawing) {
                return Cell::Holds;
            }
            shown_before = Some(drawing);
            Cell::Starts(layer.drawings[drawing].name.clone())
        })
        .collect()
}

/// Exposes the drawing called `name` on `layer` from `frame`, making a
/// blank one by that name if there isn't one. An empty name does nothing.
pub fn enter(editor: &mut Editor, layer: LayerId, frame: u32, name: &str) {
    let name = name.trim();
    let Some(layer) = editor.doc.layer_mut(layer).filter(|_| !name.is_empty()) else { return };
    let drawing = match layer.drawing_named(name) {
        Some(index) => index,
        None => layer.add_drawing(Drawing::new(name, Vec::new())),
    };
    layer.expose(frame, drawing);
}

/// The cell being typed into.
struct Typing {
    layer: LayerId,
    frame: u32,
    text: String,
    /// Whether the field has been given the keyboard yet; it's only asked
    /// for once, so clicking elsewhere can take it away.
    focused: bool,
    /// Set when the field loses the keyboard, which ends typing.
    left: bool,
}

impl Typing {
    fn new(layer: LayerId, frame: u32, text: String) -> Self {
        Self { layer, frame, text, focused: false, left: false }
    }
}

#[derive(Default)]
pub struct ExposureSheet {
    pub open: bool,
    typing: Option<Typing>,
    /// The frame shown last time, to scroll along when it changes.
    last_frame_shown: Option<u32>,
}

impl ExposureSheet {
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor, audio: &mut AudioEngine) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Exposure Sheet")
            .open(&mut open)
            .default_size([380.0, 520.0])
            .resizable(true)
            .show(ctx, |ui| self.contents(ui, editor, audio));
        self.open = open;
    }

    fn contents(&mut self, ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) {
        let layers: Vec<LayerId> =
            editor.layers().iter().rev().filter(|l| l.drawings.len() > 1).map(|l| l.id).collect();
        let sounds = if editor.timeline() == inkframe_core::TimelineId::Main {
            editor.doc.audio.clone()
        } else {
            Vec::new()
        };
        if layers.is_empty() {
            ui.label(
                "No layers swap drawings yet. Give a layer more drawings (Drawings, in the properties \
                 panel), and its column appears here.",
            );
            return;
        }
        let frames = editor.duration();
        let last_frame = editor.last_frame();
        let columns: Vec<Vec<Cell>> = layers
            .iter()
            .filter_map(|&id| editor.doc.layer(id))
            .map(|l| column(l, frames, last_frame))
            .collect();
        let fps = editor.doc.frame_rate.fps();
        let project_dir = editor.project_dir();
        let peaks: Vec<Option<std::sync::Arc<Vec<f32>>>> =
            sounds.iter().map(|s| audio.peaks(s, project_dir.as_deref(), fps)).collect();

        let visuals = ui.visuals().clone();
        let text = visuals.text_color();
        let faint = text.gamma_multiply(0.35);
        let font = FontId::proportional(12.0);
        let width = FRAME_COLUMN + COLUMN * layers.len() as f32 + SOUND_COLUMN * sounds.len() as f32;

        // Column names, above the scrolling rows.
        let (header, _) = ui.allocate_exact_size(vec2(width, ROW + 4.0), Sense::hover());
        let painter = ui.painter_at(header);
        painter.text(
            pos2(header.left() + 4.0, header.center().y),
            Align2::LEFT_CENTER,
            "Frame",
            font.clone(),
            faint,
        );
        let mut x = header.left() + FRAME_COLUMN;
        for &id in &layers {
            let name = editor.doc.layer(id).map_or("", |l| l.name.as_str());
            let color = if editor.is_selected(id) { visuals.selection.stroke.color } else { text };
            let cell = Rect::from_min_size(pos2(x, header.top()), vec2(COLUMN, header.height()));
            painter.with_clip_rect(cell).text(
                cell.left_center() + vec2(4.0, 0.0),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                color,
            );
            x += COLUMN;
        }
        for sound in &sounds {
            let cell = Rect::from_min_size(pos2(x, header.top()), vec2(SOUND_COLUMN, header.height()));
            painter.with_clip_rect(cell).text(
                cell.left_center() + vec2(4.0, 0.0),
                Align2::LEFT_CENTER,
                format!("♪ {}", sound.name),
                font.clone(),
                faint,
            );
            x += SOUND_COLUMN;
        }

        // Keep the playhead's row in view as it moves.
        let scroll_to = (self.last_frame_shown != Some(editor.frame)).then_some(editor.frame);
        self.last_frame_shown = Some(editor.frame);

        let mut clicked = None;
        let mut menu_choice = None;
        egui::ScrollArea::both().auto_shrink(false).show_rows(ui, ROW, frames as usize, |ui, rows| {
            for frame in rows.start as u32..rows.end as u32 {
                let (row, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
                if scroll_to == Some(frame) {
                    ui.scroll_to_rect(row, None);
                }
                let painter = ui.painter_at(row);
                if frame == editor.frame {
                    painter.rect_filled(row, 0.0, visuals.selection.bg_fill.gamma_multiply(0.5));
                } else if (frame + 1) % 8 == 0 {
                    // A rule every 8 frames (a third of a second at 24 fps)
                    // helps count, as on printed sheets.
                    painter.hline(row.x_range(), row.bottom(), Stroke::new(1.0, faint.gamma_multiply(0.5)));
                }
                painter.text(
                    pos2(row.left() + 4.0, row.center().y),
                    Align2::LEFT_CENTER,
                    frame + 1,
                    font.clone(),
                    faint,
                );

                let mut x = row.left() + FRAME_COLUMN;
                for (&id, cells) in layers.iter().zip(&columns) {
                    let cell = Rect::from_min_size(pos2(x, row.top()), vec2(COLUMN, ROW));
                    x += COLUMN;
                    painter.vline(cell.left(), row.y_range(), Stroke::new(1.0, faint.gamma_multiply(0.5)));
                    if let Some(typing) = self.typing.as_mut().filter(|t| t.layer == id && t.frame == frame) {
                        let field = ui.put(
                            cell.shrink(1.0),
                            egui::TextEdit::singleline(&mut typing.text).font(font.clone()),
                        );
                        if !typing.focused {
                            field.request_focus();
                            typing.focused = true;
                        } else if field.lost_focus() {
                            typing.left = true;
                        }
                        continue;
                    }
                    let response = ui.interact(cell, ui.id().with(("cell", id, frame)), Sense::click());
                    match &cells[frame as usize] {
                        Cell::Outside => {
                            painter.rect_filled(cell.shrink(1.0), 0.0, faint.gamma_multiply(0.15));
                        }
                        Cell::Starts(name) => {
                            painter.with_clip_rect(cell).text(
                                cell.left_center() + vec2(6.0, 0.0),
                                Align2::LEFT_CENTER,
                                name,
                                font.clone(),
                                text,
                            );
                        }
                        Cell::Holds => {
                            painter.vline(
                                cell.left() + 10.0,
                                row.y_range(),
                                Stroke::new(1.5, text.gamma_multiply(0.6)),
                            );
                        }
                    }
                    if response.clicked() {
                        clicked = Some((id, frame, false));
                    }
                    if response.double_clicked() {
                        clicked = Some((id, frame, true));
                    }
                    response.context_menu(|ui| {
                        let Some(layer) = editor.doc.layer(id) else { return };
                        for (index, drawing) in layer.drawings.iter().enumerate() {
                            if ui.button(&drawing.name).clicked() {
                                menu_choice = Some((id, frame, Some(index)));
                            }
                        }
                        ui.separator();
                        let has_swap = layer.exposure.iter().any(|k| k.frame == frame);
                        if ui.add_enabled(has_swap, egui::Button::new("Hold the drawing above")).clicked() {
                            menu_choice = Some((id, frame, None));
                        }
                    });
                }
                for (sound, peaks) in sounds.iter().zip(&peaks) {
                    let cell = Rect::from_min_size(pos2(x, row.top()), vec2(SOUND_COLUMN, ROW));
                    x += SOUND_COLUMN;
                    painter.vline(cell.left(), row.y_range(), Stroke::new(1.0, faint.gamma_multiply(0.5)));
                    // How loud the sound is on this frame, if it's playing.
                    let peak =
                        frame.checked_sub(sound.start).and_then(|f| peaks.as_ref()?.get(f as usize).copied());
                    if let Some(peak) = peak {
                        let bar = Rect::from_center_size(
                            cell.center(),
                            vec2((cell.width() - 8.0) * peak.min(1.0), ROW - 6.0),
                        );
                        painter.rect_filled(bar, 1.0, Color32::from_rgb(0x4c, 0xc2, 0x8a));
                    }
                }
            }
        });

        if let Some((id, frame, start_typing)) = clicked {
            editor.select(Some(id));
            editor.set_frame(frame);
            if start_typing {
                let current =
                    match &columns[layers.iter().position(|&l| l == id).unwrap_or(0)][frame as usize] {
                        Cell::Starts(name) => name.clone(),
                        _ => String::new(),
                    };
                self.typing = Some(Typing::new(id, frame, current));
            }
        }
        if let Some((id, frame, choice)) = menu_choice
            && let Some(layer) = editor.doc.layer_mut(id)
        {
            match choice {
                Some(drawing) => layer.expose(frame, drawing),
                None => {
                    layer.clear_exposure(frame);
                }
            }
        }
        self.finish_typing(ui, editor);
    }

    /// Enter exposes what was typed and moves down a row; Escape or
    /// clicking elsewhere stops.
    fn finish_typing(&mut self, ui: &egui::Ui, editor: &mut Editor) {
        if self.typing.is_none() {
            return;
        }
        let (enter_pressed, escape) =
            ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if escape {
            self.typing = None;
        } else if enter_pressed {
            let Typing { layer, frame, text, .. } = self.typing.take().expect("checked above");
            enter(editor, layer, frame, &text);
            let next = frame + 1;
            if next < editor.duration() {
                editor.set_frame(next);
                self.typing = Some(Typing::new(layer, next, String::new()));
            }
        } else if self.typing.as_ref().is_some_and(|t| t.left) {
            self.typing = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Document, Point, Transform};

    fn mouth() -> (Editor, LayerId) {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("Mouth", vec![], 0, Transform::at(Point::ZERO));
        let layer = editor.doc.layer_mut(id).unwrap();
        layer.drawings = ["closed", "A", "O"].map(|n| Drawing::new(n, Vec::new())).to_vec();
        layer.start = 2;
        layer.end = Some(9);
        (editor, id)
    }

    #[test]
    fn columns_name_each_swap_and_hold_between() {
        let (mut editor, id) = mouth();
        let layer = editor.doc.layer_mut(id).unwrap();
        layer.expose(4, 1);
        layer.expose(6, 2);
        let cells = column(editor.doc.layer(id).unwrap(), 12, 47);
        let starts = |name: &str| Cell::Starts(name.into());
        use Cell::{Holds, Outside};
        assert_eq!(
            cells,
            [
                Outside,
                Outside,
                starts("closed"),
                Holds,
                starts("A"),
                Holds,
                starts("O"),
                Holds,
                Holds,
                Holds,
                Outside,
                Outside
            ]
        );
    }

    #[test]
    fn typing_a_name_exposes_that_drawing_or_makes_one() {
        let (mut editor, id) = mouth();
        enter(&mut editor, id, 5, "o");
        assert_eq!(editor.doc.layer(id).unwrap().drawing_index_at(5), 2);
        enter(&mut editor, id, 7, "smile");
        let layer = editor.doc.layer(id).unwrap();
        assert_eq!(layer.drawings.len(), 4);
        assert_eq!(layer.drawings[layer.drawing_index_at(7)].name, "smile");
        enter(&mut editor, id, 8, "   ");
        assert_eq!(editor.doc.layer(id).unwrap().drawing_index_at(8), 3, "an empty name changes nothing");
    }
}
