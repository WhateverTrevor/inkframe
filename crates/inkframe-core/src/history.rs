use crate::document::{Document, Layer};
use crate::shape::Shape;

/// Undo/redo by snapshots.
///
/// Rather than recording every individual edit as a reversible command, we
/// keep whole copies of the document. Vector animations are small (kilobytes,
/// not megabytes), so cloning is cheap, and the approach has a big advantage
/// for an open-source project: nobody has to write an "undo" for each new
/// feature. Anything that changes the document is undoable automatically.
///
/// The editor calls [`History::commit`] once per UI frame while the user
/// isn't in the middle of a gesture. If the document differs from the last
/// committed state, that state goes on the undo stack. A whole mouse drag
/// therefore becomes a single undo step rather than one per pixel moved.
#[derive(Debug)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The document as of the last commit.
    baseline: Document,
    limit: usize,
}

/// A document state on the undo or redo stack, and a few words saying
/// what changed between it and the state after it, for the History panel.
#[derive(Debug)]
struct Step {
    doc: Document,
    label: String,
}

impl History {
    /// How many undo steps to keep before the oldest is discarded.
    pub const DEFAULT_LIMIT: usize = 200;

    pub fn new(doc: &Document) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), baseline: doc.clone(), limit: Self::DEFAULT_LIMIT }
    }

    /// Records `doc` as a new undo step if it changed since the last commit.
    pub fn commit(&mut self, doc: &Document) {
        if *doc == self.baseline {
            return;
        }
        let label = describe(&self.baseline, doc);
        let previous = std::mem::replace(&mut self.baseline, doc.clone());
        self.undo.push(Step { doc: previous, label });
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        // A fresh edit forks history; the old "future" can't be reached anymore.
        self.redo.clear();
    }

    /// Steps back. Returns `false` if there was nothing to undo.
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        // Make sure edits that haven't been committed yet (say, a text field
        // the user is still typing in) aren't lost: they become the step we
        // undo from.
        self.commit(doc);
        let Some(Step { doc: previous, label }) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.baseline, previous.clone());
        self.redo.push(Step { doc: current, label });
        *doc = previous;
        true
    }

    /// Steps forward again. Returns `false` if there was nothing to redo.
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        let Some(Step { doc: next, label }) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.baseline, next.clone());
        self.undo.push(Step { doc: current, label });
        *doc = next;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What each step did: those done, oldest first, and those undone that
    /// can be redone, next first.
    pub fn labels(&self) -> (Vec<&str>, Vec<&str>) {
        let done = self.undo.iter().map(|s| s.label.as_str()).collect();
        let undone = self.redo.iter().rev().map(|s| s.label.as_str()).collect();
        (done, undone)
    }

    /// Undoes (`steps < 0`) or redoes that many steps at once, as clicking
    /// a step in the History panel does. Returns whether anything changed.
    pub fn jump(&mut self, doc: &mut Document, steps: isize) -> bool {
        let mut changed = false;
        for _ in 0..steps.unsigned_abs() {
            let moved = if steps < 0 { self.undo(doc) } else { self.redo(doc) };
            if !moved {
                break;
            }
            changed = true;
        }
        changed
    }
}

