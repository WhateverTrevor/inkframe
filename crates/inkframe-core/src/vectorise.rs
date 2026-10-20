//! Vectorising: turning pixels (a scanned drawing, a paint layer) into
//! filled vector shapes, as Animate's Trace Bitmap and Harmony's
//! vectorisation do.
//!
//! 1. **Label** every pixel: ink or not (by brightness), or which of the
//!    picture's main colours it's closest to.
//! 2. **Despeckle**: patches smaller than the minimum area, specks of dust
//!    or pinholes in a line, take the label around them.
//! 3. **Trace** each patch's edges along the pixel grid into loops: one
//!    outline and one loop per hole. Pixels that only touch at a corner
//!    count as separate, so a loop never crosses itself.
//! 4. **Fit** each loop: Ramer–Douglas–Peucker drops the pixel staircase,
//!    then the survivors are joined by curves, except where the outline
//!    turns sharply, which stays a corner.
//! 5. **Join** the holes to the outline. Shapes are one outline, so each
//!    hole is reached by a slit: a straight line to it and back along the
//!    same line. The two directions cancel, so the slit has no width and
//!    the hole stays empty (the renderer fills by winding, and the tracer
//!    gives holes the opposite direction to outlines).

use std::collections::HashMap;

use crate::color::Color;
use crate::document::{Document, Element, LayerId};
use crate::geometry::Point;
use crate::image::ImageId;
use crate::paint::unpremultiply;
use crate::shape::{Path, Segment, Shape, Stroke, Style};

/// Premultiplied RGBA pixels, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// What counts as a shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraceMode {
    /// Pixels darker than `threshold` (0–255) are ink; everything else is
    /// paper and left out. For scanned line drawings.
    Ink { threshold: u8 },
    /// Every opaque pixel, reduced to `colours` colours, each its own
    /// shapes. `skip_white` leaves out near-white, the paper of a scan.
    Colours { colours: u8, skip_white: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceOptions {
    pub mode: TraceMode,
    /// Patches of fewer pixels than this are merged into what surrounds
    /// them.
    pub min_area: u32,
    /// How far, in pixels, the outline may stray from the pixels' edges
    /// to be smoother. Around 1 follows the drawing closely.
    pub smoothing: f32,
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self { mode: TraceMode::Ink { threshold: 128 }, min_area: 8, smoothing: 1.0 }
    }
}

/// One traced shape, in the bitmap's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Traced {
    pub path: Path,
    pub color: Color,
    /// How many pixels it covers, for drawing big shapes first.
    pub area: u32,
}

/// Turns sharper than this (in degrees) stay corners.
const CORNER_ANGLE: f32 = 55.0;
/// Pixels at least this opaque are part of the picture.
const OPAQUE: u8 = 128;
/// Colours are clustered from at most this many pixels, spread over the
/// picture: plenty to find the colours, and quick on a large scan.
const SAMPLES: usize = 40_000;

/// Traces `bitmap` into filled shapes, biggest first.
pub fn trace(bitmap: &Bitmap, options: &TraceOptions) -> Vec<Traced> {
    let (mut labels, palette) = label(bitmap, options.mode);
    let (w, h) = (bitmap.width as usize, bitmap.height as usize);
    despeckle(&mut labels, w, h, options.min_area.max(1) as usize);

    let (patches, count) = patches(&labels, w, h);
    let mut loops: Vec<Vec<Vec<Corner>>> = vec![Vec::new(); count];
    for (patch, ring) in trace_loops(&patches, w, h) {
        loops[patch].push(ring);
    }
    let mut area = vec![0u32; count];
    let mut label_of = vec![0u16; count];
    for (i, &p) in patches.iter().enumerate() {
        if p != u32::MAX {
            area[p as usize] += 1;
            label_of[p as usize] = labels[i];
        }
    }

    let mut traced: Vec<Traced> = loops
        .into_iter()
        .enumerate()
        .filter(|(patch, _)| label_of[*patch] != 0)
        .filter_map(|(patch, rings)| {
            let path = shape_with_holes(rings, options.smoothing.max(0.25))?;
            Some(Traced { path, color: palette[label_of[patch] as usize], area: area[patch] })
        })
        .collect();
    traced.sort_by_key(|t| std::cmp::Reverse(t.area));
    traced
}

