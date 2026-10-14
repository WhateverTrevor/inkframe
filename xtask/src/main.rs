//! Inkframe's build tooling, run as `cargo xtask <task>`.
//!
//! This is the "xtask" pattern: release chores are written in Rust as a
//! small workspace binary instead of shell scripts, so they work the same on
//! every platform and anyone who can read the app's code can read these too.
//! The alias that makes `cargo xtask` work lives in `.cargo/config.toml`.

mod icon;
// Bundling uses tools that only exist on macOS (iconutil, codesign, hdiutil).
#[cfg(target_os = "macos")]
mod macos;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask <task>

Tasks:
  icon                       Redraw the app icon PNGs in assets/icon/
  bundle-macos [options]     Build target/release/Inkframe.app
      --ffmpeg <path>        Bundle this ffmpeg (must be a self-contained build)
      --dmg                  Also make a disk image for distribution
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("icon") => write_icon_pngs(),
        #[cfg(target_os = "macos")]
        Some("bundle-macos") => macos::Options::parse(&args[1..]).and_then(|options| macos::bundle(&options)),
        #[cfg(not(target_os = "macos"))]
        Some("bundle-macos") => Err("bundle-macos has to run on a Mac".to_string()),
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

/// Writes the icon PNGs that are checked into the repository: a large one
/// for the README and a smaller one the app loads as its window icon.
fn write_icon_pngs() -> Result<(), String> {
    let dir = workspace_root().join("assets/icon");
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    for size in [1024, 256] {
        let path = dir.join(format!("inkframe-{size}.png"));
        icon::render(size).save_png(&path).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
