//! Editing several selected layers at once from the properties panel.
//!
//! The panel shows the layer picked last. Whatever is changed there is then
//! carried to the other selected layers, the way Animate and Figma do it:
//! position moves them all by the same amount (so a group keeps its shape),
//! and everything else (rotation, scale, skew, opacity, depth, blend, the
//! checkboxes, where the layer starts and ends) is set to the same value on
//! each. Only what was actually changed is carried over, so setting the
//! opacity of three layers leaves their different rotations alone.

use inkframe_core::{BlendMode, Layer, LayerId, Property, Transform};

use crate::editor::Editor;

/// Close enough to call two values the same, after a round trip through a
/// text field.
const SAME: f32 = 1e-5;

/// The layer settings above the transform, as they were before the panel
/// was drawn, to see what changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerSettings {
    pub visible: bool,
    pub locked: bool,
    pub guide: bool,
    pub mask: bool,
    pub blend: BlendMode,
    pub clipped: bool,
    pub start: u32,
    pub end: Option<u32>,
}

impl LayerSettings {
    pub fn of(layer: &Layer) -> Self {
        Self {
            visible: layer.visible,
            locked: layer.locked,
            guide: layer.guide,
            mask: layer.mask,
            blend: layer.blend,
            clipped: layer.clipped,
            start: layer.start,
            end: layer.end,
        }
    }
}

impl Editor {
    /// The selected layers besides `primary` that the panel's changes are
    /// carried to: not folders, which have no settings of their own.
    pub fn also_editing(&self, primary: LayerId) -> Vec<LayerId> {
        self.selected_ids()
            .into_iter()
            .filter(|&id| id != primary && self.doc.layer(id).is_some_and(|l| !l.folder))
            .collect()
    }

    /// Carries a change to `primary`'s pose on this frame, from `before` to
    /// `after`, to the other selected layers.
    pub fn carry_pose_change(&mut self, primary: LayerId, before: Transform, after: Transform) {
        let frame = self.frame;
        let moved = after.position - before.position;
        let changed = |a: f32, b: f32| (a - b).abs() > SAME;
        for id in self.also_editing(primary) {
            let Some(layer) = self.doc.layer(id) else { continue };
            let mut pose = self.doc.pose(layer, frame);
            pose.position = pose.position + moved;
            if changed(after.rotation, before.rotation) {
                pose.rotation = after.rotation;
            }
            if changed(after.scale.x, before.scale.x) {
                pose.scale.x = after.scale.x;
            }
            if changed(after.scale.y, before.scale.y) {
                pose.scale.y = after.scale.y;
            }
            if changed(after.skew.x, before.skew.x) {
                pose.skew.x = after.skew.x;
            }
            if changed(after.skew.y, before.skew.y) {
                pose.skew.y = after.skew.y;
            }
            if changed(after.opacity, before.opacity) {
                pose.opacity = after.opacity;
            }
            self.set_pose(id, pose);
        }
    }

    /// Sets the other selected layers to `primary`'s depth.
    pub fn carry_depth(&mut self, primary: LayerId, depth: f32) {
        for id in self.also_editing(primary) {
            self.set_depth(id, depth);
        }
    }

    /// Gives the other selected layers the same key (or no key) for
    /// `property` on this frame as `primary` now has: clicking a diamond
    /// keys or unkeys them all.
    pub fn carry_key(&mut self, primary: LayerId, property: Property) {
        let frame = self.frame;
        let Some(keyed) = self.doc.layer(primary).map(|l| l.track.channel(property).has_key(frame)) else {
            return;
        };
        for id in self.also_editing(primary) {
            let has = self.doc.layer(id).is_some_and(|l| l.track.channel(property).has_key(frame));
            if has != keyed {
                self.toggle_property_key(id, property);
            }
        }
    }

