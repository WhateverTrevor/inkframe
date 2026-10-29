//! What the stage draws around and over the artwork that isn't part of it:
//! the backdrop and shadow, guides, emitters, the camera frame, onion skins,
//! a layer's registration point and the hint on an empty stage.

use egui::{Align2, Color32, Pos2, Rect, Vec2, vec2};
use inkframe_core::{Affine, Point};
use inkframe_render::{RenderOptions, render_timeline_into};

use super::{GhostCache, ONION_AFTER, ONION_BEFORE, View, paint_coverage};
use crate::editor::{Editor, OnionSkin};
use crate::guides;
use crate::i18n::{tr, tr_args};

/// The safe areas and field guide, if they're on, over the stage (the
/// camera's frame).
pub(super) fn draw_guides(painter: &egui::Painter, editor: &Editor, view: View) {
    const SAFE: Color32 = Color32::from_rgb(0x4c, 0xc9, 0xf0);
    const FIELD: Color32 = Color32::from_rgb(0x5a, 0x78, 0xc8);
    let frame = editor.doc.stage_rect();
    let at = |p: Point| view.stage_to_screen(p);

    if editor.guides.field_guide {
        let (lines, labels) = guides::field_guide(frame);
        for line in &lines {
            let (width, alpha) = match line.weight {
                guides::Weight::Faint => (1.0, 0.14),
                guides::Weight::Normal => (1.0, 0.45),
                guides::Weight::Strong => (1.5, 0.8),
            };
            painter.line_segment([at(line.from), at(line.to)], (width, FIELD.gamma_multiply(alpha)));
        }
        // The small fields' labels crowd together when zoomed out; each is
        // shown only if there's room before the next one out.
        for (i, label) in labels.iter().enumerate() {
            let next = labels.get(i + 1).filter(|next| next.text.ends_with('F'));
            if next.is_some_and(|next| at(label.at).distance(at(next.at)) < 18.0) {
                continue;
            }
            let anchor = at(label.at) + label.anchor.to_sign() * -3.0;
            guide_label(painter, anchor, label.anchor, &label.text, FIELD);
        }
    }

    // The two safe areas are only a few pixels apart when zoomed out, so
    // their names go in opposite corners: action safe top right, title
    // safe bottom left (clear of the field guide's labels, top left).
    let safe = guides::safe_areas(frame, editor.guides.safe_areas);
    for (i, (rect, name)) in safe.iter().enumerate() {
        let corners = rect.corners().map(at);
        painter.add(egui::Shape::dashed_line(
            &[corners.as_slice(), &[corners[0]]].concat(),
            (1.0, SAFE),
            6.0,
            4.0,
        ));
        let (corner, anchor) =
            if i == 0 { (corners[1], Align2::RIGHT_TOP) } else { (corners[3], Align2::LEFT_BOTTOM) };
        guide_label(painter, corner + anchor.to_sign() * -4.0, anchor, name, SAFE);
    }
}

/// A guide's label, on a soft backing so it reads over the artwork.
pub(super) fn guide_label(painter: &egui::Painter, at: Pos2, anchor: Align2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), color);
    let rect = anchor.anchor_size(at, galley.size());
    painter.rect_filled(rect.expand(2.0), 3.0, Color32::from_white_alpha(170));
    painter.galley(rect.min, galley, color);
}

/// The selected particle layers' emitters: the shape particles start
/// from, and an arrow the way they head.
pub(super) fn draw_emitters(painter: &egui::Painter, editor: &Editor, view: View) {
    use inkframe_core::EmitterShape;
    const EMITTER: Color32 = Color32::from_rgb(0xff, 0xa0, 0x3c);
    for layer in editor.selected_ids().iter().filter_map(|id| editor.doc.layer(*id)) {
        let Some(emitter) = &layer.particles else { continue };
        let to_stage = editor.doc.layer_matrix(layer, editor.frame);
        let at = |x: f32, y: f32| view.to_screen(to_stage.apply(Point::new(x, y)));
        let outline: Vec<Pos2> = match emitter.shape {
            EmitterShape::Point => Vec::new(),
            EmitterShape::Line { length } => vec![at(-length / 2.0, 0.0), at(length / 2.0, 0.0)],
            EmitterShape::Rect { width, height } => {
                let (w, h) = (width / 2.0, height / 2.0);
                vec![at(-w, -h), at(w, -h), at(w, h), at(-w, h), at(-w, -h)]
            }
            EmitterShape::Circle { radius } => (0..=48)
                .map(|i| {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    at(radius * a.cos(), radius * a.sin())
                })
                .collect(),
        };
        if outline.len() > 1 {
            painter.add(egui::Shape::dashed_line(&outline, (1.5, EMITTER), 6.0, 4.0));
        }
        let centre = at(0.0, 0.0);
        painter.circle_stroke(centre, 5.0, (1.5, EMITTER));
        // The heading, turned with the layer as the particles are.
        let angle = view.screen_angle(emitter.direction.to_radians() + to_stage.b.atan2(to_stage.a));
        let tip = centre + vec2(angle.cos(), angle.sin()) * 40.0;
        painter.arrow(centre, tip - centre, (1.5, EMITTER));
    }
}

