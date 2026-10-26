//! The brush library: the ready-made brushes and the ones the person saves,
//! as Photoshop's Brushes panel holds them.
//!
//! Saved brushes live in one file in the settings folder, `brushes.json`,
//! as a list of presets (a sampled tip goes in as a small PNG). The same
//! list, written anywhere, is a brush set to share: Export Brushes writes
//! one and Import Brushes reads it back, or reads Photoshop's own `.abr`
//! files.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use inkframe_core::{BrushPreset, BrushSettings};

use crate::i18n::{tr, tr_args};

/// The file extension of an Inkframe brush set.
pub const BRUSH_SET_EXTENSION: &str = "inkbrushes";

#[derive(Debug, Clone, Default)]
pub struct BrushLibrary {
    /// The person's own brushes, in their order.
    pub saved: Vec<BrushPreset>,
    /// Where they're kept: `None` in tests, which never touch the disk.
    file: Option<PathBuf>,
    /// The name of the brush last picked, to show which is in use.
    pub picked: Option<String>,
}

impl BrushLibrary {
    /// The ready-made brushes, made once (their tips are drawn in code).
    pub fn built_in() -> &'static [BrushPreset] {
        static BUILT_IN: OnceLock<Vec<BrushPreset>> = OnceLock::new();
        BUILT_IN.get_or_init(BrushSettings::presets)
    }

    /// The person's brushes from the settings folder.
    pub fn load() -> Self {
        let file = crate::layout::settings_dir().map(|dir| dir.join("brushes.json"));
        let saved = file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self { saved, file, picked: None }
    }

    /// Every brush: the ready-made ones, then the saved ones, each with
    /// whether it's saved (so can be renamed and deleted).
    pub fn all(&self) -> impl Iterator<Item = (&BrushPreset, bool)> {
        Self::built_in().iter().map(|p| (p, false)).chain(self.saved.iter().map(|p| (p, true)))
    }

    /// Saves `settings` as a brush called `name` (numbered if the name is
    /// taken), and returns the name it got.
    pub fn add(&mut self, name: &str, settings: BrushSettings) -> String {
        let name = self.free_name(name);
        self.saved.push(BrushPreset::new(name.clone(), settings));
        self.write();
        name
    }

    pub fn rename(&mut self, index: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() || self.saved.get(index).is_none_or(|p| p.name == name) {
            return;
        }
        let name = self.free_name(name);
        self.saved[index].name = name;
        self.write();
    }

    /// Replaces a saved brush's settings with `settings`, keeping its name.
    pub fn update(&mut self, index: usize, settings: BrushSettings) {
        if let Some(preset) = self.saved.get_mut(index) {
            preset.settings = settings;
            self.write();
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.saved.len() {
            self.saved.remove(index);
            self.write();
        }
    }

    /// `name`, or `name 2`, `name 3`… if a brush already has it.
    fn free_name(&self, name: &str) -> String {
        let taken = |n: &str| self.all().any(|(p, _)| p.name == n);
        if !taken(name) {
            return name.to_owned();
        }
        (2..).map(|n| format!("{name} {n}")).find(|n| !taken(n)).expect("a number is free")
    }

    /// Adds the brushes in a brush set (`.inkbrushes`) or a Photoshop
    /// brush file (`.abr`), returning how many.
    pub fn import(&mut self, path: &Path) -> Result<usize, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let presets: Vec<BrushPreset> = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("abr")) {
            let stem =
                path.file_stem().map_or_else(|| "Brush".to_owned(), |s| s.to_string_lossy().into_owned());
            inkframe_core::abr::read(&bytes, &stem).map_err(|e| e.to_string())?
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|_| tr("That isn't a brush set Inkframe can read.").to_owned())?
        };
        if presets.is_empty() {
            return Err(tr("There are no brushes in that file.").to_owned());
        }
        let count = presets.len();
        for preset in presets {
            let name = self.free_name(&preset.name);
            self.saved.push(BrushPreset { name, ..preset });
        }
        self.write();
        Ok(count)
    }

    /// Writes the saved brushes (or just `only`, by index) as a brush set.
    pub fn export(&self, path: &Path, only: Option<usize>) -> Result<(), String> {
        let presets: Vec<&BrushPreset> = match only {
            Some(i) => self.saved.get(i).into_iter().collect(),
            None => self.saved.iter().collect(),
        };
        let json = serde_json::to_string_pretty(&presets).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| {
            tr_args(
                "Couldn't write {file}: {problem}",
                &[("file", &path.display().to_string()), ("problem", &e.to_string())],
            )
        })
    }

    fn write(&self) {
        let Some(path) = &self.file else { return };
        if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_string(&self.saved)) {
            let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(path, json));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_brushes_get_names_of_their_own() {
        let mut library = BrushLibrary::default();
        let built_in = BrushLibrary::built_in().len();
        assert_eq!(library.add("Mine", BrushSettings::default()), "Mine");
        assert_eq!(library.add("Mine", BrushSettings::default()), "Mine 2");
        // A ready-made brush's name is taken too.
        assert_eq!(library.add("Soft Round", BrushSettings::default()), "Soft Round 2");
        library.rename(0, "Soft Round");
        assert_eq!(library.saved[0].name, "Soft Round 3");
        library.remove(1);
        assert_eq!(library.all().count(), built_in + 2);
        assert!(library.all().take(built_in).all(|(_, saved)| !saved));
    }

    #[test]
    fn a_brush_set_exports_and_imports() {
        let dir = std::env::temp_dir().join(format!("inkframe-brushes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("set.inkbrushes");
        let mut library = BrushLibrary::default();
        let foliage = BrushLibrary::built_in().iter().find(|p| p.name == "Foliage").unwrap().settings.clone();
        library.add("Bushes", foliage.clone());
        library.export(&path, None).unwrap();
        let mut other = BrushLibrary::default();
        assert_eq!(other.import(&path), Ok(1));
        assert_eq!(other.saved[0], BrushPreset::new("Bushes", foliage));
        std::fs::write(&path, "nonsense").unwrap();
        assert!(other.import(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
