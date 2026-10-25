//! `cargo xtask guide`: builds the beginner's guidebook, *Inkframe for
//! Artists*, as a PDF in `target/guide/`.
//!
//! The book is written as Markdown chapters in `docs/guide/chapters/`, one
//! file each, in file-name order, so a chapter is added by adding a file.
//! Its figures are screenshots of the app itself, taken fresh on every
//! build through its screenshot hooks (`docs/guide/figures.txt` lists
//! them), so they keep up with the app. The appendices' tables of every
//! tool and command come from `inkframe --reference`, so they keep up too.
//!
//! The Markdown is turned into one HTML page here, by a small converter
//! that knows the few things the book needs (headings, lists, tables,
//! figures, Tip and Important note boxes, keys), and Google Chrome prints
//! that page to PDF: it lays out pages properly, and is already on most
//! machines, so nothing else needs installing. Set `CHROME` to use another
//! copy of Chrome or Chromium.
//!
//! The Markdown, beyond the usual:
//!
//! * `![Caption](figure)` on a line of its own is a numbered figure, by its
//!   name in `figures.txt`. After it, `{crop=x,y,w,h}` shows part of the
//!   screenshot (fractions of it), and `{callouts=1@x,y 2@x,y}` puts
//!   numbered markers on it (fractions of what's shown).
//! * `> Tip: …` and `> Important note: …` are boxed asides.
//! * `{{fig:name}}` in the words becomes that figure's number ("Figure 2.3").
//! * `[[⌘K]]` is a key to press.
//! * `{{reference:tools}}` and `{{reference:commands}}` are the generated
//!   tables.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{build_release, output, read, write};

pub struct Options {
    /// Reuse the screenshots from the last build rather than taking them
    /// again (much quicker when only the words have changed).
    pub keep_shots: bool,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Options { keep_shots: false };
        for arg in args {
            match arg.as_str() {
                "--keep-shots" => options.keep_shots = true,
                other => return Err(format!("unknown option {other:?}\n\n{}", crate::USAGE)),
            }
        }
        Ok(options)
    }
}

pub fn build(options: &Options) -> Result<(), String> {
    let root = crate::workspace_root();
    let source = root.join("docs/guide");
    let out = root.join("target/guide");
    let images = out.join("images");
    std::fs::create_dir_all(&images).map_err(|e| format!("couldn't create {}: {e}", images.display()))?;

    build_release(&root)?;
    let app = root.join("target/release").join(if cfg!(windows) { "inkframe.exe" } else { "inkframe" });

    let figures = read_figures(&source.join("figures.txt"))?;
    for figure in &figures {
        let shot = images.join(format!("{}.png", figure.name));
        if options.keep_shots && shot.exists() {
            continue;
        }
        println!("Taking {}…", figure.name);
        let mut command = Command::new(&app);
        command.current_dir(&root).env("INKFRAME_SCREENSHOT", &shot);
        for (key, value) in &figure.env {
            command.env(key, value);
        }
        if let Some(project) = &figure.project {
            command.arg(root.join(project));
        }
        output(&mut command)?;
    }
    std::fs::copy(root.join("assets/icon/inkframe-1024.png"), images.join("icon.png"))
        .map_err(|e| format!("couldn't copy the icon: {e}"))?;

    let reference = output(Command::new(&app).arg("--reference"))?;
    let (tools, commands) = reference.split_once("## The commands").unwrap_or((&reference, ""));
    let commands = format!("## The commands{commands}");

    let mut chapters: Vec<PathBuf> = std::fs::read_dir(source.join("chapters"))
        .map_err(|e| format!("couldn't read the chapters: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "md"))
        .collect();
    chapters.sort();

    let mut book = Book::new(&images, tools.to_owned(), commands);
    for chapter in &chapters {
        book.chapter(&read(chapter)?, chapter)?;
    }
    let css = read(&source.join("style.css"))?;
    let html_path = out.join("guide.html");
    write(&html_path, book.html(&css).as_bytes())?;

    let pdf = out.join("Inkframe-for-Artists.pdf");
    let chrome = std::env::var("CHROME").map(PathBuf::from).unwrap_or_else(|_| default_chrome());
    println!("Printing the PDF…");
    output(
        Command::new(&chrome)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-pdf-header-footer")
            .arg("--allow-file-access-from-files")
            .arg("--virtual-time-budget=30000")
            .arg(format!("--print-to-pdf={}", pdf.display()))
            .arg(format!("file://{}", html_path.display())),
    )
    .map_err(|e| {
        format!("{e}\n(Chrome prints the book: set CHROME to where it is, if not {})", chrome.display())
    })?;
    println!("Wrote {}", pdf.display());
    Ok(())
}

fn default_chrome() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
    } else if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    } else {
        PathBuf::from("google-chrome")
    }
}

