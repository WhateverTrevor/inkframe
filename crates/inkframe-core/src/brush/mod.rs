//! The brush engine: turning a pen's path into paint on a canvas, the way
//! Photoshop's brushes do.
//!
//! A stroke is a row of overlapping **dabs**, one every few pixels along
//! the path (the `spacing`), each a stamp of the brush's **tip**: a round
//! one worked out from its hardness, or a sampled picture (see `tip.rs`).
//! The settings are Photoshop's Brush Settings panel, section by section:
//!
//! * **Brush Tip Shape:** the tip, its size, angle, roundness, flips,
//!   hardness and spacing.
//! * **Shape Dynamics**, **Scattering**, **Color Dynamics** and
//!   **Transfer:** how each dab varies, by chance and by the pen (see
//!   `dynamics.rs`).
//! * **Texture:** a pattern pressed into the paint (see `texture.rs`).
//! * **Dual Brush:** a second tip, laid along the same path, that the
//!   first only shows through.
//! * **Noise**, **Wet Edges** and **Build-up**: grit in soft edges, paint
//!   pooling at a stroke's edges like watercolour, and paint that keeps
//!   flowing while the pen rests, like an airbrush.
//!
//! And from the options bar: **flow**, how much each dab adds, so low flow
//! builds up where dabs overlap; and **opacity**, the most a stroke covers.
//! However many dabs overlap, a stroke never gets more opaque than its
//! opacity, so a stroke at 50% stays an even 50% where it crosses itself.
//!
//! That last rule is why a stroke isn't painted straight onto the canvas.
//! Dabs pile up in the stroke's own layer (colour and coverage per pixel),
//! and the canvas is redrawn from its tiles as they were before the
//! stroke, with that layer on top. The eraser is the same stroke, taking
//! away instead of adding.

mod dynamics;
mod presets;
mod texture;
mod tip;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use dynamics::{ColorDynamics, Control, DabInput, Random, Scattering, ShapeDynamics, Transfer};
pub use presets::BrushPreset;
pub use texture::{Blend, Pattern, Texture};
pub use tip::{Tip, TipImage};

use crate::color::Color;
use crate::geometry::Point;
use crate::paint::{Canvas, TILE};
use crate::selection::Selection;

/// How a brush paints. Sizes are in canvas pixels, angles in degrees
/// (counter-clockwise, as in Photoshop), and the rest 0 to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushSettings {
    // Brush Tip Shape.
    pub tip: Tip,
    /// The dab's diameter (a sampled tip's longer side) at full size.
    pub size: f32,
    pub angle: f32,
    /// How round: 1 is as the tip is, less squashes it flatter.
    pub roundness: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    /// How crisp a round tip's edge is (a sampled tip has its own edge).
    pub hardness: f32,
    /// Distance between dabs, as a fraction of their diameter.
    pub spacing: f32,

    // The options bar.
    pub opacity: f32,
    pub flow: f32,

    // The other sections, each with its own on switch, kept while off.
    pub shape: ShapeDynamics,
    pub scattering: Scattering,
    pub texture: Texture,
    pub dual: DualBrush,
    pub color: ColorDynamics,
    pub transfer: Transfer,
    pub noise: bool,
    pub wet_edges: bool,
    pub build_up: bool,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            tip: Tip::Round,
            size: 24.0,
            angle: 0.0,
            roundness: 1.0,
            flip_x: false,
            flip_y: false,
            hardness: 0.8,
            spacing: 0.12,
            opacity: 1.0,
            flow: 1.0,
            // Pressure thins a stroke, out of the box, as people expect of
            // a pen.
            shape: ShapeDynamics { on: true, ..ShapeDynamics::default() },
            scattering: Scattering::default(),
            texture: Texture::default(),
            dual: DualBrush::default(),
            color: ColorDynamics::default(),
            transfer: Transfer::default(),
            noise: false,
            wet_edges: false,
            build_up: false,
        }
    }
}

/// Dual Brush: a second tip, laid along the same path with its own size,
/// spacing and scatter. The paint shows only where the two meet, combined
/// by the mode: a bristly or broken edge on any brush.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DualBrush {
    pub on: bool,
    pub tip: Tip,
    pub mode: Blend,
    pub size: f32,
    pub spacing: f32,
    pub scatter: f32,
    pub both_axes: bool,
    pub count: u32,
    /// Flip the second tip at random, so its stamps don't line up.
    pub flip: bool,
}

impl Default for DualBrush {
    fn default() -> Self {
        Self {
            on: false,
            tip: Tip::Round,
            mode: Blend::Multiply,
            size: 20.0,
            spacing: 0.25,
            scatter: 0.0,
            both_axes: false,
            count: 1,
            flip: false,
        }
    }
}

impl BrushSettings {
    /// The dab's radius for `dab` before any jitter: the size as the
    /// pressure (or whatever steers size) has it, never under half a pixel.
    pub fn radius_for(&self, dab: &DabInput) -> f32 {
        let scale = if self.shape.on { self.size_scale(dab) } else { 1.0 };
        (self.size * scale / 2.0).max(0.5)
    }

    /// The dab's radius at `pressure`, before any jitter.
    pub fn radius(&self, pressure: f32) -> f32 {
        self.radius_for(&DabInput { pressure, ..DabInput::default() })
    }

    /// The flow at `pressure`, as Transfer's flow control has it (without
    /// jitter).
    pub fn flow_at(&self, pressure: f32) -> f32 {
        let t = &self.transfer;
        if !t.on {
            return self.flow;
        }
        self.flow * t.flow_control.scale(&DabInput { pressure, ..DabInput::default() }, t.minimum_flow)
    }

