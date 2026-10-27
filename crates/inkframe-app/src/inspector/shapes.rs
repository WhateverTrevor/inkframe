//! The selected shapes in the Properties panel: their fill and stroke, size,
//! and the text and picture editors.

use egui::DragValue;
use inkframe_core::{Color, ImageId, LayerId, Shape, Stroke, Style, Text, TextAlign};

use super::brush_symbols;
use super::sections::section;
use crate::convert::color_button;
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::icons::Icon;

pub(super) fn shapes_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    if editor.doc.layer(id).is_none_or(|l| l.elements_at(frame).is_empty())
        || !section(ui, editor, "shapes", tr("Shapes"), Icon::Shapes)
    {
        return;
    }
    // What the image rows show, gathered before the layer is borrowed for
    // editing (it lives inside the same document).
    let images: Vec<ImageInfo> = editor
        .doc
        .images
        .iter()
        .map(|asset| ImageInfo {
            id: asset.id,
            name: asset.name.clone(),
            size: (asset.width, asset.height),
            problem: editor.images.problem(asset.id).map(str::to_owned),
        })
        .collect();

    // Why each family from a project font file is unavailable, if it is.
    let font_problems: Vec<(String, String)> = editor
        .font_problems
        .iter()
        .filter_map(|(id, problem)| {
            editor.doc.fonts.iter().find(|f| f.id == *id).map(|f| (&f.families, problem))
        })
        .flat_map(|(families, problem)| families.iter().map(move |family| (family.clone(), problem.clone())))
        .collect();
    let symbols = brush_symbols(editor);
    let Some(layer) = editor.doc.layer_mut(id) else { return };
    let elements = layer.elements_at_mut(frame);
    let count = elements.len();
    for (i, element) in elements.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            let title = match &element.shape {
                Shape::Rect { .. } => tr("Rectangle"),
                Shape::Ellipse { .. } => tr("Oval"),
                Shape::Path(_) => tr("Stroke"),
                Shape::Line { .. } => tr("Pencil Line"),
                Shape::Image { .. } => tr("Image"),
                Shape::Text(_) => tr("Text"),
                Shape::Paint { .. } => tr("Painting"),
            };
            ui.strong(if count > 1 { format!("{title} {}", i + 1) } else { title.to_owned() });
            if let Shape::Paint { width, height, .. } = element.shape {
                // Painted with the brush tools, so nothing to set here
                // beyond saying how big the canvas is.
                ui.weak(tr_args(
                    "{width} × {height} px canvas. Paint with the Brush (B) and Eraser (E).",
                    &[("width", &width.to_string()), ("height", &height.to_string())],
                ));
            } else if let Shape::Image { image, width, height } = &mut element.shape {
                // An image paints its own pixels, so it has no fill or stroke.
                if let Some(info) = images.iter().find(|info| info.id == *image) {
                    image_editor(ui, info, width, height);
                }
            } else if let Shape::Text(text) = &mut element.shape {
                let problem =
                    font_problems.iter().find(|(f, _)| *f == text.font.family).map(|(_, p)| p.as_str());
                text_editor(ui, text, problem);
                style_editor(ui, &mut element.style, &symbols);
            } else {
                shape_size_editor(ui, &mut element.shape);
                style_editor(ui, &mut element.style, &symbols);
            }
        });
    }
}

pub(super) struct ImageInfo {
    id: ImageId,
    name: String,
    /// The file's own size in pixels.
    size: (u32, u32),
    problem: Option<String>,
}

pub(super) fn image_editor(ui: &mut egui::Ui, info: &ImageInfo, width: &mut f32, height: &mut f32) {
    egui::Grid::new("image").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("File"));
        match &info.problem {
            Some(problem) => ui.colored_label(ui.visuals().error_fg_color, problem),
            None => ui.label(&info.name),
        };
        ui.end_row();
        ui.label(tr("Size"));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(width).range(0.0..=f32::MAX).prefix("w "));
            ui.add(DragValue::new(height).range(0.0..=f32::MAX).prefix("h "));
        });
        ui.end_row();
        ui.label("");
        let (w, h) = info.size;
        if ui
            .button(tr("Actual size"))
            .on_hover_text(tr_args(
                "{width} × {height}, one stage pixel per image pixel",
                &[("width", &w.to_string()), ("height", &h.to_string())],
            ))
            .clicked()
        {
            (*width, *height) = (w as f32, h as f32);
        }
        ui.end_row();
    });
}

