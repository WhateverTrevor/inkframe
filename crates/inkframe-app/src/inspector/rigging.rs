//! Rigging in the Properties panel: a layer's skeleton and its bones,
//! switch layers and pins, smart bones and their actions, and the rig,
//! deformer and warp settings.

use egui::DragValue;
use inkframe_core::{Deformer, LayerId, RotationLimits};

use super::{section, section_text};
use crate::editor::Editor;
use crate::i18n::{tr, tr_args};
use crate::icons::Icon;

/// A skeleton's picked bone, for its folder; for a layer in that folder,
/// how it follows the bones (see `skeleton.rs` in the core).
pub(super) fn skeleton_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(layer) = editor.doc.layer(id) else { return };
    if layer.skeleton.is_some() {
        if !section(ui, editor, "skeleton", tr("Skeleton"), Icon::Bones) {
            return;
        }
        let picked = editor.selected_bone;
        let mut delete = false;
        let Some(skeleton) = editor.doc.layer_mut(id).and_then(|l| l.skeleton.as_mut()) else { return };
        let count = skeleton.bones.len();
        ui.weak(match count {
            0 => tr("No bones yet: draw them with the Add Bone tool (Shift+K).").to_owned(),
            1 => tr("1 bone. Pose it with the Bone tool (K).").to_owned(),
            n => tr_args("{count} bones. Pose them with the Bone tool (K).", &[("count", &n.to_string())]),
        });
        // The bones the picked one could reach for: any but those it
        // carries (or itself).
        let target_choices: Vec<(usize, String, bool)> = match picked {
            Some(b) => skeleton
                .bones
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != b)
                .map(|(i, other)| (i, other.name.clone(), !skeleton.target_chain(b, i).is_empty()))
                .collect(),
            None => Vec::new(),
        };
        if let Some(bone) = picked.and_then(|b| skeleton.bones.get_mut(b)) {
            egui::Grid::new("bone").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
                ui.label(tr("Bone"));
                ui.text_edit_singleline(&mut bone.name);
                ui.end_row();
                ui.label(tr("Strength")).on_hover_text(
                    tr("How far it reaches into the drawings around it: points about this far away follow it about as much as the next bone"),
                );
                ui.add(DragValue::new(&mut bone.strength).range(1.0..=2000.0).speed(1.0).suffix(" px"));
                ui.end_row();
                ui.label(tr("Turn"));
                let mut turn = bone.turn.sample(frame as f32).unwrap_or(0.0);
                if ui.add(DragValue::new(&mut turn).speed(1.0).suffix("°")).changed() {
                    bone.turn.set(frame, turn);
                }
                ui.end_row();
                ui.label(tr("Stretch")).on_hover_text(tr("How long it is against how it was drawn; Shift-drag it with the Bone tool"));
                let mut stretch = bone.stretch.as_ref().and_then(|c| c.sample(frame as f32)).unwrap_or(1.0) * 100.0;
                if ui.add(DragValue::new(&mut stretch).range(5.0..=2000.0).speed(1.0).suffix("%")).changed() {
                    let channel = bone.stretch.get_or_insert_with(|| inkframe_core::Channel::new(0, 1.0));
                    channel.set(frame, stretch / 100.0);
                }
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut bone.squash, tr("Keep area")).on_hover_text(
                    tr("Stretching it longer makes it thinner, and squashing it fatter, as a bouncing ball does"),
                );
                ui.end_row();
                ui.label(tr("Limits")).on_hover_text(tr("How far it may turn from rest, in degrees"));
                ui.horizontal(|ui| {
                    let mut limited = bone.limits.is_some();
                    if ui.checkbox(&mut limited, "").changed() {
                        bone.limits = limited.then_some(inkframe_core::RotationLimits { min: -90.0, max: 90.0 });
                    }
                    if let Some(limits) = &mut bone.limits {
                        ui.add(DragValue::new(&mut limits.min).range(-360.0..=360.0).speed(1.0).suffix("°"));
                        ui.label(tr("to"));
                        ui.add(DragValue::new(&mut limits.max).range(-360.0..=360.0).speed(1.0).suffix("°"));
                    }
                });
                ui.end_row();
                ui.label(tr("Target")).on_hover_text(
                    tr("A bone its end reaches for, the bones above it bending to get it there: plant a foot on one"),
                );
                let names_and_ok = &target_choices;
                let current = bone.target;
                let label = |t: Option<usize>| {
                    t.and_then(|t| names_and_ok.iter().find(|(i, _, _)| *i == t)).map_or(tr("None").to_owned(), |(_, n, _)| n.clone())
                };
                let mut chosen = current;
                egui::ComboBox::from_id_salt("bone target").selected_text(label(current)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, tr("None"));
                    for (i, name, ok) in names_and_ok {
                        ui.add_enabled_ui(*ok, |ui| {
                            ui.selectable_value(&mut chosen, Some(*i), name);
                        });
                    }
                });
                if chosen != current {
                    bone.target = chosen;
                }
                ui.end_row();
                ui.label(tr("Dynamics")).on_hover_text(
                    tr("It swings after the bones it hangs from, overshoots and settles, as hair or a tail does"),
                );
                let mut swings = bone.dynamics.is_some();
                if ui.checkbox(&mut swings, "").changed() {
                    bone.dynamics = swings.then(inkframe_core::Dynamics::default);
                }
                ui.end_row();
                if let Some(dynamics) = &mut bone.dynamics {
                    ui.label(tr("  Spring")).on_hover_text(tr("How hard it's pulled back to its pose: low is floppy"));
                    ui.add(egui::Slider::new(&mut dynamics.spring, 0.01..=1.0));
                    ui.end_row();
                    ui.label(tr("  Damping")).on_hover_text(tr("How quickly its swing dies away"));
                    ui.add(egui::Slider::new(&mut dynamics.damping, 0.0..=1.0));
                    ui.end_row();
                    ui.label(tr("  Swing")).on_hover_text(tr("The furthest it may swing from its pose"));
                    ui.add(egui::Slider::new(&mut dynamics.limit, 0.0..=180.0).suffix("°"));
                    ui.end_row();
                }
            });
            delete = ui.small_button(tr("Delete Bone")).clicked();
        }
        if delete {
            editor.delete_bone();
        } else if let Some(bone) = picked.filter(|&b| b < count) {
            smart_bone_section(ui, editor, id, bone);
        }
        ui.add_space(8.0);
        return;
    }
    if editor.doc.skeleton_layer_of(layer).is_none()
        || !section(ui, editor, "bones", tr("Bones"), Icon::Bones)
    {
        return;
    }
    let Some(layer) = editor.doc.layer(id) else { return };
    let Some(owner) = editor.doc.skeleton_layer_of(layer) else { return };
    let names: Vec<String> =
        owner.skeleton.iter().flat_map(|s| s.bones.iter().map(|b| b.name.clone())).collect();
    let current = layer.bone_binding;
    // Bones with others hanging from them, which a layer can follow as a limb.
    let limbs: Vec<bool> = owner
        .skeleton
        .iter()
        .flat_map(|s| (0..s.bones.len()).map(|i| s.limb(i).iter().filter(|&&b| b).count() > 1))
        .collect();
    let mut chosen = current;
    ui.horizontal(|ui| {
        ui.label(tr("Follows")).on_hover_text(
            tr("Nearby bones bends the drawing round its joints; a limb bends it with one bone and those below it only (for legs drawn over each other); one bone moves it whole, as a hand or a hat should"),
        );
        chosen = binding_menu(ui, "bone binding", current, &names, &limbs);
    });
    if chosen != current {
        editor.set_bone_binding(id, chosen);
    }
    pin_controls(ui, editor, id, &names);
    ui.add_space(8.0);
}

