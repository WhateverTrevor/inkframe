//! Sprite sheets: every frame packed into one image, with a data file
//! saying where each frame is, for game engines.
//!
//! The data file is TexturePacker's "JSON (hash)" layout, which Phaser,
//! PixiJS and most Godot and Unity importers read directly. Frames are
//! keyed by name (`walk_0001`, `walk_0002`, …) and `animations` lists them
//! in order, so the engine can play the animation without being told the
//! frame names. Each frame also carries its `duration` in milliseconds, as
//! Aseprite's exports do, for loaders that time frames themselves.
//!
//! Hand-drawn animation holds drawings for several frames (on twos, on
//! threes), so frames that render exactly the same are packed only once
//! and their entries point at the same cell. A walk on twos takes half the
//! texture memory it would otherwise.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::ControlFlow;

use serde_json::{Map, Value, json};
use tiny_skia::{IntRect, Pixmap, PixmapPaint, Transform};

use crate::{ExportError, ExportSettings, FrameRenderer, Progress};

/// The largest texture side to produce. 8192 is what most GPUs, including
/// phones from the last several years, can load; larger sheets fail to
/// load in the engine, which is a worse place to find out.
const MAX_SIDE: u32 = 8192;
/// Empty pixels between cells, so texture filtering at a frame's edge
/// doesn't pick up its neighbour.
const PADDING: u32 = 2;

pub(crate) fn write(
    renderer: &FrameRenderer,
    settings: &ExportSettings,
    fps: f64,
    mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    let frames_total = settings.frames.end() - settings.frames.start() + 1;
    let source = renderer.size();

    // Render everything first: the sheet's size depends on how many
    // distinct frames there are and how much of the stage they cover.
    // Each is kept cropped to its own drawing, which is all that's needed
    // and keeps memory down on long animations.
    let mut cells: Vec<Cell> = Vec::new();
    let mut cell_of_frame = Vec::new();
    // Cells by a hash of their pixels, compared in full on a match.
    let mut seen: HashMap<u64, Vec<usize>> = HashMap::new();
    for (done, frame) in settings.frames.clone().enumerate() {
        let cell = Cell::crop(renderer.render(frame)?);
        let hash = {
            let mut hasher = DefaultHasher::new();
            cell.bounds.hash(&mut hasher);
            cell.pixels.as_ref().map(|p| p.data()).hash(&mut hasher);
            hasher.finish()
        };
        let candidates = seen.entry(hash).or_default();
        let index = match candidates.iter().find(|&&c| cells[c] == cell) {
            Some(&index) => index,
            None => {
                cells.push(cell);
                candidates.push(cells.len() - 1);
                cells.len() - 1
            }
        };
        cell_of_frame.push(index);
        if on_progress(Progress { frames_done: done as u32 + 1, frames_total }).is_break() {
            // Nothing has been written yet.
            return Err(ExportError::Cancelled);
        }
    }

    // Every cell is the same size, the area the whole animation covers, so
    // frames stay lined up with each other when played.
    let used = cells
        .iter()
        .filter(|c| c.pixels.is_some())
        .map(|c| c.bounds)
        .reduce(Area::union)
        .unwrap_or(Area { x: 0, y: 0, w: 1, h: 1 });
    let (w, h) = (used.w, used.h);
    let grid = Grid::fit(cells.len() as u32, w, h).ok_or(ExportError::SheetTooLarge(MAX_SIDE))?;
    let (sheet_w, sheet_h) = grid.size(w, h);
    let mut sheet = Pixmap::new(sheet_w, sheet_h).ok_or(ExportError::FrameTooLarge(sheet_w, sheet_h))?;
    for (i, cell) in cells.iter().enumerate() {
        let Some(pixels) = &cell.pixels else { continue };
        let (x, y) = grid.origin(i as u32, w, h);
        let (x, y) = (x + cell.bounds.x - used.x, y + cell.bounds.y - used.y);
        sheet.draw_pixmap(
            x as i32,
            y as i32,
            pixels.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }

    let stem = settings.output.file_stem().and_then(|s| s.to_str()).unwrap_or("sheet").to_owned();
    let image = settings.output.with_extension("png");
    let data = settings.output.with_extension("json");
    let image_name = image.file_name().and_then(|s| s.to_str()).unwrap_or("sheet.png");
    let layout = Layout { grid, used, source };
    let json = sheet_data(&stem, image_name, &layout, &cell_of_frame, settings.scale, fps);

    let png = sheet.encode_png().map_err(|e| ExportError::Png(e.to_string()))?;
    std::fs::write(&image, png)?;
    if let Err(error) = std::fs::write(&data, serde_json::to_string_pretty(&json).unwrap_or_default()) {
        // Half a sprite sheet is no use to an engine.
        let _ = std::fs::remove_file(&image);
        return Err(error.into());
    }
    Ok(())
}

/// Where the frames went on the sheet.
struct Layout {
    grid: Grid,
    /// The area of the stage every cell shows.
    used: Area,
    /// The size of a whole frame.
    source: (u32, u32),
}

/// A rectangle of pixels within a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Area {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Area {
    fn union(self, other: Area) -> Area {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Area { x, y, w: right - x, h: bottom - y }
    }
}

