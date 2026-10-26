//! Particles, as Harmony's particle system: a layer that sprays dots (or
//! copies of its own drawing) for sparks, snow, smoke and fire.
//!
//! A particle layer's [`Emitter`] says how many particles start each
//! second, where, which way and how fast they go, what pulls on them and
//! how they change over their life. Nothing is simulated step by step:
//! each particle's whole life follows from its number and the emitter's
//! seed by formula, so any frame can be drawn on its own (scrubbing,
//! playing backwards and exporting all agree), and a file only stores the
//! settings.
//!
//! Particles live in the space the layer moves in, not on the layer: once
//! out, a spark doesn't follow the emitter around, so moving or parenting
//! the emitter leaves a trail. The camera still moves past them as it
//! does everything else.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::document::{Document, Layer};
use crate::geometry::{Affine, Point, Rect};

/// The most particles a layer draws at once, so a careless setting can't
/// stall the editor.
pub const MAX_PARTICLES: usize = 5000;

/// Where on the emitter particles start, around the layer's origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmitterShape {
    Point,
    /// A line this long, across the layer.
    Line {
        length: f32,
    },
    Rect {
        width: f32,
        height: f32,
    },
    Circle {
        radius: f32,
    },
}

/// What each particle looks like.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Sprite {
    /// A round dot in the particle's colour. `softness` 0 is a hard edge,
    /// 1 fades all the way from the middle.
    Dot { softness: f32 },
    /// The layer's own drawing, fitted to the particle's size. The
    /// particle's colour isn't used; its opacity is.
    Drawing,
}

/// A particle layer's settings. Distances are in stage pixels, times in
/// seconds and angles in degrees, clockwise from pointing right (so -90 is
/// straight up).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Emitter {
    /// Picks which random particles these are: change it for another
    /// spray just like this one.
    pub seed: u32,
    /// New particles each second.
    pub rate: f32,
    /// Start as if it had been running a while, rather than from nothing.
    pub start_full: bool,
    pub shape: EmitterShape,
    pub direction: f32,
    /// How far either side of `direction` particles may head.
    pub spread: f32,
    pub speed: f32,
    /// How much speed varies, as a fraction of it (0–1).
    pub speed_variance: f32,
    /// Pull downwards (negative pulls up), in pixels per second².
    pub gravity: f32,
    /// Pull sideways, to the right.
    pub wind: f32,
    /// How quickly the air slows particles, per second. 0 is none.
    pub drag: f32,
    /// How far particles wander off their path, and how quickly.
    pub turbulence: f32,
    pub turbulence_frequency: f32,
    pub lifetime: f32,
    pub lifetime_variance: f32,
    /// Size at birth and at death, in pixels across.
    pub size_start: f32,
    pub size_end: f32,
    pub size_variance: f32,
    pub opacity_start: f32,
    pub opacity_end: f32,
    pub color_start: Color,
    pub color_end: Color,
    /// Turning, in degrees per second, and how much that varies (either way).
    pub spin: f32,
    pub spin_variance: f32,
    pub sprite: Sprite,
}

impl Default for Emitter {
    fn default() -> Self {
        Emitter::preset(Preset::Sparks)
    }
}

/// Ready-made emitters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Sparks,
    Snow,
    Smoke,
    Fire,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Sparks, Preset::Snow, Preset::Smoke, Preset::Fire];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Sparks => crate::i18n::tr_later("Sparks"),
            Preset::Snow => crate::i18n::tr_later("Snow"),
            Preset::Smoke => crate::i18n::tr_later("Smoke"),
            Preset::Fire => crate::i18n::tr_later("Fire"),
        }
    }
}

