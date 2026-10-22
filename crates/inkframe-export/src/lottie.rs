//! Lottie export: the animation as a Lottie JSON file, which plays as
//! vectors on web pages (lottie-web) and in iOS and Android apps.
//!
//! Lottie and Inkframe build a picture differently, so rather than
//! translating each feature (rigs, the camera, depth, motion guides, IK),
//! every layer's placement is sampled on every frame, exactly as the
//! renderer places it, and written as hold keys. The file then plays back
//! frame for frame what Inkframe shows. In detail:
//!
//! * Each layer becomes a *null* layer (invisible) carrying its transform,
//!   parented as in the rig. What it shows hangs off that null, so drawing
//!   swaps and shape tweens don't disturb the rig.
//! * What it shows is one shape layer for each run of frames that look the
//!   same: a drawing held for twelve frames is one layer; a shape tween is
//!   a layer per frame.
//! * Pictures and paintings are image layers, the pixels embedded as PNG.
//! * A symbol becomes a precomposition, played through by a time remap so
//!   loops, Play Once and Single Frame all come out right.
//! * A mask becomes a track matte: a copy of its drawing just above each
//!   layer it masks.
//! * What Lottie can't draw as vectors (a layer's filters and colour
//!   effects, particles) is embedded as that layer rendered, frame by frame.

use std::collections::HashMap;
use std::ops::ControlFlow;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use inkframe_core::{
    Affine, BlendMode, Color, Document, Element, GradientShape, Layer, LayerId, LineCap, LineJoin, Shape,
    SymbolId, TimelineId, Transform,
};
use inkframe_render::ImageLibrary;
use serde_json::{Value, json};

use crate::{ExportError, ExportSettings, Progress};

pub(crate) fn write(
    doc: &Document,
    images: &ImageLibrary,
    settings: &ExportSettings,
    mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    let first = *settings.frames.start();
    let count = settings.frames.end() - first + 1;
    let mut cancelled = false;
    let lottie = animation(doc, images, first, count, settings.scale, settings.transparent, &mut |done| {
        cancelled |= on_progress(Progress { frames_done: done, frames_total: count }).is_break();
        cancelled
    });
    if cancelled {
        return Err(ExportError::Cancelled);
    }
    std::fs::write(&settings.output, serde_json::to_vec(&lottie).expect("JSON values always serialize"))?;
    Ok(())
}

/// The whole Lottie document for frames `first` to `first + count - 1`.
/// `progress` hears how many top-level layers are done, out of `count`
/// (scaled), and returning true stops early.
pub fn animation(
    doc: &Document,
    images: &ImageLibrary,
    first: u32,
    count: u32,
    scale: f32,
    transparent: bool,
    progress: &mut dyn FnMut(u32) -> bool,
) -> Value {
    let (width, height) = inkframe_render::output_size(doc, scale);
    let mut lottie = Lottie {
        doc,
        images,
        scale,
        assets: Vec::new(),
        images_done: HashMap::new(),
        symbols_done: HashMap::new(),
    };
    let frames: Vec<u32> = (first..first + count).collect();
    let mut layers = lottie.comp(TimelineId::Main, &frames, 0, progress);
    // Lottie has no stage colour of its own: unless it's to be clear, a
    // rectangle of it goes underneath everything.
    if !transparent {
        let stage = Element {
            shape: Shape::Rect { width: doc.width as f32, height: doc.height as f32, corner_radius: 0.0 },
            style: inkframe_core::Style { fill: Some(doc.background), stroke: None, gradient: None },
        };
        let centre =
            Affine::translate(inkframe_core::Point::new(doc.width as f32 / 2.0, doc.height as f32 / 2.0));
        layers.push(json!({
            "ty": 4, "ind": 999_999, "nm": "Background", "ip": 0, "op": count, "st": 0, "sr": 1,
            "ks": static_transform(centre), "shapes": shape_group(&stage).into_iter().collect::<Vec<_>>(),
        }));
    }
    // The export's size is the stage's times `scale`: everything hangs off
    // one null that scales it.
    if (scale - 1.0).abs() > f32::EPSILON {
        let root = 1_000_000;
        for layer in &mut layers {
            if layer.get("parent").is_none() {
                layer["parent"] = json!(root);
            }
        }
        layers.push(json!({
            "ty": 3, "ind": root, "nm": "Scale", "ip": 0, "op": count, "st": 0, "sr": 1,
            "ks": static_transform(Affine::scale(scale, scale)),
        }));
    }
    json!({
        "v": "5.7.4",
        "fr": doc.frame_rate.fps(),
        "ip": 0,
        "op": count,
        "w": width,
        "h": height,
        "nm": "Inkframe",
        "ddd": 0,
        "assets": lottie.assets,
        "layers": layers,
    })
}

