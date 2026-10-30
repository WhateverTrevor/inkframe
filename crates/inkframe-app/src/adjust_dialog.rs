//! The Adjustments window: Brightness/Contrast, Hue/Saturation, Levels and
//! Curves on the selected paint layer (see `inkframe_core::adjust`).
//!
//! The layer previews live: each change is applied afresh to the canvas
//! as it was when the window opened, so dragging a slider back undoes
//! itself. OK keeps the result (one undo step); Cancel or closing puts the
//! original back. A selection limits it, as it limits painting.
//!
//! The controls are shared with adjustment layers' settings in the
//! Properties panel ([`adjustment_controls`]).

use egui::{Color32, Pos2, Sense, Stroke, pos2, vec2};
use inkframe_core::{Adjustment, Canvas, CanvasId, Selection, adjust};

use crate::editor::Editor;
use crate::i18n::tr;

/// Which adjustment to open (a menu item).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustmentKind {
    BrightnessContrast,
    HueSaturation,
    Levels,
    Curves,
}

impl AdjustmentKind {
    pub const ALL: [AdjustmentKind; 4] = [
        AdjustmentKind::BrightnessContrast,
        AdjustmentKind::HueSaturation,
        AdjustmentKind::Levels,
        AdjustmentKind::Curves,
    ];

    /// The adjustment, set to change nothing.
    pub fn neutral(self) -> Adjustment {
        match self {
            AdjustmentKind::BrightnessContrast => Adjustment::brightness_contrast(),
            AdjustmentKind::HueSaturation => Adjustment::hue_saturation(),
            AdjustmentKind::Levels => Adjustment::levels(),
            AdjustmentKind::Curves => Adjustment::curves(),
        }
    }
}

#[derive(Default)]
pub struct AdjustDialog {
    open: Option<Open>,
}

struct Open {
    adjustment: Adjustment,
    canvas: CanvasId,
    original: Canvas,
    selection: Option<Selection>,
    /// The adjustment last shown, so the canvas is only redone on a change.
    shown: Adjustment,
}

impl AdjustDialog {
    /// Opens the window for the selected paint layer, if there is one.
    pub fn open(&mut self, editor: &Editor, kind: AdjustmentKind) {
        let adjustment = kind.neutral();
        let Some((_, canvas, ..)) = editor.paint_target() else { return };
        let Some(original) = editor.doc.canvas(canvas).cloned() else { return };
        let selection = editor.selection_on(canvas).cloned();
        self.open = Some(Open { shown: adjustment.clone(), adjustment, canvas, original, selection });
    }

    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        let Some(open) = &mut self.open else { return };
        let mut window_open = true;
        let mut finished = None;
        egui::Window::new(tr(open.adjustment.name()))
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                adjustment_controls(ui, &mut open.adjustment);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() {
                        finished = Some(true);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        finished = Some(false);
                    }
                    if ui.button(tr("Reset")).clicked() {
                        open.adjustment = neutral(&open.adjustment);
                    }
                });
            });
        if !window_open {
            finished = Some(false);
        }

        // Preview: redo the canvas from the original whenever a setting
        // changes.
        if open.adjustment != open.shown {
            let adjusted = adjust(&open.original, &open.adjustment, open.selection.as_ref());
            if let Some(canvas) = editor.doc.canvas_mut(open.canvas) {
                *canvas = adjusted;
            }
            open.shown = open.adjustment.clone();
        }
        if let Some(keep) = finished {
            self.finish(editor, keep);
        }
    }

    /// Closes the window, keeping the adjusted layer (OK) or putting the
    /// original back (Cancel).
    fn finish(&mut self, editor: &mut Editor, keep: bool) {
        let Some(open) = self.open.take() else { return };
        if !keep && let Some(canvas) = editor.doc.canvas_mut(open.canvas) {
            *canvas = open.original;
        }
    }
}

