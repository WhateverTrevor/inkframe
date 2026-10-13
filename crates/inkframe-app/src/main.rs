//! Inkframe: an open-source 2D animation editor.
//!
//! Usage: `inkframe [file.inkf]`

mod app;
mod audio_engine;
mod convert;
mod editor;
mod export_dialog;
mod handles;
mod inspector;
mod library;
mod pen;
mod screenshot;
mod shortcuts;
mod stage;
mod subselect;
mod timeline;
mod toolbar;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let open_path = std::env::args_os().nth(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Inkframe")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native("Inkframe", options, Box::new(|cc| Ok(Box::new(app::InkframeApp::new(cc, open_path)))))
}
