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

use inkframe_core::{BoneBinding, BonePoints, LayerId, Point, RestPart, Skeleton};

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
}