impl Emitter {
    pub fn preset(preset: Preset) -> Self {
        let base = Emitter {
            seed: 1,
            rate: 60.0,
            start_full: false,
            shape: EmitterShape::Point,
            direction: -90.0,
            spread: 30.0,
            speed: 300.0,
            speed_variance: 0.3,
            gravity: 0.0,
            wind: 0.0,
            drag: 0.0,
            turbulence: 0.0,
            turbulence_frequency: 1.0,
            lifetime: 1.0,
            lifetime_variance: 0.3,
            size_start: 10.0,
            size_end: 10.0,
            size_variance: 0.3,
            opacity_start: 1.0,
            opacity_end: 0.0,
            color_start: Color::WHITE,
            color_end: Color::WHITE,
            spin: 0.0,
            spin_variance: 0.0,
            sprite: Sprite::Dot { softness: 0.5 },
        };
        match preset {
            Preset::Sparks => Emitter {
                rate: 140.0,
                spread: 55.0,
                speed: 750.0,
                gravity: 1000.0,
                drag: 0.6,
                lifetime: 1.1,
                size_start: 12.0,
                size_end: 3.0,
                opacity_end: 0.25,
                color_start: Color::rgb(255, 236, 160),
                color_end: Color::rgb(255, 90, 20),
                sprite: Sprite::Dot { softness: 0.3 },
                ..base
            },
            Preset::Snow => Emitter {
                rate: 40.0,
                start_full: true,
                shape: EmitterShape::Line { length: 1920.0 },
                direction: 90.0,
                spread: 10.0,
                speed: 70.0,
                wind: 10.0,
                turbulence: 25.0,
                turbulence_frequency: 0.4,
                lifetime: 16.0,
                lifetime_variance: 0.2,
                size_start: 10.0,
                size_end: 10.0,
                size_variance: 0.5,
                opacity_start: 0.9,
                opacity_end: 0.9,
                sprite: Sprite::Dot { softness: 0.6 },
                ..base
            },
            Preset::Smoke => Emitter {
                rate: 12.0,
                start_full: true,
                shape: EmitterShape::Circle { radius: 20.0 },
                spread: 15.0,
                speed: 90.0,
                wind: 25.0,
                drag: 0.3,
                turbulence: 30.0,
                turbulence_frequency: 0.3,
                lifetime: 4.0,
                size_start: 50.0,
                size_end: 220.0,
                opacity_start: 0.5,
                opacity_end: 0.0,
                color_start: Color::rgb(120, 120, 128),
                color_end: Color::rgb(190, 190, 196),
                sprite: Sprite::Dot { softness: 1.0 },
                ..base
            },
            Preset::Fire => Emitter {
                rate: 160.0,
                start_full: true,
                shape: EmitterShape::Line { length: 140.0 },
                spread: 10.0,
                speed: 180.0,
                gravity: -260.0,
                turbulence: 22.0,
                turbulence_frequency: 1.8,
                lifetime: 1.1,
                size_start: 90.0,
                size_end: 12.0,
                opacity_start: 0.7,
                opacity_end: 0.0,
                color_start: Color::rgb(255, 210, 90),
                color_end: Color::rgb(220, 40, 10),
                sprite: Sprite::Dot { softness: 0.9 },
                ..base
            },
        }
    }

    /// The longest any particle lives.
    fn longest_life(&self) -> f32 {
        self.lifetime.max(0.0) * (1.0 + self.lifetime_variance.clamp(0.0, 1.0))
    }