/// The Camera tool's view of things: the stage's edge, which is the
/// camera's frame, and how far it's zoomed and turned.
pub(super) fn draw_camera_frame(painter: &egui::Painter, editor: &Editor, view: View) {
    const CAMERA: Color32 = Color32::from_rgb(0xff, 0xa0, 0x3c);
    let corners = editor.doc.stage_rect().corners().map(|c| view.to_screen(c));
    painter.add(egui::Shape::closed_line(corners.to_vec(), (2.0, CAMERA)));
    let label = match editor.doc.camera_layer() {
        Some(camera) if !camera.visible => tr("Camera off: show its layer to use it").to_owned(),
        Some(camera) => {
            let pose = editor.doc.pose(camera, editor.frame);
            tr_args(
                "Camera  {zoom}%  {turn}°",
                &[
                    ("zoom", &format!("{:.0}", pose.scale.x * 100.0)),
                    ("turn", &format!("{:.0}", pose.rotation)),
                ],
            )
        }
        None => tr("Drag to add a camera and pan it; Shift turns, Alt zooms").to_owned(),
    };
    let at = corners[0] + vec2(6.0, 6.0);
    let galley = painter.layout_no_wrap(label, egui::FontId::proportional(12.0), Color32::BLACK);
    let back = Rect::from_min_size(at, galley.size()).expand(4.0);
    painter.rect_filled(back, 3.0, CAMERA);
    painter.galley(at, galley, Color32::BLACK);
}

/// Draws faint, tinted copies of neighbouring frames: blue for the past,
/// green for the future (the same convention as most 2D animation tools).
/// Nearer frames are stronger so the direction of motion reads clearly.
///
/// A ghost is all one colour, so only where a frame covers the stage, and
/// how solidly, matters: that's drawn once per frame and kept in `ghosts`,
/// so stepping through the animation draws just the frames newly in reach.
pub(super) fn draw_onion_skins(
    pixmap: &mut tiny_skia::Pixmap,
    editor: &Editor,
    onion: OnionSkin,
    scale: f32,
    ghosts: &mut GhostCache,
) {
    let (doc, frame, timeline, to_stage) =
        (&editor.doc, editor.frame, editor.timeline(), editor.timeline_transform());
    let (width, height) = (pixmap.width(), pixmap.height());
    let options = RenderOptions {
        scale,
        draw_background: false,
        images: Some(&editor.images),
        // A guide doesn't move with the animation, so ghosts of it would
        // only clutter.
        show_guides: false,
        show_mask: None,
        light_table: None,
        tint: None,
        opacity: 1.0,
    };
    // What this frame covers itself. A ghost only shows where its frame
    // differs from this one: a drawing that stays put would otherwise sit
    // under itself, and its soft edges would let a tinted copy show through
    // as a coloured rim.
    let current: Vec<u8> = match tiny_skia::Pixmap::new(width, height) {
        Some(mut alone) => {
            render_timeline_into(&mut alone, doc, timeline, frame, to_stage, None, &options);
            alone.pixels().iter().map(|p| p.alpha()).collect()
        }
        None => Vec::new(),
    };
    let mut wanted = Vec::new();
    // Farthest first, so nearer ghosts paint on top.
    for distance in (1..=onion.before).rev() {
        if let Some(f) = frame.checked_sub(distance) {
            wanted.push((f, distance, onion.before, ONION_BEFORE));
        }
    }
    for distance in (1..=onion.after).rev() {
        let f = frame + distance;
        if f <= editor.last_frame() {
            wanted.push((f, distance, onion.after, ONION_AFTER));
        }
    }
    // Frames out of reach go, so the cache stays the size of the ghosts.
    ghosts.frames.retain(|f, _| wanted.iter().any(|(w, ..)| w == f));
    for (f, distance, count, tint) in wanted {
        let coverage = ghosts.frames.entry(f).or_insert_with(|| {
            let Some(mut alone) = tiny_skia::Pixmap::new(width, height) else { return Vec::new() };
            render_timeline_into(&mut alone, doc, timeline, f, to_stage, None, &options);
            alone.pixels().iter().map(|p| p.alpha()).collect()
        });
        let opacity = 0.45 * (count - distance + 1) as f32 / count as f32;
        let differs: Vec<u8> =
            coverage.iter().zip(&current).map(|(&ghost, &now)| ghost.saturating_sub(now)).collect();
        paint_coverage(pixmap, &differs, tint, opacity);
    }
}