    /// How the size control scales a dab (without jitter).
    fn size_scale(&self, dab: &DabInput) -> f32 {
        let shape = &self.shape;
        match shape.size_control {
            // A pen laid flat paints with its side, bigger.
            Control::PenTilt => 1.0 + (shape.tilt_scale - 1.0) * (1.0 - Control::PenTilt.strength(dab)),
            control if control.is_heading() => 1.0,
            control => control.scale(dab, shape.minimum_diameter),
        }
    }

    /// Distance to the next dab: `spacing` of the dab's size, but never
    /// under half a pixel, or tiny brushes would stamp thousands of dabs.
    pub fn step(&self, pressure: f32) -> f32 {
        self.step_for(&DabInput { pressure, ..DabInput::default() })
    }

    fn step_for(&self, dab: &DabInput) -> f32 {
        (self.radius_for(dab) * 2.0 * self.spacing).max(0.5)
    }

    /// How much a round dab of `radius` covers a pixel whose centre is
    /// `distance` from the dab's: fully inside `radius - soft`, fading to
    /// nothing half a pixel past `radius`. The soft part is at least a
    /// pixel, so a hard brush still gets a smooth (anti-aliased) edge, and
    /// a 0% one fades all the way from its centre.
    pub fn coverage(&self, distance: f32, radius: f32) -> f32 {
        let soft = ((1.0 - self.hardness) * radius).max(1.0);
        let t = ((radius + 0.5 - distance) / (soft + 0.5)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Ready-made brushes: a starting point to adjust from.
    pub fn presets() -> Vec<BrushPreset> {
        presets::built_in()
    }
}

/// One dab's shape, worked out from the settings and the dynamics.
#[derive(Debug, Clone, Copy)]
struct Stamp {
    centre: Point,
    /// Half its width and height, before turning.
    rx: f32,
    ry: f32,
    /// Its turn, as cosine and sine.
    cos: f32,
    sin: f32,
    flip_x: bool,
    flip_y: bool,
}

impl Stamp {
    fn new(tip: &Tip, centre: Point, diameter: f32, angle: f32, roundness: f32) -> Self {
        let aspect = tip.aspect();
        let half = diameter / 2.0;
        let (rx, ry) = if aspect >= 1.0 { (half, half / aspect) } else { (half * aspect, half) };
        let turn = angle.to_radians();
        Self {
            centre,
            rx: rx.max(0.5),
            ry: (ry * roundness.clamp(0.01, 1.0)).max(0.5),
            cos: turn.cos(),
            sin: turn.sin(),
            flip_x: false,
            flip_y: false,
        }
    }

    /// How far it reaches from its centre, whichever way it's turned.
    fn reach(&self) -> f32 {
        self.rx.max(self.ry) + 1.5
    }

    /// Where a pixel centre falls in the stamp, in its own unturned
    /// coordinates (x right, y up), in pixels.
    fn local(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, up) = (x - self.centre.x, self.centre.y - y);
        let lx = dx * self.cos + up * self.sin;
        let ly = -dx * self.sin + up * self.cos;
        (if self.flip_x { -lx } else { lx }, if self.flip_y { -ly } else { ly })
    }

    /// How much `tip` covers the pixel centred at (`x`, `y`), 0 to 1.
    fn amount(&self, tip: &Tip, hardness: f32, x: f32, y: f32) -> f32 {
        let (lx, ly) = self.local(x, y);
        match tip {
            Tip::Round => {
                // An oval measured as a circle the size of its narrower
                // axis, which keeps the soft edge even all round.
                let r = self.rx.min(self.ry);
                let distance = ((lx / self.rx).powi(2) + (ly / self.ry).powi(2)).sqrt() * r;
                let soft = ((1.0 - hardness) * r).max(1.0);
                let t = ((r + 0.5 - distance) / (soft + 0.5)).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            }
            Tip::Sampled(image) => {
                let u = 0.5 + lx / (2.0 * self.rx);
                let v = 0.5 - ly / (2.0 * self.ry);
                image.sample(u, v, self.rx * 2.0)
            }
        }
    }
}

/// Where a stroke's dabs go: one at the start, then one every `step` along
/// the pen's path, with the pen's pressure and tilt in between found by
/// blending. Every stroke tool (brush, eraser, smudge, clone stamp…) places
/// dabs this way.
#[derive(Debug, Clone, Copy, Default)]
pub struct DabPath {
    last: Option<(Point, Pen)>,
    until_next: f32,
}

/// The pen at a point: how hard it's pressed (0 to 1; 1 for a mouse) and
/// how it leans (each −1 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pen {
    pub pressure: f32,
    pub tilt: (f32, f32),
}

impl Pen {
    pub fn pressed(pressure: f32) -> Self {
        Self { pressure, tilt: (0.0, 0.0) }
    }

    fn lerp(self, other: Pen, t: f32) -> Pen {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Pen {
            pressure: mix(self.pressure, other.pressure),
            tilt: (mix(self.tilt.0, other.tilt.0), mix(self.tilt.1, other.tilt.1)),
        }
    }
}

impl DabPath {
    /// Carries the path on to `at` with `pressure`, calling `dab` with the
    /// centre and pressure of each dab due on the way.
    pub fn advance(
        &mut self,
        settings: &BrushSettings,
        at: Point,
        pressure: f32,
        mut dab: impl FnMut(Point, f32),
    ) {
        self.advance_pen(
            |pen| settings.step(pen.pressure),
            at,
            Pen::pressed(pressure),
            |at, pen, _| dab(at, pen.pressure),
        );
    }

