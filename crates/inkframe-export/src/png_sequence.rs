use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use crate::{ExportError, ExportSettings, FrameRenderer, Progress};

pub(crate) fn write(
    renderer: &FrameRenderer,
    settings: &ExportSettings,
    mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    let frames_total = settings.frames.end() - settings.frames.start() + 1;
    let digits = digits_for(*settings.frames.end() + 1);
    let mut written = Vec::new();

    for (done, frame) in settings.frames.clone().enumerate() {
        let path = frame_path(&settings.output, frame, digits);
        let png = renderer.render(frame)?.encode_png().map_err(|e| ExportError::Png(e.to_string()))?;
        std::fs::write(&path, png)?;
        written.push(path);

        let progress = Progress { frames_done: done as u32 + 1, frames_total };
        if on_progress(progress).is_break() {
            // Don't leave half a sequence behind; it's easy to mistake for a
            // complete one later.
            for path in written {
                let _ = std::fs::remove_file(path);
            }
            return Err(ExportError::Cancelled);
        }
    }
    Ok(())
}

/// Builds `dir/name_0001.png` from `dir/name.png`.
///
/// Numbers start at 1 to match the frame numbers shown in the timeline, and
/// are zero-padded so the files sort correctly in every file browser.
fn frame_path(pattern: &Path, frame: u32, digits: usize) -> PathBuf {
    let stem = pattern.file_stem().and_then(|s| s.to_str()).unwrap_or("frame");
    pattern.with_file_name(format!("{stem}_{:0digits$}.png", frame + 1))
}

/// Zero-padding width: at least 4 digits, more for very long animations.
fn digits_for(last_frame_number: u32) -> usize {
    last_frame_number.to_string().len().max(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_paths_are_one_based_and_padded() {
        let path = frame_path(Path::new("/tmp/out/walk.png"), 0, 4);
        assert_eq!(path, Path::new("/tmp/out/walk_0001.png"));
    }

    #[test]
    fn padding_grows_for_long_sequences() {
        assert_eq!(digits_for(48), 4);
        assert_eq!(digits_for(123_456), 6);
    }
}