struct Lottie<'a> {
    doc: &'a Document,
    images: &'a ImageLibrary,
    /// The export's size over the stage's, for the layers drawn as pixels.
    scale: f32,
    assets: Vec<Value>,
    /// Pictures already embedded, by what they are, so each goes in once.
    images_done: HashMap<String, String>,
    /// Symbols already made into precompositions: asset id, and where the
    /// symbol's origin is in it with its size (see [`Lottie::symbol`]).
    symbols_done: HashMap<SymbolId, Precomp>,
}

/// A symbol's precomposition. Lottie clips one to its box from (0, 0), and
/// a symbol draws around its registration point, partly at negative
/// coordinates, so everything in it is moved by `origin` to fit the box.
#[derive(Clone)]
struct Precomp {
    id: String,
    origin: [f32; 2],
    size: (u32, u32),
}

/// Lottie's layer indices: each Inkframe layer's null is its own id, and
/// what it shows counts on from a high number.
fn null_index(id: LayerId) -> i64 {
    number(id)
}

/// An id's number, as it's saved in the project file.
fn number(id: impl serde::Serialize) -> i64 {
    serde_json::to_value(id).ok().and_then(|v| v.as_i64()).unwrap_or(0)
}

impl Lottie<'_> {
    /// The layers of one timeline, top first as Lottie lists them.
    /// `frames[t]` is the timeline's frame shown at the composition's
    /// frame `t`.
    fn comp(
        &mut self,
        timeline: TimelineId,
        frames: &[u32],
        depth: usize,
        progress: &mut dyn FnMut(u32) -> bool,
    ) -> Vec<Value> {
        let doc = self.doc;
        let source = doc.layers_of(timeline);
        let mut out: Vec<Value> = Vec::new();
        let mut next = ContentIndex(2_000_000);
        // Bottom first, as stored; reversed at the end.
        for (n, layer) in source.iter().enumerate() {
            if depth == 0 && progress((n as u32 + 1) * frames.len() as u32 / source.len().max(1) as u32) {
                break;
            }
            if layer.guide || layer.folder || layer.camera {
                continue;
            }
            let exported_parent = layer.parent.filter(|p| source.iter().any(|l| l.id == *p && !l.folder));
            out.push(self.null(layer, exported_parent, frames));
            if layer.mask {
                continue; // A mask's drawing goes in as mattes, below.
            }
            for content in self.content(layer, frames, depth, &mut next) {
                // A masked layer gets a matte: a copy of the mask's drawing
                // just above it (pushed after, since this list is bottom
                // first until the end).
                let masked = frames.iter().any(|&f| doc.mask_for(layer, f).is_some());
                let mut content = content;
                if masked && let Some(mask) = frames.iter().find_map(|&f| doc.mask_for(layer, f)) {
                    content["tt"] = json!(1);
                    out.push(content.clone());
                    let mut matte = self.matte(mask, &content, frames, &mut next);
                    matte["td"] = json!(1);
                    out.push(matte);
                } else {
                    out.push(content);
                }
            }
        }
        out.reverse();
        out
    }

    /// A layer's null: its placement relative to its parent (or, at the top
    /// of a rig, on the stage, the camera included), every frame.
    fn null(&self, layer: &Layer, parent: Option<LayerId>, frames: &[u32]) -> Value {
        let doc = self.doc;
        let local = |f: u32| {
            let own = doc.layer_matrix(layer, f);
            match parent.and_then(|p| doc.layer(p)) {
                Some(p) => doc.layer_matrix(p, f).inverse().unwrap_or(Affine::IDENTITY).pre_concat(own),
                None => own,
            }
        };
        let mut null = json!({
            "ty": 3, "ind": null_index(layer.id), "nm": layer.name, "ip": 0, "op": frames.len(), "st": 0, "sr": 1,
            "ks": transform(frames, local),
        });
        if let Some(parent) = parent {
            null["parent"] = json!(null_index(parent));
        }
        null
    }

    /// What a layer shows, as Lottie layers hanging off its null, bottom
    /// first.
    fn content(
        &mut self,
        layer: &Layer,
        frames: &[u32],
        depth: usize,
        next: &mut ContentIndex,
    ) -> Vec<Value> {
        let doc = self.doc;
        let shown = |t: usize| layer.visible && layer.is_shown_at(frames[t]);
        let opacity = |t: usize| {
            let f = frames[t];
            (doc.pose(layer, f).opacity * doc.camera_fade(layer, f as f32)).clamp(0.0, 1.0) * 100.0
        };

        // Filters, colour effects and particles: drawn by the renderer.
        let rendered =
            layer.particles.is_some() || frames.iter().any(|&f| !layer.track.effects_at(f as f32).is_none());
        if rendered {
            return self.rendered(layer, frames, next);
        }

        let mut out = Vec::new();
        // A symbol: a precomposition, time-remapped to the symbol's frame.
        // A swap to another symbol partway starts another one.
        if layer.instance.is_some() {
            let fps = doc.frame_rate.fps();
            let remap: Vec<Value> = (0..frames.len())
                .map(|t| {
                    let symbol_frame = doc.instance_view(layer, frames[t], depth).map_or(0, |(_, f)| f);
                    json!(symbol_frame as f64 / fps)
                })
                .collect();
            let mut t = 0;
            while t < frames.len() {
                let symbol = layer.symbol_at(frames[t]).filter(|&s| doc.symbol(s).is_some());
                let Some(symbol) = symbol.filter(|_| shown(t)) else {
                    t += 1;
                    continue;
                };
                let mut end = t + 1;
                while end < frames.len() && shown(end) && layer.symbol_at(frames[end]) == Some(symbol) {
                    end += 1;
                }
                let precomp_asset = self.symbol(symbol, depth);
                let mut precomp = self.content_layer(0, layer, next, t, end, &opacity, frames.len());
                precomp["refId"] = json!(precomp_asset.id);
                precomp["w"] = json!(precomp_asset.size.0);
                precomp["h"] = json!(precomp_asset.size.1);
                // The anchor undoes the move into the box, so the symbol's
                // origin lands on the layer's.
                precomp["ks"]["a"] = still(precomp_asset.origin);
                precomp["tm"] = held(&remap);
                out.push(precomp);
                t = end;
            }
            return out;
        }

        // Runs of frames showing the same thing: one layer each.
        let mut t = 0;
        while t < frames.len() {
            if !shown(t) {
                t += 1;
                continue;
            }
            let elements = self.doc.posed_elements(layer, frames[t]).into_owned();
            let mut end = t + 1;
            while end < frames.len()
                && shown(end)
                && *self.doc.posed_elements(layer, frames[end]) == elements[..]
            {
                end += 1;
            }
            out.extend(self.drawing(layer, &elements, t, end, &opacity, frames.len(), next));
            t = end;
        }
        out
    }

    /// A Lottie layer of type `ty` hanging off `layer`'s null, showing from
    /// composition frame `ip` to before `op`, with its opacity and blend.
    #[allow(clippy::too_many_arguments)]
    fn content_layer(
        &self,
        ty: u8,
        layer: &Layer,
        next: &mut ContentIndex,
        ip: usize,
        op: usize,
        opacity: &dyn Fn(usize) -> f32,
        length: usize,
    ) -> Value {
        let values: Vec<Value> = (0..length).map(|t| json!(opacity(t))).collect();
        json!({
            "ty": ty, "ind": next.take(), "nm": layer.name, "parent": null_index(layer.id),
            "ip": ip, "op": op, "st": 0, "sr": 1, "bm": blend(layer.blend),
            "ks": {
                "a": still([0.0, 0.0]), "p": still([0.0, 0.0]), "s": still([100.0, 100.0]),
                "r": still(0.0), "o": held(&values),
            },
        })
    }

    /// One run of a drawing: its shapes on shape layers and its pictures
    /// and paintings on image layers, in the drawing's order, bottom first.
    #[allow(clippy::too_many_arguments)]
    fn drawing(
        &mut self,
        layer: &Layer,
        elements: &[Element],
        ip: usize,
        op: usize,
        opacity: &dyn Fn(usize) -> f32,
        length: usize,
        next: &mut ContentIndex,
    ) -> Vec<Value> {
        let mut out = Vec::new();
        // Shapes in a row share a shape layer; a picture between them
        // starts another, so the stacking stays as drawn.
        let mut shapes: Vec<Value> = Vec::new();
        for element in elements {
            let picture = match &element.shape {
                Shape::Image { image, width, height } => self.images.full_size(*image).map(|pixels| {
                    let id = self.image_asset(format!("image{}", number(image)), pixels);
                    (id, *width, *height, (pixels.width(), pixels.height()))
                }),
                Shape::Paint { canvas, width, height } => {
                    inkframe_render::canvas_image(self.doc, *canvas, *width, *height).map(|pixels| {
                        let id = self.image_asset(format!("paint{}-{ip}", number(canvas)), &pixels);
                        (id, *width as f32, *height as f32, (*width, *height))
                    })
                }
                _ => None,
            };
            if let Some((id, width, height, (pixel_w, pixel_h))) = picture {
                self.flush_shapes(&mut shapes, &mut out, layer, ip, op, opacity, length, next);
                // Centred on the layer's origin, at its shown size.
                let mut image = self.content_layer(2, layer, next, ip, op, opacity, length);
                image["refId"] = json!(id);
                image["ks"]["p"] = still([-width / 2.0, -height / 2.0]);
                image["ks"]["s"] =
                    still([width / pixel_w.max(1) as f32 * 100.0, height / pixel_h.max(1) as f32 * 100.0]);
                out.push(image);
            } else if let Some(group) = shape_group(element) {
                shapes.push(group);
            }
        }
        self.flush_shapes(&mut shapes, &mut out, layer, ip, op, opacity, length, next);
        out
    }

    /// Puts the shapes gathered so far on a shape layer of their own.
    #[allow(clippy::too_many_arguments)]
    fn flush_shapes(
        &self,
        shapes: &mut Vec<Value>,
        out: &mut Vec<Value>,
        layer: &Layer,
        ip: usize,
        op: usize,
        opacity: &dyn Fn(usize) -> f32,
        length: usize,
        next: &mut ContentIndex,
    ) {
        if shapes.is_empty() {
            return;
        }
        let mut shape_layer = self.content_layer(4, layer, next, ip, op, opacity, length);
        // Lottie draws a layer's first shape on top.
        shapes.reverse();
        shape_layer["shapes"] = json!(std::mem::take(shapes));
        out.push(shape_layer);
    }

    /// Embeds a picture once, under `key`; returns its asset id.
    fn image_asset(&mut self, key: String, pixels: &tiny_skia::Pixmap) -> String {
        if let Some(id) = self.images_done.get(&key) {
            return id.clone();
        }
        let id = format!("image_{}", self.images_done.len());
        let png = pixels.encode_png().unwrap_or_default();
        self.assets.push(json!({
            "id": id, "w": pixels.width(), "h": pixels.height(), "u": "",
            "p": format!("data:image/png;base64,{}", BASE64.encode(png)), "e": 1,
        }));
        self.images_done.insert(key, id.clone());
        id
    }

    /// A symbol's precomposition (made once), every frame of its own
    /// timeline. Returns its asset id.
    fn symbol(&mut self, id: SymbolId, depth: usize) -> Precomp {
        if let Some(precomp) = self.symbols_done.get(&id) {
            return precomp.clone();
        }
        // The box around everything it shows over its whole length, with
        // room for strokes, which the box leaves out.
        let extent = inkframe_render::symbol_extent(self.doc, id)
            .unwrap_or_else(|| inkframe_core::Rect::from_points(Default::default(), Default::default()));
        let margin = 0.1 * extent.width().max(extent.height()) + 20.0;
        let origin = [margin - extent.min.x, margin - extent.min.y];
        let size =
            ((extent.width() + 2.0 * margin).ceil() as u32, (extent.height() + 2.0 * margin).ceil() as u32);
        let precomp = Precomp { id: format!("symbol_{}", self.symbols_done.len()), origin, size };
        // Noted before its layers are made, so a symbol inside itself (which
        // the editor refuses, but a file could hold) can't loop forever.
        self.symbols_done.insert(id, precomp.clone());

        let duration = self.doc.symbol(id).map_or(1, |s| s.duration.max(1));
        let frames: Vec<u32> = (0..duration).collect();
        let mut layers = self.comp(TimelineId::Symbol(id), &frames, depth + 1, &mut |_| false);
        // Everything hangs off one null that moves it into the box.
        let root = 3_000_000;
        for layer in &mut layers {
            if layer.get("parent").is_none() {
                layer["parent"] = json!(root);
            }
        }
        layers.push(json!({
            "ty": 3, "ind": root, "nm": "Origin", "ip": 0, "op": duration, "st": 0, "sr": 1,
            "ks": static_transform(Affine::translate(inkframe_core::Point::new(origin[0], origin[1]))),
        }));
        self.assets.push(json!({ "id": precomp.id, "layers": layers }));
        precomp
    }

    /// A track matte: `mask`'s drawing (as it is when `content` starts),
    /// hanging off the mask's null, over the same frames.
    fn matte(&self, mask: &Layer, content: &Value, frames: &[u32], next: &mut ContentIndex) -> Value {
        let ip = content["ip"].as_u64().unwrap_or(0) as usize;
        let op = content["op"].as_u64().unwrap_or(frames.len() as u64) as usize;
        let mut shapes: Vec<Value> = self
            .doc
            .posed_elements(mask, frames[ip.min(frames.len() - 1)])
            .iter()
            .filter_map(shape_group)
            .collect();
        shapes.reverse();
        json!({
            "ty": 4, "ind": next.take(), "nm": format!("{} (mask)", mask.name), "parent": null_index(mask.id),
            "ip": ip, "op": op, "st": 0, "sr": 1,
            "ks": static_transform(Affine::IDENTITY), "shapes": shapes,
        })
    }

    /// A layer Lottie can't draw as vectors, drawn by the renderer on each
    /// frame and embedded, one image layer per frame. Only the part with
    /// something drawn is kept, to keep the file small.
    fn rendered(&mut self, layer: &Layer, frames: &[u32], next: &mut ContentIndex) -> Vec<Value> {
        let doc = self.doc;
        // Drawn at the export's size, not the stage's: everything is scaled
        // down by the root null anyway, and these images are what make a
        // file big.
        let scale = self.scale;
        let size = inkframe_render::output_size(doc, scale);
        let mut out = Vec::new();
        for (t, &frame) in frames.iter().enumerate() {
            if !layer.visible || !layer.is_shown_at(frame) {
                continue;
            }
            let to_image = Affine::scale(scale, scale);
            let Some(pixels) =
                inkframe_render::render_layer_alone(doc, Some(self.images), layer, frame, to_image, size)
            else {
                continue;
            };
            let Some((cropped, x, y)) = crop(&pixels) else { continue };
            let id = self.image_asset(format!("render{}-{t}", number(layer.id)), &cropped);
            // Back into stage units, where the root null scales it again.
            let place = Affine::scale(1.0 / scale, 1.0 / scale)
                .pre_concat(Affine::translate(inkframe_core::Point::new(x as f32, y as f32)));
            out.push(json!({
                "ty": 2, "ind": next.take(), "nm": layer.name, "refId": id,
                "ip": t, "op": t + 1, "st": 0, "sr": 1, "bm": blend(layer.blend),
                "ks": static_transform(place),
            }));
        }
        out
    }
}

