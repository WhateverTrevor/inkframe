//! Running Rhubarb Lip Sync, which listens to dialogue and works out the
//! mouth shapes, for the Lip Sync window.
//!
//! Rhubarb is a separate program (MIT licensed, by Daniel Wolf) that the
//! installers bundle, like ffmpeg. It reads a WAV file and prints the mouth
//! shapes in the same tab-separated format as the timing files the window
//! can already import, so its output goes through `parse_rhubarb`.
//!
//! Recognising speech takes a few seconds per second of dialogue, so it
//! runs on its own thread, reporting progress, and can be cancelled.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use inkframe_core::{MouthCue, parse_rhubarb};

/// How Rhubarb recognises speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Recognizer {
    /// Recognises English words, which gives the best timing for English.
    #[default]
    English,
    /// Listens for sounds only, so it works for any language.
    AnyLanguage,
}

impl Recognizer {
    fn arg(self) -> &'static str {
        match self {
            Recognizer::English => "pocketSphinx",
            Recognizer::AnyLanguage => "phonetic",
        }
    }
}

/// Finds Rhubarb, in the order:
///
/// 1. `INKFRAME_RHUBARB`, if set: the path to the program.
/// 2. Bundled with Inkframe: `rhubarb/` beside the executable (Windows), or
///    in the app's Resources (macOS, where only code may sit beside it).
/// 3. `vendor/rhubarb/` in the source tree, for running from a checkout.
/// 4. Every directory on `PATH`.
///
/// Rhubarb finds its speech models in a `res` folder beside itself, so it's
/// always found as a whole folder rather than copied anywhere.
pub fn locate() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("INKFRAME_RHUBARB").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let exe_name = format!("rhubarb{}", std::env::consts::EXE_SUFFIX);
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    let bundled = exe_dir.into_iter().flat_map(|dir| [dir.join("rhubarb"), dir.join("../Resources/rhubarb")]);
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/rhubarb");
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    bundled
        .chain([checkout])
        .chain(on_path)
        .map(|dir| dir.join(&exe_name))
        .find(|candidate| candidate.is_file())
}

/// What a running job reports.
pub enum Event {
    /// How far along it is, 0–1.
    Progress(f32),
    Done(Result<Vec<MouthCue>, String>),
}

/// A Rhubarb run in progress.
pub struct Job {
    pub events: Receiver<Event>,
    /// The process, so it can be stopped. `None` once it has finished.
    child: Arc<Mutex<Option<Child>>>,
}

impl Job {
    /// Starts working out the mouth shapes for the sound file `audio`, at
    /// `fps`. `dialogue`, if given, is the text being spoken, which helps
    /// Rhubarb with unclear words.
    pub fn start(
        rhubarb: PathBuf,
        audio: PathBuf,
        fps: f64,
        recognizer: Recognizer,
        dialogue: Option<String>,
    ) -> Self {
        let (sender, events) = mpsc::channel();
        let child = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&child);
        std::thread::spawn(move || {
            let result = run(&rhubarb, &audio, fps, recognizer, dialogue.as_deref(), &shared, |progress| {
                let _ = sender.send(Event::Progress(progress));
            });
            let _ = sender.send(Event::Done(result));
        });
        Self { events, child }
    }

    /// Stops Rhubarb. The job then reports that it was cancelled.
    pub fn cancel(&self) {
        if let Some(child) = self.child.lock().expect("not poisoned").as_mut() {
            let _ = child.kill();
        }
    }
}

fn run(
    rhubarb: &Path,
    audio: &Path,
    fps: f64,
    recognizer: Recognizer,
    dialogue: Option<&str>,
    child_slot: &Mutex<Option<Child>>,
    mut on_progress: impl FnMut(f32),
) -> Result<Vec<MouthCue>, String> {
    // Rhubarb only reads WAV, so decode whatever the dialogue is (MP3, M4A…)
    // and hand it a WAV. The guards delete the temporary files afterwards.
    let clip = inkframe_audio::decode(audio).map_err(|e| format!("Couldn't read the dialogue: {e}"))?;
    let wav = TempFile::new("wav");
    clip.write_wav(&wav.0).map_err(|e| format!("Couldn't prepare the dialogue: {e}"))?;
    let text = match dialogue.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => {
            let file = TempFile::new("txt");
            std::fs::write(&file.0, text).map_err(|e| format!("Couldn't prepare the dialogue text: {e}"))?;
            Some(file)
        }
        None => None,
    };

    let mut command = Command::new(rhubarb);
    command.args(["--machineReadable", "--exportFormat", "tsv", "--recognizer", recognizer.arg()]);
    if let Some(text) = &text {
        command.arg("--dialogFile").arg(&text.0);
    }
    command.arg(&wav.0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_console_window(&mut command);
    let mut child = command.spawn().map_err(|e| spawn_error(&e))?;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    *child_slot.lock().expect("not poisoned") = Some(child);

    // The shapes come out on stdout at the end; read them on another
    // thread so a full pipe can't stall Rhubarb while we read progress.
    let output = std::thread::spawn(move || {
        let mut text = String::new();
        BufReader::new(stdout).read_to_string(&mut text).map(|_| text)
    });
    let mut failure = None;
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        match parse_status(&line) {
            Some(Status::Progress(value)) => on_progress(value),
            Some(Status::Failure(reason)) => failure = Some(reason),
            None => {}
        }
    }
    let status = child_slot.lock().expect("not poisoned").take().map(|mut c| c.wait());
    let text = output.join().map_err(|_| "Rhubarb's output was lost".to_string())?;
    match status {
        Some(Ok(status)) if status.success() => {
            let text = text.map_err(|e| format!("Couldn't read Rhubarb's output: {e}"))?;
            parse_rhubarb(&text, fps).map_err(|e| format!("Rhubarb's output didn't make sense: {e}"))
        }
        _ => Err(failure.unwrap_or_else(|| "Stopped.".to_string())),
    }
}

