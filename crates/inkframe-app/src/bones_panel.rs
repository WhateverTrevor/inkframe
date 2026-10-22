//! The Bones panel: every skeleton in the timeline being edited in one
//! place, as Moho keeps its bones in a list. Each shows its bones as a
//! tree (click one to pick it, for posing or for the Properties panel),
//! marked with what's special about them, and the layers it rigs, with
//! how each follows the bones. Rig setup is switched here too.

use egui::RichText;
use inkframe_core::{BoneBinding, LayerId, Skeleton};

use crate::editor::{Editor, Tool};

/// The bones in tree order, each after the bone it hangs from, with how
/// deep it hangs.
fn tree(skeleton: &Skeleton) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let mut stack: Vec<(usize, usize)> =
        (0..skeleton.bones.len()).rev().filter(|&b| skeleton.chain(b).len() == 1).map(|b| (b, 0)).collect();
    while let Some((bone, depth)) = stack.pop() {
        rows.push((bone, depth));
        let children =
            (0..skeleton.bones.len()).rev().filter(|&b| b != bone && skeleton.bones[b].parent == Some(bone));
        stack.extend(children.map(|b| (b, depth + 1)));
    }
    rows
}

/// A few words for what's special about a bone.
fn badges(bone: &inkframe_core::Bone) -> String {
    let mut marks = Vec::new();
    if !bone.actions.is_empty() {
        marks.push("smart");
    }
    if bone.dynamics.is_some() {
        marks.push("swings");
    }
    if bone.limits.is_some() {
        marks.push("limited");
    }
    if bone.stretch.is_some() {
        marks.push("stretches");
    }
    marks.join(" · ")
}

pub fn show(ui: &mut egui::Ui, editor: &mut Editor) {
    let skeletons: Vec<LayerId> =
        editor.layers().iter().rev().filter(|l| l.skeleton.is_some()).map(|l| l.id).collect();
    let mut setup = editor.rig_setup;
    if ui
        .checkbox(&mut setup, "Rig setup")
        .on_hover_text("Show skeletons at rest, to draw, paint and rig on (View › Rig Setup)")
        .changed()
    {
        editor.rig_setup = setup;
    }
    ui.add_space(4.0);
    if skeletons.is_empty() {
        ui.weak("No skeletons yet. Select a character's layers and make one: they bend with its bones, painted ones too.");
        if ui.button("New Skeleton").clicked() {
            editor.new_skeleton();
        }
        return;
    }
    for id in skeletons {
        skeleton_section(ui, editor, id);
    }
}

fn skeleton_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let Some(layer) = editor.doc.layer(id) else { return };
    let Some(skeleton) = layer.skeleton.clone() else { return };
    let (name, frame) = (layer.name.clone(), editor.frame);
    let active = editor.active_skeleton() == Some(id);
    egui::CollapsingHeader::new(RichText::new(&name).strong())
        .id_salt(("skeleton", id))
        .default_open(true)
        .show(ui, |ui| {
            if skeleton.bones.is_empty() {
                ui.weak("No bones yet.");
            }
            for (bone, depth) in tree(&skeleton) {
                let b = &skeleton.bones[bone];
                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 14.0);
                    let picked = active && editor.selected_bone == Some(bone);
                    if ui.selectable_label(picked, &b.name).clicked() {
                        if !active {
                            editor.select(Some(id));
                        }
                        editor.selected_bone = Some(bone);
                    }
                    let marks = badges(b);
                    if !marks.is_empty() {
                        ui.weak(RichText::new(marks).small());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let turn = b.turn.sample(frame as f32).unwrap_or(0.0);
                        if turn.abs() >= 0.5 {
                            ui.weak(format!("{turn:.0}°"));
                        }
                    });
                });
            }
            if ui.small_button("Add Bones").on_hover_text("Draw bones with the Add Bone tool").clicked() {
                editor.select(Some(id));
                editor.use_tool(Tool::AddBone);
            }

            // What it rigs, and how each follows.
            let members = editor.skeleton_members(id);
            if members.is_empty() {
                return;
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Rigs").small().weak());
            let names: Vec<String> = skeleton.bones.iter().map(|b| b.name.clone()).collect();
            for member in members {
                let Some(layer) = editor.doc.layer(member) else { continue };
                if layer.folder {
                    continue;
                }
                let (layer_name, current) = (layer.name.clone(), layer.bone_binding);
                let pins: usize = layer.drawings.iter().map(|d| d.bone_points.len()).sum();
                let label = |binding: BoneBinding| match binding {
                    BoneBinding::Region => "Nearby bones".to_owned(),
                    BoneBinding::Bone(i) => {
                        names.get(i).cloned().unwrap_or_else(|| "Nearby bones".to_owned())
                    }
                    BoneBinding::Unbound => "None".to_owned(),
                };
                let mut chosen = current;
                ui.horizontal(|ui| {
                    if ui.selectable_label(editor.selection == Some(member), &layer_name).clicked() {
                        editor.select(Some(member));
                    }
                    if pins > 0 {
                        ui.weak(RichText::new(format!("{pins} pinned")).small());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt(("follows", member))
                            .selected_text(label(current))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut chosen,
                                    BoneBinding::Region,
                                    label(BoneBinding::Region),
                                );
                                for i in 0..names.len() {
                                    ui.selectable_value(
                                        &mut chosen,
                                        BoneBinding::Bone(i),
                                        label(BoneBinding::Bone(i)),
                                    );
                                }
                                ui.selectable_value(
                                    &mut chosen,
                                    BoneBinding::Unbound,
                                    label(BoneBinding::Unbound),
                                );
                            });
                    });
                });
                if chosen != current {
                    editor.set_bone_binding(member, chosen);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::Point;

    #[test]
    fn bones_list_as_a_tree_each_under_the_bone_it_hangs_from() {
        let mut skeleton = Skeleton::default();
        let spine = skeleton.add_bone(None, Point::ZERO, Point::new(0.0, -100.0), 0);
        let tail = skeleton.add_bone(None, Point::ZERO, Point::new(-80.0, 40.0), 0);
        let arm = skeleton.add_bone(Some(spine), Point::new(0.0, -90.0), Point::new(80.0, -90.0), 0);
        let hand = skeleton.add_bone(Some(arm), Point::new(80.0, -90.0), Point::new(120.0, -90.0), 0);
        let head = skeleton.add_bone(Some(spine), Point::new(0.0, -100.0), Point::new(0.0, -150.0), 0);
        assert_eq!(tree(&skeleton), [(spine, 0), (arm, 1), (hand, 2), (head, 1), (tail, 0)]);
    }
}