/// Whether a folder is a switch, showing one of its layers at a time, and
/// which it shows from this frame on.
pub(super) fn switch_controls(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    let Some(folder) = editor.doc.layer(id) else { return };
    let mut switch = folder.switch;
    ui.add_space(4.0);
    let toggled = ui
        .checkbox(&mut switch, tr("Switch"))
        .on_hover_text(tr("Show one of the folder's layers at a time: hand poses, mouth shapes"))
        .changed();
    if toggled && let Some(folder) = editor.doc.layer_mut(id) {
        folder.switch = switch;
    }
    let Some(folder) = editor.doc.layer(id).filter(|l| l.switch) else { return };
    let children: Vec<(LayerId, String)> =
        editor.doc.folder_children(folder).iter().map(|c| (c.id, c.name.clone())).collect();
    let showing = editor.doc.switch_choice(folder, frame);
    let smart = editor.doc.smart_switch(folder, frame).is_some();
    let mut chosen = showing;
    let label = |id: Option<LayerId>| {
        id.and_then(|id| children.iter().find(|(c, _)| *c == id))
            .map_or(tr("(empty)").to_owned(), |(_, n)| n.clone())
    };
    ui.horizontal(|ui| {
        ui.label(tr("Showing")).on_hover_text(tr("Keyed on this frame; it holds until the next key"));
        egui::ComboBox::from_id_salt("switch showing").selected_text(label(showing)).show_ui(ui, |ui| {
            for (child, _) in &children {
                ui.selectable_value(&mut chosen, Some(*child), label(Some(*child)));
            }
        });
    });
    if smart {
        ui.weak(tr("A smart bone is choosing for now."));
    }
    if chosen != showing
        && let Some(child) = chosen
    {
        editor.doc.set_switch(id, frame, child);
    }
}