/// Hands out indices for content layers.
struct ContentIndex(i64);

impl ContentIndex {
    fn take(&mut self) -> i64 {
        self.0 += 1;
        self.0
    }
}

/// A group of Lottie shape items drawing one element: its outline, then
/// its stroke over its fill. `None` for pictures, which aren't shapes.
fn shape_group(element: &Element) -> Option<Value> {
    let outline = inkframe_render::shape_outline(&element.shape)?;
    let mut items: Vec<Value> =
        contours(&outline).into_iter().map(|c| json!({ "ty": "sh", "ks": { "a": 0, "k": c } })).collect();
    let style = &element.style;
    // A pencil line is drawn as its filled outline (see `line.rs`).
    let line = matches!(element.shape, Shape::Line { .. });
    if let Some(stroke) = style.stroke.filter(|s| s.width > 0.0 && !line) {
        let mut st = json!({
            "ty": "st", "c": still(rgb(stroke.color)), "o": still(alpha(stroke.color)), "w": still(stroke.width),
            "lc": match stroke.cap { LineCap::Butt => 1, LineCap::Round => 2, LineCap::Square => 3 },
            "lj": match stroke.join { LineJoin::Miter => 1, LineJoin::Round => 2, LineJoin::Bevel => 3 },
            "ml": 4,
        });
        if let Some(dash) = stroke.dash {
            st["d"] = json!([
                { "n": "d", "nm": "dash", "v": still(dash.length.max(0.01)) },
                { "n": "g", "nm": "gap", "v": still(dash.gap.max(0.01)) },
            ]);
        }
        items.push(st);
    }
    match (&style.gradient, style.fill) {
        (Some(gradient), _) => {
            let (from, to) = gradient.ends_in(element.shape.bounds());
            let mut stops: Vec<f32> = Vec::new();
            for stop in &gradient.stops {
                let [r, g, b] = rgb(stop.color);
                stops.extend([stop.at, r, g, b]);
            }
            if gradient.stops.iter().any(|s| s.color.a < 255) {
                for stop in &gradient.stops {
                    stops.extend([stop.at, stop.color.a as f32 / 255.0]);
                }
            }
            items.push(json!({
                "ty": "gf", "o": still(100.0), "r": 1,
                "t": match gradient.shape { GradientShape::Linear => 1, GradientShape::Radial => 2 },
                "s": still([from.x, from.y]), "e": still([to.x, to.y]),
                "h": still(0.0), "a": still(0.0),
                "g": { "p": gradient.stops.len(), "k": { "a": 0, "k": stops } },
            }));
        }
        (None, Some(fill)) => {
            items.push(json!({ "ty": "fl", "c": still(rgb(fill)), "o": still(alpha(fill)), "r": 1 }))
        }
        (None, None) => {}
    }
    items.push(json!({
        "ty": "tr", "p": still([0.0, 0.0]), "a": still([0.0, 0.0]), "s": still([100.0, 100.0]),
        "r": still(0.0), "o": still(100.0), "sk": still(0.0), "sa": still(0.0),
    }));
    Some(json!({ "ty": "gr", "it": items }))
}

