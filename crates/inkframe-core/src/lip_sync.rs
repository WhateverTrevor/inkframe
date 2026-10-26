//! Lip sync: choosing a mouth drawing for every frame of a line of dialogue.
//!
//! Animators draw a small set of standard mouth shapes and swap between
//! them (see `drawing.rs`). The set used here is Rhubarb Lip Sync's, itself
//! based on Preston Blair's classic chart, which is what Harmony and
//! OpenToonz use too:
//!
//! | Shape | Mouth | Sounds |
//! |---|---|---|
//! | A | closed | M, B, P |
//! | B | slightly open, teeth together | most consonants, EE |
//! | C | open | EH, AE |
//! | D | wide open | AA |
//! | E | slightly rounded | AO, ER |
//! | F | puckered | UW, OW, W |
//! | G | top teeth on lower lip | F, V |
//! | H | tongue up | long L |
//! | X | at rest | silence |
//!
//! The timing ("from 1.24 s, shape D") comes from one of three places:
//! a timing file from Rhubarb, which listens for the actual sounds; one from
//! Papagayo, where someone lines up the words by hand; or the loudness of
//! the dialogue, which can't tell an O from an E but opens the mouth
//! wider as the voice gets louder, often enough for quick work.
//!
//! [`MouthMap`] then says which of the mouth layer's drawings shows each
//! shape, guessed from the drawings' names, and [`Layer::apply_lip_sync`]
//! writes the result as exposure keys.

use crate::document::Layer;

/// A standard mouth shape; see the module docs for what each looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouthShape {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    X,
}

impl MouthShape {
    pub const ALL: [MouthShape; 9] = {
        use MouthShape::*;
        [A, B, C, D, E, F, G, H, X]
    };

    /// The shape's letter, as Rhubarb writes it.
    pub fn letter(self) -> &'static str {
        use MouthShape::*;
        match self {
            A => "A",
            B => "B",
            C => "C",
            D => "D",
            E => "E",
            F => "F",
            G => "G",
            H => "H",
            X => "X",
        }
    }

    pub fn from_letter(letter: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shape| shape.letter().eq_ignore_ascii_case(letter))
    }

    /// What the shape looks like and which sounds it's for.
    pub fn description(self) -> &'static str {
        use MouthShape::*;
        match self {
            A => "Closed: M, B, P",
            B => "Slightly open: most consonants, EE",
            C => "Open: EH, AE",
            D => "Wide open: AA",
            E => "Slightly rounded: AO, ER",
            F => "Puckered: OO, W",
            G => "Teeth on lip: F, V",
            H => "Tongue up: L",
            X => "At rest: silence",
        }
    }

    /// Papagayo's name for the closest of its phonemes.
    fn papagayo_name(self) -> &'static str {
        use MouthShape::*;
        match self {
            A => "MBP",
            B => "etc",
            C => "E",
            D => "AI",
            E => "O",
            F => "U",
            G => "FV",
            H => "L",
            X => "rest",
        }
    }

    fn from_papagayo(phoneme: &str) -> Option<Self> {
        use MouthShape::*;
        // WQ isn't anyone's best match, so it's not in `papagayo_name`.
        if phoneme.eq_ignore_ascii_case("WQ") {
            return Some(F);
        }
        Self::ALL.into_iter().find(|shape| shape.papagayo_name().eq_ignore_ascii_case(phoneme))
    }

    /// The shape to use instead when a mouth has no drawing for this one.
    /// Rhubarb calls G, H and X optional, and many mouths are drawn with
    /// only a few shapes, so each falls back to its nearest neighbour,
    /// ending at A (closed).
    fn fallback(self) -> Option<Self> {
        use MouthShape::*;
        match self {
            A => None,
            B | X => Some(A),
            C | G => Some(B),
            D | E | H => Some(C),
            F => Some(E),
        }
    }
}

/// From `frame` (counted from the start of the dialogue), show `shape`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouthCue {
    pub frame: u32,
    pub shape: MouthShape,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LipSyncError {
    #[error("line {0} isn't a time and a mouth shape")]
    BadLine(usize),
    #[error("\"{0}\" isn't a mouth shape Inkframe knows")]
    UnknownShape(String),
    #[error("the file has no mouth shapes in it")]
    Empty,
}

