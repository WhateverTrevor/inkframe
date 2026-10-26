//! Photoshop's brush controls: the **Brush Settings** panel, the
//! **Brushes** panel and the brush **quick menu**.
//!
//! * Brush Settings edits every part of the brush, a section at a time,
//!   chosen from the list down its left as in Photoshop: Brush Tip Shape,
//!   then the sections with a switch each (Shape Dynamics, Scattering,
//!   Texture, Dual Brush, Color Dynamics, Transfer, and Noise, Wet Edges
//!   and Build-up, which have nothing to set). A stroke along its foot
//!   shows the brush as it is.
//! * Brushes lists the ready-made and saved brushes, each by a sample
//!   stroke, to pick from, save, rename, import and export.
//! * The quick menu opens where the stage is right-clicked with a painting
//!   tool: size and hardness sliders over the brushes, for a change without
//!   leaving the canvas.
//!
//! All of them edit the brush of the tool in use (the Brush tool's when the
//! tool isn't a painting one).

use egui::{Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2, pos2, vec2};
use inkframe_core::{
    Blend, BrushMode, BrushPreset, BrushSettings, BrushStroke, Canvas, Color, Control, Pattern, Point, Tip,
};

use crate::editor::{Editor, Tool};
use crate::i18n::{tr, tr_args};
use crate::icons::{Icon, IconButton};
use crate::inspector::percent;

/// A section of the Brush Settings panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    TipShape,
    Shape,
    Scattering,
    Texture,
    Dual,
    Color,
    Transfer,
    Pose,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::TipShape => tr("Brush Tip Shape"),
            Section::Shape => tr("Shape Dynamics"),
            Section::Scattering => tr("Scattering"),
            Section::Texture => tr("Texture"),
            Section::Dual => tr("Dual Brush"),
            Section::Color => tr("Color Dynamics"),
            Section::Transfer => tr("Transfer"),
            Section::Pose => tr("Brush Pose"),
        }
    }

    /// The section's on switch, if it has one.
    fn switch(self, s: &mut BrushSettings) -> Option<&mut bool> {
        Some(match self {
            Section::TipShape => return None,
            Section::Shape => &mut s.shape.on,
            Section::Scattering => &mut s.scattering.on,
            Section::Texture => &mut s.texture.on,
            Section::Dual => &mut s.dual.on,
            Section::Color => &mut s.color.on,
            Section::Transfer => &mut s.transfer.on,
            Section::Pose => &mut s.pose.on,
        })
    }
}

/// What the brush panels remember between frames.
#[derive(Default)]
pub struct BrushPanels {
    section: Section,
    search: String,
    /// A saved brush being renamed: its index and the name so far.
    renaming: Option<(usize, String)>,
    /// Asked of the app, which owns the file dialogs.
    pub asked: Option<Asked>,
}

/// What the panels ask the app to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Asked {
    ImportBrushes,
    /// Export the saved brushes, or one of them.
    ExportBrushes(Option<usize>),
    DefineBrush,
    ImportPattern,
    DefinePattern,
    ShowBrushSettings,
    ShowBrushes,
}

impl BrushPanels {
    /// Shows a section, by its short name (for screenshots).
    pub fn show_section(&mut self, name: &str) {
        self.section = match name {
            "shape" => Section::Shape,
            "scattering" => Section::Scattering,
            "texture" => Section::Texture,
            "dual" => Section::Dual,
            "color" => Section::Color,
            "transfer" => Section::Transfer,
            "pose" => Section::Pose,
            _ => Section::TipShape,
        };
    }
}

/// The tool whose brush the panels edit.
fn brush_tool(editor: &Editor) -> Tool {
    if editor.brush_for(editor.tool).is_some() { editor.tool } else { Tool::Brush }
}

/// Whether the tool paints with the whole brush engine (the retouching
/// tools use only the tip's size and hardness).
fn full_engine(tool: Tool) -> bool {
    matches!(tool, Tool::Brush | Tool::Eraser | Tool::PatternStamp)
}

/// The Brush Settings panel.
pub fn brush_settings(ui: &mut egui::Ui, editor: &mut Editor, panels: &mut BrushPanels) {
    let tool = brush_tool(editor);
    let paint = (tool != Tool::Eraser).then_some(editor.paint_color);
    if !full_engine(tool) {
        ui.weak(tr_args(
            "The {tool} tool uses the tip's size, hardness and spacing; the other sections are for the Brush and Eraser.",
            &[("tool", tool.label())],
        ));
        ui.add_space(4.0);
    }
    let editing = editor.brush_for(tool);
    let choices = Choices::gather(editor, editing.as_ref());
    let Some(settings) = editor.brush_of(tool) else { return };
    // Side by side when there's room, as in Photoshop; in a narrow panel,
    // the list above the settings, so the sliders keep their width.
    if ui.available_width() >= 560.0 {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(150.0);
                section_list(ui, settings, panels);
            });
            ui.separator();
            ui.vertical(|ui| section_controls(ui, settings, panels.section, &choices, &mut panels.asked));
        });
    } else {
        section_menu(ui, settings, panels);
        ui.separator();
        section_controls(ui, settings, panels.section, &choices, &mut panels.asked);
    }
    ui.add_space(8.0);
    let settings = editor.brush_for(tool).expect("a painting tool");
    stroke_preview(ui, &settings, paint, "brush-settings-preview");
}