/// An outline's contours as Lottie bezier shapes: each point with its
/// in and out handles, relative to it.
fn contours(path: &tiny_skia::Path) -> Vec<Value> {
    use tiny_skia::PathSegment;
    type P = [f32; 2];
    struct Contour {
        v: Vec<P>,
        i: Vec<P>,
        o: Vec<P>,
        closed: bool,
    }
    let mut done: Vec<Contour> = Vec::new();
    let mut current: Option<Contour> = None;
    let finish = |c: Option<Contour>, done: &mut Vec<Contour>| {
        if let Some(mut c) = c {
            // A closing point on top of the first is the same point.
            if c.closed && c.v.len() > 1 {
                let (first, last) = (c.v[0], c.v[c.v.len() - 1]);
                if (first[0] - last[0]).abs() < 1e-4 && (first[1] - last[1]).abs() < 1e-4 {
                    c.i[0] = c.i[c.v.len() - 1];
                    c.v.pop();
                    c.i.pop();
                    c.o.pop();
                }
            }
            done.push(c);
        }
    };
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                finish(current.take(), &mut done);
                current = Some(Contour {
                    v: vec![[p.x, p.y]],
                    i: vec![[0.0, 0.0]],
                    o: vec![[0.0, 0.0]],
                    closed: false,
                });
            }
            PathSegment::LineTo(p) => {
                if let Some(c) = &mut current {
                    c.v.push([p.x, p.y]);
                    c.i.push([0.0, 0.0]);
                    c.o.push([0.0, 0.0]);
                }
            }
            PathSegment::QuadTo(q, p) => {
                if let Some(c) = &mut current {
                    let last = *c.v.last().expect("a contour starts with a point");
                    let c1 = [last[0] + (q.x - last[0]) * 2.0 / 3.0, last[1] + (q.y - last[1]) * 2.0 / 3.0];
                    let c2 = [p.x + (q.x - p.x) * 2.0 / 3.0, p.y + (q.y - p.y) * 2.0 / 3.0];
                    cubic(c, c1, c2, [p.x, p.y]);
                }
            }
            PathSegment::CubicTo(c1, c2, p) => {
                if let Some(c) = &mut current {
                    cubic(c, [c1.x, c1.y], [c2.x, c2.y], [p.x, p.y]);
                }
            }
            PathSegment::Close => {
                if let Some(c) = &mut current {
                    c.closed = true;
                }
            }
        }
    }
    finish(current, &mut done);

    fn cubic(c: &mut Contour, c1: P, c2: P, p: P) {
        let last = c.v.len() - 1;
        c.o[last] = [c1[0] - c.v[last][0], c1[1] - c.v[last][1]];
        c.v.push(p);
        c.i.push([c2[0] - p[0], c2[1] - p[1]]);
        c.o.push([0.0, 0.0]);
    }
    done.into_iter().map(|c| json!({ "i": c.i, "o": c.o, "v": c.v, "c": c.closed })).collect()
}

