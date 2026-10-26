//! Decoded image files, ready to draw.
//!
//! The document only records which file each image comes from. Decoding a
//! large PNG or JPEG takes far longer than drawing it, so an
//! [`ImageLibrary`] decodes each file once and keeps the pixels. The editor
//! keeps one for the open document; an export builds its own.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use inkframe_core::{Document, ImageId};
use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

/// The file types Inkframe can import, as lowercase extensions.
pub const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// Every image in a document, decoded.
#[derive(Default)]
pub struct ImageLibrary {
    entries: HashMap<ImageId, Entry>,
    /// Bumped whenever an image is loaded or dropped, so a cached render
    /// knows the pictures it drew may have changed.
    generation: u64,
}

struct Entry {
    /// The file this was decoded from. If the document's path for the image
    /// changes (the project moved, say), it's decoded again.
    path: PathBuf,
    pixels: Result<Mipmaps, String>,
}

impl fmt::Debug for ImageLibrary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageLibrary").field("images", &self.entries.len()).finish()
    }
}

impl ImageLibrary {
    /// A library with every image in `doc` decoded. Files that can't be read
    /// are recorded as problems (see [`ImageLibrary::problems`]).
    pub fn load(doc: &Document, project_dir: Option<&Path>) -> Self {
        let mut library = Self::default();
        library.sync(doc, project_dir);
        library
    }