/// The sections, each with its switch, and Noise, Wet Edges and Build-up.
fn section_list(ui: &mut egui::Ui, settings: &mut BrushSettings, panels: &mut BrushPanels) {
    if ui.small_button(tr("Brushes…")).on_hover_text(tr("Pick a saved or ready-made brush")).clicked() {
        panels.asked = Some(Asked::ShowBrushes);
    }
    ui.add_space(4.0);
    for section in [
        Section::TipShape,
        Section::Shape,
        Section::Scattering,
        Section::Texture,
        Section::Dual,
        Section::Color,
        Section::Transfer,
        Section::Pose,
    ] {
        ui.horizontal(|ui| {
            match section.switch(settings) {
                Some(on) => {
                    ui.checkbox(on, "");
                }
                None => {
                    ui.add_space(ui.spacing().interact_size.y + ui.spacing().item_spacing.x);
                }
            }
            if ui.selectable_label(panels.section == section, section.label()).clicked() {
                panels.section = section;
                if let Some(on) = section.switch(settings) {
                    *on = true;
                }
            }
        });
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut settings.noise, tr("Noise")).on_hover_text(tr("Grit in the brush's soft edges"));
        ui.checkbox(&mut settings.wet_edges, tr("Wet Edges"))
            .on_hover_text(tr("Paint pools at the stroke's edges, as watercolour does"));
        ui.checkbox(&mut settings.build_up, tr("Build-up"))
            .on_hover_text(tr("Paint keeps flowing while the pen rests, like an airbrush"));
    });
}

/// The sections as a menu, for a narrow panel: the chosen one's switch
/// beside it, and Noise, Wet Edges and Build-up below.
fn section_menu(ui: &mut egui::Ui, settings: &mut BrushSettings, panels: &mut BrushPanels) {
    const SECTIONS: [Section; 8] = [
        Section::TipShape,
        Section::Shape,
        Section::Scattering,
        Section::Texture,
        Section::Dual,
        Section::Color,
        Section::Transfer,
        Section::Pose,
    ];
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("brush_section_menu")
            .selected_text(panels.section.label())
            .width(170.0)
            .show_ui(ui, |ui| {
                for section in SECTIONS {
                    // Which are on, at a glance.
                    let on = section.switch(settings).is_some_and(|on| *on);
                    let label =
                        if on { format!("{}  ✓", section.label()) } else { section.label().to_owned() };
                    ui.selectable_value(&mut panels.section, section, label);
                }
            });
        if let Some(on) = panels.section.switch(settings) {
            ui.checkbox(on, tr("On"));
        }
        if ui.small_button(tr("Brushes…")).clicked() {
            panels.asked = Some(Asked::ShowBrushes);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut settings.noise, tr("Noise")).on_hover_text(tr("Grit in the brush's soft edges"));
        ui.checkbox(&mut settings.wet_edges, tr("Wet Edges"))
            .on_hover_text(tr("Paint pools at the stroke's edges, as watercolour does"));
        ui.checkbox(&mut settings.build_up, tr("Build-up"))
            .on_hover_text(tr("Paint keeps flowing while the pen rests, like an airbrush"));
    });
}

/// The chosen section's settings.
fn section_controls(
    ui: &mut egui::Ui,
    settings: &mut BrushSettings,
    section: Section,
    choices: &Choices,
    asked: &mut Option<Asked>,
) {
    let tips = &choices.tips;
    // The tip pickers wrap, which a grid can't size, so they go above it.
    match section {
        Section::TipShape => {
            ui.label(tr("Tip"));
            tip_picker(ui, "main_tip", &mut settings.tip, tips);
            ui.add_space(6.0);
        }
        Section::Dual => {
            ui.label(tr("Second tip"));
            tip_picker(ui, "dual_tip", &mut settings.dual.tip, tips);
            ui.add_space(6.0);
        }
        _ => {}
    }
    egui::Grid::new("brush_section").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| match section {
        Section::TipShape => tip_shape(ui, settings),
        Section::Shape => shape_dynamics(ui, settings),
        Section::Scattering => scattering(ui, settings),
        Section::Texture => texture(ui, settings, &choices.patterns, asked),
        Section::Dual => dual_brush(ui, settings),
        Section::Color => color_dynamics(ui, settings),
        Section::Transfer => transfer(ui, settings),
        Section::Pose => pose(ui, settings),
    });
}

