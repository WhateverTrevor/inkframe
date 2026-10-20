//! Photoshop (.psd) files: reading and writing layered images.
//!
//! Only what's needed to trade layered paintings with Photoshop and the
//! apps that read its files (Krita, Procreate, Affinity, GIMP…): 8-bit RGB
//! images whose layers have pixels, a name, a position, an opacity,
//! visibility and a blend mode. Layer groups are flattened into a plain
//! list, and adjustment layers, masks and effects are skipped on reading.
//!
//! The format, from Adobe's "Photoshop File Formats Specification", is a
//! header, three length-prefixed sections (colour mode data and image
//! resources, both empty here, then the layers), and finally the merged
//! image that readers without layer support show. Numbers are big-endian.
//! Pixel data is stored one channel at a time, either raw or compressed
//! with PackBits, a simple run-length scheme.

use crate::blend::BlendMode;

/// One layer: `width`×`height` straight (not premultiplied) RGBA pixels,
/// row by row, placed at `left`, `top` in the image.
#[derive(Debug, Clone, PartialEq)]
pub struct PsdLayer {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub opacity: u8,
    pub visible: bool,
    pub blend: BlendMode,
}

/// A layered image, `width`×`height`. Layers are bottom first. `merged`
/// is the whole image flattened (straight RGBA), for readers that don't
/// understand layers.
#[derive(Debug, Clone, PartialEq)]
pub struct Psd {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<PsdLayer>,
    pub merged: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PsdError {
    #[error("this isn't a Photoshop file")]
    NotPsd,
    #[error("only 8-bit RGB Photoshop files can be imported (this one is {0})")]
    Unsupported(String),
    #[error("the Photoshop file ends early or is damaged")]
    Truncated,
}

/// Photoshop's four-letter names for blend modes.
const BLEND_KEYS: [(BlendMode, &[u8; 4]); 17] = [
    (BlendMode::Normal, b"norm"),
    (BlendMode::Multiply, b"mul "),
    (BlendMode::Screen, b"scrn"),
    (BlendMode::Overlay, b"over"),
    (BlendMode::Darken, b"dark"),
    (BlendMode::Lighten, b"lite"),
    (BlendMode::ColorDodge, b"div "),
    (BlendMode::ColorBurn, b"idiv"),
    (BlendMode::Add, b"lddg"),
    (BlendMode::HardLight, b"hLit"),
    (BlendMode::SoftLight, b"sLit"),
    (BlendMode::Difference, b"diff"),
    (BlendMode::Exclusion, b"smud"),
    (BlendMode::Hue, b"hue "),
    (BlendMode::Saturation, b"sat "),
    (BlendMode::Color, b"colr"),
    (BlendMode::Luminosity, b"lum "),
];

impl Psd {
    /// The file's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer::default();
        out.bytes(b"8BPS");
        out.u16(1); // version
        out.bytes(&[0; 6]);
        out.u16(4); // channels in the merged image: RGB and alpha
        out.u32(self.height);
        out.u32(self.width);
        out.u16(8); // bits per channel
        out.u16(3); // RGB
        out.u32(0); // colour mode data
        out.u32(0); // image resources

        let layers = self.layer_info();
        // Layer and mask information: the layer info, then an empty global
        // mask section.
        out.u32(4 + layers.len() as u32 + 4);
        out.u32(layers.len() as u32);
        out.bytes(&layers);
        out.u32(0);

        // The merged image: PackBits, every row's length first, then the
        // rows, a channel at a time.
        let planes = planes(&self.merged, self.width, self.height);
        let rows: Vec<Vec<u8>> =
            planes.iter().flat_map(|plane| plane.chunks(self.width.max(1) as usize).map(pack_bits)).collect();
        out.u16(1);
        for row in &rows {
            out.u16(row.len() as u16);
        }
        for row in &rows {
            out.bytes(row);
        }
        out.0
    }