/// One distinct frame, cropped to what's drawn in it.
#[derive(Debug, PartialEq)]
struct Cell {
    bounds: Area,
    /// `None` for a frame with nothing in it.
    pixels: Option<Pixmap>,
}

impl Cell {
    fn crop(frame: Pixmap) -> Cell {
        let empty = Cell { bounds: Area { x: 0, y: 0, w: 0, h: 0 }, pixels: None };
        let Some(bounds) = drawn_area(&frame) else { return empty };
        if (bounds.w, bounds.h) == (frame.width(), frame.height()) {
            return Cell { bounds, pixels: Some(frame) };
        }
        let rect = IntRect::from_xywh(bounds.x as i32, bounds.y as i32, bounds.w, bounds.h);
        match rect.and_then(|r| frame.clone_rect(r)) {
            Some(pixels) => Cell { bounds, pixels: Some(pixels) },
            None => empty,
        }
    }
}

/// The smallest area holding every pixel that isn't fully transparent.
/// A frame with its background drawn is all drawn, so isn't cropped.
fn drawn_area(frame: &Pixmap) -> Option<Area> {
    let width = frame.width() as usize;
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for (i, pixel) in frame.pixels().iter().enumerate() {
        if pixel.alpha() > 0 {
            let (x, y) = (i % width, i / width);
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
    }
    (left <= right).then(|| Area {
        x: left as u32,
        y: top as u32,
        w: (right - left + 1) as u32,
        h: (bottom - top + 1) as u32,
    })
}

/// Where cells go: in rows, left to right, `PADDING` apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Grid {
    pub columns: u32,
    pub rows: u32,
}

impl Grid {
    /// The grid for `count` cells of `w`×`h` closest to square (squarer
    /// sheets waste less of a texture), or `None` if they can't fit within
    /// [`MAX_SIDE`] either way.
    pub fn fit(count: u32, w: u32, h: u32) -> Option<Self> {
        let count = count.max(1);
        let fits = |n: u32, side: u32| n * side + n.saturating_sub(1) * PADDING <= MAX_SIDE;
        let most_columns = (1..=count).take_while(|&c| fits(c, w)).last()?;
        // As many columns as makes the sheet about as wide as it is tall.
        let square = ((count as f32 * h as f32 / w as f32).sqrt().ceil() as u32).clamp(1, most_columns);
        let columns = count.div_ceil(count.div_ceil(square));
        let rows = count.div_ceil(columns);
        fits(rows, h).then_some(Self { columns, rows })
    }

    pub fn size(&self, w: u32, h: u32) -> (u32, u32) {
        let side = |n: u32, cell: u32| n * cell + (n - 1) * PADDING;
        (side(self.columns, w), side(self.rows, h))
    }

    pub fn origin(&self, cell: u32, w: u32, h: u32) -> (u32, u32) {
        let (column, row) = (cell % self.columns, cell / self.columns);
        (column * (w + PADDING), row * (h + PADDING))
    }
}

