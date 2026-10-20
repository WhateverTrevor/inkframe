//! The Library: everything a project can place again and again. Symbols,
//! imported images and sounds, filed in folders as in Animate's Library.
//!
//! Folders are one level deep and known by name. Each asset keeps the name
//! of the folder it's in (empty for the top level), and the document keeps
//! the list of folders, so a new folder stays until it's deleted even
//! with nothing in it yet.

use serde::{Deserialize, Serialize};

use crate::{AudioId, Document, ImageId, SymbolId};

/// One thing in the Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asset {
    Symbol(SymbolId),
    Image(ImageId),
    Sound(AudioId),
}

impl Document {
    /// Everything in the Library: symbols, then images, then sounds, each
    /// in the order they were added.
    pub fn library_assets(&self) -> Vec<Asset> {
        let symbols = self.symbols.iter().map(|s| Asset::Symbol(s.id));
        let images = self.images.iter().map(|i| Asset::Image(i.id));
        let sounds = self.audio.iter().map(|a| Asset::Sound(a.id));
        symbols.chain(images).chain(sounds).collect()
    }

    pub fn asset_name(&self, asset: Asset) -> Option<&str> {
        match asset {
            Asset::Symbol(id) => self.symbol(id).map(|s| s.name.as_str()),
            Asset::Image(id) => self.image(id).map(|i| i.name.as_str()),
            Asset::Sound(id) => self.audio_track(id).map(|a| a.name.as_str()),
        }
    }

    /// The folder an asset is filed in: empty for the top level, `None` if
    /// the asset doesn't exist.
    pub fn asset_folder(&self, asset: Asset) -> Option<&str> {
        match asset {
            Asset::Symbol(id) => self.symbol(id).map(|s| s.folder.as_str()),
            Asset::Image(id) => self.image(id).map(|i| i.folder.as_str()),
            Asset::Sound(id) => self.audio_track(id).map(|a| a.folder.as_str()),
        }
    }

    /// The name and folder fields of an asset, to change them.
    fn asset_fields_mut(&mut self, asset: Asset) -> Option<(&mut String, &mut String)> {
        match asset {
            Asset::Symbol(id) => self.symbol_mut(id).map(|s| (&mut s.name, &mut s.folder)),
            Asset::Image(id) => {
                self.images.iter_mut().find(|i| i.id == id).map(|i| (&mut i.name, &mut i.folder))
            }
            Asset::Sound(id) => self.audio_track_mut(id).map(|a| (&mut a.name, &mut a.folder)),
        }
    }

    /// Renames an asset. A blank name is ignored, since the Library would
    /// have nothing to show for it.
    pub fn rename_asset(&mut self, asset: Asset, name: &str) {
        let name = name.trim();
        if let Some((current, _)) = self.asset_fields_mut(asset)
            && !name.is_empty()
        {
            *current = name.to_owned();
        }
    }

    /// Files an asset in `folder` (empty for the top level), making the
    /// folder if there isn't one by that name.
    pub fn move_asset(&mut self, asset: Asset, folder: &str) {
        let folder = folder.trim();
        if !folder.is_empty() && !self.library_folders.iter().any(|f| f == folder) {
            self.library_folders.push(folder.to_owned());
        }
        if let Some((_, current)) = self.asset_fields_mut(asset) {
            *current = folder.to_owned();
        }
    }

