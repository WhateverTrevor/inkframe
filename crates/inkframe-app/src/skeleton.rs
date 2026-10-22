//! Rigging a character with a skeleton, as in Moho (the bones and how
//! drawings bend with them are in the core's `skeleton.rs`; dragging them
//! on the stage in `stage/bones.rs`).
//!
//! Modify › New Skeleton puts the selected layers in a folder with an empty
//! skeleton and picks the Add Bone tool. Drag on the stage to draw a bone;
//! the next one drawn hangs from it, so an arm is three drags from the
//! shoulder out. Then the Bone tool poses them: drag a bone and the ones
//! above it turn to follow, or drag a root's start to move the whole
//! character. Every layer in the folder bends with the bones near it,
//! painted ones as well as drawn ones, unless it's bound to one bone or
//! none in the Properties panel.
//!
//! Finer than that, shapes picked inside a layer, or a single point picked
//! with the Subselection tool, can be pinned to a bone of their own. And a
//! bone can be made smart: as it turns, it reshapes a layer toward another
//! of its drawings (a corrected elbow, a smile), or has a switch folder
//! show another of its layers (a fist once the wrist's turned far enough).

use inkframe_core::{
    BoneBinding, BonePoints, CorrectionKey, Document, Element, LayerId, Point, RestPart, Skeleton,
    SmartAction, corrected,
};

use crate::editor::{Editor, Tool};

impl Editor {
    /// The skeleton being worked on: the selected layer's, if it's a
    /// skeleton's folder, or the one the selected layer is bound to.
    pub fn active_skeleton(&self) -> Option<LayerId> {
        let layer = self.selected_layer()?;
        if layer.skeleton.is_some() {
            return Some(layer.id);
        }
        self.doc.skeleton_layer_of(layer).map(|l| l.id)
    }

    pub fn skeleton(&self, id: LayerId) -> Option<&Skeleton> {
        self.doc.layer(id)?.skeleton.as_ref()
    }

    fn skeleton_mut(&mut self, id: LayerId) -> Option<&mut Skeleton> {
        self.doc.layer_mut(id)?.skeleton.as_mut()
    }

    /// Puts the selected layers in a new folder with an empty skeleton,
    /// selects it and picks the Add Bone tool.
    pub fn new_skeleton(&mut self) {
        self.new_folder();
        let Some(folder) = self.selection.filter(|&id| self.doc.layer(id).is_some_and(|l| l.folder)) else {
            return;
        };
        if let Some(layer) = self.doc.layer_mut(folder) {
            layer.skeleton = Some(Skeleton::default());
            layer.name = "Skeleton".into();
        }
        self.selected_bone = None;
        self.use_tool(Tool::AddBone);
    }

    /// Adds a bone from `start` to `end` (in the bones' coordinates) to
    /// skeleton `id`, hanging from the picked bone, and picks it. Bones are
    /// drawn in their rest pose, so they key at rest on the first frame.
    pub fn add_bone(&mut self, id: LayerId, start: Point, end: Point) -> Option<usize> {
        let parent = self.selected_bone;
        let bone = self.skeleton_mut(id)?.add_bone(parent, start, end, 0);
        self.selected_bone = Some(bone);
        Some(bone)
    }

    /// Takes the picked bone out of the active skeleton, with those hanging
    /// from it. Layers bound to a removed bone go back to following the
    /// bones near them.
    pub fn delete_bone(&mut self) -> bool {
        let (Some(id), Some(bone)) = (self.active_skeleton(), self.selected_bone) else { return false };
        let Some(map) = self.skeleton_mut(id).map(|s| s.remove_bone(bone)) else { return false };
        let renumber = |old: usize| map.get(old).copied().flatten();
        for member in self.skeleton_members(id) {
            let Some(layer) = self.doc.layer_mut(member) else { continue };
            if let BoneBinding::Bone(old) = layer.bone_binding {
                layer.bone_binding = renumber(old).map_or(BoneBinding::Region, BoneBinding::Bone);
            }
            // Pins to a bone that's gone go too.
            for drawing in &mut layer.drawings {
                drawing.bone_points.retain_mut(|pin| match renumber(pin.bone) {
                    Some(bone) => {
                        pin.bone = bone;
                        true
                    }
                    None => false,
                });
            }
        }
        self.selected_bone = None;
        true
    }