/// Reads Rhubarb Lip Sync's default output: one line per change, the time
/// in seconds, a tab, and the shape's letter (tabs shown as spaces here).
///
/// ```text
/// 0.00    X
/// 0.05    D
/// 0.27    C
/// ```
pub fn parse_rhubarb(text: &str, fps: f64) -> Result<Vec<MouthCue>, LipSyncError> {
    let mut cues = Vec::new();
    for (number, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let mut parts = line.split_whitespace();
        let (Some(time), Some(letter)) = (parts.next(), parts.next()) else {
            return Err(LipSyncError::BadLine(number + 1));
        };
        let seconds: f64 = time.parse().map_err(|_| LipSyncError::BadLine(number + 1))?;
        let shape =
            MouthShape::from_letter(letter).ok_or_else(|| LipSyncError::UnknownShape(letter.into()))?;
        cues.push(MouthCue { frame: (seconds * fps).round().max(0.0) as u32, shape });
    }
    tidy(cues)
}

/// Reads a Papagayo (or Moho) switch file: a `MohoSwitch1` header, then
/// one line per change, a 1-based frame number and a phoneme. The frames
/// are at whatever rate Papagayo was set to, which should match the
/// animation's.
///
/// ```text
/// MohoSwitch1
/// 1 rest
/// 12 AI
/// 15 MBP
/// ```
pub fn parse_papagayo(text: &str) -> Result<Vec<MouthCue>, LipSyncError> {
    let mut cues = Vec::new();
    let lines = text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty());
    for (number, line) in lines.skip_while(|(_, l)| l.trim().starts_with("MohoSwitch")) {
        let mut parts = line.split_whitespace();
        let (Some(frame), Some(phoneme)) = (parts.next(), parts.next()) else {
            return Err(LipSyncError::BadLine(number + 1));
        };
        let frame: u32 = frame.parse().map_err(|_| LipSyncError::BadLine(number + 1))?;
        let shape =
            MouthShape::from_papagayo(phoneme).ok_or_else(|| LipSyncError::UnknownShape(phoneme.into()))?;
        cues.push(MouthCue { frame: frame.saturating_sub(1), shape });
    }
    tidy(cues)
}

/// Mouth cues from a timing file's text, Papagayo's (which says so on its
/// first line, `MohoSwitch1`) or else Rhubarb's.
pub fn parse_timing(text: &str, fps: f64) -> Result<Vec<MouthCue>, LipSyncError> {
    if text.trim_start().starts_with("MohoSwitch") { parse_papagayo(text) } else { parse_rhubarb(text, fps) }
}

/// Mouth shapes from loudness alone: closed in silence, opening wider as
/// the voice gets louder. `peaks` is the loudest sample in each frame.
///
/// Levels are measured against the loudest frame, so quiet and loud
/// recordings both use the full range of shapes. A shape has to last two
/// frames to show: speech flickers in loudness far faster than a mouth
/// should, and animators hold mouths for at least two frames anyway.
pub fn cues_from_loudness(peaks: &[f32]) -> Vec<MouthCue> {
    let loudest = peaks.iter().copied().fold(0.0f32, f32::max);
    if loudest <= 0.0 {
        return vec![MouthCue { frame: 0, shape: MouthShape::X }];
    }
    let shape_for = |peak: f32| match peak / loudest {
        level if level < 0.12 => MouthShape::X,
        level if level < 0.4 => MouthShape::B,
        level if level < 0.7 => MouthShape::C,
        _ => MouthShape::D,
    };
    let mut cues: Vec<MouthCue> = Vec::new();
    for (frame, window) in peaks.windows(2).enumerate() {
        let shape = shape_for(window[0]);
        // Only change if the next frame agrees; a single frame is a blip.
        let lasts = shape_for(window[1]) == shape;
        if cues.last().is_none_or(|cue| cue.shape != shape && lasts) {
            cues.push(MouthCue { frame: frame as u32, shape });
        }
    }
    // The sound has ended: close the mouth.
    cues.push(MouthCue { frame: peaks.len() as u32, shape: MouthShape::X });
    tidy(cues).unwrap_or_default()
}