fn tip_shape(ui: &mut egui::Ui, s: &mut BrushSettings) {
    ui.label(tr("Size"));
    ui.add(egui::Slider::new(&mut s.size, 1.0..=2500.0).logarithmic(true).suffix(" px"));
    ui.end_row();
    ui.label("");
    ui.horizontal(|ui| {
        ui.checkbox(&mut s.flip_x, tr("Flip X"));
        ui.checkbox(&mut s.flip_y, tr("Flip Y"));
    });
    ui.end_row();
    ui.label(tr("Angle and\nroundness"));
    ui.horizontal(|ui| {
        compass(ui, &mut s.angle, &mut s.roundness);
        ui.vertical(|ui| {
            ui.add(egui::DragValue::new(&mut s.angle).range(-180.0..=180.0).suffix("°"));
            ui.add(
                egui::DragValue::new(&mut s.roundness)
                    .range(0.01..=1.0)
                    .speed(0.005)
                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                    .custom_parser(parse_percent),
            );
        });
    });
    ui.end_row();
    ui.label(tr("Hardness"))
        .on_hover_text(tr("How crisp a round tip's edge is; a sampled tip has its own edge"));
    ui.add_enabled(s.tip == Tip::Round, percent(&mut s.hardness));
    ui.end_row();
    ui.label(tr("Spacing")).on_hover_text(tr("Distance between dabs, as a share of the brush's size"));
    ui.add(
        egui::Slider::new(&mut s.spacing, 0.01..=10.0)
            .logarithmic(true)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
}

fn shape_dynamics(ui: &mut egui::Ui, s: &mut BrushSettings) {
    let d = &mut s.shape;
    jitter(ui, tr("Size Jitter"), &mut d.size_jitter);
    control(ui, "size_control", &mut d.size_control, false);
    ui.label(tr("Minimum Diameter"));
    ui.add(percent(&mut d.minimum_diameter));
    ui.end_row();
    ui.label(tr("Tilt Scale")).on_hover_text(tr("With Pen Tilt: how much bigger a pen laid flat paints"));
    ui.add_enabled(
        d.size_control == Control::PenTilt,
        egui::Slider::new(&mut d.tilt_scale, 1.0..=4.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    separator(ui);
    jitter(ui, tr("Angle Jitter"), &mut d.angle_jitter);
    control(ui, "angle_control", &mut d.angle_control, true);
    separator(ui);
    jitter(ui, tr("Roundness Jitter"), &mut d.roundness_jitter);
    control(ui, "roundness_control", &mut d.roundness_control, false);
    ui.label(tr("Minimum Roundness"));
    ui.add(percent(&mut d.minimum_roundness));
    ui.end_row();
    separator(ui);
    ui.label("");
    ui.horizontal(|ui| {
        ui.checkbox(&mut d.flip_x_jitter, tr("Flip X Jitter"));
        ui.checkbox(&mut d.flip_y_jitter, tr("Flip Y Jitter"));
    });
    ui.end_row();
}

fn scattering(ui: &mut egui::Ui, s: &mut BrushSettings) {
    let d = &mut s.scattering;
    ui.label(tr("Scatter")).on_hover_text(tr("How far dabs are thrown from the path, in brush sizes"));
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut d.scatter, 0.0..=10.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        );
        ui.checkbox(&mut d.both_axes, tr("Both Axes"));
    });
    ui.end_row();
    control(ui, "scatter_control", &mut d.scatter_control, false);
    separator(ui);
    ui.label(tr("Count")).on_hover_text(tr("Dabs laid at each step"));
    ui.add(egui::Slider::new(&mut d.count, 1..=16));
    ui.end_row();
    jitter(ui, tr("Count Jitter"), &mut d.count_jitter);
    control(ui, "count_control", &mut d.count_control, false);
}

fn texture(ui: &mut egui::Ui, s: &mut BrushSettings, pictures: &[Pattern], asked: &mut Option<Asked>) {
    let t = &mut s.texture;
    ui.label(tr("Pattern"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("texture_pattern").selected_text(tr_pattern(&t.pattern)).show_ui(
            ui,
            |ui| {
                for pattern in Pattern::MADE.into_iter().chain(pictures.iter().cloned()) {
                    let label = tr_pattern(&pattern);
                    ui.selectable_value(&mut t.pattern, pattern, label);
                }
            },
        );
        ui.checkbox(&mut t.invert, tr("Invert"));
    });
    ui.end_row();
    ui.label("");
    // One under the other: side by side they don't fit a narrow panel.
    ui.vertical(|ui| {
        if ui
            .small_button(tr("Import…"))
            .on_hover_text(tr("A pattern from a picture: light is raised, dark is hollow"))
            .clicked()
        {
            *asked = Some(Asked::ImportPattern);
        }
        if ui
            .small_button(tr("From Selection"))
            .on_hover_text(tr("A pattern from the selected pixels of a paint layer"))
            .clicked()
        {
            *asked = Some(Asked::DefinePattern);
        }
    });
    ui.end_row();
    ui.label("");
    pattern_swatch(ui, t);
    ui.end_row();
    ui.label(tr("Scale"));
    ui.add(
        egui::Slider::new(&mut t.scale, 0.1..=10.0)
            .logarithmic(true)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    ui.label(tr("Brightness"));
    ui.add(egui::Slider::new(&mut t.brightness, -1.0..=1.0));
    ui.end_row();
    ui.label(tr("Contrast"));
    ui.add(egui::Slider::new(&mut t.contrast, -1.0..=1.0));
    ui.end_row();
    separator(ui);
    ui.label("");
    ui.checkbox(&mut t.each_tip, tr("Texture Each Tip"))
        .on_hover_text(tr("Pressed into each dab, so the depth can vary dab by dab"));
    ui.end_row();
    ui.label(tr("Mode"));
    blend_menu(ui, "texture_mode", &mut t.mode);
    ui.end_row();
    ui.label(tr("Depth"));
    ui.add(percent(&mut t.depth));
    ui.end_row();
    ui.add_enabled_ui(t.each_tip, |ui| ui.label(tr("Minimum Depth")));
    ui.add_enabled(t.each_tip, percent(&mut t.minimum_depth));
    ui.end_row();
    ui.add_enabled_ui(t.each_tip, |ui| ui.label(tr("Depth Jitter")));
    ui.add_enabled(t.each_tip, percent(&mut t.depth_jitter));
    ui.end_row();
    if t.each_tip {
        control(ui, "depth_control", &mut t.depth_control, false);
    }
}

fn dual_brush(ui: &mut egui::Ui, s: &mut BrushSettings) {
    let d = &mut s.dual;
    ui.label(tr("Mode"));
    ui.horizontal(|ui| {
        blend_menu(ui, "dual_mode", &mut d.mode);
        ui.checkbox(&mut d.flip, tr("Flip"));
    });
    ui.end_row();
    ui.label(tr("Size"));
    ui.add(egui::Slider::new(&mut d.size, 1.0..=2500.0).logarithmic(true).suffix(" px"));
    ui.end_row();
    ui.label(tr("Spacing"));
    ui.add(
        egui::Slider::new(&mut d.spacing, 0.01..=10.0)
            .logarithmic(true)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    ui.label(tr("Scatter"));
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut d.scatter, 0.0..=10.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        );
        ui.checkbox(&mut d.both_axes, tr("Both Axes"));
    });
    ui.end_row();
    ui.label(tr("Count"));
    ui.add(egui::Slider::new(&mut d.count, 1..=16));
    ui.end_row();
}

