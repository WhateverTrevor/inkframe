//! `inkframe --reference`: every tool and command as Markdown, for the
//! guidebook's appendices (see `docs/guide`). Written from the same lists
//! the toolbar and Find a Command use, so the book can't fall behind the
//! app: add a tool or a command and the next build of the book has it.

use std::fmt::Write;

use egui::ModifierNames;

use crate::command_finder::entries;
use crate::editor::Tool;
use crate::shortcuts;

/// A table cell's text, with anything that would break the table escaped.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// The reference, as Markdown.
pub fn markdown() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## The tools\n");
    let _ = writeln!(
        out,
        "The toolbar, top to bottom. Tools that share a slot are listed together: press the key for the one showing, Shift with it for the next.\n"
    );
    let _ = writeln!(out, "| Tool | Key | What it does |\n|---|---|---|");
    for group in Tool::GROUPS {
        for (i, tool) in group.iter().enumerate() {
            let key = shortcuts::tool_key(*tool).name();
            let key = if i == 0 { key.to_owned() } else { format!("⇧{key}") };
            let _ = writeln!(out, "| **{}** | {} | {} |", tool.name(), key, cell(tool.description()));
        }
    }
    let _ = writeln!(out, "\n## The commands\n");
    let _ = writeln!(
        out,
        "Every command Find a Command (⌘K) knows, by where it is in the menus. Keys are shown for a Mac; on Windows, Ctrl takes the place of ⌘ and Alt of ⌥.\n"
    );
    let _ = writeln!(out, "| Command | Where | Keys |\n|---|---|---|");
    for e in entries().iter().filter(|e| e.place != "Tools") {
        let keys = e.keys.map_or(String::new(), |k| k.format(&ModifierNames::SYMBOLS, true));
        let _ = writeln!(out, "| {} | {} | {} |", cell(e.label), cell(e.place), keys);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_and_command_is_in_the_reference() {
        let reference = markdown();
        for tool in Tool::all() {
            assert!(reference.contains(&format!("**{}**", tool.name())), "{}", tool.name());
        }
        assert!(reference.contains("| Envelope | Modify › Transform |"));
        assert!(reference.contains("⌘K") || reference.contains("Find a Command"));
    }
}