    /// Carries the path on to `at` with `pen`, calling `dab` with the
    /// centre, the pen and the heading (degrees) of each dab due, and
    /// asking `step` how far each is from the one before.
    pub fn advance_pen(
        &mut self,
        step: impl Fn(&Pen) -> f32,
        at: Point,
        pen: Pen,
        mut dab: impl FnMut(Point, Pen, f32),
    ) {
        let pen = Pen { pressure: pen.pressure.clamp(0.0, 1.0), ..pen };
        match self.last {
            None => {
                dab(at, pen, 0.0);
                self.until_next = step(&pen);
            }
            Some((from, from_pen)) => {
                let length = from.distance(at);
                // Up the screen is a positive angle, as in Photoshop.
                let heading = (from.y - at.y).atan2(at.x - from.x).to_degrees();
                let mut travelled = self.until_next;
                while travelled <= length {
                    let t = if length > 0.0 { travelled / length } else { 1.0 };
                    let p = from_pen.lerp(pen, t);
                    dab(from.lerp(at, t), p, heading);
                    travelled += step(&p);
                }
                self.until_next = travelled - length;
            }
        }
        self.last = Some((at, pen));
    }
}

/// What a stroke does to the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrushMode {
    Paint(Color),
    Erase,
}

/// The stroke's own layer over one tile: premultiplied colour and coverage
/// per pixel, as fractions.
type Layer = Vec<[f32; 4]>;

/// One stroke, from pen down to pen up.
pub struct BrushStroke {
    settings: BrushSettings,
    mode: BrushMode,
    /// What Color Dynamics wanders towards.
    background: Color,
    /// Each touched tile as it was before the stroke (`None`: it was empty).
    before: BTreeMap<(u32, u32), Option<Arc<Vec<u8>>>>,
    layer: BTreeMap<(u32, u32), Layer>,
    /// The dual brush's coverage, where it has painted.
    dual: BTreeMap<(u32, u32), Vec<f32>>,
    /// Tiles changed since the canvas was last redrawn.
    dirty: BTreeSet<(u32, u32)>,
    path: DabPath,
    dual_path: DabPath,
    /// Where it may paint, if not everywhere.
    selection: Option<Selection>,
    random: Random,
    /// Dabs laid so far, for fades.
    index: u32,
    initial_direction: Option<f32>,
    /// The colour for the whole stroke, when Color Dynamics picks one per
    /// stroke rather than per dab.
    stroke_color: Option<Color>,
    /// The last dab's place and pen, for Build-up while the pen rests.
    resting: Option<(Point, DabInput)>,
    /// Build-up time not yet spent on a dab, in seconds.
    held: f32,
}

impl BrushStroke {
    pub fn new(settings: BrushSettings, mode: BrushMode) -> Self {
        Self {
            settings,
            mode,
            background: Color::rgb(255, 255, 255),
            before: BTreeMap::new(),
            layer: BTreeMap::new(),
            dual: BTreeMap::new(),
            dirty: BTreeSet::new(),
            path: DabPath::default(),
            dual_path: DabPath::default(),
            selection: None,
            random: Random::new(1),
            index: 0,
            initial_direction: None,
            stroke_color: None,
            resting: None,
            held: 0.0,
        }
    }

    /// Paints only within `selection` (soft edges painting partly).
    pub fn within(mut self, selection: Option<Selection>) -> Self {
        self.selection = selection;
        self
    }