// ------------------------------------------------------------- labelling

/// A label per pixel (0 is nothing) and the colour of each label.
fn label(bitmap: &Bitmap, mode: TraceMode) -> (Vec<u16>, Vec<Color>) {
    let straight: Vec<[u8; 4]> = bitmap.pixels.chunks_exact(4).map(unpremultiply).collect();
    match mode {
        TraceMode::Ink { threshold } => {
            let is_ink = |p: &[u8; 4]| p[3] >= OPAQUE && luminance(p) < threshold as f32;
            let labels: Vec<u16> = straight.iter().map(|p| u16::from(is_ink(p))).collect();
            // The ink keeps its own colour (a blue pencil stays blue), as
            // the average of the ink pixels.
            let mut sum = [0u64; 3];
            let mut n = 0u64;
            for p in straight.iter().filter(|p| is_ink(p)) {
                for (total, &v) in sum.iter_mut().zip(p) {
                    *total += v as u64;
                }
                n += 1;
            }
            let n = n.max(1);
            let ink = Color::rgb((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8);
            (labels, vec![Color::TRANSPARENT, ink])
        }
        TraceMode::Colours { colours, skip_white } => {
            let centres = main_colours(&straight, colours.clamp(1, 64) as usize);
            let near_white = |c: &[f32; 3]| c.iter().all(|&v| v > 235.0);
            let labels = straight
                .iter()
                .map(|p| {
                    if p[3] < OPAQUE {
                        return 0;
                    }
                    let i = nearest(&centres, [p[0] as f32, p[1] as f32, p[2] as f32]);
                    if skip_white && near_white(&centres[i]) { 0 } else { i as u16 + 1 }
                })
                .collect();
            let palette = std::iter::once(Color::TRANSPARENT)
                .chain(centres.iter().map(|c| Color::rgb(c[0] as u8, c[1] as u8, c[2] as u8)))
                .collect();
            (labels, palette)
        }
    }
}

fn luminance(p: &[u8; 4]) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// The picture's `k` main colours.
///
/// Real colours pile up: a fill is thousands of pixels of nearly one
/// colour. The blends along edges (anti-aliasing) are spread thinly over
/// many colours in between. So the colours are counted in coarse bins,
/// and the busiest bins chosen, skipping any too close to one already
/// chosen. Each is then settled on the average of the pixels close to it
/// (but not the blends further out, which would pull a thin dark outline
/// towards the fill beside it).
fn main_colours(straight: &[[u8; 4]], k: usize) -> Vec<[f32; 3]> {
    /// Colours closer than this are the same colour.
    const DISTINCT: f32 = 48.0;
    let opaque: Vec<[f32; 3]> = straight
        .iter()
        .filter(|p| p[3] >= OPAQUE)
        .step_by((straight.len() / (SAMPLES * 4)).max(1))
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    let bin = |p: &[f32; 3]| ((p[0] as usize >> 4) << 8) | ((p[1] as usize >> 4) << 4) | (p[2] as usize >> 4);
    let mut bins = vec![(0usize, [0.0f32; 3]); 4096];
    for p in &opaque {
        let b = &mut bins[bin(p)];
        b.0 += 1;
        for (total, v) in b.1.iter_mut().zip(p) {
            *total += v;
        }
    }
    let mut busiest: Vec<(usize, [f32; 3])> =
        bins.into_iter().filter(|b| b.0 > 0).map(|(n, sum)| (n, sum.map(|v| v / n as f32))).collect();
    busiest.sort_by_key(|b| std::cmp::Reverse(b.0));

    let distance = |a: &[f32; 3], b: &[f32; 3]| (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt();
    let mut centres: Vec<[f32; 3]> = Vec::new();
    for (_, colour) in busiest {
        if centres.len() == k {
            break;
        }
        if centres.iter().all(|c| distance(c, &colour) >= DISTINCT) {
            centres.push(colour);
        }
    }
    if centres.is_empty() {
        return vec![[0.0; 3]];
    }
    // Settle each on its own pixels, leaving out the blends.
    let mut sums = vec![([0.0f32; 3], 0usize); centres.len()];
    for p in &opaque {
        let i = nearest(&centres, *p);
        if distance(&centres[i], p) < DISTINCT / 2.0 {
            for (total, v) in sums[i].0.iter_mut().zip(p) {
                *total += v;
            }
            sums[i].1 += 1;
        }
    }
    for (centre, (sum, n)) in centres.iter_mut().zip(sums) {
        if n > 0 {
            *centre = sum.map(|v| v / n as f32);
        }
    }
    centres
}

fn nearest(centres: &[[f32; 3]], p: [f32; 3]) -> usize {
    let distance = |c: &[f32; 3]| (0..3).map(|i| (c[i] - p[i]).powi(2)).sum::<f32>();
    (0..centres.len()).min_by(|&a, &b| distance(&centres[a]).total_cmp(&distance(&centres[b]))).unwrap_or(0)
}

// ------------------------------------------------------- patches and specks

/// Connected patches of the same label (pixels touching along a side),
/// numbered from 0, and how many there are. Unlabelled pixels are
/// `u32::MAX`.
fn patches(labels: &[u16], w: usize, h: usize) -> (Vec<u32>, usize) {
    let mut patch = vec![u32::MAX; labels.len()];
    let mut count = 0;
    let mut stack = Vec::new();
    for start in 0..labels.len() {
        if labels[start] == 0 || patch[start] != u32::MAX {
            continue;
        }
        let id = count as u32;
        flood(labels, w, h, start, &mut stack, |i| std::mem::replace(&mut patch[i], id) == u32::MAX);
        count += 1;
    }
    (patch, count)
}

/// Visits the pixels connected to `start` with its label. `claim` marks a
/// pixel as visited, returning whether it wasn't already.
fn flood(
    labels: &[u16],
    w: usize,
    h: usize,
    start: usize,
    stack: &mut Vec<usize>,
    mut claim: impl FnMut(usize) -> bool,
) {
    let label = labels[start];
    stack.clear();
    if claim(start) {
        stack.push(start);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        let neighbours = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ];
        for n in neighbours.into_iter().flatten() {
            if labels[n] == label && claim(n) {
                stack.push(n);
            }
        }
    }
}

/// Gives every patch smaller than `min_area` pixels (unlabelled ones
/// included, so pinholes in ink close up) the label most common around it.
fn despeckle(labels: &mut [u16], w: usize, h: usize, min_area: usize) {
    if min_area <= 1 {
        return;
    }
    let mut done = vec![false; labels.len()];
    let mut stack = Vec::new();
    let mut members = Vec::new();
    for start in 0..labels.len() {
        if done[start] {
            continue;
        }
        members.clear();
        flood(labels, w, h, start, &mut stack, |i| {
            let new = !std::mem::replace(&mut done[i], true);
            if new {
                members.push(i);
            }
            new
        });
        if members.len() >= min_area {
            continue;
        }
        let own = labels[start];
        let mut around: HashMap<u16, usize> = HashMap::new();
        for &i in &members {
            let (x, y) = (i % w, i / w);
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for n in neighbours.into_iter().flatten() {
                if labels[n] != own {
                    *around.entry(labels[n]).or_default() += 1;
                }
            }
        }
        if let Some((&label, _)) = around.iter().max_by_key(|(label, n)| (**n, std::cmp::Reverse(**label))) {
            for &i in &members {
                labels[i] = label;
            }
        }
    }
}

// --------------------------------------------------------------- tracing

/// A corner of the pixel grid: (0, 0) is the top left of the first pixel.
type Corner = (i32, i32);

/// Every patch's boundary loops, as pixel-corner points where the boundary
/// turns. Each patch is on the right of its loops going round (in
/// y-down coordinates), so outlines go clockwise on screen and holes the
/// other way.
fn trace_loops(patches: &[u32], w: usize, h: usize) -> Vec<(usize, Vec<Corner>)> {
    let at = |x: i32, y: i32| {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            u32::MAX
        } else {
            patches[y as usize * w + x as usize]
        }
    };
    // Edges by the corner they start from: (end, patch).
    let mut edges: HashMap<Corner, Vec<(Corner, u32)>> = HashMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let p = at(x, y);
            if p == u32::MAX {
                continue;
            }
            // Clockwise round the pixel on screen, keeping it on the right.
            let sides = [
                (at(x, y - 1), (x, y), (x + 1, y)),
                (at(x + 1, y), (x + 1, y), (x + 1, y + 1)),
                (at(x, y + 1), (x + 1, y + 1), (x, y + 1)),
                (at(x - 1, y), (x, y + 1), (x, y)),
            ];
            for (neighbour, from, to) in sides {
                if neighbour != p {
                    edges.entry(from).or_default().push((to, p));
                }
            }
        }
    }

    let mut loops = Vec::new();
    let mut starts: Vec<Corner> = edges.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        while let Some((first_to, patch)) = edges.get_mut(&start).and_then(Vec::pop) {
            let first = (first_to.0 - start.0, first_to.1 - start.1);
            let mut ring = vec![start];
            let mut direction = first;
            let mut at_corner = first_to;
            loop {
                // Where two edges of the patch leave one corner, two of its
                // pixels touch there only diagonally. Turning right, into
                // the patch, keeps them apart.
                let right = (-direction.1, direction.0);
                let Some(outgoing) = edges.get_mut(&at_corner) else { break };
                // Back at the start, the loop is closed if the rule would
                // take the edge it started along (and may pass through
                // the start if not, at such a corner).
                let more_here = outgoing.iter().any(|&(_, p)| p == patch);
                if at_corner == start && (first == right || !more_here) {
                    // The start is only a corner if the loop turns there.
                    if first == direction {
                        ring.remove(0);
                    }
                    break;
                }
                let pick = outgoing
                    .iter()
                    .position(|&(to, p)| p == patch && (to.0 - at_corner.0, to.1 - at_corner.1) == right)
                    .or_else(|| outgoing.iter().position(|&(_, p)| p == patch));
                let Some(pick) = pick else { break };
                let (to, _) = outgoing.swap_remove(pick);
                let turn = (to.0 - at_corner.0, to.1 - at_corner.1);
                if turn != direction {
                    ring.push(at_corner);
                }
                direction = turn;
                at_corner = to;
            }
            loops.push((patch as usize, ring));
        }
    }
    loops
}