    /// Every particle alive `time` seconds after the emitter started.
    /// `placement` gives the emitter's placement at a time (seconds, never
    /// before 0), mapping its own coordinates into the space particles
    /// move in.
    pub fn particles(&self, time: f32, placement: impl Fn(f32) -> Affine) -> Vec<Particle> {
        let rate = self.rate.max(0.0);
        let longest = self.longest_life();
        if rate <= 0.0 || longest <= 0.0 || time < 0.0 {
            return Vec::new();
        }
        // Particle `i` is born at about `i / rate` seconds. Those born before
        // `time - longest` have all died; those after `time` aren't out yet.
        let last = (time * rate).floor() as i64;
        let first_born = if self.start_full { -(longest * rate).ceil() as i64 } else { 0 };
        let first = ((time - longest) * rate).floor().max(first_born as f32) as i64;
        let first = first.max(last - MAX_PARTICLES as i64 * 2);

        let mut alive = Vec::new();
        for i in first..=last {
            let random = |channel: u64| random(self.seed, i, channel);
            // Births are spread evenly with a little jitter, so a steady
            // stream doesn't look like a drumbeat.
            let born = (i as f32 + random(0) * 0.9) / rate;
            let life =
                self.lifetime * (1.0 + self.lifetime_variance.clamp(0.0, 1.0) * (random(1) * 2.0 - 1.0));
            let age = time - born;
            if age < 0.0 || age >= life || life <= 0.0 {
                continue;
            }
            let at_birth = placement(born.max(0.0));
            let start = at_birth.apply(self.spawn_point(random(2), random(3)));

            // Heading out, turned with the emitter.
            let emitter_turn = at_birth.b.atan2(at_birth.a);
            let angle = (self.direction + self.spread * (random(4) * 2.0 - 1.0)).to_radians() + emitter_turn;
            let speed = self.speed * (1.0 + self.speed_variance.clamp(0.0, 1.0) * (random(5) * 2.0 - 1.0));
            let velocity = Point::new(angle.cos(), angle.sin()) * speed;
            let pull = Point::new(self.wind, self.gravity);
            let mut position = start + travelled(velocity, pull, self.drag.max(0.0), age);

            // Wandering: a slow wobble of its own, starting from nothing.
            if self.turbulence != 0.0 {
                let w = std::f32::consts::TAU * self.turbulence_frequency;
                let (px, py) = (random(6) * std::f32::consts::TAU, random(7) * std::f32::consts::TAU);
                position = position
                    + Point::new((w * age + px).sin() - px.sin(), (w * 1.37 * age + py).sin() - py.sin())
                        * self.turbulence;
            }

            let through = age / life;
            let grow = 1.0 + self.size_variance.clamp(0.0, 1.0) * (random(8) * 2.0 - 1.0);
            let size = lerp(self.size_start, self.size_end, through).max(0.0) * grow;
            let spin = self.spin + self.spin_variance * (random(9) * 2.0 - 1.0);
            alive.push(Particle {
                position,
                rotation: random(10) * 360.0 * f32::from(spin != 0.0) + spin * age,
                size,
                opacity: lerp(self.opacity_start, self.opacity_end, through).clamp(0.0, 1.0),
                color: mix(self.color_start, self.color_end, through),
            });
        }
        // Keep the youngest if there are too many.
        if alive.len() > MAX_PARTICLES {
            alive.drain(..alive.len() - MAX_PARTICLES);
        }
        alive
    }

    /// A point on the emitter's shape, from two random numbers.
    fn spawn_point(&self, u: f32, v: f32) -> Point {
        match self.shape {
            EmitterShape::Point => Point::ZERO,
            EmitterShape::Line { length } => Point::new(length * (u - 0.5), 0.0),
            EmitterShape::Rect { width, height } => Point::new(width * (u - 0.5), height * (v - 0.5)),
            EmitterShape::Circle { radius } => {
                // The square root spreads them evenly over the disc rather
                // than bunching them in the middle.
                let (r, a) = (radius * u.sqrt(), v * std::f32::consts::TAU);
                Point::new(r * a.cos(), r * a.sin())
            }
        }
    }
}

/// How far something goes in `t` seconds, starting at `velocity`, pulled
/// by `pull` and slowed by air `drag`.
fn travelled(velocity: Point, pull: Point, drag: f32, t: f32) -> Point {
    if drag < 1e-4 {
        return velocity * t + pull * (t * t / 2.0);
    }
    // With drag, speed settles towards `pull / drag` (the terminal velocity).
    let terminal = pull * (1.0 / drag);
    (velocity - terminal) * ((1.0 - (-drag * t).exp()) / drag) + terminal * t
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let channel = |x: u8, y: u8| lerp(x as f32, y as f32, t).round().clamp(0.0, 255.0) as u8;
    Color::rgba(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b), channel(a.a, b.a))
}

