//! File › Open Example: the sample projects, with a picture of each.
//!
//! The installers carry the projects from `examples/` (see the xtask's
//! `copy_examples`): in `Inkframe.app/Contents/Resources/Examples` on
//! macOS, and in an `Examples` folder beside `inkframe.exe` on Windows.
//! Running from a checkout, they're found in the repository instead.
//!
//! An example opens as a new, untitled project, so saving asks where to
//! put it. Saving into the app itself would change the installed files
//! (and on macOS break the app's signature).

use crate::i18n::{tr, tr_later};
use std::path::{Path, PathBuf};

use egui::TextureOptions;
use inkframe_core::Document;
use inkframe_render::{RenderOptions, render_frame};

/// The width of each picture, in points.
const PICTURE_WIDTH: f32 = 220.0;

/// What each example shows, by file name, in the order they're listed.
/// Examples not named here are listed after these, without a note.
const ABOUT: [(&str, &str); 5] = [
    (
        "bouncing-ball",
        tr_later("Keyframes, easing and squash and stretch: the getting-started tutorial, finished."),
    ),
    ("rig", tr_later("A character built from parented parts, with lip sync. Pose it with the Bone tool.")),
    (
        "ink-and-paint",
        tr_later("Line art coloured with the paint bucket from palette swatches, Harmony-style."),
    ),
    ("multiplane", tr_later("Scenery at different depths with the camera dollying in. Try the Depth View.")),
    ("particles", tr_later("A campfire of fire, smoke and spark particles, with snow falling.")),
];

/// The folder the examples are in, if there is one.
fn folder() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    let bundled =
        exe_dir.into_iter().flat_map(|dir| [dir.join("Examples"), dir.join("../Resources/Examples")]);
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    // A folder only counts if it has projects in it. Beside a build in
    // `target/release` there's Cargo's own `examples` folder, which a
    // Mac's case-insensitive disk would otherwise take for `Examples`.
    let has_projects = |dir: &PathBuf| {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|e| e == "inkf"))
    };
    bundled.chain([checkout]).find(has_projects)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    pub path: PathBuf,
    /// "Bouncing ball", from the file name.
    pub title: String,
    pub about: Option<&'static str>,
}

/// The examples in `dir`, the described ones first in their order.
fn list(dir: &Path) -> Vec<Example> {
    let mut examples: Vec<Example> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "inkf"))
        .map(|path| {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let about = ABOUT.iter().find(|(name, _)| *name == stem).map(|(_, about)| *about);
            Example { title: title(&stem), about, path }
        })
        .collect();
    let rank = |e: &Example| {
        let stem = e.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        ABOUT.iter().position(|(name, _)| *name == stem).unwrap_or(ABOUT.len())
    };
    examples.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.title.cmp(&b.title)));
    examples
}

/// "ink-and-paint" → "Ink and paint".
fn title(stem: &str) -> String {
    let words = stem.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The Open Example window.
#[derive(Default)]
pub struct ExamplesWindow {
    pub open: bool,
    /// The examples, with their pictures once drawn (`None` if one
    /// couldn't be opened).
    shown: Vec<(Example, Option<egui::TextureHandle>)>,
}

impl ExamplesWindow {
    pub fn open(&mut self, ctx: &egui::Context) {
        self.open = true;
        // Drawn afresh each time, so an example rebuilt meanwhile shows as it is.
        let examples = folder().map(|dir| list(&dir)).unwrap_or_default();
        self.shown = examples
            .into_iter()
            .map(|example| {
                let picture = picture(ctx, &example.path);
                (example, picture)
            })
            .collect();
    }

    /// Shows the window; returns the example chosen to open, if any.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if !self.open {
            return None;
        }
        let mut chosen = None;
        let mut open = self.open;
        let tall = ctx.content_rect().height() * 0.6;
        egui::Window::new(tr("Open Example"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                if self.shown.is_empty() {
                    ui.weak(tr("The examples aren't installed with this copy of Inkframe."));
                    return;
                }
                ui.weak(tr("Each opens as a new project; save it wherever you like."));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(tall).show(ui, |ui| {
                    egui::Grid::new("examples").num_columns(2).spacing([12.0, 12.0]).show(ui, |ui| {
                        for (example, picture) in &self.shown {
                            let clicked = match picture {
                                Some(texture) => {
                                    let size = texture.size_vec2() * (PICTURE_WIDTH / texture.size_vec2().x);
                                    let image =
                                        egui::Image::new(texture).fit_to_exact_size(size).corner_radius(4.0);
                                    ui.add(image.sense(egui::Sense::click()))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                }
                                None => {
                                    ui.weak(tr("Couldn't open"));
                                    false
                                }
                            };
                            ui.vertical(|ui| {
                                ui.set_max_width(280.0);
                                ui.strong(&example.title);
                                if let Some(about) = example.about {
                                    ui.label(tr(about));
                                }
                                if ui.button(tr("Open")).clicked() || clicked {
                                    chosen = Some(example.path.clone());
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
            });
        self.open = open && chosen.is_none();
        chosen
    }
}

/// A small picture of an example, partway through.
fn picture(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let doc = Document::load(path).ok()?;
    let scale = PICTURE_WIDTH * 2.0 / doc.width.max(1) as f32;
    // Partway in: some examples (particles) start from nothing.
    let frame = doc.last_frame() * 2 / 5;
    let pixmap = render_frame(&doc, frame, &RenderOptions { scale, ..RenderOptions::default() })?;
    let image = egui::ColorImage::from_rgba_premultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    );
    Some(ctx.load_texture(format!("example-{}", path.display()), image, TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_in_the_repository_is_described_and_listed_in_order() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let examples = list(&dir);
        assert!(examples.iter().all(|e| e.about.is_some()), "describe new examples in ABOUT: {examples:?}");
        let names: Vec<&str> = examples.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(names, ["Bouncing ball", "Rig", "Ink and paint", "Multiplane", "Particles"]);
    }

    #[test]
    fn an_example_opens_as_an_untitled_copy_named_for_it() {
        let editor = crate::editor::Editor::untitled_copy(Document::default(), "rig".to_owned());
        assert_eq!(editor.display_name(), "rig");
        assert!(editor.path.is_none(), "saving asks where, never into the app");
        assert!(!editor.has_unsaved_changes());
    }

    #[test]
    fn titles_read_as_words() {
        assert_eq!(title("ink-and-paint"), "Ink and paint");
        assert_eq!(title("walk_cycle"), "Walk cycle");
    }
}
