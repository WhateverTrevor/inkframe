//! The selection tools on a paint layer: dragging out a rectangle, ellipse,
//! lasso or polygon, and drawing the marching outline of what's selected.

use egui::{Color32, PointerButton, Pos2};
use inkframe_core::{Combine, Point, Selection};

use super::{StageView, View, canvas_point, canvas_to_screen};
use crate::editor::{Editor, Tool};
use crate::tablet;

impl StageView {
    /// The selection tools. Each press's modifiers say how the new
    /// selection combines with the one there: Shift adds, Alt subtracts,
    /// both keep only the overlap.
    pub(super) fn handle_selecting(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        editor: &mut Editor,
        view: View,
    ) {
        let (pressed, down, double, enter, escape, modifiers) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.button_double_clicked(PointerButton::Primary),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.modifiers,
            )
        });
        let combine = match (modifiers.shift, modifiers.alt) {
            (true, true) => Combine::Intersect,
            (true, false) => Combine::Add,
            (false, true) => Combine::Subtract,
            (false, false) => Combine::Replace,
        };
        let press = ui
            .input(|i| i.pointer.press_origin())
            .filter(|_| pressed && response.is_pointer_button_down_on());
        let pointer = response.interact_pointer_pos().or(response.hover_pos());

        // The polygonal lasso carries on between clicks.
        if let Some(Selecting::Polygon { points, combine }) = &mut self.selecting {
            let closes = |p: Pos2| points.first().is_some_and(|first| first.distance(p) <= POLYGON_CLOSE);
            if escape {
                self.selecting = None;
            } else if enter || double || press.is_some_and(closes) {
                let (points, combine) = (std::mem::take(points), *combine);
                self.selecting = None;
                finish_selection(editor, view, combine, |w, h, to_canvas| {
                    Selection::polygon(w, h, &points.iter().map(|p| to_canvas(*p)).collect::<Vec<_>>())
                });
            } else if let Some(press) = press {
                points.push(press);
            }
            return;
        }

        if let Some(press) = press {
            if editor.paint_target().is_none() {
                editor.add_paint_layer();
            }
            match editor.tool {
                Tool::RectMarquee | Tool::EllipseMarquee => {
                    self.selecting = Some(Selecting::Marquee { start: press, current: press, combine });
                }
                Tool::Lasso => self.selecting = Some(Selecting::Lasso { points: vec![press], combine }),
                Tool::PolygonLasso => {
                    self.selecting = Some(Selecting::Polygon { points: vec![press], combine })
                }
                Tool::MagicWand => {
                    let (tolerance, contiguous) = (editor.wand_tolerance, editor.wand_contiguous);
                    let Some((_, canvas, ..)) = editor.paint_target() else { return };
                    let Some(pixels) = editor.doc.canvas(canvas).cloned() else { return };
                    finish_selection(editor, view, combine, |_, _, to_canvas| {
                        let at = to_canvas(press);
                        Selection::similar(
                            &pixels,
                            at.x.floor() as i64,
                            at.y.floor() as i64,
                            tolerance,
                            contiguous,
                        )
                    });
                }
                _ => {}
            }
            return;
        }

        match &mut self.selecting {
            Some(Selecting::Marquee { current, .. }) if down => {
                if let Some(pointer) = pointer {
                    *current = pointer;
                }
            }
            Some(Selecting::Lasso { points, .. }) if down => {
                points.extend(ui.input(tablet::samples).into_iter().map(|s| s.pos));
            }
            Some(Selecting::Marquee { start, current, combine }) => {
                let (start, current, combine) = (*start, *current, *combine);
                let ellipse = editor.tool == Tool::EllipseMarquee;
                self.selecting = None;
                // A click without a drag, as in Photoshop, just deselects.
                if start.distance(current) < 2.0 {
                    if combine == Combine::Replace {
                        editor.deselect_pixels();
                    }
                    return;
                }
                finish_selection(editor, view, combine, |w, h, to_canvas| {
                    let (a, b) = (to_canvas(start), to_canvas(current));
                    if ellipse { Selection::ellipse(w, h, a, b) } else { Selection::rectangle(w, h, a, b) }
                });
            }
            Some(Selecting::Lasso { points, combine }) => {
                let (points, combine) = (std::mem::take(points), *combine);
                self.selecting = None;
                finish_selection(editor, view, combine, |w, h, to_canvas| {
                    Selection::polygon(w, h, &points.iter().map(|p| to_canvas(*p)).collect::<Vec<_>>())
                });
            }
            _ => {}
        }
    }

    /// The selection being drawn, and the active selection's marching ants.
    pub(super) fn draw_selection(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        editor: &Editor,
        view: View,
    ) {
        let dark = (2.5, Color32::from_black_alpha(160));
        let light = (1.0, Color32::WHITE);
        match &self.selecting {
            Some(Selecting::Marquee { start, current, .. }) => {
                // Drawn in the canvas's own directions, which is how the
                // selection will lie, however the layer or view is turned.
                let Some((layer, _, w, h)) = editor.paint_target() else { return };
                let to_canvas = |p: Pos2| canvas_point(editor, view, layer, (w, h), p).unwrap_or(Point::ZERO);
                let (a, b) = (to_canvas(*start), to_canvas(*current));
                let (min, max) =
                    (Point::new(a.x.min(b.x), a.y.min(b.y)), Point::new(a.x.max(b.x), a.y.max(b.y)));
                let in_canvas: Vec<Point> = if editor.tool == Tool::EllipseMarquee {
                    let center = min.lerp(max, 0.5);
                    let radius = Point::new((max.x - min.x) / 2.0, (max.y - min.y) / 2.0);
                    (0..=48)
                        .map(|i| {
                            let t = i as f32 / 48.0 * std::f32::consts::TAU;
                            Point::new(center.x + t.cos() * radius.x, center.y + t.sin() * radius.y)
                        })
                        .collect()
                } else {
                    vec![min, Point::new(max.x, min.y), max, Point::new(min.x, max.y), min]
                };
                let outline: Vec<Pos2> =
                    in_canvas.into_iter().filter_map(|p| canvas_to_screen(editor, view, layer, p)).collect();
                for stroke in [dark, light] {
                    painter.add(egui::Shape::line(outline.clone(), stroke));
                }
            }
            Some(Selecting::Lasso { points, .. } | Selecting::Polygon { points, .. }) => {
                let mut line = points.clone();
                if matches!(self.selecting, Some(Selecting::Polygon { .. }))
                    && let Some(hover) = ui.input(|i| i.pointer.hover_pos())
                {
                    line.push(hover);
                }
                for stroke in [dark, light] {
                    painter.add(egui::Shape::line(line.clone(), stroke));
                }
                if let (Some(Selecting::Polygon { .. }), Some(first)) = (&self.selecting, points.first()) {
                    painter.circle_stroke(*first, POLYGON_CLOSE, light);
                }
            }
            None => {}
        }

        // Marching ants around the active selection.
        let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
        let Some(selection) = editor.selection_on(canvas) else { return };
        if self.ants.as_ref().is_none_or(|(id, _)| *id != selection.identity()) {
            self.ants = Some((selection.identity(), selection.outline()));
        }
        let Some(layer) = editor.doc.layer(layer) else { return };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let half = Point::new(width as f32 / 2.0, height as f32 / 2.0);
        let screen = |p: Point| view.to_screen(to_stage.apply(p - half));
        // The dashes march along every tenth of a second.
        let time = ui.input(|i| i.time);
        let phase = (time * 10.0) as usize;
        let edges = &self.ants.as_ref().expect("just made").1;
        for &(a, b) in edges {
            let (a, b) = (screen(a), screen(b));
            painter.line_segment([a, b], (1.0, Color32::BLACK));
            // White dashes 4 points long.
            let length = a.distance(b);
            let mut along = ((a.x + a.y) as usize + phase) as f32 % 8.0 - 8.0;
            while along < length {
                let (from, to) = (along.max(0.0), (along + 4.0).min(length));
                if to > from {
                    painter.line_segment([a + (b - a) * (from / length), a + (b - a) * (to / length)], light);
                }
                along += 8.0;
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// A selection being drawn, in screen points, and how it'll combine with
/// the selection already there.
pub(super) enum Selecting {
    /// A rectangle or ellipse, dragged from `start`.
    Marquee { start: Pos2, current: Pos2, combine: Combine },
    /// The freehand lasso's path, while the button is held.
    Lasso { points: Vec<Pos2>, combine: Combine },
    /// The polygonal lasso's corners, one per click.
    Polygon { points: Vec<Pos2>, combine: Combine },
}

/// How close to the polygonal lasso's first corner a click closes it, in
/// screen points.
pub(super) const POLYGON_CLOSE: f32 = 8.0;

/// Makes a selection on the selected paint layer's canvas and combines it
/// with the one there. `make` gets the canvas size and a way to turn screen
/// points into canvas pixels.
pub(super) fn finish_selection(
    editor: &mut Editor,
    view: View,
    combine: Combine,
    make: impl FnOnce(u32, u32, &dyn Fn(Pos2) -> Point) -> Selection,
) {
    let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
    let to_canvas = |p: Pos2| canvas_point(editor, view, layer, (width, height), p).unwrap_or(Point::ZERO);
    let new = make(width, height, &to_canvas);
    let combined = match editor.selection_on(canvas) {
        Some(existing) => existing.combine(&new, combine),
        None if matches!(combine, Combine::Subtract | Combine::Intersect) => return,
        None => new,
    };
    editor.pixel_selection = (!combined.is_empty()).then_some((canvas, combined));
}
