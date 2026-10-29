//! Checking for a newer Inkframe: once a day at startup (unless turned
//! off in the Help menu) and from Help › Check for Updates…. When there is
//! one, a window says so, with its notes, and offers to download it, to be
//! reminded later, or to skip that version.
//!
//! The check asks GitHub for the project's latest published release, and
//! that's all it sends. It runs the system's `curl` (on every Mac, and on
//! Windows 10 and later) on a background thread rather than building a web
//! client into the app for one small request a day.
//!
//! Downloading opens the installer for this computer in the browser,
//! rather than replacing the app in place, so the person sees what they
//! get and the operating system checks it as it does any download.

use std::sync::mpsc::{self, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::i18n::{tr, tr_args};

/// The project's newest published release (drafts and pre-releases left
/// out), as GitHub describes it.
const LATEST: &str = "https://api.github.com/repos/WhateverTrevor/inkframe/releases/latest";
/// How long between automatic checks.
const DAY_SECONDS: u64 = 24 * 60 * 60;

/// A release newer than this build.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Its version, without the tag's `v`.
    pub version: String,
    /// Its page on GitHub.
    pub page: String,
    /// The installer for this computer, if the release has one.
    pub installer: Option<String>,
    /// Its notes, as written for the release.
    pub notes: String,
}

/// What a check found.
type Found = Result<Option<Release>, String>;

/// The checks and what they found, and the window that shows it.
#[derive(Default)]
pub struct Updates {
    /// A check under way, and whether it was asked for (so its result is
    /// shown even if there's nothing new).
    running: Option<(Receiver<Found>, bool)>,
    /// A newer release to show.
    found: Option<Release>,
    /// After a check asked for: that there's nothing new, or why it failed.
    message: Option<String>,
}

/// The update settings kept for next time (see `prefs.rs`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpdatePrefs {
    pub automatic: bool,
    /// When the last automatic check was, in seconds since 1970.
    pub checked_at: Option<u64>,
    /// A version the person chose to skip.
    pub skipped: Option<String>,
}

impl Updates {
    /// Starts a check on a background thread: `asked` for one from the
    /// menu, otherwise the automatic one.
    pub fn check(&mut self, asked: bool) {
        if self.running.is_some() {
            return;
        }
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(fetch());
        });
        self.running = Some((receive, asked));
        self.message = asked.then(|| tr("Checking for updates…").to_owned());
    }

    /// Shows `release` as if a check had found it: for screenshots.
    pub fn found(&mut self, release: Release) {
        self.found = Some(release);
    }

    /// The automatic check, if it's on and a day has passed since the last.
    pub fn check_if_due(&mut self, prefs: &mut UpdatePrefs) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if prefs.automatic && prefs.checked_at.is_none_or(|at| now.saturating_sub(at) >= DAY_SECONDS) {
            prefs.checked_at = Some(now);
            self.check(false);
        }
    }

    /// Picks up a finished check, and shows the window if there's anything
    /// to show.
    pub fn show(&mut self, ctx: &egui::Context, prefs: &mut UpdatePrefs) {
        if let Some((receive, asked)) = &self.running {
            match receive.try_recv() {
                Ok(found) => {
                    let asked = *asked;
                    self.running = None;
                    match found {
                        // A skipped version is only brought up when asked for.
                        Ok(Some(release)) if asked || prefs.skipped.as_deref() != Some(&release.version) => {
                            self.message = None;
                            self.found = Some(release);
                        }
                        Ok(_) => {
                            self.message = asked.then(|| {
                                tr_args(
                                    "Inkframe {version} is the newest.",
                                    &[("version", env!("CARGO_PKG_VERSION"))],
                                )
                            });
                        }
                        Err(why) => {
                            self.message =
                                asked.then(|| tr_args("Couldn't check for updates: {why}", &[("why", &why)]));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(250))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.running = None,
            }
        }
        if let Some(release) = self.found.clone() {
            self.show_release(ctx, &release, prefs);
        } else if let Some(message) = self.message.clone() {
            let mut open = true;
            egui::Window::new(tr("Check for Updates"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(&message);
                    if self.running.is_none() && ui.button(tr("OK")).clicked() {
                        self.message = None;
                    }
                });
            if !open {
                self.message = None;
            }
        }
    }

    fn show_release(&mut self, ctx: &egui::Context, release: &Release, prefs: &mut UpdatePrefs) {
        let mut open = true;
        let mut done = false;
        egui::Window::new(tr("Update Available"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.heading(tr_args("Inkframe {version} is out", &[("version", &release.version)]));
                ui.label(tr_args("You have {version}.", &[("version", env!("CARGO_PKG_VERSION"))]));
                let notes = crate::whats_new::parts_of(&release.notes);
                if !notes.is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| crate::whats_new::show_parts(ui, &notes));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let download = ui.button(tr("Download")).on_hover_text(tr(
                        "Download the installer in your browser; install it as you did this one",
                    ));
                    if download.clicked() {
                        crate::app::open_in_browser(release.installer.as_deref().unwrap_or(&release.page));
                        done = true;
                    }
                    if ui.button(tr("Later")).on_hover_text(tr("Ask again tomorrow")).clicked() {
                        done = true;
                    }
                    if ui.button(tr("Skip This Version")).clicked() {
                        prefs.skipped = Some(release.version.clone());
                        done = true;
                    }
                    if ui.link(tr("What's in it")).clicked() {
                        crate::app::open_in_browser(&release.page);
                    }
                });
            });
        if done || !open {
            self.found = None;
        }
    }
}