impl Editor {
    /// Modify › New Adjustment Layer: an adjustment of `kind`, changing
    /// nothing yet, as a layer above the selected one (or on top), picked
    /// so its settings show in Properties.
    pub fn add_adjustment_layer(&mut self, kind: AdjustmentKind) {
        let timeline = self.timeline();
        let name = self.doc.next_layer_name_in(timeline, tr(kind.neutral().name()));
        let Some(id) = self.doc.add_adjustment_layer(timeline, name, kind.neutral(), self.frame) else {
            return;
        };
        if let Some(selected) = self.selection
            && let Some(index) = self.doc.layers_of(timeline).iter().position(|l| l.id == selected)
        {
            self.doc.move_layer_to(id, index + 1);
        }
        self.select(Some(id));
    }
}

/// The same kind of adjustment, set to change nothing.
fn neutral(adjustment: &Adjustment) -> Adjustment {
    match adjustment {
        Adjustment::BrightnessContrast { .. } => Adjustment::brightness_contrast(),
        Adjustment::HueSaturation { .. } => Adjustment::hue_saturation(),
        Adjustment::Levels { .. } => Adjustment::levels(),
        Adjustment::Curves { .. } => Adjustment::curves(),
    }
}

/// The settings of an adjustment, to change in place.
pub fn adjustment_controls(ui: &mut egui::Ui, adjustment: &mut Adjustment) {
    egui::Grid::new("adjustment").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| match adjustment {
        Adjustment::BrightnessContrast { brightness, contrast } => {
            ui.label(tr("Brightness"));
            ui.add(percent(brightness));
            ui.end_row();
            ui.label(tr("Contrast"));
            ui.add(percent(contrast));
            ui.end_row();
        }
        Adjustment::HueSaturation { hue, saturation, lightness } => {
            ui.label(tr("Hue"));
            ui.add(egui::Slider::new(hue, -180.0..=180.0).suffix("°"));
            ui.end_row();
            ui.label(tr("Saturation"));
            ui.add(percent(saturation));
            ui.end_row();
            ui.label(tr("Lightness"));
            ui.add(percent(lightness));
            ui.end_row();
        }
        Adjustment::Levels { black, gamma, white, out_black, out_white } => {
            ui.label(tr("Input black"))
                .on_hover_text(tr("Everything this dark or darker becomes the darkest output"));
            ui.add(egui::Slider::new(black, 0..=254));
            ui.end_row();
            ui.label(tr("Midtones"))
                .on_hover_text(tr("Above 1 lightens the middle greys, below 1 darkens them"));
            ui.add(egui::Slider::new(gamma, 0.1..=9.99).logarithmic(true));
            ui.end_row();
            ui.label(tr("Input white"))
                .on_hover_text(tr("Everything this light or lighter becomes the lightest output"));
            ui.add(egui::Slider::new(white, 1..=255));
            ui.end_row();
            ui.label(tr("Output black"));
            ui.add(egui::Slider::new(out_black, 0..=255));
            ui.end_row();
            ui.label(tr("Output white"));
            ui.add(egui::Slider::new(out_white, 0..=255));
            ui.end_row();
        }
        Adjustment::Curves { .. } => {}
    });
    if matches!(adjustment, Adjustment::Curves { .. }) {
        curves_editor(ui, adjustment);
        ui.weak(tr("Click to add a point, drag to move it, right-click to remove it."));
    }
}

/// A slider for a −1 to 1 setting, shown as −100 to +100.
fn percent(value: &mut f32) -> egui::Slider<'_> {
    egui::Slider::new(value, -1.0..=1.0).custom_formatter(|v, _| format!("{:+.0}", v * 100.0))
}

