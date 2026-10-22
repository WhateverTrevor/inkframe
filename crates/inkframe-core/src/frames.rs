//! Editing a layer's time as Animate's frame commands do: inserting frames
//! (F5), removing them (Shift+F5) and playing a stretch backwards (Reverse
//! Frames).
//!
//! Everything on a layer that happens at a frame goes through the same
//! remapping (`Layer::retime`): every property's keys, its deformer and
//! free-form deformer keys, its drawing swaps, its shape tweens, and where
//! it starts and ends. That way nothing is left behind at the old time.

use crate::channel::AnyChannel;
use crate::document::Layer;
use crate::drawing::Exposure;
use crate::track::Property;

impl Layer {
    /// Moves everything that happens at a frame to `map(frame)`, dropping
    /// what maps to `None`. `fallback` is where a channel left with no keys
    /// keeps the value it showed.
    fn retime(&mut self, map: &dyn Fn(u32) -> Option<u32>, fallback: u32) {
        for property in Property::ALL {
            self.track.channel_mut(property).retime(map, fallback);
        }
        if let Some(deformer) = &mut self.deformer {
            deformer.bends.iter_mut().for_each(|c| c.retime(map, fallback));
        }
        if let Some(warp) = &mut self.warp {
            warp.offsets.iter_mut().for_each(|c| c.retime(map, fallback));
        }
        if let Some(skeleton) = &mut self.skeleton {
            skeleton.channels_mut().for_each(|c| c.retime(map, fallback));
        }
        self.exposure = std::mem::take(&mut self.exposure)
            .into_iter()
            .filter_map(|key| Some(Exposure { frame: map(key.frame)?, ..key }))
            .collect();
        self.exposure.sort_by_key(|key| key.frame);
        self.exposure.dedup_by_key(|key| key.frame);
        self.symbol_swaps = std::mem::take(&mut self.symbol_swaps)
            .into_iter()
            .filter_map(|swap| Some(crate::SymbolSwap { frame: map(swap.frame)?, ..swap }))
            .collect();
        self.symbol_swaps.sort_by_key(|swap| swap.frame);
        self.symbol_swaps.dedup_by_key(|swap| swap.frame);
        self.shape_tweens.retain_mut(|tween| match map(tween.end) {
            Some(end) => {
                tween.end = end;
                true
            }
            None => false,
        });
    }

    /// Animate's Insert Frame (F5): `count` frames go in after `at`, so
    /// what shows on `at` is held that much longer and everything later
    /// happens that much later. A layer that ends after `at` ends later.
    pub fn insert_frames(&mut self, at: u32, count: u32) {
        if count == 0 {
            return;
        }
        let later = |f: u32| if f > at { f + count } else { f };
        self.retime(&|f| Some(later(f)), at);
        if self.start > at {
            self.start += count;
        }
        if let Some(end) = &mut self.end
            && *end >= at
        {
            *end += count;
        }
    }

    /// Animate's Remove Frames (Shift+F5): frames `first` to `last` go, and
    /// everything after them happens that much earlier. Keys and drawing
    /// swaps on the removed frames go with them; the drawing showing just
    /// after them is kept showing there.
    pub fn remove_frames(&mut self, first: u32, last: u32) {
        let (first, last) = (first.min(last), first.max(last));
        let count = last - first + 1;
        // Pin the drawing showing just after the gap, which might come from
        // a swap being removed.
        let after = last + 1;
        if self.exposure.iter().any(|k| (first..=last).contains(&k.frame))
            && !self.exposure.iter().any(|k| k.frame == after)
        {
            let drawing = self.drawing_index_at(after);
            self.exposure.push(Exposure { frame: after, drawing });
            self.exposure.sort_by_key(|key| key.frame);
        }
        let map = |f: u32| match f {
            f if f < first => Some(f),
            f if f <= last => None,
            f => Some(f - count),
        };
        self.retime(&map, first);
        self.start = match self.start {
            s if s < first => s,
            s if s <= last => first,
            s => s - count,
        };
        if let Some(end) = &mut self.end {
            *end = match *end {
                e if e < first => e,
                e if e <= last => first.saturating_sub(1).max(self.start),
                e => e - count,
            };
        }
    }

    /// Animate's Reverse Frames: frames `first` to `last` play backwards.
    /// Each property that changes there gets keys at both ends first, so
    /// the stretch reverses exactly; drawing swaps are mirrored frame by
    /// frame. Shape tweens inside the stretch can't run backwards, so
    /// they're dropped.
    pub fn reverse_frames(&mut self, first: u32, last: u32) {
        let (first, last) = (first.min(last), first.max(last));
        if first == last {
            return;
        }
        for property in Property::ALL {
            reverse_channel(self.track.channel_mut(property), first, last);
        }
        if let Some(deformer) = &mut self.deformer {
            deformer.bends.iter_mut().for_each(|c| reverse_channel(c, first, last));
        }
        if let Some(warp) = &mut self.warp {
            warp.offsets.iter_mut().for_each(|c| reverse_channel(c, first, last));
        }
        if let Some(skeleton) = &mut self.skeleton {
            skeleton.channels_mut().for_each(|c| reverse_channel(c, first, last));
        }

        // Drawings: what showed on each frame now shows on its mirror.
        let shown: Vec<usize> = (first..=last + 1).map(|f| self.drawing_index_at(f)).collect();
        let before = first.checked_sub(1).map(|f| self.drawing_index_at(f));
        self.exposure.retain(|k| !(first..=last + 1).contains(&k.frame));
        let mut previous = before;
        for frame in first..=last + 1 {
            let drawing = if frame <= last { shown[(last - frame) as usize] } else { shown[shown.len() - 1] };
            if previous != Some(drawing) || frame == first && before.is_none() && drawing != 0 {
                self.exposure.push(Exposure { frame, drawing });
            }
            previous = Some(drawing);
        }
        self.exposure.sort_by_key(|key| key.frame);
        self.shape_tweens.retain(|tween| !(first..=last).contains(&tween.end));

        // Symbol swaps likewise, for an instance.
        if let Some(instance) = self.instance {
            let shown: Vec<_> = (first..=last + 1).filter_map(|f| self.symbol_at(f)).collect();
            let before = first.checked_sub(1).and_then(|f| self.symbol_at(f)).unwrap_or(instance.symbol);
            self.symbol_swaps.retain(|s| !(first..=last + 1).contains(&s.frame));
            let mut previous = before;
            for (i, frame) in (first..=last + 1).enumerate() {
                let symbol = if frame <= last { shown[(last - frame) as usize] } else { shown[i] };
                if symbol != previous {
                    self.symbol_swaps.push(crate::SymbolSwap { frame, symbol });
                }
                previous = symbol;
            }
            self.symbol_swaps.sort_by_key(|s| s.frame);
        }
    }
}