    /// Carries whichever of `primary`'s settings changed from `before` to
    /// the other selected layers.
    pub fn carry_settings(&mut self, primary: LayerId, before: LayerSettings) {
        let Some(after) = self.doc.layer(primary).map(LayerSettings::of) else { return };
        if after == before {
            return;
        }
        for id in self.also_editing(primary) {
            let Some(layer) = self.doc.layer_mut(id) else { continue };
            if after.visible != before.visible {
                layer.visible = after.visible;
            }
            if after.locked != before.locked {
                layer.locked = after.locked;
            }
            if after.guide != before.guide {
                layer.guide = after.guide;
            }
            if after.mask != before.mask {
                layer.mask = after.mask;
            }
            if after.blend != before.blend {
                layer.blend = after.blend;
            }
            if after.clipped != before.clipped && inkframe_core::Document::can_clip(layer) {
                layer.clipped = after.clipped;
            }
            if after.start != before.start {
                layer.set_start(after.start);
            }
            if after.end != before.end {
                layer.set_end(after.end);
            }
        }
    }

    /// Gives every other selected layer `primary`'s effects (colour effect
    /// and filters) on this frame, keyed there, replacing their own. Done
    /// from a button, not on every change, since a layer's filters are a
    /// list it may have built up itself. Returns how many layers changed.
    pub fn copy_effects_to_selected(&mut self, primary: LayerId) -> usize {
        let frame = self.frame;
        let Some(fx) = self.doc.layer(primary).map(|l| l.track.effects_at(frame as f32)) else { return 0 };
        let mut changed = 0;
        for id in self.also_editing(primary) {
            if let Some(layer) = self.doc.layer_mut(id) {
                layer.track.set_effects(frame, fx.clone());
                changed += 1;
            }
        }
        changed
    }