fn color_dynamics(ui: &mut egui::Ui, s: &mut BrushSettings) {
    let c = &mut s.color;
    ui.label("");
    ui.checkbox(&mut c.per_tip, tr("Apply Per Tip"))
        .on_hover_text(tr("Each dab gets its own colour, rather than each stroke one"));
    ui.end_row();
    jitter(ui, tr("Foreground/Background Jitter"), &mut c.fg_bg_jitter);
    control(ui, "fg_bg_control", &mut c.fg_bg_control, false);
    separator(ui);
    jitter(ui, tr("Hue Jitter"), &mut c.hue_jitter);
    jitter(ui, tr("Saturation Jitter"), &mut c.saturation_jitter);
    jitter(ui, tr("Brightness Jitter"), &mut c.brightness_jitter);
    ui.label(tr("Purity")).on_hover_text(tr("Less or more saturated, for every colour"));
    ui.add(
        egui::Slider::new(&mut c.purity, -1.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
    );
    ui.end_row();
    ui.label("");
    ui.weak(tr("The background colour is the second colour, under the tool's colour."));
    ui.end_row();
}

fn transfer(ui: &mut egui::Ui, s: &mut BrushSettings) {
    let t = &mut s.transfer;
    jitter(ui, tr("Opacity Jitter"), &mut t.opacity_jitter);
    control(ui, "opacity_control", &mut t.opacity_control, false);
    ui.label(tr("Minimum"));
    ui.add(percent(&mut t.minimum_opacity));
    ui.end_row();
    separator(ui);
    jitter(ui, tr("Flow Jitter"), &mut t.flow_jitter);
    control(ui, "flow_control", &mut t.flow_control, false);
    ui.label(tr("Minimum"));
    ui.add(percent(&mut t.minimum_flow));
    ui.end_row();
}

fn pose(ui: &mut egui::Ui, s: &mut BrushSettings) {
    let p = &mut s.pose;
    ui.label("");
    ui.weak(tr("A pen's pose to paint with when there's no pen, or always with Override."));
    ui.end_row();
    ui.label(tr("Tilt X")).on_hover_text(tr("Leaning left (−) or right (+)"));
    ui.add(egui::Slider::new(&mut p.tilt.0, -1.0..=1.0));
    ui.end_row();
    ui.label(tr("Tilt Y")).on_hover_text(tr("Leaning away (−) or towards you (+)"));
    ui.add(egui::Slider::new(&mut p.tilt.1, -1.0..=1.0));
    ui.end_row();
    ui.label("");
    ui.checkbox(&mut p.override_tilt, tr("Override Tilt"));
    ui.end_row();
    ui.label(tr("Rotation"))
        .on_hover_text(tr("Turns the tip, as turning a pen in the fingers turns a flat brush"));
    ui.add(egui::Slider::new(&mut p.rotation, -180.0..=180.0).suffix("°"));
    ui.end_row();
    ui.label(tr("Pressure"));
    ui.add(percent(&mut p.pressure));
    ui.end_row();
    ui.label("");
    ui.checkbox(&mut p.override_pressure, tr("Override Pressure"));
    ui.end_row();
}

fn jitter(ui: &mut egui::Ui, label: &str, value: &mut f32) {
    ui.label(label);
    ui.add(percent(value));
    ui.end_row();
}

/// A Control menu, with a Fade's steps beside it. `heading` allows the
/// direction controls, which only angles have.
fn control(ui: &mut egui::Ui, id: &str, value: &mut Control, heading: bool) {
    ui.label(tr("Control"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id).selected_text(tr(value.label())).width(130.0).show_ui(ui, |ui| {
            for option in Control::ALL.into_iter().filter(|c| heading || !c.is_heading()) {
                let picked = std::mem::discriminant(value) == std::mem::discriminant(&option);
                if ui.selectable_label(picked, tr(option.label())).clicked() && !picked {
                    *value = option;
                }
            }
        });
        if let Control::Fade(steps) = value {
            ui.add(egui::DragValue::new(steps).range(1..=9999).suffix(tr(" steps")));
        }
    });
    ui.end_row();
}

fn blend_menu(ui: &mut egui::Ui, id: &str, value: &mut Blend) {
    egui::ComboBox::from_id_salt(id).selected_text(tr(value.label())).show_ui(ui, |ui| {
        for mode in Blend::ALL {
            ui.selectable_value(value, mode, tr(mode.label()));
        }
    });
}

fn separator(ui: &mut egui::Ui) {
    ui.separator();
    ui.separator();
    ui.end_row();
}