/// The text itself and its font settings. (Text can also be typed straight
/// onto the stage; see `text_edit.rs`.) `font_problem` explains why the
/// project's own font file for this family couldn't be used, if that's the case.
pub(super) fn text_editor(ui: &mut egui::Ui, text: &mut Text, font_problem: Option<&str>) {
    egui::TextEdit::multiline(&mut text.content).desired_rows(2).desired_width(f32::INFINITY).show(ui);

    egui::Grid::new("text").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Font"));
        ui.horizontal(|ui| {
            let families = inkframe_render::font_families();
            egui::ComboBox::from_id_salt("font").selected_text(&text.font.family).width(160.0).show_ui(
                ui,
                |ui| {
                    // The project's own fonts first, where they're easy to find
                    // among hundreds of installed ones.
                    if !families.project.is_empty() {
                        ui.weak(tr("This project"));
                        for family in &families.project {
                            ui.selectable_value(&mut text.font.family, family.clone(), family);
                        }
                        ui.separator();
                        ui.weak(tr("Installed"));
                    }
                    for family in &families.installed {
                        ui.selectable_value(&mut text.font.family, family.clone(), family);
                    }
                },
            );
            if ui
                .small_button(tr("Refresh"))
                .on_hover_text(tr("Look again for fonts installed since Inkframe started"))
                .clicked()
            {
                inkframe_render::refresh_installed_fonts();
            }
        });
        ui.end_row();
        // Name the stand-in when the chosen font isn't available (a project
        // from someone else's computer, say), and why, if it's known.
        let used = inkframe_render::layout_text(text).family.clone();
        if used != text.font.family {
            let reason = font_problem.unwrap_or(tr("Not installed"));
            ui.label("");
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr_args("{reason}; showing {font}", &[("reason", reason), ("font", &used)]),
            );
            ui.end_row();
        }

        ui.label(tr("Style"));
        ui.horizontal(|ui| {
            ui.toggle_value(&mut text.font.bold, tr("Bold"));
            ui.toggle_value(&mut text.font.italic, tr("Italic"));
        });
        ui.end_row();
        ui.label(tr("Size"));
        ui.add(DragValue::new(&mut text.size).range(1.0..=2000.0).suffix(" px"));
        ui.end_row();
        ui.label(tr("Align"));
        ui.horizontal(|ui| {
            for align in TextAlign::ALL {
                ui.selectable_value(&mut text.align, align, tr(align.label()));
            }
        });
        ui.end_row();
        ui.label(tr("Line spacing"));
        ui.add(DragValue::new(&mut text.line_spacing).range(0.5..=5.0).speed(0.01).suffix("×"));
        ui.end_row();
    });
}

pub(super) fn shape_size_editor(ui: &mut egui::Ui, shape: &mut Shape) {
    egui::Grid::new("shape").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match shape {
        Shape::Rect { width, height, corner_radius } => {
            ui.label(tr("Size"));
            ui.horizontal(|ui| {
                ui.add(DragValue::new(width).range(0.0..=f32::MAX).prefix("w "));
                ui.add(DragValue::new(height).range(0.0..=f32::MAX).prefix("h "));
            });
            ui.end_row();
            ui.label(tr("Corners"));
            ui.add(DragValue::new(corner_radius).range(0.0..=f32::MAX).suffix(" px"));
            ui.end_row();
        }
        Shape::Ellipse { radius_x, radius_y } => {
            ui.label(tr("Radius"));
            ui.horizontal(|ui| {
                ui.add(DragValue::new(radius_x).range(0.0..=f32::MAX).prefix("x "));
                ui.add(DragValue::new(radius_y).range(0.0..=f32::MAX).prefix("y "));
            });
            ui.end_row();
        }
        // Freehand paths are edited by redrawing, for now. Images and text
        // have their own editors.
        // A pencil line's widths are set with the Width tool, on the stage;
        // here, all of them at once.
        Shape::Line { widths, .. } => {
            ui.label(tr("Width")).on_hover_text(tr(
                "Scales the whole line; the Width tool (Shift+A) shapes it point by point",
            ));
            let widest = widths.iter().copied().fold(0.0f32, f32::max).max(0.01);
            let mut shown = widest;
            if ui.add(DragValue::new(&mut shown).range(0.1..=200.0).speed(0.1).suffix(" px")).changed() {
                let factor = shown / widest;
                widths.iter_mut().for_each(|w| *w *= factor);
            }
            ui.end_row();
        }
        Shape::Path(_) | Shape::Image { .. } | Shape::Text(_) | Shape::Paint { .. } => {}
    });
}

