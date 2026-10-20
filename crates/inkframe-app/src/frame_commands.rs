//! Animate's frame commands, for drawing animation by hand: Insert Frame
//! (F5), Remove Frames (Shift+F5), Insert Blank Keyframe (F7), Convert to
//! Keyframes and to Blank Keyframes, and Reverse Frames. They act on the
//! frames picked in the timeline (Shift-click or Shift-drag along a row),
//! or the current frame when none are, in every selected layer.
//!
//! Also Edit Multiple Frames: with it on, dragging layers on the stage
//! moves them across the picked frames at once, not just on the current
//! frame. The time edits themselves are in the core (`frames.rs`).

use inkframe_core::{Affine, Element, LayerId, Point, Property};

use crate::editor::Editor;

/// What dragging with Edit Multiple Frames on is moving, as it was when the
/// drag began, so every update works from the start.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiFrameMove {
    layers: Vec<(LayerId, Moving)>,
}

#[derive(Debug, Clone, PartialEq)]
enum Moving {
    /// A layer that moves by keys: its position keys on the picked frames,
    /// with the map into the space they're in (its parent's).
    Keys { keys: Vec<(u32, Point)>, into_space: Affine },
    /// A layer drawn frame by frame (its position never changes), or with
    /// no position keys on the picked frames: the
    /// drawings it shows on the picked frames (line art and colour art),
    /// with the map into its own coordinates.
    Drawings { drawings: Vec<(usize, Vec<Element>, Vec<Element>)>, to_local: Affine },
}

impl Editor {
    /// The picked frames, first and last, or the current frame alone.
    pub fn frame_span(&self) -> (u32, u32) {
        match self.frame_range {
            Some((a, b)) => (a.min(b), a.max(b)),
            None => (self.frame, self.frame),
        }
    }

    /// The layers the frame commands change: the selected ones (a folder
    /// meaning what's in it), unless locked.
    fn frame_layers(&self) -> Vec<LayerId> {
        self.selected_with_folders()
            .into_iter()
            .filter(|&id| self.doc.layer(id).is_some_and(|l| !l.folder && !l.locked))
            .collect()
    }

    /// Lengthens the timeline being edited so `frame` is on it, as Animate's
    /// timeline grows when frames are added past its end.
    fn grow_to(&mut self, frame: u32) {
        if frame <= self.last_frame() {
            return;
        }
        match self.timeline() {
            inkframe_core::TimelineId::Main => self.doc.duration = frame + 1,
            inkframe_core::TimelineId::Symbol(id) => {
                if let Some(symbol) = self.doc.symbol_mut(id) {
                    symbol.duration = frame + 1;
                }
            }
        }
    }

    /// The last frame anything on a layer happens: its last key or swap.
    fn last_event(&self, id: LayerId) -> u32 {
        let Some(layer) = self.doc.layer(id) else { return 0 };
        let keys = Property::ALL.into_iter().flat_map(|p| layer.track.channel(p).key_frames());
        keys.chain(layer.exposure.iter().map(|k| k.frame)).chain(layer.end).max().unwrap_or(0)
    }

