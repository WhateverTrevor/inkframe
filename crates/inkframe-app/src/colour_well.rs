//! The colours the current tool uses, in one place: the well under the
//! toolbar, as Adobe's apps have it, and the Colour tab that edits them.
//!
//! What the well holds depends on the tool. The shape tools draw with a
//! **Fill** and a **Stroke**, as Illustrator's and Animate's do; the
//! painting tools paint with a **Colour** and keep a second one behind it
//! to swap with, as Photoshop's foreground and background colours; the
//! Pencil draws a line, so it has just its **Stroke**. One of them is
//! *active*, drawn in front: the Colour tab edits it (see `color_picker`).
//!
//! X makes the other one active (or, painting, swaps the two, as in
//! Photoshop), and Shift+X swaps the colours themselves, as in Illustrator.

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use inkframe_core::Color;

use crate::color_picker::paint_swatch;
use crate::editor::Editor;
use crate::i18n::tr;
use crate::icons::{Icon, IconButton};

/// One of the colours in the well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourChoice {
    /// New shapes' fill.
    Fill,
    /// New shapes' outline.
    Stroke,
    /// The Pencil's line.
    Line,
    /// The colour the painting tools paint with.
    Paint,
    /// The painting tools' second colour, behind the first.
    Second,
}

impl ColourChoice {
    pub fn label(self) -> &'static str {
        match self {
            ColourChoice::Fill => tr("Fill"),
            ColourChoice::Stroke | ColourChoice::Line => tr("Stroke"),
            ColourChoice::Paint => tr("Colour"),
            ColourChoice::Second => tr("Second colour"),
        }
    }

    /// Whether it can be no colour at all: a shape without a fill or an
    /// outline. A line or paint always has one.
    pub fn can_be_none(self) -> bool {
        matches!(self, ColourChoice::Fill | ColourChoice::Stroke)
    }
}

impl Editor {
    /// The colours the current tool uses, the first drawn in front when
    /// it's active.
    pub fn colour_choices(&self) -> &'static [ColourChoice] {
        if self.tool == crate::editor::Tool::Pencil {
            &[ColourChoice::Line]
        } else if self.tool.on_pixels() {
            &[ColourChoice::Paint, ColourChoice::Second]
        } else {
            &[ColourChoice::Fill, ColourChoice::Stroke]
        }
    }

    /// The colour the Colour tab edits: the active one of the current
    /// tool's.
    pub fn active_colour(&self) -> ColourChoice {
        let choices = self.colour_choices();
        let chosen =
            if choices.contains(&ColourChoice::Paint) { self.paint_choice } else { self.shape_choice };
        if choices.contains(&chosen) { chosen } else { choices[0] }
    }

    /// Makes `choice` the active colour, for the tools that use it.
    pub fn choose_colour(&mut self, choice: ColourChoice) {
        match choice {
            ColourChoice::Fill | ColourChoice::Stroke => self.shape_choice = choice,
            ColourChoice::Paint | ColourChoice::Second => self.paint_choice = choice,
            ColourChoice::Line => {}
        }
    }

    /// A colour of the well, or none for a shape drawn without it.
    pub fn colour(&self, choice: ColourChoice) -> Option<Color> {
        match choice {
            ColourChoice::Fill => self.draw_style.fill,
            ColourChoice::Stroke => self.draw_style.stroke.map(|s| s.color),
            ColourChoice::Line => Some(self.pencil_stroke.color),
            ColourChoice::Paint => Some(self.paint_color),
            ColourChoice::Second => Some(self.second_color),
        }
    }

    /// Sets a colour of the well. None turns a shape's fill or outline off
    /// (and is ignored for the rest, which always have one).
    pub fn set_colour(&mut self, choice: ColourChoice, color: Option<Color>) {
        match (choice, color) {
            (ColourChoice::Fill, color) => self.draw_style.fill = color,
            (ColourChoice::Stroke, color) => {
                // A new colour keeps the outline's width, ends and dashes.
                let previous = self.draw_style.stroke;
                self.draw_style.stroke = color.map(|color| inkframe_core::Stroke {
                    color,
                    ..previous.unwrap_or(inkframe_core::Stroke::new(color, 2.0))
                });
            }
            (ColourChoice::Line, Some(color)) => self.pencil_stroke.color = color,
            (ColourChoice::Paint, Some(color)) => self.paint_color = color,
            (ColourChoice::Second, Some(color)) => self.second_color = color,
            (_, None) => {}
        }
    }

    /// X: the other colour becomes the active one, as in Illustrator; or,
    /// painting, the two colours swap places, as in Photoshop.
    pub fn switch_colour(&mut self) {
        match self.colour_choices() {
            [ColourChoice::Paint, ColourChoice::Second] => self.swap_colours(),
            [first, second] => {
                let other = if self.active_colour() == *first { *second } else { *first };
                self.choose_colour(other);
            }
            _ => {}
        }
    }

    /// Shift+X (and the well's arrow): the two colours swap, a fill
    /// becoming the stroke and the stroke the fill.
    pub fn swap_colours(&mut self) {
        if let [first, second] = *self.colour_choices() {
            let (a, b) = (self.colour(first), self.colour(second));
            self.set_colour(first, b);
            self.set_colour(second, a);
        }
    }

    /// The well's default colours: a white fill with a black outline, as
    /// Illustrator's, or black paint over white, as Photoshop's.
    pub fn default_colours(&mut self) {
        match *self.colour_choices() {
            [ColourChoice::Paint, ColourChoice::Second] => {
                (self.paint_color, self.second_color) = (Color::BLACK, Color::WHITE);
            }
            [ColourChoice::Fill, ColourChoice::Stroke] => {
                self.set_colour(ColourChoice::Fill, Some(Color::WHITE));
                self.set_colour(ColourChoice::Stroke, Some(Color::BLACK));
            }
            _ => self.pencil_stroke.color = Color::BLACK,
        }
    }
}