    /// The Library's folders in alphabetical order, including any an asset
    /// names that the list has lost track of (from a hand-edited file).
    pub fn library_folder_names(&self) -> Vec<String> {
        let used = self.library_assets().into_iter().filter_map(|a| self.asset_folder(a).map(str::to_owned));
        let mut names: Vec<String> =
            self.library_folders.iter().cloned().chain(used).filter(|f| !f.is_empty()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// A new, empty folder called "Folder 1", "Folder 2"… whichever is
    /// free. Returns its name.
    pub fn add_library_folder(&mut self) -> String {
        let taken = self.library_folder_names();
        let name = (1..)
            .map(|n| format!("Folder {n}"))
            .find(|name| !taken.contains(name))
            .expect("there is always an unused number");
        self.library_folders.push(name.clone());
        name
    }

    /// Renames a folder, keeping what's in it. Refused (returning false)
    /// for a blank name or one another folder already has.
    pub fn rename_library_folder(&mut self, old: &str, new: &str) -> bool {
        let new = new.trim();
        if new.is_empty() || (new != old && self.library_folder_names().iter().any(|f| f == new)) {
            return false;
        }
        for name in self.library_folders.iter_mut().filter(|f| *f == old) {
            *name = new.to_owned();
        }
        if !self.library_folders.iter().any(|f| f == new) {
            self.library_folders.push(new.to_owned());
        }
        for asset in self.library_assets() {
            if let Some((_, folder)) = self.asset_fields_mut(asset)
                && folder == old
            {
                *folder = new.to_owned();
            }
        }
        true
    }

    /// Deletes a folder. What was in it moves to the top level rather than
    /// being deleted with it, since symbols and sounds may be in use.
    pub fn remove_library_folder(&mut self, name: &str) {
        self.library_folders.retain(|f| f != name);
        self.rename_folder_contents(name, "");
    }

    fn rename_folder_contents(&mut self, from: &str, to: &str) {
        for asset in self.library_assets() {
            if let Some((_, folder)) = self.asset_fields_mut(asset)
                && folder == from
            {
                *folder = to.to_owned();
            }
        }
    }

    /// Whether an asset can be deleted from the Library. A symbol or image
    /// that's still on the stage can't; a sound can, since deleting it is
    /// how it comes off the timeline.
    pub fn can_remove_asset(&self, asset: Asset) -> bool {
        match asset {
            Asset::Symbol(id) => self.symbol(id).is_some() && self.symbol_use_count(id) == 0,
            Asset::Image(id) => self.image(id).is_some() && !self.image_in_use(id),
            Asset::Sound(id) => self.audio_track(id).is_some(),
        }
    }

    /// Deletes an asset if [`can_remove_asset`](Self::can_remove_asset)
    /// allows it. Returns whether it did.
    pub fn remove_asset(&mut self, asset: Asset) -> bool {
        if !self.can_remove_asset(asset) {
            return false;
        }
        match asset {
            Asset::Symbol(id) => self.remove_symbol(id),
            Asset::Image(id) => {
                self.images.retain(|i| i.id != id);
                true
            }
            Asset::Sound(id) => self.remove_audio(id).is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{Shape, Style, Transform};

    fn library() -> (Document, Asset, Asset, Asset) {
        let mut doc = Document::default();
        let layer = doc.add_layer("Ball", Vec::new(), 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(layer, 0).unwrap();
        let image = doc.add_image(Path::new("/art/sky.png"), None, (64, 32));
        let sound = doc.add_audio(Path::new("/sound/boing.wav"), None, 0);
        (doc, Asset::Symbol(symbol), Asset::Image(image), Asset::Sound(sound))
    }

    #[test]
    fn folders_hold_assets_and_outlive_being_empty() {
        let (mut doc, symbol, image, sound) = library();
        assert_eq!(doc.library_assets(), vec![symbol, image, sound]);

        let props = doc.add_library_folder();
        assert_eq!(props, "Folder 1");
        doc.move_asset(symbol, &props);
        doc.move_asset(sound, "Sounds");
        assert_eq!(doc.asset_folder(symbol), Some("Folder 1"));
        assert_eq!(doc.asset_folder(image), Some(""));
        assert_eq!(doc.library_folder_names(), vec!["Folder 1", "Sounds"]);

        doc.move_asset(symbol, "");
        assert_eq!(doc.library_folder_names(), vec!["Folder 1", "Sounds"], "an empty folder stays");

        assert!(doc.rename_library_folder("Sounds", "Audio"));
        assert_eq!(doc.asset_folder(sound), Some("Audio"));
        assert!(!doc.rename_library_folder("Audio", "Folder 1"), "taken");
        assert!(!doc.rename_library_folder("Audio", "  "), "blank");

        doc.remove_library_folder("Audio");
        assert_eq!(doc.asset_folder(sound), Some(""), "back to the top level, not deleted");
        assert_eq!(doc.library_folder_names(), vec!["Folder 1"]);
    }

    #[test]
    fn assets_on_the_stage_cant_be_deleted() {
        let (mut doc, symbol, image, sound) = library();
        assert!(!doc.can_remove_asset(symbol), "the converted layer uses it");
        let Asset::Image(id) = image else { unreachable!() };
        let picture = crate::Element {
            shape: Shape::Image { image: id, width: 64.0, height: 32.0 },
            style: Style { fill: None, stroke: None },
        };
        let layer = doc.add_layer("Sky", vec![picture], 0, Transform::default());
        assert!(!doc.remove_asset(image));
        doc.remove_layer(layer);
        assert!(doc.remove_asset(image));
        assert!(doc.remove_asset(sound));
        assert_eq!(doc.library_assets(), vec![symbol]);
    }

    #[test]
    fn renaming_ignores_blank_names() {
        let (mut doc, symbol, ..) = library();
        doc.rename_asset(symbol, "  Hero ");
        assert_eq!(doc.asset_name(symbol), Some("Hero"));
        doc.rename_asset(symbol, " ");
        assert_eq!(doc.asset_name(symbol), Some("Hero"));
    }
}
