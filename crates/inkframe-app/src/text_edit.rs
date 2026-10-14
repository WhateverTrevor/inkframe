//! Typing into text on the stage: the cursor, selection and keyboard rules.
//!
//! Positions are byte offsets into the text's content, always on a
//! character boundary. They're mapped to and from the stage with the
//! renderer's layout, which knows where every character landed (see
//! `inkframe_render::TextLayout`). Drawing and mouse handling are in
//! `stage.rs`; this file is the plain logic, so it can be tested without a
//! window.
//!
//! The keys follow each platform's conventions: Option+arrow moves by word
//! on a Mac (Ctrl+arrow elsewhere), ⌘+arrow to the start or end of a line.

use std::ops::Range;

use egui::{Event, Key, Modifiers};
use inkframe_core::{Point, Rect};
use inkframe_render::TextLayout;

/// Where the text cursor is, and the other end of the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextCursor {
    /// Where typing goes.
    pub cursor: usize,
    /// Where the selection started. Equal to `cursor` when nothing's selected.
    pub anchor: usize,
}

/// What a key or other input asks of whoever is editing.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// Stop editing (Escape).
    pub finish: bool,
    /// Put this on the clipboard (copy or cut).
    pub copy: Option<String>,
}

impl TextCursor {
    pub fn at(position: usize) -> Self {
        Self { cursor: position, anchor: position }
    }

    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    /// Moves the cursor, keeping the anchor if `extend` (Shift held), so the
    /// selection grows or shrinks; otherwise collapsing it.
    pub fn move_to(&mut self, position: usize, extend: bool) {
        self.cursor = position;
        if !extend {
            self.anchor = position;
        }
    }

    /// Replaces the selection with `inserted`.
    fn replace(&mut self, content: &mut String, inserted: &str) {
        let selection = self.selection();
        content.replace_range(selection.clone(), inserted);
        *self = Self::at(selection.start + inserted.len());
    }

    /// Applies one input event to `content`. `layout` must be the layout of
    /// `content` as it is now, for moving up and down between lines.
    pub fn handle(&mut self, content: &mut String, layout: &TextLayout, event: &Event) -> Outcome {
        let mut outcome = Outcome::default();
        match event {
            Event::Text(typed) => {
                // Typed text never contains control characters worth keeping;
                // Enter and Tab arrive as keys and are handled below.
                let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
                if !typed.is_empty() {
                    self.replace(content, &typed);
                }
            }
            Event::Paste(pasted) => {
                // Text pasted from elsewhere may use Windows line endings.
                self.replace(content, &pasted.replace("\r\n", "\n"));
            }
            Event::Copy | Event::Cut => {
                let selection = self.selection();
                if !selection.is_empty() {
                    outcome.copy = Some(content[selection].to_owned());
                    if matches!(event, Event::Cut) {
                        self.replace(content, "");
                    }
                }
            }
            Event::Key { key, pressed: true, modifiers, .. } => {
                outcome.finish = self.handle_key(content, layout, *key, *modifiers);
            }
            _ => {}
        }
        outcome
    }

    /// Returns whether the key ends editing.
    fn handle_key(
        &mut self,
        content: &mut String,
        layout: &TextLayout,
        key: Key,
        modifiers: Modifiers,
    ) -> bool {
        let extend = modifiers.shift;
        // Word-wise movement and deletion: Option on a Mac, Ctrl elsewhere.
        let by_word = if cfg!(target_os = "macos") { modifiers.alt } else { modifiers.ctrl };
        // ⌘ on a Mac jumps to the start or end of the line.
        let to_line_end = modifiers.mac_cmd;
        let selection = self.selection();
        match key {
            Key::Escape => return true,
            Key::Enter => self.replace(content, "\n"),
            Key::A if modifiers.command => *self = Self { anchor: 0, cursor: content.len() },
            Key::Backspace => {
                if selection.is_empty() {
                    let start = if by_word {
                        word_start(content, self.cursor)
                    } else {
                        previous(content, self.cursor)
                    };
                    self.anchor = start;
                }
                self.replace(content, "");
            }
            Key::Delete => {
                if selection.is_empty() {
                    let end =
                        if by_word { word_end(content, self.cursor) } else { next(content, self.cursor) };
                    self.anchor = end;
                }
                self.replace(content, "");
            }
            Key::ArrowLeft | Key::ArrowRight => {
                let left = key == Key::ArrowLeft;
                let position = if to_line_end {
                    let line = line_of(layout, self.cursor);
                    if left { line.start } else { line.end }
                } else if by_word {
                    if left { word_start(content, self.cursor) } else { word_end(content, self.cursor) }
                } else if !selection.is_empty() && !extend {
                    // With a selection, an arrow goes to the selection's edge
                    // rather than one character past where the cursor is.
                    if left { selection.start } else { selection.end }
                } else if left {
                    previous(content, self.cursor)
                } else {
                    next(content, self.cursor)
                };
                self.move_to(position, extend);
            }
            Key::ArrowUp | Key::ArrowDown => {
                let down = key == Key::ArrowDown;
                let position =
                    vertical(layout, self.cursor, down).unwrap_or(if down { content.len() } else { 0 });
                self.move_to(position, extend);
            }
            Key::Home | Key::End => {
                let line = line_of(layout, self.cursor);
                self.move_to(if key == Key::Home { line.start } else { line.end }, extend);
            }
            _ => {}
        }
        false
    }
}