/// A screenshot to take: its name, a project to open, and the screenshot
/// hook settings that set the scene.
struct Figure {
    name: String,
    project: Option<String>,
    env: Vec<(String, String)>,
}

/// `figures.txt`: one figure a line, `name | project | KEY=value …`, with
/// `-` for no project and `#` starting a comment.
fn read_figures(path: &Path) -> Result<Vec<Figure>, String> {
    let text = read(path)?;
    let mut figures = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        let [name, project, settings] = parts[..] else {
            return Err(format!("{}: expected `name | project | settings` in {line:?}", path.display()));
        };
        let env = settings_words(settings)
            .iter()
            .filter_map(|pair| pair.split_once('='))
            .map(|(k, v)| (format!("INKFRAME_SCREENSHOT_{k}"), v.trim_matches('"').to_owned()))
            .collect();
        let project = (project != "-").then(|| project.to_owned());
        figures.push(Figure { name: name.to_owned(), project, env });
    }
    Ok(figures)
}

/// Splits settings at spaces, except inside quotes (`TOOL="Add Bone"`).
fn settings_words(settings: &str) -> Vec<String> {
    let mut words = vec![String::new()];
    let mut quoted = false;
    for c in settings.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                words.last_mut().expect("one word at least").push(c);
            }
            c if c.is_whitespace() && !quoted => words.push(String::new()),
            c => words.last_mut().expect("one word at least").push(c),
        }
    }
    words.retain(|w| !w.is_empty());
    words
}

/// A chapter's line in the table of contents: its label ("Chapter 3",
/// "Preface"), title and anchor, and its sections' titles and anchors.
struct Contents {
    label: String,
    title: String,
    anchor: String,
    sections: Vec<(String, String)>,
}

/// The book being put together: the chapters' HTML, and what the table
/// of contents needs.
struct Book<'a> {
    images: &'a Path,
    tools: String,
    commands: String,
    body: String,
    contents: Vec<Contents>,
    numbered: usize,
    appendices: usize,
}

impl<'a> Book<'a> {
    fn new(images: &'a Path, tools: String, commands: String) -> Self {
        Self {
            images,
            tools,
            commands,
            body: String::new(),
            contents: Vec::new(),
            numbered: 0,
            appendices: 0,
        }
    }

    /// Adds a chapter. Its file name says what kind: one starting `00` is
    /// front matter (unnumbered), one starting `9` an appendix (lettered),
    /// any other a numbered chapter.
    fn chapter(&mut self, markdown: &str, path: &Path) -> Result<(), String> {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (label, figure_prefix) = if stem.starts_with("00") {
            (String::new(), "P".to_owned())
        } else if stem.starts_with('9') {
            self.appendices += 1;
            let letter = (b'A' + (self.appendices - 1) as u8) as char;
            (format!("Appendix {letter}"), letter.to_string())
        } else {
            self.numbered += 1;
            (format!("Chapter {}", self.numbered), self.numbered.to_string())
        };
        let mut chapter = Chapter {
            book: self,
            label,
            figure_prefix,
            figures: 0,
            title: String::new(),
            anchor: String::new(),
            sections: Vec::new(),
            html: String::new(),
        };
        chapter.render(markdown)?;
        let Chapter { label, title, anchor, sections, html, .. } = chapter;
        self.contents.push(Contents { label, title, anchor, sections });
        self.body.push_str(&html);
        Ok(())
    }