fn parse_percent(text: &str) -> Option<f64> {
    text.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)
}

/// A pattern's name: a made one's translated, a picture's as it is.
fn tr_pattern(pattern: &Pattern) -> String {
    pattern.made_label().map_or_else(|| pattern.label().to_owned(), |name| tr(name).to_owned())
}

/// Photoshop's angle and roundness widget: an oval in a circle, turned to
/// the angle and squashed to the roundness. Dragging the arrow end turns
/// it; dragging the dots on its sides squashes it.
fn compass(ui: &mut egui::Ui, angle: &mut f32, roundness: &mut f32) {
    let size = 64.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());
    let centre = rect.center();
    let radius = size / 2.0 - 4.0;
    let turn = angle.to_radians();
    // Up the screen is positive, as the angle is.
    let along = vec2(turn.cos(), -turn.sin());
    let across = vec2(turn.sin(), turn.cos());
    let squash_dot = centre + across * radius * *roundness;
    let id = response.id.with("squashing");
    if response.drag_started()
        && let Some(p) = response.interact_pointer_pos()
    {
        let near_dot = p.distance(squash_dot).min(p.distance(centre - across * radius * *roundness)) < 8.0;
        ui.data_mut(|d| d.insert_temp(id, near_dot));
    }
    if response.dragged()
        && let Some(p) = response.interact_pointer_pos()
    {
        let offset = p - centre;
        if ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
            *roundness = (offset.dot(across).abs() / radius).clamp(0.01, 1.0);
        } else if offset.length() > 2.0 {
            *angle = (-offset.y).atan2(offset.x).to_degrees().round();
        }
    }
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.circle_stroke(centre, radius, Stroke::new(1.0, visuals.weak_text_color()));
    let points: Vec<Pos2> = (0..48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            centre + along * a.cos() * radius + across * a.sin() * radius * *roundness
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(
        points,
        visuals.selection.bg_fill.gamma_multiply(0.5),
        Stroke::new(1.0, visuals.text_color()),
    ));
    painter.arrow(centre - along * radius, along * radius * 2.0, Stroke::new(1.5, visuals.text_color()));
    for dot in [squash_dot, centre - across * radius * *roundness] {
        painter.circle_filled(dot, 3.5, visuals.text_color());
    }
    response.on_hover_text(tr("Drag the arrow to turn the tip, or a dot to squash it"));
}

/// The tips and picture patterns to choose from, gathered from the
/// brushes (each once) and the brush being edited.
struct Choices {
    /// Round first, then each sampled tip.
    tips: Vec<Tip>,
    patterns: Vec<Pattern>,
}

impl Choices {
    fn gather(editor: &Editor, editing: Option<&BrushSettings>) -> Self {
        let mut choices = Self { tips: vec![Tip::Round], patterns: Vec::new() };
        let brushes = editor.brushes.all().map(|(preset, _)| &preset.settings).chain(editing);
        for settings in brushes {
            for tip in [&settings.tip, &settings.dual.tip] {
                if let Tip::Sampled(image) = tip
                    && !choices.tips.iter().any(|t| matches!(t, Tip::Sampled(i) if same(i, image)))
                {
                    choices.tips.push(tip.clone());
                }
            }
            if let Pattern::Picture(image) = &settings.texture.pattern
                && !choices.patterns.iter().any(|p| matches!(p, Pattern::Picture(i) if same(i, image)))
            {
                choices.patterns.push(settings.texture.pattern.clone());
            }
        }
        choices
    }
}

/// The patterns to choose from: the made ones, then pictures in the
/// brushes and the Pattern Stamp's.
pub(crate) fn pattern_choices(editor: &Editor) -> Vec<Pattern> {
    let mut patterns: Vec<Pattern> = Pattern::MADE.into();
    for picture in
        Choices::gather(editor, None).patterns.into_iter().chain([editor.stamp_pattern.pattern.clone()])
    {
        if !patterns.contains(&picture) {
            patterns.push(picture);
        }
    }
    patterns
}

/// A menu of `patterns`, the picked one in `value`.
pub(crate) fn pattern_menu(ui: &mut egui::Ui, id: &str, value: &mut Pattern, patterns: &[Pattern]) {
    egui::ComboBox::from_id_salt(id).selected_text(tr_pattern(value)).show_ui(ui, |ui| {
        for pattern in patterns {
            ui.selectable_value(value, pattern.clone(), tr_pattern(pattern));
        }
    });
}

/// Whether two pictures are the same one.
fn same(a: &inkframe_core::TipImage, b: &inkframe_core::TipImage) -> bool {
    std::ptr::eq(a, b) || (a.name == b.name && a.amounts == b.amounts)
}

/// A row of tips to pick from, the picked one outlined.
fn tip_picker(ui: &mut egui::Ui, id: &str, value: &mut Tip, tips: &[Tip]) {
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            let mut shown = tips.to_vec();
            if !shown.contains(value) {
                shown.push(value.clone());
            }
            for tip in &shown {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                let picked = tip == value;
                let visuals = ui.visuals();
                ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(0xf4, 0xf1, 0xea));
                match tip {
                    Tip::Round => {
                        ui.painter().circle_filled(rect.center(), 11.0, Color32::from_gray(30));
                    }
                    Tip::Sampled(image) => {
                        let texture = tip_texture(ui.ctx(), image);
                        let aspect = image.width as f32 / image.height as f32;
                        let size =
                            if aspect >= 1.0 { vec2(28.0, 28.0 / aspect) } else { vec2(28.0 * aspect, 28.0) };
                        ui.painter().image(
                            texture.id(),
                            Rect::from_center_size(rect.center(), size),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                }
                if picked {
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(2.0, visuals.selection.stroke.color),
                        egui::StrokeKind::Inside,
                    );
                }
                let name = match tip {
                    Tip::Round => tr("Round").to_owned(),
                    Tip::Sampled(image) => image.name.clone(),
                };
                if response.on_hover_text(name).clicked() {
                    *value = tip.clone();
                }
            }
        });
    });
}