/// Asks GitHub for the latest release, and makes it a [`Release`] if it's
/// newer than this build.
fn fetch() -> Found {
    let output = std::process::Command::new("curl")
        .args(["--silent", "--show-error", "--fail", "--location", "--max-time", "15"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .args(["--user-agent", concat!("Inkframe/", env!("CARGO_PKG_VERSION"))])
        .arg(LATEST)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if why.is_empty() { tr("no answer from GitHub").to_owned() } else { why });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let release = parse_release(&text).ok_or_else(|| tr("GitHub's answer didn't make sense").to_owned())?;
    Ok(is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(release))
}

/// A release from GitHub's description of it.
fn parse_release(json: &str) -> Option<Release> {
    let data: serde_json::Value = serde_json::from_str(json).ok()?;
    let version = data["tag_name"].as_str()?.trim_start_matches('v').to_owned();
    let installer = data["assets"].as_array().and_then(|assets| {
        assets.iter().find_map(|asset| {
            let name = asset["name"].as_str()?;
            let url = asset["browser_download_url"].as_str()?;
            is_installer_for_here(name).then(|| url.to_owned())
        })
    });
    Some(Release {
        version,
        page: data["html_url"].as_str()?.to_owned(),
        installer,
        notes: data["body"].as_str().unwrap_or_default().to_owned(),
    })
}

/// Whether a release file is the installer for this computer, by the names
/// the release build gives them (see `.github/workflows`).
fn is_installer_for_here(name: &str) -> bool {
    if cfg!(target_os = "macos") {
        name.ends_with("-macos-arm64.dmg")
    } else if cfg!(windows) {
        name.ends_with("-windows-x64-setup.exe")
    } else {
        false
    }
}

/// Whether `candidate` is a later version than `current`, comparing their
/// numbers (`0.10.0` is after `0.9.2`).
fn is_newer(candidate: &str, current: &str) -> bool {
    let numbers = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+']).take(3).map(|part| part.parse().unwrap_or(0)).collect()
    };
    numbers(candidate) > numbers(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(is_newer("0.10.0", "0.9.2"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.8.0", "0.8.0"));
        assert!(!is_newer("0.7.1", "0.8.0"));
    }

    #[test]
    fn a_release_is_read_with_the_installer_for_this_computer() {
        let json = r#"{
            "tag_name": "v9.1.0",
            "html_url": "https://github.com/WhateverTrevor/inkframe/releases/tag/v9.1.0",
            "body": "Faster brushes.",
            "assets": [
                { "name": "Inkframe-9.1.0-macos-arm64.dmg", "browser_download_url": "https://example.com/mac.dmg" },
                { "name": "Inkframe-9.1.0-windows-x64-setup.exe", "browser_download_url": "https://example.com/win.exe" },
                { "name": "Inkframe-for-Artists.pdf", "browser_download_url": "https://example.com/guide.pdf" }
            ]
        }"#;
        let release = parse_release(json).unwrap();
        assert_eq!(release.version, "9.1.0");
        assert_eq!(release.notes, "Faster brushes.");
        let expected = if cfg!(target_os = "macos") {
            Some("https://example.com/mac.dmg")
        } else if cfg!(windows) {
            Some("https://example.com/win.exe")
        } else {
            None
        };
        assert_eq!(release.installer.as_deref(), expected);
        assert!(parse_release("{}").is_none());
    }

    #[test]
    fn the_automatic_check_waits_a_day_and_can_be_turned_off() {
        let mut updates = Updates::default();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut prefs = UpdatePrefs { automatic: false, checked_at: None, skipped: None };
        updates.check_if_due(&mut prefs);
        assert!(updates.running.is_none(), "off");
        prefs = UpdatePrefs { automatic: true, checked_at: Some(now - 60), skipped: None };
        updates.check_if_due(&mut prefs);
        assert!(updates.running.is_none(), "checked a minute ago");
    }
}