/// Sorts cues, keeps the last of any on the same frame, and drops those
/// that repeat the shape before them.
fn tidy(mut cues: Vec<MouthCue>) -> Result<Vec<MouthCue>, LipSyncError> {
    if cues.is_empty() {
        return Err(LipSyncError::Empty);
    }
    cues.sort_by_key(|cue| cue.frame);
    let mut tidied: Vec<MouthCue> = Vec::with_capacity(cues.len());
    for cue in cues {
        match tidied.last_mut() {
            Some(last) if last.frame == cue.frame => *last = cue,
            _ => tidied.push(cue),
        }
    }
    tidied.dedup_by(|later, earlier| later.shape == earlier.shape);
    Ok(tidied)
}

/// Which of a mouth layer's drawings shows each shape. `None` means "no
/// drawing of its own; use the fallback shape's".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MouthMap {
    drawings: [Option<usize>; 9],
}

impl MouthMap {
    /// Guesses from the drawings' names. Drawings named with Rhubarb's
    /// letters (A–H, X) or Papagayo's phonemes (MBP, etc, E, AI, O, U, FV,
    /// L, rest) are matched; "closed" and "open" are understood too. The
    /// two schemes share "E" with different meanings, so the layer's other
    /// names decide which it uses.
    pub fn guess(layer: &Layer) -> Self {
        let named = |name: &str| layer.drawings.iter().position(|d| d.name.trim().eq_ignore_ascii_case(name));
        let papagayo = ["MBP", "etc", "AI", "rest", "FV"].iter().any(|n| named(n).is_some());
        let mut map = Self::default();
        for shape in MouthShape::ALL {
            let own = if papagayo { named(shape.papagayo_name()) } else { named(shape.letter()) };
            let common = match shape {
                MouthShape::A => named("closed"),
                MouthShape::D => named("open"),
                _ => None,
            };
            map.set(shape, own.or(common));
        }
        map
    }

    pub fn get(&self, shape: MouthShape) -> Option<usize> {
        self.drawings[shape as usize]
    }

    pub fn set(&mut self, shape: MouthShape, drawing: Option<usize>) {
        self.drawings[shape as usize] = drawing;
    }

    /// The drawing to show for `shape`: its own, or its fallback's, or the
    /// layer's first drawing if nothing along the way has one.
    pub fn drawing_for(&self, shape: MouthShape) -> usize {
        std::iter::successors(Some(shape), |s| s.fallback()).find_map(|s| self.get(s)).unwrap_or(0)
    }
}

