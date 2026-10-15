//! The project file: a zip holding the document and its painted pixels.
//!
//! ```text
//! project.inkf (a zip)
//!   document.json              the document, as `Document::to_json` writes it
//!   canvases/3/0_1.png         canvas 3's tile in column 0, row 1
//!   canvases/3/1_1.png
//! ```
//!
//! This is the approach Krita (.kra) and OpenRaster take: one file to copy
//! around, the document still readable inside it, and pixels kept as
//! compressed images rather than as text. Only painted tiles are stored.
//!
//! Before version 17, project files were the JSON alone, and those still
//! open: a zip starts with "PK", JSON never does.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::document::{Document, DocumentError};
use crate::paint::{decode_tile, encode_tile};

const DOCUMENT: &str = "document.json";

impl Document {
    /// Opens a project file, either kind.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DocumentError> {
        Self::from_bytes(&std::fs::read(path)?)
    }

    /// Reads a project file's contents: a zip, or the plain JSON that older
    /// versions saved.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocumentError> {
        if !bytes.starts_with(b"PK") {
            let text =
                std::str::from_utf8(bytes).map_err(|_| DocumentError::Invalid("not a project file"))?;
            return Self::from_json(text);
        }
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
        let mut json = String::new();
        zip.by_name(DOCUMENT).map_err(zip_error)?.read_to_string(&mut json)?;
        let mut doc = Self::from_json(&json)?;

        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(zip_error)?;
            let name = entry.name().map_err(zip_error)?.into_owned();
            let Some((canvas, at)) = tile_name(&name) else { continue };
            let Some(canvas) = doc.canvases.get_mut(&canvas) else { continue };
            let mut png = Vec::new();
            entry.read_to_end(&mut png)?;
            let pixels = decode_tile(&png).map_err(|e| DocumentError::Corrupt(format!("{name}: {e}")))?;
            canvas.set_tile(at, pixels);
        }
        Ok(doc)
    }

    /// The project file's contents: the zip [`Document::save`] writes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, DocumentError> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let compressed = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        // PNGs are compressed already; deflating them again only costs time.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        zip.start_file(DOCUMENT, compressed).map_err(zip_error)?;
        zip.write_all(self.to_json()?.as_bytes())?;
        for id in self.used_canvases() {
            let Some(canvas) = self.canvas(id) else { continue };
            for ((column, row), tile) in canvas.tiles() {
                let png = encode_tile(tile).map_err(|e| DocumentError::Corrupt(e.to_string()))?;
                zip.start_file(format!("canvases/{id}/{column}_{row}.png"), stored).map_err(zip_error)?;
                zip.write_all(&png)?;
            }
        }
        Ok(zip.finish().map_err(zip_error)?.into_inner())
    }

    /// Saves atomically: write to a temporary file next to the target, then
    /// rename over it. A crash halfway through can't leave a corrupt file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        let tmp = path.with_extension("inkf.tmp");
        std::fs::write(&tmp, self.to_bytes()?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Reads `canvases/<id>/<column>_<row>.png`.
fn tile_name(name: &str) -> Option<(crate::paint::CanvasId, (u32, u32))> {
    let rest = name.strip_prefix("canvases/")?.strip_suffix(".png")?;
    let (id, tile) = rest.split_once('/')?;
    let (column, row) = tile.split_once('_')?;
    Some((crate::paint::CanvasId(id.parse().ok()?), (column.parse().ok()?, row.parse().ok()?)))
}

fn zip_error(error: zip::result::ZipError) -> DocumentError {
    DocumentError::Corrupt(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, Point, Shape, Style, Transform};

    #[test]
    fn paintings_survive_saving_and_opening() {
        let mut doc = Document::default();
        let canvas = doc.add_canvas(600, 400);
        doc.canvas_mut(canvas).unwrap().tile_mut((2, 1))[..4].copy_from_slice(&[0, 0, 200, 200]);
        let paint = Element {
            shape: Shape::Paint { canvas, width: 600, height: 400 },
            style: Style { fill: None, stroke: None },
        };
        doc.add_layer("Paint", vec![paint], 0, Transform::at(Point::ZERO));

        let bytes = doc.to_bytes().unwrap();
        assert!(bytes.starts_with(b"PK"));
        let opened = Document::from_bytes(&bytes).unwrap();
        assert_eq!(opened, doc);
        assert_eq!(opened.canvas(canvas).unwrap().pixel(512, 256), [0, 0, 200, 200]);
    }

    #[test]
    fn unused_canvases_are_left_out() {
        let mut doc = Document::default();
        let orphan = doc.add_canvas(10, 10);
        doc.canvas_mut(orphan).unwrap().tile_mut((0, 0))[3] = 255;
        let opened = Document::from_bytes(&doc.to_bytes().unwrap()).unwrap();
        assert!(opened.canvas(orphan).unwrap().is_empty());
    }

    #[test]
    fn plain_json_files_from_before_still_open() {
        let doc = Document::default();
        let opened = Document::from_bytes(doc.to_json().unwrap().as_bytes()).unwrap();
        assert_eq!(opened, doc);
    }

    #[test]
    fn tile_names_are_read_back() {
        assert_eq!(tile_name("canvases/12/3_4.png"), Some((crate::paint::CanvasId(12), (3, 4))));
        assert_eq!(tile_name("document.json"), None);
        assert_eq!(tile_name("canvases/x/3_4.png"), None);
    }
}
