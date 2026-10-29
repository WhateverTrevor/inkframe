//! Sprite sheets in: the frames of an animation packed into one picture
//! (or several), turned back into a paint layer to edit, a drawing per
//! frame. The export side is in `inkframe-export`.
//!
//! Where the frames are comes from the sheet's data file when there is
//! one: TexturePacker's JSON, as a hash keyed by frame name or as an
//! array, which is also what Aseprite, Inkframe's own export and most game
//! tools write. A frame may be *trimmed* (packed without its empty border,
//! with `spriteSourceSize` saying where it sat in the full `sourceSize`)
//! or *rotated* (packed turned a quarter clockwise to fit better); both
//! are undone, so every drawing is the full frame as it was drawn. Without
//! a data file, the picture is cut into a grid of equal cells.

use serde_json::Value;

use crate::document::{Document, Element, LayerId};
use crate::drawing::{Drawing, Exposure};
use crate::geometry::Point;
use crate::shape::{Shape, Style};
use crate::{TimelineId, Transform};

/// One frame of a sprite sheet: where it's packed, and how it goes back.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteFrame {
    /// Which of the sheets it's on (several, for a multipack set).
    pub sheet: usize,
    /// Its place on the sheet, and its size as drawn (before any turning).
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Packed turned a quarter clockwise.
    pub rotated: bool,
    /// Where it sits in the full frame it was trimmed from.
    pub offset: (u32, u32),
    /// The full frame's size.
    pub source: (u32, u32),
    /// How long it shows, if the data says.
    pub duration_ms: Option<f32>,
}

/// What a sheet's data file says.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheetData {
    /// The picture it describes, by file name.
    pub image: Option<String>,
    pub frame_rate: Option<f64>,
    /// The frames in the order they play.
    pub frames: Vec<SpriteFrame>,
    /// The other data files of a multipack set.
    pub related: Vec<String>,
}

/// A sheet's pixels: straight (not premultiplied) RGBA, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpriteError {
    #[error("the data file isn't JSON: {0}")]
    NotJson(String),
    #[error("the data file lists no frames")]
    NoFrames,
}

/// Reads a sheet's data file. `sheet` numbers the picture it goes with,
/// for a multipack set read sheet by sheet.
pub fn read_sheet_data(json: &str, sheet: usize) -> Result<SheetData, SpriteError> {
    let data: Value = serde_json::from_str(json).map_err(|e| SpriteError::NotJson(e.to_string()))?;
    let uint = |v: &Value| v.as_u64().unwrap_or(0) as u32;
    let frame_of = |entry: &Value| -> Option<SpriteFrame> {
        let rect = entry.get("frame")?;
        let (width, height) = (uint(&rect["w"]), uint(&rect["h"]));
        if width == 0 || height == 0 {
            return None;
        }
        let trim = &entry["spriteSourceSize"];
        let source = &entry["sourceSize"];
        Some(SpriteFrame {
            sheet,
            x: uint(&rect["x"]),
            y: uint(&rect["y"]),
            width,
            height,
            rotated: entry["rotated"].as_bool().unwrap_or(false),
            offset: (uint(&trim["x"]), uint(&trim["y"])),
            source: if source.is_object() {
                (uint(&source["w"]), uint(&source["h"]))
            } else {
                (width, height)
            },
            duration_ms: entry["duration"].as_f64().map(|d| d as f32),
        })
    };
    // Frames by name, in the order the file has them.
    let named: Vec<(String, SpriteFrame)> = match &data["frames"] {
        Value::Object(map) => {
            let mut named: Vec<_> =
                map.iter().filter_map(|(name, entry)| Some((name.clone(), frame_of(entry)?))).collect();
            // A hash's keys come in no set order; frames are numbered.
            named.sort_by(|a, b| natural_order(&a.0, &b.0));
            named
        }
        Value::Array(list) => list
            .iter()
            .filter_map(|entry| {
                Some((entry["filename"].as_str().unwrap_or_default().to_owned(), frame_of(entry)?))
            })
            .collect(),
        _ => Vec::new(),
    };
    if named.is_empty() {
        return Err(SpriteError::NoFrames);
    }
    // An animation listed by name plays in its own order (the first, if
    // there are several); otherwise every frame plays, in order.
    let listed: Option<Vec<SpriteFrame>> = data["animations"].as_object().and_then(|animations| {
        let names = animations.values().next()?.as_array()?;
        let frames: Vec<SpriteFrame> = names
            .iter()
            .filter_map(|name| {
                named.iter().find(|(n, _)| Some(n.as_str()) == name.as_str()).map(|(_, f)| f.clone())
            })
            .collect();
        (!frames.is_empty()).then_some(frames)
    });
    let meta = &data["meta"];
    Ok(SheetData {
        image: meta["image"].as_str().map(str::to_owned),
        frame_rate: meta["frameRate"].as_f64().filter(|fps| *fps > 0.0),
        frames: listed.unwrap_or_else(|| named.into_iter().map(|(_, f)| f).collect()),
        related: meta["related_multi_packs"]
            .as_array()
            .map(|names| names.iter().filter_map(|n| Some(n.as_str()?.to_owned())).collect())
            .unwrap_or_default(),
    })
}

