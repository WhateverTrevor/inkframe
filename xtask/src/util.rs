//! Steps shared by the macOS and Windows packaging tasks, plus thin wrappers
//! around file and process calls that turn their errors into messages
//! saying which file or command failed.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Builds the editor in release mode.
pub fn build_release(root: &Path) -> Result<(), String> {
    println!("Building the release executable…");
    // Cargo tells the programs it runs where it is; use the same one.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    run(Command::new(cargo).current_dir(root).args(["build", "--release", "--package", "inkframe-app"]))
}

/// Copies Inkframe's own licence files into `dir`, and the licence of the
/// emoji font built into it (the SIL Open Font License asks for its notice
/// to travel with the font).
pub fn copy_licenses(root: &Path, dir: &Path) -> Result<(), String> {
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copy(&root.join(license), &dir.join(license))?;
    }
    copy(&root.join("assets/fonts/Noto-COLRv1-LICENSE.txt"), &dir.join("Noto-Color-Emoji-OFL.txt"))
}

/// Copies ffmpeg in, along with its licence.
///
/// ffmpeg's licence depends on how it was built (LGPL, or GPL when it
/// includes x264), so rather than guess, ask the binary itself: `ffmpeg -L`
/// prints the licence it was built under, and `-version` records exactly
/// which build this is, so its source can be found.
pub fn bundle_ffmpeg(ffmpeg: &Path, destination: &Path, license_dir: &Path) -> Result<(), String> {
    copy(ffmpeg, destination)?;
    fresh_dir(license_dir)?;
    let license = output(Command::new(ffmpeg).args(["-hide_banner", "-L"]))?;
    write(&license_dir.join("LICENSE.txt"), license.as_bytes())?;
    let version = output(Command::new(ffmpeg).args(["-hide_banner", "-version"]))?;
    let note = format!(
        "Inkframe runs this ffmpeg as a separate program to encode video.\n\
         Source code for FFmpeg is available at https://ffmpeg.org/download.html\n\n{version}"
    );
    write(&license_dir.join("VERSION.txt"), note.as_bytes())
}

/// Empties `dir`, creating it if needed. Packaging starts each output from
/// nothing, so files from an earlier run (an ffmpeg that's no longer
/// wanted, say) can't linger.
pub fn fresh_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| format!("couldn't clear {}: {e}", dir.display()))?;
    }
    fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))
}

pub fn remove_dir(dir: &Path) -> Result<(), String> {
    fs::remove_dir_all(dir).map_err(|e| format!("couldn't clean up {}: {e}", dir.display()))
}

pub fn run(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("couldn't run {command:?}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{command:?} failed ({status})")) }
}

/// Runs a command and returns what it printed.
pub fn output(command: &mut Command) -> Result<String, String> {
    let out = command.output().map_err(|e| format!("couldn't run {command:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{command:?} failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn copy(from: &Path, to: &Path) -> Result<(), String> {
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("couldn't copy {} to {}: {e}", from.display(), to.display()))
}

pub fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("couldn't read {}: {e}", path.display()))
}

pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}
