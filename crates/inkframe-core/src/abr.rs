//! Reading Photoshop brush files (`.abr`), so brushes made for Photoshop
//! can be painted with here.
//!
//! The layout is the one GIMP and Krita read. All numbers are big-endian.
//!
//! * **Versions 1 and 2** (Photoshop 6 and older): a count, then each brush
//!   in turn, round ("computed", with its diameter, hardness, roundness,
//!   angle and spacing) or sampled (a greyscale picture).
//! * **Versions 6 to 10** (Photoshop 7 on): tagged sections. `samp` holds
//!   the sampled tips, each with an id, and `desc` an action descriptor (the
//!   structure Photoshop saves settings in) listing the brushes: each one's
//!   name, tip, and the Brush Settings sections, which map onto Inkframe's
//!   one by one. A file with tips and no descriptions gives a plain brush
//!   for each tip.
//!
//! Patterns (`patt`), for textures, aren't read yet: a brush whose texture
//! uses one gets Inkframe's paper texture instead, at the same settings.

use std::collections::HashMap;
use std::sync::Arc;

use crate::brush::{
    Blend, BrushPreset, BrushSettings, ColorDynamics, Control, DualBrush, Scattering, ShapeDynamics, Texture,
    Tip, TipImage, Transfer,
};

/// Reads a brush file's brushes. `stem` names those that have no name of
/// their own: "stem 1", "stem 2"…
pub fn read(bytes: &[u8], stem: &str) -> Result<Vec<BrushPreset>, String> {
    let mut r = Reader::new(bytes);
    let version = r.u16()?;
    let presets = match version {
        1 | 2 => read_old(&mut r, version, stem)?,
        6..=10 => read_tagged(&mut r, stem)?,
        _ => return Err(format!("Photoshop brush files of version {version} aren't supported")),
    };
    if presets.is_empty() {
        return Err("there are no brushes in the file".into());
    }
    Ok(presets)
}

/// Versions 1 and 2: brushes one after another.
fn read_old(r: &mut Reader, version: u16, stem: &str) -> Result<Vec<BrushPreset>, String> {
    let count = r.u16()?;
    let mut presets = Vec::new();
    for n in 1..=count {
        let kind = r.u16()?;
        let size = r.u32()? as usize;
        let start = r.at;
        let name = format!("{stem} {n}");
        match kind {
            // Computed: a round brush.
            1 => {
                r.skip(4)?; // misc
                let spacing = r.u16()? as f32 / 100.0;
                let diameter = r.u16()? as f32;
                let roundness = r.u16()? as f32 / 100.0;
                let angle = r.i16()? as f32;
                let hardness = r.u16()? as f32 / 100.0;
                let settings = BrushSettings {
                    size: diameter.max(1.0),
                    spacing: spacing.max(0.01),
                    roundness: roundness.clamp(0.01, 1.0),
                    angle,
                    hardness: hardness.clamp(0.0, 1.0),
                    ..BrushSettings::default()
                };
                presets.push(BrushPreset::new(name, settings));
            }
            2 => {
                r.skip(4)?; // misc
                let spacing = r.u16()? as f32 / 100.0;
                let name =
                    if version == 2 { r.unicode()?.filter(|n| !n.is_empty()).unwrap_or(name) } else { name };
                r.skip(1 + 8)?; // antialiasing, short bounds
                let tip = read_sample(r, &name)?;
                presets.push(BrushPreset::new(name, plain_sampled(tip, spacing)));
            }
            _ => {}
        }
        r.at = start + size;
    }
    Ok(presets)
}