    /// Poses bone `bone` of skeleton `id` so its end reaches for `target`
    /// on the current frame (see `Skeleton::reach`).
    pub fn pose_bone(&mut self, id: LayerId, bone: usize, target: Point, alone: bool) {
        let frame = self.frame;
        if let Some(skeleton) = self.skeleton_mut(id) {
            skeleton.reach(bone, target, frame, alone);
        }
    }

    /// Stretches bone `bone` of skeleton `id` to `factor` times its rest
    /// length on the current frame, keying it.
    pub fn stretch_bone(&mut self, id: LayerId, bone: usize, factor: f32) {
        let frame = self.frame;
        let Some(bone) = self.skeleton_mut(id).and_then(|s| s.bones.get_mut(bone)) else { return };
        // The first stretch keys the rest length on the first frame too, so
        // the frames before it aren't stretched as well.
        let channel = bone.stretch.get_or_insert_with(|| inkframe_core::Channel::new(0, 1.0));
        channel.set(frame, factor.clamp(0.05, 20.0));
    }

    /// Moves bone `bone` of skeleton `id` to start at `to` on the current
    /// frame, carrying what hangs from it.
    pub fn move_bone(&mut self, id: LayerId, bone: usize, to: Point) {
        let frame = self.frame;
        if let Some(skeleton) = self.skeleton_mut(id) {
            skeleton.move_bone(bone, to, frame);
        }
    }

    /// Moves the rest pose of bone `bone`, or just its end, by `delta`.
    pub fn shift_bone_rest(&mut self, id: LayerId, bone: usize, delta: Point, part: RestPart) {
        if let Some(skeleton) = self.skeleton_mut(id) {
            skeleton.shift_rest(bone, delta, part);
        }
    }

    /// The layers bound to skeleton `id`: everything in its folder.
    pub fn skeleton_members(&self, id: LayerId) -> Vec<LayerId> {
        let Some(timeline) = self.doc.timeline_of(id) else { return Vec::new() };
        self.doc
            .layers_of(timeline)
            .iter()
            .filter(|l| self.doc.skeleton_layer_of(l).is_some_and(|s| s.id == id))
            .map(|l| l.id)
            .collect()
    }

    /// Pins the shapes picked inside a layer to `bone` (whole), or with
    /// `None`, unpins them and their points. Returns how many there were.
    pub fn pin_shapes(&mut self, bone: Option<usize>) -> usize {
        let Some(id) = self.inside_layer() else { return 0 };
        let picked = self.picked_shapes().to_vec();
        let frame = self.frame;
        let Some(layer) = self.doc.layer_mut(id) else { return 0 };
        let index = layer.drawing_index_at(frame);
        let Some(drawing) = layer.drawings.get_mut(index) else { return 0 };
        for shape in &picked {
            drawing.bone_points.retain(|p| p.colour_art != shape.colour_art || p.element != shape.index);
            if let Some(bone) = bone {
                drawing.bone_points.push(BonePoints {
                    colour_art: shape.colour_art,
                    element: shape.index,
                    anchors: None,
                    bone,
                });
            }
        }
        picked.len()
    }

    /// Pins point `anchor` of line-art shape `element` in `layer`'s drawing
    /// on this frame to `bone`, or with `None`, unpins it.
    pub fn pin_point(&mut self, layer: LayerId, element: usize, anchor: usize, bone: Option<usize>) {
        let frame = self.frame;
        let Some(layer) = self.doc.layer_mut(layer) else { return };
        let index = layer.drawing_index_at(frame);
        let Some(drawing) = layer.drawings.get_mut(index) else { return };
        // Out of whatever pinned it before (a whole shape keeps the rest of
        // its points, now listed one by one).
        let count = drawing.elements.get(element).map_or(0, |e| e.shape.to_anchors().0.len());
        for pin in drawing.bone_points.iter_mut().filter(|p| !p.colour_art && p.element == element) {
            let anchors = pin.anchors.get_or_insert_with(|| (0..count).collect());
            anchors.retain(|&a| a != anchor);
        }
        drawing.bone_points.retain(|p| p.anchors.as_ref().is_none_or(|a| !a.is_empty()));
        if let Some(bone) = bone {
            match drawing
                .bone_points
                .iter_mut()
                .find(|p| !p.colour_art && p.element == element && p.bone == bone)
            {
                Some(pin) => pin.anchors.get_or_insert_with(Vec::new).push(anchor),
                None => drawing.bone_points.push(BonePoints {
                    colour_art: false,
                    element,
                    anchors: Some(vec![anchor]),
                    bone,
                }),
            }
        }
    }