    /// Gives the other selected layers' tweens through this frame the same
    /// easing as `primary`'s, property by property. A layer with no tween
    /// of that property here is left alone. Returns how many tweens changed.
    pub fn copy_easing_to_selected(&mut self, primary: LayerId) -> usize {
        let frame = self.frame;
        let Some(layer) = self.doc.layer(primary) else { return 0 };
        let easings: Vec<(Property, inkframe_core::Easing)> = Property::ALL
            .into_iter()
            .filter_map(|p| layer.track.channel(p).tween_at(frame).map(|t| (p, t.easing)))
            .collect();
        let mut changed = 0;
        for id in self.also_editing(primary) {
            let Some(layer) = self.doc.layer_mut(id) else { continue };
            for &(property, easing) in &easings {
                if let Some(e) = layer.track.channel_mut(property).tween_easing_mut(frame) {
                    *e = easing;
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Whether some property of the selected layers' poses differs from
    /// `primary`'s, for marking it in the panel.
    pub fn pose_differs(&self, primary: LayerId, part: impl Fn(&Transform) -> [f32; 2]) -> bool {
        let frame = self.frame;
        let Some(mine) = self.doc.layer(primary).map(|l| part(&self.doc.pose(l, frame))) else {
            return false;
        };
        self.also_editing(primary).into_iter().filter_map(|id| self.doc.layer(id)).any(|layer| {
            let theirs = part(&self.doc.pose(layer, frame));
            (theirs[0] - mine[0]).abs() > SAME || (theirs[1] - mine[1]).abs() > SAME
        })
    }
}

#[cfg(test)]
mod tests {
    use inkframe_core::{Document, Point};

    use super::*;

    /// Three layers at different places and turns, all selected, C last.
    fn three_selected() -> (Editor, [LayerId; 3]) {
        let mut editor = Editor::new(Document::default());
        let mut at = |name: &str, x: f32, rotation: f32| {
            let mut pose = Transform::at(Point::new(x, 100.0));
            pose.rotation = rotation;
            editor.doc.add_layer(name, Vec::new(), 0, pose)
        };
        let ids = [at("A", 100.0, 0.0), at("B", 200.0, 30.0), at("C", 300.0, 60.0)];
        editor.select_many(&ids);
        (editor, ids)
    }

    fn pose(editor: &Editor, id: LayerId) -> Transform {
        editor.doc.pose(editor.doc.layer(id).unwrap(), editor.frame)
    }

    #[test]
    fn moving_one_moves_them_all_by_as_much() {
        let (mut editor, [a, b, c]) = three_selected();
        let before = pose(&editor, c);
        let after = Transform { position: before.position + Point::new(10.0, -5.0), ..before };
        editor.set_pose(c, after);
        editor.carry_pose_change(c, before, after);
        assert_eq!(pose(&editor, a).position, Point::new(110.0, 95.0));
        assert_eq!(pose(&editor, b).position, Point::new(210.0, 95.0));
        assert_eq!(pose(&editor, b).rotation, 30.0, "rotation wasn't touched, so stays its own");
    }

    #[test]
    fn other_values_are_set_on_each_but_only_those_changed() {
        let (mut editor, [a, b, c]) = three_selected();
        let before = pose(&editor, c);
        let after = Transform { opacity: 0.5, ..before };
        editor.set_pose(c, after);
        editor.carry_pose_change(c, before, after);
        for id in [a, b] {
            assert_eq!(pose(&editor, id).opacity, 0.5);
        }
        assert_eq!(pose(&editor, a).rotation, 0.0);
        assert_eq!(pose(&editor, b).rotation, 30.0);

        let before = pose(&editor, c);
        let after = Transform { rotation: 45.0, ..before };
        editor.carry_pose_change(c, before, after);
        assert_eq!(pose(&editor, a).rotation, 45.0);
        assert_eq!(pose(&editor, a).position, Point::new(100.0, 100.0), "not moved");
    }

    #[test]
    fn checkboxes_blend_and_keys_reach_every_selected_layer() {
        let (mut editor, [a, b, c]) = three_selected();
        let before = LayerSettings::of(editor.doc.layer(c).unwrap());
        let layer = editor.doc.layer_mut(c).unwrap();
        layer.visible = false;
        layer.blend = BlendMode::Multiply;
        editor.carry_settings(c, before);
        for id in [a, b] {
            let layer = editor.doc.layer(id).unwrap();
            assert!(!layer.visible);
            assert_eq!(layer.blend, BlendMode::Multiply);
            assert!(!layer.locked, "untouched");
        }

        editor.set_frame(10);
        editor.toggle_property_key(c, Property::Scale);
        editor.carry_key(c, Property::Scale);
        for id in [a, b] {
            assert!(editor.doc.layer(id).unwrap().track.scale.key_at(10).is_some());
        }
    }

    #[test]
    fn differing_values_are_noticed() {
        let (editor, [_, _, c]) = three_selected();
        assert!(editor.pose_differs(c, |p| [p.rotation, 0.0]));
        assert!(!editor.pose_differs(c, |p| [p.opacity, 0.0]));
    }

    #[test]
    fn effects_and_easing_are_copied_on_request() {
        use inkframe_core::{AnyChannel, ColorEffect, Easing};
        let (mut editor, [a, b, c]) = three_selected();
        // C is faded and tinted; A and B tween their position from frame 0
        // to 20, as does C.
        let mut fx = editor.doc.layer(c).unwrap().track.effects_at(0.0);
        fx.color = ColorEffect::Brightness { amount: 0.3 };
        editor.doc.layer_mut(c).unwrap().track.set_effects(0, fx.clone());
        for id in [a, b, c] {
            editor.doc.layer_mut(id).unwrap().track.position.set(20, Point::new(0.0, 0.0));
        }
        *editor.doc.layer_mut(c).unwrap().track.position.tween_easing_mut(10).unwrap() = Easing::EaseIn;

        assert_eq!(editor.copy_effects_to_selected(c), 2);
        for id in [a, b] {
            assert_eq!(editor.doc.layer(id).unwrap().track.effects_at(0.0), fx);
        }
        editor.set_frame(10);
        assert_eq!(editor.copy_easing_to_selected(c), 2);
        for id in [a, b] {
            let track = &editor.doc.layer(id).unwrap().track;
            assert_eq!(track.position.tween_at(10).unwrap().easing, Easing::EaseIn);
        }
    }
}