    /// The background colour, which Color Dynamics can wander towards.
    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// Where the stroke's chance starts: strokes with different seeds
    /// scatter and jitter differently.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.random = Random::new(seed);
        self
    }

    /// Carries the stroke on to `at` (canvas pixels) with `pressure` (0 to
    /// 1; 1 for a mouse), laying dabs along the way, and updates `canvas`.
    pub fn add_point(&mut self, canvas: &mut Canvas, at: Point, pressure: f32) {
        self.add_pen(canvas, at, Pen::pressed(pressure));
    }

    /// [`Self::add_point`] with the pen's tilt as well.
    pub fn add_pen(&mut self, canvas: &mut Canvas, at: Point, pen: Pen) {
        let mut dabs = Vec::new();
        let settings = &self.settings;
        self.path.advance_pen(
            |pen| {
                settings.step_for(&DabInput { pressure: pen.pressure, tilt: pen.tilt, ..DabInput::default() })
            },
            at,
            pen,
            |at, pen, heading| dabs.push((at, pen, heading)),
        );
        let mut duals = Vec::new();
        if self.settings.dual.on {
            let step = (self.settings.dual.size * self.settings.dual.spacing).max(0.5);
            self.dual_path.advance_pen(|_| step, at, pen, |at, _, heading| duals.push((at, heading)));
        }
        for (at, pen, heading) in dabs {
            let initial = *self.initial_direction.get_or_insert(heading);
            let input = DabInput {
                pressure: pen.pressure,
                tilt: pen.tilt,
                direction: heading,
                initial_direction: initial,
                index: self.index,
            };
            self.lay(canvas, at, &input);
            self.resting = Some((at, input));
        }
        for (at, heading) in duals {
            self.lay_dual(canvas, at, heading);
        }
        self.held = 0.0;
        self.redraw(canvas);
    }

    /// With Build-up on, keeps paint flowing at the pen's last place while
    /// it rests for `seconds`, as an airbrush does.
    pub fn rest(&mut self, canvas: &mut Canvas, seconds: f32) {
        if !self.settings.build_up {
            return;
        }
        let Some((at, input)) = self.resting else { return };
        // About thirty dabs a second, as Photoshop's airbrush lays them.
        self.held += seconds;
        let mut laid = false;
        while self.held >= 1.0 / 30.0 {
            self.held -= 1.0 / 30.0;
            self.lay(canvas, at, &DabInput { index: self.index, ..input });
            laid = true;
        }
        if laid {
            self.redraw(canvas);
        }
    }

    /// Lays one step's dabs (several, with Scattering's count) around `at`.
    fn lay(&mut self, canvas: &Canvas, at: Point, input: &DabInput) {
        let s = &self.settings;
        let random = &mut self.random;
        let count = if s.scattering.on {
            let c = s.scattering.count.clamp(1, 16) as f32
                * s.scattering.count_control.scale(input, 0.0)
                * (1.0 - s.scattering.count_jitter * random.next());
            (c.round() as u32).max(1)
        } else {
            1
        };
        for _ in 0..count {
            let stamp = self.stamp_for(at, input);
            let (opacity, flow, color, depth) = self.transfer_for(input);
            self.stamp(canvas, &stamp, opacity, flow, color, depth);
        }
        self.index += 1;
    }

    /// One dab's shape: the tip shape's settings, varied by Shape Dynamics
    /// and thrown off the path by Scattering.
    fn stamp_for(&mut self, at: Point, input: &DabInput) -> Stamp {
        let s = &self.settings;
        let random = &mut self.random;
        let shape = &s.shape;
        let mut diameter = s.size;
        let mut angle = s.angle;
        let mut roundness = s.roundness;
        let (mut flip_x, mut flip_y) = (s.flip_x, s.flip_y);
        if shape.on {
            let mut scale = s.size_scale(input) * (1.0 - shape.size_jitter * random.next());
            if shape.size_control != Control::Off || shape.size_jitter > 0.0 {
                scale = scale.max(shape.minimum_diameter.min(1.0));
            }
            diameter *= scale;
            angle += match shape.angle_control {
                Control::Off => 0.0,
                Control::Direction => input.direction,
                Control::InitialDirection => input.initial_direction,
                // Pressure, tilt or a fade turn it all the way round.
                control => (1.0 - control.strength(input)) * 360.0,
            };
            angle += shape.angle_jitter * random.signed() * 180.0;
            if shape.roundness_control != Control::Off || shape.roundness_jitter > 0.0 {
                let r = shape.roundness_control.scale(input, shape.minimum_roundness)
                    * (1.0 - shape.roundness_jitter * random.next());
                roundness *= r.max(shape.minimum_roundness);
            }
            if shape.flip_x_jitter && random.next() < 0.5 {
                flip_x = !flip_x;
            }
            if shape.flip_y_jitter && random.next() < 0.5 {
                flip_y = !flip_y;
            }
        }
        let mut centre = at;
        if s.scattering.on {
            let sc = &s.scattering;
            let reach = sc.scatter * diameter * sc.scatter_control.scale(input, 0.0) / 2.0;
            let heading = input.direction.to_radians();
            // Across the path (and along it, with both axes).
            let (ax, ay) = (-heading.sin(), -heading.cos());
            let (lx, ly) = (heading.cos(), -heading.sin());
            let across = reach * random.signed();
            centre.x += ax * across;
            centre.y += ay * across;
            if sc.both_axes {
                let along = reach * random.signed();
                centre.x += lx * along;
                centre.y += ly * along;
            }
        }
        let mut stamp = Stamp::new(&s.tip, centre, diameter.max(1.0), angle, roundness);
        stamp.flip_x = flip_x;
        stamp.flip_y = flip_y;
        stamp
    }

    /// One dab's opacity ceiling, flow, colour and texture depth.
    fn transfer_for(&mut self, input: &DabInput) -> (f32, f32, Color, f32) {
        let s = &self.settings;
        let random = &mut self.random;
        let (mut opacity, mut flow) = (s.opacity, s.flow);
        if s.transfer.on {
            let t = &s.transfer;
            opacity *=
                t.opacity_control.scale(input, t.minimum_opacity) * (1.0 - t.opacity_jitter * random.next());
            flow *= t.flow_control.scale(input, t.minimum_flow) * (1.0 - t.flow_jitter * random.next());
        }
        let foreground = match self.mode {
            BrushMode::Paint(color) => color,
            BrushMode::Erase => Color::rgb(0, 0, 0),
        };
        let color = if !s.color.on || self.mode == BrushMode::Erase {
            foreground
        } else if s.color.per_tip {
            s.color.color(foreground, self.background, input, random)
        } else {
            *self
                .stroke_color
                .get_or_insert_with(|| s.color.color(foreground, self.background, input, random))
        };
        let t = &s.texture;
        let depth = if t.on && t.each_tip {
            (t.depth * t.depth_control.scale(input, t.minimum_depth) * (1.0 - t.depth_jitter * random.next()))
                .max(t.minimum_depth)
        } else {
            t.depth
        };
        (opacity.clamp(0.0, 1.0), flow.clamp(0.0, 1.0), color, depth)
    }

    /// Adds one stamp of the tip to the stroke's layer.
    fn stamp(&mut self, canvas: &Canvas, stamp: &Stamp, opacity: f32, flow: f32, color: Color, depth: f32) {
        let Some((x0, y0, x1, y1)) = bounds(canvas, stamp.centre, stamp.reach()) else { return };
        let s = &self.settings;
        let ceiling = opacity * color.a as f32 / 255.0;
        let rgb = [color.r, color.g, color.b].map(|v| v as f32 / 255.0);
        let index = self.index;
        for ty in y0 / TILE..=(y1 - 1) / TILE {
            for tx in x0 / TILE..=(x1 - 1) / TILE {
                let tile = (tx, ty);
                self.before.entry(tile).or_insert_with(|| canvas.shared_tile(tile));
                let layer = self.layer.entry(tile).or_insert_with(|| vec![[0.0; 4]; (TILE * TILE) as usize]);
                let (left, top) = (tx * TILE, ty * TILE);
                for y in y0.max(top)..y1.min(top + TILE) {
                    for x in x0.max(left)..x1.min(left + TILE) {
                        let mut m = stamp.amount(&s.tip, s.hardness, x as f32 + 0.5, y as f32 + 0.5);
                        if m <= 0.0 {
                            continue;
                        }
                        if s.texture.on && s.texture.each_tip {
                            m = s.texture.apply(m, x, y, depth);
                        }
                        if s.noise {
                            // Grit in the soft part of the edge only.
                            let grit = texture::hash(x, y, index) - 0.5;
                            m = (m + grit * 4.0 * m * (1.0 - m)).clamp(0.0, 1.0);
                        }
                        let selected = self
                            .selection
                            .as_ref()
                            .map_or(1.0, |sel| sel.at(x as i64, y as i64) as f32 / 255.0);
                        let amount = m * flow * selected;
                        if amount <= 0.0 {
                            continue;
                        }
                        deposit(&mut layer[((y - top) * TILE + x - left) as usize], amount, ceiling, rgb);
                    }
                }
                self.dirty.insert(tile);
            }
        }
    }

    /// Lays the dual brush's stamps around `at`.
    fn lay_dual(&mut self, canvas: &Canvas, at: Point, heading: f32) {
        let dual = self.settings.dual.clone();
        for _ in 0..dual.count.clamp(1, 16) {
            let mut centre = at;
            let reach = dual.scatter * dual.size / 2.0;
            let turn = heading.to_radians();
            let across = reach * self.random.signed();
            centre.x -= turn.sin() * across;
            centre.y -= turn.cos() * across;
            if dual.both_axes {
                let along = reach * self.random.signed();
                centre.x += turn.cos() * along;
                centre.y -= turn.sin() * along;
            }
            let mut stamp = Stamp::new(&dual.tip, centre, dual.size.max(1.0), 0.0, 1.0);
            if dual.flip {
                stamp.flip_x = self.random.next() < 0.5;
                stamp.flip_y = self.random.next() < 0.5;
            }
            let Some((x0, y0, x1, y1)) = bounds(canvas, stamp.centre, stamp.reach()) else { continue };
            for ty in y0 / TILE..=(y1 - 1) / TILE {
                for tx in x0 / TILE..=(x1 - 1) / TILE {
                    let tile = (tx, ty);
                    let coverage = self.dual.entry(tile).or_insert_with(|| vec![0.0; (TILE * TILE) as usize]);
                    let (left, top) = (tx * TILE, ty * TILE);
                    for y in y0.max(top)..y1.min(top + TILE) {
                        for x in x0.max(left)..x1.min(left + TILE) {
                            let m = stamp.amount(&dual.tip, 1.0, x as f32 + 0.5, y as f32 + 0.5);
                            let c = &mut coverage[((y - top) * TILE + x - left) as usize];
                            *c += m * (1.0 - *c);
                        }
                    }
                    if self.layer.contains_key(&tile) {
                        self.dirty.insert(tile);
                    }
                }
            }
        }
    }

    /// Redraws the tiles the stroke changed: as they were before it, with
    /// the stroke on top (or taken out, erasing), after the texture and the
    /// dual brush have had their say.
    fn redraw(&mut self, canvas: &mut Canvas) {
        let s = &self.settings;
        let whole_texture = s.texture.on && !s.texture.each_tip;
        let dual_depth = if matches!(s.dual.mode, Blend::Height | Blend::LinearHeight) { 0.5 } else { 1.0 };
        for tile in std::mem::take(&mut self.dirty) {
            let Some(layer) = self.layer.get(&tile) else { continue };
            let dual = if s.dual.on { self.dual.get(&tile) } else { None };
            let mut pixels = match &self.before[&tile] {
                Some(before) => before.as_ref().clone(),
                None => vec![0; (TILE * TILE * 4) as usize],
            };
            let (left, top) = (tile.0 * TILE, tile.1 * TILE);
            for (i, (pixel, paint)) in pixels.chunks_exact_mut(4).zip(layer).enumerate() {
                let mut alpha = paint[3];
                if alpha <= 0.0 {
                    continue;
                }
                let (x, y) = (left + i as u32 % TILE, top + i as u32 / TILE);
                let mut shown = alpha;
                if whole_texture {
                    shown = s.texture.apply(shown, x, y, s.texture.depth);
                }
                if s.dual.on {
                    let other = dual.map_or(0.0, |d| d[i]);
                    shown = s.dual.mode.combine(shown, other, dual_depth);
                }
                if s.wet_edges {
                    shown = wet(shown, s.opacity);
                }
                // The colour scales with what's left of the coverage.
                let keep = if alpha > 0.0 { shown / alpha } else { 0.0 };
                alpha = shown;
                if alpha <= 0.0 {
                    continue;
                }
                match self.mode {
                    BrushMode::Paint(_) => {
                        // Premultiplied "source over": the stroke, then what
                        // was there, showing through.
                        for (channel, c) in pixel[..3].iter_mut().zip(&paint[..3]) {
                            *channel =
                                (c * keep * 255.0 + *channel as f32 * (1.0 - alpha)).round().min(255.0) as u8;
                        }
                        pixel[3] = (alpha * 255.0 + pixel[3] as f32 * (1.0 - alpha)).round().min(255.0) as u8;
                    }
                    BrushMode::Erase => {
                        for channel in pixel.iter_mut() {
                            *channel = (*channel as f32 * (1.0 - alpha)).round() as u8;
                        }
                    }
                }
            }
            canvas.set_tile(tile, pixels);
        }
    }
}

