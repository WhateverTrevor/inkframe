//! Inkframe: an open-source 2D animation editor.
//!
//! Usage: `inkframe [file.inkf]`, or `inkframe --export …` to export or
//! `inkframe --script …` to run a script without opening a window (see
//! `cli.rs` and `cli_script.rs`).

// On Windows, release builds are GUI programs, so launching Inkframe doesn't
// also open a console window. Debug builds keep the console for log output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adjust_dialog;
mod app;
mod audio_engine;
mod autosave;
mod batch_dialog;
mod bones_panel;
mod cli;
mod cli_script;
mod command_finder;
mod context_bar;
mod convert;
mod depth_view;
mod easing_editor;
mod editor;
mod envelope;
mod examples;
mod export_dialog;
mod frame_commands;
mod guides;
mod handles;
mod icons;
mod inside;
mod inspector;
mod layer_thumbnails;
mod layout;
mod library;
mod lip_sync_dialog;
mod motion_path;
mod motion_presets;
mod multi_edit;
mod node_view;
mod panels;
mod pen;
mod prefs;
mod rhubarb;
mod screenshot;
mod scripts;
mod shape_ops;
mod shortcuts;
mod skeleton;
mod stage;
mod subselect;
mod swap_dialog;
mod tablet;
mod templates;
mod text_edit;
mod theme;
mod timeline;
mod toolbar;
mod value_graph;
mod vectorise_dialog;
mod xsheet;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "--export") {
        std::process::exit(cli::run(args.skip(1).collect()));
    }
    if args.peek().is_some_and(|a| a == "--script") {
        std::process::exit(cli_script::run(args.skip(1).collect()));
    }
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
