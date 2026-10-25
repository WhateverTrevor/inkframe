//! Inkframe's build tooling, run as `cargo xtask <task>`.
//!
//! This is the "xtask" pattern: release chores are written in Rust as a
//! small workspace binary instead of shell scripts, so they work the same on
//! every platform and anyone who can read the app's code can read these too.
//! The alias that makes `cargo xtask` work lives in `.cargo/config.toml`.

mod guide;
mod icon;
mod samples;
// Bundling uses tools that only exist on macOS (iconutil, codesign, hdiutil).
#[cfg(target_os = "macos")]
mod macos;
// Each platform's packaging uses a different subset of these helpers.
mod util;
// Building the installer needs Inno Setup, a Windows program.
#[cfg(windows)]
mod windows;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  guide [--keep-shots]       Build the guidebook PDF in target/guide/ (needs Google Chrome)
  icon                       Redraw the app icon PNGs in assets/icon/
  samples                    Rebuild the sample projects in examples/
  bundle-macos [options]     Build target/release/Inkframe.app
      --ffmpeg <path>        Bundle this ffmpeg (must be a self-contained build)
      --rhubarb <folder>     Bundle Rhubarb Lip Sync from this unpacked release
      --dmg                  Also make a disk image for distribution
  bundle-windows [options]   Build an installer in target/release/ (needs Inno Setup 6)
      --ffmpeg <path>        Bundle this ffmpeg.exe (must be a static build)
      --rhubarb <folder>     Bundle Rhubarb Lip Sync from this unpacked release
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("icon") => write_icon_pngs(),
        Some("guide") => guide::Options::parse(&args[1..]).and_then(|options| guide::build(&options)),
        Some("samples") => samples::write_all(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples")),
        #[cfg(target_os = "macos")]
        Some("bundle-macos") => macos::Options::parse(&args[1..]).and_then(|options| macos::bundle(&options)),
        #[cfg(not(target_os = "macos"))]
        Some("bundle-macos") => Err("bundle-macos has to run on a Mac".to_string()),
        #[cfg(windows)]
        Some("bundle-windows") => {
            windows::Options::parse(&args[1..]).and_then(|options| windows::bundle(&options))
        }
        #[cfg(not(windows))]
        Some("bundle-windows") => Err("bundle-windows has to run on Windows".to_string()),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// The repository root. xtask always runs from the workspace, so this is
/// simply the parent of xtask's own manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives inside the workspace").to_path_buf()
}

/// Writes the icon files that are checked into the repository: a large PNG
/// for the README, a smaller one the app loads as its window icon, and the
/// `.ico` that Windows builds embed in the executable.
fn write_icon_pngs() -> Result<(), String> {
    let dir = workspace_root().join("assets/icon");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    for size in [1024, 256] {
        let path = dir.join(format!("inkframe-{size}.png"));
        icon::render(size).save_png(&path).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    // Windows picks the closest size for each place it shows the icon.
    let path = dir.join("inkframe.ico");
    std::fs::write(&path, icon::ico(&[16, 24, 32, 48, 64, 128, 256]))
        .map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}