/// A transform sampled on every frame, as Lottie's position, rotation,
/// scale and skew, each still if it never changes.
fn transform(frames: &[u32], matrix: impl Fn(u32) -> Affine) -> Value {
    let poses: Vec<Transform> = frames.iter().map(|&f| Transform::default().from_affine(matrix(f))).collect();
    let values = |part: fn(&Transform) -> Value| poses.iter().map(part).collect::<Vec<_>>();
    json!({
        "a": still([0.0, 0.0]),
        "p": held(&values(|t| json!([t.position.x, t.position.y]))),
        "r": held(&values(|t| json!(t.rotation))),
        "s": held(&values(|t| json!([t.scale.x * 100.0, t.scale.y * 100.0]))),
        // Lottie skews the same way round as Inkframe, checked against
        // lottie-web.
        "sk": held(&values(|t| json!(t.skew.x))),
        "sa": still(0.0),
        "o": still(100.0),
    })
}

fn static_transform(m: Affine) -> Value {
    transform(&[0], |_| m)
}

/// One value per frame as Lottie keys: still if they're all the same,
/// otherwise a hold key wherever it changes.
fn held(values: &[Value]) -> Value {
    let Some(first) = values.first() else { return still(0.0) };
    if values.iter().all(|v| v == first) {
        return json!({ "a": 0, "k": first });
    }
    let as_array = |v: &Value| if v.is_array() { v.clone() } else { json!([v]) };
    let mut keys: Vec<Value> = Vec::new();
    let mut previous: Option<&Value> = None;
    for (t, value) in values.iter().enumerate() {
        if previous != Some(value) {
            keys.push(json!({ "t": t, "s": as_array(value), "h": 1 }));
            previous = Some(value);
        }
    }
    json!({ "a": 1, "k": keys })
}

