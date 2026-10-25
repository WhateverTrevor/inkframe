//! The bar along the top of the stage: what the current tool is and does,
//! and buttons for what can be done right now, as Animate's Properties
//! for a tool and Photoshop's options bar. The point is that nothing needs
//! hunting for in the menus: pick something and its commands are here.
//!
//! On the left, the tool's icon, name and a line on how to use it. Then
//! what fits the moment: Distort, Envelope and Convert to Symbol with a
//! layer selected; combining shapes picked inside a layer; New Skeleton
//! and Rig Setup with the bone tools; Finish and Cancel while a Distort or
//! a smart bone correction is under way. On the right, view switches that
//! are handy at any time: rulers, the perspective guide, onion skins, the
//! light table.

use egui::{Align, Layout, RichText, Sense, Stroke, vec2};
use inkframe_core::{PerspectiveKind, ShapeOp, TimelineId};

use crate::app::Command;
use crate::editor::{Editor, Tool};
use crate::envelope::LatticeKind;
use crate::icons::{Icon, IconButton, LabelledButton};

/// How tall the bar is.
pub const HEIGHT: f32 = 34.0;

/// Shows the bar. Returns the command a button asked for, if any; the
/// switches that are just settings change the editor directly.
pub(crate) fn show(ui: &mut egui::Ui, editor: &mut Editor) -> Option<Command> {
    let mut command = None;
    let frame = egui::Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color));
    frame.show(ui, |ui| {
        ui.set_height(HEIGHT - 6.0);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // The view switches go on the right; laid out first, so the
            // actions on the left get what's left over.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                view_switches(ui, editor, &mut command);
                ui.separator();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    tool_title(ui, editor.tool);
                    ui.separator();
                    actions(ui, editor, &mut command);
                });
            });
        });
    });
    command
}

