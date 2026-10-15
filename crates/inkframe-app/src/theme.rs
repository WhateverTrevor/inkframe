//! Inkframe's look: the Inter typeface, a palette of near-neutral greys
//! with one blue accent, and roomier spacing than egui's defaults.
//!
//! Colours that the editors draw with themselves (area headers, timeline
//! rows) come from here too, so the whole window reads as one design.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals,
};

/// Around the stage: lighter than the panels, so the stage's edge and the
/// panels' edges both stand out, as Animate's pasteboard does.
pub const PASTEBOARD: Color32 = Color32::from_rgb(0x2f, 0x31, 0x36);
/// The darkest tone: behind the stage, inside text fields and graphs.
pub const SUNKEN: Color32 = Color32::from_rgb(0x17, 0x18, 0x1b);
/// The window and panel background.
pub const PANEL: Color32 = Color32::from_rgb(0x22, 0x23, 0x27);
/// Area headers, the menu bar and alternate rows: a step up from PANEL.
pub const RAISED: Color32 = Color32::from_rgb(0x2b, 0x2c, 0x31);
/// Buttons at rest.
pub const CONTROL: Color32 = Color32::from_rgb(0x34, 0x36, 0x3c);
/// Buttons under the pointer.
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(0x3e, 0x40, 0x47);
/// Lines between areas and around controls.
pub const LINE: Color32 = Color32::from_rgb(0x3a, 0x3c, 0x42);
pub const TEXT: Color32 = Color32::from_rgb(0xe3, 0xe5, 0xea);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(0x8d, 0x91, 0x9b);
/// The accent: selection, the current tool, toggles that are on.
pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0x8b, 0xfd);
/// The accent dimmed, behind selected rows and items.
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x24, 0x4a, 0x80);

/// Applies the fonts and style to the whole app.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.visuals = visuals();
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Heading, FontId::proportional(17.0)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(8.0, 6.0);
        spacing.button_padding = egui::vec2(8.0, 3.0);
        spacing.interact_size.y = 22.0;
        spacing.menu_margin = egui::Margin::same(6);
        spacing.window_margin = egui::Margin::same(12);
    });
}

/// Inter for the interface, with egui's own fonts behind it for the
/// symbols and scripts Inter doesn't have.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_owned(),
        FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.otf")).into(),
    );
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "Inter".to_owned());
    }
    fonts
}

fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = None;
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = RAISED;
    visuals.extreme_bg_color = SUNKEN;
    visuals.code_bg_color = SUNKEN;
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.weak_text_color = Some(TEXT_WEAK);

    let radius = CornerRadius::same(4);
    let widgets = &mut visuals.widgets;
    // Labels and panels.
    widgets.noninteractive.bg_fill = PANEL;
    widgets.noninteractive.weak_bg_fill = PANEL;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.noninteractive.corner_radius = radius;
    // Buttons at rest: filled, borderless.
    widgets.inactive.bg_fill = CONTROL;
    widgets.inactive.weak_bg_fill = CONTROL;
    widgets.inactive.bg_stroke = Stroke::NONE;
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.inactive.corner_radius = radius;
    widgets.hovered.bg_fill = CONTROL_HOVER;
    widgets.hovered.weak_bg_fill = CONTROL_HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, LINE);
    widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;
    widgets.active.bg_fill = ACCENT_DIM;
    widgets.active.weak_bg_fill = ACCENT_DIM;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;
    widgets.open = widgets.active;
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_comes_first_with_egui_fonts_behind_it() {
        let fonts = fonts();
        let family = &fonts.families[&FontFamily::Proportional];
        assert_eq!(family[0], "Inter");
        assert!(family.len() > 1, "fallbacks kept for symbols");
    }

    #[test]
    fn text_stands_out_from_every_background() {
        // A rough contrast check: text is far brighter than any surface.
        let luma = |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
        for background in [SUNKEN, PANEL, RAISED, CONTROL, CONTROL_HOVER] {
            assert!(luma(TEXT) - luma(background) > 150.0);
            assert!(luma(TEXT_WEAK) - luma(background) > 60.0);
        }
    }
}
