//! Symbols: reusable animations, and instances of them placed on layers.
//!
//! A [`Symbol`] is a small timeline of its own: some layers and a length,
//! stored once in the document's library. A layer can hold an [`Instance`]
//! of a symbol, which draws the symbol's layers through the layer's own
//! transform. Change the symbol and every instance changes with it.
//!
//! Instances follow Adobe Animate's *graphic symbol* timing. The symbol's
//! frame is derived from the main timeline's frame rather than playing on
//! its own clock, so what you scrub to is exactly what gets exported.

use serde::{Deserialize, Serialize};

use crate::document::{Document, Layer, LayerId, TimelineId};
use crate::drawing::Drawing;
use crate::geometry::{Point, Rect};
use crate::transform::Transform;

/// How deeply symbols may nest. Validation rejects symbols that contain
/// themselves, so this is a backstop rather than a limit anyone should hit.
pub const MAX_SYMBOL_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SymbolId(pub(crate) u64);

/// A reusable animation in the document's library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    /// Length of the symbol's own timeline, in frames.
    pub duration: u32,
    /// Bottom-most layer first, like the document's.
    pub layers: Vec<Layer>,
    /// The Library folder it's filed in; empty for the top level.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

/// From `frame` on, an instance shows `symbol` instead (see
/// `Layer::symbol_swaps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolSwap {
    pub frame: u32,
    pub symbol: SymbolId,
}

impl Layer {
    /// The symbol an instance shows on `frame`: its own, or the last one
    /// swapped in by then.
    pub fn symbol_at(&self, frame: u32) -> Option<SymbolId> {
        let instance = self.instance?;
        let swapped = self.symbol_swaps.iter().take_while(|s| s.frame <= frame).last();
        Some(swapped.map_or(instance.symbol, |s| s.symbol))
    }

    /// Every symbol the layer uses: the ones an instance shows at some
    /// point, and the ones its strokes are brushed with (see
    /// `art_brush.rs`).
    pub fn symbols_used(&self) -> impl Iterator<Item = SymbolId> + '_ {
        let brushes = self
            .drawings
            .iter()
            .flat_map(|d| d.elements.iter().chain(&d.colour_art))
            .filter_map(|e| e.style.stroke.and_then(|s| s.brush).map(|b| b.symbol));
        self.instance
            .map(|i| i.symbol)
            .into_iter()
            .chain(self.symbol_swaps.iter().map(|s| s.symbol))
            .chain(brushes)
    }
}

/// A symbol placed on a layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub symbol: SymbolId,
    #[serde(default)]
    pub playback: Playback,
    /// The symbol frame shown on the layer's first frame.
    #[serde(default)]
    pub first_frame: u32,
}

/// Why layers couldn't become a symbol.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConvertError {
    #[error("choose the layers to make into a symbol")]
    NothingChosen,
    #[error("{0} is attached to a layer that isn't chosen: choose that layer too, or detach it first")]
    AttachedOutside(String),
}

/// How an instance moves through its symbol's frames as the main timeline plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Playback {
    /// Play the symbol and start again from the beginning when it ends.
    #[default]
    Loop,
    /// Play the symbol once, then hold its last frame.
    PlayOnce,
    /// Always show `first_frame`. Handy for symbols used as a set of poses,
    /// like mouth shapes for lip sync.
    SingleFrame,
}

impl Playback {
    pub const ALL: [Playback; 3] = [Playback::Loop, Playback::PlayOnce, Playback::SingleFrame];

    pub fn label(self) -> &'static str {
        match self {
            Playback::Loop => crate::i18n::tr_later("Loop"),
            Playback::PlayOnce => crate::i18n::tr_later("Play Once"),
            Playback::SingleFrame => crate::i18n::tr_later("Single Frame"),
        }
    }
}

impl Instance {
    /// Which symbol frame to show when the parent timeline is on `frame`.
    ///
    /// Time inside the symbol starts at `first_frame` on the layer's first
    /// frame (`layer_start`) and advances one frame per parent frame.
    pub fn local_frame(&self, layer_start: u32, frame: u32, symbol_duration: u32) -> u32 {
        let duration = symbol_duration.max(1);
        let elapsed = frame.saturating_sub(layer_start);
        match self.playback {
            Playback::Loop => (self.first_frame + elapsed) % duration,
            Playback::PlayOnce => (self.first_frame + elapsed).min(duration - 1),
            Playback::SingleFrame => self.first_frame.min(duration - 1),
        }
    }
}