    /// Which bone point `anchor` of line-art shape `element` is pinned to.
    pub fn pinned_bone(&self, layer: LayerId, element: usize, anchor: usize) -> Option<usize> {
        let layer = self.doc.layer(layer)?;
        let drawing = layer.drawings.get(layer.drawing_index_at(self.frame))?;
        drawing
            .bone_points
            .iter()
            .find(|p| {
                !p.colour_art
                    && p.element == element
                    && p.anchors.as_ref().is_none_or(|a| a.contains(&anchor))
            })
            .map(|p| p.bone)
    }

    /// The bone every shape picked inside the layer is pinned to whole, if
    /// they're all pinned to the same one.
    pub fn shapes_pinned_bone(&self) -> Option<usize> {
        let layer = self.doc.layer(self.inside_layer()?)?;
        let drawing = layer.drawings.get(layer.drawing_index_at(self.frame))?;
        let mut bones = self.picked_shapes().iter().map(|s| {
            drawing
                .bone_points
                .iter()
                .find(|p| p.colour_art == s.colour_art && p.element == s.index && p.anchors.is_none())
                .map(|p| p.bone)
        });
        let first = bones.next()??;
        bones.all(|b| b == Some(first)).then_some(first)
    }

    /// Puts the selected layers in a new switch folder, which shows one of
    /// them at a time, and selects it.
    pub fn new_switch(&mut self) {
        self.new_folder();
        if let Some(folder) = self.selection.and_then(|id| self.doc.layer_mut(id)).filter(|l| l.folder) {
            folder.switch = true;
            folder.name = "Switch".into();
        }
    }

    pub fn set_bone_binding(&mut self, layer: LayerId, binding: BoneBinding) {
        if let Some(layer) = self.doc.layer_mut(layer) {
            layer.bone_binding = binding;
        }
    }

    /// Drops the picked bone if it's gone or its skeleton isn't the one
    /// being worked on. Called once a frame.
    pub fn check_bone(&mut self) {
        let count = self.active_skeleton().and_then(|id| self.skeleton(id)).map_or(0, |s| s.bones.len());
        if self.selected_bone.is_some_and(|b| b >= count) {
            self.selected_bone = None;
        }
    }
}

/// A smart bone's correction being drawn (see `SmartAction::Correct`): the
/// layer's drawing holds the shapes as reshaped at `turn`, to edit with any
/// tool, and the action is out of the bone meanwhile (so it doesn't reshape
/// them again). Finishing keys the shapes at `turn` and puts both back.
#[derive(Debug, Clone)]
pub struct Correcting {
    pub skeleton: LayerId,
    pub bone: usize,
    /// The action, and its place among the bone's.
    pub action: SmartAction,
    index: usize,
    pub layer: LayerId,
    pub drawing: usize,
    pub turn: f32,
    /// The drawing as drawn at rest: its colour art and line art.
    rest: (Vec<Element>, Vec<Element>),
}

/// Puts `correcting` back into `doc`: the shapes in the drawing keyed at its
/// turn (or not, if `keep` is false), the drawing as it was at rest, and the
/// action back on its bone.
fn settle(doc: &mut Document, correcting: &Correcting, keep: bool) {
    let Correcting { skeleton, bone, action, index, layer, drawing, turn, rest } = correcting;
    let mut action = action.clone();
    if let Some(found) = doc.layer_mut(*layer).and_then(|l| l.drawings.get_mut(*drawing)) {
        let colour_art = std::mem::replace(&mut found.colour_art, rest.0.clone());
        let line_art = std::mem::replace(&mut found.elements, rest.1.clone());
        if keep && let SmartAction::Correct { keys, .. } = &mut action {
            // Keyed again at much the same turn, the new shapes replace the old.
            keys.retain(|k| (k.turn - turn).abs() >= 0.5);
            keys.push(CorrectionKey { turn: *turn, colour_art, line_art });
            keys.sort_by(|a, b| a.turn.total_cmp(&b.turn));
        }
    }
    if let Some(actions) = doc
        .layer_mut(*skeleton)
        .and_then(|l| l.skeleton.as_mut())
        .and_then(|s| s.bones.get_mut(*bone))
        .map(|b| &mut b.actions)
    {
        actions.insert((*index).min(actions.len()), action);
    }
}