/// Fill and stroke controls. Shared by selected shapes and the default
/// style for new shapes.
pub fn style_editor(ui: &mut egui::Ui, style: &mut Style, symbols: &[(inkframe_core::SymbolId, String)]) {
    use inkframe_core::{Dash, GradientShape, LineCap, LineJoin, ShapeGradient};
    egui::Grid::new("style").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        // Fill: none, a colour, or a gradient (see `ShapeGradient`).
        ui.label(tr("Fill"));
        ui.horizontal(|ui| {
            let mut on = style.fill.is_some() || style.gradient.is_some();
            if ui.checkbox(&mut on, "").changed() {
                style.fill = on.then_some(style.fill.unwrap_or(Color::rgb(0x3d, 0x8b, 0xfd)));
                if !on {
                    style.gradient = None;
                }
            }
            if !on {
                return;
            }
            let kind = style.gradient.as_ref().map(|g| g.shape);
            let label = match kind {
                None => tr("Solid"),
                Some(GradientShape::Linear) => tr("Linear"),
                Some(GradientShape::Radial) => tr("Radial"),
            };
            let base = style.fill.unwrap_or(Color::BLACK);
            egui::ComboBox::from_id_salt("fill_kind").selected_text(label).width(80.0).show_ui(ui, |ui| {
                if ui.selectable_label(kind.is_none(), tr("Solid")).clicked() {
                    style.gradient = None;
                }
                for (shape, name) in [(GradientShape::Linear, tr("Linear")), (GradientShape::Radial, tr("Radial"))] {
                    if ui.selectable_label(kind == Some(shape), name).clicked() && kind != Some(shape) {
                        // Switching between the two keeps the colours; from
                        // solid it fades the fill colour out to white.
                        let stops = style.gradient.take().map(|g| g.stops);
                        let mut gradient = match shape {
                            GradientShape::Linear => ShapeGradient::linear(base, Color::WHITE),
                            GradientShape::Radial => ShapeGradient::radial(base, Color::WHITE),
                        };
                        if let Some(stops) = stops {
                            gradient.stops = stops;
                        }
                        style.gradient = Some(gradient);
                    }
                }
            });
            if style.gradient.is_none()
                && let Some(fill) = &mut style.fill
            {
                color_button(ui, fill);
            }
        });
        ui.end_row();
        if let Some(gradient) = &mut style.gradient {
            ui.label("");
            ui.vertical(|ui| {
                gradient_stops(ui, gradient);
                ui.weak(tr("To move its ends, double-click the shape on the stage, then click it."));
            });
            ui.end_row();
            // What wants one colour (the bucket, palettes) goes by the first.
            style.fill = gradient.stops.first().map(|s| s.color);
        }

        ui.label(tr("Stroke"));
        ui.horizontal(|ui| {
            let mut on = style.stroke.is_some();
            if ui.checkbox(&mut on, "").changed() {
                style.stroke = on.then_some(style.stroke.unwrap_or(Stroke::new(Color::BLACK, 2.0)));
            }
            if let Some(stroke) = &mut style.stroke {
                color_button(ui, &mut stroke.color);
                ui.add(DragValue::new(&mut stroke.width).range(0.1..=200.0).speed(0.1).suffix(" px"));
            }
        });
        ui.end_row();
        let Some(stroke) = &mut style.stroke else { return };

        ui.label(tr("Ends"));
        let caps = [(LineCap::Round, tr("Round")), (LineCap::Butt, tr("Flat")), (LineCap::Square, tr("Square"))];
        choice(ui, "stroke_cap", &mut stroke.cap, &caps);
        ui.end_row();
        ui.label(tr("Corners"));
        let joins = [(LineJoin::Round, tr("Round")), (LineJoin::Miter, tr("Sharp")), (LineJoin::Bevel, tr("Bevelled"))];
        choice(ui, "stroke_join", &mut stroke.join, &joins);
        ui.end_row();

        ui.label(tr("Line"));
        ui.horizontal(|ui| {
            #[derive(Clone, Copy, PartialEq)]
            enum Kind {
                Solid,
                Dashed,
                Dotted,
            }
            let kind = match stroke.dash {
                None => Kind::Solid,
                Some(dash) if dash.is_dotted() => Kind::Dotted,
                Some(_) => Kind::Dashed,
            };
            let mut chosen = kind;
            choice(
                ui,
                "stroke_dash",
                &mut chosen,
                &[(Kind::Solid, tr("Solid")), (Kind::Dashed, tr("Dashed")), (Kind::Dotted, tr("Dotted"))],
            );
            if chosen != kind {
                stroke.dash = match chosen {
                    Kind::Solid => None,
                    Kind::Dashed => Some(Dash::dashed(stroke.width)),
                    Kind::Dotted => {
                        // Dots need round ends to be dots.
                        stroke.cap = LineCap::Round;
                        Some(Dash::dotted(stroke.width))
                    }
                };
            }
            if let Some(dash) = &mut stroke.dash {
                if !dash.is_dotted() {
                    ui.add(DragValue::new(&mut dash.length).range(0.5..=500.0).speed(0.2).prefix(format!("{} ", tr("dash"))));
                }
                ui.add(DragValue::new(&mut dash.gap).range(0.5..=500.0).speed(0.2).prefix(format!("{} ", tr("gap"))));
            }
        });
        ui.end_row();

        // Drawn as a symbol's picture laid along it (see `art_brush.rs`).
        ui.label(tr("Brush")).on_hover_text(
            tr("Draw the line as a symbol's picture: stretched along it (Art) or repeated (Pattern), as Animate's Paint Brush does"),
        );
        ui.horizontal(|ui| {
            use inkframe_core::{BrushKind, StrokeBrush};
            #[derive(Clone, Copy, PartialEq)]
            enum Kind {
                Plain,
                Art,
                Pattern,
            }
            let kind = match stroke.brush.map(|b| b.kind) {
                None => Kind::Plain,
                Some(BrushKind::Art) => Kind::Art,
                Some(BrushKind::Pattern) => Kind::Pattern,
            };
            let mut chosen = kind;
            ui.add_enabled_ui(!symbols.is_empty(), |ui| {
                choice(ui, "stroke_brush", &mut chosen, &[(Kind::Plain, tr("Plain")), (Kind::Art, tr("Art")), (Kind::Pattern, tr("Pattern"))]);
            });
            if chosen != kind {
                let symbol = stroke.brush.map(|b| b.symbol).or(symbols.first().map(|(id, _)| *id));
                stroke.brush = match (chosen, symbol) {
                    (Kind::Art, Some(symbol)) => Some(StrokeBrush { kind: BrushKind::Art, symbol }),
                    (Kind::Pattern, Some(symbol)) => Some(StrokeBrush { kind: BrushKind::Pattern, symbol }),
                    _ => None,
                };
            }
            if let Some(brush) = &mut stroke.brush {
                let name = symbols.iter().find(|(id, _)| *id == brush.symbol).map_or(tr("(gone)"), |(_, n)| n.as_str());
                egui::ComboBox::from_id_salt("stroke_brush_symbol").selected_text(name).width(110.0).show_ui(ui, |ui| {
                    for (id, name) in symbols {
                        ui.selectable_value(&mut brush.symbol, *id, name);
                    }
                });
            } else if symbols.is_empty() {
                ui.weak(tr("Make a symbol to brush with first."));
            }
        });
        ui.end_row();
    });
}