/// A small crosshair marking a symbol's registration point: the origin its
/// instances rotate and scale around.
pub(super) fn draw_registration_point(painter: &egui::Painter, at: Pos2) {
    let stroke = (1.5, Color32::from_gray(40));
    painter.line_segment([at - vec2(8.0, 0.0), at + vec2(8.0, 0.0)], stroke);
    painter.line_segment([at - vec2(0.0, 8.0), at + vec2(0.0, 8.0)], stroke);
    painter.circle_stroke(at, 4.0, stroke);
}

/// The classic grey checkerboard that signals "transparent here".
/// The grey and white squares that show the stage is transparent, turned
/// with the view. Squares are about 10 screen points, but bigger when
/// zoomed far in, so there are never too many to draw.
pub(super) fn draw_checkerboard(painter: &egui::Painter, view: View) {
    let corners = view.stage_corners();
    painter.add(egui::Shape::convex_polygon(corners.to_vec(), Color32::from_gray(204), egui::Stroke::NONE));
    let mut cell = 10.0 / view.stage_zoom;
    while (view.size.x / cell) * (view.size.y / cell) > 20_000.0 {
        cell *= 2.0;
    }
    let stage = View { timeline_to_stage: Affine::IDENTITY, ..view };
    let clip = painter.clip_rect();
    let (columns, rows) = ((view.size.x / cell).ceil() as i32, (view.size.y / cell).ceil() as i32);
    for row in 0..rows {
        for col in 0..columns {
            if (row + col) % 2 == 0 {
                continue;
            }
            let (x0, y0) = (col as f32 * cell, row as f32 * cell);
            let (x1, y1) = ((x0 + cell).min(view.size.x), (y0 + cell).min(view.size.y));
            let square: Vec<Pos2> = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                .iter()
                .map(|&(x, y)| stage.to_screen(Point::new(x, y)))
                .collect();
            if square.iter().any(|p| clip.contains(*p)) {
                painter.add(egui::Shape::convex_polygon(square, Color32::from_gray(153), egui::Stroke::NONE));
            }
        }
    }
}

/// A soft shadow under the stage, so the page seems to sit above the
/// pasteboard: a few widening, fading copies of its outline, nudged down.
/// Built from the corners rather than a rectangle so it turns with the view.
pub(super) fn draw_stage_shadow(painter: &egui::Painter, corners: &[Pos2; 4]) {
    let center = corners.iter().fold(Vec2::ZERO, |sum, c| sum + c.to_vec2()) / 4.0;
    for step in (1..=6).rev() {
        let spread = step as f32 * 2.5;
        let grown: Vec<Pos2> = corners
            .iter()
            .map(|&c| c + (c.to_vec2() - center).normalized() * spread + vec2(0.0, 3.0))
            .collect();
        let alpha = 34 - step * 5;
        painter.add(egui::Shape::convex_polygon(
            grown,
            Color32::from_black_alpha(alpha as u8),
            egui::Stroke::NONE,
        ));
    }
}

/// A new, empty project says how to start, in the middle of the stage, in
/// a grey that reads on light and dark backgrounds alike.
pub(super) fn draw_empty_stage_hint(
    painter: &egui::Painter,
    corners: &[Pos2; 4],
    background: inkframe_core::Color,
) {
    let center = corners.iter().fold(Vec2::ZERO, |sum, c| sum + c.to_vec2()) / 4.0;
    let center = center.to_pos2();
    let light = background.r as u32 + background.g as u32 + background.b as u32 > 3 * 128;
    let color = if light { Color32::from_black_alpha(110) } else { Color32::from_white_alpha(110) };
    painter.text(
        center - vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        tr("Pick a drawing tool and draw to start"),
        egui::FontId::proportional(16.0),
        color,
    );
    painter.text(
        center + vec2(0.0, 14.0),
        Align2::CENTER_CENTER,
        tr("Or press + in the timeline for an empty layer, or try File › Open Example…"),
        egui::FontId::proportional(12.0),
        color,
    );
}