impl Document {
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.id == id)
    }

    pub fn symbol_mut(&mut self, id: SymbolId) -> Option<&mut Symbol> {
        self.symbols.iter_mut().find(|s| s.id == id)
    }

    /// A name like "Symbol 2" that isn't already taken.
    pub fn next_symbol_name(&self) -> String {
        (1..)
            .map(|n| format!("Symbol {n}"))
            .find(|name| !self.symbols.iter().any(|s| &s.name == name))
            .expect("there is always an unused number")
    }

    /// How many layers, anywhere in the document, hold an instance of `id`.
    pub fn symbol_use_count(&self, id: SymbolId) -> usize {
        self.all_layers().filter(|l| l.symbols_used().any(|used| used == id)).count()
    }

    /// Removes a symbol from the library. Refuses (returns `false`) while
    /// any instance still uses it, rather than leaving instances that point
    /// at nothing.
    pub fn remove_symbol(&mut self, id: SymbolId) -> bool {
        if self.symbol_use_count(id) > 0 {
            return false;
        }
        let before = self.symbols.len();
        self.symbols.retain(|s| s.id != id);
        self.symbols.len() != before
    }

    /// Turns a layer into a symbol, replacing its contents with an instance
    /// of that symbol. The layer looks exactly the same afterwards.
    ///
    /// The symbol's registration point (its local origin, which instances
    /// rotate and scale around) is placed where the layer's pivot was on
    /// `frame`. To keep everything in place, the layer's animation moves
    /// into the symbol shifted by that offset, and the instance sits at the
    /// offset, so the two cancel out.
    ///
    /// This works for layers inside symbols too (`frame` is then a symbol
    /// frame); the new symbol is as long as the timeline the layer was in.
    pub fn convert_layer_to_symbol(&mut self, id: LayerId, frame: u32) -> Option<SymbolId> {
        let layer = self.layer(id)?;
        // The camera has nothing to put in a symbol.
        if layer.camera {
            return None;
        }
        // A folder goes in with everything in it (and its skeleton), and
        // still becomes the instance.
        if layer.folder {
            return self.convert_layers_to_symbol(&[id], frame).ok().map(|(symbol, _)| symbol);
        }
        let timeline = self.timeline_of(id)?;
        let duration = self.duration_of(timeline);
        let inner_id = self.allocate_layer_id();
        let symbol_id = SymbolId(self.next_symbol_id);
        self.next_symbol_id += 1;
        let name = self.next_symbol_name();

        let layer = self.layer_mut(id)?;
        let pivot = layer.transform_at(frame).position;

        let mut inner = layer.clone();
        inner.id = inner_id;
        inner.track.position.map_values(|p| p - pivot);

        // The original layer becomes the instance. Its first frame lines the
        // symbol's time up with the main timeline's, so keys stay put.
        // Its drawings, and the exposure swapping between them, went inside.
        layer.drawings = vec![Drawing::new("1", Vec::new())];
        layer.exposure.clear();
        layer.instance =
            Some(Instance { symbol: symbol_id, playback: Playback::Loop, first_frame: layer.start });
        layer.track = crate::track::Track::new(0, Transform::at(pivot));

        self.symbols.push(Symbol {
            id: symbol_id,
            name,
            duration,
            layers: vec![inner],
            folder: String::new(),
        });
        Some(symbol_id)
    }

    /// Turns several layers into one symbol, as Animate's Convert to Symbol
    /// does with a selection: the layers move inside it, each keeping its
    /// own animation, and an instance takes their place in the stack, where
    /// the topmost of them was. Everything looks exactly the same
    /// afterwards; double-click the instance to work on the layers inside.
    ///
    /// Layers attached to the chosen ones (an arm's hand) come along too, as
    /// do a chosen folder's layers; a skeleton folder brings its bones.
    /// One layer chosen becomes the instance itself, keeping its id.
    /// Rig parents, masks and motion guides among them still connect inside
    /// the symbol. A chosen layer attached to one that isn't chosen can't
    /// be separated from it, so that's refused. The camera and folders are
    /// left out (a folder's layers can be chosen instead).
    ///
    /// The layers are drawn together inside the instance, so chosen layers
    /// that weren't next to each other in the stack no longer have the
    /// layers between them in between (Animate does the same).
    ///
    /// The symbol's registration point is the middle of what the layers
    /// show on `frame`, and the symbol keeps the timeline's timing (its
    /// length is the timeline's, and the instance starts where the earliest
    /// layer did), so nothing moves in space or time.
    pub fn convert_layers_to_symbol(
        &mut self,
        ids: &[LayerId],
        frame: u32,
    ) -> Result<(SymbolId, LayerId), ConvertError> {
        let first = *ids.first().ok_or(ConvertError::NothingChosen)?;
        let timeline = self.timeline_of(first).ok_or(ConvertError::NothingChosen)?;
        let layers = self.layers_of(timeline);
        let chosen = |l: &Layer| ids.contains(&l.id) && !l.camera;
        // The chosen layers and everything attached to them, in stacking order.
        let mut taken: Vec<LayerId> = layers.iter().filter(|l| chosen(l)).map(|l| l.id).collect();
        loop {
            let more: Vec<LayerId> = layers
                .iter()
                .filter(|l| {
                    let attached = |to: Option<LayerId>| to.is_some_and(|p| taken.contains(&p));
                    !taken.contains(&l.id) && (attached(l.parent) || attached(l.in_folder))
                })
                .map(|l| l.id)
                .collect();
            if more.is_empty() {
                break;
            }
            taken.extend(more);
        }
        if taken.is_empty() {
            return Err(ConvertError::NothingChosen);
        }
        if let Some(stray) = layers
            .iter()
            .filter(|l| taken.contains(&l.id))
            .find(|l| l.parent.is_some_and(|p| !taken.contains(&p) && self.layer(p).is_some()))
        {
            return Err(ConvertError::AttachedOutside(stray.name.clone()));
        }
        let taken: Vec<LayerId> = layers.iter().filter(|l| taken.contains(&l.id)).map(|l| l.id).collect();

        // The middle of what they show now, as the registration point.
        let corners: Vec<Point> = taken
            .iter()
            .filter_map(|id| self.layer(*id))
            .filter_map(|layer| {
                let bounds = self.layer_bounds(layer, frame)?;
                let placed = self.placement_without_camera(layer, frame as f32);
                Some(bounds.corners().map(|c| placed.apply(c)))
            })
            .flatten()
            .collect();
        let middle = Rect::bounding(corners).map_or_else(|| self.stage_rect().center(), |r| r.center());
        let start = taken.iter().filter_map(|id| self.layer(*id)).map(|l| l.start).min().unwrap_or(0);
        let duration = self.duration_of(timeline);

        // New ids inside the symbol, and the links between them kept.
        let mapping: Vec<(LayerId, LayerId)> =
            taken.iter().map(|&old| (old, self.allocate_layer_id())).collect();
        let renamed =
            |id: Option<LayerId>| id.and_then(|id| mapping.iter().find(|(o, _)| *o == id).map(|(_, n)| *n));
        let layers = self.layers_of_mut(timeline).ok_or(ConvertError::NothingChosen)?;
        let top = layers.iter().rposition(|l| taken.contains(&l.id)).unwrap_or(0);
        let mut inner = Vec::new();
        let mut kept = Vec::new();
        for (index, layer) in std::mem::take(layers).into_iter().enumerate() {
            if !taken.contains(&layer.id) {
                kept.push((index, layer));
                continue;
            }
            let mut layer = layer;
            layer.id = renamed(Some(layer.id)).expect("every taken layer has a new id");
            if layer.parent.is_some() {
                layer.parent = renamed(layer.parent);
            } else {
                // Placed around the registration point; attached layers
                // follow their parents and stay as they are.
                layer.track.position.map_values(|p| p - middle);
                // Bones are in the timeline's coordinates, and their moves
                // are from rest, so moving the rest moves the whole rig.
                if let Some(skeleton) = &mut layer.skeleton {
                    skeleton.translate(Point::ZERO - middle);
                }
            }
            layer.masked_by = renamed(layer.masked_by);
            layer.follow = renamed(layer.follow);
            layer.in_folder = renamed(layer.in_folder);
            inner.push(layer);
        }

        let symbol_id = SymbolId(self.next_symbol_id);
        self.next_symbol_id += 1;
        let name = self.next_symbol_name();
        // The taken layers all have new ids, so a single one's is free for
        // the instance: a script holding the layer then holds the instance.
        let instance_id = if let [one] = ids { *one } else { self.allocate_layer_id() };
        // It keeps that layer's name too, as a plain layer made a symbol does.
        let instance_name = match ids {
            [one] => inner.iter().find(|l| renamed(Some(*one)) == Some(l.id)).map(|l| l.name.clone()),
            _ => None,
        };
        let mut instance = Layer {
            id: instance_id,
            name: instance_name.unwrap_or_else(|| name.clone()),
            drawings: vec![Drawing::new("1", Vec::new())],
            exposure: Vec::new(),
            ..inner[0].clone()
        };
        // A plain layer apart from being the instance: no rig, mask, guide
        // or effects of the layers that went inside.
        instance.instance =
            Some(Instance { symbol: symbol_id, playback: Playback::Loop, first_frame: start });
        instance.start = start;
        instance.end = None;
        instance.track = crate::track::Track::new(start, Transform::at(middle));
        (instance.parent, instance.masked_by, instance.follow, instance.in_folder) = (None, None, None, None);
        (instance.mask, instance.guide, instance.particles, instance.warp, instance.deformer) =
            (false, false, None, None, None);
        (instance.folder, instance.skeleton, instance.switch, instance.bone_binding) =
            (false, None, false, crate::BoneBinding::default());
        instance.switch_keys.clear();
        instance.shape_tweens.clear();
        instance.pivot = Point::ZERO;
        instance.blend = crate::BlendMode::Normal;

        // The instance goes where the topmost of them was.
        let layers = self.layers_of_mut(timeline).ok_or(ConvertError::NothingChosen)?;
        let at = kept.iter().position(|(index, _)| *index > top).unwrap_or(kept.len());
        layers.extend(kept.into_iter().map(|(_, layer)| layer));
        layers.insert(at, instance);
        self.symbols.push(Symbol { id: symbol_id, name, duration, layers: inner, folder: String::new() });
        if timeline == TimelineId::Main {
            self.keep_camera_on_top();
        }
        Ok((symbol_id, instance_id))
    }

    /// Whether an instance of `symbol` may be placed in `timeline`: not if
    /// that would put a symbol inside itself, directly or through others.
    pub fn can_place(&self, timeline: TimelineId, symbol: SymbolId) -> bool {
        match timeline {
            TimelineId::Main => true,
            TimelineId::Symbol(host) => !self.symbol_contains(symbol, host),
        }
    }

    /// Adds a layer to `timeline` holding an instance of `symbol`, starting
    /// at `frame` with the symbol's registration point at `position`.
    /// Returns `None` if the placement isn't allowed (see [`Document::can_place`]).
    pub fn add_instance_layer(
        &mut self,
        timeline: TimelineId,
        symbol: SymbolId,
        frame: u32,
        position: Point,
    ) -> Option<LayerId> {
        if !self.can_place(timeline, symbol) {
            return None;
        }
        let name = self.symbol(symbol)?.name.clone();
        let id = self.add_layer_to(timeline, name, Vec::new(), frame, Transform::at(position))?;
        let layer = self.layer_mut(id)?;
        layer.start = frame;
        layer.instance = Some(Instance { symbol, playback: Playback::Loop, first_frame: 0 });
        Some(id)
    }

    /// Whether `symbol`'s contents include an instance of `target`, directly
    /// or through other symbols. Placing `target` inside `symbol` would then
    /// make a symbol that contains itself.
    pub fn symbol_contains(&self, symbol: SymbolId, target: SymbolId) -> bool {
        let mut stack = vec![symbol];
        let mut seen = Vec::new();
        while let Some(id) = stack.pop() {
            if id == target {
                return true;
            }
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            if let Some(s) = self.symbol(id) {
                stack.extend(s.layers.iter().flat_map(Layer::symbols_used));
            }
        }
        false
    }

    /// Bounds of a layer's contents at `frame`, in the layer's local
    /// coordinates. Instances include their symbol's layers at the matching
    /// symbol frame, each transformed into place.
    pub fn layer_bounds(&self, layer: &Layer, frame: u32) -> Option<Rect> {
        self.layer_bounds_at_depth(layer, frame, 0)
    }

    fn layer_bounds_at_depth(&self, layer: &Layer, frame: u32, depth: usize) -> Option<Rect> {
        // A particle layer's drawing is the particles' look, not drawn as
        // itself; what it covers is where the particles are.
        if layer.particles.is_some() {
            return self.particle_bounds(layer, frame as f32);
        }
        let shapes = self.element_bounds(&self.posed_elements(layer, frame));
        let nested = self.instance_view(layer, frame, depth).and_then(|(symbol, local_frame)| {
            symbol
                .layers
                .iter()
                .filter(|inner| inner.is_shown_at(local_frame))
                .filter_map(|inner| {
                    let bounds = self.layer_bounds_at_depth(inner, local_frame, depth + 1)?;
                    let to_parent = self.layer_matrix(inner, local_frame);
                    Rect::bounding(bounds.corners().map(|c| to_parent.apply(c)))
                })
                .reduce(Rect::union)
        });
        match (shapes, nested) {
            (Some(a), Some(b)) => Some(a.union(b)),
            (a, b) => a.or(b),
        }
    }

    /// Whether `point` (in the coordinates of the layer's parent) hits the
    /// layer's contents at `frame`.
    ///
    /// The point is mapped into the layer's local space with the inverse
    /// transform, so each shape can do its hit test in its own simple,
    /// unrotated coordinates. For instances the same happens again, one
    /// level down, for each layer inside the symbol.
    pub fn layer_hit(&self, layer: &Layer, point: Point, frame: u32, tolerance: f32) -> bool {
        self.layer_hit_at_depth(layer, point, frame, tolerance, 0)
    }

    fn layer_hit_at_depth(
        &self,
        layer: &Layer,
        point: Point,
        frame: u32,
        tolerance: f32,
        depth: usize,
    ) -> bool {
        let matrix = self.layer_matrix(layer, frame);
        let Some(to_local) = matrix.inverse() else {
            return false; // Scaled to nothing: there's nothing to click on.
        };
        let local = to_local.apply(point);
        // Tolerance is given in parent units; convert it to local units so a
        // layer scaled up 4× (by itself or its parents) doesn't get a 4×
        // bigger click target. The matrix's area scale gives the average.
        let scale = matrix.average_scale().max(1e-6);
        let local_tolerance = tolerance / scale;

        self.posed_elements(layer, frame).iter().any(|e| self.element_hit(e, local, local_tolerance))
            || self.instance_view(layer, frame, depth).is_some_and(|(symbol, local_frame)| {
                symbol.layers.iter().filter(|inner| inner.is_shown_at(local_frame)).any(|inner| {
                    self.layer_hit_at_depth(inner, local, local_frame, local_tolerance, depth + 1)
                })
            })
    }

    /// Has instance layer `id` show `symbol` instead: on every frame
    /// (`from` is `None`, or at or before the layer's start), or from frame
    /// `from` on, until any later swap. Its animation, timing and playback
    /// stay as they were. Refused (false) for a layer that isn't an
    /// instance, or a swap that would put a symbol inside itself.
    pub fn swap_symbol(&mut self, id: LayerId, symbol: SymbolId, from: Option<u32>) -> bool {
        let Some(timeline) = self.timeline_of(id) else { return false };
        if self.symbol(symbol).is_none() || !self.can_place(timeline, symbol) {
            return false;
        }
        let Some(layer) = self.layer_mut(id).filter(|l| l.instance.is_some()) else { return false };
        match from.filter(|&f| f > layer.start) {
            None => {
                if let Some(instance) = &mut layer.instance {
                    instance.symbol = symbol;
                }
                layer.symbol_swaps.clear();
            }
            Some(frame) => {
                layer.symbol_swaps.retain(|s| s.frame != frame);
                layer.symbol_swaps.push(SymbolSwap { frame, symbol });
                layer.symbol_swaps.sort_by_key(|s| s.frame);
                // A swap to what's already showing does nothing.
                let mut previous = layer.instance.map(|i| i.symbol);
                layer.symbol_swaps.retain(|s| {
                    let changes = Some(s.symbol) != previous;
                    previous = Some(s.symbol);
                    changes
                });
            }
        }
        true
    }

    /// A copy of a symbol, named "… copy", for changing without changing
    /// everything that uses the original (Animate's Duplicate Symbol). Its
    /// layers get new ids, with their rig, masks and guides linked among
    /// themselves.
    pub fn duplicate_symbol(&mut self, id: SymbolId) -> Option<SymbolId> {
        let original = self.symbol(id)?.clone();
        let new_id = SymbolId(self.next_symbol_id);
        self.next_symbol_id += 1;
        let ids: std::collections::HashMap<LayerId, LayerId> =
            original.layers.iter().map(|l| (l.id, self.allocate_layer_id())).collect();
        let relink = |old: Option<LayerId>| old.map(|o| ids.get(&o).copied().unwrap_or(o));
        let layers = original
            .layers
            .iter()
            .map(|layer| {
                let mut copy = layer.clone();
                copy.id = ids[&layer.id];
                copy.parent = relink(layer.parent);
                copy.masked_by = relink(layer.masked_by);
                copy.follow = relink(layer.follow);
                copy.in_folder = relink(layer.in_folder);
                copy
            })
            .collect();
        let taken: Vec<String> = self.symbols.iter().map(|s| s.name.clone()).collect();
        let name = (1..)
            .map(|n| {
                if n == 1 { format!("{} copy", original.name) } else { format!("{} copy {n}", original.name) }
            })
            .find(|name| !taken.contains(name))
            .expect("there is always an unused name");
        self.symbols.push(Symbol { id: new_id, name, layers, ..original });
        Some(new_id)
    }

    /// The symbol an instance layer shows, and which of its frames, if the
    /// layer is an instance and we're not nested impossibly deep.
    pub fn instance_view(&self, layer: &Layer, frame: u32, depth: usize) -> Option<(&Symbol, u32)> {
        if depth >= MAX_SYMBOL_DEPTH {
            return None;
        }
        let instance = layer.instance?;
        let symbol = self.symbol(layer.symbol_at(frame)?)?;
        Some((symbol, instance.local_frame(layer.start, frame, symbol.duration)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::document::Element;
    use crate::shape::{Shape, Style};

    fn square() -> Element {
        Element {
            shape: Shape::Rect { width: 100.0, height: 100.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        }
    }

    #[test]
    fn loop_wraps_and_play_once_holds() {
        let mut instance = Instance { symbol: SymbolId(1), playback: Playback::Loop, first_frame: 2 };
        assert_eq!(instance.local_frame(10, 10, 5), 2);
        assert_eq!(instance.local_frame(10, 13, 5), 0); // 2 + 3 = 5 wraps to 0.
        instance.playback = Playback::PlayOnce;
        assert_eq!(instance.local_frame(10, 30, 5), 4);
        instance.playback = Playback::SingleFrame;
        assert_eq!(instance.local_frame(10, 30, 5), 2);
    }

    #[test]
    fn converting_keeps_the_layer_where_it_was() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::at(Point::new(300.0, 200.0)));
        doc.layer_mut(id).unwrap().track.set_pose(20, Transform::at(Point::new(700.0, 200.0)));

        let before: Vec<Rect> = (0..30).map(|f| stage_bounds(&doc, id, f)).collect();
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        let after: Vec<Rect> = (0..30).map(|f| stage_bounds(&doc, id, f)).collect();

        assert_eq!(doc.symbol_use_count(symbol), 1);
        assert!(doc.layer(id).unwrap().all_elements().next().is_none());
        for (a, b) in before.iter().zip(&after) {
            assert!(a.min.distance(b.min) < 1e-3 && a.max.distance(b.max) < 1e-3, "{a:?} vs {b:?}");
        }
        // Clicking still finds the layer, through its instance.
        assert_eq!(doc.hit_test(Point::new(700.0, 200.0), 20, 0.0), Some(id));
    }

    fn stage_bounds(doc: &Document, id: LayerId, frame: u32) -> Rect {
        let layer = doc.layer(id).unwrap();
        let local = doc.layer_bounds(layer, frame).unwrap();
        let to_stage = layer.transform_at(frame).to_affine();
        Rect::bounding(local.corners().map(|c| to_stage.apply(c))).unwrap()
    }

    #[test]
    fn instances_share_their_symbol() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::at(Point::new(100.0, 100.0)));
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        let copy = doc.add_instance_layer(TimelineId::Main, symbol, 5, Point::new(500.0, 100.0)).unwrap();

        assert_eq!(doc.symbol_use_count(symbol), 2);
        assert_eq!(doc.layer(copy).unwrap().start, 5);
        assert_eq!(doc.hit_test(Point::new(500.0, 100.0), 5, 0.0), Some(copy));

        // Making the symbol's square bigger affects both instances.
        let inner = &mut doc.symbol_mut(symbol).unwrap().layers[0];
        inner.drawings[0].elements[0].shape = Shape::Rect { width: 300.0, height: 100.0, corner_radius: 0.0 };
        assert_eq!(doc.hit_test(Point::new(620.0, 100.0), 5, 0.0), Some(copy));
        assert_eq!(doc.hit_test(Point::new(220.0, 100.0), 5, 0.0), Some(id));
    }

    #[test]
    fn symbols_in_use_cannot_be_removed() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        assert!(!doc.remove_symbol(symbol));
        doc.remove_layer(id);
        assert!(doc.remove_symbol(symbol));
    }

    #[test]
    fn files_with_self_containing_symbols_are_rejected() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        // Hand-edit the symbol so it holds an instance of itself.
        doc.symbol_mut(symbol).unwrap().layers[0].instance =
            Some(Instance { symbol, playback: Playback::Loop, first_frame: 0 });

        let json = doc.to_json().unwrap();
        assert!(matches!(Document::from_json(&json), Err(crate::DocumentError::Invalid(_))));
    }

    #[test]
    fn symbols_cannot_be_placed_inside_themselves() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        let inner = doc.convert_layer_to_symbol(id, 0).unwrap();
        let outer = doc.convert_layer_to_symbol(id, 0).unwrap();

        // Directly, and through the symbol nested inside it.
        assert!(doc.add_instance_layer(TimelineId::Symbol(outer), outer, 0, Point::ZERO).is_none());
        assert!(doc.add_instance_layer(TimelineId::Symbol(inner), outer, 0, Point::ZERO).is_none());
        // The other way round is fine.
        assert!(doc.add_instance_layer(TimelineId::Symbol(outer), inner, 0, Point::ZERO).is_some());
    }

    #[test]
    fn layers_inside_symbols_can_be_edited_and_converted() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(id, 0).unwrap();
        let inner = doc.symbol(symbol).unwrap().layers[0].id;

        assert_eq!(doc.timeline_of(inner), Some(TimelineId::Symbol(symbol)));
        let copy = doc.duplicate_layer(inner).unwrap();
        assert_eq!(doc.layers_of(TimelineId::Symbol(symbol)).len(), 2);
        // Converting a layer that lives in a symbol nests a new symbol there.
        let nested = doc.convert_layer_to_symbol(copy, 0).unwrap();
        assert!(doc.symbol_contains(symbol, nested));
        assert!(doc.remove_layer(copy).is_some());
        assert_eq!(doc.layers_of(TimelineId::Symbol(symbol)).len(), 1);
    }

    #[test]
    fn nested_containment_is_detected() {
        let mut doc = Document::default();
        let id = doc.add_layer("Box", vec![square()], 0, Transform::default());
        let inner = doc.convert_layer_to_symbol(id, 0).unwrap();
        let outer = doc.convert_layer_to_symbol(id, 0).unwrap();
        assert!(doc.symbol_contains(outer, inner));
        assert!(!doc.symbol_contains(inner, outer));
    }

    #[test]
    fn converting_several_layers_keeps_their_links_and_refuses_stray_parents() {
        let mut doc = Document::default();
        let body = doc.add_layer("Body", vec![], 0, Transform::default());
        let head = doc.add_layer("Head", vec![], 0, Transform::default());
        let mask = doc.add_layer("Mask", vec![], 0, Transform::default());
        doc.layer_mut(head).unwrap().parent = Some(body);
        doc.layer_mut(head).unwrap().masked_by = Some(mask);
        doc.layer_mut(mask).unwrap().mask = true;
        let other = doc.add_layer("Other", vec![], 0, Transform::default());

        // A layer attached to one left out can't go alone.
        assert_eq!(
            doc.convert_layers_to_symbol(&[head], 0).unwrap_err(),
            ConvertError::AttachedOutside("Head".into())
        );
        let (symbol, _) = doc.convert_layers_to_symbol(&[body, mask], 0).unwrap();
        let inside = &doc.symbol(symbol).unwrap().layers;
        let id_of = |name: &str| inside.iter().find(|l| l.name == name).unwrap().id;
        let head_inside = inside.iter().find(|l| l.name == "Head").unwrap();
        assert_eq!(head_inside.parent, Some(id_of("Body")), "the rig still connects");
        assert_eq!(head_inside.masked_by, Some(id_of("Mask")), "so does the mask");
        assert!(doc.layer(other).is_some(), "the rest stay");
    }

    /// Two symbols, A and B, and a layer showing A.
    fn two_symbols() -> (Document, LayerId, SymbolId, SymbolId) {
        let mut doc = Document::default();
        let a = doc.add_layer("A", vec![square()], 0, Transform::default());
        let a = doc.convert_layer_to_symbol(a, 0).unwrap();
        let b = doc.add_layer("B", vec![square()], 0, Transform::default());
        let b = doc.convert_layer_to_symbol(b, 0).unwrap();
        let layer = doc.add_instance_layer(TimelineId::Main, a, 0, Point::ZERO).unwrap();
        (doc, layer, a, b)
    }

    #[test]
    fn a_symbol_swapped_in_from_a_frame_shows_from_then_on() {
        let (mut doc, layer, a, b) = two_symbols();
        assert!(doc.swap_symbol(layer, b, Some(10)));
        let shown = doc.layer(layer).unwrap();
        assert_eq!(shown.symbol_at(9), Some(a));
        assert_eq!(shown.symbol_at(10), Some(b));
        assert_eq!(doc.instance_view(shown, 30, 0).unwrap().0.id, b);
        // Both are in use now, so neither can be deleted.
        assert_eq!(doc.symbol_use_count(b), 2, "the converted B layer and the swap");
        assert!(!doc.remove_symbol(a));

        // Swapping for the whole layer replaces it everywhere.
        assert!(doc.swap_symbol(layer, b, None));
        let shown = doc.layer(layer).unwrap();
        assert_eq!(shown.symbol_at(0), Some(b));
        assert!(shown.symbol_swaps.is_empty());
    }

    #[test]
    fn swaps_move_with_the_frame_commands() {
        let (mut doc, layer, _, b) = two_symbols();
        doc.swap_symbol(layer, b, Some(10));
        doc.layer_mut(layer).unwrap().insert_frames(5, 3);
        assert_eq!(doc.layer(layer).unwrap().symbol_swaps[0].frame, 13);
    }

    #[test]
    fn a_duplicate_symbol_is_separate_with_its_rig_relinked() {
        let mut doc = Document::default();
        let arm = doc.add_layer("Arm", vec![square()], 0, Transform::default());
        let hand = doc.add_layer("Hand", vec![square()], 0, Transform::default());
        doc.layer_mut(hand).unwrap().parent = Some(arm);
        let symbol = doc.convert_layers_to_symbol(&[arm, hand], 0).unwrap().0;
        let copy = doc.duplicate_symbol(symbol).unwrap();

        let (original, duplicate) = (doc.symbol(symbol).unwrap(), doc.symbol(copy).unwrap());
        assert_eq!(duplicate.name, format!("{} copy", original.name));
        let ids: Vec<LayerId> = duplicate.layers.iter().map(|l| l.id).collect();
        assert!(ids.iter().all(|id| original.layers.iter().all(|l| l.id != *id)), "new ids");
        let copied_hand = duplicate.layers.iter().find(|l| l.name == "Hand").unwrap();
        assert!(ids.contains(&copied_hand.parent.unwrap()), "the hand hangs off the copied arm");

        doc.symbol_mut(copy).unwrap().layers[0].name = "Changed".into();
        assert!(doc.symbol(symbol).unwrap().layers.iter().all(|l| l.name != "Changed"));
    }

    #[test]
    fn a_symbol_cant_be_swapped_into_itself() {
        let (mut doc, _, a, b) = two_symbols();
        // An instance of A inside B: swapping it to B would put B in B.
        let inside = doc.add_instance_layer(TimelineId::Symbol(b), a, 0, Point::ZERO).unwrap();
        assert!(!doc.swap_symbol(inside, b, None));
    }
}