fn previous(content: &str, position: usize) -> usize {
    content[..position].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next(content: &str, position: usize) -> usize {
    content[position..].chars().next().map_or(position, |c| position + c.len_utf8())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// The start of the word before `position`, skipping any spaces first, the
/// way Option+Left behaves in Mac text fields.
fn word_start(content: &str, position: usize) -> usize {
    let before = &content[..position];
    let trimmed = before.trim_end_matches(|c: char| !is_word(c));
    trimmed.trim_end_matches(is_word).len()
}

/// The end of the word after `position`, skipping any spaces first.
fn word_end(content: &str, position: usize) -> usize {
    let after = &content[position..];
    let skipped = after.len() - after.trim_start_matches(|c: char| !is_word(c)).len();
    let word = after[skipped..].len() - after[skipped..].trim_start_matches(is_word).len();
    position + skipped + word
}

/// The whole word around `position`, for double-click selection.
pub fn word_at(content: &str, position: usize) -> Range<usize> {
    let start = content[..position].trim_end_matches(is_word).len();
    let end = position + content[position..].len() - content[position..].trim_start_matches(is_word).len();
    start..end
}

/// The range of the line containing `position`.
fn line_of(layout: &TextLayout, position: usize) -> Range<usize> {
    layout
        .lines
        .iter()
        .find(|line| line.range.start <= position && position <= line.range.end)
        .map_or(position..position, |line| line.range.clone())
}

/// The position on the line above or below with the nearest x, or `None`
/// at the first or last line.
fn vertical(layout: &TextLayout, position: usize, down: bool) -> Option<usize> {
    let index = layout.lines.iter().position(|l| l.range.start <= position && position <= l.range.end)?;
    let (x, _) = caret_position(layout, position);
    let target = if down { layout.lines.get(index + 1)? } else { layout.lines.get(index.checked_sub(1)?)? };
    nearest_caret(&target.carets, x)
}

fn nearest_caret(carets: &[(usize, f32)], x: f32) -> Option<usize> {
    carets.iter().min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs())).map(|c| c.0)
}

/// The cursor position nearest a point in the text's local coordinates,
/// for clicking to place the cursor.
pub fn position_at(layout: &TextLayout, point: Point) -> usize {
    // The line whose band (half a line either side of its middle) holds the
    // point; above the first line or below the last picks those lines.
    let middle = |baseline: f32| baseline + (layout.descent - layout.ascent) / 2.0;
    let line = layout
        .lines
        .iter()
        .min_by(|a, b| (middle(a.baseline) - point.y).abs().total_cmp(&(middle(b.baseline) - point.y).abs()));
    line.and_then(|line| nearest_caret(&line.carets, point.x)).unwrap_or(0)
}

/// Where to draw the cursor for `position`: its x and the line's baseline.
pub fn caret_position(layout: &TextLayout, position: usize) -> (f32, f32) {
    for line in &layout.lines {
        if let Some(&(_, x)) = line.carets.iter().find(|(byte, _)| *byte == position) {
            return (x, line.baseline);
        }
    }
    (0.0, 0.0)
}

