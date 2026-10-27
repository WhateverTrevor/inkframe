//! The document settings in the Properties panel: size, frame rate,
//! background and playback, the soundtrack, and web page actions.

use egui::DragValue;
use inkframe_core::FrameRate;

use super::choice;
use crate::audio_engine::AudioEngine;
use crate::convert::color_button;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};

pub(super) fn document_section(ui: &mut egui::Ui, editor: &mut Editor) {
    let doc = &mut editor.doc;
    egui::Grid::new("document").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Stage size"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut doc.width).range(1..=16384).suffix(" w"));
            ui.add(DragValue::new(&mut doc.height).range(1..=16384).suffix(" h"));
        });
        ui.end_row();

        ui.label(tr("Frame rate"));
        egui::ComboBox::from_id_salt("frame_rate").selected_text(doc.frame_rate.to_string()).show_ui(
            ui,
            |ui| {
                for rate in FrameRate::PRESETS {
                    ui.selectable_value(&mut doc.frame_rate, rate, rate.to_string());
                }
            },
        );
        ui.end_row();

        ui.label(tr("Duration"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut doc.duration).range(1..=100_000).suffix(format!(" {}", tr("frames"))));
            ui.weak(format!("{:.2}s", doc.duration as f64 / doc.frame_rate.fps()));
        });
        ui.end_row();

        ui.label(tr("Background"));
        color_button(ui, &mut doc.background);
        ui.end_row();

        ui.label(tr("Stacking"));
        ui.checkbox(&mut doc.stack_by_depth, tr("By depth")).on_hover_text(STACK_BY_DEPTH);
        ui.end_row();
    });
    // Shortening the animation can strand the playhead past the end.
    editor.set_frame(editor.frame);
}

/// What "stack by depth" does, for its checkboxes.
pub const STACK_BY_DEPTH: &str = "Draw layers far to near by their Depth on each frame, so a layer that moves back \
     passes behind the others. Layers at the same depth keep the timeline's order. Off, the timeline's order \
     decides what's in front.";

/// The document's audio tracks: when each starts, how loud it is, and
/// whether it plays at all. Sound files are added with File › Import Audio.
pub fn audio_section(ui: &mut egui::Ui, editor: &mut Editor, audio: &mut AudioEngine) -> Option<AudioAction> {
    let mut action = None;
    let header = tr_args("Audio ({count})", &[("count", &editor.doc.audio.len().to_string())]);
    egui::CollapsingHeader::new(header).id_salt("audio").default_open(!editor.doc.audio.is_empty()).show(
        ui,
        |ui| {
            if let Some(error) = audio.output_error() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr_args("No sound output: {error}", &[("error", error)]),
                );
            }
            ui.checkbox(&mut audio.scrub, tr("Scrub audio"))
                .on_hover_text(tr("Play each frame's sound as you move the playhead, for lip sync"));

            let last_frame = editor.doc.last_frame();
            let mut remove = None;
            for track in &mut editor.doc.audio {
                ui.push_id(track.id, |ui| {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut track.name).desired_width(150.0));
                        if ui
                            .add(crate::icons::IconButton::new(
                                crate::icons::Icon::Trash,
                                tr("Remove this audio"),
                            ))
                            .clicked()
                        {
                            remove = Some(track.id);
                        }
                    });
                    egui::Grid::new("audio_track").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label(tr("Starts at"));
                        let mut shown = track.start + 1;
                        if ui
                            .add(
                                DragValue::new(&mut shown)
                                    .range(1..=last_frame + 1)
                                    .prefix(format!("{} ", tr("frame"))),
                            )
                            .changed()
                        {
                            track.start = shown - 1;
                        }
                        ui.end_row();

                        ui.label(tr("Volume"));
                        let mut percent = track.volume * 100.0;
                        if ui.add(egui::Slider::new(&mut percent, 0.0..=200.0).suffix("%")).changed() {
                            track.volume = percent / 100.0;
                        }
                        ui.end_row();

                        ui.label("");
                        ui.checkbox(&mut track.muted, tr("Mute"));
                        ui.end_row();
                    });
                    ui.weak(track.path.display().to_string());
                });
            }
            if let Some(id) = remove {
                editor.doc.remove_audio(id);
            }

            ui.add_space(4.0);
            if ui.button(tr("Import Audio…")).clicked() {
                action = Some(AudioAction::Import);
            }
        },
    );
    action
}

/// Something the audio section asks the app to do, because it needs a
/// file dialog.
pub enum AudioAction {
    Import,
}

/// Picks a web page action (see `web_actions.rs` in the core), or none:
/// what a button does, or a frame. Frames are shown 1-based.
pub fn web_action_editor(
    ui: &mut egui::Ui,
    id: &str,
    action: &mut Option<inkframe_core::WebAction>,
    last_frame: u32,
) {
    use inkframe_core::WebAction;
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        None,
        Play,
        Stop,
        GotoAndPlay,
        GotoAndStop,
        Link,
    }
    let kind = match action {
        None => Kind::None,
        Some(WebAction::Play) => Kind::Play,
        Some(WebAction::Stop) => Kind::Stop,
        Some(WebAction::GotoAndPlay(_)) => Kind::GotoAndPlay,
        Some(WebAction::GotoAndStop(_)) => Kind::GotoAndStop,
        Some(WebAction::OpenLink(_)) => Kind::Link,
    };
    let mut chosen = kind;
    choice(
        ui,
        id,
        &mut chosen,
        &[
            (Kind::None, tr("Nothing")),
            (Kind::Play, tr("Play")),
            (Kind::Stop, tr("Stop")),
            (Kind::GotoAndPlay, tr("Go to & play")),
            (Kind::GotoAndStop, tr("Go to & stop")),
            (Kind::Link, tr("Open link")),
        ],
    );
    if chosen != kind {
        let frame = match action {
            Some(WebAction::GotoAndPlay(f) | WebAction::GotoAndStop(f)) => *f,
            _ => 0,
        };
        *action = match chosen {
            Kind::None => None,
            Kind::Play => Some(WebAction::Play),
            Kind::Stop => Some(WebAction::Stop),
            Kind::GotoAndPlay => Some(WebAction::GotoAndPlay(frame)),
            Kind::GotoAndStop => Some(WebAction::GotoAndStop(frame)),
            Kind::Link => Some(WebAction::OpenLink("https://".into())),
        };
    }
    match action {
        Some(WebAction::GotoAndPlay(f) | WebAction::GotoAndStop(f)) => {
            let mut shown = *f + 1;
            if ui
                .add(DragValue::new(&mut shown).range(1..=last_frame + 1).prefix(format!("{} ", tr("frame"))))
                .changed()
            {
                *f = shown - 1;
            }
        }
        Some(WebAction::OpenLink(url)) => {
            ui.add(egui::TextEdit::singleline(url).desired_width(140.0));
        }
        _ => {}
    }
}