/// The JSON describing the sheet, frame by frame.
fn sheet_data(
    stem: &str,
    image: &str,
    &Layout { grid, used, source: (source_w, source_h) }: &Layout,
    cell_of_frame: &[usize],
    scale: f32,
    fps: f64,
) -> Value {
    let (w, h) = (used.w, used.h);
    // Engines put a trimmed frame back where it was on the stage from
    // `spriteSourceSize`, so the sprite's origin stays the stage's corner.
    let trimmed = (used.x, used.y, w, h) != (0, 0, source_w, source_h);
    let digits = cell_of_frame.len().to_string().len().max(4);
    let duration = (1000.0 / fps.max(1.0)).round() as u32;
    let names: Vec<String> = (1..=cell_of_frame.len()).map(|n| format!("{stem}_{n:0digits$}")).collect();
    let mut frames = Map::new();
    for (name, &cell) in names.iter().zip(cell_of_frame) {
        let (x, y) = grid.origin(cell as u32, w, h);
        frames.insert(
            name.clone(),
            json!({
                "frame": { "x": x, "y": y, "w": w, "h": h },
                "rotated": false,
                "trimmed": trimmed,
                "spriteSourceSize": { "x": used.x, "y": used.y, "w": w, "h": h },
                "sourceSize": { "w": source_w, "h": source_h },
                "duration": duration,
            }),
        );
    }
    let (sheet_w, sheet_h) = grid.size(w, h);
    json!({
        "frames": frames,
        "animations": { stem: names },
        "meta": {
            "app": "Inkframe",
            "version": env!("CARGO_PKG_VERSION"),
            "image": image,
            "format": "RGBA8888",
            "size": { "w": sheet_w, "h": sheet_h },
            // TexturePacker writes the scale as a string; loaders expect that.
            "scale": scale.to_string(),
            "frameRate": fps,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_come_out_about_square() {
        assert_eq!(Grid::fit(1, 100, 100), Some(Grid { columns: 1, rows: 1 }));
        assert_eq!(Grid::fit(9, 100, 100), Some(Grid { columns: 3, rows: 3 }));
        // Wide frames stack in fewer columns.
        let wide = Grid::fit(16, 400, 100).unwrap();
        let (w, h) = wide.size(400, 100);
        assert!(w.abs_diff(h) < 500, "{wide:?}");
        // No empty last row.
        let ten = Grid::fit(10, 100, 100).unwrap();
        assert!(ten.columns * (ten.rows - 1) < 10);
    }

    #[test]
    fn too_many_frames_for_one_texture_is_an_error() {
        assert!(Grid::fit(200, 1920, 1080).is_none());
        assert!(Grid::fit(1, MAX_SIDE + 1, 10).is_none());
    }

    #[test]
    fn cells_are_padded_apart() {
        let grid = Grid { columns: 3, rows: 2 };
        assert_eq!(grid.origin(4, 10, 20), (10 + PADDING, 20 + PADDING));
        assert_eq!(grid.size(10, 20), (30 + 2 * PADDING, 40 + PADDING));
    }

    #[test]
    fn held_frames_share_a_cell_in_the_data() {
        let grid = Grid { columns: 2, rows: 1 };
        let used = Area { x: 5, y: 0, w: 10, h: 10 };
        let layout = Layout { grid, used, source: (20, 10) };
        let data = sheet_data("walk", "walk.png", &layout, &[0, 0, 1], 1.0, 24.0);
        let frame = |name: &str| data["frames"][name]["frame"]["x"].as_u64().unwrap();
        assert_eq!(frame("walk_0001"), frame("walk_0002"));
        assert_eq!(frame("walk_0003"), 10 + PADDING as u64);
        assert_eq!(data["animations"]["walk"].as_array().unwrap().len(), 3);
        assert_eq!(data["frames"]["walk_0001"]["duration"], 42);
        assert_eq!(data["meta"]["image"], "walk.png");
        assert_eq!(data["frames"]["walk_0001"]["trimmed"], true);
        assert_eq!(data["frames"]["walk_0001"]["spriteSourceSize"]["x"], 5);
        assert_eq!(data["frames"]["walk_0001"]["sourceSize"]["w"], 20);
    }

    #[test]
    fn frames_are_cropped_to_their_drawing() {
        let mut frame = Pixmap::new(20, 10).unwrap();
        frame.fill_rect(
            tiny_skia::Rect::from_xywh(4.0, 2.0, 3.0, 5.0).unwrap(),
            &tiny_skia::Paint::default(),
            Transform::identity(),
            None,
        );
        let cell = Cell::crop(frame);
        assert_eq!(cell.bounds, Area { x: 4, y: 2, w: 3, h: 5 });
        assert_eq!(cell.pixels.unwrap().width(), 3);
        assert!(Cell::crop(Pixmap::new(4, 4).unwrap()).pixels.is_none());
    }
}