/// A multipack set's sheets read together: the frames of all of them,
/// in the order they play. Their names number them through the whole
/// animation, so they're put in order by name across the sheets.
pub fn merge_sheets(sheets: Vec<SheetData>) -> SheetData {
    let mut merged = SheetData::default();
    for sheet in sheets {
        merged.image = merged.image.or(sheet.image);
        merged.frame_rate = merged.frame_rate.or(sheet.frame_rate);
        merged.frames.extend(sheet.frames);
    }
    merged
}

/// Names compared with their numbers as numbers: `walk_2` before `walk_10`.
fn natural_order(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| {
        let digits = s.len() - s.chars().rev().take_while(char::is_ascii_digit).count();
        (s[..digits].to_owned(), s[digits..].parse::<u64>().ok())
    };
    split(a).cmp(&split(b)).then_with(|| a.cmp(b))
}

/// A picture with no data file cut into `columns` × `rows` equal cells,
/// left to right and top to bottom. Pixels left over at the right and
/// bottom (a sheet not quite a multiple of its cells) are left out.
pub fn grid_frames(width: u32, height: u32, columns: u32, rows: u32) -> Vec<SpriteFrame> {
    let (columns, rows) = (columns.max(1), rows.max(1));
    let (cell_w, cell_h) = (width / columns, height / rows);
    if cell_w == 0 || cell_h == 0 {
        return Vec::new();
    }
    (0..rows)
        .flat_map(|row| (0..columns).map(move |column| (column, row)))
        .map(|(column, row)| SpriteFrame {
            sheet: 0,
            x: column * cell_w,
            y: row * cell_h,
            width: cell_w,
            height: cell_h,
            rotated: false,
            offset: (0, 0),
            source: (cell_w, cell_h),
            duration_ms: None,
        })
        .collect()
}

/// A guess at a bare sheet's grid: square cells as tall as the picture
/// (a strip of frames in a row) or as wide (a column), or else one cell.
pub fn guess_grid(width: u32, height: u32) -> (u32, u32) {
    if height > 0 && width > height && width % height == 0 {
        (width / height, 1)
    } else if width > 0 && height > width && height % width == 0 {
        (1, height / width)
    } else {
        (1, 1)
    }
}