    /// Decodes images that are new to the document (or whose files moved),
    /// and forgets ones it no longer has. Cheap when nothing changed, so the
    /// editor can call it every frame.
    pub fn sync(&mut self, doc: &Document, project_dir: Option<&Path>) {
        let before = self.entries.len();
        self.entries.retain(|id, _| doc.image(*id).is_some());
        let mut changed = self.entries.len() != before;

        for asset in &doc.images {
            let path = asset.resolve(project_dir);
            if self.entries.get(&asset.id).is_some_and(|e| e.path == path) {
                continue;
            }
            let pixels = decode(&path).map(Mipmaps::new);
            self.entries.insert(asset.id, Entry { path, pixels });
            changed = true;
        }
        if changed {
            self.generation += 1;
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Images on the stage whose files couldn't be read, with the reason.
    pub fn problems<'a>(&'a self, doc: &'a Document) -> impl Iterator<Item = (&'a str, &'a str)> {
        doc.images.iter().filter(|asset| doc.image_in_use(asset.id)).filter_map(|asset| {
            match &self.entries.get(&asset.id)?.pixels {
                Err(message) => Some((asset.name.as_str(), message.as_str())),
                Ok(_) => None,
            }
        })
    }

    /// Why an image's file couldn't be read, if it couldn't.
    pub fn problem(&self, id: ImageId) -> Option<&str> {
        self.entries.get(&id)?.pixels.as_ref().err().map(String::as_str)
    }

    /// The decoded pixels, or `None` if the file is missing or unreadable.
    pub(crate) fn get(&self, id: ImageId) -> Option<&Mipmaps> {
        self.entries.get(&id)?.pixels.as_ref().ok()
    }

    /// An image's pixels at full size, premultiplied, for tracing.
    pub fn full_size(&self, id: ImageId) -> Option<&Pixmap> {
        self.get(id).map(|m| &m.levels[0])
    }
}

/// An image's size in pixels, read from the file's header without decoding
/// the whole picture.
pub fn image_size(path: &Path) -> Result<(u32, u32), String> {
    image::image_dimensions(path).map_err(|e| describe(path, e))
}

/// An image file's pixels, straight (not premultiplied) RGBA, row by row,
/// with its width and height: for turning a picture into something else,
/// such as a brush texture.
pub fn read_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let rgba = image::open(path).map_err(|e| describe(path, e))?.into_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

fn decode(path: &Path) -> Result<Pixmap, String> {
    let rgba = image::open(path).map_err(|e| describe(path, e))?.into_rgba8();
    let size = IntSize::from_wh(rgba.width(), rgba.height()).ok_or("the image is empty")?;
    // tiny-skia blends premultiplied colors, where each channel is already
    // scaled by alpha. Image files store straight alpha, so convert.
    let mut data = rgba.into_raw();
    for pixel in data.chunks_exact_mut(4) {
        let alpha = pixel[3] as u16;
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u16 * alpha + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(data, size).ok_or_else(|| "the image is too large".to_owned())
}

fn describe(path: &Path, error: image::ImageError) -> String {
    let name =
        path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    match error {
        image::ImageError::IoError(e) if e.kind() == std::io::ErrorKind::NotFound => {
            format!("{name} is missing")
        }
        e => format!("couldn't read {name}: {e}"),
    }
}

/// An image plus copies at half, quarter, … size.
///
/// Shrinking a picture by sampling it, as tiny-skia's filters do, only
/// looks at a few source pixels per output pixel. Shrink a 4000-pixel photo
/// to 500 pixels that way and most of its detail is skipped, which shows up
/// as jagged edges and shimmering when it moves. Starting from a copy that's
/// already close to the right size avoids that. (GPUs do the same thing
/// with "mipmaps".)
pub(crate) struct Mipmaps {
    /// Full size first, each next level half the size of the one before.
    levels: Vec<Pixmap>,
}

impl Mipmaps {
    fn new(full: Pixmap) -> Self {
        let mut levels = vec![full];
        while let Some(half) = levels.last().and_then(halve) {
            levels.push(half);
        }
        Self { levels }
    }

    pub fn width(&self) -> u32 {
        self.levels[0].width()
    }

    pub fn height(&self) -> u32 {
        self.levels[0].height()
    }

    /// The smallest copy that still has at least one pixel per output pixel
    /// when the full image is drawn at `scale` (output pixels per image
    /// pixel), and the scale that copy needs relative to the full image.
    pub fn level_for(&self, scale: f32) -> (&Pixmap, f32) {
        let mut chosen = 0;
        while chosen + 1 < self.levels.len() && scale <= 0.5f32.powi(chosen as i32 + 1) {
            chosen += 1;
        }
        (&self.levels[chosen], 2f32.powi(chosen as i32))
    }
}

/// A half-size copy, or `None` once the image is down to a single pixel in
/// either direction.
///
/// Drawing at exactly half size with bilinear filtering samples on the
/// corner between each 2×2 block of pixels, which averages the four: a
/// proper box filter, with no detail skipped.
fn halve(image: &Pixmap) -> Option<Pixmap> {
    let (width, height) = (image.width() / 2, image.height() / 2);
    if width == 0 || height == 0 {
        return None;
    }
    let mut half = Pixmap::new(width, height)?;
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    half.draw_pixmap(0, 0, image.as_ref(), &paint, Transform::from_scale(0.5, 0.5), None);
    Some(half)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
        pixmap
    }

    #[test]
    fn mipmaps_halve_down_to_a_pixel() {
        let mips = Mipmaps::new(solid(16, 4));
        let sizes: Vec<_> = mips.levels.iter().map(|l| (l.width(), l.height())).collect();
        assert_eq!(sizes, [(16, 4), (8, 2), (4, 1)]);
        // Averaging a solid color leaves it unchanged.
        assert_eq!(mips.levels[2].pixel(1, 0), mips.levels[0].pixel(5, 2));
    }

    #[test]
    fn small_draws_use_a_smaller_copy() {
        let mips = Mipmaps::new(solid(64, 64));
        assert_eq!(mips.level_for(1.0).1, 1.0);
        assert_eq!(mips.level_for(0.6).1, 1.0);
        let (level, factor) = mips.level_for(0.2);
        assert_eq!((level.width(), factor), (16, 4.0));
    }

    #[test]
    fn decoding_premultiplies_alpha() {
        let dir = std::env::temp_dir().join(format!("inkframe-images-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("half.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 128])).save(&path).unwrap();

        let pixmap = decode(&path).unwrap();
        let pixel = pixmap.pixel(0, 0).unwrap();
        assert_eq!((pixel.red(), pixel.alpha()), (128, 128));
        assert_eq!(image_size(&path).unwrap(), (2, 2));
        assert!(decode(&dir.join("nope.png")).unwrap_err().contains("missing"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