    fn html(&self, css: &str) -> String {
        let version = env!("CARGO_PKG_VERSION");
        let mut contents = String::new();
        for Contents { label, title, anchor, sections } in &self.contents {
            let _ = write!(
                contents,
                r##"<li class="toc-chapter"><a href="#{anchor}"><span class="toc-label">{label}</span> {title}</a>"##
            );
            if !sections.is_empty() {
                contents.push_str("<ul>");
                for (section, id) in sections {
                    let _ = write!(contents, r##"<li><a href="#{id}">{section}</a></li>"##);
                }
                contents.push_str("</ul>");
            }
            contents.push_str("</li>");
        }
        format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Inkframe for Artists</title>
<style>{css}</style>
</head>
<body>
<section class="cover">
  <div class="cover-band"></div>
  <img class="cover-icon" src="images/icon.png" alt="">
  <h1 class="cover-title">Inkframe for Artists</h1>
  <p class="cover-subtitle">A beginner's guide to drawing, painting, rigging and animating</p>
  <p class="cover-edition">For Inkframe {version}</p>
</section>
<section class="contents">
  <h1 class="contents-title">Contents</h1>
  <ol class="toc">{contents}</ol>
</section>
{body}
</body>
</html>
"#,
            body = self.body
        )
    }
}

/// The tallest a figure may be, in inches: a little under half a page.
const MAX_FIGURE_HEIGHT: f32 = 3.5;

/// One chapter being rendered.
struct Chapter<'b, 'a> {
    book: &'b Book<'a>,
    label: String,
    figure_prefix: String,
    figures: usize,
    title: String,
    anchor: String,
    sections: Vec<(String, String)>,
    html: String,
}

