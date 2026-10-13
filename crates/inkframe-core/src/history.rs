use crate::document::Document;

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
    undo: Vec<Document>,
    redo: Vec<Document>,
    /// The document as of the last commit.
    baseline: Document,
    limit: usize,
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
        let previous = std::mem::replace(&mut self.baseline, doc.clone());
        self.undo.push(previous);
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
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.baseline, previous.clone()));
        *doc = previous;
        true
    }

    /// Steps forward again. Returns `false` if there was nothing to redo.
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.baseline, next.clone()));
        *doc = next;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
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
}