/// The boxes to highlight for a selection, one per line it touches, in the
/// text's local coordinates.
pub fn selection_boxes(layout: &TextLayout, selection: Range<usize>) -> Vec<Rect> {
    if selection.is_empty() {
        return Vec::new();
    }
    layout
        .lines
        .iter()
        .filter_map(|line| {
            let xs: Vec<f32> = line
                .carets
                .iter()
                .filter(|(byte, _)| selection.contains(byte) || *byte == selection.end)
                .map(|(_, x)| *x)
                .collect();
            let (left, right) =
                xs.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), x| (l.min(*x), r.max(*x)));
            // A selection that runs past this line's end includes its
            // newline; show that with a little extra width, as editors do.
            let newline = if selection.contains(&line.range.end) && line.range.end < selection.end {
                layout.ascent * 0.3
            } else {
                0.0
            };
            (left <= right).then(|| {
                Rect::from_points(
                    Point::new(left, line.baseline - layout.ascent),
                    Point::new(right + newline, line.baseline + layout.descent),
                )
            })
        })
        .filter(|r| r.width() > 0.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::Text;

    fn layout(content: &str) -> std::sync::Arc<TextLayout> {
        let mut text = Text::new(content, 100.0);
        text.font.family = "Ubuntu".into(); // bundled, so tests don't depend on installed fonts
        inkframe_render::layout_text(&text)
    }

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// Applies events to `content` with the cursor at `cursor`, laying the
    /// text out afresh before each, as the stage does.
    fn type_into(content: &str, cursor: TextCursor, events: &[Event]) -> (String, TextCursor) {
        let (mut content, mut cursor) = (content.to_owned(), cursor);
        for event in events {
            let layout = layout(&content);
            cursor.handle(&mut content, &layout, event);
        }
        (content, cursor)
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_replaces_the_selection() {
        let (content, cursor) = type_into("Hllo", TextCursor::at(1), &[Event::Text("e".into())]);
        assert_eq!((content.as_str(), cursor), ("Hello", TextCursor::at(2)));

        let selected = TextCursor { anchor: 0, cursor: 5 };
        let (content, _) = type_into("Hello world", selected, &[Event::Text("Goodbye".into())]);
        assert_eq!(content, "Goodbye world");
    }

    #[test]
    fn backspace_and_delete_remove_one_character_or_the_selection() {
        let (content, cursor) =
            type_into("Hello", TextCursor::at(5), &[key(Key::Backspace, Modifiers::NONE)]);
        assert_eq!((content.as_str(), cursor.cursor), ("Hell", 4));
        let (content, _) = type_into("Hello", TextCursor::at(0), &[key(Key::Delete, Modifiers::NONE)]);
        assert_eq!(content, "ello");
        let (content, _) =
            type_into("Hello", TextCursor { anchor: 1, cursor: 4 }, &[key(Key::Backspace, Modifiers::NONE)]);
        assert_eq!(content, "Ho");
        // At the very start there's nothing to delete.
        let (content, _) = type_into("Hi", TextCursor::at(0), &[key(Key::Backspace, Modifiers::NONE)]);
        assert_eq!(content, "Hi");
    }

    #[test]
    fn accented_and_emoji_characters_are_deleted_whole() {
        let (content, _) = type_into("café", TextCursor::at(5), &[key(Key::Backspace, Modifiers::NONE)]);
        assert_eq!(content, "caf");
        let (content, cursor) = type_into("a😀", TextCursor::at(1), &[key(Key::ArrowRight, Modifiers::NONE)]);
        assert_eq!((content.as_str(), cursor.cursor), ("a😀", 5));
    }

    #[test]
    fn shift_arrows_select_and_plain_arrows_collapse_to_the_edge() {
        let (_, cursor) = type_into(
            "Hello",
            TextCursor::at(1),
            &[key(Key::ArrowRight, Modifiers::SHIFT), key(Key::ArrowRight, Modifiers::SHIFT)],
        );
        assert_eq!(cursor.selection(), 1..3);
        let (_, cursor) = type_into("Hello", cursor, &[key(Key::ArrowLeft, Modifiers::NONE)]);
        assert_eq!(cursor, TextCursor::at(1));
    }

    #[test]
    fn words_are_skipped_whole() {
        assert_eq!(word_start("one two  three", 14), 9);
        assert_eq!(word_start("one two  ", 9), 4);
        assert_eq!(word_end("one two", 0), 3);
        assert_eq!(word_end("one two", 3), 7);
        assert_eq!(word_at("one two three", 5), 4..7);
    }

    #[test]
    fn enter_starts_a_new_line_and_up_down_move_between_lines() {
        let (content, cursor) = type_into("ab", TextCursor::at(1), &[key(Key::Enter, Modifiers::NONE)]);
        assert_eq!((content.as_str(), cursor.cursor), ("a\nb", 2));
        // From the end of a long line down to a short one lands at its end;
        // from the start of the second line up lands at the start of the first.
        let (_, cursor) =
            type_into("long line\nab", TextCursor::at(9), &[key(Key::ArrowDown, Modifiers::NONE)]);
        assert_eq!(cursor.cursor, 12);
        let (_, cursor) =
            type_into("long line\nab", TextCursor::at(10), &[key(Key::ArrowUp, Modifiers::NONE)]);
        assert_eq!(cursor.cursor, 0);
    }

    #[test]
    fn copy_and_cut_hand_back_the_selection() {
        let mut content = "Hello world".to_owned();
        let mut cursor = TextCursor { anchor: 6, cursor: 11 };
        let layout = layout(&content);
        assert_eq!(cursor.handle(&mut content, &layout, &Event::Copy).copy.as_deref(), Some("world"));
        assert_eq!(content, "Hello world");
        assert_eq!(cursor.handle(&mut content, &layout, &Event::Cut).copy.as_deref(), Some("world"));
        assert_eq!(content, "Hello ");
    }

    #[test]
    fn clicks_land_on_the_nearest_boundary_of_the_nearest_line() {
        let layout = layout("ab\ncd");
        let (x_of_b, _) = caret_position(&layout, 1);
        // Just right of the boundary between "a" and "b", on the first line.
        assert_eq!(position_at(&layout, Point::new(x_of_b + 1.0, -20.0)), 1);
        // Far below and to the right: the end of the last line.
        assert_eq!(position_at(&layout, Point::new(9999.0, 9999.0)), 5);
    }

    #[test]
    fn selections_highlight_each_line_they_touch() {
        let layout = layout("abc\ndef");
        let boxes = selection_boxes(&layout, 1..6);
        assert_eq!(boxes.len(), 2);
        assert!(boxes[0].min.y < boxes[1].min.y);
        assert!(selection_boxes(&layout, 2..2).is_empty());
    }
}