/// A sampled tip's picture: its bounds, depth and pixels (raw or
/// PackBits-compressed rows).
fn read_sample(r: &mut Reader, name: &str) -> Result<TipImage, String> {
    let top = r.i32()?;
    let left = r.i32()?;
    let bottom = r.i32()?;
    let right = r.i32()?;
    let depth = r.u16()?;
    let compressed = r.u8()? == 1;
    let (width, height) = ((right - left).max(0) as u32, (bottom - top).max(0) as u32);
    if width == 0 || height == 0 || width > 5000 || height > 5000 {
        return Err(format!("a brush tip of {width}×{height} pixels"));
    }
    let bytes_per = (depth / 8).max(1) as usize;
    let row = width as usize * bytes_per;
    let raw = if compressed {
        let lengths: Vec<usize> =
            (0..height).map(|_| r.u16().map(|n| n as usize)).collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(row * height as usize);
        for length in lengths {
            let packed = r.take(length)?;
            out.extend(crate::psd::unpack_bits(packed, row).map_err(|_| "a brush tip's rows are cut short")?);
        }
        out
    } else {
        r.take(row * height as usize)?.to_vec()
    };
    // 16-bit tips keep their top byte.
    let amounts = raw.chunks(bytes_per).map(|v| v[0]).take((width * height) as usize).collect::<Vec<u8>>();
    if amounts.len() != (width * height) as usize {
        return Err("a brush tip's pixels are cut short".into());
    }
    Ok(TipImage::new(name, width, height, amounts))
}

/// A plain brush with a sampled tip, as Photoshop makes one from a picture:
/// its own size, spacing as given.
fn plain_sampled(tip: TipImage, spacing: f32) -> BrushSettings {
    let size = tip.width.max(tip.height) as f32;
    BrushSettings {
        tip: Tip::Sampled(Arc::new(tip)),
        size,
        spacing: if spacing > 0.0 { spacing } else { 0.25 },
        shape: ShapeDynamics::default(),
        ..BrushSettings::default()
    }
}

/// Versions 6 to 10: tagged sections.
fn read_tagged(r: &mut Reader, stem: &str) -> Result<Vec<BrushPreset>, String> {
    let subversion = r.u16()?;
    let mut tips: Vec<(String, TipImage)> = Vec::new();
    let mut descriptor = None;
    while r.left() >= 12 {
        if r.take(4)? != b"8BIM" {
            break;
        }
        let key = r.take(4)?.to_vec();
        let length = r.u32()? as usize;
        let end = r.at + length;
        match &key[..] {
            b"samp" => {
                while r.at + 4 <= end {
                    let size = r.u32()? as usize;
                    let next = r.at + size.div_ceil(4) * 4;
                    let id_length = r.u8()? as usize;
                    let id = String::from_utf8_lossy(r.take(id_length)?).into_owned();
                    // The rest of the fixed header before the bounds.
                    let header = if subversion == 1 { 47 } else { 301 };
                    r.skip(header - 1 - id_length.min(header - 1))?;
                    if let Ok(tip) = read_sample(r, &format!("{stem} {}", tips.len() + 1)) {
                        tips.push((id, tip));
                    }
                    r.at = next.min(end);
                }
            }
            b"desc" => {
                let mut d = Reader::new(r.take(length)?);
                d.skip(4)?; // the descriptor's version
                descriptor = Some(d.descriptor()?);
            }
            _ => {}
        }
        r.at = end;
        // Sections are padded to an even length.
        if r.at % 2 == 1 {
            r.at += 1;
        }
    }
    let presets = descriptor.as_ref().map(|d| presets_from(d, &tips, stem)).unwrap_or_default();
    if !presets.is_empty() {
        return Ok(presets);
    }
    // No descriptions: a plain brush for each tip.
    Ok(tips
        .into_iter()
        .map(|(_, tip)| BrushPreset::new(tip.name.clone(), plain_sampled(tip, 0.25)))
        .collect())
}