/// Pinning the shapes picked inside this layer, or the point picked with
/// the Subselection tool, to a bone of their own.
pub(super) fn pin_controls(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, names: &[String]) {
    if names.is_empty() {
        return;
    }
    let shapes = if editor.inside_layer() == Some(id) { editor.picked_shapes().len() } else { 0 };
    let point = editor.picked_anchor.filter(|(layer, _, _)| *layer == id);
    if shapes == 0 && point.is_none() {
        ui.weak(tr("To pin shapes or points to one bone, pick them: double-click the layer, or use the Subselection tool (A)."));
        return;
    }
    let what = match (point, shapes) {
        (Some(_), _) => tr("Pin point to").to_owned(),
        (None, 1) => tr("Pin shape to").to_owned(),
        (None, n) => tr_args("Pin {count} shapes to", &[("count", &n.to_string())]),
    };
    let pinned = match point {
        Some((layer, element, anchor)) => editor.pinned_bone(layer, element, anchor),
        None => editor.shapes_pinned_bone(),
    };
    let mut chosen = pinned;
    let label = |bone: Option<usize>| {
        bone.and_then(|b| names.get(b).cloned()).unwrap_or_else(|| tr("(not pinned)").into())
    };
    ui.horizontal(|ui| {
        ui.label(what)
            .on_hover_text(tr("They follow that bone alone, whatever the rest of the drawing does"));
        egui::ComboBox::from_id_salt("pin bone").selected_text(label(chosen)).show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, label(None));
            for i in 0..names.len() {
                ui.selectable_value(&mut chosen, Some(i), label(Some(i)));
            }
        });
    });
    if chosen != pinned {
        match point {
            Some((layer, element, anchor)) => editor.pin_point(layer, element, anchor, chosen),
            None => {
                editor.pin_shapes(chosen);
            }
        }
    }
}

/// What a layer following `binding` is called in a menu, with the bones'
/// `names`.
pub(crate) fn binding_label(binding: inkframe_core::BoneBinding, names: &[String]) -> String {
    use inkframe_core::BoneBinding;
    let name = |i: usize| names.get(i).cloned();
    match binding {
        BoneBinding::Region => tr("Nearby bones").to_owned(),
        BoneBinding::Limb(i) => name(i)
            .map_or_else(|| tr("Nearby bones").to_owned(), |n| tr_args("{bone} and below", &[("bone", &n)])),
        BoneBinding::Bone(i) => name(i).unwrap_or_else(|| tr("Nearby bones").to_owned()),
        BoneBinding::Unbound => tr("None").to_owned(),
    }
}

