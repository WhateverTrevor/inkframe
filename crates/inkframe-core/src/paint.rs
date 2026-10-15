//! Painted pixels: the canvases behind paint layers.
//!
//! Everything else in a document is vector shapes, a few numbers each. A
//! painting is pixels, megabytes of them, so canvases are kept apart from
//! the shapes and handled with care in three places:
//!
//! * **Memory.** A canvas is split into 256×256 tiles, and only tiles that
//!   have been painted on exist. A small sketch on a big stage costs a few
//!   tiles, not the whole stage.
//! * **Undo.** Undo keeps whole copies of the document (see `history.rs`).
//!   Tiles are shared between copies (`Arc`), and painting copies only the
//!   tiles it touches, so a brush stroke adds a few tiles to the undo
//!   history rather than a whole canvas.
//! * **Saving.** The project file is a zip (see `container.rs`): the
//!   document as JSON, and each tile as a PNG beside it. The JSON only
//!   records each canvas's size.
//!
//! Pixels are premultiplied RGBA, 8 bits per channel, the way tiny-skia
//! draws them, so the renderer can draw tiles without converting them.
//!
//! A paint layer's drawing holds one [`Shape::Paint`](crate::Shape::Paint)
//! naming its canvas, so painting frame by frame is drawing substitution
//! (see `drawing.rs`): each drawing is its own canvas.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::document::{Document, Element, LayerId};
use crate::geometry::{Point, Rect};
use crate::shape::Shape;

/// Tiles are this many pixels square.
pub const TILE: u32 = 256;
const TILE_BYTES: usize = (TILE * TILE * 4) as usize;

/// A stable identifier for a canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CanvasId(pub(crate) u64);

impl std::fmt::Display for CanvasId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A grid of pixels, `width`×`height`, stored as tiles. Only its size goes
/// into the document's JSON; the pixels are saved separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Painted tiles by (column, row), premultiplied RGBA. A missing tile
    /// is fully transparent.
    #[serde(skip)]
    tiles: BTreeMap<(u32, u32), Arc<Vec<u8>>>,
}

impl PartialEq for Canvas {
    /// Two canvases are equal if their tiles are. Tiles nobody painted on
    /// since are the same shared tile, so comparing a canvas with its last
    /// undo snapshot only looks at bytes for the tiles a stroke changed.
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.tiles.len() == other.tiles.len()
            && self
                .tiles
                .iter()
                .zip(&other.tiles)
                .all(|((ka, a), (kb, b))| ka == kb && (Arc::ptr_eq(a, b) || a == b))
    }
}

