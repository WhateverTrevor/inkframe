//! `cargo xtask strings`: lists every text in the app that can be
//! translated, for translators, in `locales/template.txt`.
//!
//! A text is translatable where the app's code wraps it in `tr("…")`,
//! `tr_args("…", …)` or `tr_later("…")` (see `i18n.rs`). This reads the
//! code for those, in file order, and writes each text once, as a line
//! `English = ` for a translation to go after. A test checks the file is up
//! to date, so a text added without running this is noticed.

use std::path::{Path, PathBuf};

const HEADER: &str = "\
# Every text in Inkframe's interface that can be translated, written by
# `cargo xtask strings`. To translate Inkframe, copy this file to
# locales/<language code>.txt (de.txt, pt_BR.txt…) and write each text's
# translation after its \" = \". Keep {names} in braces as they are: they're
# filled in. \\n is a new line. See docs/TRANSLATING.md.
";

/// Where the template goes.
pub fn template_path(root: &Path) -> PathBuf {
    root.join("locales/template.txt")
}

/// Writes the template.
pub fn write_template() -> Result<(), String> {
    let root = crate::workspace_root();
    let path = template_path(&root);
    std::fs::create_dir_all(path.parent().expect("in a folder"))
        .map_err(|e| format!("couldn't create {}: {e}", path.display()))?;
    let texts = collect(&root.join("crates/inkframe-app/src"))?;
    crate::util::write(&path, template(&texts).as_bytes())?;
    println!("Wrote {} texts to {}", texts.len(), path.display());
    Ok(())
}

pub fn template(texts: &[String]) -> String {
    let mut out = HEADER.to_owned();
    for text in texts {
        out.push_str(&escape(text));
        out.push_str(" = \n");
    }
    out
}

/// Every translatable text in the Rust files under `dir`, in file order,
/// each once.
pub fn collect(dir: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    rust_files(dir, &mut files)?;
    files.sort();
    let mut texts: Vec<String> = Vec::new();
    for file in files {
        let source =
            std::fs::read_to_string(&file).map_err(|e| format!("couldn't read {}: {e}", file.display()))?;
        for text in texts_in(&source) {
            if !texts.contains(&text) {
                texts.push(text);
            }
        }
    }
    Ok(texts)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("couldn't read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The texts in one file: the string literal opening each `tr(`,
/// `tr_args(` or `tr_later(`, as Rust reads it (escapes and line
/// continuations worked out). Calls not on a literal are skipped; the
/// `tr_later` marks are how those texts are found.
pub fn texts_in(source: &str) -> Vec<String> {
    let mut texts = Vec::new();
    let mut rest = source;
    while let Some(at) = ["tr(", "tr_args(", "tr_later("]
        .iter()
        .filter_map(|call| find_call(rest, call).map(|i| (i, call.len())))
        .min()
    {
        let after = rest[at.0 + at.1..].trim_start();
        rest = &rest[at.0 + at.1..];
        if let Some(text) = after.strip_prefix('"').and_then(literal) {
            texts.push(text);
        }
    }
    texts
}

/// Where `call` starts in `text` as a call of its own (not the end of a
/// longer name, such as `attr(` for `tr(`).
fn find_call(text: &str, call: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find(call).map(|i| from + i) {
        let before = text[..i].chars().next_back();
        if !before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            return Some(i);
        }
        from = i + call.len();
    }
    None
}

/// A string literal's text, from just after its opening quote.
fn literal(source: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = source.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                // A line continuation: the newline and the next line's
                // leading spaces aren't in the text.
                '\n' => {
                    let skipped: String = chars.clone().take_while(|c| c.is_whitespace()).collect();
                    for _ in skipped.chars() {
                        chars.next();
                    }
                }
                other => {
                    out.push('\\');
                    out.push(other);
                }
            },
            c => out.push(c),
        }
    }
    None
}

/// A text as a translation file writes it: new lines as `\n`.
fn escape(text: &str) -> String {
    text.replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_found_in_calls_as_rust_reads_them() {
        let source = r#"
            ui.button(tr("Save As…"));
            let x = attr("not this");
            tr_args("Undo {what}", &[("what", l)]);
            title: tr_later("Long \
                     line"),
            ui.weak(tr("Two\nlines with \"quotes\""));
            tr(name); // not a literal: skipped
        "#;
        assert_eq!(texts_in(source), ["Save As…", "Undo {what}", "Long line", "Two\nlines with \"quotes\""]);
        assert!(template(&["Two\nlines".into()]).ends_with("Two\\nlines = \n"));
    }

    /// The template lists exactly what the code wraps: run `cargo xtask
    /// strings` after adding or changing texts.
    #[test]
    fn the_template_is_up_to_date() {
        let root = crate::workspace_root();
        let texts = collect(&root.join("crates/inkframe-app/src")).unwrap();
        // Windows checkouts may turn its line endings into \r\n.
        let on_disk = std::fs::read_to_string(template_path(&root)).unwrap_or_default().replace("\r\n", "\n");
        assert!(
            on_disk == template(&texts),
            "locales/template.txt is out of date: run `cargo xtask strings`"
        );
    }

    /// Every translation translates texts the app still has.
    #[test]
    fn translations_are_of_texts_the_app_has() {
        let root = crate::workspace_root();
        let texts: Vec<String> =
            collect(&root.join("crates/inkframe-app/src")).unwrap().iter().map(|t| escape(t)).collect();
        for entry in std::fs::read_dir(root.join("locales")).unwrap().flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == "template.txt") {
                continue;
            }
            let file = std::fs::read_to_string(&path).unwrap();
            let stale: Vec<&str> = file
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .filter_map(|l| l.split_once(" = ").map(|(english, _)| english.trim()))
                .filter(|english| !texts.iter().any(|t| t == english))
                .collect();
            assert!(stale.is_empty(), "{} translates texts the app no longer has: {stale:?}", path.display());
        }
    }
}