/// The tool's icon and name, and on hover, how to use it.
fn tool_title(ui: &mut egui::Ui, tool: Tool) {
    let (rect, response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    crate::toolbar::draw_icon(
        ui.painter(),
        tool,
        rect.shrink(1.0),
        Stroke::new(1.5, ui.visuals().strong_text_color()),
    );
    ui.label(RichText::new(tool.name()).strong()).union(response).on_hover_text(tool.description());
}

/// A button for a command, with an icon and a word or two.
fn act(ui: &mut egui::Ui, command: &mut Option<Command>, icon: Icon, label: &str, tip: &str, what: Command) {
    if ui.add(LabelledButton::new(icon, label)).on_hover_text(tip).clicked() {
        *command = Some(what);
    }
}

/// What can be done right now, from what's going on and the tool.
fn actions(ui: &mut egui::Ui, editor: &mut Editor, command: &mut Option<Command>) {
    // Something under way comes first: it wants finishing.
    if let Some(c) = &editor.correcting {
        let layer = editor.doc.layer(c.layer).map_or("the layer".to_owned(), |l| l.name.clone());
        ui.label(format!("Correcting {layer} at {:.0}°: reshape it with any tool", c.turn));
        act(
            ui,
            command,
            Icon::Check,
            "Finish",
            "Key the shapes at this turn (Enter)",
            Command::FinishCorrection,
        );
        act(ui, command, Icon::Cross, "Cancel", "Drop the changes (Esc)", Command::CancelCorrection);
        return;
    }
    // A bone shown at a turn of its own, from its action timeline: the pose
    // on the stage isn't the animation's, so say so.
    if let Some((id, bone, _)) = editor.previewing
        && let Some(turn) = editor.previewed_turn(id, bone)
    {
        let name =
            editor.skeleton(id).and_then(|s| s.bones.get(bone)).map_or_else(String::new, |b| b.name.clone());
        ui.label(format!("Previewing {name} at {turn:.0}°: nothing is keyed"));
        act(
            ui,
            command,
            Icon::Cross,
            "Back to the animation",
            "Show the bone as animated again (Esc)",
            Command::EndPreview,
        );
        return;
    }
    if let Some(lattice) = &editor.lattice {
        ui.label(format!("{}: drag the handles", lattice.kind.name()));
        act(
            ui,
            command,
            Icon::Check,
            "Finish",
            "Keep the shapes as they are now (Enter)",
            Command::FinishLattice,
        );
        return;
    }
    let has_layer = editor.selection.is_some();
    match editor.tool {
        Tool::Select | Tool::Subselect | Tool::Width => {
            if editor.inside_layer().is_some() {
                act(
                    ui,
                    command,
                    Icon::StepOut,
                    "Step out",
                    "Back to working with whole layers (Esc)",
                    Command::ExitLayer,
                );
                let picked = editor.picked_shapes().len();
                if picked >= 2 {
                    combine_menu(ui, command);
                }
            } else if editor.timeline() != TimelineId::Main {
                act(
                    ui,
                    command,
                    Icon::StepOut,
                    "Exit symbol",
                    "Back to the scene the symbol is in (Esc)",
                    Command::ExitSymbol,
                );
            }
            if has_layer {
                act(
                    ui,
                    command,
                    Icon::Symbol,
                    "Symbol",
                    "Convert to Symbol: reuse it, animate inside it (F8)",
                    Command::ConvertToSymbol,
                );
                act(
                    ui,
                    command,
                    Icon::Distort,
                    "Distort",
                    "Corner handles to lean, taper or put it in perspective",
                    Command::StartLattice(LatticeKind::Distort),
                );
                act(
                    ui,
                    command,
                    Icon::Envelope,
                    "Envelope",
                    "Handles to bend and bulge it",
                    Command::StartLattice(LatticeKind::Envelope),
                );
                act(
                    ui,
                    command,
                    Icon::BreakApart,
                    "Break apart",
                    "Text into letters, pencil lines into shapes (⌘B)",
                    Command::BreakApart,
                );
                if editor.shape_tween_here().is_some() {
                    act(
                        ui,
                        command,
                        Icon::Inbetween,
                        "Inbetweens",
                        "Draw the frames between this drawing and the next",
                        Command::MakeInbetweens,
                    );
                }
            } else {
                ui.weak("Click something on the stage to see what you can do with it.");
            }
        }
        Tool::Pen | Tool::Pencil | Tool::Rectangle | Tool::Ellipse => {
            let mut into = editor.draw_into_layer;
            if ui
                .add(LabelledButton::new(Icon::Plus, "Draw into layer").selected(into))
                .on_hover_text(
                    "On: new shapes go into the selected layer. Off: each shape gets a layer of its own",
                )
                .clicked()
            {
                into = !into;
                editor.draw_into_layer = into;
            }
            act(ui, command, Icon::Plus, "New layer", "An empty layer to draw on (⌘⇧N)", Command::NewLayer);
            if editor.tool == Tool::Pen && editor.doc.perspective.is_some() {
                ui.weak("Shift snaps to the perspective guide");
            }
        }
        Tool::Bone | Tool::AddBone => {
            if editor.active_skeleton().is_none() {
                act(
                    ui,
                    command,
                    Icon::Bones,
                    "New skeleton",
                    "Put the selected layers in a skeleton, to rig with bones as in Moho",
                    Command::NewSkeleton,
                );
            } else if editor.tool == Tool::Bone {
                act(
                    ui,
                    command,
                    Icon::Plus,
                    "Add bones",
                    "Draw more bones (Add Bone tool)",
                    Command::UseTool(Tool::AddBone),
                );
            } else {
                act(
                    ui,
                    command,
                    Icon::Bones,
                    "Pose",
                    "Pose the bones (Bone tool)",
                    Command::UseTool(Tool::Bone),
                );
            }
            if editor.active_skeleton().is_some() {
                ui.weak("Pick a bone for its settings in Properties");
            }
        }
        Tool::Camera => {
            if editor.doc.camera_layer().is_none() {
                act(
                    ui,
                    command,
                    Icon::Camera,
                    "Add camera",
                    "A camera to pan, zoom and turn the whole scene",
                    Command::AddCamera,
                );
            }
        }
        tool if tool.paints() || tool.selects() => {
            if editor.paint_target().is_none() {
                act(
                    ui,
                    command,
                    Icon::Plus,
                    "New paint layer",
                    "A layer to paint on",
                    Command::NewPaintLayer,
                );
            }
            ui.weak("Brush size and the like: Properties › Tool");
        }
        _ => {}
    }
}

/// Combine Shapes, as a little menu.
fn combine_menu(ui: &mut egui::Ui, command: &mut Option<Command>) {
    let response = ui
        .add(LabelledButton::new(Icon::Combine, "Combine"))
        .on_hover_text("Merge the picked shapes, or cut one with another");
    egui::Popup::menu(&response).show(|ui| {
        for (label, op) in [
            ("Union", ShapeOp::Union),
            ("Intersect", ShapeOp::Intersect),
            ("Punch", ShapeOp::Punch),
            ("Crop", ShapeOp::Crop),
        ] {
            if ui.button(label).clicked() {
                *command = Some(Command::CombineShapes(op));
            }
        }
    });
}

/// Rulers, the perspective guide, onion skins, the light table, and rig
/// setup when there's a skeleton: settings to flip at any time.
fn view_switches(ui: &mut egui::Ui, editor: &mut Editor, command: &mut Option<Command>) {
    // Right to left, so the last added is leftmost.
    if editor.layers().iter().any(|l| l.skeleton.is_some()) {
        let tip = "Rig setup: skeletons at rest, to draw, paint and rig on (⌥⌘B)";
        if ui.add(IconButton::new(Icon::RigSetup, tip).selected(editor.rig_setup)).clicked() {
            *command = Some(Command::ToggleRigSetup);
        }
    }
    let tip = "Light table: fade every layer but the selected one";
    if ui.add(IconButton::new(Icon::LightTable, tip).selected(editor.light_table)).clicked() {
        editor.light_table = !editor.light_table;
    }
    let tip = "Onion skin: see the frames either side, faintly";
    if ui.add(IconButton::new(Icon::Onion, tip).selected(editor.onion.enabled)).clicked() {
        editor.onion.enabled = !editor.onion.enabled;
    }
    let perspective = editor.doc.perspective.as_ref().map(|g| g.kind);
    let response =
        ui.add(IconButton::new(Icon::Perspective, "Perspective guide").selected(perspective.is_some()));
    egui::Popup::menu(&response).show(|ui| {
        for (label, kind) in [
            ("None", None),
            ("One-Point", Some(PerspectiveKind::OnePoint)),
            ("Two-Point", Some(PerspectiveKind::TwoPoint)),
            ("Three-Point", Some(PerspectiveKind::ThreePoint)),
        ] {
            if ui.add(egui::Button::new(label).selected(perspective == kind)).clicked() {
                *command = Some(Command::SetPerspective(kind));
            }
        }
    });
    if ui
        .add(
            IconButton::new(Icon::Ruler, "Rulers (⌥⇧⌘R): drag guides out of them")
                .selected(editor.show_rulers),
        )
        .clicked()
    {
        *command = Some(Command::ToggleRulers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use inkframe_core::{Document, Point, Transform};

    type State = (Editor, Option<Command>);

    fn harness(editor: Editor) -> Harness<'static, State> {
        let mut h = Harness::new_ui_state(
            |ui, (editor, asked): &mut State| {
                if let Some(command) = show(ui, editor) {
                    *asked = Some(command);
                }
            },
            (editor, None),
        );
        h.set_size(egui::vec2(1400.0, 60.0));
        h.run();
        h
    }

    #[test]
    fn with_a_layer_picked_the_bar_offers_what_to_do_with_it() {
        let mut editor = Editor::new(Document::default());
        let id = editor.doc.add_layer("Box", Vec::new(), 0, Transform::at(Point::new(100.0, 100.0)));
        editor.select(Some(id));
        let mut h = harness(editor);
        h.get_by_label("Distort").click();
        h.run();
        assert_eq!(h.state().1, Some(Command::StartLattice(LatticeKind::Distort)));
    }

    #[test]
    fn with_a_bone_tool_and_no_skeleton_it_offers_to_make_one() {
        let mut editor = Editor::new(Document::default());
        editor.tool = Tool::Bone;
        let mut h = harness(editor);
        h.get_by_label("New skeleton").click();
        h.run();
        assert_eq!(h.state().1, Some(Command::NewSkeleton));
    }

    #[test]
    fn the_onion_skin_switch_flips_it() {
        let mut h = harness(Editor::new(Document::default()));
        assert!(!h.state().0.onion.enabled);
        h.get_by_label("Onion skin: see the frames either side, faintly").click();
        h.run();
        assert!(h.state().0.onion.enabled);
    }
}
