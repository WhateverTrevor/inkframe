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
//! * `INKFRAME_SCREENSHOT_EXPORT=sprite` opens the Export window set to sprite sheets
//!   (or another format by the start of its name, such as `png` or `h264`).
//! * `INKFRAME_SCREENSHOT_BATCH=1` opens Batch Export with the open project and the
//!   ones beside it, and starts exporting them to a temporary folder.
//! * `INKFRAME_SCREENSHOT_GUIDES=safe,field` shows the safe areas, the field guide, or both.
//! * `INKFRAME_SCREENSHOT_VECTORISE=ink` opens Vectorise on the selected layer (with
//!   `_PAINT`, the strokes it paints), in line-art mode, or `=colours`; `,apply` vectorises.
//! * `INKFRAME_SCREENSHOT_PARTICLES=1` adds the four particle presets on a dark stage,
//!   with the fire selected.
//! * `INKFRAME_SCREENSHOT_MULTIPLANE=1` builds a scene at several depths with the camera
//!   dollying in, and shows the Depth View in place of Properties; `=stack` also has the
//!   tree drift back past the hills, stacked by depth.
//! * `INKFRAME_SCREENSHOT_PREFS=1` loads the saved settings (which screenshots otherwise
//!   leave alone; point `HOME` somewhere else first) and opens Batch Export to show them.
//! * `INKFRAME_SCREENSHOT_RECOVERY=1` shows the Recover Unsaved Work window, for a copy
//!   of the opened project left in a scratch folder.
//! * `INKFRAME_SCREENSHOT_XSHEET=1` opens the exposure sheet.
//! * `INKFRAME_SCREENSHOT_ROTATE=30` turns the view 30° clockwise.
//! * `INKFRAME_SCREENSHOT_LAYOUT=custom` splits the stage in two and puts an exposure sheet
//!   beside the timeline, to show the layout's areas.
//! * `INKFRAME_SCREENSHOT_PAINT=1` paints a few strokes on a new paint layer, as a pen
//!   would (pressing harder through the middle of each), to show the brushes; `=tools`
//!   also uses the bucket, gradient and retouching tools on them; `=select` then selects
//!   an ellipse, shifts the hue inside it, and sets the strokes to Multiply.

use std::path::PathBuf;

use crate::editor::{Editor, Tool};

/// UI frames to wait before capturing, so layout and textures have settled.
const SETTLE_FRAMES: u32 = 10;

pub struct AutoScreenshot {
    path: PathBuf,
    frames_until_capture: u32,
    /// When the app started. Windows fade in over a moment of real time,
    /// however fast frames come, so capture also waits for `SETTLE_TIME`.
    started: std::time::Instant,
    requested: bool,
}

const SETTLE_TIME: std::time::Duration = std::time::Duration::from_millis(600);