/// How big a square of the well is, and how far the back one sits from
/// the front one.
const SQUARE: f32 = 26.0;
const OFFSET: f32 = 12.0;

/// The well, under the toolbar's tools: the current tool's colours, the
/// active one in front. Clicking one makes it the active one and brings up
/// the Colour tab to change it. Returns true when the Colour tab should be
/// shown.
pub fn show(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let choices = editor.colour_choices();
    let active = editor.active_colour();
    let mut reveal = false;
    let size = Vec2::splat(SQUARE + if choices.len() > 1 { OFFSET } else { 0.0 });
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    // The front square top left, the back one bottom right; the active
    // colour in front.
    let front = Rect::from_min_size(rect.min, Vec2::splat(SQUARE));
    let back = front.translate(Vec2::splat(OFFSET));
    let mut order: Vec<ColourChoice> = choices.to_vec();
    order.sort_by_key(|c| *c == active);
    for choice in order {
        let square = if choice == active || choices.len() == 1 { front } else { back };
        let id = ui.id().with(("colour well", choice as u8));
        // The back square's corner under the front one isn't its to click.
        let response = ui.interact(square, id, Sense::click());
        draw_square(ui.painter(), square, choice, editor.colour(choice));
        let tip = match choice {
            ColourChoice::Paint => {
                tr("Colour: what the painting tools paint with. X swaps it with the second colour")
            }
            ColourChoice::Second => {
                tr("Second colour: the gradient's other end, and the pattern stamp's light parts")
            }
            ColourChoice::Fill => tr("Fill: new shapes' inside. X switches to the stroke"),
            ColourChoice::Stroke | ColourChoice::Line => {
                tr("Stroke: new shapes' outline. X switches to the fill")
            }
        };
        if response.on_hover_text(tip).clicked() {
            editor.choose_colour(choice);
            reveal = true;
        }
    }

    // The well's buttons: swap, defaults and, for a shape's, none.
    if choices.len() > 1 {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if swap_button(ui).clicked() {
                editor.swap_colours();
            }
            if drawn_button(ui, draw_defaults).on_hover_text(tr("Default colours")).clicked() {
                editor.default_colours();
            }
            if active.can_be_none() {
                let none = editor.colour(active).is_none();
                if ui.add(IconButton::new(Icon::NoColor, tr("No colour")).selected(none)).clicked() && !none {
                    editor.set_colour(active, None);
                }
            }
        });
    }
    if editor.tool == crate::editor::Tool::Pencil {
        ui.add(egui::DragValue::new(&mut editor.pencil_stroke.width).range(0.5..=100.0).speed(0.1))
            .on_hover_text(tr("Line width"));
    }
    reveal
}

/// The well's swap button: Photoshop's bent arrow, a head at each end.
/// Also beside the Colour tab's two colours.
pub fn swap_button(ui: &mut egui::Ui) -> egui::Response {
    drawn_button(ui, draw_swap).on_hover_text(tr("Swap the two colours (Shift+X)"))
}

/// A small button showing a drawing rather than text: the fonts have no
/// signs like these, and a drawing stays crisp at any size.
fn drawn_button(ui: &mut egui::Ui, draw: fn(&egui::Painter, Rect, Color32)) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
    let visuals = ui.style().interact(&response);
    if response.hovered() {
        ui.painter().rect_filled(rect, 3.0, visuals.weak_bg_fill);
    }
    draw(ui.painter(), rect.shrink(3.0), visuals.fg_stroke.color);
    response
}