/// A square graph of input (across) against output (up), with the curve
/// and its points.
fn curves_editor(ui: &mut egui::Ui, adjustment: &mut Adjustment) {
    const SIZE: f32 = 220.0;
    let (rect, response) = ui.allocate_exact_size(vec2(SIZE, SIZE), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let grid = Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4));
    for i in 1..4 {
        let t = i as f32 / 4.0;
        painter.vline(rect.left() + rect.width() * t, rect.y_range(), grid);
        painter.hline(rect.x_range(), rect.top() + rect.height() * t, grid);
    }
    painter.line_segment([rect.left_bottom(), rect.right_top()], grid);

    let to_screen =
        |(x, y): (f32, f32)| pos2(rect.left() + x * rect.width(), rect.bottom() - y * rect.height());
    let from_screen = |p: Pos2| {
        (
            ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
            ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0),
        )
    };

    let Adjustment::Curves { points } = adjustment else { return };
    let near = |points: &[(f32, f32)], p: Pos2| points.iter().position(|q| to_screen(*q).distance(p) <= 8.0);
    // The point being dragged, kept between frames.
    let dragging_id = response.id.with("dragging");
    let mut dragging: Option<usize> = ui.data(|d| d.get_temp(dragging_id));
    if response.drag_started()
        && let Some(p) = response.interact_pointer_pos()
    {
        dragging = near(points, p).or_else(|| {
            points.push(from_screen(p));
            Some(points.len() - 1)
        });
    }
    if let (Some(i), Some(p)) = (dragging, response.interact_pointer_pos())
        && response.dragged()
        && i < points.len()
    {
        points[i] = from_screen(p);
    }
    if response.drag_stopped() {
        dragging = None;
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    ui.data_mut(|d| d.insert_temp(dragging_id, dragging));
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
        && near(points, p).is_none()
    {
        points.push(from_screen(p));
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    // A curve needs two points, so the last two stay.
    if response.secondary_clicked()
        && let Some(i) = response.interact_pointer_pos().and_then(|p| near(points, p))
        && points.len() > 2
    {
        points.remove(i);
    }

    if let Some(table) = adjustment.table() {
        let curve: Vec<Pos2> =
            (0..256).map(|i| to_screen((i as f32 / 255.0, table[i] as f32 / 255.0))).collect();
        painter.add(egui::Shape::line(curve, Stroke::new(2.0, ui.visuals().text_color())));
    }
    if let Adjustment::Curves { points } = adjustment {
        for point in points {
            painter.circle(to_screen(*point), 4.0, Color32::WHITE, Stroke::new(1.0, Color32::BLACK));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Document, Point};

    #[test]
    fn an_adjustment_layer_goes_above_the_selected_layer_and_is_picked() {
        let mut editor = Editor::new(Document::default());
        let at = inkframe_core::Transform::at(Point::ZERO);
        let low = editor.doc.add_layer("Low", Vec::new(), 0, at);
        editor.doc.add_layer("High", Vec::new(), 0, at);
        editor.select(Some(low));
        editor.add_adjustment_layer(AdjustmentKind::Curves);
        let names: Vec<&str> = editor.doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Low", "Curves 1", "High"], "bottom first");
        let picked = editor.selected_layer().unwrap();
        assert_eq!(picked.adjustment, Some(Adjustment::curves()));
        assert!(!picked.draws_vectors(), "nothing is drawn into it");
    }

    #[test]
    fn cancelling_puts_the_original_back() {
        let mut editor = Editor::new(Document::default());
        editor.add_paint_layer();
        let (_, canvas, ..) = editor.paint_target().unwrap();
        editor.doc.canvas_mut(canvas).unwrap().write_region(0, 0, 1, 1, &[100, 100, 100, 255]);
        let selection = Selection::rectangle(1920, 1080, Point::new(0.0, 0.0), Point::new(10.0, 10.0));
        editor.pixel_selection = Some((canvas, selection));

        let mut dialog = AdjustDialog::default();
        dialog.open(&editor, AdjustmentKind::BrightnessContrast);
        let open = dialog.open.as_mut().unwrap();
        open.adjustment = Adjustment::BrightnessContrast { brightness: 0.5, contrast: 0.0 };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| dialog.show(ui.ctx(), &mut editor));
        // No renderer here to upload egui's font textures to.
        output.textures_delta.clear();
        assert!(editor.doc.canvas(canvas).unwrap().pixel(0, 0)[0] > 150, "previewed brighter");

        dialog.finish(&mut editor, false);
        assert!(!dialog.is_open());
        assert_eq!(editor.doc.canvas(canvas).unwrap().pixel(0, 0), [100, 100, 100, 255]);
    }
}