/// The brushes an action descriptor describes.
fn presets_from(root: &Descriptor, tips: &[(String, TipImage)], stem: &str) -> Vec<BrushPreset> {
    let Some(Value::List(brushes)) = root.get("Brsh") else { return Vec::new() };
    let tip_by_id: HashMap<&str, &TipImage> = tips.iter().map(|(id, tip)| (id.as_str(), tip)).collect();
    brushes
        .iter()
        .enumerate()
        .filter_map(|(i, value)| {
            let Value::Descriptor(d) = value else { return None };
            let name =
                d.text("Nm  ").filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{stem} {}", i + 1));
            Some(BrushPreset::new(name, settings_from(d, &tip_by_id)))
        })
        .collect()
}

/// One brush's settings from its description.
fn settings_from(d: &Descriptor, tips: &HashMap<&str, &TipImage>) -> BrushSettings {
    let mut s = BrushSettings { shape: ShapeDynamics::default(), ..BrushSettings::default() };
    if let Some(Value::Descriptor(tip)) = d.get("Brsh") {
        tip_shape(tip, tips, &mut s.tip, &mut s.size, &mut s.spacing);
        s.angle = tip.number("Angl").unwrap_or(0.0);
        s.roundness = tip.fraction("Rndn").unwrap_or(1.0).clamp(0.01, 1.0);
        s.hardness = tip.fraction("Hrdn").unwrap_or(1.0).clamp(0.0, 1.0);
        s.flip_x = tip.flag("flipX").unwrap_or(false);
        s.flip_y = tip.flag("flipY").unwrap_or(false);
    }
    s.shape = ShapeDynamics {
        on: d.flag("useTipDynamics").unwrap_or(false),
        minimum_diameter: d.fraction("minimumDiameter").unwrap_or(0.0),
        tilt_scale: d.fraction("tiltScale").map_or(2.0, |t| t * 2.0),
        minimum_roundness: d.fraction("minimumRoundness").unwrap_or(0.25),
        flip_x_jitter: d.flag("flipX").unwrap_or(false),
        flip_y_jitter: d.flag("flipY").unwrap_or(false),
        ..ShapeDynamics::default()
    };
    (s.shape.size_jitter, s.shape.size_control) = variation(d, "szVr");
    (s.shape.angle_jitter, s.shape.angle_control) = variation(d, "angleDynamics");
    (s.shape.roundness_jitter, s.shape.roundness_control) = variation(d, "roundnessDynamics");

    let (scatter, scatter_control) = variation(d, "scatterDynamics");
    let (count_jitter, count_control) = variation(d, "countDynamics");
    s.scattering = Scattering {
        on: d.flag("useScatter").unwrap_or(false),
        scatter,
        both_axes: d.flag("bothAxes").unwrap_or(false),
        scatter_control,
        count: d.number("Cnt ").map_or(1, |c| c.round().clamp(1.0, 16.0) as u32),
        count_jitter,
        count_control,
    };

    let (fg_bg_jitter, fg_bg_control) = variation(d, "clVr");
    s.color = ColorDynamics {
        on: d.flag("useColorDynamics").unwrap_or(false),
        per_tip: d.flag("colorDynamicsPerTip").unwrap_or(true),
        fg_bg_jitter,
        fg_bg_control,
        hue_jitter: d.fraction("H   ").unwrap_or(0.0),
        saturation_jitter: d.fraction("Strt").unwrap_or(0.0),
        brightness_jitter: d.fraction("Brgh").unwrap_or(0.0),
        purity: d.fraction("purity").unwrap_or(0.0),
    };

    let (opacity_jitter, opacity_control) = variation(d, "opVr");
    let (flow_jitter, flow_control) = variation(d, "prVr");
    s.transfer = Transfer {
        on: d.flag("usePaintDynamics").unwrap_or(false),
        opacity_jitter,
        opacity_control,
        minimum_opacity: minimum(d, "opVr"),
        flow_jitter,
        flow_control,
        minimum_flow: minimum(d, "prVr"),
    };

    let (depth_jitter, depth_control) = variation(d, "textureDepthDynamics");
    s.texture = Texture {
        on: d.flag("useTexture").unwrap_or(false),
        invert: d.flag("InvT").unwrap_or(false),
        scale: d.fraction("textureScale").unwrap_or(1.0).max(0.01),
        brightness: d.number("textureBrightness").unwrap_or(0.0) / 150.0,
        contrast: d.number("textureContrast").unwrap_or(0.0) / 50.0,
        each_tip: d.flag("TxtC").unwrap_or(false),
        mode: d.enumeration("textureBlendMode").map_or(Blend::Multiply, blend_named),
        depth: d.fraction("textureDepth").unwrap_or(1.0),
        minimum_depth: d.fraction("minimumDepth").unwrap_or(0.0),
        depth_jitter,
        depth_control,
        ..Texture::default()
    };

    if let Some(Value::Descriptor(dual)) = d.get("dualBrush") {
        let mut tip = Tip::Round;
        let (mut size, mut spacing) = (25.0, 0.25);
        if let Some(Value::Descriptor(t)) = dual.get("Brsh") {
            tip_shape(t, tips, &mut tip, &mut size, &mut spacing);
        }
        let (scatter, _) = variation(dual, "scatterDynamics");
        s.dual = DualBrush {
            on: d.flag("useDualBrush").or_else(|| dual.flag("useDualBrush")).unwrap_or(false),
            tip,
            mode: dual.enumeration("BlnM").map_or(Blend::Multiply, blend_named),
            size,
            spacing,
            scatter,
            both_axes: dual.flag("bothAxes").unwrap_or(false),
            count: dual.number("Cnt ").map_or(1, |c| c.round().clamp(1.0, 16.0) as u32),
            flip: dual.flag("Flip").unwrap_or(false),
        };
    }
    s.noise = d.flag("Nose").unwrap_or(false);
    s.wet_edges = d.flag("Wtdg").unwrap_or(false);
    s.build_up = d.flag("Rpt ").unwrap_or(false);
    s
}

