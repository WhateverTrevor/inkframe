# Translating Inkframe

Inkframe's whole interface can be translated, including the names it
gives new layers and undo steps.
A translation is one plain text file; no programming is needed.

## Translating

1. Copy `locales/template.txt` to `locales/<code>.txt`, named for the
   language's code: `de.txt` for German, `fr.txt` for French, `pt_BR.txt`
   for Brazilian Portuguese, `ja.txt` for Japanese.
2. Add a first line saying which language it is, as a comment:
   `# Deutsch`.
3. After each line's ` = `, write the translation:

   ```
   Save As… = Speichern unter…
   Undo {what} = {what} widerrufen
   ```

   - Keep `{names}` in braces exactly as they are: Inkframe fills them
     in. They can move anywhere in the sentence.
   - `\n` is a new line, where the English has one.
   - A line left empty after ` = ` stays in English, so a translation can
     be done a little at a time.
   - Shortcuts (`⌘K`, `F6`) and the names of other programs (Animate,
     Rhubarb) are usually best left as they are.
4. Add the file to the list in `crates/inkframe-app/src/i18n.rs`
   (`BUILT_IN`), so it's built into Inkframe:

   ```rust
   const BUILT_IN: &[(&str, &str)] = &[("de", include_str!("../../../locales/de.txt"))];
   ```

Inkframe uses the system's language when it has a translation for it, or
the one picked in View › Language (from the next time it opens). To try
one without changing that, start Inkframe with `INKFRAME_LANGUAGE` set:

```sh
INKFRAME_LANGUAGE=de target/release/inkframe
```

## For people writing Inkframe's code

Text that people see is written in English where it's used, wrapped so it
can be translated:

| | |
|---|---|
| `tr("Save As…")` | A text |
| `tr_args("Undo {what}", &[("what", label)])` | A text with something filled in. Never build sentences from pieces with `format!`: word order differs between languages |
| `tr_later("Welcome")` | A text kept in a constant table and translated where it's shown, with `tr(text)` |

The core and the exporter keep the names of things in English, as those
are also data (saved in files, used by scripts): `inkframe_core::i18n::tr_later`
marks them, and the app shows them with `tr(mode.label())`. Text the core
writes for people, the names of undo steps, uses `inkframe_core::i18n::tr`,
which translates with the app's `tr` once the app has started.

After adding or changing texts, run `cargo xtask strings`, which lists
them all in `locales/template.txt` for translators; a test fails if it's
out of date. Another test checks every translation file is of texts the
app still has.

To see what's been wrapped and what hasn't, start Inkframe in the
pseudo-language:

```sh
INKFRAME_LANGUAGE=pseudo target/release/inkframe
```

Every wrapped text shows accented, a third longer and in brackets,
`[Sávé Ås…~~]`, so text still in plain English hasn't been wrapped, and
a cramped layout shows where longer translations won't fit.

### What's wrapped

- The menu bar, its menus and their tips
- The tools' names and tips, and the toolbar
- The context bar above the stage
- The timeline: its buttons, tips and right-click menus
- Properties, with its search
- Find a Command
- The Library, the panels (Swatches, Align, Controllers, History…) and
  the dialogs (export, batch export, lip sync, scripts, presets…)
- Messages: errors, and what the stage says when it's empty
- The window's areas: their names, the area menu and its groups
- The names of things: workspaces, blend modes, easings, filters,
  adjustments, particle presets, export formats and the like
- The names new layers and folders are given, and undo steps' names
- The first-run tour
- What's New's window (its notes come from the changelog, which isn't
  translated)

Left in English on purpose: the command line, the screenshot hooks, and
the guidebook's appendix, which is written in English.

The long lists of words in the template (`name visible hidden show…`)
are what Properties' search looks for in each section: translate them as
words someone might search by. Searching finds the English words too, as
does Find a Command.

Names that are also data (a tool's or an area's, which the screenshot
hooks and the guidebook's appendix use in English) keep an English
`name()` marked with `tr_later`, and a `label()` that translates it for
showing.
