use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use inkframe_core::{Document, Point, Transform};

use super::*;

#[test]
fn searching_shows_just_the_sections_that_have_it() {
    let mut editor = Editor::new(Document::default());
    let id = editor.doc.add_layer("Ball", Vec::new(), 0, Transform::default());
    editor.select(Some(id));
    // Folded, but a search finds it all the same.
    editor.folded_sections.insert("effects".into());
    editor.properties_search = "blur".into();
    let mut h = Harness::new_ui_state(
        |ui, editor: &mut Editor| {
            egui::ScrollArea::vertical().show(ui, |ui| layer_tab(ui, editor));
        },
        editor,
    );
    h.set_size(vec2(360.0, 3000.0));
    h.run();
    assert!(h.query_by_label("Effects").is_some());
    assert!(h.query_by_label("Colour Effect").is_some(), "open while searching");
    assert!(h.query_by_label("Position").is_none());
    assert!(h.query_by_label("Easing").is_none());
    assert!(h.query_by_label("Blend").is_none(), "the layer's own settings hide too");

    h.state_mut().properties_search = "zzz".into();
    h.run();
    assert!(h.query_by_label_contains("No settings of this layer match").is_some());

    // Cleared, everything's back, folded sections folded.
    h.state_mut().properties_search.clear();
    h.run();
    assert!(h.query_by_label("Position").is_some() && h.query_by_label("Blend").is_some());
    assert!(h.query_by_label("Colour Effect").is_none());
}

#[test]
fn a_section_folds_away_and_opens_again() {
    let mut editor = Editor::new(Document::default());
    let id = editor.doc.add_layer("Ball", Vec::new(), 0, Transform::default());
    editor.select(Some(id));
    let mut h = Harness::new_ui_state(
        |ui, editor: &mut Editor| {
            egui::ScrollArea::vertical().show(ui, |ui| layer_tab(ui, editor));
        },
        editor,
    );
    h.set_size(vec2(360.0, 3000.0));
    h.run();
    assert!(h.query_by_label("Position").is_some());
    h.get_by_label("Transform at frame 1").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Position").is_none(), "folded away");
    assert!(h.state().folded_sections.contains("transform"));
    h.get_by_label("Transform at frame 1").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Position").is_some(), "open again");
    assert!(h.state().folded_sections.is_empty());
}

#[test]
fn with_several_layers_selected_effects_can_be_given_to_them_all() {
    let mut editor = Editor::new(Document::default());
    let ids: Vec<LayerId> = (0..3)
        .map(|i| {
            editor.doc.add_layer(format!("L{i}"), Vec::new(), 0, Transform::at(Point::new(i as f32, 0.0)))
        })
        .collect();
    editor.select_many(&ids);
    let last = *ids.last().unwrap();
    let mut fx = editor.doc.layer(last).unwrap().track.effects_at(0.0);
    fx.color = inkframe_core::ColorEffect::Brightness { amount: 0.4 };
    editor.doc.layer_mut(last).unwrap().track.set_effects(0, fx.clone());

    let mut h = Harness::new_ui_state(
        |ui, editor: &mut Editor| {
            egui::ScrollArea::vertical().show(ui, |ui| layer_tab(ui, editor));
        },
        editor,
    );
    h.set_size(vec2(360.0, 3000.0));
    h.run();
    assert!(h.query_by_label("3 layers selected").is_some());
    h.get_by_label("Apply to the other 2 selected layers").click();
    h.run();
    for &id in &ids {
        assert_eq!(h.state().doc.layer(id).unwrap().track.effects_at(0.0), fx);
    }
}