/// A drop-down to pick one of a few named values.
pub(super) fn choice<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, value: &mut T, options: &[(T, &str)]) {
    let current = options.iter().find(|(v, _)| v == value).map_or("", |(_, name)| *name);
    egui::ComboBox::from_id_salt(id).selected_text(current).width(90.0).show_ui(ui, |ui| {
        for &(option, name) in options {
            ui.selectable_value(value, option, name);
        }
    });
}

/// A gradient's colours: each stop's colour and place along it, with
/// buttons to add one, take one away, or turn the gradient round.
pub(super) fn gradient_stops(ui: &mut egui::Ui, gradient: &mut inkframe_core::ShapeGradient) {
    ui.vertical(|ui| {
        let mut remove = None;
        let count = gradient.stops.len();
        for (i, stop) in gradient.stops.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                color_button(ui, &mut stop.color);
                let mut percent = stop.at * 100.0;
                if ui.add(DragValue::new(&mut percent).range(0.0..=100.0).speed(0.5).suffix("%")).changed() {
                    stop.at = percent / 100.0;
                }
                // A gradient needs two colours to be one.
                if count > 2 && ui.small_button("×").on_hover_text(tr("Remove this colour")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            gradient.stops.remove(i);
        }
        ui.horizontal(|ui| {
            if ui
                .small_button(tr("+ Colour"))
                .on_hover_text(tr("Add a colour halfway along the widest gap"))
                .clicked()
            {
                let widest = gradient
                    .stops
                    .windows(2)
                    .enumerate()
                    .max_by(|a, b| (a.1[1].at - a.1[0].at).total_cmp(&(b.1[1].at - b.1[0].at)));
                if let Some((i, pair)) = widest {
                    let at = (pair[0].at + pair[1].at) / 2.0;
                    let color = gradient.color_at(at);
                    gradient.stops.insert(i + 1, inkframe_core::GradientStop { at, color });
                }
            }
            if ui.small_button(tr("Reverse")).on_hover_text(tr("Swap the start and end colours")).clicked() {
                for stop in &mut gradient.stops {
                    stop.at = 1.0 - stop.at;
                }
            }
        });
        gradient.tidy();
    });
}