impl Layer {
    /// Replaces the layer's drawing swaps between `start` and `end`
    /// (inclusive) with the mouth shapes in `cues`, whose frames count from
    /// `start`. Cues past `end` are left out.
    pub fn apply_lip_sync(&mut self, cues: &[MouthCue], map: &MouthMap, start: u32, end: u32) {
        self.exposure.retain(|key| key.frame < start || key.frame > end);
        for cue in cues {
            let frame = start + cue.frame;
            if frame <= end {
                self.expose(frame, map.drawing_for(cue.shape));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Drawing, Point, Transform};
    use MouthShape::*;

    fn cue(frame: u32, shape: MouthShape) -> MouthCue {
        MouthCue { frame, shape }
    }

    fn mouth(names: &[&str]) -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("Mouth", vec![], 0, Transform::at(Point::ZERO));
        let mut layer = doc.layer(id).unwrap().clone();
        layer.drawings = names.iter().map(|n| Drawing::new(*n, Vec::new())).collect();
        layer
    }

    #[test]
    fn reads_rhubarb_timing() {
        let text = "0.00\tX\n0.05\tD\n0.27\tC\n0.31\tB\n0.43\tX\n";
        let cues = parse_rhubarb(text, 24.0).unwrap();
        assert_eq!(cues, [cue(0, X), cue(1, D), cue(6, C), cue(7, B), cue(10, X)]);
    }

    #[test]
    fn changes_closer_than_a_frame_keep_the_later() {
        // At 24 fps, 0.30 and 0.31 both land on frame 7.
        let cues = parse_rhubarb("0.0 X\n0.30 C\n0.31 B\n", 24.0).unwrap();
        assert_eq!(cues, [cue(0, X), cue(7, B)]);
    }

    #[test]
    fn rhubarb_errors_name_the_problem() {
        assert_eq!(parse_rhubarb("0.0 X\nhello\n", 24.0), Err(LipSyncError::BadLine(2)));
        assert_eq!(parse_rhubarb("0.0 Q\n", 24.0), Err(LipSyncError::UnknownShape("Q".into())));
        assert_eq!(parse_rhubarb("\n", 24.0), Err(LipSyncError::Empty));
    }

    #[test]
    fn reads_papagayo_switch_files() {
        let text = "MohoSwitch1\n1 rest\n12 AI\n15 MBP\n18 WQ\n20 etc\n";
        let cues = parse_papagayo(text).unwrap();
        assert_eq!(cues, [cue(0, X), cue(11, D), cue(14, A), cue(17, F), cue(19, B)]);
    }

    #[test]
    fn loudness_opens_the_mouth_wider_when_louder() {
        let peaks = [0.0, 0.0, 0.3, 0.3, 0.9, 0.9, 0.9, 0.5, 0.5, 0.0, 0.0];
        let cues = cues_from_loudness(&peaks);
        assert_eq!(cues, [cue(0, X), cue(2, B), cue(4, D), cue(7, C), cue(9, X)]);
    }

    #[test]
    fn single_frame_blips_are_ignored() {
        let peaks = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        assert_eq!(cues_from_loudness(&peaks), [cue(0, X), cue(5, D), cue(7, X)]);
    }

    #[test]
    fn silence_is_a_closed_mouth() {
        assert_eq!(cues_from_loudness(&[0.0; 5]), [cue(0, X)]);
    }

    #[test]
    fn drawings_named_with_rhubarb_letters_are_matched() {
        let map = MouthMap::guess(&mouth(&["X", "A", "B", "C", "D", "E", "F"]));
        assert_eq!(map.drawing_for(X), 0);
        assert_eq!(map.drawing_for(D), 4);
        assert_eq!(map.drawing_for(E), 5);
        // No G or H drawings: they borrow B and C's.
        assert_eq!(map.drawing_for(G), 2);
        assert_eq!(map.drawing_for(H), 3);
    }

    #[test]
    fn papagayo_names_read_e_as_its_open_vowel() {
        let map = MouthMap::guess(&mouth(&["rest", "MBP", "etc", "E", "AI", "O", "U"]));
        // Papagayo's "E" is Rhubarb's C, and its "O" is Rhubarb's E.
        assert_eq!(map.drawing_for(C), 3);
        assert_eq!(map.drawing_for(E), 5);
        assert_eq!(map.drawing_for(A), 1);
    }

    #[test]
    fn a_two_drawing_mouth_opens_and_closes() {
        let map = MouthMap::guess(&mouth(&["closed", "open"]));
        assert_eq!([A, B, C, D, E, F, G, H, X].map(|s| map.drawing_for(s)), [0, 0, 0, 1, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn lip_sync_replaces_the_swaps_under_the_dialogue() {
        let mut layer = mouth(&["X", "A", "D"]);
        layer.expose(2, 1);
        layer.expose(12, 2);
        layer.expose(40, 1);
        let map = MouthMap::guess(&layer);
        layer.apply_lip_sync(&[cue(0, D), cue(3, A), cue(30, D)], &map, 10, 20);

        let shown: Vec<usize> = [2, 10, 12, 13, 25, 40].map(|f| layer.drawing_index_at(f)).to_vec();
        // Before and after the dialogue is untouched; the cue past its end
        // is dropped.
        assert_eq!(shown, [1, 2, 2, 1, 1, 1]);
        assert!(layer.exposure.iter().all(|k| k.frame != 12), "the old swap inside was removed");
    }
}