/// Twice the signed area of a loop: positive for outlines, negative for
/// holes.
fn signed_area(ring: &[Corner]) -> i64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.0 as i64 * b.1 as i64 - b.0 as i64 * a.1 as i64
        })
        .sum()
}

// --------------------------------------------------------------- fitting

/// A patch's outline with its holes joined in, as one closed path.
fn shape_with_holes(rings: Vec<Vec<Corner>>, smoothing: f32) -> Option<Path> {
    let (mut outlines, holes): (Vec<_>, Vec<_>) = rings.into_iter().partition(|r| signed_area(r) > 0);
    let outline = fit_loop(&outlines.pop()?, smoothing)?;
    let mut path = outline;
    for hole in holes.iter().filter_map(|h| fit_loop(h, smoothing)) {
        path = join_hole(path, hole);
    }
    Some(path)
}

/// A smooth closed path round a pixel loop.
fn fit_loop(ring: &[Corner], smoothing: f32) -> Option<Path> {
    let points = staircase_free(ring);
    if points.len() < 3 {
        return None;
    }
    let mut kept = simplify_closed(&points, smoothing);
    if kept.len() < 3 {
        kept = points;
    }
    let n = kept.len();
    let at = |i: isize| kept[i.rem_euclid(n as isize) as usize];
    let corner = |i: isize| {
        let (a, b) = (at(i) - at(i - 1), at(i + 1) - at(i));
        let cos = (a.x * b.x + a.y * b.y) / (a.length() * b.length()).max(1e-6);
        cos.clamp(-1.0, 1.0).acos().to_degrees() > CORNER_ANGLE
    };
    let tangent = |i: isize| {
        let d = at(i + 1) - at(i - 1);
        d * (1.0 / d.length().max(1e-6))
    };
    let segments = (0..n as isize)
        .map(|i| {
            let (from, to) = (at(i), at(i + 1));
            let (sharp_from, sharp_to) = (corner(i), corner(i + 1));
            if sharp_from && sharp_to {
                return Segment::Line { to };
            }
            let third = from.distance(to) / 3.0;
            let ctrl1 = if sharp_from { from + (to - from) * (1.0 / 3.0) } else { from + tangent(i) * third };
            let ctrl2 = if sharp_to { to - (to - from) * (1.0 / 3.0) } else { to - tangent(i + 1) * third };
            Segment::Cubic { ctrl1, ctrl2, to }
        })
        .collect();
    Some(Path { start: kept[0], segments, closed: true })
}