impl Canvas {
    /// An empty, transparent canvas.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width: width.max(1), height: height.max(1), tiles: BTreeMap::new() }
    }

    /// How many tiles across and down.
    pub fn tile_counts(&self) -> (u32, u32) {
        (self.width.div_ceil(TILE), self.height.div_ceil(TILE))
    }

    /// The painted tiles, by (column, row).
    pub fn tiles(&self) -> impl Iterator<Item = ((u32, u32), &[u8])> {
        self.tiles.iter().map(|(&at, tile)| (at, tile.as_slice()))
    }

    pub fn tile(&self, at: (u32, u32)) -> Option<&[u8]> {
        self.tiles.get(&at).map(|t| t.as_slice())
    }

    /// A tile to paint on, created transparent if it wasn't there. If an
    /// undo snapshot shares it, it's copied first, so the snapshot keeps
    /// the old pixels.
    pub fn tile_mut(&mut self, at: (u32, u32)) -> &mut [u8] {
        Arc::make_mut(self.tiles.entry(at).or_insert_with(|| Arc::new(vec![0; TILE_BYTES]))).as_mut_slice()
    }

    /// Replaces a tile, or removes it if `pixels` is fully transparent.
    pub fn set_tile(&mut self, at: (u32, u32), pixels: Vec<u8>) {
        debug_assert_eq!(pixels.len(), TILE_BYTES);
        if pixels.chunks_exact(4).all(|p| p[3] == 0) {
            self.tiles.remove(&at);
        } else {
            self.tiles.insert(at, Arc::new(pixels));
        }
    }

    /// The tile `at` as it is, shared, for a stroke to paint over later.
    pub(crate) fn shared_tile(&self, at: (u32, u32)) -> Option<Arc<Vec<u8>>> {
        self.tiles.get(&at).cloned()
    }

    /// The premultiplied colour of pixel (`x`, `y`), transparent outside.
    pub fn pixel(&self, x: i64, y: i64) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return [0; 4];
        }
        let (x, y) = (x as u32, y as u32);
        let Some(tile) = self.tile((x / TILE, y / TILE)) else { return [0; 4] };
        let i = (((y % TILE) * TILE + x % TILE) * 4) as usize;
        [tile[i], tile[i + 1], tile[i + 2], tile[i + 3]]
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The smallest rectangle, in canvas pixels, holding every painted
    /// pixel, or `None` if nothing is painted.
    pub fn painted_bounds(&self) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        for (&(tx, ty), tile) in &self.tiles {
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (TILE, TILE, 0, 0);
            for (i, pixel) in tile.chunks_exact(4).enumerate() {
                if pixel[3] != 0 {
                    let (x, y) = (i as u32 % TILE, i as u32 / TILE);
                    (min_x, min_y, max_x, max_y) =
                        (min_x.min(x), min_y.min(y), max_x.max(x + 1), max_y.max(y + 1));
                }
            }
            if min_x < max_x {
                let at = |x: u32, y: u32| Point::new((tx * TILE + x) as f32, (ty * TILE + y) as f32);
                let tile_bounds = Rect::from_points(at(min_x, min_y), at(max_x, max_y));
                bounds = Some(bounds.map_or(tile_bounds, |b| b.union(tile_bounds)));
            }
        }
        bounds
    }

    /// A rectangle of pixels, `width`×`height` from (`x`, `y`), row by row,
    /// premultiplied RGBA. Parts outside the canvas come back transparent.
    pub fn read_region(&self, x: i64, y: i64, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![0; (width * height * 4) as usize];
        self.each_tile_span(x, y, width, height, |tile_at, from, to, count| {
            if let Some(tile) = self.tile(tile_at) {
                pixels[to..to + count * 4].copy_from_slice(&tile[from..from + count * 4]);
            }
        });
        pixels
    }

    /// Writes a rectangle read by [`Canvas::read_region`] back, after
    /// changing it. Parts outside the canvas are dropped, and tiles left
    /// fully transparent are removed.
    pub fn write_region(&mut self, x: i64, y: i64, width: u32, height: u32, pixels: &[u8]) {
        let mut spans = Vec::new();
        self.each_tile_span(x, y, width, height, |tile_at, from, to, count| {
            spans.push((tile_at, from, to, count))
        });
        let mut touched = Vec::new();
        for (tile_at, from, to, count) in spans {
            let source = &pixels[to..to + count * 4];
            // Writing nothing onto nothing needn't make a tile.
            if self.tile(tile_at).is_none() && source.iter().all(|&b| b == 0) {
                continue;
            }
            self.tile_mut(tile_at)[from..from + count * 4].copy_from_slice(source);
            touched.push(tile_at);
        }
        touched.dedup();
        for tile_at in touched {
            if self.tile(tile_at).is_some_and(|t| t.chunks_exact(4).all(|p| p[3] == 0)) {
                self.tiles.remove(&tile_at);
            }
        }
    }

    /// Splits a rectangle into its rows' runs within each tile, calling
    /// `span(tile, offset in tile, offset in the rectangle, pixel count)`
    /// for each; byte offsets, for copying between the two.
    fn each_tile_span(
        &self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        mut span: impl FnMut((u32, u32), usize, usize, usize),
    ) {
        for row in 0..height as i64 {
            let cy = y + row;
            if cy < 0 || cy >= self.height as i64 {
                continue;
            }
            let (start, end) = (x.max(0), (x + width as i64).min(self.width as i64));
            let mut cx = start;
            while cx < end {
                let (tx, ty) = (cx as u32 / TILE, cy as u32 / TILE);
                let run_end = end.min(((tx + 1) * TILE) as i64);
                let count = (run_end - cx) as usize;
                let from = (((cy as u32 % TILE) * TILE + cx as u32 % TILE) * 4) as usize;
                let to = ((row * width as i64 + (cx - x)) * 4) as usize;
                span((tx, ty), from, to, count);
                cx = run_end;
            }
        }
    }

    /// The whole canvas as one premultiplied RGBA image, row by row.
    pub fn to_pixels(&self) -> Vec<u8> {
        let mut pixels = vec![0; (self.width * self.height * 4) as usize];
        for (&(tx, ty), tile) in &self.tiles {
            for row in 0..TILE {
                let y = ty * TILE + row;
                if y >= self.height {
                    break;
                }
                let x = tx * TILE;
                let count = TILE.min(self.width.saturating_sub(x)) as usize;
                let from = (row * TILE * 4) as usize;
                let to = ((y * self.width + x) * 4) as usize;
                pixels[to..to + count * 4].copy_from_slice(&tile[from..from + count * 4]);
            }
        }
        pixels
    }
}

