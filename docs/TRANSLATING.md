# Translating Inkframe

Inkframe's interface can be translated: nearly all of it now (see "What's
wrapped so far" below), and the rest as it's converted.
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

Inkframe uses the system's language when it has a translation for it. To
try one regardless, start Inkframe with `INKFRAME_LANGUAGE` set:

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

### What's wrapped so far

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
- The first-run tour
- What's New's window (its notes come from the changelog, which isn't
  translated)

Still to do: the names that come from elsewhere in the code (workspaces,
adjustments, particle presets, perspective kinds, easings, blend modes,
filters), the names new layers are given, and the names of undo steps.

The long lists of words in the template (`name visible hidden show…`)
are what Properties' search looks for in each section: translate them as
words someone might search by. Searching finds the English words too, as
does Find a Command.

Names that are also data (a tool's or an area's, which the screenshot
hooks and the guidebook's appendix use in English) keep an English
`name()` marked with `tr_later`, and a `label()` that translates it for
showing.