/// The pixels a stamp of `reach` at `centre` may touch, clipped to the
/// canvas: left, top, right and bottom (exclusive), or `None` if it misses.
fn bounds(canvas: &Canvas, centre: Point, reach: f32) -> Option<(u32, u32, u32, u32)> {
    if centre.x + reach < 0.0 || centre.y + reach < 0.0 {
        return None;
    }
    let x0 = (centre.x - reach).floor().max(0.0) as u32;
    let y0 = (centre.y - reach).floor().max(0.0) as u32;
    let x1 = ((centre.x + reach).ceil().max(0.0) as u32).min(canvas.width);
    let y1 = ((centre.y + reach).ceil().max(0.0) as u32).min(canvas.height);
    (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
}

/// Wet Edges, on a stroke's coverage: where it's solid it shows at half,
/// and where it thins towards its edges it shows more, so the paint looks
/// to have pooled at the rim as watercolour dries. Measured against the
/// stroke's opacity, its most.
fn wet(coverage: f32, opacity: f32) -> f32 {
    if opacity <= 0.0 {
        return 0.0;
    }
    let a = (coverage / opacity).min(1.0);
    (0.5 * a + 1.6 * a * (1.0 - a)).min(1.0) * opacity
}

/// Adds `amount` of paint in colour `rgb` to one pixel of a stroke's layer,
/// letting its coverage rise towards `ceiling` but never past it: the rule
/// that keeps a stroke at its opacity however its dabs overlap.
fn deposit(pixel: &mut [f32; 4], amount: f32, ceiling: f32, rgb: [f32; 3]) {
    let covered = pixel[3];
    if covered >= ceiling || covered >= 1.0 {
        return;
    }
    let after = covered + amount * (ceiling - covered);
    // As "source over" with the alpha that gets from `covered` to `after`.
    let source = (after - covered) / (1.0 - covered);
    for (channel, value) in pixel[..3].iter_mut().zip(rgb) {
        *channel = value * source + *channel * (1.0 - source);
    }
    pixel[3] = after;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> BrushMode {
        BrushMode::Paint(Color::rgb(255, 0, 0))
    }

    fn line(settings: BrushSettings, mode: BrushMode, canvas: &mut Canvas, from: Point, to: Point) {
        let mut stroke = BrushStroke::new(settings, mode);
        stroke.add_point(canvas, from, 1.0);
        stroke.add_point(canvas, to, 1.0);
    }

    #[test]
    fn a_dab_paints_its_middle_solid_and_fades_at_the_edge() {
        let mut canvas = Canvas::new(100, 100);
        let settings = BrushSettings { size: 20.0, hardness: 0.0, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, red());
        stroke.add_point(&mut canvas, Point::new(50.0, 50.0), 1.0);
        // Nearly solid in the middle (a 0% brush is only completely solid
        // at its exact centre, and this pixel's centre is just off it).
        let [r, g, b, a] = canvas.pixel(50, 50);
        assert!(a > 245 && r == a && g == 0 && b == 0, "{:?}", [r, g, b, a]);
        let edge = canvas.pixel(57, 50)[3];
        assert!(edge > 0 && edge < 200, "soft edge alpha {edge}");
        assert_eq!(canvas.pixel(65, 50), [0; 4]);
    }

    #[test]
    fn a_stroke_is_continuous_between_points() {
        let mut canvas = Canvas::new(300, 100);
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 50.0), Point::new(290.0, 50.0));
        // Unbroken along its length, across the tile boundary at x = 256.
        for x in 10..290 {
            assert!(canvas.pixel(x, 50)[3] > 200, "gap at x = {x}");
        }
    }

    #[test]
    fn opacity_caps_a_stroke_that_crosses_itself() {
        let mut canvas = Canvas::new(100, 100);
        let settings = BrushSettings { opacity: 0.5, hardness: 1.0, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, red());
        for p in [(20.0, 50.0), (80.0, 50.0), (20.0, 50.0), (80.0, 50.0)] {
            stroke.add_point(&mut canvas, Point::new(p.0, p.1), 1.0);
        }
        let alpha = canvas.pixel(50, 50)[3];
        assert!((126..=129).contains(&alpha), "alpha {alpha}");
    }

    #[test]
    fn low_flow_builds_up_where_dabs_overlap() {
        let mut canvas = Canvas::new(100, 100);
        let settings = BrushSettings {
            flow: 0.1,
            hardness: 1.0,
            shape: ShapeDynamics::default(),
            ..BrushSettings::default()
        };
        line(settings.clone(), red(), &mut canvas, Point::new(50.0, 50.0), Point::new(50.5, 50.0));
        let light = canvas.pixel(50, 50)[3];
        let mut canvas = Canvas::new(100, 100);
        line(settings, red(), &mut canvas, Point::new(40.0, 50.0), Point::new(60.0, 50.0));
        let built_up = canvas.pixel(50, 50)[3];
        assert!(built_up > light * 3, "{light} then {built_up}");
    }

    #[test]
    fn pressure_makes_lighter_touches_thinner() {
        let mut canvas = Canvas::new(200, 100);
        let settings = BrushSettings { size: 40.0, hardness: 1.0, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, red());
        stroke.add_point(&mut canvas, Point::new(20.0, 50.0), 0.1);
        stroke.add_point(&mut canvas, Point::new(180.0, 50.0), 1.0);
        let height_at = |x: i64| (0..100).filter(|&y| canvas.pixel(x, y)[3] > 128).count();
        assert!(height_at(30) < height_at(170) / 2, "{} vs {}", height_at(30), height_at(170));
    }

    #[test]
    fn erasing_takes_paint_away() {
        let mut canvas = Canvas::new(100, 100);
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 50.0), Point::new(90.0, 50.0));
        line(
            BrushSettings::default(),
            BrushMode::Erase,
            &mut canvas,
            Point::new(50.0, 10.0),
            Point::new(50.0, 90.0),
        );
        assert_eq!(canvas.pixel(50, 50), [0; 4]);
        assert_eq!(canvas.pixel(20, 50)[3], 255);
    }

    #[test]
    fn a_stroke_leaves_the_tiles_it_didnt_touch_shared() {
        let mut canvas = Canvas::new(600, 100);
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 50.0), Point::new(20.0, 50.0));
        line(BrushSettings::default(), red(), &mut canvas, Point::new(500.0, 50.0), Point::new(520.0, 50.0));
        let before = canvas.clone();
        line(BrushSettings::default(), red(), &mut canvas, Point::new(10.0, 80.0), Point::new(20.0, 80.0));
        // Only tile (0, 0) differs; the right-hand one is still the same.
        assert_ne!(before, canvas);
        assert!(std::ptr::eq(before.tile((1, 0)).unwrap(), canvas.tile((1, 0)).unwrap()));
    }

    #[test]
    fn a_height_texture_breaks_paint_up_where_the_paper_is_low() {
        let mut canvas = Canvas::new(200, 100);
        let texture = Texture { on: true, mode: Blend::Height, depth: 0.5, ..Texture::default() };
        let settings = BrushSettings { size: 60.0, texture, hardness: 1.0, ..BrushSettings::default() };
        line(settings, red(), &mut canvas, Point::new(20.0, 50.0), Point::new(180.0, 50.0));
        let row: Vec<u8> = (30..170).map(|x| canvas.pixel(x, 50)[3]).collect();
        assert!(row.iter().any(|&a| a > 200), "the paper's peaks catch paint");
        assert!(row.iter().any(|&a| a < 60), "its hollows don't");
    }

    #[test]
    fn a_selection_keeps_paint_inside_it() {
        let mut canvas = Canvas::new(100, 100);
        let selection = Selection::rectangle(100, 100, Point::new(0.0, 0.0), Point::new(50.0, 100.0));
        let mut stroke = BrushStroke::new(BrushSettings::default(), red()).within(Some(selection));
        stroke.add_point(&mut canvas, Point::new(10.0, 50.0), 1.0);
        stroke.add_point(&mut canvas, Point::new(90.0, 50.0), 1.0);
        assert_eq!(canvas.pixel(40, 50)[3], 255);
        assert_eq!(canvas.pixel(60, 50), [0; 4]);
    }

    #[test]
    fn painting_off_the_canvas_is_harmless() {
        let mut canvas = Canvas::new(50, 50);
        line(
            BrushSettings::default(),
            red(),
            &mut canvas,
            Point::new(-100.0, -100.0),
            Point::new(-80.0, 200.0),
        );
        assert!(canvas.is_empty());
    }

    /// How many pixels of a row (or column) are painted more than half.
    fn painted(canvas: &Canvas, along: impl Iterator<Item = (i64, i64)>) -> usize {
        along.filter(|&(x, y)| canvas.pixel(x, y)[3] > 128).count()
    }

    #[test]
    fn roundness_and_angle_shape_the_dab() {
        let flat = BrushSettings {
            size: 40.0,
            roundness: 0.25,
            hardness: 1.0,
            shape: ShapeDynamics::default(),
            ..BrushSettings::default()
        };
        let mut canvas = Canvas::new(100, 100);
        BrushStroke::new(flat.clone(), red()).add_point(&mut canvas, Point::new(50.0, 50.0), 1.0);
        let wide = painted(&canvas, (0..100).map(|x| (x, 50)));
        let tall = painted(&canvas, (0..100).map(|y| (50, y)));
        assert!(wide > 35 && tall < 12, "flat: {wide} wide, {tall} tall");
        let mut canvas = Canvas::new(100, 100);
        BrushStroke::new(BrushSettings { angle: 90.0, ..flat }, red()).add_point(
            &mut canvas,
            Point::new(50.0, 50.0),
            1.0,
        );
        let wide = painted(&canvas, (0..100).map(|x| (x, 50)));
        let tall = painted(&canvas, (0..100).map(|y| (50, y)));
        assert!(tall > 35 && wide < 12, "turned: {wide} wide, {tall} tall");
    }

    #[test]
    fn a_sampled_tip_paints_its_picture() {
        // A tip that's a ring: paint round the edge, none in the middle.
        let amounts = (0..32 * 32)
            .map(|i| {
                let (x, y) = ((i % 32) as f32 - 15.5, (i / 32) as f32 - 15.5);
                let r = (x * x + y * y).sqrt();
                if (11.0..15.0).contains(&r) { 255 } else { 0 }
            })
            .collect();
        let tip = Tip::Sampled(Arc::new(TipImage::new("ring", 32, 32, amounts)));
        let settings =
            BrushSettings { tip, size: 64.0, shape: ShapeDynamics::default(), ..BrushSettings::default() };
        let mut canvas = Canvas::new(100, 100);
        BrushStroke::new(settings, red()).add_point(&mut canvas, Point::new(50.0, 50.0), 1.0);
        assert_eq!(canvas.pixel(50, 50)[3], 0, "the ring's middle is empty");
        assert!(canvas.pixel(50 + 26, 50)[3] > 200, "its rim is painted");
    }

    #[test]
    fn scattering_throws_dabs_off_the_path_the_same_way_for_a_seed() {
        let settings = BrushSettings {
            size: 6.0,
            hardness: 1.0,
            spacing: 1.0,
            shape: ShapeDynamics::default(),
            scattering: Scattering { on: true, scatter: 8.0, count: 3, ..Scattering::default() },
            ..BrushSettings::default()
        };
        let paint = |seed| {
            let mut canvas = Canvas::new(200, 100);
            let mut stroke = BrushStroke::new(settings.clone(), red()).with_seed(seed);
            stroke.add_point(&mut canvas, Point::new(20.0, 50.0), 1.0);
            stroke.add_point(&mut canvas, Point::new(180.0, 50.0), 1.0);
            canvas
        };
        let canvas = paint(3);
        let off_path = (0..200)
            .flat_map(|x| (0..100).map(move |y| (x, y)))
            .filter(|&(x, y)| (y - 50i64).abs() > 8 && canvas.pixel(x, y)[3] > 0)
            .count();
        assert!(off_path > 50, "{off_path} pixels painted off the path");
        assert_eq!(paint(3), canvas, "the same seed, the same stroke");
        assert_ne!(paint(4), canvas);
    }

    #[test]
    fn per_tip_colour_dynamics_vary_the_paint() {
        let settings = BrushSettings {
            size: 10.0,
            spacing: 1.5,
            shape: ShapeDynamics::default(),
            color: ColorDynamics { on: true, hue_jitter: 1.0, ..ColorDynamics::default() },
            ..BrushSettings::default()
        };
        let mut canvas = Canvas::new(300, 40);
        line(settings, red(), &mut canvas, Point::new(10.0, 20.0), Point::new(290.0, 20.0));
        let mut colours: Vec<[u8; 4]> =
            (10..290).step_by(15).map(|x| canvas.pixel(x, 20)).filter(|p| p[3] == 255).collect();
        colours.dedup();
        assert!(colours.len() > 5, "{colours:?}");
    }

    #[test]
    fn transfer_and_wet_edges_keep_to_their_ceilings() {
        let jittery = BrushSettings {
            size: 30.0,
            hardness: 1.0,
            spacing: 2.0,
            shape: ShapeDynamics::default(),
            transfer: Transfer { on: true, opacity_jitter: 1.0, ..Transfer::default() },
            ..BrushSettings::default()
        };
        let mut canvas = Canvas::new(400, 40);
        line(jittery, red(), &mut canvas, Point::new(20.0, 20.0), Point::new(380.0, 20.0));
        let alphas: Vec<u8> = (20..380).step_by(60).map(|x| canvas.pixel(x, 20)[3]).collect();
        assert!(alphas.iter().max() != alphas.iter().min(), "dabs differ: {alphas:?}");

        let wet = BrushSettings {
            size: 40.0,
            hardness: 0.3,
            wet_edges: true,
            shape: ShapeDynamics::default(),
            ..BrushSettings::default()
        };
        let mut canvas = Canvas::new(200, 100);
        line(wet, red(), &mut canvas, Point::new(20.0, 50.0), Point::new(180.0, 50.0));
        let middle = canvas.pixel(100, 50)[3];
        let rim = (51..70).map(|y| canvas.pixel(100, y)[3]).max().unwrap();
        assert!((120..=135).contains(&middle), "the middle tops out at half: {middle}");
        assert!(rim > middle + 20, "the rim pools darker: {rim} over {middle}");
    }

    #[test]
    fn a_dual_brush_breaks_the_paint_up() {
        let dual = DualBrush { on: true, size: 6.0, spacing: 3.0, ..DualBrush::default() };
        let settings = BrushSettings {
            size: 40.0,
            hardness: 1.0,
            dual,
            shape: ShapeDynamics::default(),
            ..BrushSettings::default()
        };
        let mut canvas = Canvas::new(200, 100);
        line(settings, red(), &mut canvas, Point::new(20.0, 50.0), Point::new(180.0, 50.0));
        let row: Vec<u8> = (30..170).map(|x| canvas.pixel(x, 50)[3]).collect();
        assert!(row.iter().any(|&a| a > 200), "paint where the second tip landed");
        assert!(row.iter().any(|&a| a < 20), "none between its dabs");
    }

    #[test]
    fn build_up_keeps_painting_while_the_pen_rests() {
        let airbrush = BrushSettings {
            size: 30.0,
            flow: 0.05,
            hardness: 0.0,
            build_up: true,
            shape: ShapeDynamics::default(),
            ..BrushSettings::default()
        };
        let mut canvas = Canvas::new(100, 100);
        let mut stroke = BrushStroke::new(airbrush, red());
        stroke.add_point(&mut canvas, Point::new(50.0, 50.0), 1.0);
        let first = canvas.pixel(50, 50)[3];
        stroke.rest(&mut canvas, 1.0);
        let later = canvas.pixel(50, 50)[3];
        assert!(later > first * 5, "{first} then {later}");
    }

    #[test]
    fn angle_can_follow_the_stroke() {
        // A flat tip whose angle follows the stroke stays thin across it,
        // whichever way the stroke goes.
        let follow = BrushSettings {
            size: 30.0,
            roundness: 0.2,
            hardness: 1.0,
            angle: 90.0,
            shape: ShapeDynamics {
                on: true,
                size_control: Control::Off,
                angle_control: Control::Direction,
                ..ShapeDynamics::default()
            },
            ..BrushSettings::default()
        };
        let mut down = Canvas::new(100, 200);
        line(follow.clone(), red(), &mut down, Point::new(50.0, 20.0), Point::new(50.0, 180.0));
        let mut across = Canvas::new(200, 100);
        line(follow, red(), &mut across, Point::new(20.0, 50.0), Point::new(180.0, 50.0));
        let width_down = painted(&down, (0..100).map(|x| (x, 100)));
        let width_across = painted(&across, (0..100).map(|y| (100, y)));
        assert!(width_down > 20 && width_across > 20, "{width_down} and {width_across}: the flat side leads");
    }
}
