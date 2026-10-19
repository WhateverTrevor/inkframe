//! Video encoding by piping raw frames into an `ffmpeg` child process.
//!
//! The data flow is:
//!
//! ```text
//! render frame ──► RGBA bytes ──► ffmpeg stdin ──► encoder ──► .mov / .mp4
//! ```
//!
//! ffmpeg is told the input is headerless `rawvideo` of a known size, pixel
//! format and frame rate, so we can stream frames one at a time without ever
//! holding the whole animation in memory.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;

use inkframe_core::FrameRate;

use inkframe_audio::Clip;

use crate::{ExportError, ExportFormat, ExportSettings, FrameRenderer, Progress};

/// Finds the ffmpeg executable.
///
/// Checked in order:
/// 1. The `INKFRAME_FFMPEG` environment variable, for custom builds.
/// 2. Next to the Inkframe executable, so packaged releases can bundle it.
/// 3. Every directory on `PATH`.
/// 4. Common install locations. Apps launched from the macOS Finder don't
///    inherit the shell's `PATH`, so Homebrew's directories would otherwise
///    be invisible to the packaged app even though `ffmpeg` works in Terminal.
pub fn locate() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("INKFRAME_FFMPEG").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }

    let exe_name = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    let beside_us = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let well_known = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from);

    beside_us
        .into_iter()
        .chain(on_path)
        .chain(well_known)
        .map(|dir| dir.join(&exe_name))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn encode(
    renderer: &FrameRenderer,
    settings: &ExportSettings,
    audio: Option<&Clip>,
    mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    let ffmpeg = locate().ok_or(ExportError::FfmpegNotFound)?;
    let (width, height) = renderer.size();
    let transparent = settings.transparent_output();

    // ffmpeg reads the soundtrack from a temporary WAV file alongside the
    // frames on stdin. The guard deletes it however the export ends.
    let soundtrack = match audio {
        Some(clip) => {
            let file = TempFile::new("wav");
            clip.write_wav(&file.0).map_err(|e| ExportError::Audio(e.to_string()))?;
            Some(file)
        }
        None => None,
    };
    let audio_path = soundtrack.as_ref().map(|f| f.0.as_path());
    let args = build_args(
        settings.format,
        width,
        height,
        renderer.doc.frame_rate,
        transparent,
        audio_path,
        &settings.output,
    );

    let mut encoder = Encoder::spawn(&ffmpeg, &args)?;
    let frames_total = settings.frames.end() - settings.frames.start() + 1;
    let mut straight = Vec::new();

    for (done, frame) in settings.frames.clone().enumerate() {
        let pixmap = renderer.render(frame)?;
        let bytes = if transparent {
            unpremultiply(&pixmap, &mut straight);
            &straight[..]
        } else {
            // With an opaque background every pixel has alpha = 255, and then
            // premultiplied and straight alpha are identical. Skip the copy.
            pixmap.data()
        };

        if let Err(err) = encoder.write_frame(bytes) {
            // A broken pipe usually means ffmpeg exited early. Its own error
            // message is far more useful than "broken pipe", so prefer that.
            return Err(encoder.finish().err().unwrap_or(err.into()));
        }

        let progress = Progress { frames_done: done as u32 + 1, frames_total };
        if on_progress(progress).is_break() {
            encoder.kill();
            let _ = std::fs::remove_file(&settings.output);
            return Err(ExportError::Cancelled);
        }
    }

    encoder.finish()
}

/// tiny-skia stores premultiplied alpha (each color channel already
/// multiplied by alpha), which makes blending fast. Video encoders expect
/// straight alpha, where color and transparency are independent, so we have
/// to divide it back out before handing the pixels over.
fn unpremultiply(pixmap: &tiny_skia::Pixmap, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
}

