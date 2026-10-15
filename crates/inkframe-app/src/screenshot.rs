//! Development helper for capturing the editor window without a human.
//!
//! ```sh
//! INKFRAME_SCREENSHOT=window.png inkframe examples/bouncing-ball.inkf
//! ```
//!
//! saves a PNG of the whole window once the UI has settled, then quits.
//! Useful for README images, bug reports and checking UI changes in CI.
//! Optional variables set up the scene first:
//!
//! * `INKFRAME_SCREENSHOT_FRAME=12` moves the playhead (1-based, like the timeline).
//! * `INKFRAME_SCREENSHOT_SELECT=Ball` selects the layer with that name (or `Ball,Floor` for several).
//! * `INKFRAME_SCREENSHOT_ONION=1` turns on onion skinning.
//! * `INKFRAME_SCREENSHOT_TOOL=Subselection` picks a tool by its toolbar name.
//! * `INKFRAME_SCREENSHOT_EXPAND=1` expands every layer's property rows in the timeline.
//! * `INKFRAME_SCREENSHOT_EDIT=Ball` opens the named symbol on its own (before `_SELECT`).
//! * `INKFRAME_SCREENSHOT_OPEN=Ball` opens the named instance layer's symbol in place (after `_FRAME`).
//! * `INKFRAME_SCREENSHOT_TYPE=Title` starts typing into the named text layer, with its text selected.
//! * `INKFRAME_SCREENSHOT_GRAPH=1` shows the value graph in the timeline.
//! * `INKFRAME_SCREENSHOT_LIPSYNC=1` opens the Lip Sync window for the selected layer;
//!   `=apply` also applies it from loudness once the sound has loaded.

use std::path::PathBuf;

use crate::editor::{Editor, Tool};

/// UI frames to wait before capturing, so layout and textures have settled.
const SETTLE_FRAMES: u32 = 10;

pub struct AutoScreenshot {
    path: PathBuf,
    frames_until_capture: u32,
    requested: bool,
}

impl AutoScreenshot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("INKFRAME_SCREENSHOT")?;
        Some(Self { path: path.into(), frames_until_capture: SETTLE_FRAMES, requested: false })
    }

    /// Applies the optional scene-setup variables to the editor.
    pub fn set_up_scene(editor: &mut Editor) {
        let var = |name| std::env::var(name).ok();
        if let Some(name) = var("INKFRAME_SCREENSHOT_EDIT") {
            match editor.doc.symbols.iter().find(|s| s.name == name).map(|s| s.id) {
                Some(symbol) => editor.enter_symbol(symbol, 0),
                None => eprintln!("No symbol named {name:?} for INKFRAME_SCREENSHOT_EDIT"),
            }
        }
        if let Some(shown) = var("INKFRAME_SCREENSHOT_FRAME").and_then(|v| v.parse::<u32>().ok()) {
            editor.set_frame(shown.saturating_sub(1));
        }
        if let Some(name) = var("INKFRAME_SCREENSHOT_OPEN") {
            match editor.layers().iter().find(|l| l.name == name).map(|l| l.id) {
                Some(layer) => editor.enter_instance(layer),
                None => eprintln!("No layer named {name:?} for INKFRAME_SCREENSHOT_OPEN"),
            }
        }
        if let Some(names) = var("INKFRAME_SCREENSHOT_SELECT") {
            let ids: Vec<_> = names
                .split(',')
                .filter_map(|name| editor.layers().iter().find(|l| l.name == name.trim()).map(|l| l.id))
                .collect();
            editor.select_many(&ids);
        }
        if var("INKFRAME_SCREENSHOT_ONION").is_some() {
            editor.onion.enabled = true;
        }
        if let Some(name) = var("INKFRAME_SCREENSHOT_TOOL") {
            match Tool::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(&name)) {
                Some(tool) => editor.tool = tool,
                None => eprintln!("Unknown tool {name:?} in INKFRAME_SCREENSHOT_TOOL"),
            }
        }
    }

    /// The text layer to start typing into, if any.
    pub fn type_into() -> Option<String> {
        std::env::var("INKFRAME_SCREENSHOT_TYPE").ok()
    }

    /// Whether to open the Lip Sync window.
    pub fn lip_sync() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_LIPSYNC").is_some()
    }

    /// Whether to also press its Apply button.
    pub fn lip_sync_apply() -> bool {
        std::env::var("INKFRAME_SCREENSHOT_LIPSYNC").is_ok_and(|v| v == "apply")
    }

    /// Whether the timeline should show the value graph.
    pub fn graph() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_GRAPH").is_some()
    }

    /// Whether the timeline should start with every layer expanded.
    pub fn expand_timeline() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_EXPAND").is_some()
    }

    /// Call once per UI frame.
    ///
    /// While `busy` (audio still decoding, say), capture waits, so the
    /// screenshot shows the finished state rather than a loading message.
    pub fn update(&mut self, ctx: &egui::Context, busy: bool) {
        let image = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            match save_png(&self.path, &image) {
                Ok(()) => eprintln!("Saved screenshot to {}", self.path.display()),
                Err(err) => eprintln!("Couldn't save screenshot: {err}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if self.requested {
            return;
        }
        if busy {
            ctx.request_repaint();
            return;
        }
        if self.frames_until_capture == 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested = true;
        } else {
            self.frames_until_capture -= 1;
            // egui only repaints on input; keep frames coming until we capture.
            ctx.request_repaint();
        }
    }
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    // egui and tiny-skia both store premultiplied RGBA, so the bytes carry over.
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let size = tiny_skia::IntSize::from_wh(image.width() as u32, image.height() as u32)
        .ok_or("screenshot is empty")?;
    let pixmap = tiny_skia::Pixmap::from_vec(bytes, size).ok_or("screenshot has the wrong size")?;
    pixmap.save_png(path).map_err(|e| e.to_string())
}