/// A tip description's shape: sampled (found by its id) or round, its
/// diameter and spacing.
fn tip_shape(
    t: &Descriptor,
    tips: &HashMap<&str, &TipImage>,
    tip: &mut Tip,
    size: &mut f32,
    spacing: &mut f32,
) {
    if let Some(found) = t.text("sampledData").and_then(|id| tips.get(id.as_str()).copied()) {
        *tip = Tip::Sampled(Arc::new(TipImage::new(
            found.name.clone(),
            found.width,
            found.height,
            found.amounts.clone(),
        )));
    }
    if let Some(diameter) = t.number("Dmtr") {
        *size = diameter.max(1.0);
    }
    if let Some(gap) = t.fraction("Spcn") {
        *spacing = gap.max(0.01);
    }
}

/// A variation (`brVr`) description: its jitter and its control.
fn variation(d: &Descriptor, key: &str) -> (f32, Control) {
    let Some(Value::Descriptor(v)) = d.get(key) else { return (0.0, Control::Off) };
    let control = match v.number("bVTy").map(|n| n as i64) {
        Some(1) => Control::Fade(v.number("fStp").map_or(25, |n| n.max(1.0) as u32)),
        Some(2) => Control::PenPressure,
        Some(3) => Control::PenTilt,
        Some(5) => Control::InitialDirection,
        Some(6) => Control::Direction,
        _ => Control::Off,
    };
    (v.fraction("jitter").unwrap_or(0.0), control)
}

fn minimum(d: &Descriptor, key: &str) -> f32 {
    match d.get(key) {
        Some(Value::Descriptor(v)) => v.fraction("Mnm ").unwrap_or(0.0),
        _ => 0.0,
    }
}

fn blend_named(name: &str) -> Blend {
    match name {
        "Sbtr" | "subtract" => Blend::Subtract,
        "Drkn" | "darken" => Blend::Darken,
        "Ovrl" | "overlay" => Blend::Overlay,
        "CDdg" | "colorDodge" => Blend::ColorDodge,
        "CBrn" | "colorBurn" => Blend::ColorBurn,
        "linearBurn" => Blend::LinearBurn,
        "hardMix" => Blend::HardMix,
        "linearHeight" => Blend::LinearHeight,
        "height" => Blend::Height,
        _ => Blend::Multiply,
    }
}