impl SheetImage {
    /// The pixel at `(x, y)`, or clear past the edge.
    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let at = ((y * self.width + x) * 4) as usize;
        self.pixels[at..at + 4].try_into().expect("four bytes")
    }

    /// Whether a frame has nothing in it, as a grid's spare cells don't.
    pub fn is_empty(&self, frame: &SpriteFrame) -> bool {
        let (w, h) = if frame.rotated { (frame.height, frame.width) } else { (frame.width, frame.height) };
        (0..h).all(|y| (0..w).all(|x| self.pixel(frame.x + x, frame.y + y)[3] == 0))
    }

    /// A frame's pixels, premultiplied, as drawn: turned back if it was
    /// packed turned, and in its place in the full frame.
    fn frame_pixels(&self, frame: &SpriteFrame) -> Vec<u8> {
        let (full_w, full_h) = (frame.source.0.max(1), frame.source.1.max(1));
        let mut out = vec![0u8; (full_w * full_h * 4) as usize];
        for y in 0..frame.height {
            for x in 0..frame.width {
                // Packed a quarter turn clockwise, the frame's left column
                // runs along the top of its packed place, right to left.
                let on_sheet = if frame.rotated {
                    self.pixel(frame.x + frame.height - 1 - y, frame.y + x)
                } else {
                    self.pixel(frame.x + x, frame.y + y)
                };
                let (to_x, to_y) = (frame.offset.0 + x, frame.offset.1 + y);
                if to_x < full_w && to_y < full_h {
                    let at = ((to_y * full_w + to_x) * 4) as usize;
                    out[at..at + 4].copy_from_slice(&crate::premultiply(&on_sheet));
                }
            }
        }
        out
    }
}

impl Document {
    /// Adds a paint layer of a sprite sheet's frames to `timeline` from
    /// `frame` on, a drawing per frame, each held for its number of
    /// timeline frames in `holds`. Frames packed once and listed again (as
    /// a sheet of an animation on twos does) share one drawing. The layer
    /// sits in the middle of the stage. Returns `None` if there's nothing
    /// to add.
    pub fn add_sprite_layer(
        &mut self,
        timeline: TimelineId,
        name: &str,
        frame: u32,
        sheets: &[SheetImage],
        frames: &[SpriteFrame],
        holds: &[u32],
    ) -> Option<LayerId> {
        let mut drawings: Vec<Drawing> = Vec::new();
        let mut packed: Vec<&SpriteFrame> = Vec::new();
        let mut exposure = Vec::new();
        let mut at = frame;
        for (i, sprite) in frames.iter().enumerate() {
            let Some(sheet) = sheets.get(sprite.sheet) else { continue };
            let drawing = match packed.iter().position(|p| *p == sprite) {
                Some(same) => same,
                None => {
                    let (w, h) = (sprite.source.0.max(1), sprite.source.1.max(1));
                    let canvas = self.add_canvas(w, h);
                    if let Some(c) = self.canvas_mut(canvas) {
                        c.write_region(0, 0, w, h, &sheet.frame_pixels(sprite));
                    }
                    let paint = Element {
                        shape: Shape::Paint { canvas, width: w, height: h },
                        style: Style { fill: None, stroke: None, gradient: None },
                    };
                    drawings.push(Drawing::new((drawings.len() + 1).to_string(), vec![paint]));
                    packed.push(sprite);
                    drawings.len() - 1
                }
            };
            exposure.push(Exposure { frame: at, drawing });
            at += holds.get(i).copied().unwrap_or(1).max(1);
        }
        if drawings.is_empty() {
            return None;
        }
        let centre = Point::new(self.width as f32 / 2.0, self.height as f32 / 2.0);
        let id = self.add_layer_to(timeline, name, Vec::new(), frame, Transform::at(centre))?;
        let layer = self.layer_mut(id)?;
        layer.drawings = drawings;
        layer.exposure = exposure;
        layer.start = frame;
        // The layer lasts as long as the frames do; the timeline grows to
        // fit it if it has to.
        layer.end = Some(at - 1);
        if timeline == TimelineId::Main && self.duration < at {
            self.duration = at;
        }
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet `width` × `height`, each pixel a colour made from where it is.
    fn sheet(width: u32, height: u32) -> SheetImage {
        let pixels =
            (0..height).flat_map(|y| (0..width).flat_map(move |x| [x as u8, y as u8, 7, 255])).collect();
        SheetImage { width, height, pixels }
    }

    #[test]
    fn a_hash_data_file_reads_in_numbered_order_with_trims() {
        let json = r#"{
            "frames": {
                "walk_10": { "frame": {"x":0,"y":0,"w":4,"h":4}, "rotated": false, "trimmed": true,
                             "spriteSourceSize": {"x":2,"y":1,"w":4,"h":4}, "sourceSize": {"w":8,"h":8}, "duration": 125 },
                "walk_2":  { "frame": {"x":4,"y":0,"w":4,"h":4}, "rotated": false, "trimmed": false,
                             "spriteSourceSize": {"x":0,"y":0,"w":4,"h":4}, "sourceSize": {"w":4,"h":4} }
            },
            "meta": { "image": "walk.png", "frameRate": 8 }
        }"#;
        let data = read_sheet_data(json, 0).unwrap();
        assert_eq!(data.image.as_deref(), Some("walk.png"));
        assert_eq!(data.frame_rate, Some(8.0));
        assert_eq!(data.frames.len(), 2);
        assert_eq!(data.frames[0].x, 4, "walk_2 before walk_10");
        assert_eq!((data.frames[1].offset, data.frames[1].source), ((2, 1), (8, 8)));
        assert_eq!(data.frames[1].duration_ms, Some(125.0));
        assert!(read_sheet_data("{}", 0).is_err());
    }