/// Points along a pixel loop that follow the edge it stands for rather
/// than its stairs: the middle of each straight run, which is where a
/// slanted edge really passes, plus the corners between two runs of at
/// least `CORNER_RUN` pixels, which are real corners. Starts at a corner
/// if there is one, so simplifying keeps it.
fn staircase_free(ring: &[Corner]) -> Vec<Point> {
    const CORNER_RUN: i32 = 3;
    let n = ring.len();
    let point = |i: usize| Point::new(ring[i % n].0 as f32, ring[i % n].1 as f32);
    let run = |i: usize| {
        let (a, b) = (ring[i % n], ring[(i + 1) % n]);
        (a.0 - b.0).abs() + (a.1 - b.1).abs()
    };
    let mut points = Vec::with_capacity(n * 2);
    let mut first_corner = None;
    for i in 0..n {
        if run(i + n - 1) >= CORNER_RUN && run(i) >= CORNER_RUN {
            first_corner.get_or_insert(points.len());
            points.push(point(i));
        }
        let (a, b) = (point(i), point(i + 1));
        points.push(Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
    }
    points.rotate_left(first_corner.unwrap_or(0));
    points
}

/// Ramer–Douglas–Peucker for a loop: split at the point farthest from the
/// first, and simplify each half.
fn simplify_closed(points: &[Point], tolerance: f32) -> Vec<Point> {
    let far = (1..points.len())
        .max_by(|&a, &b| points[0].distance(points[a]).total_cmp(&points[0].distance(points[b])));
    let Some(far) = far else { return points.to_vec() };
    let mut first = simplify_open(&points[..=far], tolerance);
    let mut second: Vec<Point> = points[far..].to_vec();
    second.push(points[0]);
    let second = simplify_open(&second, tolerance);
    first.pop();
    first.extend(&second[..second.len() - 1]);
    first
}

fn simplify_open(points: &[Point], tolerance: f32) -> Vec<Point> {
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        let (farthest, distance) = (first + 1..last)
            .map(|i| (i, points[i].distance_to_segment(points[first], points[last])))
            .fold((first, 0.0f32), |best, cur| if cur.1 > best.1 { cur } else { best });
        if distance > tolerance {
            keep[farthest] = true;
            stack.push((first, farthest));
            stack.push((farthest, last));
        }
    }
    points.iter().zip(keep).filter_map(|(&p, k)| k.then_some(p)).collect()
}