/// Converts a tile to PNG. PNG stores colour that isn't premultiplied, so
/// that's undone first; any image program can open the tiles.
pub(crate) fn encode_tile(tile: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let straight: Vec<u8> = tile.chunks_exact(4).flat_map(unpremultiply).collect();
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, TILE, TILE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&straight)?;
    Ok(bytes)
}

/// Reads a tile saved by [`encode_tile`].
pub(crate) fn decode_tile(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let info = reader.info();
    if (info.width, info.height) != (TILE, TILE)
        || info.color_type != png::ColorType::Rgba
        || info.bit_depth != png::BitDepth::Eight
    {
        return Err("a canvas tile isn't a 256×256 8-bit RGBA PNG".into());
    }
    let mut straight = vec![0; reader.output_buffer_size().ok_or("tile too large")?];
    reader.next_frame(&mut straight).map_err(|e| e.to_string())?;
    Ok(straight.chunks_exact(4).flat_map(premultiply).collect())
}

pub fn premultiply(p: &[u8]) -> [u8; 4] {
    let a = p[3] as u32;
    let scale = |c: u8| ((c as u32 * a + 127) / 255) as u8;
    [scale(p[0]), scale(p[1]), scale(p[2]), p[3]]
}

pub fn unpremultiply(p: &[u8]) -> [u8; 4] {
    let a = p[3] as u32;
    if a == 0 {
        return [0; 4];
    }
    let scale = |c: u8| ((c as u32 * 255 + a / 2) / a).min(255) as u8;
    [scale(p[0]), scale(p[1]), scale(p[2]), p[3]]
}

impl Document {
    /// Adds an empty canvas and returns its id.
    pub fn add_canvas(&mut self, width: u32, height: u32) -> CanvasId {
        let id = CanvasId(self.next_canvas_id);
        self.next_canvas_id += 1;
        self.canvases.insert(id, Canvas::new(width, height));
        id
    }

    pub fn canvas(&self, id: CanvasId) -> Option<&Canvas> {
        self.canvases.get(&id)
    }

    pub fn canvas_mut(&mut self, id: CanvasId) -> Option<&mut Canvas> {
        self.canvases.get_mut(&id)
    }

    /// A copy of a canvas under a new id. The copy shares the original's
    /// tiles until either is painted on.
    pub fn copy_canvas(&mut self, id: CanvasId) -> Option<CanvasId> {
        let canvas = self.canvases.get(&id)?.clone();
        let copy = CanvasId(self.next_canvas_id);
        self.next_canvas_id += 1;
        self.canvases.insert(copy, canvas);
        Some(copy)
    }

    /// Gives each paint shape in `elements` a canvas of its own: a copy of
    /// the one it had (`blank` false), or an empty one the same size. Used
    /// whenever a drawing or layer is duplicated, so painting on the copy
    /// doesn't paint on the original too.
    pub fn separate_canvases(&mut self, elements: &mut [Element], blank: bool) {
        for element in elements {
            if let Shape::Paint { canvas, width, height } = &mut element.shape {
                let fresh =
                    if blank { Some(self.add_canvas(*width, *height)) } else { self.copy_canvas(*canvas) };
                if let Some(fresh) = fresh {
                    *canvas = fresh;
                }
            }
        }
    }