/// A few words for what changed from `before` to `after`: "Add “Ball”",
/// "Paint on “Sky”", "Restack layers". Found by comparing the two, so
/// every edit gets a name without each feature having to give one.
pub fn describe(before: &Document, after: &Document) -> String {
    let named = |layer: &Layer| format!("“{}”", layer.name);
    let old: Vec<&Layer> = before.all_layers().collect();
    let new: Vec<&Layer> = after.all_layers().collect();
    let added: Vec<&&Layer> = new.iter().filter(|l| !old.iter().any(|o| o.id == l.id)).collect();
    let removed: Vec<&&Layer> = old.iter().filter(|l| !new.iter().any(|n| n.id == l.id)).collect();
    match (added.as_slice(), removed.as_slice()) {
        ([layer], []) if layer.folder => return "New folder".to_owned(),
        ([layer], []) => return format!("Add {}", named(layer)),
        ([], [layer]) => return format!("Delete {}", named(layer)),
        ([_, ..], []) => return format!("Add {} layers", added.len()),
        ([], [_, ..]) => return format!("Delete {} layers", removed.len()),
        ([_, ..], [_, ..]) => return "Change layers".to_owned(),
        ([], []) => {}
    }

    let changed: Vec<(&Layer, &Layer)> = old
        .iter()
        .filter_map(|o| Some((*o, *new.iter().find(|n| n.id == o.id)?)))
        .filter(|(o, n)| o != n)
        .collect();
    let order = |doc: &Document| doc.all_layers().map(|l| l.id).collect::<Vec<_>>();
    match changed.as_slice() {
        [(o, n)] => {
            let what = if o.name != n.name {
                "Rename"
            } else if o.visible != n.visible {
                if n.visible { "Show" } else { "Hide" }
            } else if o.locked != n.locked {
                if n.locked { "Lock" } else { "Unlock" }
            } else if o.in_folder != n.in_folder {
                "Move into folder"
            } else if o.drawings != n.drawings || o.exposure != n.exposure {
                "Draw on"
            } else if o.track != n.track {
                "Animate"
            } else if (o.start, o.end) != (n.start, n.end) {
                "Change frames of"
            } else if o.color != n.color {
                "Recolour"
            } else if (o.parent, o.pivot, o.rotation_limits) != (n.parent, n.pivot, n.rotation_limits) {
                "Rig"
            } else {
                "Edit"
            };
            if what == "Move into folder" {
                return format!("Move {} into folder", named(n));
            }
            return format!("{what} {}", named(n));
        }
        [_, _, ..] => return format!("Edit {} layers", changed.len()),
        [] => {}
    }
    if order(before) != order(after) {
        return "Restack layers".to_owned();
    }
    if before.canvases != after.canvases {
        // Name the layer painted on, if one shows the changed canvas.
        let painted =
            after.canvases.iter().find(|(id, c)| before.canvases.get(id) != Some(c)).map(|(id, _)| *id);
        let layer = after.all_layers().find(|l| {
            l.drawings
                .iter()
                .flat_map(|d| &d.elements)
                .any(|e| matches!(e.shape, Shape::Paint { canvas, .. } if Some(canvas) == painted))
        });
        return layer.map_or_else(|| "Paint".to_owned(), |l| format!("Paint on {}", named(l)));
    }
    if before.symbols != after.symbols {
        return "Edit symbol".to_owned();
    }
    if before.audio != after.audio {
        return "Change audio".to_owned();
    }
    if before.images != after.images || before.fonts != after.fonts {
        return "Import".to_owned();
    }
    if before.palette != after.palette {
        return "Change palette".to_owned();
    }
    "Document settings".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_width(width: u32) -> Document {
        Document { width, ..Document::default() }
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut doc = doc_with_width(100);
        let mut history = History::new(&doc);

        doc.width = 200;
        history.commit(&doc);
        doc.width = 300;
        history.commit(&doc);

        assert!(history.undo(&mut doc));
        assert_eq!(doc.width, 200);
        assert!(history.undo(&mut doc));
        assert_eq!(doc.width, 100);
        assert!(!history.undo(&mut doc));

        assert!(history.redo(&mut doc));
        assert_eq!(doc.width, 200);
    }

    #[test]
    fn unchanged_commits_are_ignored() {
        let doc = doc_with_width(100);
        let mut history = History::new(&doc);
        history.commit(&doc);
        assert!(!history.can_undo());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut doc = doc_with_width(100);
        let mut history = History::new(&doc);
        doc.width = 200;
        history.commit(&doc);
        history.undo(&mut doc);
        doc.width = 999;
        history.commit(&doc);
        assert!(!history.can_redo());
    }

    #[test]
    fn undo_includes_uncommitted_changes() {
        let mut doc = doc_with_width(100);
        let mut history = History::new(&doc);
        doc.width = 200; // Not committed yet.
        assert!(history.undo(&mut doc));
        assert_eq!(doc.width, 100);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.width, 200);
    }

    #[test]
    fn steps_are_named_for_what_changed() {
        let mut doc = Document::default();
        let mut history = History::new(&doc);
        let id = doc.add_layer("Ball", Vec::new(), 0, crate::Transform::default());
        history.commit(&doc);
        doc.layer_mut(id).unwrap().visible = false;
        history.commit(&doc);
        doc.width = 640;
        history.commit(&doc);
        assert_eq!(history.labels().0, ["Add “Ball”", "Hide “Ball”", "Document settings"]);

        // Jumping back two steps moves those names to the redo side.
        assert!(history.jump(&mut doc, -2));
        assert_eq!(history.labels(), (vec!["Add “Ball”"], vec!["Hide “Ball”", "Document settings"]));
        assert!(doc.layer(id).unwrap().visible);
        assert!(history.jump(&mut doc, 1));
        assert!(!doc.layer(id).unwrap().visible);
    }
}
