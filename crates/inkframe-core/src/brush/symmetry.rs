//! Symmetry painting, as Photoshop's: every dab is stamped again in each
//! mirror copy (across a line, across two, or turned round a centre), all
//! into the one stroke, so where the copies overlap they're capped at the
//! stroke's opacity together.

use serde::{Deserialize, Serialize};

use crate::geometry::{Affine, Point};

/// How a stroke is repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Symmetry {
    #[default]
    Off,
    /// Mirrored left to right, across an up-and-down line.
    Vertical,
    /// Mirrored top to bottom, across a level line.
    Horizontal,
    /// Mirrored both ways: four copies.
    Both,
    /// Turned round the centre, this many times in all.
    Radial(u32),
    /// Turned round the centre and mirrored in each turn, like a mandala:
    /// twice this many copies.
    Mandala(u32),
}

impl Symmetry {
    pub const ALL: [Symmetry; 6] = [
        Symmetry::Off,
        Symmetry::Vertical,
        Symmetry::Horizontal,
        Symmetry::Both,
        Symmetry::Radial(6),
        Symmetry::Mandala(6),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Symmetry::Off => crate::i18n::tr_later("Off"),
            Symmetry::Vertical => crate::i18n::tr_later("Vertical"),
            Symmetry::Horizontal => crate::i18n::tr_later("Horizontal"),
            Symmetry::Both => crate::i18n::tr_later("Dual Axis"),
            Symmetry::Radial(_) => crate::i18n::tr_later("Radial"),
            Symmetry::Mandala(_) => crate::i18n::tr_later("Mandala"),
        }
    }

    /// How many ways round a radial or mandala symmetry is, if it is one.
    pub fn segments(self) -> Option<u32> {
        match self {
            Symmetry::Radial(n) | Symmetry::Mandala(n) => Some(n),
            _ => None,
        }
    }

    /// The copies besides the stroke itself, as maps from where the pen is
    /// to where each copy goes, around `centre` (canvas pixels).
    pub fn copies(self, centre: Point) -> Vec<Affine> {
        let around = |m: Affine| {
            Affine::translate(centre)
                .pre_concat(m)
                .pre_concat(Affine::translate(Point::new(-centre.x, -centre.y)))
        };
        let mirror_x = Affine::scale(-1.0, 1.0);
        let mirror_y = Affine::scale(1.0, -1.0);
        let turns =
            |n: u32| (1..n.max(1)).map(move |i| Affine::rotate(std::f32::consts::TAU * i as f32 / n as f32));
        let copies: Vec<Affine> = match self {
            Symmetry::Off => Vec::new(),
            Symmetry::Vertical => vec![mirror_x],
            Symmetry::Horizontal => vec![mirror_y],
            Symmetry::Both => vec![mirror_x, mirror_y, Affine::scale(-1.0, -1.0)],
            Symmetry::Radial(n) => turns(n).collect(),
            Symmetry::Mandala(n) => turns(n)
                .chain(std::iter::once(Affine::IDENTITY).chain(turns(n)).map(|t| t.pre_concat(mirror_x)))
                .collect(),
        };
        copies.into_iter().map(around).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_land_where_the_symmetry_puts_them() {
        let centre = Point::new(100.0, 100.0);
        let pen = Point::new(130.0, 80.0);
        let land = |s: Symmetry| s.copies(centre).iter().map(|m| m.apply(pen)).collect::<Vec<_>>();
        let near = |a: Point, b: Point| a.distance(b) < 1e-3;
        assert!(land(Symmetry::Off).is_empty());
        assert!(near(land(Symmetry::Vertical)[0], Point::new(70.0, 80.0)));
        assert!(near(land(Symmetry::Horizontal)[0], Point::new(130.0, 120.0)));
        assert_eq!(land(Symmetry::Both).len(), 3);
        let radial = land(Symmetry::Radial(4));
        assert_eq!(radial.len(), 3);
        assert!(radial.iter().all(|p| (p.distance(centre) - pen.distance(centre)).abs() < 1e-3));
        assert_eq!(land(Symmetry::Mandala(4)).len(), 7, "four turns, each mirrored, less the stroke itself");
    }
}