    /// The bounds of `elements`, in their layer's coordinates. Paint shapes
    /// count only their painted pixels, not their whole canvas, so a small
    /// sketch on a stage-sized canvas is selected as the sketch.
    pub fn element_bounds(&self, elements: &[Element]) -> Option<Rect> {
        elements
            .iter()
            .filter_map(|e| match e.shape {
                Shape::Paint { canvas, width, height } => {
                    let painted = self.canvas(canvas)?.painted_bounds()?;
                    let corner = Point::new(width as f32 / 2.0, height as f32 / 2.0);
                    Some(Rect::from_points(painted.min - corner, painted.max - corner))
                }
                _ => Some(e.bounds()),
            })
            .reduce(Rect::union)
    }

    /// Whether `local` (in the element's layer's coordinates) lands on it.
    /// A paint shape is hit only where something is painted (within
    /// `tolerance`), so clicking through the empty part of a canvas reaches
    /// the layers below.
    pub fn element_hit(&self, element: &Element, local: Point, tolerance: f32) -> bool {
        let Shape::Paint { canvas, width, height } = element.shape else {
            return element.shape.hit_test(local, &element.style, tolerance);
        };
        let Some(canvas) = self.canvas(canvas) else { return false };
        let (x, y) = (local.x + width as f32 / 2.0, local.y + height as f32 / 2.0);
        let reach = tolerance.ceil().clamp(0.0, 16.0) as i64;
        (-reach..=reach).any(|dy| {
            (-reach..=reach).any(|dx| {
                (dx * dx + dy * dy) as f32 <= tolerance * tolerance + 0.5
                    && canvas.pixel(x.floor() as i64 + dx, y.floor() as i64 + dy)[3] > 24
            })
        })
    }

    /// Adds a paint layer, a canvas the size of the stage that's lined up
    /// with it pixel for pixel, on top of `timeline`'s stack from `frame`
    /// on. Returns its id.
    pub fn add_paint_layer(
        &mut self,
        timeline: crate::TimelineId,
        name: impl Into<String>,
        frame: u32,
    ) -> Option<LayerId> {
        let (width, height) = (self.width, self.height);
        let canvas = self.add_canvas(width, height);
        let paint = Element {
            shape: Shape::Paint { canvas, width, height },
            style: crate::Style { fill: None, stroke: None },
        };
        let center = Point::new(width as f32 / 2.0, height as f32 / 2.0);
        let id = self.add_layer_to(timeline, name, vec![paint], frame, crate::Transform::at(center))?;
        if let Some(layer) = self.layer_mut(id) {
            layer.start = frame;
        }
        Some(id)
    }