/// A random number in [0, 1) for particle `index`, the same every time.
fn random(seed: u32, index: i64, channel: u64) -> f32 {
    // SplitMix64: cheap, and good enough that neighbouring particles and
    // channels look unrelated.
    let mut z = (seed as u64)
        ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ channel.wrapping_mul(0xD1B5_4A32_D192_ED03);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// One particle, where it is now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// In the space the emitter moves in.
    pub position: Point,
    pub rotation: f32,
    /// Pixels across.
    pub size: f32,
    pub opacity: f32,
    pub color: Color,
}

impl Particle {
    /// Maps a sprite one unit across, centred on the origin, onto the
    /// particle.
    pub fn placement(&self) -> Affine {
        Affine::translate(self.position)
            .pre_concat(Affine::rotate(self.rotation.to_radians()))
            .pre_concat(Affine::scale(self.size, self.size))
    }
}

impl Document {
    /// The particles of a particle layer showing on `frame` (fractional
    /// for motion blur), placed on the layer: each particle's placement
    /// maps a unit sprite into the layer's own coordinates.
    pub fn particles_on_layer(&self, layer: &Layer, frame: f32) -> Vec<(Particle, Affine)> {
        let Some(emitter) = &layer.particles else { return Vec::new() };
        let fps = self.frame_rate.fps() as f32;
        let time = (frame - layer.start as f32) / fps;
        let world = |seconds: f32| self.placement_without_camera(layer, layer.start as f32 + seconds * fps);
        let Some(to_layer) = world(time).inverse() else { return Vec::new() };
        emitter
            .particles(time, world)
            .into_iter()
            .map(|p| {
                let placement = to_layer.pre_concat(p.placement());
                (p, placement)
            })
            .collect()
    }

    /// Where a particle layer's particles reach on `frame`, on the layer.
    pub fn particle_bounds(&self, layer: &Layer, frame: f32) -> Option<Rect> {
        let unit = Rect::from_points(Point::new(-0.5, -0.5), Point::new(0.5, 0.5));
        self.particles_on_layer(layer, frame)
            .into_iter()
            .filter_map(|(_, placement)| Rect::bounding(unit.corners().map(|c| placement.apply(c))))
            .reduce(Rect::union)
    }

