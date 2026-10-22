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

use inkframe_core::{BoneBinding, LayerId, Point, RestPart, Skeleton};

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
        let members: Vec<LayerId> = self
            .doc
            .layers_of(self.timeline())
            .iter()
            .filter(|l| self.doc.skeleton_layer_of(l).is_some_and(|s| s.id == id))
            .map(|l| l.id)
            .collect();
        for member in members {
            if let Some(layer) = self.doc.layer_mut(member)
                && let BoneBinding::Bone(old) = layer.bone_binding
            {
                layer.bone_binding = match map.get(old).copied().flatten() {
                    Some(new) => BoneBinding::Bone(new),
                    None => BoneBinding::Region,
                };
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
}