impl AutoScreenshot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("INKFRAME_SCREENSHOT")?;
        Some(Self {
            path: path.into(),
            frames_until_capture: SETTLE_FRAMES,
            started: std::time::Instant::now(),
            requested: false,
        })
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
        if let Some(paint) = var("INKFRAME_SCREENSHOT_PAINT") {
            paint_demo(editor);
            if paint == "tools" || paint == "select" {
                retouch_demo(editor);
            }
            if paint == "select" {
                selection_demo(editor);
            }
        }
        if let Some(name) = var("INKFRAME_SCREENSHOT_TOOL") {
            match Tool::all().find(|t| t.name().eq_ignore_ascii_case(&name)) {
                Some(tool) => editor.use_tool(tool),
                None => eprintln!("Unknown tool {name:?} in INKFRAME_SCREENSHOT_TOOL"),
            }
        }
    }

    /// The text layer to start typing into, if any.
    pub fn type_into() -> Option<String> {
        std::env::var("INKFRAME_SCREENSHOT_TYPE").ok()
    }

    /// The format to open the Export window with, if any.
    pub fn export() -> Option<inkframe_export::ExportFormat> {
        let name = std::env::var("INKFRAME_SCREENSHOT_EXPORT").ok()?.to_lowercase();
        let squash = |s: &str| s.to_lowercase().replace(['.', ' ', '-'], "");
        inkframe_export::ExportFormat::ALL.into_iter().find(|f| squash(f.label()).starts_with(&squash(&name)))
    }

    /// Which framing guides to show, as a list such as `safe,field`.
    pub fn guides() -> Option<String> {
        std::env::var("INKFRAME_SCREENSHOT_GUIDES").ok()
    }

    /// How to open the Vectorise window, if at all: `ink` or `colours`,
    /// and `apply` to vectorise straight away.
    pub fn vectorise() -> Option<String> {
        std::env::var("INKFRAME_SCREENSHOT_VECTORISE").ok()
    }

    /// Whether to build the multiplane scene.
    pub fn multiplane() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_MULTIPLANE").is_some()
    }

    /// Whether to add the particle presets.
    pub fn particles() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_PARTICLES").is_some()
    }

    /// Whether to show the recovery window.
    pub fn recovery() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_RECOVERY").is_some()
    }

    /// Whether to load the saved settings.
    pub fn load_prefs() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_PREFS").is_some()
    }

    /// Whether to open Batch Export and start a batch.
    pub fn batch() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_BATCH").is_some()
    }

    /// Whether to open the Lip Sync window.
    pub fn lip_sync() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_LIPSYNC").is_some()
    }

    /// Whether to rearrange the areas.
    pub fn custom_layout() -> bool {
        std::env::var("INKFRAME_SCREENSHOT_LAYOUT").is_ok_and(|v| v == "custom")
    }

    /// A workspace to show instead of the usual one, by name.
    pub fn workspace() -> Option<crate::layout::Workspace> {
        let name = std::env::var("INKFRAME_SCREENSHOT_WORKSPACE").ok()?;
        crate::layout::Workspace::ALL.into_iter().find(|w| w.name().eq_ignore_ascii_case(&name))
    }

    /// Whether to add a camera, zoomed in and turned a little, and show it
    /// with the Camera tool.
    pub fn camera() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_CAMERA").is_some()
    }

    /// Whether to add a ball that flies across the stage, motion blurred.
    pub fn motion_blur() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_MOTION_BLUR").is_some()
    }

    /// Whether to add pencil lines of varying width, with the Width tool.
    pub fn pressure_lines() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_LINES").is_some()
    }

    /// Whether to show the Templates panel, holding templates made from
    /// the rig, in a temporary folder.
    pub fn templates() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_TEMPLATES").is_some()
    }

    /// Whether to show a Node View in place of the timeline.
    pub fn node_view() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_NODES").is_some()
    }

    /// Whether to turn the light table on, with the rig's head selected.
    pub fn light_table() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_LIGHT_TABLE").is_some()
    }

    /// Whether to add an inked head, with a gap in its outline, filled with
    /// the paint bucket.
    pub fn ink_and_paint() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_INK").is_some()
    }

    /// Whether to make a palette of the rig's colours, with a darker
    /// "Night" version switched on.
    pub fn palette() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_PALETTE").is_some()
    }

    /// Whether to give the rig a drop shadow, a glow and a tint, and show
    /// the effects in Properties.
    pub fn effects() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_EFFECTS").is_some()
    }

    /// Whether to add a layer morphing from a square into a circle.
    pub fn shape_tween() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_SHAPE_TWEEN").is_some()
    }

    /// Whether to put the arm's layers (in examples/rig.inkf) in a folder.
    pub fn folder() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_FOLDER").is_some()
    }

    /// Whether to show the Stage area's editor menu open.
    pub fn area_menu() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_AREA_MENU").is_some()
    }

    /// How far to turn the view, in degrees.
    pub fn view_rotation() -> Option<f32> {
        std::env::var("INKFRAME_SCREENSHOT_ROTATE").ok()?.parse().ok()
    }

    /// Whether to open the exposure sheet.
    pub fn exposure_sheet() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_XSHEET").is_some()
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
        if self.frames_until_capture == 0 && self.started.elapsed() >= SETTLE_TIME {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.requested = true;
        } else {
            self.frames_until_capture = self.frames_until_capture.saturating_sub(1);
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

/// Paints sample strokes on a new paint layer: wavy lines in a few colours
/// and sizes whose pressure rises and falls along them, so their ends
/// taper as a pen's do; a soft, low-flow stroke; and an eraser pass.
fn paint_demo(editor: &mut Editor) {
    use inkframe_core::{BrushMode, BrushSettings, BrushStroke, Color, Point};
    let Some(layer) = editor.add_paint_layer() else { return };
    let Some((_, canvas, ..)) = editor.paint_target().filter(|(id, ..)| *id == layer) else { return };
    let strokes = [
        (Color::rgb(0x1a, 0x1a, 0x1a), 18.0, 0.9, 1.0, 300.0),
        (Color::rgb(0xd6, 0x3a, 0x3a), 40.0, 0.7, 1.0, 420.0),
        (Color::rgb(0x2f, 0x7d, 0xd6), 70.0, 0.4, 0.8, 560.0),
        (Color::rgb(0xf0, 0xb0, 0x3c), 160.0, 0.0, 0.08, 720.0),
    ];
    for (color, size, hardness, flow, y) in strokes {
        let settings = BrushSettings { size, hardness, flow, ..BrushSettings::default() };
        let mut stroke = BrushStroke::new(settings, BrushMode::Paint(color));
        for i in 0..=120 {
            let t = i as f32 / 120.0;
            let at = Point::new(300.0 + t * 1300.0, y + (t * 9.0).sin() * 50.0);
            // Light at the ends, firm in the middle.
            let pressure = (t * std::f32::consts::PI).sin();
            if let Some(canvas) = editor.doc.canvas_mut(canvas) {
                stroke.add_point(canvas, at, pressure);
            }
        }
    }
    let mut eraser = BrushStroke::new(editor.eraser, BrushMode::Erase);
    for i in 0..=40 {
        let at = Point::new(1250.0, 220.0 + i as f32 * 15.0);
        if let Some(canvas) = editor.doc.canvas_mut(canvas) {
            eraser.add_point(canvas, at, 1.0);
        }
    }
}

/// After `paint_demo`: a gradient sky on a layer of its own underneath, a
/// bucket-filled ring, and each retouching tool used once on the strokes.
fn retouch_demo(editor: &mut Editor) {
    use inkframe_core::{
        BrushSettings, BucketSettings, Color, GradientShape, Point, Retouch, RetouchStroke, ToneRange,
    };
    let Some((strokes, canvas, ..)) = editor.paint_target() else { return };

    // The sky, moved below the strokes.
    if let Some(sky) = editor.add_paint_layer()
        && let Some((_, sky_canvas, ..)) = editor.paint_target()
    {
        if let Some(c) = editor.doc.canvas_mut(sky_canvas) {
            let (top, bottom) = (Color::rgb(0x9c, 0xc8, 0xf0), Color::rgb(0xfb, 0xf4, 0xe6));
            let sky = inkframe_core::Gradient {
                from: Point::new(0.0, 0.0),
                to: Point::new(0.0, 1080.0),
                start: top,
                end: bottom,
                shape: GradientShape::Linear,
                opacity: 1.0,
            };
            inkframe_core::gradient(c, &sky, None);
        }
        editor.doc.move_layer(sky, -1);
        editor.select(Some(strokes));
    }

    let Some(c) = editor.doc.canvas_mut(canvas) else { return };
    // A ring drawn with the brush, then filled with the bucket.
    let ring = BrushSettings { size: 10.0, hardness: 1.0, pressure_size: false, ..BrushSettings::default() };
    let mut stroke =
        inkframe_core::BrushStroke::new(ring, inkframe_core::BrushMode::Paint(Color::rgb(30, 30, 30)));
    for i in 0..=90 {
        let t = i as f32 / 90.0 * std::f32::consts::TAU;
        stroke.add_point(c, Point::new(1700.0 + t.cos() * 110.0, 880.0 + t.sin() * 110.0), 1.0);
    }
    inkframe_core::bucket_fill(c, 1700, 880, Color::rgb(0x6c, 0xc0, 0x6c), BucketSettings::default(), None);

    // Each retouching tool once, on the strokes.
    let soft =
        |size: f32| BrushSettings { size, hardness: 0.3, pressure_size: false, ..BrushSettings::default() };
    // Each pass: what it does, its brush size, and the points it passes.
    type Pass<'a> = (Retouch, f32, &'a [(f32, f32)]);
    let passes: [Pass; 5] = [
        (Retouch::Smudge { strength: 0.9 }, 40.0, &[(470.0, 350.0), (520.0, 300.0), (570.0, 240.0)]),
        (
            Retouch::Blur { strength: 1.0 },
            80.0,
            &[(900.0, 420.0), (1100.0, 420.0), (900.0, 420.0), (1100.0, 420.0)],
        ),
        (
            Retouch::Dodge { exposure: 0.6, range: ToneRange::Midtones },
            70.0,
            &[(700.0, 560.0), (800.0, 560.0)],
        ),
        (
            Retouch::Burn { exposure: 0.7, range: ToneRange::Midtones },
            90.0,
            &[(430.0, 765.0), (560.0, 770.0)],
        ),
        (
            Retouch::Clone { offset: Point::new(-300.0, 0.0), opacity: 1.0 },
            120.0,
            &[(1650.0, 300.0), (1650.0, 600.0)],
        ),
    ];
    for (kind, size, points) in passes {
        let mut stroke = RetouchStroke::new(soft(size), kind, c);
        for &(x, y) in points {
            stroke.add_point(c, Point::new(x, y), 1.0);
        }
    }
}

/// After `retouch_demo`: an elliptical selection across the strokes, their
/// hue turned inside it only, and the strokes layer set to Multiply.
fn selection_demo(editor: &mut Editor) {
    use inkframe_core::{Adjustment, BlendMode, Point, Selection, adjust};
    let Some((layer, canvas, width, height)) = editor.paint_target() else { return };
    let selection = Selection::ellipse(width, height, Point::new(620.0, 250.0), Point::new(1100.0, 650.0));
    if let Some(pixels) = editor.doc.canvas(canvas) {
        let hue = Adjustment::HueSaturation { hue: 140.0, saturation: 0.2, lightness: 0.0 };
        let shifted = adjust(pixels, &hue, Some(&selection));
        if let Some(c) = editor.doc.canvas_mut(canvas) {
            *c = shifted;
        }
    }
    if let Some(layer) = editor.doc.layer_mut(layer) {
        layer.blend = BlendMode::Multiply;
    }
    editor.pixel_selection = Some((canvas, selection));
}