    /// Every canvas some layer's drawing uses. Others (left behind by a
    /// deleted drawing, say) aren't saved.
    pub fn used_canvases(&self) -> Vec<CanvasId> {
        let mut used: Vec<CanvasId> = self
            .all_layers()
            .flat_map(|layer| layer.all_elements())
            .filter_map(|e| match e.shape {
                Shape::Paint { canvas, .. } => Some(canvas),
                _ => None,
            })
            .collect();
        used.sort_unstable();
        used.dedup();
        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_canvases_have_no_tiles() {
        let canvas = Canvas::new(1920, 1080);
        assert!(canvas.is_empty());
        assert_eq!(canvas.tile_counts(), (8, 5));
        assert_eq!(canvas.pixel(100, 100), [0; 4]);
        assert_eq!(canvas.painted_bounds(), None);
    }

    #[test]
    fn painting_a_pixel_makes_just_its_tile() {
        let mut canvas = Canvas::new(1000, 1000);
        let tile = canvas.tile_mut((1, 2));
        let i = ((10 * TILE + 20) * 4) as usize;
        tile[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        assert_eq!(canvas.tiles().count(), 1);
        assert_eq!(canvas.pixel(256 + 20, 512 + 10), [255, 0, 0, 255]);
        let bounds = canvas.painted_bounds().unwrap();
        assert_eq!((bounds.min, bounds.max), (Point::new(276.0, 522.0), Point::new(277.0, 523.0)));
    }

    #[test]
    fn copies_share_tiles_until_painted() {
        let mut canvas = Canvas::new(512, 512);
        canvas.tile_mut((0, 0))[3] = 255;
        let snapshot = canvas.clone();
        assert_eq!(snapshot, canvas);
        canvas.tile_mut((0, 0))[3] = 128;
        assert_ne!(snapshot, canvas, "the snapshot kept the old pixel");
        assert_eq!(snapshot.pixel(0, 0)[3], 255);
    }

    #[test]
    fn tiles_round_trip_through_png() {
        let mut tile = vec![0u8; TILE_BYTES];
        // Half-transparent red, premultiplied.
        tile[..4].copy_from_slice(&premultiply(&[255, 0, 0, 128]));
        tile[4..8].copy_from_slice(&[10, 20, 30, 255]);
        let decoded = decode_tile(&encode_tile(&tile).unwrap()).unwrap();
        assert_eq!(decoded, tile);
    }

    /// A document with a paint layer that has one red pixel at (300, 10).
    fn painted() -> (Document, LayerId, CanvasId) {
        let mut doc = Document::default();
        let layer = doc.add_paint_layer(crate::TimelineId::Main, "Paint", 0).unwrap();
        let Shape::Paint { canvas, .. } = doc.layer(layer).unwrap().drawings[0].elements[0].shape else {
            unreachable!()
        };
        let i = ((10 * TILE + 300 - TILE) * 4) as usize;
        doc.canvas_mut(canvas).unwrap().tile_mut((1, 0))[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        (doc, layer, canvas)
    }

    fn canvas_of(doc: &Document, layer: LayerId) -> CanvasId {
        match doc.layer(layer).unwrap().drawings[0].elements[0].shape {
            Shape::Paint { canvas, .. } => canvas,
            _ => unreachable!(),
        }
    }

    #[test]
    fn paint_layers_line_up_with_the_stage() {
        let (doc, layer, _) = painted();
        // The red pixel is at stage (300, 10): clicking there hits the layer,
        // clicking an empty part of the canvas doesn't.
        assert!(doc.hits_layer(doc.layer(layer).unwrap(), Point::new(300.5, 10.5), 0, 0.0));
        assert!(!doc.hits_layer(doc.layer(layer).unwrap(), Point::new(900.0, 500.0), 0, 2.0));
        let bounds = doc.layer_bounds(doc.layer(layer).unwrap(), 0).unwrap();
        assert_eq!(bounds.min, Point::new(300.0 - 960.0, 10.0 - 540.0));
    }

    #[test]
    fn duplicates_and_pastes_get_their_own_canvas() {
        let (mut doc, layer, canvas) = painted();
        let copy = doc.duplicate_layer(layer).unwrap();
        assert_ne!(canvas_of(&doc, copy), canvas);
        assert_eq!(doc.canvas(canvas_of(&doc, copy)), doc.canvas(canvas));

        let copied = doc.copy_layers(&[layer], None).unwrap();
        let mut other = Document::default();
        let pasted = other.paste_layers(&copied, crate::TimelineId::Main, Point::ZERO, None)[0];
        let pasted_canvas = other.canvas(canvas_of(&other, pasted)).unwrap();
        assert_eq!(pasted_canvas.pixel(300, 10), [255, 0, 0, 255]);
    }

    #[test]
    fn regions_read_and_write_across_tiles() {
        let mut canvas = Canvas::new(600, 600);
        // A 20×20 block straddling four tiles, partly off the top left.
        let block = vec![9u8; 20 * 20 * 4];
        canvas.write_region(246, 246, 20, 20, &block);
        assert_eq!(canvas.tiles().count(), 4);
        assert_eq!(canvas.read_region(246, 246, 20, 20), block);
        assert_eq!(canvas.pixel(265, 265), [9; 4]);
        // Off the canvas reads as transparent.
        let edge = canvas.read_region(-5, -5, 10, 10);
        assert!(edge.iter().all(|&b| b == 0));
        // Writing transparency clears the tiles away again.
        canvas.write_region(246, 246, 20, 20, &vec![0; 20 * 20 * 4]);
        assert!(canvas.is_empty());
    }

    #[test]
    fn to_pixels_lays_tiles_out_in_rows() {
        let mut canvas = Canvas::new(300, 2);
        canvas.tile_mut((1, 0))[..4].copy_from_slice(&[1, 2, 3, 255]);
        let pixels = canvas.to_pixels();
        assert_eq!(pixels.len(), 300 * 2 * 4);
        assert_eq!(&pixels[256 * 4..256 * 4 + 4], &[1, 2, 3, 255]);
    }
}