/// Where each segment of a closed path ends, starting with its start.
fn anchor_points(path: &Path) -> Vec<Point> {
    std::iter::once(path.start).chain(path.segments.iter().map(|s| s.end())).collect()
}

/// `outline` with `hole` spliced in through a slit between their nearest
/// anchors: out along the slit, round the hole, and back along the slit.
fn join_hole(outline: Path, hole: Path) -> Path {
    let outer = anchor_points(&outline);
    let inner = anchor_points(&hole);
    let mut best = (0, 0, f32::MAX);
    for (i, a) in outer.iter().enumerate().take(outline.segments.len()) {
        for (j, b) in inner.iter().enumerate().take(hole.segments.len()) {
            let d = a.distance(*b);
            if d < best.2 {
                best = (i, j, d);
            }
        }
    }
    let (i, j, _) = best;
    let mut segments = outline.segments[..i].to_vec();
    segments.push(Segment::Line { to: inner[j] });
    segments.extend_from_slice(&hole.segments[j..]);
    segments.extend_from_slice(&hole.segments[..j]);
    segments.push(Segment::Line { to: outer[i] });
    segments.extend_from_slice(&outline.segments[i..]);
    Path { start: outline.start, segments, closed: true }
}

// ---------------------------------------------------------- into a layer

impl Document {
    /// Traces the pictures and painting in layer `id`'s drawings into a
    /// new vector layer just above it, placed and animated the same, and
    /// hides the original. `image` gives an image's pixels (the decoded
    /// files live outside the document). Returns the new layer, or `None`
    /// if there was nothing to trace.
    pub fn vectorise_layer(
        &mut self,
        id: LayerId,
        options: &TraceOptions,
        image: impl Fn(ImageId) -> Option<Bitmap>,
    ) -> Option<LayerId> {
        let source = self.layer(id)?;
        let mut traced_any = false;
        let mut drawings = source.drawings.clone();
        for drawing in &mut drawings {
            let mut elements = Vec::new();
            for element in &drawing.elements {
                let Some((bitmap, size)) = self.element_pixels(element, &image) else {
                    elements.push(element.clone());
                    continue;
                };
                traced_any = true;
                // Pixels to layer units: the picture is `size` wide and
                // tall, centred on the origin.
                let scale = Point::new(size.x / bitmap.width as f32, size.y / bitmap.height as f32);
                let to_layer =
                    |p: Point| Point::new(p.x * scale.x - size.x / 2.0, p.y * scale.y - size.y / 2.0);
                // Shapes of several colours meet edge to edge, and smoothing
                // can leave a hairline between them; a stroke of their own
                // colour closes it. Ink shapes stand alone and need none.
                let seam = matches!(options.mode, TraceMode::Colours { .. });
                elements.extend(trace(&bitmap, options).into_iter().map(|t| Element {
                    shape: Shape::Path(map_path(&t.path, to_layer)),
                    style: Style {
                        fill: Some(t.color),
                        stroke: seam.then_some(Stroke::new(t.color, scale.x.max(scale.y))),
                        gradient: None,
                    },
                }));
            }
            drawing.elements = elements;
        }
        if !traced_any {
            return None;
        }
        let new_id = self.duplicate_layer(id)?;
        let name = self.layer(id).map(|l| format!("{} (vector)", l.name)).unwrap_or_default();
        let copy = self.layer_mut(new_id)?;
        copy.name = name;
        copy.drawings = drawings;
        self.layer_mut(id)?.visible = false;
        Some(new_id)
    }

