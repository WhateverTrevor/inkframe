//! Packaging Inkframe as a macOS app.
//!
//! A `.app` is just a folder with a fixed layout that Finder shows as a
//! single file:
//!
//! ```text
//! Inkframe.app/
//!   Contents/
//!     Info.plist          name, version, icon file, bundle id
//!     MacOS/
//!       inkframe          the executable
//!       ffmpeg            optional; found by inkframe-export's `locate`
//!     Resources/
//!       Inkframe.icns     the icon at every size macOS asks for
//!       rhubarb/          optional; Rhubarb Lip Sync and its speech models
//!       licenses/         Inkframe's licenses (and ffmpeg's and Rhubarb's)
//!
//! Rhubarb lives in Resources rather than beside the executable because it
//! reads data files from `res/` next to itself, and `codesign` takes
//! everything in `MacOS/` for code.
//! ```
//!
//! The bundle is signed "ad hoc" (with no identity). Apple Silicon refuses to
//! run unsigned code at all, and once the bundle's files are rearranged the
//! signature the linker gave the executable no longer covers the whole app.
//! An ad-hoc signature runs fine on the Mac that built it. Other Macs will
//! warn that the developer can't be verified until the app is signed with a
//! Developer ID and notarized, which needs an Apple Developer account.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{
    build_release, bundle_ffmpeg, bundle_rhubarb, copy, copy_licenses, fresh_dir, output, read, remove_dir,
    run, write,
};
use crate::{icon, workspace_root};

pub struct Options {
    ffmpeg: Option<PathBuf>,
    rhubarb: Option<PathBuf>,
    dmg: bool,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self { ffmpeg: None, rhubarb: None, dmg: false };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ffmpeg" => {
                    let path = args.next().ok_or("--ffmpeg needs a path")?;
                    options.ffmpeg = Some(PathBuf::from(path));
                }
                "--rhubarb" => {
                    let path = args.next().ok_or("--rhubarb needs a path")?;
                    options.rhubarb = Some(PathBuf::from(path));
                }
                "--dmg" => options.dmg = true,
                other => return Err(format!("unknown option {other:?}\n\n{}", crate::USAGE)),
            }
        }
        Ok(options)
    }
}

pub fn bundle(options: &Options) -> Result<(), String> {
    let root = workspace_root();
    let version = env!("CARGO_PKG_VERSION");

    // Check ffmpeg first, so a bad path fails in a second, not after a build.
    if let Some(ffmpeg) = &options.ffmpeg {
        check_self_contained(ffmpeg)?;
    }

    build_release(&root)?;

    let release = root.join("target/release");
    let app = release.join("Inkframe.app");
    let contents = app.join("Contents");
    let macos_dir = contents.join("MacOS");
    let resources = contents.join("Resources");
    fresh_dir(&app)?;
    for dir in [&macos_dir, &resources.join("licenses")] {
        fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    }

    copy(&release.join("inkframe"), &macos_dir.join("inkframe"))?;
    let plist = read(&root.join("packaging/macos/Info.plist"))?.replace("{{VERSION}}", version);
    write(&contents.join("Info.plist"), plist.as_bytes())?;
    make_icns(&release, &resources.join("Inkframe.icns"))?;
    copy_licenses(&root, &resources.join("licenses"))?;

    if let Some(ffmpeg) = &options.ffmpeg {
        bundle_ffmpeg(ffmpeg, &macos_dir.join("ffmpeg"), &resources.join("licenses/ffmpeg"))?;
    }
    if let Some(rhubarb) = &options.rhubarb {
        bundle_rhubarb(rhubarb, &resources.join("rhubarb"), &resources.join("licenses/rhubarb"))?;
    }

    // Downloaded files (Rhubarb's, ffmpeg's) carry extended attributes such
    // as the quarantine flag, and copies keep them; `codesign` refuses a
    // bundle with any, so clear them all first.
    run(Command::new("xattr").arg("-cr").arg(&app))?;

    // Sign from the inside out: a bundle's signature records the signatures
    // of the code inside it, so nested executables must be signed first.
    if options.ffmpeg.is_some() {
        codesign(&macos_dir.join("ffmpeg"))?;
    }
    if options.rhubarb.is_some() {
        codesign(&resources.join("rhubarb/rhubarb"))?;
    }
    codesign(&app)?;
    println!("Built {}", app.display());

    if options.dmg {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { std::env::consts::ARCH };
        let dmg = release.join(format!("Inkframe-{version}-macos-{arch}.dmg"));
        make_dmg(&release, &app, &dmg)?;
        println!("Built {}", dmg.display());
    }
    Ok(())
}