impl Chapter<'_, '_> {
    fn render(&mut self, markdown: &str) -> Result<(), String> {
        let mut markdown = markdown
            .replace("{{reference:tools}}", &self.book.tools.clone())
            .replace("{{reference:commands}}", &self.book.commands.clone());
        // Figures are numbered in order, so the words can name one by its
        // screenshot (`{{fig:workspace}}`) and stay right as figures come
        // and go.
        let names: Vec<String> = markdown
            .lines()
            .filter_map(|line| line.trim().strip_prefix("!["))
            .filter_map(|line| line.split_once("](").and_then(|(_, rest)| rest.split_once(')')))
            .map(|(name, _)| name.to_owned())
            .collect();
        for (n, name) in names.iter().enumerate() {
            markdown = markdown.replace(
                &format!("{{{{fig:{name}}}}}"),
                &format!("*Figure {}.{}*", self.figure_prefix, n + 1),
            );
        }
        if let Some(at) = markdown.find("{{fig:") {
            let name = markdown[at..].split("}}").next().unwrap_or_default();
            return Err(format!("{name}}}}} names a figure this chapter doesn't have"));
        }
        let lines: Vec<&str> = markdown.lines().collect();
        let mut i = 0;
        let mut paragraph: Vec<&str> = Vec::new();
        let flush = |paragraph: &mut Vec<&str>, html: &mut String| {
            if !paragraph.is_empty() {
                let _ = writeln!(html, "<p>{}</p>", inline(&paragraph.join(" ")));
                paragraph.clear();
            }
        };
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("<!--") {
                flush(&mut paragraph, &mut self.html);
                i += 1;
                continue;
            }
            if let Some(title) = trimmed.strip_prefix("# ") {
                flush(&mut paragraph, &mut self.html);
                self.title = inline(title);
                self.anchor = slug(title);
                let label = if self.label.is_empty() {
                    String::new()
                } else {
                    format!(r#"<div class="chapter-label">{}</div>"#, self.label)
                };
                let _ = writeln!(
                    self.html,
                    r#"<section class="chapter" id="{}">{label}<h1>{}</h1>"#,
                    self.anchor, self.title
                );
            } else if let Some(title) = trimmed.strip_prefix("## ") {
                flush(&mut paragraph, &mut self.html);
                let id = format!("{}-{}", self.anchor, slug(title));
                let _ = writeln!(self.html, r#"<h2 id="{id}">{}</h2>"#, inline(title));
                self.sections.push((inline(title), id));
            } else if let Some(title) = trimmed.strip_prefix("### ") {
                flush(&mut paragraph, &mut self.html);
                let _ = writeln!(self.html, "<h3>{}</h3>", inline(title));
            } else if trimmed.starts_with("![") {
                flush(&mut paragraph, &mut self.html);
                self.figure(trimmed)?;
            } else if trimmed.starts_with('>') {
                flush(&mut paragraph, &mut self.html);
                let mut quote = Vec::new();
                while i < lines.len() && lines[i].trim().starts_with('>') {
                    quote.push(lines[i].trim().trim_start_matches('>').trim());
                    i += 1;
                }
                self.aside(&quote.join(" "));
                continue;
            } else if trimmed.starts_with('|') {
                flush(&mut paragraph, &mut self.html);
                let mut rows = Vec::new();
                while i < lines.len() && lines[i].trim().starts_with('|') {
                    rows.push(lines[i].trim());
                    i += 1;
                }
                self.table(&rows);
                continue;
            } else if is_item(trimmed) {
                flush(&mut paragraph, &mut self.html);
                let start = i;
                while i < lines.len()
                    && (is_item(lines[i].trim())
                        || (lines[i].starts_with("  ") && !lines[i].trim().is_empty()))
                {
                    i += 1;
                }
                self.list(&lines[start..i]);
                continue;
            } else {
                paragraph.push(trimmed);
            }
            i += 1;
        }
        flush(&mut paragraph, &mut self.html);
        self.html.push_str("</section>\n");
        Ok(())
    }

    /// `![Caption](name)` with an optional `{crop=…}` and `{callouts=…}`.
    fn figure(&mut self, line: &str) -> Result<(), String> {
        let caption_end = line.find("](").ok_or_else(|| format!("a figure without a name: {line}"))?;
        let caption = &line[2..caption_end];
        let rest = &line[caption_end + 2..];
        let name_end = rest.find(')').ok_or_else(|| format!("a figure without a closing bracket: {line}"))?;
        let name = &rest[..name_end];
        let options = &rest[name_end + 1..];
        let option = |key: &str| {
            options.find(&format!("{{{key}=")).and_then(|start| {
                let from = start + key.len() + 2;
                options[from..].find('}').map(|end| options[from..from + end].to_owned())
            })
        };
        let path = self.book.images.join(format!("{name}.png"));
        let (width, height) = png_size(&path)
            .ok_or_else(|| format!("no screenshot {} (is it in figures.txt?)", path.display()))?;
        let crop: Vec<f32> = option("crop")
            .map(|c| c.split(',').filter_map(|v| v.trim().parse().ok()).collect())
            .filter(|c: &Vec<f32>| c.len() == 4)
            .unwrap_or_else(|| vec![0.0, 0.0, 1.0, 1.0]);
        let (x, y, w, h) = (crop[0], crop[1], crop[2], crop[3]);
        // The window shows `w` of the picture's width and `h` of its height;
        // the picture is scaled up so that part fills it.
        let ratio = (w * width as f32) / (h * height as f32);
        // However wide it is, a figure is at most this tall, so a tall crop
        // doesn't fill a page.
        let max_width = MAX_FIGURE_HEIGHT * ratio;
        let mut markers = String::new();
        for callout in option("callouts").unwrap_or_default().split_whitespace() {
            let Some((number, at)) = callout.split_once('@') else { continue };
            let Some((cx, cy)) = at.split_once(',') else { continue };
            let _ = write!(
                markers,
                r#"<span class="callout" style="left:{}%;top:{}%">{number}</span>"#,
                cx.parse::<f32>().unwrap_or(0.0) * 100.0,
                cy.parse::<f32>().unwrap_or(0.0) * 100.0
            );
        }
        self.figures += 1;
        let number = format!("{}.{}", self.figure_prefix, self.figures);
        let _ = writeln!(
            self.html,
            r#"<figure><div class="shot" style="aspect-ratio:{ratio};width:min(100%,{max_width}in)"><img src="images/{name}.png" alt="" style="width:{}%;left:{}%;top:{}%">{markers}</div><figcaption><b>Figure {number}</b> – {}</figcaption></figure>"#,
            100.0 / w,
            -x / w * 100.0,
            -y / h * 100.0,
            inline(caption)
        );
        Ok(())
    }

    /// A Tip or an Important note.
    fn aside(&mut self, text: &str) {
        let (class, label, text) = if let Some(rest) = text.strip_prefix("Tip:") {
            ("tip", "Tip", rest)
        } else if let Some(rest) = text.strip_prefix("Important note:") {
            ("note", "Important note", rest)
        } else {
            ("quote", "", text)
        };
        let label = if label.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="aside-label">{label}</div>"#)
        };
        let _ =
            writeln!(self.html, r#"<div class="aside {class}">{label}<p>{}</p></div>"#, inline(text.trim()));
    }

    fn table(&mut self, rows: &[&str]) {
        let cells = |row: &str| -> Vec<String> {
            // Split on bars, but not escaped ones.
            let row = row.trim().trim_start_matches('|').trim_end_matches('|');
            let mut cells = vec![String::new()];
            let mut chars = row.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '\\' if chars.peek() == Some(&'|') => {
                        cells.last_mut().expect("one cell at least").push('|');
                        chars.next();
                    }
                    '|' => cells.push(String::new()),
                    c => cells.last_mut().expect("one cell at least").push(c),
                }
            }
            cells.into_iter().map(|c| c.trim().to_owned()).collect()
        };
        self.html.push_str("<table>");
        for (n, row) in rows.iter().enumerate() {
            if n == 1 && row.contains("---") {
                continue;
            }
            // The heading row goes in a <thead>, which Chrome repeats at
            // the top of each page a long table runs onto.
            let tag = if n == 0 { "th" } else { "td" };
            self.html.push_str(if n == 0 { "<thead><tr>" } else { "<tr>" });
            for cell in cells(row) {
                let _ = write!(self.html, "<{tag}>{}</{tag}>", inline(&cell));
            }
            self.html.push_str(if n == 0 { "</tr></thead>" } else { "</tr>" });
        }
        self.html.push_str("</table>\n");
    }

    /// A list, numbered or not, with items continued on indented lines
    /// and one level of indented items inside.
    fn list(&mut self, lines: &[&str]) {
        let ordered = lines.first().is_some_and(|l| l.trim().starts_with(|c: char| c.is_ascii_digit()));
        let tag = if ordered { "ol" } else { "ul" };
        let mut items: Vec<(String, Vec<String>)> = Vec::new();
        for line in lines {
            let indented = line.starts_with("  ");
            let trimmed = line.trim();
            if indented && is_item(trimmed) && !items.is_empty() {
                items.last_mut().expect("checked").1.push(item_text(trimmed).to_owned());
            } else if is_item(trimmed) && !indented {
                items.push((item_text(trimmed).to_owned(), Vec::new()));
            } else if let Some(last) = items.last_mut() {
                match last.1.last_mut() {
                    Some(inner) => {
                        inner.push(' ');
                        inner.push_str(trimmed);
                    }
                    None => {
                        last.0.push(' ');
                        last.0.push_str(trimmed);
                    }
                }
            }
        }
        let _ = write!(self.html, "<{tag}>");
        for (text, inner) in items {
            let _ = write!(self.html, "<li>{}", inline(&text));
            if !inner.is_empty() {
                self.html.push_str("<ul>");
                for item in inner {
                    let _ = write!(self.html, "<li>{}</li>", inline(&item));
                }
                self.html.push_str("</ul>");
            }
            self.html.push_str("</li>");
        }
        let _ = writeln!(self.html, "</{tag}>");
    }
}

