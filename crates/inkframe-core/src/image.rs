//! Bitmap images placed on the stage: reference drawings to trace over,
//! painted backgrounds, photos to composite.
//!
//! Like audio, the document only *refers* to image files and never embeds
//! them, so project files stay small, readable and diffable. Each imported
//! file becomes an [`ImageAsset`] in the document, and shapes of type
//! [`Shape::Image`](crate::Shape::Image) place it on a layer. Decoding the
//! pixels is the renderer's job; this crate only records which file, and
//! its size in pixels.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::audio::resolve_linked;
use crate::document::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageId(pub(crate) u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageAsset {
    pub id: ImageId,
    pub name: String,
    /// The image file, relative to the project file's folder if it was
    /// inside it when imported, otherwise absolute.
    pub path: PathBuf,
    /// Size in pixels when imported. Kept in the document so layouts and
    /// selection work even while the file is missing or still loading.
    pub width: u32,
    pub height: u32,
    /// The Library folder it's filed in; empty for the top level.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

impl ImageAsset {
    /// The file to actually open, given the folder the project file is in.
    /// See [`AudioTrack::resolve`](crate::AudioTrack::resolve), which works
    /// the same way.
    pub fn resolve(&self, project_dir: Option<&Path>) -> PathBuf {
        resolve_linked(&self.path, project_dir)
    }
}

impl Document {
    /// Records an image file of `width`×`height` pixels. `path` is stored
    /// relative to `project_dir` when the file is inside it.
    ///
    /// Importing the same file twice reuses the existing asset, so the
    /// renderer only decodes it once.
    pub fn add_image(
        &mut self,
        path: &Path,
        project_dir: Option<&Path>,
        (width, height): (u32, u32),
    ) -> ImageId {
        let stored = project_dir
            .and_then(|dir| path.strip_prefix(dir).ok())
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf);
        if let Some(existing) = self.images.iter_mut().find(|i| i.path == stored) {
            // The file may have been edited since; trust its current size.
            (existing.width, existing.height) = (width, height);
            return existing.id;
        }
        let id = ImageId(self.next_image_id);
        self.next_image_id += 1;
        let name = path.file_stem().map_or_else(|| "Image".to_owned(), |s| s.to_string_lossy().into_owned());
        self.images.push(ImageAsset { id, name, path: stored, width, height, folder: String::new() });
        id
    }

    pub fn image(&self, id: ImageId) -> Option<&ImageAsset> {
        self.images.iter().find(|i| i.id == id)
    }

    /// Whether any layer, in any timeline, shows the image. An asset stays
    /// in the document after its last layer is deleted (undo may bring the
    /// layer back), so this is what decides whether a missing file matters.
    pub fn image_in_use(&self, id: ImageId) -> bool {
        self.all_layers()
            .flat_map(|layer| layer.all_elements())
            .any(|element| matches!(element.shape, crate::Shape::Image { image, .. } if image == id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_inside_the_project_folder_are_stored_relative() {
        let mut doc = Document::default();
        let dir = Path::new("/projects/shot");
        let id = doc.add_image(&dir.join("art/sky.png"), Some(dir), (640, 480));
        let image = doc.image(id).unwrap();
        assert_eq!(image.path, Path::new("art/sky.png"));
        assert_eq!(image.name, "sky");
        assert_eq!(image.resolve(Some(dir)), dir.join("art/sky.png"));
    }

    #[test]
    fn importing_a_file_again_reuses_its_asset() {
        let mut doc = Document::default();
        let first = doc.add_image(Path::new("/tmp/a.png"), None, (10, 10));
        let again = doc.add_image(Path::new("/tmp/a.png"), None, (12, 10));
        assert_eq!(first, again);
        assert_eq!(doc.images.len(), 1);
        assert_eq!(doc.image(first).unwrap().width, 12);
    }
}