impl Editor {
    /// Starts drawing correction `action` of bone `bone` (in skeleton
    /// `id`) at the bone's turn now: the layer's drawing is reshaped as the
    /// correction has it there, ready to edit.
    pub fn start_correction(&mut self, id: LayerId, bone: usize, action: usize) -> Result<(), String> {
        self.finish_correction();
        let frame = self.frame;
        let skeleton = self.skeleton(id).ok_or("The skeleton has gone.")?;
        let b = skeleton.bones.get(bone).ok_or("The bone has gone.")?;
        let turn = b.turn.sample(frame as f32).unwrap_or(0.0);
        if turn.abs() < 1.0 {
            return Err("Turn the bone first: a correction is keyed at the turn it's drawn at.".into());
        }
        let Some(SmartAction::Correct { layer, drawing, keys }) = b.actions.get(action).cloned() else {
            return Err("That isn't a correction.".into());
        };
        let found = self.doc.layer(layer).ok_or("The layer it corrects has gone.")?;
        if found.drawing_index_at(frame) != drawing {
            return Err("The drawing it corrects isn't showing on this frame.".into());
        }
        let rest_drawing = found.drawings.get(drawing).ok_or("The drawing it corrects has gone.")?;
        let rest = (rest_drawing.colour_art.clone(), rest_drawing.elements.clone());
        let (colour, line) = corrected((&rest.0, &rest.1), &keys, turn);
        let taken = self.skeleton_mut(id).map(|s| s.bones[bone].actions.remove(action));
        let Some(taken) = taken else { return Err("The skeleton has gone.".into()) };
        if let Some(d) = self.doc.layer_mut(layer).and_then(|l| l.drawings.get_mut(drawing)) {
            d.colour_art = colour;
            d.elements = line;
        }
        self.correcting =
            Some(Correcting { skeleton: id, bone, action: taken, index: action, layer, drawing, turn, rest });
        Ok(())
    }

    /// Keys the correction being drawn at its turn, and puts everything back.
    pub fn finish_correction(&mut self) {
        if let Some(correcting) = self.correcting.take() {
            settle(&mut self.doc, &correcting, true);
        }
    }

    /// Stops drawing the correction without keying it.
    pub fn cancel_correction(&mut self) {
        if let Some(correcting) = self.correcting.take() {
            settle(&mut self.doc, &correcting, false);
        }
    }

