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
//!
//! When the frames don't fit on one sheet of [`MAX_SIDE`], they're shared
//! out over as few sheets as they need, as TexturePacker's "multipack"
//! does: `walk-0.png` with `walk-0.json`, `walk-1.png` with `walk-1.json`,
//! and so on. Each data file describes its own sheet in the same layout,
//! listing the frames on it (named for the whole animation, so they play
//! in order across sheets), and names the others in
//! `meta.related_multi_packs`, which PixiJS reads to load them together.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::ControlFlow;
use std::path::Path;

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
    on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    write_sheets(renderer, settings, fps, MAX_SIDE, on_progress)
}

/// [`write`], with sheets no bigger than `max_side` (smaller in tests,
/// which would otherwise need huge frames to fill several sheets).
fn write_sheets(
    renderer: &FrameRenderer,
    settings: &ExportSettings,
    fps: f64,
    max_side: u32,
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
    // Only a single frame bigger than a sheet can't be packed at all.
    let sheets = pack(cells.len() as u32, w, h, max_side).ok_or(ExportError::SheetTooLarge(max_side))?;

    let stem = settings.output.file_stem().and_then(|s| s.to_str()).unwrap_or("sheet").to_owned();
    let dir = settings.output.parent().map(Path::to_path_buf).unwrap_or_default();
    // One sheet keeps the name chosen; several are numbered from 0, as
    // TexturePacker numbers them.
    let name = |n: usize| if sheets.len() == 1 { stem.clone() } else { format!("{stem}-{n}") };
    let names: Vec<String> = (0..sheets.len()).map(name).collect();

    let mut written = Vec::new();
    let result = (|| {
        for (n, sheet) in sheets.iter().enumerate() {
            let pixels = draw_sheet(sheet, &cells, used)?;
            let image = dir.join(format!("{}.png", names[n]));
            let data = dir.join(format!("{}.json", names[n]));
            let others: Vec<String> = names
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != n)
                .map(|(_, name)| format!("{name}.json"))
                .collect();
            let layout = Layout { sheet: *sheet, used, source };
            let json = sheet_data(
                &stem,
                &format!("{}.png", names[n]),
                &layout,
                &cell_of_frame,
                &others,
                settings.scale,
                fps,
            );
            let png = pixels.encode_png().map_err(|e| ExportError::Png(e.to_string()))?;
            std::fs::write(&image, png)?;
            written.push(image);
            std::fs::write(&data, serde_json::to_string_pretty(&json).unwrap_or_default())?;
            written.push(data);
        }
        Ok(())
    })();
    if result.is_err() {
        // Part of a set of sprite sheets is no use to an engine.
        for path in written {
            let _ = std::fs::remove_file(path);
        }
    }
    result
}

