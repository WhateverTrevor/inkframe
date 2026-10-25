# Inkframe for Artists: the guidebook

A beginner's guide to Inkframe for artists, in the style of a Packt
technical book: a chapter per group of tools, each with numbered steps,
screenshots of the real app, Tip and Important note boxes and a summary.
It's written here as Markdown and built into a PDF.

## Building it

```sh
cargo xtask guide
```

This builds the app, takes every screenshot the book uses, and prints the
book with Google Chrome to `target/guide/Inkframe-for-Artists.pdf`. It's
printed twice: Chrome can't number the contents itself, so the build reads
back which page each heading landed on and prints it again with the page
numbers in. It
takes about a minute, most of it screenshots; when only the words have
changed, `cargo xtask guide --keep-shots` reuses the last ones and takes
seconds. Set `CHROME` to use a Chrome or Chromium somewhere other than
its usual place.

## What's here

| | |
|---|---|
| `chapters/` | The book, a Markdown file a chapter, in file-name order. `00-…` is front matter (unnumbered), `91-…` and on are appendices (lettered), and the rest are numbered chapters. |
| `figures.txt` | The screenshots: a name, a project to open, and the app's screenshot hooks that set the scene (see `crates/inkframe-app/src/screenshot.rs`). |
| `style.css` | The look: page size, chapter openers, figures, boxes and tables. |

The tables of every tool and command in the appendices aren't written
here: the app prints them (`inkframe --reference`), so they always match
it.

## Adding to it

When a tool or feature lands, it usually needs:

1. **Words**, in the chapter it belongs to, or a new chapter file between
   the others (numbered files are renumbered automatically).
2. **A screenshot**, if it shows something new: add a line to
   `figures.txt`, adding a screenshot hook to the app if no existing one
   sets up the scene, and show it in the chapter with
   `![Caption](name)`.
3. Nothing for the appendices: a new tool or Find a Command entry appears
   there by itself.

### The Markdown

Besides headings, paragraphs, **bold**, *italics*, `code`, links, lists
(one level of nesting) and tables:

- `![Caption](name)` on a line of its own is a numbered figure.
  `{crop=x,y,w,h}` after it shows part of the screenshot, in fractions of
  its size, and `{callouts=1@x,y 2@x,y}` puts numbered markers on it, in
  fractions of what's shown. Figures are kept under 3.5 inches tall.
- `{{fig:name}}` in the text becomes that figure's number, *Figure 2.3*,
  so references stay right as figures come and go.
- `> Tip: …` and `> Important note: …` are boxed asides.
- `[[⌘K]]` is a key to press.
- `{{reference:tools}}` and `{{reference:commands}}` are the generated
  tables.

Each chapter follows the same shape: a short introduction, "In this
chapter, we're going to cover the following main topics:" with the
sections as a list, Technical requirements, the sections, and a Summary
that leads into the next chapter.