/// A sampled tip as a picture: its paint dark on clear.
fn tip_texture(ctx: &egui::Context, image: &inkframe_core::TipImage) -> TextureHandle {
    let id = egui::Id::new(("tip-texture", &image.name, image.width, image.height, image.amounts.len()));
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let pixels: Vec<Color32> =
        image.amounts.iter().map(|&a| Color32::from_rgba_premultiplied(0, 0, 0, a)).collect();
    let picture = egui::ColorImage {
        size: [image.width as usize, image.height as usize],
        pixels,
        source_size: Vec2::new(image.width as f32, image.height as f32),
    };
    let texture = ctx.load_texture(format!("tip-{}", image.name), picture, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// A square of the texture's pattern as it'll bite: dark where the paint
/// is kept out.
fn pattern_swatch(ui: &mut egui::Ui, texture: &inkframe_core::Texture) {
    const SIDE: u32 = 96;
    let key = format!(
        "{:?}",
        (&texture.pattern, texture.invert, texture.scale, texture.brightness, texture.contrast)
    );
    let id = egui::Id::new("pattern-swatch");
    let cached: Option<(String, TextureHandle)> = ui.data(|d| d.get_temp(id));
    let handle = match cached {
        Some((k, handle)) if k == key => handle,
        _ => {
            let pixels = (0..SIDE * SIDE)
                .map(|i| Color32::from_gray((texture.height(i % SIDE, i / SIDE) * 255.0) as u8))
                .collect();
            let picture =
                egui::ColorImage { size: [SIDE as usize; 2], pixels, source_size: Vec2::splat(SIDE as f32) };
            let handle = ui.ctx().load_texture("pattern-swatch", picture, egui::TextureOptions::NEAREST);
            ui.data_mut(|d| d.insert_temp(id, (key, handle.clone())));
            handle
        }
    };
    ui.image((handle.id(), Vec2::splat(SIDE as f32 * 0.75)));
}

/// A stroke painted with `settings`, swelling and thinning as a pen's
/// would, along an S. For the eraser (`paint` is `None`), a stroke erased
/// out of a patch of colour. Big brushes are shown smaller, to fit. Redrawn
/// only when the settings change.
pub fn stroke_preview(ui: &mut egui::Ui, settings: &BrushSettings, paint: Option<Color>, id: &str) {
    const WIDTH: u32 = 300;
    const HEIGHT: u32 = 90;
    let shown = settings.size.min(HEIGHT as f32 * 0.55);
    let texture = stroke_texture(ui.ctx(), id, settings, paint, (WIDTH, HEIGHT), shown);
    let size = vec2(WIDTH as f32, HEIGHT as f32) * (ui.available_width() / WIDTH as f32).min(1.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 4.0, paper_behind(paint));
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if shown < settings.size {
        ui.weak(tr_args(
            "Shown at {shown} px of {size} px",
            &[("shown", &format!("{shown:.0}")), ("size", &format!("{:.0}", settings.size))],
        ));
    }
}

/// Paper behind a sample stroke, light or dark to suit its colour.
fn paper_behind(paint: Option<Color>) -> Color32 {
    if paint.is_some_and(|c| (c.r as u32 + c.g as u32 + c.b as u32) > 600) {
        Color32::from_gray(60)
    } else {
        Color32::from_rgb(0xf4, 0xf1, 0xea)
    }
}

/// What a sample stroke was painted with: redrawn when it changes.
type StrokeKey = (BrushSettings, Option<Color>, f32);

/// A sample stroke as a texture, `width`×`height`, with the brush at
/// `size`: cached under `id` until the settings change.
fn stroke_texture(
    ctx: &egui::Context,
    id: &str,
    settings: &BrushSettings,
    paint: Option<Color>,
    (width, height): (u32, u32),
    size: f32,
) -> TextureHandle {
    let key = egui::Id::new(("stroke-texture", id));
    let wanted = (settings.clone(), paint, size);
    let cached: Option<(StrokeKey, TextureHandle)> = ctx.data(|d| d.get_temp(key));
    if let Some((had, texture)) = cached
        && had == wanted
    {
        return texture;
    }
    let mut canvas = Canvas::new(width, height);
    let ink = paint.unwrap_or(Color::rgb(120, 130, 150));
    if paint.is_none() {
        // Something to erase from.
        let patch: Vec<u8> = (0..width * height).flat_map(|_| [ink.r, ink.g, ink.b, 255]).collect();
        canvas.write_region(0, 0, width, height, &patch);
    }
    let mode = if paint.is_some() { BrushMode::Paint(ink) } else { BrushMode::Erase };
    // Scattering and the dual brush are in brush sizes, so they shrink
    // with it.
    let scale = size / settings.size.max(1.0);
    let mut shrunk = BrushSettings { size, ..settings.clone() };
    shrunk.dual.size *= scale;
    let mut stroke = BrushStroke::new(shrunk, mode).with_background(Color::rgb(255, 255, 255)).with_seed(7);
    let steps = 120;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let margin = (size * 0.6).min(width as f32 * 0.2) + 6.0;
        let x = margin + t * (width as f32 - margin * 2.0);
        let y = height as f32 / 2.0 + (t * std::f32::consts::TAU).sin() * height as f32 * 0.2;
        // Light at the ends, full in the middle.
        let pressure = 0.15 + 0.85 * (t * std::f32::consts::PI).sin();
        stroke.add_point(&mut canvas, Point::new(x, y), pressure);
    }
    let image =
        egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], &canvas.to_pixels());
    let texture = ctx.load_texture(format!("stroke-{id}"), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(key, (wanted, texture.clone())));
    texture
}