/// An action descriptor: keyed values.
#[derive(Debug, Clone, PartialEq, Default)]
struct Descriptor(Vec<(String, Value)>);

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Descriptor(Descriptor),
    List(Vec<Value>),
    Number(f64),
    /// A number with a unit: `#Prc` (percent), `#Pxl`, `#Ang` and so on.
    Unit(String, f64),
    Text(String),
    Enum(String),
    Bool(bool),
    /// Anything not needed here, read past.
    Other,
}

impl Descriptor {
    fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn number(&self, key: &str) -> Option<f32> {
        match self.get(key)? {
            Value::Number(n) | Value::Unit(_, n) => Some(*n as f32),
            _ => None,
        }
    }

    /// A percentage as a fraction (a plain number is taken as is).
    fn fraction(&self, key: &str) -> Option<f32> {
        match self.get(key)? {
            Value::Unit(unit, n) if unit == "#Prc" => Some(*n as f32 / 100.0),
            Value::Unit(_, n) | Value::Number(n) => Some(*n as f32),
            _ => None,
        }
    }

    fn flag(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn text(&self, key: &str) -> Option<String> {
        match self.get(key)? {
            Value::Text(t) => Some(t.clone()),
            _ => None,
        }
    }

    fn enumeration(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Enum(e) => Some(e),
            _ => None,
        }
    }
}