/// A menu of how a layer can follow a skeleton's bones: nearby bones, a
/// limb (each bone with others hanging from it, marked in `limbs`), one
/// bone whole, or none. Returns the choice.
pub(crate) fn binding_menu(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    current: inkframe_core::BoneBinding,
    names: &[String],
    limbs: &[bool],
) -> inkframe_core::BoneBinding {
    use inkframe_core::BoneBinding;
    let mut chosen = current;
    egui::ComboBox::from_id_salt(id).selected_text(binding_label(current, names)).show_ui(ui, |ui| {
        let mut option = |ui: &mut egui::Ui, binding| {
            ui.selectable_value(&mut chosen, binding, binding_label(binding, names));
        };
        option(ui, BoneBinding::Region);
        if limbs.iter().any(|&l| l) {
            ui.label(section_text(tr("Bend with a limb")));
            for i in (0..names.len()).filter(|&i| limbs.get(i).copied().unwrap_or(false)) {
                option(ui, BoneBinding::Limb(i));
            }
        }
        ui.label(section_text(tr("Move with one bone")));
        for i in 0..names.len() {
            option(ui, BoneBinding::Bone(i));
        }
        ui.separator();
        option(ui, BoneBinding::Unbound);
    });
    chosen
}

/// A switch folder for the smart bone section: its id and name, and its
/// layers' ids and names.
pub(super) type SwitchChoices = (LayerId, String, Vec<(LayerId, String)>);