    /// The document as it will be once the correction being drawn is keyed:
    /// what's saved, autosaved and exported, so a correction under way is
    /// never written out half done.
    pub fn settled_doc(&self) -> std::borrow::Cow<'_, Document> {
        match &self.correcting {
            Some(correcting) => {
                let mut doc = self.doc.clone();
                settle(&mut doc, correcting, true);
                std::borrow::Cow::Owned(doc)
            }
            None => std::borrow::Cow::Borrowed(&self.doc),
        }
    }

    /// Finishes the correction being drawn if what it was drawing at has
    /// moved: another frame, or the layer or skeleton gone. Called once a
    /// frame.
    pub fn check_correction(&mut self) {
        let Some(c) = &self.correcting else { return };
        let turn_now = self
            .skeleton(c.skeleton)
            .and_then(|s| s.bones.get(c.bone))
            .and_then(|b| b.turn.sample(self.frame as f32));
        let still = self.doc.layer(c.layer).is_some_and(|l| l.drawing_index_at(self.frame) == c.drawing)
            && turn_now.is_some_and(|t| (t - c.turn).abs() < 0.5);
        if !still {
            self.finish_correction();
        }
    }

    /// Adds a correction of `layer`'s drawing showing now to bone `bone`,
    /// and starts drawing it at the bone's turn. Returns the action's place.
    pub fn add_correction(&mut self, id: LayerId, bone: usize, layer: LayerId) -> Result<usize, String> {
        let drawing = self.doc.layer(layer).ok_or("The layer has gone.")?.drawing_index_at(self.frame);
        let actions = &mut self
            .skeleton_mut(id)
            .and_then(|s| s.bones.get_mut(bone))
            .ok_or("The bone has gone.")?
            .actions;
        actions.push(SmartAction::Correct { layer, drawing, keys: Vec::new() });
        let index = actions.len() - 1;
        self.start_correction(id, bone, index).map(|_| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inside::ShapeRef;
    use inkframe_core::{Color, Document, Element, Shape, Style, Transform};

    fn editor_with_arm_layer() -> (Editor, LayerId) {
        let mut doc = Document::default();
        let bar = Element {
            shape: Shape::Rect { width: 200.0, height: 20.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        let arm = doc.add_layer("Arm", vec![bar], 0, Transform::at(Point::new(400.0, 300.0)));
        let mut editor = Editor::new(doc);
        editor.select(Some(arm));
        (editor, arm)
    }

    #[test]
    fn a_new_skeleton_holds_the_selected_layers_and_bones_chain_up() {
        let (mut editor, arm) = editor_with_arm_layer();
        editor.new_skeleton();
        let skeleton = editor.active_skeleton().expect("the new folder is selected");
        assert_eq!(editor.doc.layer(arm).unwrap().in_folder, Some(skeleton));
        assert_eq!(editor.tool, Tool::AddBone);

        let upper = editor.add_bone(skeleton, Point::new(300.0, 300.0), Point::new(400.0, 300.0)).unwrap();
        let fore = editor.add_bone(skeleton, Point::new(400.0, 300.0), Point::new(500.0, 300.0)).unwrap();
        assert_eq!(editor.skeleton(skeleton).unwrap().bones[fore].parent, Some(upper));
        // The arm layer is bound to it, by region.
        editor.select(Some(arm));
        assert_eq!(editor.active_skeleton(), Some(skeleton));
    }

    #[test]
    fn posing_bends_the_bound_layer_and_keys_the_frame() {
        let (mut editor, arm) = editor_with_arm_layer();
        editor.new_skeleton();
        let skeleton = editor.active_skeleton().unwrap();
        editor.add_bone(skeleton, Point::new(300.0, 300.0), Point::new(400.0, 300.0));
        let fore = editor.add_bone(skeleton, Point::new(400.0, 300.0), Point::new(500.0, 300.0)).unwrap();
        editor.frame = 10;
        editor.pose_bone(skeleton, fore, Point::new(400.0, 400.0), true);

        let folder = editor.doc.layer(skeleton).unwrap();
        assert!(folder.key_frames().contains(&10));
        let layer = editor.doc.layer(arm).unwrap();
        let bounds = editor.doc.posed_elements(layer, 10)[0].shape.bounds();
        // The forearm's end, 100 below the elbow, in the arm layer's own
        // coordinates (centred on 400, 300).
        assert!(bounds.max.y > 90.0, "{bounds:?}");
        // On frame 0 it's straight.
        let rest = editor.doc.posed_elements(layer, 0)[0].shape.bounds();
        assert!(rest.max.y < 11.0, "{rest:?}");
    }

    #[test]
    fn deleting_a_bone_frees_layers_bound_to_it() {
        let (mut editor, arm) = editor_with_arm_layer();
        editor.new_skeleton();
        let skeleton = editor.active_skeleton().unwrap();
        let root = editor.add_bone(skeleton, Point::new(300.0, 300.0), Point::new(400.0, 300.0)).unwrap();
        let tip = editor.add_bone(skeleton, Point::new(400.0, 300.0), Point::new(500.0, 300.0)).unwrap();
        editor.set_bone_binding(arm, BoneBinding::Bone(tip));
        editor.selected_bone = Some(root);
        assert!(editor.delete_bone());
        assert!(editor.skeleton(skeleton).unwrap().bones.is_empty());
        assert_eq!(editor.doc.layer(arm).unwrap().bone_binding, BoneBinding::Region);
    }

    /// The arm layer in a skeleton with an upper arm and a forearm.
    fn rigged_arm() -> (Editor, LayerId, LayerId) {
        let (mut editor, arm) = editor_with_arm_layer();
        editor.new_skeleton();
        let skeleton = editor.active_skeleton().unwrap();
        editor.add_bone(skeleton, Point::new(300.0, 300.0), Point::new(400.0, 300.0));
        editor.add_bone(skeleton, Point::new(400.0, 300.0), Point::new(500.0, 300.0));
        (editor, skeleton, arm)
    }

    fn pins(editor: &Editor, layer: LayerId) -> Vec<BonePoints> {
        editor.doc.layer(layer).unwrap().drawings[0].bone_points.clone()
    }

    #[test]
    fn picked_shapes_pin_whole_and_unpin() {
        let (mut editor, _, arm) = rigged_arm();
        editor.enter_layer(arm);
        editor.pick_shapes(vec![ShapeRef::line(0)]);
        assert_eq!(editor.pin_shapes(Some(1)), 1);
        assert_eq!(editor.shapes_pinned_bone(), Some(1));
        assert_eq!(
            pins(&editor, arm),
            [BonePoints { colour_art: false, element: 0, anchors: None, bone: 1 }]
        );
        editor.pin_shapes(None);
        assert!(pins(&editor, arm).is_empty());
    }

    #[test]
    fn pinning_one_point_of_a_whole_pinned_shape_splits_it() {
        let (mut editor, _, arm) = rigged_arm();
        editor.enter_layer(arm);
        editor.pick_shapes(vec![ShapeRef::line(0)]);
        editor.pin_shapes(Some(0));
        // Point 2 of the box moves to the forearm; the other three stay.
        editor.pin_point(arm, 0, 2, Some(1));
        assert_eq!(editor.pinned_bone(arm, 0, 2), Some(1));
        assert_eq!(editor.pinned_bone(arm, 0, 0), Some(0));
        editor.pin_point(arm, 0, 2, None);
        assert_eq!(editor.pinned_bone(arm, 0, 2), None);
    }

    #[test]
    fn deleting_a_bone_drops_and_renumbers_pins() {
        let (mut editor, skeleton, arm) = rigged_arm();
        // A third bone, a root of its own, after the forearm.
        editor.selected_bone = None;
        editor.add_bone(skeleton, Point::new(300.0, 400.0), Point::new(300.0, 500.0));
        editor.pin_point(arm, 0, 0, Some(1));
        editor.pin_point(arm, 0, 1, Some(2));
        editor.select(Some(skeleton));
        editor.selected_bone = Some(0);
        editor.delete_bone();
        // The arm's two bones are gone; the third is now the first.
        assert_eq!(
            pins(&editor, arm),
            [BonePoints { colour_art: false, element: 0, anchors: Some(vec![1]), bone: 0 }]
        );
    }

    #[test]
    fn deleting_a_shape_keeps_pins_on_the_shapes_after_it() {
        let (mut editor, _, arm) = rigged_arm();
        let copy = editor.doc.layer(arm).unwrap().elements_at(0)[0].clone();
        editor.doc.layer_mut(arm).unwrap().elements_at_mut(0).push(copy);
        editor.pin_point(arm, 1, 0, Some(1));
        editor.enter_layer(arm);
        editor.pick_shapes(vec![ShapeRef::line(0)]);
        editor.delete_shapes();
        assert_eq!(editor.pinned_bone(arm, 0, 0), Some(1), "the second shape is the first now");
    }

    #[test]
    fn a_new_switch_holds_the_selected_layers() {
        let (mut editor, arm) = editor_with_arm_layer();
        editor.new_switch();
        let folder = editor.selection.unwrap();
        let switch = editor.doc.layer(folder).unwrap();
        assert!(switch.switch && switch.folder);
        assert_eq!(editor.doc.switch_choice(switch, 0), Some(arm));
    }

    #[test]
    fn shapes_drawn_into_a_bent_layer_show_where_theyre_drawn() {
        let (mut editor, skeleton, arm) = rigged_arm();
        // The forearm (from 400, 300 to 500, 300) hangs straight down.
        editor.skeleton_mut(skeleton).unwrap().bones[1].turn.set(0, 90.0);
        editor.select(Some(arm));
        editor.draw_into_layer = true;
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 5.0, radius_y: 5.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None, gradient: None },
        };
        // On the hanging forearm, 80 below the elbow.
        editor.add_drawn_shape("Dot", dot, Point::new(400.0, 380.0));
        let layer = editor.doc.layer(arm).unwrap();
        let shown = editor.doc.posed_elements(layer, 0);
        let centre = editor.doc.layer_matrix(layer, 0).apply(shown.last().unwrap().shape.bounds().center());
        assert!(centre.distance(Point::new(400.0, 380.0)) < 1.5, "{centre:?}");
        // And at rest it's on the forearm, 80 along it.
        let rest = editor
            .doc
            .layer_matrix(layer, 0)
            .apply(layer.elements_at(0).last().unwrap().shape.bounds().center());
        assert!(rest.distance(Point::new(480.0, 300.0)) < 1.5, "{rest:?}");
    }

    /// The arm, unbound (so only corrections reshape it), its forearm
    /// turned 45°.
    fn arm_to_correct() -> (Editor, LayerId, LayerId) {
        let (mut editor, skeleton, arm) = rigged_arm();
        editor.set_bone_binding(arm, BoneBinding::Unbound);
        editor.skeleton_mut(skeleton).unwrap().bones[1].turn.set(0, 45.0);
        (editor, skeleton, arm)
    }

    fn arm_height(editor: &Editor, arm: LayerId) -> f32 {
        let layer = editor.doc.layer(arm).unwrap();
        editor.doc.posed_elements(layer, 0)[0].shape.bounds().height()
    }

    /// Makes the arm's (only) shape `height` tall, as reshaping it by hand
    /// would.
    fn reshape(editor: &mut Editor, arm: LayerId, height: f32) {
        let shape = &mut editor.doc.layer_mut(arm).unwrap().drawings[0].elements[0].shape;
        *shape = Shape::Rect { width: 200.0, height, corner_radius: 0.0 };
    }

    #[test]
    fn a_correction_drawn_at_a_turn_is_keyed_there() {
        let (mut editor, skeleton, arm) = arm_to_correct();
        editor.add_correction(skeleton, 1, arm).unwrap();
        assert!(editor.correcting.is_some());
        assert!(
            editor.skeleton(skeleton).unwrap().bones[1].actions.is_empty(),
            "out of the bone while drawn"
        );
        reshape(&mut editor, arm, 60.0);
        editor.finish_correction();

        let action = &editor.skeleton(skeleton).unwrap().bones[1].actions[0];
        let SmartAction::Correct { keys, .. } = action else { panic!("a correction") };
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].turn, 45.0);
        // The drawing's back as drawn; the bone's turn reshapes it.
        let drawn = editor.doc.layer(arm).unwrap().drawings[0].elements[0].shape.bounds().height();
        assert!((drawn - 20.0).abs() < 1e-3, "{drawn}");
        assert!((arm_height(&editor, arm) - 60.0).abs() < 0.5);
        // Halfway back to rest, halfway between.
        editor.skeleton_mut(skeleton).unwrap().bones[1].turn.set(0, 22.5);
        assert!((arm_height(&editor, arm) - 40.0).abs() < 0.5, "{}", arm_height(&editor, arm));
    }

    #[test]
    fn cancelling_a_correction_keys_nothing() {
        let (mut editor, skeleton, arm) = arm_to_correct();
        editor.add_correction(skeleton, 1, arm).unwrap();
        reshape(&mut editor, arm, 60.0);
        editor.cancel_correction();
        let SmartAction::Correct { keys, .. } = &editor.skeleton(skeleton).unwrap().bones[1].actions[0]
        else {
            panic!("still a correction, with no keys")
        };
        assert!(keys.is_empty());
        assert!((arm_height(&editor, arm) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn whats_saved_never_has_a_correction_half_drawn() {
        let (mut editor, skeleton, arm) = arm_to_correct();
        editor.add_correction(skeleton, 1, arm).unwrap();
        reshape(&mut editor, arm, 60.0);
        let settled = editor.settled_doc().into_owned();
        // As drawn in the drawing, the key in the action.
        let drawn = settled.layer(arm).unwrap().drawings[0].elements[0].shape.bounds().height();
        assert!((drawn - 20.0).abs() < 1e-3);
        assert_eq!(settled.layer(skeleton).unwrap().skeleton.as_ref().unwrap().bones[1].actions.len(), 1);
        // And the correction's still being drawn in the editor.
        assert!(editor.correcting.is_some());
    }
}