    /// The layer info section: the count, each layer's record, then each
    /// layer's channel data, padded to an even length.
    fn layer_info(&self) -> Vec<u8> {
        let mut records = Writer::default();
        let mut channels = Writer::default();
        // A negative count says the first alpha channel is the merged
        // image's transparency, which it is.
        records.u16((-(self.layers.len() as i16)) as u16);
        for layer in &self.layers {
            // Alpha first (id −1), then red, green and blue.
            let planes = planes(&layer.pixels, layer.width, layer.height);
            let order = [(-1i16, &planes[3]), (0, &planes[0]), (1, &planes[1]), (2, &planes[2])];
            records.u32(layer.top as u32);
            records.u32(layer.left as u32);
            records.u32((layer.top + layer.height as i32) as u32);
            records.u32((layer.left + layer.width as i32) as u32);
            records.u16(4);
            let mut data = Vec::new();
            for (id, plane) in order {
                let mut channel = Writer::default();
                channel.u16(1); // PackBits
                let rows: Vec<Vec<u8>> = plane.chunks(layer.width.max(1) as usize).map(pack_bits).collect();
                for row in &rows {
                    channel.u16(row.len() as u16);
                }
                for row in &rows {
                    channel.bytes(row);
                }
                records.u16(id as u16);
                records.u32(channel.0.len() as u32);
                data.push(channel.0);
            }
            records.bytes(b"8BIM");
            let key =
                BLEND_KEYS.iter().find(|(mode, _)| *mode == layer.blend).map_or(b"norm", |(_, key)| key);
            records.bytes(key);
            records.bytes(&[layer.opacity, 0, if layer.visible { 0 } else { 2 }, 0]);
            let extra = layer_extra(&layer.name);
            records.u32(extra.len() as u32);
            records.bytes(&extra);
            for channel in data {
                channels.bytes(&channel);
            }
        }
        let mut info = records.0;
        info.extend(channels.0);
        if info.len() % 2 == 1 {
            info.push(0);
        }
        info
    }

    /// Reads a file's bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Psd, PsdError> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != b"8BPS" {
            return Err(PsdError::NotPsd);
        }
        if r.u16()? != 1 {
            return Err(PsdError::Unsupported("a large-document (.psb) file".into()));
        }
        r.take(6)?;
        let merged_channels = r.u16()?;
        let (height, width) = (r.u32()?, r.u32()?);
        let depth = r.u16()?;
        let mode = r.u16()?;
        if depth != 8 || mode != 3 {
            let mode = match mode {
                1 => "greyscale",
                2 => "indexed colour",
                4 => "CMYK",
                9 => "Lab",
                _ => "an unusual colour mode",
            };
            return Err(PsdError::Unsupported(if depth != 8 { format!("{depth}-bit") } else { mode.into() }));
        }
        let skip = |r: &mut Reader| -> Result<(), PsdError> {
            let length = r.u32()? as usize;
            r.take(length).map(|_| ())
        };
        skip(&mut r)?; // colour mode data
        skip(&mut r)?; // image resources

        let section_end = {
            let length = r.u32()? as usize;
            r.at + length
        };
        let mut layers = Vec::new();
        if section_end > r.at {
            let info_length = r.u32()? as usize;
            if info_length > 0 {
                layers = read_layers(&mut r)?;
            }
        }
        r.at = section_end;

        let merged = read_merged(&mut r, width, height, merged_channels).unwrap_or_default();
        Ok(Psd { width, height, layers, merged })
    }
}

/// A layer record's extra data: no mask, no blending ranges, the name as a
/// Pascal string padded to four bytes, and the name again in full Unicode
/// ("luni"), which Photoshop prefers.
fn layer_extra(name: &str) -> Vec<u8> {
    let mut extra = Writer::default();
    extra.u32(0); // mask
    extra.u32(0); // blending ranges
    let short: Vec<u8> = name.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).take(255).collect();
    extra.bytes(&[short.len() as u8]);
    extra.bytes(&short);
    while extra.0.len() % 4 != 0 {
        extra.bytes(&[0]);
    }
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut unicode = Writer::default();
    unicode.u32(units.len() as u32);
    for unit in units {
        unicode.u16(unit);
    }
    while unicode.0.len() % 4 != 0 {
        unicode.bytes(&[0]);
    }
    extra.bytes(b"8BIMluni");
    extra.u32(unicode.0.len() as u32);
    extra.bytes(&unicode.0);
    extra.0
}

/// One layer record as read, before its pixels.
struct Record {
    top: i32,
    left: i32,
    width: u32,
    height: u32,
    channels: Vec<(i16, usize)>,
    blend: BlendMode,
    opacity: u8,
    visible: bool,
    name: String,
    /// A group's start or end marker, which has no pixels of its own.
    divider: bool,
}