/// What a smart bone does as it turns: each action on its own row, with
/// buttons to add more.
pub(super) fn smart_bone_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, bone: usize) {
    use inkframe_core::SmartAction;
    let frame = editor.frame;
    if !section(ui, editor, "smart bone", tr("Smart bone"), Icon::Dial) {
        return;
    }
    // First, as it may preview the bone at another turn, or retime actions.
    action_timeline(ui, editor, id, bone);
    let members = editor.skeleton_members(id);
    let name = |id: LayerId| editor.doc.layer(id).map_or_else(|| tr("(gone)").to_owned(), |l| l.name.clone());
    // Layers that can be reshaped (they have another drawing to morph
    // toward), and switches (with their layers).
    let morphable: Vec<(LayerId, String, Vec<String>)> = members
        .iter()
        .filter_map(|&m| editor.doc.layer(m))
        .filter(|l| l.drawings.len() > 1)
        .map(|l| (l.id, l.name.clone(), l.drawings.iter().map(|d| d.name.clone()).collect()))
        .collect();
    let switches: Vec<SwitchChoices> = members
        .iter()
        .filter_map(|&m| editor.doc.layer(m))
        .filter(|l| l.switch)
        .map(|l| {
            let children = editor.doc.folder_children(l).iter().map(|c| (c.id, c.name.clone())).collect();
            (l.id, l.name.clone(), children)
        })
        .collect();
    let Some(skeleton) = editor.skeleton(id) else { return };
    let Some(the_bone) = skeleton.bones.get(bone) else { return };
    // As shown: previewed on the action timeline, or as animated.
    let now = the_bone.shown_turn(frame as f32);

    // Layers a correction can reshape: any drawn layer it rigs.
    let correctable: Vec<(LayerId, String)> = members
        .iter()
        .filter_map(|&m| editor.doc.layer(m))
        .filter(|l| !l.folder && l.draws_vectors())
        .map(|l| (l.id, l.name.clone()))
        .collect();

    let mut actions = the_bone.actions.clone();
    // A correction being drawn on this bone: say so, and how to finish.
    if let Some(c) = editor.correcting.as_ref().filter(|c| c.skeleton == id && c.bone == bone) {
        let what = tr_args(
            "Drawing the correction of {layer} at {turn}°: reshape it with any tool.",
            &[("layer", &name(c.layer)), ("turn", &format!("{:.0}", c.turn))],
        );
        ui.label(egui::RichText::new(what).strong());
        let (mut finish, mut cancel) = (false, false);
        ui.horizontal(|ui| {
            finish = ui.button(tr("Finish")).on_hover_text(tr("Key the shapes at this turn")).clicked();
            cancel = ui.button(tr("Cancel")).clicked();
        });
        if finish {
            editor.finish_correction();
        } else if cancel {
            editor.cancel_correction();
        }
        return;
    }
    // Starting or adding a correction changes the document, so it waits
    // until the actions have been written back.
    enum Request {
        Edit(usize),
        Add(LayerId),
    }
    let mut request = None;
    let mut remove = None;
    for (i, action) in actions.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| match action {
            SmartAction::Morph { layer, drawing, turn } => {
                ui.label(tr("Reshape"));
                egui::ComboBox::from_id_salt(("smart layer", i)).selected_text(name(*layer)).show_ui(
                    ui,
                    |ui| {
                        for (m, n, _) in &morphable {
                            ui.selectable_value(layer, *m, n);
                        }
                    },
                );
                let drawings = morphable
                    .iter()
                    .find(|(m, _, _)| m == layer)
                    .map(|(_, _, d)| d.clone())
                    .unwrap_or_default();
                let shown = drawings.get(*drawing).cloned().unwrap_or_else(|| "?".into());
                ui.label(tr("to"));
                egui::ComboBox::from_id_salt(("smart drawing", i)).selected_text(shown).show_ui(ui, |ui| {
                    for (d, n) in drawings.iter().enumerate() {
                        ui.selectable_value(drawing, d, n);
                    }
                });
                ui.label(tr("at"));
                ui.add(DragValue::new(turn).speed(1.0).suffix("°"));
                remove = remove.or(ui.small_button("×").clicked().then_some(i));
            }
            SmartAction::Switch { folder, layer, turn } => {
                ui.label(tr("Switch"));
                egui::ComboBox::from_id_salt(("smart switch", i)).selected_text(name(*folder)).show_ui(
                    ui,
                    |ui| {
                        for (m, n, _) in &switches {
                            ui.selectable_value(folder, *m, n);
                        }
                    },
                );
                let children = switches
                    .iter()
                    .find(|(m, _, _)| m == folder)
                    .map(|(_, _, c)| c.clone())
                    .unwrap_or_default();
                ui.label(tr("to"));
                egui::ComboBox::from_id_salt(("smart child", i)).selected_text(name(*layer)).show_ui(
                    ui,
                    |ui| {
                        for (c, n) in &children {
                            ui.selectable_value(layer, *c, n);
                        }
                    },
                );
                ui.label(tr("past"));
                ui.add(DragValue::new(turn).speed(1.0).suffix("°"));
                remove = remove.or(ui.small_button("×").clicked().then_some(i));
            }
            SmartAction::Correct { layer, keys, .. } => {
                ui.label(tr_args("Correct {layer}", &[("layer", &name(*layer))]));
                // The keys themselves are on the action timeline above.
                ui.weak(match keys.len() {
                    0 => tr("(no keys yet)").to_owned(),
                    1 => tr("1 key").to_owned(),
                    n => tr_args("{count} keys", &[("count", &n.to_string())]),
                });
                let edit = ui
                    .add_enabled(
                        now.abs() >= 1.0,
                        egui::Button::new(tr_args("Edit at {turn}°", &[("turn", &format!("{now:.0}"))]))
                            .small(),
                    )
                    .on_hover_text(tr(
                        "Reshape the layer as it should be at the bone's turn now, keying it there",
                    ))
                    .on_disabled_hover_text(tr("Turn the bone first: a correction is keyed at a turn"));
                if edit.clicked() {
                    request = Some(Request::Edit(i));
                }
                remove = remove.or(ui.small_button(tr("Remove")).clicked().then_some(i));
            }
        });
    }
    // A new action is reached at the bone's turn now, if it's turned, so
    // the way to set one up is to pose the bone, then add it.
    let at = if now.abs() > 1.0 { now } else { 90.0 };
    ui.horizontal(|ui| {
        if let Some((layer, _)) = correctable.iter().find(|(l, _)| Some(*l) == editor.selection).or(correctable.first())
            && ui
                .add_enabled(now.abs() >= 1.0, egui::Button::new(tr("+ Correction")).small())
                .on_hover_text(tr("Reshape a layer by hand at this turn of the bone, as Moho's smart bone actions; key more turns as you go"))
                .on_disabled_hover_text(tr("Turn the bone first: a correction is keyed at a turn"))
                .clicked()
        {
            request = Some(Request::Add(*layer));
        }
        if let Some((layer, _, _)) = morphable.first()
            && ui
                .small_button(tr("+ Reshape"))
                .on_hover_text(tr("Morph a layer toward another of its drawings as the bone turns"))
                .clicked()
        {
            actions.push(SmartAction::Morph { layer: *layer, drawing: 1, turn: at });
        }
        if let Some((folder, _, children)) = switches.first()
            && let Some((child, _)) = children.last()
            && ui
                .small_button(tr("+ Switch"))
                .on_hover_text(tr("Have a switch show another of its layers once the bone's turned far enough"))
                .clicked()
        {
            actions.push(SmartAction::Switch { folder: *folder, layer: *child, turn: at });
        }
    });
    if morphable.is_empty() && switches.is_empty() {
        ui.weak(tr("To make it smart, give a layer in the skeleton a second drawing to reshape toward, or add a switch (Modify › New Switch)."));
    }
    if let Some(i) = remove {
        actions.remove(i);
    }
    let changed = editor.skeleton(id).and_then(|s| s.bones.get(bone)).is_some_and(|b| b.actions != actions);
    if changed
        && let Some(b) =
            editor.doc.layer_mut(id).and_then(|l| l.skeleton.as_mut()).and_then(|s| s.bones.get_mut(bone))
    {
        b.actions = actions;
    }
    let started = match request {
        Some(Request::Edit(i)) => editor.start_correction(id, bone, i),
        Some(Request::Add(layer)) => editor.add_correction(id, bone, layer).map(|_| ()),
        None => Ok(()),
    };
    if let Err(problem) = started {
        crate::app::show_error(tr("Couldn't draw the correction"), &problem);
    }
}