/// Builds the ffmpeg command line for a format.
///
/// Every format shares the same input description. The output side picks a
/// codec plus a pixel format, which decides chroma subsampling, bit depth
/// and whether an alpha plane is kept.
pub(crate) fn build_args(
    format: ExportFormat,
    width: u32,
    height: u32,
    rate: FrameRate,
    transparent: bool,
    audio: Option<&Path>,
    output: &Path,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));

    // -y: overwrite (the UI already asked). -nostdin is *not* used because
    // stdin is our frame pipe.
    push(&["-hide_banner", "-loglevel", "error", "-y"]);
    push(&["-f", "rawvideo", "-pix_fmt", "rgba"]);
    push(&["-video_size", &format!("{width}x{height}")]);
    // Passing the exact fraction keeps 23.976 as 24000/1001, not 23.976.
    push(&["-framerate", &format!("{}/{}", rate.numerator, rate.denominator)]);
    push(&["-i", "-"]);
    if let Some(audio) = audio {
        args.extend(["-i".into(), audio.as_os_str().to_owned()]);
        // Video from the pipe (input 0), sound from the file (input 1).
        args.extend(["-map", "0:v", "-map", "1:a"].map(OsString::from));
    }
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));

    // RGB → YUV conversion needs to know which color matrix to use. ffmpeg's
    // default is BT.601 (standard definition), which makes colors shift
    // slightly when players decode HD video as BT.709. We convert with 709
    // and tag the file to say so, so every player agrees on the colors.
    let to_yuv = |pix_fmt: &str| format!("scale=out_color_matrix=bt709:out_range=tv,format={pix_fmt}");
    let tag_bt709 = ["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"];

    match format {
        ExportFormat::ProRes4444 | ExportFormat::ProRes4444Xq | ExportFormat::ProRes422Hq => {
            // prores_ks profile numbers: 3 = 422 HQ, 4 = 4444, 5 = 4444 XQ.
            let (profile, pix_fmt) = match format {
                ExportFormat::ProRes422Hq => ("3", "yuv422p10le"),
                ExportFormat::ProRes4444 if transparent => ("4", "yuva444p10le"),
                ExportFormat::ProRes4444 => ("4", "yuv444p10le"),
                _ if transparent => ("5", "yuva444p10le"),
                _ => ("5", "yuv444p10le"),
            };
            push(&["-c:v", "prores_ks", "-profile:v", profile]);
            // "apl0" makes the file identify itself as made by Apple's encoder.
            // Some strict tools refuse ProRes files without it.
            push(&["-vendor", "apl0"]);
            if transparent {
                push(&["-alpha_bits", "16"]);
            }
            if format == ExportFormat::ProRes422Hq {
                // 4:2:2 shares color between horizontal pixel pairs, so the
                // width must be even. Pad by one transparent column if needed.
                push(&["-vf", &format!("pad=ceil(iw/2)*2:ih,{}", to_yuv(pix_fmt))]);
            } else {
                push(&["-vf", &to_yuv(pix_fmt)]);
            }
            push(&tag_bt709);
        }
        ExportFormat::H264 => {
            push(&["-c:v", "libx264", "-preset", "slow"]);
            // CRF 16 is visually lossless for flat-color animation, where H.264
            // otherwise tends to produce banding in gradients.
            push(&["-crf", "16"]);
            // 4:2:0 halves color resolution in both directions, so both
            // dimensions must be even.
            push(&["-vf", &format!("pad=ceil(iw/2)*2:ceil(ih/2)*2,{}", to_yuv("yuv420p"))]);
            push(&tag_bt709);
            // Move the index to the front of the file so browsers can start
            // playing before the whole file has downloaded.
            push(&["-movflags", "+faststart"]);
        }
        ExportFormat::HevcWithAlpha => {
            push(&["-c:v", "hevc_videotoolbox"]);
            // Apple's encoder takes the alpha channel from BGRA input; it
            // handles the RGB→YUV conversion itself.
            push(&["-pix_fmt", "bgra", "-alpha_quality", "0.9", "-q:v", "75"]);
            // "hvc1" is the tag Apple's players require for HEVC.
            push(&["-tag:v", "hvc1"]);
        }
        ExportFormat::PngSequence | ExportFormat::SpriteSheet => {
            unreachable!("image exports don't go through ffmpeg")
        }
    }

    if audio.is_some() {
        match format {
            // Uncompressed 24-bit audio is what editors expect alongside
            // ProRes masters.
            ExportFormat::ProRes4444 | ExportFormat::ProRes4444Xq | ExportFormat::ProRes422Hq => {
                push(&["-c:a", "pcm_s24le"]);
            }
            // AAC is what MP4 players and Apple devices play.
            _ => push(&["-c:a", "aac", "-b:a", "320k"]),
        }
        // Rounding can leave the audio a few samples longer than the
        // video; end the file with the video.
        push(&["-shortest"]);
    }

    args.push(output.as_os_str().to_owned());
    args
}