fn draw_swap(painter: &egui::Painter, r: Rect, color: Color32) {
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let line = Stroke::new(1.4, color);
    painter.add(egui::Shape::line(vec![at(0.05, 0.3), at(0.7, 0.3), at(0.7, 0.95)], line));
    painter.add(egui::Shape::convex_polygon(
        vec![at(0.0, 0.3), at(0.3, 0.05), at(0.3, 0.55)],
        color,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![at(0.7, 1.0), at(0.45, 0.7), at(0.95, 0.7)],
        color,
        Stroke::NONE,
    ));
}

/// Two small squares, dark over light: the colours as they start.
fn draw_defaults(painter: &egui::Painter, r: Rect, color: Color32) {
    let side = r.width() * 0.6;
    let back = Rect::from_min_size(r.max - Vec2::splat(side), Vec2::splat(side));
    let front = Rect::from_min_size(r.min, Vec2::splat(side));
    painter.rect_filled(back, 0.0, Color32::WHITE);
    painter.rect_stroke(back, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
    painter.rect_filled(front, 0.0, Color32::BLACK);
    painter.rect_stroke(front, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
}

/// One square of the well: the colour, a stroke drawn as a hollow square
/// (an outline's colour, as Adobe's apps show it), and no colour as
/// Animate's red slash.
fn draw_square(painter: &egui::Painter, square: Rect, choice: ColourChoice, color: Option<Color>) {
    let hollow = matches!(choice, ColourChoice::Stroke);
    let edge = Stroke::new(1.0, Color32::from_black_alpha(140));
    match color {
        Some(color) if hollow => {
            // A thick ring of the colour round a hole showing the panel.
            let ring = SQUARE * 0.28;
            let inner = square.shrink(ring);
            for band in [
                Rect::from_min_max(square.min, pos2(square.max.x, inner.min.y)),
                Rect::from_min_max(pos2(square.min.x, inner.max.y), square.max),
                Rect::from_min_max(pos2(square.min.x, inner.min.y), pos2(inner.min.x, inner.max.y)),
                Rect::from_min_max(pos2(inner.max.x, inner.min.y), pos2(square.max.x, inner.max.y)),
            ] {
                paint_swatch(painter, band, color);
            }
            painter.rect_stroke(inner, 0.0, edge, StrokeKind::Outside);
        }
        Some(color) => paint_swatch(painter, square, color),
        None => {
            painter.rect_filled(square, 2.0, Color32::WHITE);
            painter.line_segment(
                [square.left_bottom() + vec2(2.0, -2.0), square.right_top() + vec2(-2.0, 2.0)],
                Stroke::new(2.0, Color32::from_rgb(0xe0, 0x30, 0x30)),
            );
        }
    }
    painter.rect_stroke(square, 2.0, edge, StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Tool;
    use inkframe_core::Document;

    #[test]
    fn the_well_holds_what_the_tool_draws_with() {
        let mut editor = Editor::new(Document::default());
        editor.tool = Tool::Rectangle;
        assert_eq!(editor.colour_choices(), [ColourChoice::Fill, ColourChoice::Stroke]);
        editor.tool = Tool::Brush;
        assert_eq!(editor.colour_choices(), [ColourChoice::Paint, ColourChoice::Second]);
        assert_eq!(editor.active_colour(), ColourChoice::Paint);
        editor.tool = Tool::Pencil;
        assert_eq!(editor.active_colour(), ColourChoice::Line);
    }

    #[test]
    fn x_switches_the_shapes_colour_but_swaps_the_paints() {
        let mut editor = Editor::new(Document::default());
        editor.tool = Tool::Pen;
        editor.switch_colour();
        assert_eq!(editor.active_colour(), ColourChoice::Stroke);
        editor.switch_colour();
        assert_eq!(editor.active_colour(), ColourChoice::Fill);

        editor.tool = Tool::Brush;
        (editor.paint_color, editor.second_color) = (Color::BLACK, Color::WHITE);
        editor.switch_colour();
        assert_eq!((editor.paint_color, editor.second_color), (Color::WHITE, Color::BLACK));
        assert_eq!(editor.active_colour(), ColourChoice::Paint, "the front one stays the one painted with");
    }

    #[test]
    fn swapping_a_fill_and_no_stroke_keeps_the_strokes_width() {
        let mut editor = Editor::new(Document::default());
        editor.tool = Tool::Ellipse;
        editor.draw_style.stroke = Some(inkframe_core::Stroke::new(Color::BLACK, 7.0));
        editor.draw_style.fill = Some(Color::WHITE);
        editor.swap_colours();
        assert_eq!(editor.draw_style.fill, Some(Color::BLACK));
        assert_eq!(editor.draw_style.stroke.map(|s| (s.color, s.width)), Some((Color::WHITE, 7.0)));

        editor.set_colour(ColourChoice::Fill, None);
        editor.swap_colours();
        assert_eq!(editor.draw_style.stroke, None, "no fill made no stroke");
        assert_eq!(editor.draw_style.fill, Some(Color::WHITE));
    }
}