fn is_item(line: &str) -> bool {
    line.starts_with("- ") || line.starts_with("* ") || ordered_marker(line).is_some()
}

/// The length of a `1. ` marker at the start of `line`, if it has one.
fn ordered_marker(line: &str) -> Option<usize> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && line[digits..].starts_with(". ")).then_some(digits + 2)
}

fn item_text(line: &str) -> &str {
    match ordered_marker(line) {
        Some(n) => &line[n..],
        None => &line[2..],
    }
}

/// An anchor for a heading: its words, lower case, joined by hyphens.
fn slug(text: &str) -> String {
    let words: Vec<String> =
        text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
    words.join("-")
}

/// Inline Markdown: code, keys, bold, italics and links, with everything
/// else escaped for HTML.
fn inline(text: &str) -> String {
    let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut out = String::new();
    // Code first, so nothing inside it is touched.
    for (n, part) in escaped.split('`').enumerate() {
        if n % 2 == 1 {
            let _ = write!(out, "<code>{part}</code>");
        } else {
            out.push_str(&spans(part));
        }
    }
    out
}

fn spans(text: &str) -> String {
    let mut text = text.to_owned();
    text = pairs(&text, "[[", "]]", "<kbd>", "</kbd>");
    text = pairs(&text, "**", "**", "<b>", "</b>");
    text = pairs(&text, "*", "*", "<i>", "</i>");
    // Links: [words](address).
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|c| open + c) else { break };
        let Some(end) = rest[close..].find(')').map(|e| close + e) else { break };
        out.push_str(&rest[..open]);
        let _ = write!(out, r#"<a href="{}">{}</a>"#, &rest[close + 2..end], &rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Replaces each `open … close` pair in `text` with the tags.
fn pairs(text: &str, open: &str, close: &str, start: &str, end: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(a) = rest.find(open) {
        // What's inside is at least a character, so `[[]]]` is the ] key
        // and `**` alone isn't empty bold.
        let inside = a + open.len();
        let Some(first) = rest[inside..].chars().next() else { break };
        let Some(b) = rest[inside + first.len_utf8()..].find(close).map(|b| inside + first.len_utf8() + b)
        else {
            break;
        };
        out.push_str(&rest[..a]);
        let _ = write!(out, "{start}{}{end}", &rest[a + open.len()..b]);
        rest = &rest[b + close.len()..];
    }
    out.push_str(rest);
    out
}

/// A PNG's width and height, from its header.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    let be = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    Some((be(16)?, be(20)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_markdown_becomes_html() {
        assert_eq!(
            inline("Press [[⌘K]] for **Find a Command**"),
            "Press <kbd>⌘K</kbd> for <b>Find a Command</b>"
        );
        assert_eq!(inline("an *idea* in `code <b>`"), "an <i>idea</i> in <code>code &lt;b&gt;</code>");
        assert_eq!(inline("see [the site](https://x.org)"), r#"see <a href="https://x.org">the site</a>"#);
        assert_eq!(inline("[[[]] and [[]]]"), "<kbd>[</kbd> and <kbd>]</kbd>");
    }

    #[test]
    fn settings_can_quote_spaces() {
        assert_eq!(settings_words(r#"SKELETON=1  TOOL="Add Bone""#), ["SKELETON=1", r#"TOOL="Add Bone""#]);
    }

    #[test]
    fn headings_make_anchors() {
        assert_eq!(slug("Meet Inkframe: the workspace"), "meet-inkframe-the-workspace");
    }
}
