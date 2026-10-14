//! Turns an Inkframe document into image sequences and video files.
//!
//! Frames are rendered with `inkframe-render` (the same code that draws the
//! editor's stage). PNG sequences are written directly. Video formats are
//! handed to an external `ffmpeg` process as raw RGBA frames over a pipe.
//!
//! Using ffmpeg as a separate program, rather than linking its libraries,
//! keeps the build simple on every platform and avoids codec licensing
//! questions for this crate. The trade-off is that users need ffmpeg
//! installed (or bundled with the app); see [`ffmpeg::locate`].

pub mod ffmpeg;
mod png_sequence;

use std::ops::{ControlFlow, RangeInclusive};
use std::path::PathBuf;

use inkframe_audio::{Clip, sample_frame};
use inkframe_core::Document;
use inkframe_render::{ImageLibrary, RenderOptions, render_frame};
use tiny_skia::Pixmap;

/// The output formats Inkframe can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    /// One lossless PNG per frame. Works everywhere and keeps transparency.
    PngSequence,
    /// Apple ProRes 4444: visually lossless 10-bit-plus, with alpha. The usual
    /// choice for handing animation to an editor or compositor.
    ProRes4444,
    /// ProRes 4444 XQ: like 4444 but with a higher data rate, for mastering.
    ProRes4444Xq,
    /// ProRes 422 HQ: high quality, smaller than 4444, no alpha channel.
    ProRes422Hq,
    /// H.264 in an MP4 container: plays everywhere, ideal for review and the
    /// web. No transparency and lossy compression, so not for final masters.
    H264,
    /// HEVC with an alpha channel, using Apple's hardware encoder. Plays in
    /// Safari, Final Cut Pro and on iOS. Only available on macOS.
    HevcWithAlpha,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 6] = [
        ExportFormat::PngSequence,
        ExportFormat::ProRes4444,
        ExportFormat::ProRes4444Xq,
        ExportFormat::ProRes422Hq,
        ExportFormat::H264,
        ExportFormat::HevcWithAlpha,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::PngSequence => "PNG Sequence",
            ExportFormat::ProRes4444 => "Apple ProRes 4444 (.mov)",
            ExportFormat::ProRes4444Xq => "Apple ProRes 4444 XQ (.mov)",
            ExportFormat::ProRes422Hq => "Apple ProRes 422 HQ (.mov)",
            ExportFormat::H264 => "H.264 (.mp4)",
            ExportFormat::HevcWithAlpha => "HEVC with Alpha (.mov)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::PngSequence => "png",
            ExportFormat::H264 => "mp4",
            _ => "mov",
        }
    }

    pub fn supports_alpha(self) -> bool {
        matches!(
            self,
            ExportFormat::PngSequence
                | ExportFormat::ProRes4444
                | ExportFormat::ProRes4444Xq
                | ExportFormat::HevcWithAlpha
        )
    }

    pub fn needs_ffmpeg(self) -> bool {
        self != ExportFormat::PngSequence
    }

    /// Whether this format can be produced on the current platform.
    pub fn is_available(self) -> bool {
        match self {
            // VideoToolbox is Apple's hardware video framework.
            ExportFormat::HevcWithAlpha => cfg!(target_os = "macos"),
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub format: ExportFormat,
    /// For video, the output file. For PNG sequences, the path of the first
    /// image's *name pattern*: `shots/walk.png` produces `shots/walk_0001.png`,
    /// `shots/walk_0002.png` and so on.
    pub output: PathBuf,
    /// Zero-based, inclusive frame range to export.
    pub frames: RangeInclusive<u32>,
    /// Resolution multiplier; 1.0 is the document's own size.
    pub scale: f32,
    /// Leave the background transparent. Ignored by formats without alpha,
    /// which always get the document's background color.
    pub transparent: bool,
    /// The folder the project file is in, for finding audio files that were
    /// saved with paths relative to it.
    pub project_dir: Option<PathBuf>,
}

impl ExportSettings {
    fn transparent_output(&self) -> bool {
        self.transparent && self.format.supports_alpha()
    }
}

/// Reported after each frame is written.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub frames_done: u32,
    pub frames_total: u32,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        self.frames_done as f32 / self.frames_total.max(1) as f32
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the frame range is empty")]
    NoFrames,
    #[error("couldn't allocate a {0}×{1} frame")]
    FrameTooLarge(u32, u32),
    #[error("{0} isn't available on this platform")]
    Unavailable(&'static str),
    #[error(
        "ffmpeg wasn't found. Install it (for example `brew install ffmpeg` on macOS) \
         or set the INKFRAME_FFMPEG environment variable to its path"
    )]
    FfmpegNotFound,
    #[error("ffmpeg failed: {0}")]
    FfmpegFailed(String),
    #[error("couldn't write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't encode PNG: {0}")]
    Png(String),
    #[error("couldn't use an audio file: {0}")]
    Audio(String),
    #[error("couldn't use an image: {0}")]
    Image(String),
    #[error("couldn't use a font: {0}")]
    Font(String),
    #[error("export cancelled")]
    Cancelled,
}

