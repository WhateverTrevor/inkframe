//! Brush tips: the shape each dab stamps.
//!
//! A tip is either **round**, worked out from the hardness at any size, or
//! **sampled**: a greyscale picture (dark is paint, as in Photoshop's
//! Define Brush Preset), such as a bristle print, a sponge or a leaf,
//! scaled, turned and squashed for each dab. A sampled tip keeps a few
//! halved copies of itself, so a big tip painted small is averaged down
//! rather than picked at, which would sparkle.

use std::sync::{Arc, OnceLock};

use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The shape a brush stamps.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum Tip {
    /// A disc (an oval, squashed by the roundness), its edge as soft as the
    /// hardness says.
    #[default]
    Round,
    Sampled(Arc<TipImage>),
}

impl Tip {
    /// The tip's width over its height: 1 for a round tip.
    pub fn aspect(&self) -> f32 {
        match self {
            Tip::Round => 1.0,
            Tip::Sampled(image) => image.width as f32 / image.height as f32,
        }
    }
}

/// A sampled tip's picture: how much paint each pixel puts down, 0 to 255.
pub struct TipImage {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub amounts: Vec<u8>,
    /// Halved copies, made the first time they're wanted.
    smaller: OnceLock<Vec<(u32, u32, Vec<u8>)>>,
}

// Its name and size only: printing every pixel would swamp a log.
impl std::fmt::Debug for TipImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TipImage({:?}, {}×{})", self.name, self.width, self.height)
    }
}

impl PartialEq for TipImage {
    fn eq(&self, other: &Self) -> bool {
        (self.width, self.height, &self.name, &self.amounts)
            == (other.width, other.height, &other.name, &other.amounts)
    }
}

impl TipImage {
    /// A tip from amounts of paint (0 to 255), row by row.
    pub fn new(name: impl Into<String>, width: u32, height: u32, amounts: Vec<u8>) -> Self {
        assert_eq!(amounts.len(), (width * height) as usize, "a tip's amounts fill it");
        Self {
            name: name.into(),
            width: width.max(1),
            height: height.max(1),
            amounts,
            smaller: OnceLock::new(),
        }
    }

    /// A tip from a picture's pixels (straight RGBA, row by row), as
    /// Photoshop makes one from a selection: dark, opaque pixels are paint,
    /// light or clear ones aren't. The picture is trimmed to its paint.
    pub fn from_picture(name: impl Into<String>, width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        let amounts: Vec<u8> = rgba
            .chunks_exact(4)
            .map(|p| {
                let luma = (p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000;
                ((255 - luma) * p[3] as u32 / 255) as u8
            })
            .collect();
        let painted = |x: u32, y: u32| amounts[(y * width + x) as usize] > 0;
        // The first and last painted column and row (the same, if one).
        let ends = |mut found: Box<dyn DoubleEndedIterator<Item = u32> + '_>| {
            let first = found.next()?;
            Some((first, found.next_back().unwrap_or(first)))
        };
        let (left, right) = ends(Box::new((0..width).filter(|&x| (0..height).any(|y| painted(x, y)))))?;
        let (top, bottom) = ends(Box::new((0..height).filter(|&y| (0..width).any(|x| painted(x, y)))))?;
        let (w, h) = (right - left + 1, bottom - top + 1);
        let trimmed = (top..=bottom)
            .flat_map(|y| (left..=right).map(move |x| (x, y)))
            .map(|(x, y)| amounts[(y * width + x) as usize]);
        Some(Self::new(name, w, h, trimmed.collect()))
    }

    /// A texture's pattern from a picture's pixels (straight RGBA, row by
    /// row), all of it, as Photoshop's Define Pattern takes one: light is a
    /// peak and dark a hollow (kept, like a tip, as dark being paint), with
    /// see-through parts as white paper. A picture over `PATTERN_SIDE`
    /// across is halved until it fits, so brushes stay small to save.
    pub fn pattern_from_picture(name: impl Into<String>, width: u32, height: u32, rgba: &[u8]) -> Self {
        let mut amounts: Vec<u8> = rgba
            .chunks_exact(4)
            .map(|p| {
                let luma = (p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000;
                let over_white = (luma * p[3] as u32 + 255 * (255 - p[3] as u32)) / 255;
                (255 - over_white) as u8
            })
            .collect();
        let (mut w, mut h) = (width.max(1), height.max(1));
        while w.max(h) > PATTERN_SIDE {
            (amounts, w, h) = halved(&amounts, w, h);
        }
        Self::new(name, w, h, amounts)
    }

    /// How much paint the tip puts down at (`u`, `v`), each 0 to 1 across
    /// it, for a tip drawn `across` pixels wide: from the copy nearest that
    /// size, blended between its pixels.
    pub fn sample(&self, u: f32, v: f32, across: f32) -> f32 {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return 0.0;
        }
        let (width, height, amounts) = self.level_for(across);
        let x = u * width as f32 - 0.5;
        let y = v * height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (tx, ty) = (x - x0, y - y0);
        let at = |x: f32, y: f32| -> f32 {
            if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
                return 0.0;
            }
            amounts[(y as u32 * width + x as u32) as usize] as f32 / 255.0
        };
        let top = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
        top + (bottom - top) * ty
    }

    /// The copy to sample for a tip drawn `across` pixels wide: the
    /// smallest still at least that wide, so each pixel blends at most four.
    fn level_for(&self, across: f32) -> (u32, u32, &[u8]) {
        let mut best = (self.width, self.height, self.amounts.as_slice());
        for (w, h, amounts) in self.smaller() {
            if (*w as f32) < across {
                break;
            }
            best = (*w, *h, amounts.as_slice());
        }
        best
    }

    fn smaller(&self) -> &[(u32, u32, Vec<u8>)] {
        self.smaller.get_or_init(|| {
            let mut levels: Vec<(u32, u32, Vec<u8>)> = Vec::new();
            let (mut w, mut h, mut amounts) = (self.width, self.height, self.amounts.clone());
            while w > 4 && h > 4 {
                (amounts, w, h) = halved(&amounts, w, h);
                levels.push((w, h, amounts.clone()));
            }
            levels
        })
    }

    /// The tip as a greyscale PNG, for saving.
    fn to_png(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("writing to memory");
        writer.write_image_data(&self.amounts).expect("writing to memory");
        drop(writer);
        bytes
    }

    fn from_png(name: String, bytes: &[u8]) -> Result<Self, String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut buffer = vec![0; reader.output_buffer_size().ok_or("tip too big")?];
        let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
        if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight {
            return Err("a tip is 8-bit greyscale".into());
        }
        buffer.truncate((info.width * info.height) as usize);
        Ok(Self::new(name, info.width, info.height, buffer))
    }
}