    /// Makes a particle layer from `preset` on top of the main timeline,
    /// starting on `frame`, with its emitter at `at`.
    pub fn add_particle_layer(&mut self, preset: Preset, frame: u32, at: Point) -> Option<crate::LayerId> {
        let id = self.add_layer_to(
            crate::TimelineId::Main,
            preset.name(),
            Vec::new(),
            frame,
            crate::Transform::at(at),
        )?;
        let mut emitter = Emitter::preset(preset);
        if let EmitterShape::Line { length } = &mut emitter.shape
            && preset == Preset::Snow
        {
            // Snow falls across the whole stage.
            *length = self.width as f32 * 1.2;
        }
        let layer = self.layer_mut(id)?;
        layer.start = frame;
        layer.particles = Some(emitter);
        // Light adds up: fire and sparks glow over what's behind them.
        if matches!(preset, Preset::Sparks | Preset::Fire) {
            layer.blend = crate::BlendMode::Add;
        }
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn still(_: f32) -> Affine {
        Affine::IDENTITY
    }

    fn straight_up() -> Emitter {
        Emitter {
            rate: 10.0,
            spread: 0.0,
            speed: 100.0,
            speed_variance: 0.0,
            lifetime: 1.0,
            lifetime_variance: 0.0,
            size_variance: 0.0,
            start_full: false,
            ..Emitter::preset(Preset::Sparks)
        }
        .without_forces()
    }

    impl Emitter {
        fn without_forces(self) -> Self {
            Emitter { gravity: 0.0, wind: 0.0, drag: 0.0, turbulence: 0.0, ..self }
        }
    }

    #[test]
    fn particles_stream_out_at_the_rate_and_die_at_their_lifetime() {
        let emitter = straight_up();
        assert!(emitter.particles(0.0, still).len() <= 1);
        // Ten a second, each living a second: about ten at any time after.
        let count = emitter.particles(5.0, still).len();
        assert!((9..=11).contains(&count), "{count}");
    }

    #[test]
    fn a_particle_flies_in_its_direction_at_its_speed() {
        let emitter = straight_up();
        let particles = emitter.particles(5.0, still);
        for p in &particles {
            assert!(p.position.x.abs() < 1e-3, "{p:?}");
            // Never more than a second out, at 100 px a second.
            assert!((-100.0..=0.0).contains(&p.position.y), "{p:?}");
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_particles_and_another_seed_others() {
        let emitter = Emitter::preset(Preset::Smoke);
        assert_eq!(emitter.particles(2.5, still), emitter.particles(2.5, still));
        let other = Emitter { seed: 2, ..emitter.clone() };
        assert_ne!(emitter.particles(2.5, still), other.particles(2.5, still));
    }

    #[test]
    fn gravity_pulls_and_drag_slows() {
        let fall = travelled(Point::ZERO, Point::new(0.0, 100.0), 0.0, 2.0);
        assert!((fall.y - 200.0).abs() < 1e-3);
        // With drag, it can't fall faster than gravity / drag.
        let slowed = travelled(Point::ZERO, Point::new(0.0, 100.0), 2.0, 100.0);
        assert!((slowed.y / 100.0 - 50.0).abs() < 1.0, "{slowed:?}");
    }

    #[test]
    fn particles_stay_where_they_were_born_when_the_emitter_moves_on() {
        let emitter = Emitter { speed: 0.0, ..straight_up() };
        // The emitter moves 100 px right each second.
        let moving = |t: f32| Affine::translate(Point::new(t * 100.0, 0.0));
        let particles = emitter.particles(3.0, moving);
        let xs: Vec<f32> = particles.iter().map(|p| p.position.x).collect();
        // Born over the last second, so spread between 200 and 300.
        assert!(xs.iter().all(|x| (200.0..=300.0).contains(x)), "{xs:?}");
        assert!(xs.iter().any(|&x| x < 250.0) && xs.iter().any(|&x| x > 250.0));
    }

    #[test]
    fn starting_full_means_particles_from_the_first_frame() {
        let emitter = Emitter { start_full: true, ..straight_up() };
        assert!(emitter.particles(0.0, still).len() >= 9);
    }

    #[test]
    fn particles_fade_and_shrink_as_they_age() {
        let emitter = Emitter {
            size_start: 10.0,
            size_end: 0.0,
            opacity_start: 1.0,
            opacity_end: 0.0,
            ..straight_up()
        };
        let particles = emitter.particles(5.0, still);
        let oldest = particles.iter().min_by(|a, b| a.position.y.total_cmp(&b.position.y)).unwrap();
        let youngest = particles.iter().max_by(|a, b| a.position.y.total_cmp(&b.position.y)).unwrap();
        assert!(oldest.opacity < youngest.opacity && oldest.size < youngest.size);
    }

    #[test]
    fn a_particle_layer_saves_and_loads() {
        let mut doc = Document::default();
        let id = doc.add_particle_layer(Preset::Snow, 0, Point::new(960.0, 0.0)).unwrap();
        let loaded = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(loaded.layer(id).unwrap().particles, doc.layer(id).unwrap().particles);
        assert!(!loaded.particles_on_layer(loaded.layer(id).unwrap(), 10.0).is_empty());
    }
}