/// The Brushes panel: a size slider, the brushes to pick from, and saving,
/// importing and exporting them.
pub fn brushes(ui: &mut egui::Ui, editor: &mut Editor, panels: &mut BrushPanels) {
    let tool = brush_tool(editor);
    if let Some(settings) = editor.brush_of(tool) {
        ui.horizontal(|ui| {
            ui.label(tr("Size"));
            ui.add(egui::Slider::new(&mut settings.size, 1.0..=2500.0).logarithmic(true).suffix(" px"));
        });
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut panels.search)
                .hint_text(tr("Search brushes"))
                .desired_width(f32::INFINITY),
        );
    });
    ui.add_space(4.0);
    if let Some(picked) = preset_list(ui, editor, panels, false) {
        apply_preset(editor, tool, picked);
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if ui.add(IconButton::new(Icon::Plus, tr("Save the brush as a new preset"))).clicked() {
            let settings = editor.brush_for(tool).expect("a painting tool");
            let name = editor.brushes.add(tr("Brush"), settings);
            let index = editor.brushes.saved.len() - 1;
            editor.brushes.picked = Some(name.clone());
            panels.renaming = Some((index, name));
        }
        if ui
            .small_button(tr("Import…"))
            .on_hover_text(tr("Add brushes from a brush set or a Photoshop .abr file"))
            .clicked()
        {
            panels.asked = Some(Asked::ImportBrushes);
        }
        if ui
            .add_enabled(!editor.brushes.saved.is_empty(), egui::Button::new(tr("Export…")).small())
            .on_hover_text(tr("Save your brushes as a brush set to share"))
            .clicked()
        {
            panels.asked = Some(Asked::ExportBrushes(None));
        }
        if ui
            .small_button(tr("Define from Selection"))
            .on_hover_text(tr(
                "A new brush tip from the selected pixels (or the whole paint layer): dark is paint",
            ))
            .clicked()
        {
            panels.asked = Some(Asked::DefineBrush);
        }
    });
}

/// Makes `preset` the brush of `tool`.
fn apply_preset(editor: &mut Editor, tool: Tool, preset: BrushPreset) {
    if let Some(settings) = editor.brush_of(tool) {
        *settings = preset.settings;
    }
    editor.brushes.picked = Some(preset.name);
}

