//! Font files that travel with a project.
//!
//! Text normally uses the fonts installed on the computer. A project can
//! also link font files of its own, the same way it links images and sound,
//! so its text looks the same on any machine that has the project folder,
//! whether or not the font is installed there. That matters most for a
//! final render, where a substituted font would quietly change the look.
//!
//! Text refers to fonts by family name, so linking a file is all it takes:
//! the renderer looks in the project's fonts before the system's.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::audio::resolve_linked;
use crate::document::Document;
use crate::shape::Shape;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FontFileId(pub(crate) u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontFile {
    pub id: FontFileId,
    /// The font file, relative to the project file's folder if it was inside
    /// it when imported, otherwise absolute.
    pub path: PathBuf,
    /// The family names the file provides (usually one; a collection may
    /// hold several), recorded at import for showing in menus even while
    /// the file is missing.
    pub families: Vec<String>,
}

impl FontFile {
    /// The file to actually open. See [`AudioTrack::resolve`](crate::AudioTrack::resolve).
    pub fn resolve(&self, project_dir: Option<&Path>) -> PathBuf {
        resolve_linked(&self.path, project_dir)
    }

    /// Whether any text in `doc` is set in one of this file's families.
    pub fn in_use(&self, doc: &Document) -> bool {
        doc.all_layers().flat_map(|layer| layer.all_elements()).any(|element| match &element.shape {
            Shape::Text(text) => self.families.contains(&text.font.family),
            _ => false,
        })
    }
}

impl Document {
    /// Links a font file providing `families`. `path` is stored relative to
    /// `project_dir` when the file is inside it. Linking the same file again
    /// just refreshes its family names.
    pub fn add_font_file(
        &mut self,
        path: &Path,
        project_dir: Option<&Path>,
        families: Vec<String>,
    ) -> FontFileId {
        let stored = project_dir
            .and_then(|dir| path.strip_prefix(dir).ok())
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf);
        if let Some(existing) = self.fonts.iter_mut().find(|f| f.path == stored) {
            existing.families = families;
            return existing.id;
        }
        let id = FontFileId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.push(FontFile { id, path: stored, families });
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Style, Text, Transform};

    #[test]
    fn font_files_are_linked_once_and_know_when_theyre_used() {
        let mut doc = Document::default();
        let dir = Path::new("/projects/titles");
        let id = doc.add_font_file(&dir.join("fonts/Brand.otf"), Some(dir), vec!["Brand".into()]);
        let again = doc.add_font_file(&dir.join("fonts/Brand.otf"), Some(dir), vec!["Brand Sans".into()]);
        assert_eq!(id, again);
        assert_eq!(doc.fonts.len(), 1);
        assert_eq!(doc.fonts[0].path, Path::new("fonts/Brand.otf"));
        assert!(!doc.fonts[0].in_use(&doc));

        let mut text = Text::new("Hello", 40.0);
        text.font.family = "Brand Sans".into();
        let style = Style { fill: Some(Color::BLACK), stroke: None, gradient: None };
        doc.add_layer("Title", vec![Element { shape: Shape::Text(text), style }], 0, Transform::default());
        assert!(doc.fonts[0].in_use(&doc));
    }
}