/// Reverses one channel's keys from `first` to `last`, if it changes there,
/// keying both ends first so the reversal is exact.
fn reverse_channel(channel: &mut dyn AnyChannel, first: u32, last: u32) {
    let moves = channel.key_frames().iter().any(|f| (first..=last).contains(f))
        || channel.tween_at(first).is_some()
        || channel.tween_at(last).is_some();
    if moves {
        channel.insert_key(first);
        channel.insert_key(last);
        channel.reverse_span(first, last);
    }
}

#[cfg(test)]
mod tests {
    use crate::{Document, Drawing, Easing, Point, Transform};

    use super::*;

    /// A layer moving from x 0 on frame 0 to x 100 on frame 10, easing in,
    /// with a second drawing swapped in on frame 5.
    fn moving_layer() -> Layer {
        let mut doc = Document::default();
        let id = doc.add_layer("Ball", Vec::new(), 0, Transform::default());
        let mut layer = doc.layer(id).unwrap().clone();
        layer.track.position.set(10, Point::new(100.0, 0.0));
        *layer.track.position.tween_easing_mut(0).unwrap() = Easing::EaseIn;
        let second = layer.add_drawing(Drawing::new("2", Vec::new()));
        layer.expose(5, second);
        layer
    }

    #[test]
    fn inserting_frames_holds_the_current_one_longer() {
        let mut layer = moving_layer();
        layer.end = Some(20);
        layer.insert_frames(5, 3);
        assert_eq!(layer.track.position.key_frames(), vec![0, 13]);
        assert_eq!(layer.exposure.iter().map(|k| k.frame).collect::<Vec<_>>(), vec![5], "the swap is on 5");
        assert_eq!(layer.drawing_index_at(8), 1, "frame 5's drawing holds through 8");
        assert_eq!(layer.end, Some(23));
    }

    #[test]
    fn removing_frames_brings_the_rest_earlier() {
        let mut layer = moving_layer();
        layer.end = Some(20);
        layer.remove_frames(4, 6);
        assert_eq!(layer.track.position.key_frames(), vec![0, 7]);
        // The swap on 5 went with the frames, but frame 7's drawing (the
        // second) still shows where frame 7 now is, on 4.
        assert_eq!(layer.drawing_index_at(3), 0);
        assert_eq!(layer.drawing_index_at(4), 1);
        assert_eq!(layer.end, Some(17));

        // Removing every key leaves the value that showed.
        let mut layer = moving_layer();
        layer.remove_frames(0, 10);
        assert_eq!(layer.track.position.keys().len(), 1);
    }

    #[test]
    fn reversing_frames_plays_them_backwards() {
        let mut layer = moving_layer();
        layer.reverse_frames(0, 10);
        let position = |f: f32| layer.track.position.sample(f).unwrap().x;
        assert_eq!(position(0.0), 100.0);
        assert_eq!(position(10.0), 0.0);
        // It eased in going forwards, so backwards it eases out: fast first.
        assert!(position(2.0) < 100.0 - 20.0, "{}", position(2.0));
        assert_eq!(layer.track.position.tween_at(0).unwrap().easing, Easing::EaseOut);
        // Drawing 1 showed on 5 to 10; now on 0 to 5, then the first again.
        assert_eq!(layer.drawing_index_at(0), 1);
        assert_eq!(layer.drawing_index_at(5), 1);
        assert_eq!(layer.drawing_index_at(6), 0);
        assert_eq!(layer.drawing_index_at(10), 0);
        assert_eq!(layer.drawing_index_at(11), 1, "after the stretch, as before");
    }

    #[test]
    fn easing_reverses_into_its_mirror() {
        let curve = Easing::CubicBezier { x1: 0.1, y1: 0.2, x2: 0.3, y2: 0.4 };
        let Easing::CubicBezier { x1, y1, x2, y2 } = curve.reversed() else { panic!() };
        assert!((x1 - 0.7).abs() < 1e-6 && (y1 - 0.6).abs() < 1e-6);
        assert!((x2 - 0.9).abs() < 1e-6 && (y2 - 0.8).abs() < 1e-6);
        let Easing::CubicBezier { x1, y2, .. } = curve.reversed().reversed() else { panic!() };
        assert!((x1 - 0.1).abs() < 1e-6 && (y2 - 0.4).abs() < 1e-6, "twice is the original");
        assert_eq!(Easing::EaseIn.reversed(), Easing::EaseOut);
    }
}