/// Draws one sheet's cells onto it.
fn draw_sheet(sheet: &Sheet, cells: &[Cell], used: Area) -> Result<Pixmap, ExportError> {
    let (w, h) = (used.w, used.h);
    let (sheet_w, sheet_h) = sheet.grid.size(w, h);
    let mut pixmap = Pixmap::new(sheet_w, sheet_h).ok_or(ExportError::FrameTooLarge(sheet_w, sheet_h))?;
    for (i, cell) in cells.iter().enumerate().skip(sheet.first as usize).take(sheet.count as usize) {
        let Some(pixels) = &cell.pixels else { continue };
        let (x, y) = sheet.grid.origin(i as u32 - sheet.first, w, h);
        let (x, y) = (x + cell.bounds.x - used.x, y + cell.bounds.y - used.y);
        pixmap.draw_pixmap(
            x as i32,
            y as i32,
            pixels.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    Ok(pixmap)
}

/// One sheet of a set: which cells are on it (a run of them, from
/// `first`), and how they're laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sheet {
    first: u32,
    count: u32,
    grid: Grid,
}

impl Sheet {
    fn holds(&self, cell: usize) -> bool {
        (self.first..self.first + self.count).contains(&(cell as u32))
    }
}

/// Shares `count` cells of `w`×`h` out over as few sheets no bigger than
/// `max_side` as will hold them, about evenly, or `None` if a single cell
/// is too big for one.
fn pack(count: u32, w: u32, h: u32, max_side: u32) -> Option<Vec<Sheet>> {
    let count = count.max(1);
    let most = |side: u32| (max_side + PADDING) / (side + PADDING);
    let room = most(w) * most(h);
    if room == 0 {
        return None;
    }
    let sheets = count.div_ceil(room);
    // Evenly, so the last sheet isn't a sliver.
    let each = count.div_ceil(sheets);
    (0..sheets)
        .map(|n| {
            let first = n * each;
            let count = each.min(count - first);
            Grid::fit(count, w, h, max_side).map(|grid| Sheet { first, count, grid })
        })
        .collect()
}

/// Where the frames went on one sheet.
struct Layout {
    sheet: Sheet,
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
    /// `max_side` either way.
    pub fn fit(count: u32, w: u32, h: u32, max_side: u32) -> Option<Self> {
        let count = count.max(1);
        let fits = |n: u32, side: u32| n * side + n.saturating_sub(1) * PADDING <= max_side;
        let most_columns = (1..=count).take_while(|&c| fits(c, w)).last()?;
        // As many columns as makes the sheet about as wide as it is tall.
        let square = ((count as f32 * h as f32 / w as f32).sqrt().ceil() as u32).clamp(1, most_columns);
        let columns = count.div_ceil(count.div_ceil(square));
        let rows = count.div_ceil(columns);
        if fits(rows, h) {
            return Some(Self { columns, rows });
        }
        // Too tall that way: as wide as the sheet allows, for the fewest rows.
        let rows = count.div_ceil(most_columns);
        fits(rows, h).then_some(Self { columns: most_columns, rows })
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

/// The JSON describing one sheet, frame by frame: the frames on it, and
/// the data files of the other sheets in the set (`others`).
fn sheet_data(
    stem: &str,
    image: &str,
    &Layout { sheet, used, source: (source_w, source_h) }: &Layout,
    cell_of_frame: &[usize],
    others: &[String],
    scale: f32,
    fps: f64,
) -> Value {
    let grid = sheet.grid;
    let (w, h) = (used.w, used.h);
    // Engines put a trimmed frame back where it was on the stage from
    // `spriteSourceSize`, so the sprite's origin stays the stage's corner.
    let trimmed = (used.x, used.y, w, h) != (0, 0, source_w, source_h);
    let digits = cell_of_frame.len().to_string().len().max(4);
    let duration = (1000.0 / fps.max(1.0)).round() as u32;
    // Frames are numbered through the whole animation, whichever sheet
    // they're on, so they play in order across a set.
    let names: Vec<String> = (1..=cell_of_frame.len())
        .zip(cell_of_frame)
        .filter(|&(_, &cell)| sheet.holds(cell))
        .map(|(n, _)| format!("{stem}_{n:0digits$}"))
        .collect();
    let on_sheet = cell_of_frame.iter().filter(|&&cell| sheet.holds(cell));
    let mut frames = Map::new();
    for (name, &cell) in names.iter().zip(on_sheet) {
        let (x, y) = grid.origin(cell as u32 - sheet.first, w, h);
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
    let mut meta = json!({
        "app": "Inkframe",
        "version": env!("CARGO_PKG_VERSION"),
        "image": image,
        "format": "RGBA8888",
        "size": { "w": sheet_w, "h": sheet_h },
        // TexturePacker writes the scale as a string; loaders expect that.
        "scale": scale.to_string(),
        "frameRate": fps,
    });
    if !others.is_empty() {
        meta["related_multi_packs"] = json!(others);
    }
    json!({
        "frames": frames,
        "animations": { stem: names },
        "meta": meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_come_out_about_square() {
        assert_eq!(Grid::fit(1, 100, 100, MAX_SIDE), Some(Grid { columns: 1, rows: 1 }));
        assert_eq!(Grid::fit(9, 100, 100, MAX_SIDE), Some(Grid { columns: 3, rows: 3 }));
        // Wide frames stack in fewer columns.
        let wide = Grid::fit(16, 400, 100, MAX_SIDE).unwrap();
        let (w, h) = wide.size(400, 100);
        assert!(w.abs_diff(h) < 500, "{wide:?}");
        // No empty last row.
        let ten = Grid::fit(10, 100, 100, MAX_SIDE).unwrap();
        assert!(ten.columns * (ten.rows - 1) < 10);
    }

    #[test]
    fn frames_too_many_for_one_sheet_are_shared_out_over_several() {
        // Everything fits on one.
        let one = pack(10, 100, 100, MAX_SIDE).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!((one[0].first, one[0].count), (0, 10));
        // 200 full-HD frames: 7 × 4 to a sheet of 8192, so 8 sheets of 25.
        let many = pack(200, 1920, 1080, MAX_SIDE).unwrap();
        assert_eq!(many.len(), 8);
        assert!(many.iter().all(|sheet| sheet.count == 25));
        assert_eq!(many.iter().map(|s| s.count).sum::<u32>(), 200);
        for sheet in &many {
            let (w, h) = sheet.grid.size(1920, 1080);
            assert!(w <= MAX_SIDE && h <= MAX_SIDE, "{sheet:?}");
            assert!(sheet.grid.columns * sheet.grid.rows >= sheet.count);
        }
        // Four to a sheet, but five are shared out evenly, the runs following on.
        let odd = pack(5, 10, 10, 22).unwrap();
        assert_eq!(odd.iter().map(|s| (s.first, s.count)).collect::<Vec<_>>(), [(0, 3), (3, 2)]);
        // A single frame bigger than a sheet can't go anywhere.
        assert!(pack(1, MAX_SIDE + 1, 10, MAX_SIDE).is_none());
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
        let layout = Layout { sheet: Sheet { first: 0, count: 2, grid }, used, source: (20, 10) };
        let data = sheet_data("walk", "walk.png", &layout, &[0, 0, 1], &[], 1.0, 24.0);
        let frame = |name: &str| data["frames"][name]["frame"]["x"].as_u64().unwrap();
        assert_eq!(frame("walk_0001"), frame("walk_0002"));
        assert_eq!(frame("walk_0003"), 10 + PADDING as u64);
        assert_eq!(data["animations"]["walk"].as_array().unwrap().len(), 3);
        assert_eq!(data["frames"]["walk_0001"]["duration"], 42);
        assert_eq!(data["meta"]["image"], "walk.png");
        assert_eq!(data["frames"]["walk_0001"]["trimmed"], true);
        assert_eq!(data["frames"]["walk_0001"]["spriteSourceSize"]["x"], 5);
        assert_eq!(data["frames"]["walk_0001"]["sourceSize"]["w"], 20);
        assert!(data["meta"].get("related_multi_packs").is_none());
    }

    #[test]
    fn each_sheet_of_a_set_lists_its_own_frames_and_the_others() {
        let grid = Grid { columns: 2, rows: 1 };
        let used = Area { x: 0, y: 0, w: 10, h: 10 };
        // Frames 1-2 on cells 0 and 1 (the first sheet), 3-4 on cell 2 (the second).
        let second = Layout { sheet: Sheet { first: 2, count: 1, grid }, used, source: (10, 10) };
        let data =
            sheet_data("walk", "walk-1.png", &second, &[0, 1, 2, 2], &["walk-0.json".into()], 1.0, 24.0);
        let names: Vec<&str> =
            data["animations"]["walk"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
        assert_eq!(names, ["walk_0003", "walk_0004"]);
        assert_eq!(data["frames"]["walk_0003"]["frame"]["x"], 0);
        assert_eq!(data["meta"]["image"], "walk-1.png");
        assert_eq!(data["meta"]["related_multi_packs"][0], "walk-0.json");
    }

    #[test]
    fn frames_too_big_for_one_sheet_are_written_to_several() {
        use inkframe_core::{Color, Document, Element, Point, Shape, Style};

        // A square sliding across a 64 × 36 stage, its background drawn so
        // nothing is cropped: with sheets 80 pixels square, two frames go on
        // each, so three different frames need two sheets.
        let mut doc = Document::default();
        (doc.width, doc.height, doc.duration) = (64, 36, 4);
        let square = Element {
            shape: Shape::Rect { width: 16.0, height: 16.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let id =
            doc.add_layer("Square", vec![square], 0, inkframe_core::Transform::at(Point::new(10.0, 18.0)));
        doc.layer_mut(id).unwrap().track.set_pose(2, inkframe_core::Transform::at(Point::new(54.0, 18.0)));
        let dir = std::env::temp_dir().join(format!("inkframe-multipack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = ExportSettings {
            format: crate::ExportFormat::SpriteSheet,
            output: dir.join("walk.png"),
            frames: 0..=3,
            scale: 1.0,
            transparent: false,
            project_dir: None,
        };
        let images = inkframe_render::ImageLibrary::load(&doc, None);
        let renderer = FrameRenderer::new(&doc, &images, &settings);
        write_sheets(&renderer, &settings, 24.0, 80, |_| ControlFlow::Continue(())).unwrap();

        assert!(!dir.join("walk.png").exists());
        // Shown to the person as the first sheet.
        assert_eq!(
            crate::exported_file(crate::ExportFormat::SpriteSheet, &dir.join("walk.png")),
            dir.join("walk-0.png")
        );
        let read = |name: &str| -> Value {
            serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap()
        };
        let (first, second) = (read("walk-0.json"), read("walk-1.json"));
        assert_eq!(first["meta"]["image"], "walk-0.png");
        assert_eq!(first["meta"]["related_multi_packs"][0], "walk-1.json");
        assert_eq!(second["meta"]["related_multi_packs"][0], "walk-0.json");
        let frames = |data: &Value| data["animations"]["walk"].as_array().unwrap().len();
        // Frames 1 and 2 on the first sheet; 3, and 4 holding it, on the second.
        assert_eq!((frames(&first), frames(&second)), (2, 2));
        assert_eq!(second["frames"]["walk_0003"]["frame"], second["frames"]["walk_0004"]["frame"]);
        for (data, image) in [(&first, "walk-0.png"), (&second, "walk-1.png")] {
            let sheet = Pixmap::load_png(dir.join(image)).unwrap();
            assert!(sheet.width() <= 80 && sheet.height() <= 80);
            assert_eq!(data["meta"]["size"]["w"], sheet.width());
        }
        std::fs::remove_dir_all(dir).unwrap();
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
