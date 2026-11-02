//! Packaging Inkframe as a Linux AppImage.
//!
//! An AppImage is a single executable file that carries a compressed folder
//! (the "AppDir") and mounts it when run, so it works on most distributions
//! without installing anything. The AppDir is laid out like the Windows
//! staging folder, everything beside the executable:
//!
//! ```text
//! Inkframe.AppDir/
//!   AppRun              what the AppImage starts: a link to `inkframe`
//!   inkframe            the editor
//!   inkframe.desktop    name, icon and categories, for menus and launchers
//!   inkframe.png        the icon
//!   .DirIcon            a link to the icon, which file managers show
//!   ffmpeg              optional; found by inkframe-export's `locate`
//!   rhubarb/            optional; Rhubarb Lip Sync and its speech models
//!   licenses/           Inkframe's licenses (and ffmpeg's and Rhubarb's)
//!   Examples/           the sample projects, for File › Open Example
//! ```
//!
//! [appimagetool](https://github.com/AppImage/appimagetool) packs the folder
//! into the AppImage, in front of a small "runtime" program that mounts it.
//! appimagetool would download the latest runtime itself; `--runtime`
//! passes one that's been checked instead, so a build is repeatable.
//!
//! Only Inkframe's own executable goes in, not the system libraries it
//! loads (the C library, the sound and graphics drivers): those are on every
//! desktop, and a bundled copy of a graphics driver is worse than none.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{
    build_release, bundle_ffmpeg, bundle_rhubarb, copy, copy_examples, copy_licenses, fresh_dir, output,
    read, remove_dir, run, write,
};
use crate::{icon, workspace_root};

pub struct Options {
    ffmpeg: Option<PathBuf>,
    rhubarb: Option<PathBuf>,
    appimagetool: Option<PathBuf>,
    runtime: Option<PathBuf>,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self { ffmpeg: None, rhubarb: None, appimagetool: None, runtime: None };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let mut path =
                |name: &str| args.next().map(PathBuf::from).ok_or_else(|| format!("{name} needs a path"));
            match arg.as_str() {
                "--ffmpeg" => options.ffmpeg = Some(path("--ffmpeg")?),
                "--rhubarb" => options.rhubarb = Some(path("--rhubarb")?),
                "--appimagetool" => options.appimagetool = Some(path("--appimagetool")?),
                "--runtime" => options.runtime = Some(path("--runtime")?),
                other => return Err(format!("unknown option {other:?}\n\n{}", crate::USAGE)),
            }
        }
        Ok(options)
    }
}

pub fn bundle(options: &Options) -> Result<(), String> {
    let root = workspace_root();
    let version = env!("CARGO_PKG_VERSION");
    // The release build names its file for x86_64, and the bundled ffmpeg
    // and Rhubarb are x86_64 programs.
    if !cfg!(target_arch = "x86_64") {
        return Err("bundle-linux only builds x86_64 AppImages so far".into());
    }

    // Check the tools first, so a missing one fails in a second, not after a build.
    let appimagetool = options.appimagetool.clone().map_or_else(find_appimagetool, Ok)?;
    if let Some(ffmpeg) = &options.ffmpeg {
        check_self_contained(ffmpeg)?;
    }

    build_release(&root)?;

    let release = root.join("target/release");
    let app_dir = release.join("Inkframe.AppDir");
    fresh_dir(&app_dir)?;
    fresh_dir(&app_dir.join("licenses"))?;
    copy(&release.join("inkframe"), &app_dir.join("inkframe"))?;
    symlink("inkframe", &app_dir.join("AppRun"))?;
    let desktop = read(&root.join("packaging/linux/inkframe.desktop"))?.replace("{{VERSION}}", version);
    write(&app_dir.join("inkframe.desktop"), desktop.as_bytes())?;
    let png = app_dir.join("inkframe.png");
    icon::render(256).save_png(&png).map_err(|e| format!("couldn't write {}: {e}", png.display()))?;
    symlink("inkframe.png", &app_dir.join(".DirIcon"))?;
    copy_licenses(&root, &app_dir.join("licenses"))?;
    copy_examples(&root, &app_dir.join("Examples"))?;
    if let Some(ffmpeg) = &options.ffmpeg {
        bundle_ffmpeg(ffmpeg, &app_dir.join("ffmpeg"), &app_dir.join("licenses/ffmpeg"))?;
    }
    if let Some(rhubarb) = &options.rhubarb {
        bundle_rhubarb(rhubarb, &app_dir.join("rhubarb"), &app_dir.join("licenses/rhubarb"))?;
    }

    let appimage = release.join(format!("Inkframe-{version}-linux-x86_64.AppImage"));
    let mut pack = Command::new(&appimagetool);
    // appimagetool asks which architecture the AppDir is for when it
    // can't tell; say so up front.
    pack.env("ARCH", "x86_64");
    if let Some(runtime) = &options.runtime {
        pack.arg("--runtime-file").arg(runtime);
    }
    run(pack.arg(&app_dir).arg(&appimage))?;
    remove_dir(&app_dir)?;

    println!("Built {}", appimage.display());
    Ok(())
}

/// Finds appimagetool on `PATH`, under either of the names it's usually
/// given: as downloaded, or renamed to drop the architecture.
fn find_appimagetool() -> Result<PathBuf, String> {
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    on_path
        .iter()
        .flat_map(|dir| [dir.join("appimagetool"), dir.join("appimagetool-x86_64.AppImage")])
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "couldn't find appimagetool. Download it from https://github.com/AppImage/appimagetool/releases \
             and put it on PATH, or pass it with --appimagetool."
                .to_string()
        })
}

/// Refuses an ffmpeg that loads libraries a fresh desktop may not have.
///
/// A distribution's own ffmpeg is split into libavcodec and friends, which
/// come and go with that distribution's packages: copied into the AppImage
/// it would run here and fail on a computer without them. `ldd` lists what
/// a program loads; a static build loads nothing, or (as most do) only the
/// C library's own pieces, which every Linux has.
fn check_self_contained(ffmpeg: &Path) -> Result<(), String> {
    if !ffmpeg.is_file() {
        return Err(format!("no ffmpeg at {}", ffmpeg.display()));
    }
    // `ldd` exits with an error for a fully static program, so its output
    // can't be relied on there; ask `file` first.
    let kind = output(Command::new("file").arg("--brief").arg(ffmpeg))?;
    if kind.contains("statically linked") || kind.contains("static-pie linked") {
        return Ok(());
    }
    let listing = output(Command::new("ldd").arg(ffmpeg))?;
    let base = ["linux-vdso", "libc.so", "libm.so", "libdl.so", "libpthread.so", "librt.so", "ld-linux"];
    let outside: Vec<&str> = listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|lib| !base.iter().any(|b| lib.contains(b)))
        .collect();
    if outside.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} isn't self-contained; it loads libraries other computers may not have:\n  {}\n\n\
         Use a static ffmpeg build instead (see packaging/linux/README.md).",
        ffmpeg.display(),
        outside.join("\n  ")
    ))
}

fn symlink(target: &str, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| format!("couldn't link {}: {e}", link.display()))
}