/// The bone's action timeline (see `action_timeline.rs`): its actions laid
/// out along its turns, to scrub through and retime. What it changes is
/// written straight back, before the rest of the section reads the actions.
pub(super) fn action_timeline(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, bone: usize) {
    use crate::action_timeline::{self, Asked};
    use inkframe_core::SmartAction;
    let Some(the_bone) = editor.skeleton(id).and_then(|s| s.bones.get(bone)) else { return };
    let now = the_bone.shown_turn(editor.frame as f32);
    if the_bone.actions.is_empty() {
        return;
    }
    let mut actions = the_bone.actions.clone();
    let name = |id: LayerId| editor.doc.layer(id).map_or_else(|| tr("(gone)").to_owned(), |l| l.name.clone());
    let names: Vec<String> = actions
        .iter()
        .map(|action| match action {
            SmartAction::Morph { layer, .. } => tr_args("Reshape {layer}", &[("layer", &name(*layer))]),
            SmartAction::Switch { folder, .. } => tr_args("Switch {folder}", &[("folder", &name(*folder))]),
            SmartAction::Correct { layer, .. } => tr_args("Correct {layer}", &[("layer", &name(*layer))]),
        })
        .collect();
    let span = action_timeline::span(&actions, the_bone.limits, now);
    let previewing = editor.previewed_turn(id, bone).is_some();
    let asked = action_timeline::show(ui, &mut actions, &names, span, now, previewing);
    if let Some(b) =
        editor.doc.layer_mut(id).and_then(|l| l.skeleton.as_mut()).and_then(|s| s.bones.get_mut(bone))
        && b.actions != actions
    {
        b.actions = actions;
    }
    match asked {
        Some(Asked::Preview(turn)) => editor.preview_turn(id, bone, turn),
        Some(Asked::EndPreview) => editor.end_preview(),
        Some(Asked::Edit { action, turn }) => {
            editor.preview_turn(id, bone, turn);
            if let Err(problem) = editor.start_correction(id, bone, action) {
                crate::app::show_error(tr("Couldn't draw the correction"), &problem);
            }
        }
        None => {}
    }
}