fn read_layers(r: &mut Reader) -> Result<Vec<PsdLayer>, PsdError> {
    let count = (r.u16()? as i16).unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let (top, left, bottom, right) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let channel_count = r.u16()? as usize;
        let channels = (0..channel_count)
            .map(|_| Ok((r.u16()? as i16, r.u32()? as usize)))
            .collect::<Result<Vec<_>, PsdError>>()?;
        if r.take(4)? != b"8BIM" {
            return Err(PsdError::Truncated);
        }
        let key: [u8; 4] = r.take(4)?.try_into().expect("four bytes");
        let blend = BLEND_KEYS.iter().find(|(_, k)| **k == key).map_or(BlendMode::Normal, |(mode, _)| *mode);
        let [opacity, _clipping, flags, _filler]: [u8; 4] = r.take(4)?.try_into().expect("four bytes");
        let extra_end = {
            let length = r.u32()? as usize;
            r.at + length
        };
        let mask_length = r.u32()? as usize;
        r.take(mask_length)?;
        let ranges_length = r.u32()? as usize;
        r.take(ranges_length)?;
        let name_length = r.take(1)?[0] as usize;
        let mut name = String::from_utf8_lossy(r.take(name_length)?).into_owned();
        // Padded so the length byte and name fill a multiple of four.
        r.take((4 - (name_length + 1) % 4) % 4)?;
        let mut divider = false;
        while r.at + 12 <= extra_end {
            let signature = r.take(4)?;
            if signature != b"8BIM" && signature != b"8B64" {
                break;
            }
            let key = r.take(4)?.to_vec();
            let length = r.u32()? as usize;
            let block = r.take(length)?;
            match &key[..] {
                b"luni" if block.len() >= 4 => {
                    let units = u32::from_be_bytes(block[..4].try_into().expect("four bytes")) as usize;
                    let utf16: Vec<u16> = block[4..]
                        .chunks_exact(2)
                        .take(units)
                        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                        .collect();
                    name = String::from_utf16_lossy(&utf16);
                }
                b"lsct" | b"lsdk" => divider = true,
                _ => {}
            }
        }
        r.at = extra_end;
        let (width, height) = ((right - left).max(0) as u32, (bottom - top).max(0) as u32);
        let visible = flags & 2 == 0;
        records.push(Record { top, left, width, height, channels, blend, opacity, visible, name, divider });
    }

    let mut layers = Vec::new();
    for record in records {
        let size = (record.width * record.height) as usize;
        let mut planes = [vec![0u8; size], vec![0u8; size], vec![0u8; size], vec![255u8; size]];
        for &(id, length) in &record.channels {
            let data = r.take(length)?;
            let plane = match id {
                0..=2 => Some(id as usize),
                -1 => Some(3),
                _ => None, // masks
            };
            if let (Some(plane), true) = (plane, size > 0) {
                planes[plane] = decode_channel(data, record.width, record.height)?;
            }
        }
        if record.divider || size == 0 {
            continue;
        }
        let pixels =
            (0..size).flat_map(|i| [planes[0][i], planes[1][i], planes[2][i], planes[3][i]]).collect();
        layers.push(PsdLayer {
            name: record.name,
            left: record.left,
            top: record.top,
            width: record.width,
            height: record.height,
            pixels,
            opacity: record.opacity,
            visible: record.visible,
            blend: record.blend,
        });
    }
    Ok(layers)
}