/// A line of Rhubarb's `--machineReadable` log that matters here.
#[derive(Debug, PartialEq)]
enum Status {
    Progress(f32),
    Failure(String),
}

/// Reads one line of Rhubarb's log, which is a JSON object per line such
/// as `{ "type": "progress", "value": 0.42, … }`.
fn parse_status(line: &str) -> Option<Status> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    match value.get("type")?.as_str()? {
        "progress" => Some(Status::Progress(value.get("value")?.as_f64()? as f32)),
        "failure" => Some(Status::Failure(value.get("reason")?.as_str()?.to_string())),
        _ => None,
    }
}

/// Explains a failure to start Rhubarb. The one worth singling out: the
/// macOS build of Rhubarb is for Intel Macs, so Apple Silicon runs it
/// through Rosetta, which not every Mac has installed.
fn spawn_error(error: &std::io::Error) -> String {
    // EBADARCH: "Bad CPU type in executable".
    if cfg!(target_os = "macos") && error.raw_os_error() == Some(86) {
        return "Rhubarb needs Rosetta, Apple's translator for Intel programs. Install it by running \
                `softwareupdate --install-rosetta` in Terminal, then try again."
            .to_string();
    }
    format!("Couldn't start Rhubarb: {error}")
}

/// Keeps a console window from flashing up on Windows while Rhubarb runs.
fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// A file in the temporary folder, deleted when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(extension: &str) -> Self {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        Self(
            std::env::temp_dir().join(format!("inkframe-rhubarb-{}-{nanos}.{extension}", std::process::id())),
        )
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn reads_progress_and_failures_from_the_log() {
        let progress = r#"{ "type": "progress", "value": 0.42, "log": { "level": "Trace", "message": "Progress: 42%" } }"#;
        assert_eq!(parse_status(progress), Some(Status::Progress(0.42)));
        let failure = r#"{ "type": "failure", "reason": "Could not open sound file.", "log": {} }"#;
        assert_eq!(parse_status(failure), Some(Status::Failure("Could not open sound file.".into())));
        assert_eq!(parse_status(r#"{ "type": "start", "file": "a.wav" }"#), None);
        assert_eq!(parse_status("not json"), None);
    }

    /// Runs the real Rhubarb on a tone, if it can be found. CI sets
    /// `INKFRAME_REQUIRE_RHUBARB` so that a missing Rhubarb fails rather
    /// than silently skipping.
    #[test]
    fn rhubarb_runs_and_reports_mouth_shapes() {
        let Some(rhubarb) = locate() else {
            assert!(std::env::var_os("INKFRAME_REQUIRE_RHUBARB").is_none(), "Rhubarb wasn't found");
            eprintln!("Rhubarb not found; skipping");
            return;
        };
        let audio = std::env::temp_dir().join(format!("inkframe-rhubarb-test-{}.wav", std::process::id()));
        write_hum(&audio);
        let job = Job::start(rhubarb, audio.clone(), 24.0, Recognizer::AnyLanguage, None);
        let mut progressed = false;
        let result = loop {
            match job.events.recv().expect("the job reports back") {
                Event::Progress(_) => progressed = true,
                Event::Done(result) => break result,
            }
        };
        let _ = std::fs::remove_file(&audio);
        let cues = result.expect("Rhubarb succeeds");
        assert!(progressed, "progress was reported");
        assert_eq!(cues.first().map(|c| c.frame), Some(0));
    }

    /// A second of silence, a second of hum, and a second of silence: not
    /// speech, but enough for Rhubarb to run on.
    pub(crate) fn write_hum(path: &Path) {
        let rate = inkframe_audio::SAMPLE_RATE as usize;
        let samples = (0..rate * 3)
            .flat_map(|i| {
                let t = i as f32 / rate as f32;
                let level = if (1.0..2.0).contains(&t) { 0.5 } else { 0.0 };
                let s = level * (t * 220.0 * std::f32::consts::TAU).sin();
                [s, s]
            })
            .collect();
        inkframe_audio::Clip { samples }.write_wav(path).expect("writes the test sound");
    }
}