    #[test]
    fn an_animation_list_and_an_array_set_the_order() {
        let json = r#"{
            "frames": [
                { "filename": "a", "frame": {"x":0,"y":0,"w":2,"h":2} },
                { "filename": "b", "frame": {"x":2,"y":0,"w":2,"h":2} }
            ],
            "animations": { "run": ["b", "a", "b"] }
        }"#;
        let xs: Vec<u32> = read_sheet_data(json, 0).unwrap().frames.iter().map(|f| f.x).collect();
        assert_eq!(xs, [2, 0, 2]);
    }

    #[test]
    fn grids_are_cut_and_guessed() {
        assert_eq!(grid_frames(64, 16, 4, 1).len(), 4);
        assert_eq!(grid_frames(64, 16, 4, 1)[3].x, 48);
        assert_eq!(guess_grid(64, 16), (4, 1));
        assert_eq!(guess_grid(16, 48), (1, 3));
        assert_eq!(guess_grid(30, 20), (1, 1));
    }

    #[test]
    fn rotated_and_trimmed_frames_come_back_as_drawn() {
        let sheet = sheet(10, 10);
        // A 3 × 2 frame packed turned: on the sheet it's 2 wide, 3 tall.
        let turned = SpriteFrame {
            sheet: 0,
            x: 1,
            y: 1,
            width: 3,
            height: 2,
            rotated: true,
            offset: (1, 0),
            source: (5, 2),
            duration_ms: None,
        };
        let pixels = sheet.frame_pixels(&turned);
        let at = |x: u32, y: u32| &pixels[((y * 5 + x) * 4) as usize..((y * 5 + x) * 4 + 4) as usize];
        assert_eq!(at(0, 0), [0, 0, 0, 0], "the trimmed border is clear");
        // The frame's top-left came from the packed place's top-right.
        assert_eq!(at(1, 0), [2, 1, 7, 255]);
        // Its bottom-left, from the packed top-left.
        assert_eq!(at(1, 1), [1, 1, 7, 255]);
        // Along its top, down the packed right edge.
        assert_eq!(at(3, 0), [2, 3, 7, 255]);
    }

    #[test]
    fn a_sheet_becomes_a_layer_of_drawings_held_as_long_as_they_last() {
        let mut doc = Document::default();
        let sheets = [sheet(8, 4)];
        let frames = grid_frames(8, 4, 2, 1);
        // On twos, the first frame again at the end: it reuses its drawing.
        let order = [frames[0].clone(), frames[1].clone(), frames[0].clone()];
        let id = doc.add_sprite_layer(TimelineId::Main, "Walk", 0, &sheets, &order, &[2, 2, 2]).unwrap();
        let layer = doc.layer(id).unwrap();
        assert_eq!(layer.drawings.len(), 2);
        let exposed: Vec<(u32, usize)> = layer.exposure.iter().map(|e| (e.frame, e.drawing)).collect();
        assert_eq!(exposed, [(0, 0), (2, 1), (4, 0)]);
        assert_eq!(layer.end, Some(5));
        let (canvas, w, h) = doc.paint_canvas_at(id, 3).unwrap();
        assert_eq!((w, h), (4, 4));
        assert_eq!(doc.canvas(canvas).unwrap().pixel(0, 0), [4, 0, 7, 255], "the second cell");
    }
}
