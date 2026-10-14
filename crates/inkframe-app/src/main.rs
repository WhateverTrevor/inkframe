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
            .with_min_inner_size([960.0, 600.0])
            .with_icon(window_icon()),
        ..Default::default()
    };

    eframe::run_native("Inkframe", options, Box::new(|cc| Ok(Box::new(app::InkframeApp::new(cc, open_path)))))
}

/// The icon shown in the Dock or taskbar while Inkframe runs. A packaged
/// macOS app also has an `.icns` file, but setting it here covers
/// `cargo run` and Windows too. The PNG is drawn by `cargo xtask icon`.
fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon/inkframe-256.png"))
        .expect("the bundled icon is a valid PNG")
}