/// Exports `doc` according to `settings`.
///
/// This blocks until the export finishes, so call it from a background
/// thread. `on_progress` runs after every frame; returning
/// `ControlFlow::Break(())` cancels the export and cleans up partial output.
pub fn export(
    doc: &Document,
    settings: &ExportSettings,
    on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    if settings.frames.is_empty() {
        return Err(ExportError::NoFrames);
    }
    if !settings.format.is_available() {
        return Err(ExportError::Unavailable(settings.format.label()));
    }

    let audio = soundtrack(doc, settings)?;
    // As with audio, a missing picture fails the export up front instead of
    // putting a placeholder into the final render.
    let images = ImageLibrary::load(doc, settings.project_dir.as_deref());
    if let Some((name, problem)) = images.problems(doc).next() {
        return Err(ExportError::Image(format!("{name}: {problem}")));
    }
    // A font file that's gone would make its text fall back to another
    // font, quietly changing the look of the final render.
    let fonts = inkframe_render::sync_project_fonts(doc, settings.project_dir.as_deref());
    let in_use = |id| doc.fonts.iter().any(|f| f.id == id && f.in_use(doc));
    if let Some((_, problem)) = fonts.into_iter().find(|(id, _)| in_use(*id)) {
        return Err(ExportError::Font(problem));
    }
    let renderer = FrameRenderer::new(doc, &images, settings);
    match settings.format {
        ExportFormat::PngSequence => png_sequence::write(&renderer, settings, audio.as_ref(), on_progress),
        _ => ffmpeg::encode(&renderer, settings, audio.as_ref(), on_progress),
    }
}

/// The mixed audio for the exported frame range, or `None` if there's
/// nothing to hear.
///
/// A file that can't be read fails the export rather than being skipped:
/// a final render quietly missing its dialogue is worse than an error
/// message.
fn soundtrack(doc: &Document, settings: &ExportSettings) -> Result<Option<Clip>, ExportError> {
    if doc.audio.iter().all(|t| t.muted) {
        return Ok(None);
    }
    let (mix, errors) = inkframe_audio::mix_document(doc, settings.project_dir.as_deref());
    if let Some((path, error)) = errors.into_iter().next() {
        return Err(ExportError::Audio(format!("{}: {error}", path.display())));
    }
    Ok(mix.map(|clip| {
        let fps = doc.frame_rate.fps();
        let channels = inkframe_audio::CHANNELS as usize;
        let start = sample_frame(*settings.frames.start(), fps) * channels;
        let end = (sample_frame(settings.frames.end() + 1, fps) * channels).min(clip.samples.len());
        Clip { samples: clip.samples[start.min(end)..end].to_vec() }
    }))
}

/// Renders frames for an export with fixed settings.
struct FrameRenderer<'a> {
    doc: &'a Document,
    options: RenderOptions<'a>,
}

impl<'a> FrameRenderer<'a> {
    fn new(doc: &'a Document, images: &'a ImageLibrary, settings: &ExportSettings) -> Self {
        let options = RenderOptions {
            scale: settings.scale,
            draw_background: !settings.transparent_output(),
            images: Some(images),
            ..RenderOptions::default()
        };
        Self { doc, options }
    }

    fn size(&self) -> (u32, u32) {
        inkframe_render::output_size(self.doc, self.options.scale)
    }

    fn render(&self, frame: u32) -> Result<Pixmap, ExportError> {
        let (w, h) = self.size();
        render_frame(self.doc, frame, &self.options).ok_or(ExportError::FrameTooLarge(w, h))
    }
}
