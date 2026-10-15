//! Packaging Inkframe as a Windows installer.
//!
//! The release files are gathered into a staging folder:
//!
//! ```text
//! inkframe.exe        the editor, with its icon embedded (see inkframe-app/build.rs)
//! ffmpeg.exe          optional; found by inkframe-export's `locate`
//! rhubarb\            optional; Rhubarb Lip Sync and its speech models
//! licenses\           Inkframe's licenses (and ffmpeg's and Rhubarb's)
//! ```
//!
//! [Inno Setup](https://jrsoftware.org/isinfo.php) then turns that folder
//! into a single `setup.exe`, following `packaging/windows/inkframe.iss`.
//! The installer and the uninstaller it leaves behind are Inno Setup's;
//! this task just feeds it the files and the version number.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{
    build_release, bundle_ffmpeg, bundle_rhubarb, copy, copy_licenses, fresh_dir, remove_dir, run,
};
use crate::workspace_root;

pub struct Options {
    ffmpeg: Option<PathBuf>,
    rhubarb: Option<PathBuf>,
}

impl Options {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self { ffmpeg: None, rhubarb: None };
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
                other => return Err(format!("unknown option {other:?}\n\n{}", crate::USAGE)),
            }
        }
        Ok(options)
    }
}

pub fn bundle(options: &Options) -> Result<(), String> {
    let root = workspace_root();
    let version = env!("CARGO_PKG_VERSION");
    // The installer script only allows x64 for now; Windows on Arm runs x64
    // programs through emulation.
    if !cfg!(target_arch = "x86_64") {
        return Err("bundle-windows only builds x64 installers so far".into());
    }

    // Check the tools first, so a missing one fails in a second, not after a build.
    let iscc = find_inno_setup()?;
    if let Some(ffmpeg) = &options.ffmpeg {
        check_self_contained(ffmpeg)?;
    }

    build_release(&root)?;

    let release = root.join("target").join("release");
    let staging = release.join("windows-staging");
    fresh_dir(&staging)?;
    fresh_dir(&staging.join("licenses"))?;
    copy(&release.join("inkframe.exe"), &staging.join("inkframe.exe"))?;
    copy_licenses(&root, &staging.join("licenses"))?;
    if let Some(ffmpeg) = &options.ffmpeg {
        bundle_ffmpeg(ffmpeg, &staging.join("ffmpeg.exe"), &staging.join("licenses").join("ffmpeg"))?;
    }
    if let Some(rhubarb) = &options.rhubarb {
        bundle_rhubarb(rhubarb, &staging.join("rhubarb"), &staging.join("licenses").join("rhubarb"))?;
    }

    // Inno Setup's `/D` defines fill in the `{#Name}` placeholders in the script.
    let output_name = format!("Inkframe-{version}-windows-x64-setup");
    let define = |name: &str, value: &Path| {
        let mut arg = std::ffi::OsString::from(format!("/D{name}="));
        arg.push(value);
        arg
    };
    run(Command::new(&iscc)
        .arg("/Qp") // quiet, apart from progress and errors
        .arg(format!("/DAppVersion={version}"))
        .arg(define("SourceDir", &staging))
        .arg(define("IconFile", &root.join("assets").join("icon").join("inkframe.ico")))
        .arg(define("OutputDir", &release))
        .arg(format!("/DOutputName={output_name}"))
        .arg(root.join("packaging").join("windows").join("inkframe.iss")))?;
    remove_dir(&staging)?;

    println!("Built {}", release.join(format!("{output_name}.exe")).display());
    Ok(())
}

/// Finds Inno Setup's command-line compiler. It isn't usually on `PATH`, so
/// also look where its installer puts it: Program Files for an install for
/// all users, or the user's own Programs folder otherwise.
fn find_inno_setup() -> Result<PathBuf, String> {
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let installed = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|base| {
            let base = PathBuf::from(base);
            // Per-user installs go under %LOCALAPPDATA%\Programs.
            if base.ends_with("Local") {
                base.join("Programs").join("Inno Setup 6")
            } else {
                base.join("Inno Setup 6")
            }
        });
    on_path
        .into_iter()
        .chain(installed)
        .map(|dir| dir.join("ISCC.exe"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "couldn't find Inno Setup 6 (ISCC.exe). Install it from https://jrsoftware.org/isdl.php \
             or with `winget install JRSoftware.InnoSetup`."
                .to_string()
        })
}

/// Refuses an ffmpeg that comes with its own DLLs.
///
/// Windows ffmpeg downloads come in two flavours: "static", one large
/// `ffmpeg.exe`, and "shared", a small `ffmpeg.exe` that needs `avcodec-*.dll`
/// and friends from the same folder. Only `ffmpeg.exe` is copied into the
/// installer, so a shared build would fail to start on the user's machine.
/// The DLLs sitting beside it give a shared build away.
fn check_self_contained(ffmpeg: &Path) -> Result<(), String> {
    if !ffmpeg.is_file() {
        return Err(format!("no ffmpeg at {}", ffmpeg.display()));
    }
    let folder = ffmpeg.parent().unwrap_or(Path::new("."));
    let entries =
        std::fs::read_dir(folder).map_err(|e| format!("couldn't list {}: {e}", folder.display()))?;
    let libraries: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_lowercase())
        .filter(|name| name.starts_with("av") && name.ends_with(".dll"))
        .collect();
    if libraries.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} looks like a \"shared\" ffmpeg build that needs the DLLs beside it ({}).\n\
         Use a static build instead (see packaging/windows/README.md).",
        ffmpeg.display(),
        libraries.join(", ")
    ))
}