/// Refuses an ffmpeg that loads libraries from outside macOS itself.
///
/// Homebrew's ffmpeg, for one, is split into a dozen libraries in
/// `/opt/homebrew`. Copied into the app it would run on this Mac and crash
/// on any Mac without Homebrew, which is exactly the Mac a bundled ffmpeg is
/// for. `otool -L` lists the libraries a program loads; anything outside
/// `/System` and `/usr/lib` (both part of every macOS install) won't be there.
fn check_self_contained(ffmpeg: &Path) -> Result<(), String> {
    if !ffmpeg.is_file() {
        return Err(format!("no ffmpeg at {}", ffmpeg.display()));
    }
    let listing = output(Command::new("otool").arg("-L").arg(ffmpeg))?;
    let outside: Vec<&str> = listing
        .lines()
        .skip(1) // otool's first line repeats the file name
        .filter_map(|line| line.split_whitespace().next())
        .filter(|lib| !lib.starts_with("/System/") && !lib.starts_with("/usr/lib/"))
        .collect();
    if outside.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} isn't self-contained; it loads libraries other Macs won't have:\n  {}\n\n\
         Use a static ffmpeg build instead (see packaging/macos/README.md).",
        ffmpeg.display(),
        outside.join("\n  ")
    ))
}

/// Builds the `.icns` file. macOS picks whichever size suits the context
/// (Finder list view, Dock, Get Info), so each is rendered from the vector
/// drawing rather than scaled from one bitmap. `iconutil` expects a folder of
/// PNGs with these exact names.
fn make_icns(scratch: &Path, icns: &Path) -> Result<(), String> {
    let iconset = scratch.join("Inkframe.iconset");
    fresh_dir(&iconset)?;
    for points in [16, 32, 128, 256, 512] {
        for (scale, suffix) in [(1, ""), (2, "@2x")] {
            let path = iconset.join(format!("icon_{points}x{points}{suffix}.png"));
            icon::render(points * scale)
                .save_png(&path)
                .map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        }
    }
    run(Command::new("iconutil").args(["--convert", "icns", "--output"]).arg(icns).arg(&iconset))?;
    remove_dir(&iconset)
}

/// Signs ad hoc (`--sign -`). Part B of packaging swaps this for a
/// Developer ID with the hardened runtime, which notarization requires.
fn codesign(path: &Path) -> Result<(), String> {
    run(Command::new("codesign").args(["--force", "--sign", "-"]).arg(path))
}

/// A compressed disk image holding the app next to a link to /Applications,
/// so installing is a single drag.
fn make_dmg(scratch: &Path, app: &Path, dmg: &Path) -> Result<(), String> {
    let staging = scratch.join("dmg-staging");
    fresh_dir(&staging)?;
    // `ditto` copies a bundle exactly, keeping the extended attributes and
    // symlinks the code signature depends on.
    run(Command::new("ditto").arg(app).arg(staging.join("Inkframe.app")))?;
    std::os::unix::fs::symlink("/Applications", staging.join("Applications"))
        .map_err(|e| format!("couldn't link /Applications: {e}"))?;
    run(Command::new("hdiutil")
        .args(["create", "-volname", "Inkframe", "-format", "UDZO", "-ov", "-srcfolder"])
        .arg(&staging)
        .arg(dmg))?;
    remove_dir(&staging)
}