/// Reads big-endian numbers and Photoshop's structures from bytes.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn left(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end =
            self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or("the brush file ends early")?;
        let bytes = &self.bytes[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().expect("two bytes")))
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_be_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    /// A UTF-16 string with its length in characters first, its trailing
    /// nul dropped.
    fn unicode(&mut self) -> Result<Option<String>, String> {
        let length = self.u32()? as usize;
        let units: Vec<u16> =
            self.take(length * 2)?.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        Ok(Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_owned()))
    }

    /// A key or class id: four characters, or a longer name with its
    /// length first (a length of 0 means four characters follow).
    fn id(&mut self) -> Result<String, String> {
        let length = self.u32()? as usize;
        let length = if length == 0 { 4 } else { length };
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }

    fn descriptor(&mut self) -> Result<Descriptor, String> {
        self.unicode()?; // the class's display name
        self.id()?; // its class id
        let count = self.u32()?;
        let mut items = Vec::new();
        for _ in 0..count {
            let key = self.id()?;
            let value = self.value()?;
            items.push((key, value));
        }
        Ok(Descriptor(items))
    }

    fn value(&mut self) -> Result<Value, String> {
        let kind = self.take(4)?;
        Ok(match kind {
            b"Objc" | b"GlbO" => Value::Descriptor(self.descriptor()?),
            b"VlLs" => {
                let count = self.u32()?;
                Value::List((0..count).map(|_| self.value()).collect::<Result<_, _>>()?)
            }
            b"doub" => Value::Number(self.f64()?),
            b"UntF" => {
                let unit = String::from_utf8_lossy(self.take(4)?).into_owned();
                Value::Unit(unit, self.f64()?)
            }
            b"UnFl" => {
                self.skip(4)?;
                let count = self.u32()? as usize;
                self.skip(count * 8)?;
                Value::Other
            }
            b"TEXT" => Value::Text(self.unicode()?.unwrap_or_default()),
            b"enum" => {
                self.id()?; // the enumeration's type
                Value::Enum(self.id()?)
            }
            b"long" => Value::Number(self.i32()? as f64),
            b"comp" => Value::Number(
                self.take(8).map(|b| i64::from_be_bytes(b.try_into().expect("eight bytes")))? as f64,
            ),
            b"bool" => Value::Bool(self.u8()? != 0),
            b"type" | b"GlbC" => {
                self.unicode()?;
                self.id()?;
                Value::Other
            }
            b"alis" | b"tdta" | b"Pth " => {
                let length = self.u32()? as usize;
                self.skip(length)?;
                Value::Other
            }
            b"obj " => {
                self.reference()?;
                Value::Other
            }
            other => {
                return Err(format!(
                    "an unknown setting type {:?} in the brush file",
                    String::from_utf8_lossy(other)
                ));
            }
        })
    }

    /// A reference (`obj `), read past.
    fn reference(&mut self) -> Result<(), String> {
        let count = self.u32()?;
        for _ in 0..count {
            match self.take(4)? {
                b"prop" => {
                    self.unicode()?;
                    self.id()?;
                    self.id()?;
                }
                b"Clss" => {
                    self.unicode()?;
                    self.id()?;
                }
                b"Enmr" => {
                    self.unicode()?;
                    self.id()?;
                    self.id()?;
                    self.id()?;
                }
                b"rele" => {
                    self.unicode()?;
                    self.id()?;
                    self.skip(4)?;
                }
                b"Idnt" | b"indx" => self.skip(4)?,
                b"name" => {
                    self.unicode()?;
                    self.id()?;
                    self.unicode()?;
                }
                other => {
                    return Err(format!(
                        "an unknown reference {:?} in the brush file",
                        String::from_utf8_lossy(other)
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds brush files the way Photoshop writes them, for the tests.
    #[derive(Default)]
    struct Writer(Vec<u8>);

    impl Writer {
        fn u8(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        fn u16(&mut self, v: u16) -> &mut Self {
            self.0.extend(v.to_be_bytes());
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_be_bytes());
            self
        }
        fn bytes(&mut self, b: &[u8]) -> &mut Self {
            self.0.extend(b);
            self
        }
        fn unicode(&mut self, s: &str) -> &mut Self {
            let units: Vec<u16> = s.encode_utf16().chain([0]).collect();
            self.u32(units.len() as u32);
            for u in units {
                self.u16(u);
            }
            self
        }
        fn id(&mut self, s: &str) -> &mut Self {
            if s.len() == 4 {
                self.u32(0).bytes(s.as_bytes())
            } else {
                self.u32(s.len() as u32).bytes(s.as_bytes())
            }
        }
        fn unit(&mut self, key: &str, unit: &str, v: f64) -> &mut Self {
            self.id(key).bytes(b"UntF").bytes(unit.as_bytes()).bytes(&v.to_be_bytes())
        }
        fn flag(&mut self, key: &str, v: bool) -> &mut Self {
            self.id(key).bytes(b"bool").u8(v as u8)
        }
        fn start(&mut self, items: u32) -> &mut Self {
            self.unicode("").id("null").u32(items)
        }
    }

    /// A 2×2 tip: its bounds, depth and pixels, uncompressed.
    fn tip_pixels(w: &mut Writer, pixels: [u8; 4]) {
        w.u32(0).u32(0).u32(2).u32(2).u16(8).u8(0).bytes(&pixels);
    }

    #[test]
    fn an_old_file_gives_round_and_sampled_brushes() {
        let mut w = Writer::default();
        w.u16(2).u16(2);
        // A round brush: 30 px, 50% hard, 80% round, 45°, 10% spacing.
        w.u16(1).u32(14).u32(0).u16(10).u16(30).u16(80).u16(45).u16(50);
        // A sampled one, named.
        let mut sampled = Writer::default();
        sampled.u32(0).u16(25).unicode("Dots").u8(1).bytes(&[0; 8]);
        tip_pixels(&mut sampled, [0, 255, 255, 0]);
        w.u16(2).u32(sampled.0.len() as u32).bytes(&sampled.0);
        let presets = read(&w.0, "Set").unwrap();
        assert_eq!(presets.len(), 2);
        let round = &presets[0];
        assert_eq!(round.name, "Set 1");
        assert_eq!((round.settings.size, round.settings.hardness, round.settings.angle), (30.0, 0.5, 45.0));
        assert!((round.settings.roundness - 0.8).abs() < 1e-5);
        let dots = &presets[1];
        assert_eq!(dots.name, "Dots");
        let Tip::Sampled(tip) = &dots.settings.tip else { panic!("a sampled tip") };
        assert_eq!((tip.width, tip.height, &tip.amounts[..]), (2, 2, &[0, 255, 255, 0][..]));
        assert_eq!(dots.settings.spacing, 0.25);
    }

    #[test]
    fn a_new_file_gives_its_described_brushes_with_their_dynamics() {
        let id = "$11111111-2222-3333-4444-555555555555"; // 37 characters
        let mut samp = Writer::default();
        let mut one = Writer::default();
        one.u8(id.len() as u8).bytes(id.as_bytes()).bytes(&[0; 47 - 1 - 37]);
        tip_pixels(&mut one, [255, 0, 0, 255]);
        samp.u32(one.0.len() as u32).bytes(&one.0);
        while samp.0.len() % 4 != 0 {
            samp.u8(0);
        }

        let mut desc = Writer::default();
        desc.u32(16).start(1).id("Brsh").bytes(b"VlLs").u32(1).bytes(b"Objc").start(7);
        desc.id("Nm  ").bytes(b"TEXT").unicode("Leafy");
        desc.id("Brsh").bytes(b"Objc").start(3);
        desc.unit("Dmtr", "#Pxl", 42.0)
            .unit("Spcn", "#Prc", 30.0)
            .id("sampledData")
            .bytes(b"TEXT")
            .unicode(id);
        desc.flag("useTipDynamics", true);
        desc.id("szVr").bytes(b"Objc").start(2).id("bVTy").bytes(b"long").u32(2).unit("jitter", "#Prc", 40.0);
        desc.flag("useScatter", true);
        desc.unit("scatterDynamics", "#Prc", 0.0); // not a variation: ignored
        desc.flag("Wtdg", true);

        let mut w = Writer::default();
        w.u16(10).u16(2);
        w.bytes(b"8BIM").bytes(b"samp").u32(samp.0.len() as u32).bytes(&samp.0);
        w.bytes(b"8BIM").bytes(b"desc").u32(desc.0.len() as u32).bytes(&desc.0);
        // The test file's tips use the short header.
        w.0[3] = 1;
        let presets = read(&w.0, "Set").unwrap();
        assert_eq!(presets.len(), 1);
        let leafy = &presets[0];
        assert_eq!(leafy.name, "Leafy");
        let s = &leafy.settings;
        assert!(matches!(&s.tip, Tip::Sampled(t) if t.amounts == [255, 0, 0, 255]));
        assert_eq!((s.size, s.spacing), (42.0, 0.3));
        assert!(s.shape.on);
        assert_eq!((s.shape.size_jitter, s.shape.size_control), (0.4, Control::PenPressure));
        assert!(s.scattering.on && s.wet_edges && !s.build_up);
    }

    #[test]
    fn tips_without_descriptions_become_plain_brushes() {
        let mut one = Writer::default();
        one.u8(4).bytes(b"abcd").bytes(&[0; 47 - 5]);
        tip_pixels(&mut one, [1, 2, 3, 4]);
        let mut w = Writer::default();
        w.u16(6)
            .u16(1)
            .bytes(b"8BIM")
            .bytes(b"samp")
            .u32(one.0.len() as u32 + 4)
            .u32(one.0.len() as u32)
            .bytes(&one.0);
        let presets = read(&w.0, "Mine").unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Mine 1");
    }

    #[test]
    fn nonsense_is_refused_not_panicked_over() {
        assert!(read(&[], "x").is_err());
        assert!(read(&[0, 9, 1, 2], "x").is_err());
        assert!(read(&[0, 2, 0, 5, 0, 2], "x").is_err());
    }
}