fn still(value: impl Into<Value>) -> Value {
    let value: Value = value.into();
    json!({ "a": 0, "k": value })
}

fn rgb(c: Color) -> [f32; 3] {
    [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0]
}

fn alpha(c: Color) -> f32 {
    c.a as f32 / 255.0 * 100.0
}

/// Lottie's number for a blend mode.
fn blend(mode: BlendMode) -> u8 {
    match mode {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Overlay => 3,
        BlendMode::Darken => 4,
        BlendMode::Lighten => 5,
        BlendMode::ColorDodge => 6,
        BlendMode::ColorBurn => 7,
        BlendMode::HardLight => 8,
        BlendMode::SoftLight => 9,
        BlendMode::Difference => 10,
        BlendMode::Exclusion => 11,
        BlendMode::Hue => 12,
        BlendMode::Saturation => 13,
        BlendMode::Color => 14,
        BlendMode::Luminosity => 15,
        BlendMode::Add => 16,
    }
}

/// The part of an image with anything drawn, and where it was.
fn crop(pixels: &tiny_skia::Pixmap) -> Option<(tiny_skia::Pixmap, u32, u32)> {
    let (w, h) = (pixels.width(), pixels.height());
    let data = pixels.data();
    let opaque = |x: u32, y: u32| data[((y * w + x) * 4 + 3) as usize] > 0;
    let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| opaque(x, y))).collect();
    let cols: Vec<u32> = (0..w).filter(|&x| (0..h).any(|y| opaque(x, y))).collect();
    let (top, bottom) = (*rows.first()?, *rows.last()?);
    let (left, right) = (*cols.first()?, *cols.last()?);
    let rect = tiny_skia::IntRect::from_ltrb(left as i32, top as i32, right as i32 + 1, bottom as i32 + 1)?;
    Some((pixels.clone_rect(rect)?, left, top))
}