/// The longest side a pattern made from a picture keeps.
const PATTERN_SIDE: u32 = 1024;

/// `amounts` (`w`×`h`) at half the size, each pixel the average of four.
fn halved(amounts: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let at = |x: u32, y: u32| amounts[(y.min(h - 1) * w + x.min(w - 1)) as usize] as u32;
    let half = (0..nh)
        .flat_map(|y| (0..nw).map(move |x| (x, y)))
        .map(|(x, y)| {
            ((at(x * 2, y * 2) + at(x * 2 + 1, y * 2) + at(x * 2, y * 2 + 1) + at(x * 2 + 1, y * 2 + 1)) / 4)
                as u8
        })
        .collect();
    (half, nw, nh)
}

/// How a sampled tip is saved: its name and its picture as a PNG, in
/// base64, which keeps brush files small and readable.
#[derive(Serialize, Deserialize)]
struct SavedTip {
    name: String,
    png: String,
}

impl Serialize for TipImage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let png = base64::engine::general_purpose::STANDARD.encode(self.to_png());
        SavedTip { name: self.name.clone(), png }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TipImage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = SavedTip::deserialize(deserializer)?;
        let bytes =
            base64::engine::general_purpose::STANDARD.decode(saved.png).map_err(serde::de::Error::custom)?;
        TipImage::from_png(saved.name, &bytes).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_becomes_a_trimmed_tip_with_dark_as_paint() {
        // 4×3: a black pixel and a grey one on white, the rest clear.
        let mut rgba = vec![0u8; 4 * 3 * 4];
        let mut set =
            |x: usize, y: usize, v: u8, a: u8| rgba[(y * 4 + x) * 4..][..4].copy_from_slice(&[v, v, v, a]);
        set(1, 1, 0, 255);
        set(2, 1, 128, 255);
        set(0, 0, 255, 255);
        let tip = TipImage::from_picture("dots", 4, 3, &rgba).unwrap();
        assert_eq!((tip.width, tip.height), (2, 1));
        assert_eq!(tip.amounts[0], 255);
        assert!((120..=130).contains(&tip.amounts[1]));
        assert!(TipImage::from_picture("blank", 2, 2, &[255; 16]).is_none(), "white is no paint");
    }

    #[test]
    fn a_tip_is_sampled_smoothly_and_nothing_outside_it() {
        let tip = TipImage::new("half", 2, 1, vec![255, 0]);
        assert!(tip.sample(0.25, 0.5, 2.0) > 0.9);
        assert!(tip.sample(0.75, 0.5, 2.0) < 0.1);
        let middle = tip.sample(0.5, 0.5, 2.0);
        assert!((0.4..0.6).contains(&middle), "{middle}");
        assert_eq!(tip.sample(1.5, 0.5, 2.0), 0.0);
    }

    #[test]
    fn a_big_tip_painted_small_uses_an_averaged_copy() {
        // A fine checkerboard averages to grey once halved.
        let amounts = (0..64 * 64).map(|i| if (i % 64 + i / 64) % 2 == 0 { 255 } else { 0 }).collect();
        let tip = TipImage::new("checks", 64, 64, amounts);
        let small = tip.sample(0.3, 0.3, 8.0);
        assert!((0.4..0.6).contains(&small), "{small}");
    }

    #[test]
    fn a_picture_becomes_a_whole_pattern_with_light_as_peaks() {
        // White, black, clear, half-clear black: nothing trimmed.
        let rgba = [255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 128];
        let pattern = TipImage::pattern_from_picture("p", 2, 2, &rgba);
        assert_eq!((pattern.width, pattern.height), (2, 2));
        assert_eq!(pattern.amounts[..3], [0, 255, 0], "white is a peak, black a hollow, clear is paper");
        assert!((120..=135).contains(&pattern.amounts[3]));
        let big = TipImage::pattern_from_picture("big", 3000, 10, &vec![128; 3000 * 10 * 4]);
        assert!(big.width <= 1024 && big.height >= 1, "{}×{}", big.width, big.height);
    }

    #[test]
    fn tips_save_and_load() {
        let tip = Tip::Sampled(Arc::new(TipImage::new("leaf", 3, 2, vec![0, 128, 255, 10, 20, 30])));
        let json = serde_json::to_string(&tip).unwrap();
        assert_eq!(serde_json::from_str::<Tip>(&json).unwrap(), tip);
    }
}