/// One channel's data as stored in a layer: a compression flag, then
/// either raw bytes or PackBits rows.
fn decode_channel(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, PsdError> {
    let mut r = Reader { bytes: data, at: 0 };
    let size = (width * height) as usize;
    match r.u16()? {
        0 => Ok(r.take(size)?.to_vec()),
        1 => {
            let lengths: Vec<usize> =
                (0..height).map(|_| r.u16().map(usize::from)).collect::<Result<_, _>>()?;
            let mut plane = Vec::with_capacity(size);
            for length in lengths {
                plane.extend(unpack_bits(r.take(length)?, width as usize)?);
            }
            Ok(plane)
        }
        other => Err(PsdError::Unsupported(format!("compression type {other}"))),
    }
}

/// The merged image at the end of the file.
fn read_merged(r: &mut Reader, width: u32, height: u32, channels: u16) -> Result<Vec<u8>, PsdError> {
    let size = (width * height) as usize;
    let channels = channels as usize;
    let mut planes = vec![vec![255u8; size]; channels.max(4)];
    match r.u16()? {
        0 => {
            for plane in planes.iter_mut().take(channels) {
                *plane = r.take(size)?.to_vec();
            }
        }
        1 => {
            let lengths: Vec<usize> = (0..height as usize * channels)
                .map(|_| r.u16().map(usize::from))
                .collect::<Result<_, _>>()?;
            for (channel, plane) in planes.iter_mut().take(channels).enumerate() {
                plane.clear();
                for row in 0..height as usize {
                    plane.extend(unpack_bits(
                        r.take(lengths[channel * height as usize + row])?,
                        width as usize,
                    )?);
                }
            }
        }
        other => return Err(PsdError::Unsupported(format!("compression type {other}"))),
    }
    Ok((0..size).flat_map(|i| [planes[0][i], planes[1][i], planes[2][i], planes[3][i]]).collect())
}

/// RGBA pixels split into four planes (red, green, blue, alpha).
fn planes(pixels: &[u8], width: u32, height: u32) -> [Vec<u8>; 4] {
    let size = (width * height) as usize;
    std::array::from_fn(|c| pixels.chunks_exact(4).take(size).map(|p| p[c]).collect())
}

/// PackBits: runs of the same byte as (count, byte), anything else as
/// (count, bytes…), each run at most 128 long.
fn pack_bits(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < row.len() {
        let run = row[i..].iter().take(128).take_while(|&&b| b == row[i]).count();
        if run >= 3 {
            out.push((257 - run) as u8);
            out.push(row[i]);
            i += run;
        } else {
            // Literal bytes, up to the next run of three or more.
            let start = i;
            while i < row.len()
                && i - start < 128
                && !(i + 2 < row.len() && row[i] == row[i + 1] && row[i] == row[i + 2])
            {
                i += 1;
            }
            out.push((i - start - 1) as u8);
            out.extend_from_slice(&row[start..i]);
        }
    }
    out
}

fn unpack_bits(data: &[u8], expected: usize) -> Result<Vec<u8>, PsdError> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < data.len() && out.len() < expected {
        let header = data[i] as i8;
        i += 1;
        if header >= 0 {
            let count = header as usize + 1;
            out.extend_from_slice(data.get(i..i + count).ok_or(PsdError::Truncated)?);
            i += count;
        } else if header != -128 {
            let byte = *data.get(i).ok_or(PsdError::Truncated)?;
            out.extend(std::iter::repeat_n(byte, (1 - header as isize) as usize));
            i += 1;
        }
    }
    out.resize(expected, 0);
    Ok(out)
}