/// The brushes, each a row with its sample stroke and name (or, `compact`,
/// a grid of strokes), filtered by the search. Returns the one clicked.
fn preset_list(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    panels: &mut BrushPanels,
    compact: bool,
) -> Option<BrushPreset> {
    let query = panels.search.trim().to_lowercase();
    let mut picked = None;
    let mut action: Option<(usize, &str)> = None;
    let dark = ui.visuals().dark_mode;
    let ink = if dark { Color::rgb(235, 235, 235) } else { Color::rgb(30, 30, 30) };
    let entries: Vec<(BrushPreset, Option<usize>)> = {
        let mut saved_index = 0;
        editor
            .brushes
            .all()
            .map(|(preset, saved)| {
                let index = saved.then(|| {
                    saved_index += 1;
                    saved_index - 1
                });
                (preset.clone(), index)
            })
            .filter(|(preset, _)| {
                query.is_empty()
                    || preset.name.to_lowercase().contains(&query)
                    || preset_name(&preset.name).to_lowercase().contains(&query)
            })
            .collect()
    };
    let current = editor.brushes.picked.clone();
    let layout = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
        if compact {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                add(ui)
            });
        } else {
            ui.vertical(|ui| add(ui));
        }
    };
    layout(ui, &mut |ui: &mut egui::Ui| {
        for (preset, saved) in &entries {
            let size = if compact { vec2(72.0, 34.0) } else { vec2(ui.available_width(), 40.0) };
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let is_current = current.as_deref() == Some(preset.name.as_str());
            let visuals = ui.visuals();
            if is_current {
                ui.painter().rect_filled(rect, 4.0, visuals.selection.bg_fill.gamma_multiply(0.5));
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
            }
            let thumb_width = if compact { 72 } else { 110 };
            let texture = stroke_texture(
                ui.ctx(),
                &format!("preset-{}", preset.name),
                &preset.settings,
                Some(ink),
                (thumb_width, 34),
                preset.settings.size.min(16.0),
            );
            let thumb = Rect::from_min_size(
                rect.min + vec2(2.0, (rect.height() - 34.0) / 2.0),
                vec2(thumb_width as f32, 34.0),
            );
            ui.painter().image(
                texture.id(),
                thumb,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            let name = preset_name(&preset.name);
            if !compact {
                let renaming = saved.zip(panels.renaming.as_mut()).filter(|(i, (r, _))| i == r);
                let text_rect = Rect::from_min_max(pos2(thumb.right() + 8.0, rect.top()), rect.max);
                if let Some((index, (_, text))) = renaming {
                    let edit = ui.put(text_rect.shrink2(vec2(0.0, 8.0)), egui::TextEdit::singleline(text));
                    edit.request_focus();
                    if edit.lost_focus() {
                        let name = text.clone();
                        editor.brushes.rename(index, &name);
                        editor.brushes.picked = editor.brushes.saved.get(index).map(|p| p.name.clone());
                        panels.renaming = None;
                    }
                } else {
                    ui.painter().text(
                        pos2(text_rect.left(), rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &name,
                        egui::FontId::proportional(13.0),
                        visuals.text_color(),
                    );
                    ui.painter().text(
                        pos2(rect.right() - 6.0, rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        format!("{:.0}", preset.settings.size),
                        egui::FontId::proportional(11.0),
                        visuals.weak_text_color(),
                    );
                }
            }
            let response = response.on_hover_text(&name);
            if response.clicked() {
                picked = Some(preset.clone());
            }
            if let Some(index) = saved {
                response.context_menu(|ui| {
                    if ui.button(tr("Rename")).clicked() {
                        action = Some((*index, "rename"));
                        ui.close();
                    }
                    if ui.button(tr("Update from the Current Brush")).clicked() {
                        action = Some((*index, "update"));
                        ui.close();
                    }
                    if ui.button(tr("Export…")).clicked() {
                        action = Some((*index, "export"));
                        ui.close();
                    }
                    if ui.button(tr("Delete")).clicked() {
                        action = Some((*index, "delete"));
                        ui.close();
                    }
                });
            }
        }
    });
    if entries.is_empty() {
        ui.weak(tr("No brushes match."));
    }
    match action {
        Some((index, "rename")) => panels.renaming = Some((index, editor.brushes.saved[index].name.clone())),
        Some((index, "update")) => {
            let settings = editor.brush_for(brush_tool(editor)).expect("a painting tool");
            editor.brushes.update(index, settings);
        }
        Some((index, "export")) => panels.asked = Some(Asked::ExportBrushes(Some(index))),
        Some((index, "delete")) => editor.brushes.remove(index),
        _ => {}
    }
    picked
}

/// A ready-made brush's name, translated; a saved one's as it is.
pub(crate) fn preset_name(name: &str) -> String {
    match crate::brushes::BrushLibrary::built_in().iter().find(|p| p.name == name) {
        // The ready-made brushes live for good, so their names can be
        // looked up as texts.
        Some(preset) => tr(preset.name.as_str()).to_owned(),
        None => name.to_owned(),
    }
}

/// The quick menu, if open: where the stage was right-clicked, a small
/// panel with the size and hardness over the brushes. Closes on a click
/// elsewhere or Escape.
pub fn quick_menu(ctx: &egui::Context, editor: &mut Editor, panels: &mut BrushPanels) {
    let Some((x, y)) = editor.brush_quick_menu else { return };
    let at = pos2(x, y);
    let tool = brush_tool(editor);
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));
    let area = egui::Area::new(egui::Id::new("brush-quick-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(330.0);
                if let Some(settings) = editor.brush_of(tool) {
                    egui::Grid::new("quick_brush").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label(tr("Size"));
                        ui.add(
                            egui::Slider::new(&mut settings.size, 1.0..=2500.0)
                                .logarithmic(true)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label(tr("Hardness"));
                        ui.add_enabled(settings.tip == Tip::Round, percent(&mut settings.hardness));
                        ui.end_row();
                        if full_engine(tool) {
                            ui.label(tr("Opacity"));
                            ui.add(percent(&mut settings.opacity));
                            ui.end_row();
                            ui.label(tr("Flow"));
                            ui.add(percent(&mut settings.flow));
                            ui.end_row();
                        }
                    });
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
                    if let Some(preset) = preset_list(ui, editor, panels, true) {
                        apply_preset(editor, tool, preset);
                    }
                });
                ui.separator();
                if ui.small_button(tr("Brush Settings…")).clicked() {
                    panels.asked = Some(Asked::ShowBrushSettings);
                    close = true;
                }
            });
        });
    let clicked_outside = ctx.input(|i| i.pointer.any_pressed())
        && ctx.pointer_interact_pos().is_some_and(|p| !area.response.rect.contains(p));
    if close || clicked_outside {
        editor.brush_quick_menu = None;
    }
}

/// The pen-pressure switches of Photoshop's options bar: pressure for size
/// (Shape Dynamics' size control) and for opacity (Transfer's).
pub fn pressure_switches(ui: &mut egui::Ui, settings: &mut BrushSettings) {
    let mut size = settings.shape.on && settings.shape.size_control == Control::PenPressure;
    let mut opacity = settings.transfer.on && settings.transfer.opacity_control == Control::PenPressure;
    if ui.checkbox(&mut size, tr("Size")).on_hover_text(tr("Pressing harder paints bigger")).changed() {
        settings.shape.size_control = if size { Control::PenPressure } else { Control::Off };
        settings.shape.on |= size;
    }
    if ui
        .checkbox(&mut opacity, tr("Opacity"))
        .on_hover_text(tr("Pressing harder paints more solidly"))
        .changed()
    {
        settings.transfer.opacity_control = if opacity { Control::PenPressure } else { Control::Off };
        settings.transfer.on |= opacity;
    }
}