/// How the layer hangs in a character rig: the layer it's attached to, and
/// the pivot it turns around.
pub(super) fn rig_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId, frame: u32) {
    if !section(ui, editor, "parent and pivot", tr("Parent and pivot"), Icon::Nodes) {
        return;
    }
    let Some(layer) = editor.doc.layer(id) else { return };
    // Only layers it can attach to without making a loop.
    let parents: Vec<(LayerId, String)> = editor
        .layers()
        .iter()
        .filter(|other| editor.doc.can_parent(id, other.id))
        .map(|other| (other.id, other.name.clone()))
        .collect();
    let (old_parent, old_pivot, old_limits) = (layer.parent, layer.pivot, layer.rotation_limits);
    let (mut parent, mut pivot, mut limits) = (old_parent, old_pivot, old_limits);
    let rotation = editor.doc.pose(layer, frame).rotation;
    let bounds = editor.doc.layer_bounds(layer, frame);

    egui::Grid::new("rig").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Parent")).on_hover_text(tr(
            "The layer this one is attached to. It moves, turns and scales with its parent, \
             and its own position is measured from the parent.",
        ));
        let current = parents.iter().find(|(p, _)| Some(*p) == parent).map_or(tr("None"), |(_, name)| name);
        egui::ComboBox::from_id_salt("parent").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut parent, None, tr("None"));
            for (other, name) in &parents {
                ui.selectable_value(&mut parent, Some(*other), name);
            }
        });
        ui.end_row();

        ui.label(tr("Pivot"))
            .on_hover_text(tr("The point the layer turns and scales around, in its own coordinates. \
             Option-drag (Alt-drag) the ring on the stage to move it."));
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut pivot.x).prefix("x ").speed(1.0));
            ui.add(DragValue::new(&mut pivot.y).prefix("y ").speed(1.0));
            if let Some(bounds) = bounds
                && ui
                    .small_button(tr("Center"))
                    .on_hover_text(tr("Put the pivot in the middle of the layer"))
                    .clicked()
            {
                pivot = bounds.center();
            }
        });
        ui.end_row();

        // Only joints bend, so only attached layers have limits.
        if parent.is_some() {
            ui.label(tr("Bends")).on_hover_text(tr(
                "How far the Bone tool may turn this joint, as the lowest and highest rotation, \
                 so an elbow or knee only bends one way.",
            ));
            ui.horizontal(|ui| {
                let mut limited = limits.is_some();
                if ui.checkbox(&mut limited, "").changed() {
                    // Start from a range around where it is now.
                    limits = limited.then_some(RotationLimits { min: rotation - 45.0, max: rotation + 45.0 });
                }
                match &mut limits {
                    Some(range) => {
                        ui.add(DragValue::new(&mut range.min).suffix("°").speed(1.0));
                        ui.label(tr("to"));
                        ui.add(DragValue::new(&mut range.max).suffix("°").speed(1.0));
                    }
                    None => {
                        ui.weak(tr("freely"));
                    }
                }
            });
            ui.end_row();
        }
    });

    // Both keep the layer where it is on screen: attaching it re-measures
    // its keys from the parent, and moving the pivot shifts its position.
    if parent != old_parent {
        editor.doc.set_parent(id, parent, frame);
    }
    if pivot != old_pivot {
        editor.doc.set_pivot(id, pivot);
    }
    if limits != old_limits
        && let Some(layer) = editor.doc.layer_mut(id)
    {
        layer.rotation_limits = limits;
    }
}

