//! Translation, for the little text the core writes for people to read
//! (the names of undo steps) and the names the editor shows.
//!
//! The core has no translations of its own: the editor hands it its
//! translating function at start-up ([`set_translator`]), and until then,
//! as in tests and the command line, everything is English. Names that are
//! also data (blend modes, easings, export formats…) stay English in the
//! core and are only marked, with [`tr_later`], for the editor to translate
//! where it shows them.

use std::sync::OnceLock;

static TRANSLATOR: OnceLock<fn(&'static str) -> &'static str> = OnceLock::new();

/// Sets how text is translated. Call once, at start-up; later calls do
/// nothing.
pub fn set_translator(translate: fn(&'static str) -> &'static str) {
    let _ = TRANSLATOR.set(translate);
}

/// `english` in the language in use.
pub fn tr(english: &'static str) -> &'static str {
    TRANSLATOR.get().map_or(english, |translate| translate(english))
}

/// [`tr`] with `{name}`s filled in, in whatever order the translation has
/// them.
pub fn tr_args(english: &'static str, args: &[(&str, &str)]) -> String {
    args.iter()
        .fold(tr(english).to_owned(), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

/// Marks a name the editor shows translated. The core keeps names in
/// English, as they're also data (saved, scripted, matched); the editor
/// passes them through its `tr` where it shows them, and `cargo xtask
/// strings` lists every name marked so for translators.
pub const fn tr_later(english: &'static str) -> &'static str {
    english
}
