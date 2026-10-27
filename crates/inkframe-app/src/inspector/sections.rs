//! The Properties panel's collapsible sections and searching them: each
//! section has words people might type, so a search shows just the sections
//! that match.

use crate::editor::Editor;
use crate::i18n::{tr, tr_later};
use crate::icons::Icon;

/// The search box at the top of the layer tab: typing in it shows only the
/// sections with matching settings (see `searched_for`).
pub(super) fn search_box(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
        crate::icons::paint(ui.painter(), Icon::Search, rect, crate::theme::TEXT_WEAK);
        let searching = !editor.properties_search.is_empty();
        let width = ui.available_width() - if searching { 26.0 } else { 0.0 };
        let field = ui.add(
            egui::TextEdit::singleline(&mut editor.properties_search)
                .hint_text(tr("Search settings"))
                .desired_width(width),
        );
        // Escape clears it, as in most search boxes.
        if field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            editor.properties_search.clear();
        }
        if searching && ui.small_button("×").on_hover_text(tr("Clear the search")).clicked() {
            editor.properties_search.clear();
        }
    });
    ui.add_space(2.0);
}

/// A section of the properties panel, under a title row: an arrow, an
/// icon, small spaced capitals and a hairline running on to the panel's
/// edge, so a long panel is easy to scan for the part you want. Clicking
/// the row folds the section away, or opens it again. Returns whether it's
/// open: a folded section shows nothing more.
///
/// What's folded is kept by `key` (a title can change, as the transform's
/// does with the frame), with the person's settings for next time.
pub(super) fn section(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    key: &str,
    title: impl Into<String>,
    icon: Icon,
) -> bool {
    let title = title.into();
    // While searching, a section is shown, open, only if it has what's
    // looked for.
    let searching = !editor.properties_search.trim().is_empty();
    if !searched_for(editor, key, &title) {
        return false;
    }
    let open = searching || !editor.folded_sections.contains(key);
    ui.add_space(8.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::click());
    // Named for screen readers (and tests), as egui's own folding headers are.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, &title));
    let colour = if response.hovered() { crate::theme::TEXT } else { crate::theme::TEXT_WEAK };
    let painter = ui.painter();
    let middle = rect.center().y;
    // The arrow: down when open, right when folded.
    let (x, s) = (rect.left() + 5.0, 3.5);
    let arrow = if open {
        vec![
            egui::pos2(x - s, middle - s / 2.0),
            egui::pos2(x, middle + s / 2.0),
            egui::pos2(x + s, middle - s / 2.0),
        ]
    } else {
        vec![
            egui::pos2(x - s / 2.0, middle - s),
            egui::pos2(x + s / 2.0, middle),
            egui::pos2(x - s / 2.0, middle + s),
        ]
    };
    painter.add(egui::Shape::line(arrow, egui::Stroke::new(1.4, colour)));
    crate::icons::paint(
        painter,
        icon,
        egui::Rect::from_center_size(egui::pos2(rect.left() + 21.0, middle), egui::vec2(13.0, 13.0)),
        colour,
    );
    let galley = egui::WidgetText::from(section_text(&title).color(colour)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        rect.width() - 40.0,
        egui::TextStyle::Body,
    );
    let text_at = egui::pos2(rect.left() + 33.0, middle - galley.size().y / 2.0);
    let text_end = text_at.x + galley.size().x;
    ui.painter().galley(text_at, galley, colour);
    if text_end + 8.0 < rect.right() {
        ui.painter().hline(
            text_end + 8.0..=rect.right(),
            middle,
            (1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if open {
        tr("Fold this section away")
    } else {
        tr("Show this section")
    });
    if response.clicked() {
        if open {
            editor.folded_sections.insert(key.to_owned());
        } else {
            editor.folded_sections.remove(key);
        }
    }
    // Folded or opened this frame, it's drawn as it now is from the next.
    open
}

/// What each section of the layer tab holds, by its key, for the search
/// box: the words someone might look for a setting by.
pub(super) const SECTION_WORDS: &[(&str, &str)] = &[
    (
        "layer",
        tr_later(
            "name visible hidden show hide locked lock guide mask frames start end span blend mode multiply screen overlay lines texture pencil charcoal chalk grain web button click link",
        ),
    ),
    (
        "transform",
        tr_later(
            "position move x y rotation rotate turn angle scale size skew slant opacity alpha fade transparency depth z distance orient path straighten motion masked follow guide",
        ),
    ),
    (
        "parent and pivot",
        tr_later("parent attach rig child pivot centre center origin bends bend limits joint elbow knee"),
    ),
    ("easing", tr_later("easing ease in out linear hold curve timing tween speed slow overshoot anticipate")),
    (
        "skeleton",
        tr_later(
            "bone bones strength turn stretch keep area squash limits target ik dynamics spring damping swing delete",
        ),
    ),
    ("bones", tr_later("follows follow bind binding limb nearby region pin pins pinned")),
    (
        "smart bone",
        tr_later("smart action actions timeline reshape switch correction correct morph preview keys"),
    ),
    ("instance", tr_later("symbol instance loop play once single frame first length swap duplicate")),
    (
        "particles",
        tr_later(
            "particles particle emitter rate gravity wind drag wander turbulence speed spread direction lifetime size colour color spin seed preset sparks snow smoke fire",
        ),
    ),
    (
        "effects",
        tr_later(
            "effects effect colour color brightness tint advanced filters filter drop shadow blur glow adjust hue saturation contrast motion shutter",
        ),
    ),
    ("deformer", tr_later("deformer bend bends bones softness soft rubber hose")),
    ("free-form deformer", tr_later("free form freeform deformer grid warp points straighten bulge")),
    (
        "drawings",
        tr_later(
            "drawings drawing new duplicate delete swap substitution shape tween hints mouth lip sync flip",
        ),
    ),
    (
        "shapes",
        tr_later(
            "shapes shape fill stroke outline gradient linear radial colour color line width dash dashed dotted ends corners cap join brush art pattern text font size align spacing image picture actual size",
        ),
    ),
];

/// Whether the section `key` (titled `title`) has what the properties
/// panel's search box asks for: every word typed, somewhere in its title
/// or the words it holds. With nothing typed, every section does. Counts
/// the matches, so the panel can say when there are none.
pub(super) fn searched_for(editor: &mut Editor, key: &str, title: &str) -> bool {
    let query = editor.properties_search.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let words = SECTION_WORDS.iter().find(|(k, _)| *k == key).map_or("", |(_, w)| *w);
    // The words in English as well as translated, so either finds it.
    let haystack = format!("{} {key} {words} {}", title.to_lowercase(), tr(words).to_lowercase());
    let found = query.split_whitespace().all(|term| haystack.contains(term));
    if found {
        editor.properties_matches += 1;
    }
    found
}

/// A section title's text, for titles that share their row with controls.
pub fn section_text(title: impl Into<String>) -> egui::RichText {
    egui::RichText::new(title.into().to_uppercase())
        .size(11.0)
        .extra_letter_spacing(0.6)
        .color(crate::theme::TEXT_WEAK)
}