impl crate::Document {
    /// Adds a Photoshop file's layers to `timeline` as paint layers, from
    /// `frame` on, bottom first: each on a canvas the size of the file,
    /// centred on the stage, keeping its name, opacity, visibility and
    /// blend mode. A file with no layers (a flat image) becomes one layer
    /// of its merged image. Returns the new layers.
    pub fn import_psd(
        &mut self,
        psd: &Psd,
        timeline: crate::TimelineId,
        frame: u32,
        name: &str,
    ) -> Vec<crate::LayerId> {
        let flat;
        let layers = if psd.layers.is_empty() {
            flat = [PsdLayer {
                name: name.to_owned(),
                left: 0,
                top: 0,
                width: psd.width,
                height: psd.height,
                pixels: psd.merged.clone(),
                opacity: 255,
                visible: true,
                blend: BlendMode::Normal,
            }];
            &flat[..]
        } else {
            &psd.layers[..]
        };
        let center = crate::Point::new(self.width as f32 / 2.0, self.height as f32 / 2.0);
        let mut added = Vec::new();
        for layer in layers {
            let canvas = self.add_canvas(psd.width, psd.height);
            let premultiplied: Vec<u8> = layer.pixels.chunks_exact(4).flat_map(crate::premultiply).collect();
            if let Some(c) = self.canvas_mut(canvas) {
                c.write_region(
                    layer.left as i64,
                    layer.top as i64,
                    layer.width,
                    layer.height,
                    &premultiplied,
                );
            }
            let paint = crate::Element {
                shape: crate::Shape::Paint { canvas, width: psd.width, height: psd.height },
                style: crate::Style { fill: None, stroke: None, gradient: None },
            };
            let transform =
                crate::Transform { opacity: layer.opacity as f32 / 255.0, ..crate::Transform::at(center) };
            let Some(id) = self.add_layer_to(timeline, layer.name.clone(), vec![paint], frame, transform)
            else {
                continue;
            };
            if let Some(new) = self.layer_mut(id) {
                new.start = frame;
                new.visible = layer.visible;
                new.blend = layer.blend;
            }
            added.push(id);
        }
        added
    }
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_be_bytes());
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PsdError> {
        let slice = self.bytes.get(self.at..self.at + n).ok_or(PsdError::Truncated)?;
        self.at += n;
        Ok(slice)
    }
    fn u16(&mut self) -> Result<u16, PsdError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().expect("two bytes")))
    }
    fn u32(&mut self) -> Result<u32, PsdError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
    fn i32(&mut self) -> Result<i32, PsdError> {
        Ok(self.u32()? as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Psd {
        let solid = |rgba: [u8; 4], w: u32, h: u32| rgba.repeat((w * h) as usize);
        Psd {
            width: 8,
            height: 6,
            layers: vec![
                PsdLayer {
                    name: "Background".into(),
                    left: 0,
                    top: 0,
                    width: 8,
                    height: 6,
                    pixels: solid([255, 255, 255, 255], 8, 6),
                    opacity: 255,
                    visible: true,
                    blend: BlendMode::Normal,
                },
                PsdLayer {
                    name: "Ombre – ink".into(),
                    left: 2,
                    top: 1,
                    width: 3,
                    height: 2,
                    pixels: vec![
                        10, 20, 30, 255, 40, 50, 60, 128, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 9,
                    ],
                    opacity: 200,
                    visible: false,
                    blend: BlendMode::Multiply,
                },
            ],
            merged: solid([255, 255, 255, 255], 8, 6),
        }
    }

    #[test]
    fn a_written_file_reads_back_the_same() {
        let psd = sample();
        let bytes = psd.to_bytes();
        assert_eq!(&bytes[..4], b"8BPS");
        assert_eq!(Psd::from_bytes(&bytes).unwrap(), psd);
    }

    #[test]
    fn importing_makes_a_paint_layer_per_photoshop_layer() {
        let mut doc = crate::Document::default();
        let ids = doc.import_psd(&sample(), crate::TimelineId::Main, 0, "sample");
        assert_eq!(ids.len(), 2);
        let ink = doc.layer(ids[1]).unwrap();
        assert_eq!((ink.name.as_str(), ink.visible, ink.blend), ("Ombre – ink", false, BlendMode::Multiply));
        assert!((doc.pose(ink, 0).opacity - 200.0 / 255.0).abs() < 1e-6);
        let (canvas, width, height) = doc.paint_canvas_at(ids[1], 0).unwrap();
        assert_eq!((width, height), (8, 6));
        // Its first pixel sits at (2, 1).
        assert_eq!(doc.canvas(canvas).unwrap().pixel(2, 1), [10, 20, 30, 255]);
    }

    #[test]
    fn pack_bits_round_trips() {
        let rows: [&[u8]; 4] = [&[], &[7], &[1, 1, 1, 1, 2, 3, 3, 4, 4, 4], &[9; 300]];
        for row in rows {
            assert_eq!(unpack_bits(&pack_bits(row), row.len()).unwrap(), row);
        }
        assert!(pack_bits(&[0; 256]).len() <= 4, "long runs pack small");
    }

    #[test]
    fn other_files_are_refused_clearly() {
        assert_eq!(Psd::from_bytes(b"\\x89PNG...."), Err(PsdError::NotPsd));
        let mut cmyk = sample().to_bytes();
        cmyk[25] = 4;
        assert_eq!(Psd::from_bytes(&cmyk), Err(PsdError::Unsupported("CMYK".into())));
        let whole = sample().to_bytes();
        assert_eq!(Psd::from_bytes(&whole[..whole.len() / 2]), Err(PsdError::Truncated));
    }
}
