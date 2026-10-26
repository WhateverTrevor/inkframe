//! Translating the interface.
//!
//! Text shown to people is written in English where it's used, wrapped in
//! [`tr`]: `ui.button(tr("Save As…"))`. The English is the key, as gettext
//! does it, so the code reads as it always has, and anything not yet
//! translated shows in English. A translation is a plain text file in
//! `locales/`, one line per text, `English = translation` (see
//! `docs/TRANSLATING.md`); `locales/template.txt`, written by `cargo xtask
//! strings`, lists every text there is to translate.
//!
//! The language is chosen once, at start-up: `INKFRAME_LANGUAGE`, or else
//! the one picked in View › Language, or else the system's (`LANG`), if
//! Inkframe has it. Texts are kept for good once translated, so changing
//! language takes a restart. `pseudo` is a made-up one for
//! checking the interface: every translated text accented and longer, so
//! text not yet wrapped in `tr` stands out, and so does a layout with no
//! room for longer words.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// The translations built into Inkframe: each language's code (as in `LANG`,
/// such as `de` or `pt_BR`) and its file.
const BUILT_IN: &[(&str, &str)] = &[];

/// The language in use, set once at start-up (English if it isn't).
static LANGUAGE: OnceLock<Language> = OnceLock::new();

enum Language {
    English,
    /// English text → translation.
    Translated(HashMap<&'static str, &'static str>),
    /// Made up as asked for, and kept, so each text is made once.
    Pseudo(RwLock<HashMap<&'static str, &'static str>>),
}

/// Chooses the language: `INKFRAME_LANGUAGE`, else `picked` (from View ›
/// Language), else `LANG`. Call once, before anything is shown; later
/// calls do nothing.
pub fn choose_language(picked: Option<&str>) {
    let wanted = std::env::var("INKFRAME_LANGUAGE")
        .ok()
        .or_else(|| picked.map(str::to_owned))
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
    let _ = LANGUAGE.set(language_for(&wanted));
    // The core names undo steps; it translates them with this.
    inkframe_core::i18n::set_translator(tr);
}

fn language_for(wanted: &str) -> Language {
    // "pt_BR.UTF-8" → "pt_BR", then "pt".
    let code = wanted.split(['.', '@']).next().unwrap_or_default();
    if code == "pseudo" {
        return Language::Pseudo(RwLock::default());
    }
    let short = code.split(['_', '-']).next().unwrap_or_default();
    BUILT_IN
        .iter()
        .find(|(c, _)| *c == code)
        .or_else(|| BUILT_IN.iter().find(|(c, _)| *c == short))
        .map_or(Language::English, |(_, file)| Language::Translated(parse(file)))
}

/// The languages there are to pick, by code and by name in their own
/// language (a translation file's first line, `# Deutsch`, names it).
/// English, then the translations, then the pseudo-language for checking
/// layouts.
pub fn languages() -> Vec<(&'static str, &'static str)> {
    let built_in = BUILT_IN.iter().map(|(code, file)| (*code, name_of(code, file)));
    std::iter::once(("en", "English"))
        .chain(built_in)
        .chain(std::iter::once(("pseudo", "Pseudo-language (for checking layouts)")))
        .collect()
}

/// A translation file's name for its language: its first comment line.
fn name_of(code: &'static str, file: &'static str) -> &'static str {
    file.lines().next().and_then(|line| line.strip_prefix('#')).map_or(code, str::trim)
}

/// Reads a translation file: `English = translation` a line, with `#`
/// starting a comment line, and `\n` for a new line within a text. Lines
/// with nothing after the `=` aren't translated yet, so stay English.
pub fn parse(file: &'static str) -> HashMap<&'static str, &'static str> {
    file.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once(" = "))
        .filter(|(_, translation)| !translation.trim().is_empty())
        .map(|(english, translation)| (unescape(english.trim()), unescape(translation.trim())))
        .collect()
}

