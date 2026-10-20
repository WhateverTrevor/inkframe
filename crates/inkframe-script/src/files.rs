//! The files a script may touch: projects it opens and saves, and exports.
//!
//! That's all a script can reach on disk, and only at paths it names. A
//! project must be an `.inkf` file, and an export must have its format's
//! extension, so a mistaken path can't overwrite, say, a document or a
//! program. Relative paths start from the project's folder (or, from the
//! command line, the current folder).

use std::path::{Path, PathBuf};

use inkframe_core::Document;
use inkframe_export::{ExportFormat, ExportSettings};
use serde_json::Value;

/// `path` made absolute against `base`, with `~` for the home folder.
pub(crate) fn resolve(path: &str, base: Option<&Path>) -> Result<PathBuf, String> {
    let path = match path.strip_prefix("~/") {
        Some(rest) => home().ok_or("there's no home folder to start ~ from")?.join(rest),
        None => PathBuf::from(path),
    };
    if path.is_absolute() {
        return Ok(path);
    }
    match base {
        Some(base) => Ok(base.join(path)),
        None => Err(format!(
            "{} is relative, but this project hasn't been saved, so there's no folder to start from: save it first, or give a full path",
            path.display()
        )),
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Exports `doc` as `options` say: `format` (a short name such as `"h264"`,
/// as the command line takes), `to`, and optionally `scale`, `transparent`
/// and `frames: [first, last]` (from 1). Returns where it went.
pub(crate) fn export(
    doc: &Document,
    doc_path: Option<&Path>,
    base: Option<&Path>,
    options: &Value,
) -> Result<PathBuf, String> {
    let slug = options["format"].as_str().ok_or("export needs a format, like { format: \"h264\" }")?;
    let format = ExportFormat::from_slug(slug).ok_or_else(|| {
        let names: Vec<&str> = ExportFormat::ALL.iter().map(|f| f.slug()).collect();
        format!("{slug} isn't an export format: use one of {}", names.join(", "))
    })?;
    let to =
        options["to"].as_str().ok_or("export needs a file to write, like { to: \"renders/shot.mp4\" }")?;
    let output = resolve(to, base)?;
    if output.extension().is_none_or(|e| !e.eq_ignore_ascii_case(format.extension())) {
        return Err(format!(
            "{} needs to end in .{} for {}",
            output.display(),
            format.extension(),
            format.label()
        ));
    }
    let last = doc.last_frame();
    let frames = match options["frames"].as_array().map(Vec::as_slice) {
        None => 0..=last,
        Some([first, end]) => {
            let (first, end) =
                (first.as_u64().unwrap_or(1).max(1) as u32 - 1, end.as_u64().unwrap_or(1).max(1) as u32 - 1);
            first.min(last)..=end.min(last)
        }
        Some(_) => return Err("frames takes the first and last frame, like [1, 24]".to_owned()),
    };
    let settings = ExportSettings {
        format,
        output: output.clone(),
        frames,
        scale: options["scale"].as_f64().unwrap_or(1.0).clamp(0.05, 8.0) as f32,
        transparent: options["transparent"].as_bool().unwrap_or(true),
        project_dir: doc_path.and_then(Path::parent).map(Path::to_path_buf),
    };
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    }
    inkframe_export::export(doc, &settings, |_| std::ops::ControlFlow::Continue(()))
        .map_err(|e| format!("couldn't export {}: {e}", output.display()))?;
    Ok(output)
}