    /// The pixels of a picture or painting, with its size in layer units.
    fn element_pixels(
        &self,
        element: &Element,
        image: &impl Fn(ImageId) -> Option<Bitmap>,
    ) -> Option<(Bitmap, Point)> {
        match element.shape {
            Shape::Paint { canvas, width, height } => {
                let pixels = self.canvas(canvas)?.to_pixels();
                Some((Bitmap { width, height, pixels }, Point::new(width as f32, height as f32)))
            }
            Shape::Image { image: id, width, height } => Some((image(id)?, Point::new(width, height))),
            _ => None,
        }
    }
}

fn map_path(path: &Path, f: impl Fn(Point) -> Point) -> Path {
    let segments = path
        .segments
        .iter()
        .map(|s| match *s {
            Segment::Line { to } => Segment::Line { to: f(to) },
            Segment::Cubic { ctrl1, ctrl2, to } => {
                Segment::Cubic { ctrl1: f(ctrl1), ctrl2: f(ctrl2), to: f(to) }
            }
        })
        .collect();
    Path { start: f(path.start), segments, closed: path.closed }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white bitmap with black where `ink` says.
    fn drawn(w: u32, h: u32, ink: impl Fn(u32, u32) -> bool) -> Bitmap {
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.extend(if ink(x, y) { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
            }
        }
        Bitmap { width: w, height: h, pixels }
    }

    fn ink() -> TraceOptions {
        TraceOptions { mode: TraceMode::Ink { threshold: 128 }, min_area: 4, smoothing: 1.0 }
    }

    fn style() -> Style {
        Style { fill: Some(Color::BLACK), stroke: None, gradient: None }
    }

    #[test]
    fn a_square_of_ink_becomes_a_square() {
        let shapes = trace(&drawn(20, 20, |x, y| (5..15).contains(&x) && (5..15).contains(&y)), &ink());
        assert_eq!(shapes.len(), 1);
        let shape = Shape::Path(shapes[0].path.clone());
        let bounds = shape.bounds();
        assert_eq!((bounds.min, bounds.max), (Point::new(5.0, 5.0), Point::new(15.0, 15.0)));
        // Four sharp corners, so four straight sides.
        assert_eq!(shapes[0].path.segments.len(), 4);
        assert!(shapes[0].path.segments.iter().all(|s| matches!(s, Segment::Line { .. })));
        assert_eq!(shapes[0].color, Color::BLACK);
    }

    #[test]
    fn a_ring_keeps_its_hole_empty() {
        let ring = |x: u32, y: u32| {
            let d = ((x as f32 - 30.0).powi(2) + (y as f32 - 30.0).powi(2)).sqrt();
            (12.0..24.0).contains(&d)
        };
        let shapes = trace(&drawn(60, 60, ring), &ink());
        assert_eq!(shapes.len(), 1);
        let shape = Shape::Path(shapes[0].path.clone());
        assert!(shape.hit_test(Point::new(30.0, 12.5), &style(), 0.0), "on the ring");
        assert!(!shape.hit_test(Point::new(30.0, 30.0), &style(), 0.0), "in the hole");
        assert!(!shape.hit_test(Point::new(2.0, 2.0), &style(), 0.0), "outside");
    }

    #[test]
    fn specks_are_dropped_and_pinholes_closed() {
        let shapes = trace(
            &drawn(40, 40, |x, y| {
                let block = (10..30).contains(&x) && (10..30).contains(&y) && (x, y) != (20, 20);
                block || (x, y) == (2, 2)
            }),
            &ink(),
        );
        assert_eq!(shapes.len(), 1, "the speck is gone");
        assert!(Shape::Path(shapes[0].path.clone()).hit_test(Point::new(20.5, 20.5), &style(), 0.0));
    }

    #[test]
    fn diagonal_neighbours_are_separate_shapes() {
        let shapes = trace(
            &drawn(20, 20, |x, y| {
                ((2..6).contains(&x) && (2..6).contains(&y)) || ((6..10).contains(&x) && (6..10).contains(&y))
            }),
            &ink(),
        );
        assert_eq!(shapes.len(), 2);
    }

    #[test]
    fn colours_each_get_their_own_shapes() {
        let mut bitmap = drawn(30, 10, |_, _| false);
        for y in 0..10 {
            for x in 0..30 {
                let colour = match x {
                    0..10 => [255, 0, 0, 255],
                    10..20 => [0, 0, 255, 255],
                    _ => [255, 255, 255, 255],
                };
                bitmap.pixels[((y * 30 + x) * 4) as usize..][..4].copy_from_slice(&colour);
            }
        }
        let options = TraceOptions { mode: TraceMode::Colours { colours: 3, skip_white: true }, ..ink() };
        let mut colours: Vec<Color> = trace(&bitmap, &options).iter().map(|t| t.color).collect();
        colours.sort_by_key(|c| (c.r, c.b));
        assert_eq!(colours, [Color::rgb(0, 0, 255), Color::rgb(255, 0, 0)]);
    }

    #[test]
    fn a_traced_layer_lines_up_with_the_painting() {
        let mut doc = Document { width: 40, height: 20, ..Document::default() };
        let layer = doc.add_paint_layer(crate::TimelineId::Main, "Scan", 0).unwrap();
        let (canvas, _, _) = doc.paint_canvas_at(layer, 0).unwrap();
        let square: Vec<u8> = drawn(40, 20, |x, y| (10..20).contains(&x) && (5..15).contains(&y)).pixels;
        doc.canvas_mut(canvas).unwrap().write_region(0, 0, 40, 20, &square);

        let vector = doc.vectorise_layer(layer, &ink(), |_| None).unwrap();
        assert!(!doc.layer(layer).unwrap().visible, "the original is hidden");
        let traced = doc.layer(vector).unwrap();
        assert!(traced.draws_vectors());
        let bounds = traced.drawings[0].elements[0].shape.bounds();
        // The canvas is centred on the origin: pixel 10 is at -10.
        assert_eq!((bounds.min, bounds.max), (Point::new(-10.0, -5.0), Point::new(0.0, 5.0)));
    }

    #[test]
    fn a_picture_is_traced_at_its_size_on_the_stage() {
        let mut doc = Document::default();
        // A 10×10 picture shown 100 units wide: each pixel is 10 units.
        let picture = Element {
            shape: Shape::Image { image: ImageId(7), width: 100.0, height: 100.0 },
            style: Style { fill: None, stroke: None, gradient: None },
        };
        let layer = doc.add_layer("Scan", vec![picture], 0, crate::Transform::default());
        let pixels = drawn(10, 10, |x, y| x < 5 && y < 5);
        let vector =
            doc.vectorise_layer(layer, &ink(), |id| (id == ImageId(7)).then(|| pixels.clone())).unwrap();
        let bounds = doc.layer(vector).unwrap().drawings[0].elements[0].shape.bounds();
        assert_eq!((bounds.min, bounds.max), (Point::new(-50.0, -50.0), Point::new(0.0, 0.0)));
        assert!(
            doc.vectorise_layer(layer, &ink(), |_| None).is_none(),
            "nothing to trace without its pixels"
        );
    }

    #[test]
    fn traced_line_art_can_be_filled_with_the_paint_bucket() {
        // A drawn box outline, 3 pixels thick.
        let outline = |x: u32, y: u32| {
            let inside = (10..50).contains(&x) && (10..50).contains(&y);
            let hollow = (13..47).contains(&x) && (13..47).contains(&y);
            inside && !hollow
        };
        let lines: Vec<Element> = trace(&drawn(60, 60, outline), &ink())
            .into_iter()
            .map(|t| Element { shape: Shape::Path(t.path), style: style() })
            .collect();
        let mut doc = Document::default();
        let id = doc.add_layer("Ink", lines, 0, crate::Transform::default());
        let layer = doc.layer_mut(id).unwrap();
        let filled = layer.fill_enclosed(0, Point::new(30.0, 30.0), Color::rgb(255, 0, 0), 0.0);
        assert_eq!(filled, crate::FillResult::Filled);
    }
}