    /// F5: as many frames as are picked (or one) go in at the start of the
    /// picked frames, holding what shows there longer.
    pub fn insert_frames(&mut self) {
        let (first, last) = self.frame_span();
        let count = last - first + 1;
        for id in self.frame_layers() {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.insert_frames(first, count);
            }
            self.grow_to(self.last_event(id));
        }
    }

    /// Shift+F5: the picked frames (or the current one) go, and what came
    /// after them moves up.
    pub fn remove_frames(&mut self) {
        let (first, last) = self.frame_span();
        for id in self.frame_layers() {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.remove_frames(first, last);
            }
        }
        self.frame_range = None;
    }

    /// Plays the picked frames backwards. Needs at least two picked.
    pub fn reverse_frames(&mut self) -> bool {
        let (first, last) = self.frame_span();
        if first == last {
            return false;
        }
        for id in self.frame_layers() {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.reverse_frames(first, last);
            }
        }
        true
    }

    /// F7: a new, empty drawing on the current frame, to draw the next
    /// pose from scratch. With frames picked, one on each of them (Convert
    /// to Blank Keyframes).
    pub fn insert_blank_keyframe(&mut self) {
        if self.frame_range.is_some() {
            self.convert_to_keyframes(true);
            return;
        }
        let frame = self.frame;
        for id in self.frame_layers() {
            self.new_drawing_on(id, frame, false);
            self.extend_layer_to(id, frame);
        }
    }

    /// Animate's Convert to Keyframes: every picked frame gets a drawing of
    /// its own (a copy of what showed there, or with `blank`, an empty
    /// one), and properties tweening through them get a key on each, so
    /// every frame can be changed by itself.
    pub fn convert_to_keyframes(&mut self, blank: bool) {
        let (first, last) = self.frame_span();
        for id in self.frame_layers() {
            if !blank && let Some(layer) = self.doc.layer_mut(id) {
                for property in Property::ALL {
                    let channel = layer.track.channel_mut(property);
                    for frame in first..=last {
                        if channel.tween_at(frame).is_some() {
                            channel.insert_key(frame);
                        }
                    }
                }
            }
            for frame in first..=last {
                self.new_drawing_on(id, frame, !blank);
            }
            self.extend_layer_to(id, last);
        }
    }

    /// Makes a layer that ended before `frame` last until it.
    fn extend_layer_to(&mut self, id: LayerId, frame: u32) {
        if let Some(layer) = self.doc.layer_mut(id)
            && layer.end.is_some_and(|end| end < frame)
        {
            layer.set_end(Some(frame));
        }
        self.grow_to(frame);
    }

    /// The range Edit Multiple Frames moves across: the picked frames, or
    /// the whole timeline when none are.
    pub fn multiple_frames(&self) -> (u32, u32) {
        if self.frame_range.is_some() { self.frame_span() } else { (0, self.last_frame()) }
    }

    /// Notes what dragging `ids` with Edit Multiple Frames on will move: an
    /// animated layer's position keys in the range, or for a layer drawn
    /// frame by frame (or with no keys there), the drawings it shows in
    /// the range.
    pub fn begin_multi_frame_move(&self, ids: &[LayerId]) -> MultiFrameMove {
        let (first, last) = self.multiple_frames();
        let frame = self.frame;
        let layers = ids
            .iter()
            .filter_map(|&id| {
                let layer = self.doc.layer(id)?;
                let keys: Vec<(u32, Point)> = layer
                    .track
                    .position
                    .keys()
                    .iter()
                    .filter(|k| (first..=last).contains(&k.frame))
                    .map(|k| (k.frame, k.value))
                    .collect();
                // A layer whose position never changes is one drawn frame
                // by frame (its one key just places it): its drawings move.
                let animated = layer.track.position.keys().len() > 1;
                let moving = if keys.is_empty() || !animated {
                    let mut shown: Vec<usize> = (first..=last).map(|f| layer.drawing_index_at(f)).collect();
                    shown.sort_unstable();
                    shown.dedup();
                    let drawings = shown
                        .into_iter()
                        .filter_map(|i| {
                            let d = layer.drawings.get(i)?;
                            Some((i, d.elements.clone(), d.colour_art.clone()))
                        })
                        .collect();
                    let to_local = self.doc.layer_matrix(layer, frame).inverse()?;
                    Moving::Drawings { drawings, to_local }
                } else {
                    let into_space =
                        self.doc.parent_matrix(layer, frame).inverse().unwrap_or(Affine::IDENTITY);
                    Moving::Keys { keys, into_space }
                };
                Some((id, moving))
            })
            .collect();
        MultiFrameMove { layers }
    }

    /// Moves everything noted in `moving` by the drag from `grab` to
    /// `grab + delta` (in the timeline's coordinates).
    pub fn apply_multi_frame_move(&mut self, moving: &MultiFrameMove, grab: Point, delta: Point) {
        for (id, what) in &moving.layers {
            let Some(layer) = self.doc.layer_mut(*id) else { continue };
            match what {
                Moving::Keys { keys, into_space } => {
                    let by = into_space.apply(grab + delta) - into_space.apply(grab);
                    for &(frame, start) in keys {
                        layer.track.position.set(frame, start + by);
                    }
                }
                Moving::Drawings { drawings, to_local } => {
                    let by = to_local.apply(grab + delta) - to_local.apply(grab);
                    let shift = Affine::translate(by);
                    for (index, lines, colour) in drawings {
                        let Some(drawing) = layer.drawings.get_mut(*index) else { continue };
                        let moved = |e: &Element| e.transformed(shift).unwrap_or_else(|| e.clone());
                        drawing.elements = lines.iter().map(moved).collect();
                        drawing.colour_art = colour.iter().map(moved).collect();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use inkframe_core::{AnyChannel, Color, Document, Shape, Style, Transform};

    use super::*;

    /// A ball keyed at x 0 on frame 0 and x 100 on frame 10, selected.
    fn ball() -> (Editor, LayerId) {
        let mut editor = Editor::new(Document::default());
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let id = editor.doc.add_layer("Ball", vec![dot], 0, Transform::default());
        editor.doc.layer_mut(id).unwrap().track.position.set(10, Point::new(100.0, 0.0));
        editor.select(Some(id));
        (editor, id)
    }

    #[test]
    fn f5_and_shift_f5_hold_and_close_up_frames() {
        let (mut editor, id) = ball();
        editor.set_frame(4);
        editor.frame_range = Some((4, 6));
        editor.insert_frames();
        assert_eq!(editor.doc.layer(id).unwrap().track.position.key_frames(), vec![0, 13]);
        editor.remove_frames();
        assert_eq!(editor.doc.layer(id).unwrap().track.position.key_frames(), vec![0, 10]);
        assert_eq!(editor.frame_range, None);
    }

    #[test]
    fn inserting_past_the_end_lengthens_the_timeline() {
        let (mut editor, id) = ball();
        editor.doc.duration = 12;
        editor.set_frame(5);
        editor.frame_range = Some((5, 9));
        editor.insert_frames();
        assert_eq!(editor.doc.layer(id).unwrap().track.position.key_frames(), vec![0, 15]);
        assert_eq!(editor.doc.duration, 16);
    }

    #[test]
    fn f7_and_convert_to_keyframes_give_frames_drawings_of_their_own() {
        let (mut editor, id) = ball();
        editor.set_frame(3);
        editor.insert_blank_keyframe();
        let layer = editor.doc.layer(id).unwrap();
        assert!(layer.elements_at(3).is_empty(), "a blank drawing from frame 3");
        assert_eq!(layer.elements_at(2).len(), 1);

        let (mut editor, id) = ball();
        editor.frame_range = Some((2, 4));
        editor.convert_to_keyframes(false);
        let layer = editor.doc.layer(id).unwrap();
        let shown: Vec<usize> = (1..=5).map(|f| layer.drawing_index_at(f)).collect();
        assert_eq!(shown[0], 0);
        assert!(shown[1] != shown[2] && shown[2] != shown[3], "frames 2, 3, 4 each their own: {shown:?}");
        assert_eq!(layer.elements_at(3).len(), 1, "copies of the drawing");
        assert_eq!(layer.track.position.key_frames(), vec![0, 2, 3, 4, 10], "the tween keyed on each");
    }

    #[test]
    fn edit_multiple_frames_moves_the_whole_motion() {
        let (mut editor, id) = ball();
        editor.frame_range = Some((0, 10));
        let moving = editor.begin_multi_frame_move(&[id]);
        editor.apply_multi_frame_move(&moving, Point::ZERO, Point::new(0.0, 50.0));
        let keys: Vec<Point> =
            editor.doc.layer(id).unwrap().track.position.keys().iter().map(|k| k.value).collect();
        assert_eq!(keys, vec![Point::new(0.0, 50.0), Point::new(100.0, 50.0)]);
    }

    #[test]
    fn edit_multiple_frames_moves_frame_by_frame_drawings() {
        let (mut editor, id) = ball();
        // Hand-drawn instead: no motion, a second drawing from frame 5.
        let layer = editor.doc.layer_mut(id).unwrap();
        layer.track.position.remove_key(10);
        editor.new_drawing_on(id, 5, true);
        let moving = editor.begin_multi_frame_move(&[id]);
        editor.apply_multi_frame_move(&moving, Point::ZERO, Point::new(30.0, 0.0));
        let layer = editor.doc.layer(id).unwrap();
        for frame in [0, 5] {
            let center = layer.elements_at(frame)[0].bounds().center();
            assert!(center.distance(Point::new(30.0, 0.0)) < 1e-3, "frame {frame}: {center:?}");
        }
        assert_eq!(layer.track.position.keys().len(), 1, "the layer itself didn't move");
    }
}