/// A bone deformer bending the layer's drawing: adding one, its bones'
/// bends on this frame, and how soft the bends are.
pub(super) fn deformer_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    if editor.doc.layer(id).is_none_or(|l| l.instance.is_some())
        || !section(ui, editor, "deformer", tr("Deformer"), Icon::Envelope)
    {
        return;
    }
    let Some(layer) = editor.doc.layer_mut(id).filter(|l| l.instance.is_none()) else { return };
    // The drawing as drawn, not bent, is what the bones are laid along.
    let rest_bounds = layer.elements_at(frame).iter().map(|e| e.bounds()).reduce(|a, b| a.union(b));

    let Some(deformer) = &mut layer.deformer else {
        let add = ui
            .add_enabled(rest_bounds.is_some(), egui::Button::new(tr("Add Bend Deformer")))
            .on_hover_text(tr("Bones through the drawing that bend it smoothly, for rubber-hose limbs and tails. Pose them with the Bone tool."));
        if add.clicked()
            && let Some(bounds) = rest_bounds
        {
            layer.deformer = Some(Deformer::along(bounds, 2, frame));
        }
        ui.add_space(8.0);
        return;
    };

    let mut remove = false;
    let mut bones = deformer.bone_count();
    egui::Grid::new("deformer").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
        ui.label(tr("Bones")).on_hover_text(tr("Changing the count lays the bones out straight again"));
        ui.add(DragValue::new(&mut bones).range(1..=8));
        ui.end_row();
        ui.label(tr("Smoothness"))
            .on_hover_text(tr("How soft each bend is: 0 bends like a hinge, 1 like a hose"));
        ui.add(egui::Slider::new(&mut deformer.smoothness, 0.0..=1.0));
        ui.end_row();
        for (i, mut bend) in deformer.bends_at(frame).into_iter().enumerate() {
            ui.label(tr_args("Bend {number}", &[("number", &(i + 1).to_string())]));
            if ui.add(DragValue::new(&mut bend).suffix("°").speed(1.0)).changed() {
                deformer.bends[i].set(frame, bend);
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        ui.weak(tr("Drag its joints with the Bone tool (K)."));
        remove = ui.small_button(tr("Remove")).clicked();
    });
    if bones != deformer.bone_count()
        && let Some(bounds) = rest_bounds
    {
        *deformer = Deformer { smoothness: deformer.smoothness, ..Deformer::along(bounds, bones, frame) };
    }
    if remove {
        layer.deformer = None;
    }
    ui.add_space(8.0);
}

/// The free-form deformer: a grid of control points over the drawing that
/// warp it when dragged (see `warp.rs` in the core).
pub(super) fn warp_section(ui: &mut egui::Ui, editor: &mut Editor, id: LayerId) {
    let frame = editor.frame;
    if editor.doc.layer(id).is_none_or(|l| l.instance.is_some())
        || !section(ui, editor, "free-form deformer", tr("Free-Form Deformer"), Icon::Distort)
    {
        return;
    }
    let Some(layer) = editor.doc.layer_mut(id).filter(|l| l.instance.is_none()) else { return };
    let rest_bounds = layer.elements_at(frame).iter().map(|e| e.bounds()).reduce(|a, b| a.union(b));

    let Some(warp) = &mut layer.warp else {
        ui.horizontal(|ui| {
            for size in [3, 4] {
                let add = ui
                    .add_enabled(rest_bounds.is_some(), egui::Button::new(tr_args("Add {size}×{size} Grid", &[("size", &size.to_string())])))
                    .on_hover_text(tr("A grid of points over the drawing: drag them with the Bone tool to squash, bulge and sway it."));
                if add.clicked()
                    && let Some(bounds) = rest_bounds
                {
                    layer.warp = Some(inkframe_core::Warp::over(bounds, size, size, frame));
                }
            }
        });
        ui.add_space(8.0);
        return;
    };
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.weak(tr_args(
            "{columns}×{rows} grid. Drag its points with the Bone tool (K).",
            &[("columns", &warp.columns.to_string()), ("rows", &warp.rows.to_string())],
        ));
        if ui.small_button(tr("Straighten")).on_hover_text(tr("Put every point back on this frame")).clicked()
        {
            for channel in &mut warp.offsets {
                if channel.sample(frame as f32).is_some_and(|o| o != inkframe_core::Point::ZERO)
                    || channel.key_at(frame).is_some()
                {
                    channel.set(frame, inkframe_core::Point::ZERO);
                }
            }
        }
        remove = ui.small_button(tr("Remove")).clicked();
    });
    if remove {
        layer.warp = None;
    }
    ui.add_space(8.0);
}