#[cfg(test)]
mod tests {
    use inkframe_core::{Point, Style};

    use super::*;

    fn square(doc: &mut Document, name: &str, at: Point) -> LayerId {
        let element = Element {
            shape: Shape::Rect { width: 20.0, height: 10.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::rgb(255, 0, 0)), stroke: None, gradient: None },
        };
        doc.add_layer(name, vec![element], 0, Transform::at(at))
    }

    fn export(doc: &Document, count: u32) -> Value {
        animation(doc, &ImageLibrary::default(), 0, count, 1.0, true, &mut |_| false)
    }

    #[test]
    fn a_layer_is_a_null_moving_frame_by_frame_with_its_shapes_hanging_off_it() {
        let mut doc = Document::default();
        let id = square(&mut doc, "Square", Point::new(100.0, 50.0));
        doc.layer_mut(id).unwrap().track.set_pose(2, Transform::at(Point::new(120.0, 50.0)));
        let lottie = export(&doc, 3);
        assert_eq!(lottie["fr"], json!(24.0));
        assert_eq!(lottie["op"], json!(3));

        let layers = lottie["layers"].as_array().unwrap();
        let null = layers.iter().find(|l| l["ty"] == 3).unwrap();
        let keys = null["ks"]["p"]["k"].as_array().unwrap();
        assert_eq!(keys.len(), 3, "a hold key on each frame it moves");
        assert_eq!(keys[0]["s"], json!([100.0, 50.0]));
        assert_eq!(keys[2]["s"], json!([120.0, 50.0]));
        assert_eq!(null["ks"]["r"], json!({ "a": 0, "k": 0.0 }), "still: no keys");

        let shapes = layers.iter().find(|l| l["ty"] == 4).unwrap();
        assert_eq!(shapes["parent"], null["ind"]);
        let group = &shapes["shapes"][0]["it"];
        let path = &group[0]["ks"]["k"];
        assert_eq!(path["v"], json!([[-10.0, -5.0], [10.0, -5.0], [10.0, 5.0], [-10.0, 5.0]]));
        assert_eq!(path["c"], json!(true));
        assert_eq!(group[1]["ty"], "fl");
        assert_eq!(group[1]["c"]["k"], json!([1.0, 0.0, 0.0]));
    }

    #[test]
    fn a_drawing_swap_starts_a_new_shape_layer() {
        let mut doc = Document::default();
        let id = square(&mut doc, "Mouth", Point::new(10.0, 10.0));
        let layer = doc.layer_mut(id).unwrap();
        let open = layer.add_drawing(inkframe_core::Drawing::new("open", Vec::new()));
        layer.expose(2, open);
        let lottie = export(&doc, 4);
        let spans: Vec<(u64, u64)> = lottie["layers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["ty"] == 4)
            .map(|l| (l["ip"].as_u64().unwrap(), l["op"].as_u64().unwrap()))
            .collect();
        // The second drawing is empty, so only the first span has shapes.
        assert_eq!(spans, vec![(0, 2)]);
    }

    #[test]
    fn a_symbol_is_a_time_remapped_precomposition_fitted_to_its_box() {
        let mut doc = Document::default();
        let id = square(&mut doc, "Spinner", Point::new(200.0, 100.0));
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        doc.symbol_mut(symbol).unwrap().duration = 2;
        let lottie = export(&doc, 4);

        let asset = &lottie["assets"][0];
        assert_eq!(asset["id"], "symbol_0");
        let origin = asset["layers"].as_array().unwrap().iter().find(|l| l["nm"] == "Origin").unwrap();
        let shift = &origin["ks"]["p"]["k"];
        let precomp = lottie["layers"].as_array().unwrap().iter().find(|l| l["ty"] == 0).unwrap();
        assert_eq!(precomp["ks"]["a"]["k"], *shift, "the anchor undoes the shift into the box");
        // Looping every 2 frames: symbol frames 0, 1, 0, 1, in seconds.
        let remap: Vec<f64> =
            precomp["tm"]["k"].as_array().unwrap().iter().map(|k| k["s"][0].as_f64().unwrap()).collect();
        assert_eq!(remap, vec![0.0, 1.0 / 24.0, 0.0, 1.0 / 24.0]);
    }

    #[test]
    fn values_that_never_change_are_still() {
        assert_eq!(held(&[json!(5), json!(5)]), json!({ "a": 0, "k": 5 }));
        let keys = held(&[json!(1), json!(1), json!(2)]);
        assert_eq!(keys["k"].as_array().unwrap().len(), 2, "a key only where it changes");
    }
}