/// `\n` in a translation file is a new line in the text. Texts with one are
/// rare, so only they're copied (and kept for good, as the texts are).
fn unescape(text: &'static str) -> &'static str {
    if text.contains("\\n") { Box::leak(text.replace("\\n", "\n").into_boxed_str()) } else { text }
}

/// `english` in the language in use.
pub fn tr(english: &'static str) -> &'static str {
    match LANGUAGE.get() {
        None | Some(Language::English) => english,
        Some(Language::Translated(texts)) => texts.get(english).copied().unwrap_or(english),
        Some(Language::Pseudo(made)) => {
            if let Some(text) = made.read().expect("not poisoned").get(english) {
                return text;
            }
            let text: &'static str = Box::leak(pseudo(english).into_boxed_str());
            made.write().expect("not poisoned").insert(english, text);
            text
        }
    }
}

/// Marks `english` as a text to translate where it's shown rather than
/// here, for tables of text kept as constants (the tour's cards): it's
/// listed for translators, and passed through [`tr`] when it's shown.
pub const fn tr_later(english: &'static str) -> &'static str {
    english
}

/// [`tr`] with `{name}`s in the text filled in: `tr_args("Undo {what}",
/// &[("what", label)])`. A translation can put them in any order.
pub fn tr_args(english: &'static str, args: &[(&str, &str)]) -> String {
    args.iter()
        .fold(tr(english).to_owned(), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

/// The pseudo-language: vowels accented, a third longer, in brackets, with
/// `{name}`s left as they are.
fn pseudo(english: &str) -> String {
    let mut out = String::from("[");
    let mut in_name = false;
    for c in english.chars() {
        in_name = match c {
            '{' => true,
            '}' => false,
            _ => in_name,
        };
        out.push(if in_name {
            c
        } else {
            match c {
                'a' => 'á',
                'e' => 'é',
                'i' => 'î',
                'o' => 'ö',
                'u' => 'ü',
                'A' => 'Å',
                'E' => 'É',
                'O' => 'Ø',
                'I' => 'Î',
                'U' => 'Ü',
                c => c,
            }
        });
    }
    // Longer by a third, in short runs so long texts still wrap.
    let longer = english.chars().filter(|c| c.is_alphabetic()).count() / 3;
    for i in 0..longer {
        out.push(if i > 0 && i % 6 == 0 { ' ' } else { '~' });
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_files_are_read_line_by_line() {
        let texts = parse(
            "# Deutsch\nSave = Speichern\nOpen… = Öffnen…\nNot yet = \nTwo lines = Zwei\\nZeilen\nUndo {what} = {what} widerrufen\n",
        );
        assert_eq!(texts.get("Save"), Some(&"Speichern"));
        assert_eq!(texts.get("Open…"), Some(&"Öffnen…"));
        assert_eq!(texts.get("Not yet"), None, "an empty translation stays English");
        assert_eq!(texts.get("Two lines"), Some(&"Zwei\nZeilen"));
        assert_eq!(texts.len(), 4);
    }

    #[test]
    fn names_in_braces_are_filled_in_and_kept_by_the_pseudo_language() {
        // With no language chosen (as in tests), English.
        assert_eq!(tr_args("Undo {what}", &[("what", "Add “Ball”")]), "Undo Add “Ball”");
        let made = pseudo("Undo {what} on {frame}");
        assert!(made.starts_with("[Ündö {what} ön {frame}") && made.ends_with(']'), "{made}");
    }

    #[test]
    fn languages_are_named_by_their_files_first_line() {
        assert_eq!(name_of("de", "# Deutsch\nSave = Speichern\n"), "Deutsch");
        assert_eq!(name_of("xx", "Save = x\n"), "xx");
        let codes: Vec<&str> = languages().iter().map(|(code, _)| *code).collect();
        assert_eq!(codes.first(), Some(&"en"));
        assert_eq!(codes.last(), Some(&"pseudo"));
    }

    #[test]
    fn a_language_is_found_by_its_code_or_its_family() {
        assert!(matches!(language_for("pseudo"), Language::Pseudo(_)));
        // None are built in yet, so everything else is English.
        assert!(matches!(language_for("de_DE.UTF-8"), Language::English));
        assert!(matches!(language_for(""), Language::English));
    }
}