/// A temporary file that's deleted when this goes out of scope.
struct TempFile(PathBuf);

impl TempFile {
    fn new(extension: &str) -> Self {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let name = format!("inkframe-{}-{nanos}.{extension}", std::process::id());
        Self(std::env::temp_dir().join(name))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A running ffmpeg process that we feed frames to.
struct Encoder {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    stderr_reader: Option<JoinHandle<String>>,
}

impl Encoder {
    fn spawn(ffmpeg: &Path, args: &[OsString]) -> Result<Self, ExportError> {
        let mut command = Command::new(ffmpeg);
        command.args(args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        // Inkframe is a windowed program, so on Windows a console program it
        // starts gets a brand-new console window of its own, which would
        // flash up over the editor for the whole export. This flag runs
        // ffmpeg without one; its output still comes back through the pipes.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;

        // ffmpeg's stderr has to be drained continuously on its own thread.
        // If nobody reads it and ffmpeg prints enough to fill the OS pipe
        // buffer, ffmpeg blocks writing its log while we block writing frames
        // to it — a deadlock. Collecting it also gives us the error message.
        let mut stderr = child.stderr.take().expect("stderr was piped");
        let stderr_reader = std::thread::spawn(move || {
            let mut log = String::new();
            let _ = stderr.read_to_string(&mut log);
            log
        });

        let stdin = child.stdin.take();
        Ok(Self { child, stdin, stderr_reader: Some(stderr_reader) })
    }

    fn write_frame(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.stdin.as_mut().expect("frames are only written before finish()").write_all(bytes)
    }

    /// Closes the pipe so ffmpeg sees end-of-input, then waits for it to
    /// flush and finalize the file.
    fn finish(mut self) -> Result<(), ExportError> {
        drop(self.stdin.take());
        let status = self.child.wait()?;
        let log = self.stderr_log();
        if status.success() {
            Ok(())
        } else {
            let message = log.trim();
            Err(ExportError::FfmpegFailed(if message.is_empty() {
                status.to_string()
            } else {
                message.to_owned()
            }))
        }
    }

    fn kill(mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn stderr_log(&mut self) -> String {
        self.stderr_reader.take().and_then(|handle| handle.join().ok()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(format: ExportFormat, transparent: bool) -> Vec<String> {
        build_args(format, 1920, 1080, FrameRate::new(24000, 1001), transparent, None, Path::new("out.mov"))
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect()
    }

    fn value_after(args: &[String], flag: &str) -> Option<String> {
        args.iter().position(|a| a == flag).map(|i| args[i + 1].clone())
    }

    #[test]
    fn input_rate_is_an_exact_fraction() {
        let args = args_for(ExportFormat::H264, false);
        assert_eq!(value_after(&args, "-framerate").unwrap(), "24000/1001");
    }

    #[test]
    fn prores_4444_keeps_alpha_only_when_asked() {
        let with = args_for(ExportFormat::ProRes4444, true);
        assert!(value_after(&with, "-vf").unwrap().ends_with("yuva444p10le"));
        assert_eq!(value_after(&with, "-alpha_bits").unwrap(), "16");

        let without = args_for(ExportFormat::ProRes4444, false);
        assert!(value_after(&without, "-vf").unwrap().ends_with("yuv444p10le"));
        assert!(value_after(&without, "-alpha_bits").is_none());
    }

    #[test]
    fn output_path_comes_last() {
        let args = args_for(ExportFormat::ProRes422Hq, false);
        assert_eq!(args.last().unwrap(), "out.mov");
    }
}
