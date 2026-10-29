//! What's New: after an update, a window with what changed in this
//! version, taken from the changelog built into the app.
//!
//! The changelog (`CHANGELOG.md`) is the one place the changes are written
//! down, for the release notes too, so the window reads this version's
//! section of it rather than keeping words of its own: headings become
//! headings, bullets bullets, and `**bold**` and backticks are dropped. A
//! build whose version has no section yet (one between releases) shows
//! nothing.

use egui::RichText;

use crate::i18n::{tr, tr_args};
use crate::theme;

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// One part of a version's notes.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// What the version is about, before its first heading.
    Intro(String),
    Heading(String),
    Bullet(String),
}

/// The notes for `version` from the changelog, or `None` if it has none.
pub fn notes(version: &str) -> Option<Vec<Part>> {
    notes_in(CHANGELOG, version)
}

fn notes_in(changelog: &str, version: &str) -> Option<Vec<Part>> {
    let heading = format!("## [{version}]");
    let start = changelog.lines().position(|line| line.trim() == heading)? + 1;
    let section: Vec<&str> =
        changelog.lines().skip(start).take_while(|line| !line.trim().starts_with("## ")).collect();
    let parts = parts_of(&section.join("\n"));
    (!parts.is_empty()).then_some(parts)
}

/// A version's notes, written as the changelog writes them (as its release
/// notes are too), in parts to show.
pub fn parts_of(notes: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut paragraph = String::new();
    let flush = |paragraph: &mut String, parts: &mut Vec<Part>, bullet: bool| {
        let text = plain(paragraph.trim());
        if !text.is_empty() {
            parts.push(if bullet { Part::Bullet(text) } else { Part::Intro(text) });
        }
        paragraph.clear();
    };
    let mut in_bullet = false;
    for line in notes.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("### ").or_else(|| trimmed.strip_prefix("## ")) {
            flush(&mut paragraph, &mut parts, in_bullet);
            parts.push(Part::Heading(plain(title)));
            in_bullet = false;
        } else if let Some(rest) = trimmed.strip_prefix("- ") {
            flush(&mut paragraph, &mut parts, in_bullet);
            paragraph.push_str(rest);
            in_bullet = true;
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut parts, in_bullet);
            in_bullet = false;
        } else {
            paragraph.push(' ');
            paragraph.push_str(trimmed);
        }
    }
    flush(&mut paragraph, &mut parts, in_bullet);
    parts
}

/// Shows notes' parts: the intro, headings, and bullets with a hanging
/// indent.
pub fn show_parts(ui: &mut egui::Ui, parts: &[Part]) {
    for part in parts {
        match part {
            Part::Intro(text) => {
                ui.label(RichText::new(text).color(theme::TEXT));
            }
            Part::Heading(text) => {
                ui.add_space(8.0);
                ui.label(RichText::new(text).strong().size(14.0).color(theme::TEXT));
            }
            Part::Bullet(text) => {
                // A hanging indent: wrapped lines line up with the text.
                ui.horizontal_top(|ui| {
                    ui.label("•");
                    ui.add(egui::Label::new(text.as_str()).wrap());
                });
            }
        }
    }
}

/// Markdown's marks taken out: bold, code and links' brackets.
fn plain(text: &str) -> String {
    let text = text.replace("**", "").replace('`', "");
    // [words](address) → words
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|c| open + c) else { break };
        let Some(end) = rest[close..].find(')').map(|e| close + e) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

#[derive(Default)]
pub struct WhatsNew {
    /// The notes showing, with the version they're for.
    showing: Option<(String, Vec<Part>)>,
}

impl WhatsNew {
    /// Opens the window for this version, if the changelog has notes for it.
    /// Returns whether it opened.
    pub fn open(&mut self) -> bool {
        let version = env!("CARGO_PKG_VERSION");
        self.showing = notes(version).map(|parts| (version.to_owned(), parts));
        self.showing.is_some()
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let Some((version, parts)) = &self.showing else { return };
        let mut open = true;
        let mut close = false;
        egui::Window::new(tr_args("What's New in Inkframe {version}", &[("version", version)]))
            .id(egui::Id::new("whats-new"))
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.6)
                    .show(ui, |ui| show_parts(ui, parts));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(tr("Got it")).fill(theme::ACCENT_DIM)).clicked() {
                        close = true;
                    }
                    if ui.button(tr("Read the guidebook")).clicked() {
                        crate::app::open_in_browser(crate::app::GUIDEBOOK_URL);
                    }
                });
            });
        if !open || close {
            self.showing = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Changelog\n\n## Unreleased\n\n- Not yet.\n\n## [0.5.0]\n\nCopying frames,\nand **more**.\n\n### Timeline\n\n- Cut, Copy and Paste Frames: picked\n  frames' `keys`, as [Animate](https://x)'s.\n- Two.\n\n### Speed\n\n- Quicker.\n\n## [0.4.0]\n\n- Older.\n";

    #[test]
    fn a_versions_notes_come_from_its_changelog_section() {
        let parts = notes_in(SAMPLE, "0.5.0").unwrap();
        assert_eq!(
            parts,
            [
                Part::Intro("Copying frames, and more.".into()),
                Part::Heading("Timeline".into()),
                Part::Bullet("Cut, Copy and Paste Frames: picked frames' keys, as Animate's.".into()),
                Part::Bullet("Two.".into()),
                Part::Heading("Speed".into()),
                Part::Bullet("Quicker.".into()),
            ]
        );
        assert!(notes_in(SAMPLE, "9.9.9").is_none(), "a version with no notes shows nothing");
    }

    #[test]
    fn the_built_in_changelog_has_notes_for_released_versions() {
        assert!(notes("0.4.0").is_some_and(|parts| parts.len() > 3));
    }
}
