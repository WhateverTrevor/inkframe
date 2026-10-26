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
//! * `INKFRAME_SCREENSHOT_NEW_LAYER=1` adds an empty layer above the selection (after
//!   `_SELECT`) and draws a hat into it, as + and two shapes would.
//! * `INKFRAME_SCREENSHOT_INSIDE=1` steps inside the selected layer and picks its top shape.
//! * `INKFRAME_SCREENSHOT_FRAMES=4-12` picks frames in the timeline; `_MULTIPLE=1` turns on
//!   Edit Multiple Frames.
//! * `INKFRAME_SCREENSHOT_STYLES=1` adds shapes with gradient fills and dashed and dotted
//!   lines, and steps inside the sun to show its gradient's handles.
//! * `INKFRAME_SCREENSHOT_COMBINE=1` shows shapes combined: two circles in a union, a
//!   square with a circle punched out, and text broken apart into letters (one picked).
//! * `INKFRAME_SCREENSHOT_RULERS=1` shows the rulers, with three guides.
//! * `INKFRAME_SCREENSHOT_SKELETON=1` shows a character rigged with a skeleton: a painted
//!   arm bent at the elbow, a drawn body leaning, and a head bound to its own bone.
//! * `INKFRAME_SCREENSHOT_SMART=1` adds to that a hand in a switch, which the forearm, a
//!   smart bone, turns from open to a fist, and a body that stretches as the spine leans.
//! * `INKFRAME_SCREENSHOT_DYNAMICS=1` shows that character a few frames into a quick turn:
//!   a ponytail with dynamics trailing behind, the body squashed keeping its area, and the
//!   upper arm's limits.
//! * `INKFRAME_SCREENSHOT_WELDS=1` shows welded points: a house of three shapes sharing
//!   corners, picked with the Subselection tool, and a sleeve pinned to the upper arm that
//!   stays joined to the forearm at the elbow as the arm bends.
//! * `INKFRAME_SCREENSHOT_BONES=1` shows the Bones panel in place of Properties (with
//!   `_SMART` and the like).
//! * `INKFRAME_SCREENSHOT_SETUP=1` turns on rig setup, showing skeletons at rest (with
//!   `_SKELETON` and the like).
//! * `INKFRAME_SCREENSHOT_WALK=rigged` builds the guidebook's walk project: a
//!   character from the side whose feet, on target bones, step through a
//!   24-frame walk cycle as the hips bob (`=drawn` stops before the rig;
//!   `=follows` selects the near leg, to show how it follows its limb).
//! * `INKFRAME_SCREENSHOT_BANNER=1` builds the guidebook's web banner project: a
//!   970 × 250 stage with a title that pops in, a Shop Now button that opens a
//!   link, and a Stop on the last frame.
//! * `INKFRAME_SCREENSHOT_TARGETS=1` shows two legs whose shins reach for target bones on
//!   the ground, crouching as the hips drop with the feet staying planted.
//! * `INKFRAME_SCREENSHOT_BRUSHES=1` shows strokes drawn with art and pattern brushes: a
//!   vine of leaves, a tapered swoosh, and a card framed with beads.
//! * `INKFRAME_SCREENSHOT_TEXTURES=1` shows a wavy line inked flat, then with each line
//!   texture: pencil, charcoal and chalk.
//! * `INKFRAME_SCREENSHOT_PERSPECTIVE=1` shows a two-point perspective guide with a block
//!   drawn to it.
//! * `INKFRAME_SCREENSHOT_INBETWEENS=1` shows inbetweens drawn between a square and a star,
//!   seen through onion skins.
//! * `INKFRAME_SCREENSHOT_CORRECTION=1` adds to `_SKELETON`'s character a correction on the
//!   spine, keyed at its turn: the body reshaped by hand into a pear as it leans.
//! * `INKFRAME_SCREENSHOT_BUTTONS=1` shows a ball that stops on its last frame (a frame
//!   action), with Replay and Pause buttons for a web page export, Replay picked.
//! * `INKFRAME_SCREENSHOT_FINDER=envel` opens Find a Command with that typed.
//! * `INKFRAME_SCREENSHOT_PEN_HANDLES=1` shows a shape drawn with the Pen, one of its
//!   corners turned into a curve by Alt-dragging handles out of it, picked with the Pen.
//! * `INKFRAME_SCREENSHOT_ENVELOPE=1` shows a striped card distorted into perspective,
//!   and a word bent by an envelope, its handles still out.
//! * `INKFRAME_SCREENSHOT_TOOL=Subselection` picks a tool by its toolbar name.
//! * `INKFRAME_SCREENSHOT_SEARCH=blur` types that into the properties panel's search box.
//! * `INKFRAME_SCREENSHOT_FOLD=transform,easing` folds those Properties sections
//!   (by key, as in `inspector.rs`).
//! * `INKFRAME_SCREENSHOT_ACTIONS=40` previews the picked bone turned 40° on its
//!   action timeline (with `_SMART` or `_CORRECTION`).
//! * `INKFRAME_SCREENSHOT_EXPAND=1` expands every layer's property rows in the timeline.
//! * `INKFRAME_SCREENSHOT_EDIT=Ball` opens the named symbol on its own (before `_SELECT`).
//! * `INKFRAME_SCREENSHOT_OPEN=Ball` opens the named instance layer's symbol in place (after `_FRAME`).
//! * `INKFRAME_SCREENSHOT_TYPE=Title` starts typing into the named text layer, with its text selected.
//! * `INKFRAME_SCREENSHOT_GRAPH=1` shows the value graph in the timeline.
//! * `INKFRAME_SCREENSHOT_LIPSYNC=1` opens the Lip Sync window for the selected layer
//!   (giving a project with no sound a made-up line of dialogue first); `=apply` also
//!   applies it from loudness once the sound has loaded.
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
//! * `INKFRAME_SCREENSHOT_EXAMPLES=1` opens File › Open Example.
//! * `INKFRAME_SCREENSHOT_SCRIPT=path.js` runs a script on the opened project, as the
//!   Scripts menu does.
//! * `INKFRAME_SCREENSHOT_CONSOLE=path.js` shows the Script Console in place of
//!   Properties, with that script typed in and run.
//! * `INKFRAME_SCREENSHOT_GROUP=1` puts every layer into one symbol (a 24-frame loop), as
//!   selecting them all and pressing F8 would; `=open` then opens it in place.
//! * `INKFRAME_SCREENSHOT_LIBRARY=1` makes each layer a symbol (some in a folder), adds the
//!   sample pictures from `docs/images` and a sound, and shows the Library with the first card
//!   previewed. With `_GROUP=1`, the grouped symbol is the one previewed.
//! * `INKFRAME_SCREENSHOT_SWAP=1` (after `_GROUP`) duplicates the instance's symbol, swaps the
//!   copy in from frame 13, and opens Swap Symbol.
//! * `INKFRAME_SCREENSHOT_PRESETS=1` shows Motion Presets in place of Properties, with the
//!   selected layer's movement saved (in a temporary folder) and Bounce In picked.
//! * `INKFRAME_SCREENSHOT_XSHEET=1` opens the exposure sheet.
//! * `INKFRAME_SCREENSHOT_BENCH=48` steps through that many frames, one per
//!   update, as scrubbing does, before the screenshot, and prints how long
//!   the updates took: a check on how the editor copes with a big scene.
//! * `INKFRAME_SCREENSHOT_TOUR=2` shows the first-run tour's second card.
//! * `INKFRAME_SCREENSHOT_WHATS_NEW=1` shows this version's What's New.
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
        // Picked frames, 1-based like the timeline: `_FRAMES=4-12`.
        if let Some((first, last)) = var("INKFRAME_SCREENSHOT_FRAMES").and_then(|v| {
            let (a, b) = v.split_once('-')?;
            Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?))
        }) {
            editor.frame_range = Some((first.saturating_sub(1), last.saturating_sub(1)));
        }
        if var("INKFRAME_SCREENSHOT_MULTIPLE").is_some() {
            editor.edit_multiple = true;
        }
        if var("INKFRAME_SCREENSHOT_RULERS").is_some() {
            editor.show_rulers = true;
            for (vertical, at) in [(true, 960.0), (false, 540.0), (false, 180.0)] {
                editor.doc.ruler_guides.push(inkframe_core::RulerGuide { vertical, at });
            }
        }
        let rigged = [
            "INKFRAME_SCREENSHOT_SKELETON",
            "INKFRAME_SCREENSHOT_SMART",
            "INKFRAME_SCREENSHOT_DYNAMICS",
            "INKFRAME_SCREENSHOT_CORRECTION",
        ];
        if rigged.iter().any(|v| var(v).is_some()) {
            skeleton_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_SMART").is_some() {
            smart_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_DYNAMICS").is_some() {
            dynamics_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_CORRECTION").is_some() {
            correction_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_WELDS").is_some() {
            welds_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_SETUP").is_some() {
            editor.rig_setup = true;
        }
        if var("INKFRAME_SCREENSHOT_TARGETS").is_some() {
            targets_demo(editor);
        }
        if let Some(stage) = var("INKFRAME_SCREENSHOT_WALK") {
            walk_demo(editor, &stage);
        }
        if var("INKFRAME_SCREENSHOT_BANNER").is_some() {
            banner_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_BRUSHES").is_some() {
            brushes_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_TEXTURES").is_some() {
            textures_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_PERSPECTIVE").is_some() {
            perspective_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_INBETWEENS").is_some() {
            inbetweens_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_BUTTONS").is_some() {
            buttons_demo(editor);
        }
        if Self::pen_handles() {
            pen_handles_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_ENVELOPE").is_some() {
            envelope_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_COMBINE").is_some() {
            combine_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_STYLES").is_some() {
            styles_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_NEW_LAYER").is_some() {
            new_layer_demo(editor);
        }
        if var("INKFRAME_SCREENSHOT_INSIDE").is_some()
            && let Some(layer) = editor.selection
            && editor.enter_layer(layer)
        {
            let last = editor.doc.layer(layer).map_or(0, |l| l.elements_at(editor.frame).len());
            editor.pick_shapes(vec![crate::inside::ShapeRef::line(last.saturating_sub(1))]);
        }
        if let Some(name) = var("INKFRAME_SCREENSHOT_TOOL") {
            match Tool::all().find(|t| t.name().eq_ignore_ascii_case(&name)) {
                Some(tool) => editor.use_tool(tool),
                None => eprintln!("Unknown tool {name:?} in INKFRAME_SCREENSHOT_TOOL"),
            }
        }
        if let Some(text) = var("INKFRAME_SCREENSHOT_SEARCH") {
            editor.properties_search = text;
        }
        if let Some(keys) = var("INKFRAME_SCREENSHOT_FOLD") {
            editor.folded_sections.extend(keys.split(',').map(str::to_owned));
        }
        if let Some(turn) = var("INKFRAME_SCREENSHOT_ACTIONS").and_then(|v| v.parse::<f32>().ok())
            && let (Some(id), Some(bone)) = (editor.active_skeleton(), editor.selected_bone)
        {
            editor.preview_turn(id, bone, turn);
        }
    }

    /// Whether to show the Pen picking a point it's pulled handles out of.
    pub fn pen_handles() -> bool {
        std::env::var("INKFRAME_SCREENSHOT_PEN_HANDLES").is_ok()
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

    /// A script to type into the Script Console and run.
    pub fn console() -> Option<std::path::PathBuf> {
        std::env::var_os("INKFRAME_SCREENSHOT_CONSOLE").map(Into::into)
    }

    /// A script to run, as from the Scripts menu.
    pub fn script() -> Option<std::path::PathBuf> {
        std::env::var_os("INKFRAME_SCREENSHOT_SCRIPT").map(Into::into)
    }

    /// Whether to open the examples gallery.
    pub fn examples() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_EXAMPLES").is_some()
    }

    /// Whether to swap the selected instance's symbol partway and open
    /// Swap Symbol.
    pub fn swap() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_SWAP").is_some()
    }

    /// Whether to show Motion Presets.
    /// What to type into Find a Command, opening it.
    pub fn finder() -> Option<String> {
        std::env::var("INKFRAME_SCREENSHOT_FINDER").ok()
    }

    /// Whether to show the Bones panel in place of Properties.
    pub fn bones_panel() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_BONES").is_some()
    }

    pub fn presets() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_PRESETS").is_some()
    }

    /// Whether to fill and show the Library.
    pub fn library() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_LIBRARY").is_some()
    }

    /// Whether to group every layer into a symbol, and whether to open it.
    pub fn group() -> Option<bool> {
        std::env::var("INKFRAME_SCREENSHOT_GROUP").ok().map(|v| v == "open")
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

    /// Whether to show this version's What's New.
    pub fn whats_new() -> bool {
        std::env::var_os("INKFRAME_SCREENSHOT_WHATS_NEW").is_some()
    }

    /// Which card of the first-run tour to show, counting from 1.
    pub fn tour() -> Option<usize> {
        std::env::var("INKFRAME_SCREENSHOT_TOUR").ok()?.parse().ok()
    }

    /// How many frames to step through, timing each update, first.
    pub fn bench() -> Option<u32> {
        std::env::var("INKFRAME_SCREENSHOT_BENCH").ok()?.parse().ok()
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

/// A new layer with a hat drawn into it: a brim and a crown, both going
/// into the one layer as they would with the Rectangle tool.
fn new_layer_demo(editor: &mut Editor) {
    use inkframe_core::{Color, Element, Point, Shape, Style};
    editor.add_empty_layer();
    let style = Style { fill: Some(Color::rgb(60, 50, 70)), stroke: None, gradient: None };
    let rect = |width, height| Element {
        shape: Shape::Rect { width, height, corner_radius: 6.0 },
        style: style.clone(),
    };
    editor.add_drawn_shape("Rectangle", rect(220.0, 24.0), Point::new(785.0, 150.0));
    editor.add_drawn_shape("Rectangle", rect(130.0, 110.0), Point::new(785.0, 90.0));
}

/// Fills the Library for a screenshot: every layer becomes a symbol of its
/// own (unless there are symbols already, from `_GROUP`), the far ones
/// filed under Scenery, with the sample pictures and a short silent sound
/// beside them.
pub fn library_demo(editor: &mut Editor) {
    use inkframe_core::Asset;
    let layers: Vec<_> = if editor.doc.symbols.is_empty() {
        editor.doc.layers.iter().filter(|l| !l.camera).map(|l| (l.id, l.name.clone())).collect()
    } else {
        Vec::new()
    };
    for (i, (layer, name)) in layers.into_iter().enumerate() {
        if let Some(symbol) = editor.doc.convert_layer_to_symbol(layer, 0) {
            editor.doc.rename_asset(Asset::Symbol(symbol), &name);
            if i < 3 {
                editor.doc.move_asset(Asset::Symbol(symbol), "Scenery");
            }
        }
    }
    let pictures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/images");
    for name in ["sample-rig.png", "sample-particles.png", "sample-ink-and-paint.png"] {
        if let Ok(image) = editor.add_library_image(&pictures.join(name)) {
            editor.doc.move_asset(Asset::Image(image), "Reference");
        }
    }
    // A tenth of a second of silence.
    let sound = std::env::temp_dir().join("whoosh.wav");
    if write_wav(&sound, &[0; 4410]).is_ok() {
        editor.import_audio(&sound);
    }
}

/// Gives a project with no sound a made-up line of dialogue, for the Lip
/// Sync window to work from: two seconds of a hum that swells and fades
/// like syllables, with pauses between words, so its loudness reads like
/// speech.
pub fn add_dialogue(editor: &mut Editor) {
    if !editor.doc.audio.is_empty() {
        return;
    }
    const RATE: f32 = 44_100.0;
    let samples: Vec<i16> = (0..2 * 44_100)
        .map(|n| {
            let t = n as f32 / RATE;
            // Five syllables a second, with a word's pause every third.
            let syllable = (t * 5.0 * std::f32::consts::PI).sin().abs();
            let word = if (t * 5.0) as u32 % 3 == 2 { 0.15 } else { 1.0 };
            let voice =
                (t * 180.0 * std::f32::consts::TAU).sin() + 0.4 * (t * 360.0 * std::f32::consts::TAU).sin();
            (voice * syllable * word * 9_000.0) as i16
        })
        .collect();
    let sound = std::env::temp_dir().join("dialogue.wav");
    if write_wav(&sound, &samples).is_ok() {
        editor.import_audio(&sound);
    }
}

/// Writes 16-bit mono samples at 44.1 kHz as a WAV file: a 44-byte header
/// and the samples.
fn write_wav(path: &std::path::Path, samples: &[i16]) -> std::io::Result<()> {
    let bytes = samples.len() as u32 * 2;
    let mut wav = Vec::with_capacity(44 + bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    for value in [16u32, 1 | (1 << 16), 44_100, 88_200, 2 | (16 << 16)] {
        wav.extend_from_slice(&value.to_le_bytes());
    }
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, wav)
}

/// Gradient fills and line styles: a sky fading top to bottom, a sun with
/// a radial glow (picked inside its layer, so its gradient's handles show),
/// and a dashed and a dotted outline.
fn styles_demo(editor: &mut Editor) {
    use inkframe_core::{
        Color, Dash, Element, GradientStop, LineCap, Point, Shape, ShapeGradient, Stroke, Style, Transform,
    };
    let mut sky = ShapeGradient::linear(Color::rgb(40, 70, 160), Color::rgb(250, 170, 110));
    (sky.from, sky.to) = (Point::new(0.5, 0.0), Point::new(0.5, 1.0));
    sky.stops.insert(1, GradientStop { at: 0.6, color: Color::rgb(230, 120, 140) });
    let sun = ShapeGradient::radial(Color::rgb(255, 245, 190), Color::rgb(250, 140, 60));
    let filled = |gradient: ShapeGradient| Style {
        fill: Some(gradient.stops[0].color),
        stroke: None,
        gradient: Some(gradient),
    };
    let outline = |dash: Dash, cap: LineCap| Style {
        fill: None,
        stroke: Some(Stroke { dash: Some(dash), cap, ..Stroke::new(Color::rgb(40, 40, 60), 8.0) }),
        gradient: None,
    };
    let shapes = [
        (
            "Sky",
            Shape::Rect { width: 1920.0, height: 1080.0, corner_radius: 0.0 },
            filled(sky),
            Point::new(960.0, 540.0),
        ),
        ("Sun", Shape::Ellipse { radius_x: 220.0, radius_y: 220.0 }, filled(sun), Point::new(1300.0, 560.0)),
        (
            "Dashed",
            Shape::Rect { width: 420.0, height: 260.0, corner_radius: 30.0 },
            outline(Dash { length: 30.0, gap: 18.0 }, LineCap::Butt),
            Point::new(450.0, 380.0),
        ),
        (
            "Dotted",
            Shape::Ellipse { radius_x: 160.0, radius_y: 160.0 },
            outline(Dash::dotted(8.0), LineCap::Round),
            Point::new(450.0, 790.0),
        ),
    ];
    let mut sun_layer = None;
    for (name, shape, style, at) in shapes {
        let id = editor.doc.add_layer(name, vec![Element { shape, style }], 0, Transform::at(at));
        if name == "Sun" {
            sun_layer = Some(id);
        }
    }
    if let Some(sun) = sun_layer
        && editor.enter_layer(sun)
    {
        editor.pick_shapes(vec![crate::inside::ShapeRef::line(0)]);
    }
}

/// Combined shapes: a union of two circles, a circle punched out of a
/// square, and a word broken apart into letters, the last of them picked.
fn combine_demo(editor: &mut Editor) {
    use inkframe_core::{Affine, Color, Element, Point, Shape, ShapeOp, Style, Transform};
    let fill = |c: Color| Style {
        fill: Some(c),
        stroke: Some(inkframe_core::Stroke::new(Color::rgb(30, 30, 40), 4.0)),
        gradient: None,
    };
    let at = |shape: Shape, c: Color, x: f32, y: f32| {
        Element { shape, style: fill(c) }.transformed(Affine::translate(Point::new(x, y))).unwrap()
    };
    let circle = |r: f32| Shape::Ellipse { radius_x: r, radius_y: r };
    let shapes = vec![
        at(circle(130.0), Color::rgb(90, 160, 230), -150.0, 0.0),
        at(circle(130.0), Color::rgb(90, 160, 230), 0.0, 0.0),
        at(
            Shape::Rect { width: 300.0, height: 300.0, corner_radius: 20.0 },
            Color::rgb(240, 150, 90),
            400.0,
            0.0,
        ),
        at(circle(90.0), Color::rgb(240, 150, 90), 450.0, 40.0),
    ];
    let layer = editor.doc.add_layer("Shapes", shapes, 0, Transform::at(Point::new(700.0, 380.0)));
    editor.enter_layer(layer);
    editor.pick_shapes(vec![crate::inside::ShapeRef::line(2), crate::inside::ShapeRef::line(3)]);
    let _ = editor.combine_shapes(ShapeOp::Punch);
    editor.pick_shapes(vec![crate::inside::ShapeRef::line(0), crate::inside::ShapeRef::line(1)]);
    let _ = editor.combine_shapes(ShapeOp::Union);
    editor.exit_layer();

    let Some(text) = editor.add_text(Point::new(560.0, 800.0)) else { return };
    if let Some(element) = editor.doc.layer_mut(text).and_then(|l| l.elements_at_mut(0).first_mut()) {
        if let Shape::Text(t) = &mut element.shape {
            t.content = "Inkframe".into();
            t.size = 160.0;
        }
        element.style = fill(Color::rgb(120, 200, 140));
    }
    editor.select(Some(text));
    editor.break_apart();
    if editor.enter_layer(text) {
        let letters = editor.doc.layer(text).map_or(0, |l| l.elements_at(0).len());
        editor.pick_shapes(vec![crate::inside::ShapeRef::line(letters.saturating_sub(1))]);
    }
}

/// A striped card leaning back with Distort, and a word bowed with
/// Envelope, left with its handles out.
fn envelope_demo(editor: &mut Editor) {
    use crate::envelope::LatticeKind;
    use inkframe_core::{Color, Element, Point, Shape, Style, Transform};
    let ink = Color::rgb(30, 30, 40);
    let style = |c: Color| Style {
        fill: Some(c),
        stroke: Some(inkframe_core::Stroke::new(ink, 4.0)),
        gradient: None,
    };
    let band = |y: f32, c: Color| {
        let shape = Shape::Rect { width: 420.0, height: 70.0, corner_radius: 0.0 };
        Element { shape, style: style(c) }
            .transformed(inkframe_core::Affine::translate(Point::new(0.0, y)))
            .unwrap()
    };
    let stripes = vec![
        band(-105.0, Color::rgb(230, 90, 100)),
        band(-35.0, Color::rgb(250, 200, 90)),
        band(35.0, Color::rgb(90, 190, 140)),
        band(105.0, Color::rgb(90, 150, 230)),
    ];
    let card = editor.doc.add_layer("Card", stripes, 0, Transform::at(Point::new(480.0, 560.0)));
    editor.select(Some(card));
    if editor.start_lattice(LatticeKind::Distort).is_ok() {
        // The top pulled in and down: lying back, away from us.
        let top = editor.lattice.as_ref().map(|e| (e.lattice.points[0], e.lattice.points[1]));
        if let Some((left, right)) = top {
            editor.move_lattice_point(0, left + Point::new(90.0, 60.0), true);
            editor.move_lattice_point(1, right + Point::new(-90.0, 60.0), true);
        }
        editor.finish_lattice();
    }
    editor.exit_layer();

    let Some(text) = editor.add_text(Point::new(1300.0, 560.0)) else { return };
    if let Some(element) = editor.doc.layer_mut(text).and_then(|l| l.elements_at_mut(0).first_mut()) {
        if let Shape::Text(t) = &mut element.shape {
            t.content = "WAVE".into();
            t.size = 200.0;
        }
        element.style = style(Color::rgb(160, 120, 230));
    }
    editor.select(Some(text));
    editor.break_apart();
    editor.exit_layer();
    editor.select(Some(text));
    if editor.start_lattice(LatticeKind::Envelope).is_ok() {
        // The middles of the top and bottom turned, their handles in
        // line: up on the left and down on the right, so the word rides a
        // wave.
        for (handle, lift) in [(2, Point::new(-30.0, -110.0)), (14, Point::new(30.0, 110.0))] {
            let at = editor.lattice.as_ref().map(|e| e.lattice.points[handle]);
            if let Some(at) = at {
                editor.move_lattice_point(handle, at + lift, true);
            }
        }
    }
}

/// A three-cornered shape as the Pen draws it with clicks, its top corner
/// then Alt-dragged into a curve, and the Pen picked. The stage picks the
/// point afterwards (see `StageView::pick_anchor`), to show its handles.
fn pen_handles_demo(editor: &mut Editor) {
    use inkframe_core::{Anchor, Color, Element, Point, Shape, Stroke, Style, Transform, path_from_anchors};
    let corners = [Point::new(-180.0, 120.0), Point::new(0.0, -140.0), Point::new(180.0, 120.0)];
    let anchors: Vec<Anchor> = corners.into_iter().map(Anchor::corner).collect();
    let anchors = crate::subselect::pull_handles(&anchors, 1, Point::new(140.0, -140.0));
    let Some(path) = path_from_anchors(&anchors, true) else { return };
    let style = Style {
        fill: Some(Color::rgb(120, 200, 140)),
        stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 5.0)),
        gradient: None,
    };
    let shape = Element { shape: Shape::Path(path), style };
    let layer = editor.doc.add_layer("Hill", vec![shape], 0, Transform::at(Point::new(960.0, 560.0)));
    editor.select(Some(layer));
    editor.tool = Tool::Pen;
}

/// A character rigged with a skeleton and posed: a spine, a head, and an
/// arm whose upper half and forearm are painted on a paint layer. The body
/// is drawn, and bends with the spine; the head is bound to its bone.
fn skeleton_demo(editor: &mut Editor) {
    use inkframe_core::{
        BoneBinding, Color, Element, Point, Shape, Skeleton, Stroke, Style, TILE, TimelineId, Transform,
    };
    let ink = Stroke::new(Color::rgb(30, 30, 40), 5.0);
    let shape = |shape: Shape, fill: Color| Element {
        shape,
        style: Style { fill: Some(fill), stroke: Some(ink), gradient: None },
    };
    let doc = &mut editor.doc;
    let body = doc.add_layer(
        "Body",
        vec![shape(Shape::Ellipse { radius_x: 90.0, radius_y: 170.0 }, Color::rgb(90, 150, 230))],
        0,
        Transform::at(Point::new(960.0, 690.0)),
    );
    let head = doc.add_layer(
        "Head",
        vec![shape(Shape::Ellipse { radius_x: 70.0, radius_y: 70.0 }, Color::rgb(250, 210, 160))],
        0,
        Transform::at(Point::new(960.0, 440.0)),
    );
    // The arm, painted: a soft-edged band from the shoulder out, narrowing
    // to the wrist, on a canvas lined up with the stage.
    let Some(arm) = doc.add_paint_layer(TimelineId::Main, "Arm (painted)", 0) else { return };
    let Some((canvas, _, _)) = doc.paint_canvas_at(arm, 0) else { return };
    let Some(canvas) = doc.canvas_mut(canvas) else { return };
    for x in 960..1320u32 {
        let along = (x - 960) as f32 / 360.0;
        let half = 34.0 - 14.0 * along;
        for y in 480..600u32 {
            let edge = half - (y as f32 - 540.0).abs();
            if edge <= 0.0 {
                continue;
            }
            let a = (edge / 3.0).min(1.0);
            // Warm orange, darker toward the edge as a painted stroke would be.
            let shade = 0.75 + 0.25 * (edge / half);
            let colour = [240.0 * shade, 140.0 * shade, 60.0 * shade].map(|c| (c * a) as u8);
            let tile = canvas.tile_mut((x / TILE, y / TILE));
            let at = (((y % TILE) * TILE + x % TILE) * 4) as usize;
            tile[at..at + 4].copy_from_slice(&[colour[0], colour[1], colour[2], (255.0 * a) as u8]);
        }
    }

    let Some(folder) = doc.add_folder(TimelineId::Main, &[body, head, arm]) else { return };
    let mut skeleton = Skeleton::default();
    let spine = skeleton.add_bone(None, Point::new(960.0, 840.0), Point::new(960.0, 520.0), 0);
    let neck = skeleton.add_bone(Some(spine), Point::new(960.0, 520.0), Point::new(960.0, 380.0), 0);
    let upper = skeleton.add_bone(Some(spine), Point::new(980.0, 540.0), Point::new(1140.0, 540.0), 0);
    let fore = skeleton.add_bone(Some(upper), Point::new(1140.0, 540.0), Point::new(1310.0, 540.0), 0);
    for (bone, name) in [(spine, "Spine"), (neck, "Head"), (upper, "Upper arm"), (fore, "Forearm")] {
        skeleton.bones[bone].name = name.into();
    }
    skeleton.bones[spine].strength = 120.0;
    for (bone, turn) in [(spine, 10.0), (neck, -14.0), (upper, -35.0), (fore, -70.0)] {
        skeleton.bones[bone].turn.set(0, turn);
    }
    if let Some(layer) = doc.layer_mut(folder) {
        layer.name = "Character".into();
        layer.skeleton = Some(skeleton);
    }
    if let Some(layer) = doc.layer_mut(head) {
        layer.bone_binding = BoneBinding::Bone(neck);
    }
    editor.select(Some(folder));
    editor.selected_bone = Some(fore);
    editor.use_tool(Tool::Bone);
}

/// On top of `skeleton_demo`'s character: a hand in a switch (open, and a
/// fist) bound to the forearm, which as a smart bone shows the fist once
/// it's turned past 50°; and a body that stretches taller as the spine
/// leans, toward a second drawing.
fn smart_demo(editor: &mut Editor) {
    use inkframe_core::{
        BoneBinding, Color, Element, Point, Shape, SmartAction, Stroke, Style, TimelineId, Transform,
    };
    let Some(folder) = editor.selection else { return };
    let ink = Stroke::new(Color::rgb(30, 30, 40), 5.0);
    let skin = Style { fill: Some(Color::rgb(250, 210, 160)), stroke: Some(ink), gradient: None };
    let at = |shape: Shape, x: f32, y: f32| {
        Element { shape, style: skin.clone() }
            .transformed(inkframe_core::Affine::translate(Point::new(x, y)))
            .unwrap()
    };
    let doc = &mut editor.doc;
    // The open hand: a palm and four fingers, reaching on along the arm.
    let mut open = vec![at(Shape::Ellipse { radius_x: 30.0, radius_y: 26.0 }, 0.0, 0.0)];
    for (i, dy) in [-18.0, -6.0, 6.0, 18.0].into_iter().enumerate() {
        let length = if i == 0 || i == 3 { 30.0 } else { 38.0 };
        open.push(at(
            Shape::Rect { width: length, height: 10.0, corner_radius: 5.0 },
            30.0 + length / 2.0,
            dy,
        ));
    }
    let fist = vec![at(Shape::Ellipse { radius_x: 32.0, radius_y: 30.0 }, 4.0, 0.0)];
    let wrist = Transform::at(Point::new(1330.0, 540.0));
    let fist = doc.add_layer("Fist", fist, 0, wrist);
    let open = doc.add_layer("Open", open, 0, wrist);
    let Some(hand) = doc.add_folder(TimelineId::Main, &[fist, open]) else { return };
    doc.set_folder(hand, Some(folder));
    if let Some(layer) = doc.layer_mut(hand) {
        layer.name = "Hand".into();
        layer.switch = true;
    }
    let fore = 3;
    for id in [open, fist] {
        if let Some(layer) = doc.layer_mut(id) {
            layer.bone_binding = BoneBinding::Bone(fore);
        }
    }
    // The body's second drawing, taller and narrower, to stretch toward.
    let body = doc.layers.iter().find(|l| l.name == "Body").map(|l| l.id);
    let mut stretch = None;
    if let Some(layer) = body.and_then(|id| doc.layer_mut(id)) {
        let mut tall = layer.drawings[0].clone();
        tall.name = "Stretch".into();
        tall.elements[0].shape = Shape::Ellipse { radius_x: 75.0, radius_y: 195.0 };
        stretch = Some(layer.add_drawing(tall));
    }
    if let Some(skeleton) = doc.layer_mut(folder).and_then(|l| l.skeleton.as_mut()) {
        skeleton.bones[fore].actions.push(SmartAction::Switch { folder: hand, layer: fist, turn: -50.0 });
        if let (Some(layer), Some(drawing)) = (body, stretch) {
            skeleton.bones[0].actions.push(SmartAction::Morph { layer, drawing, turn: 10.0 });
        }
    }
    editor.select(Some(folder));
    editor.selected_bone = Some(fore);
}

/// On top of `skeleton_demo`'s character: a ponytail of two bones with
/// dynamics hanging from the head, and a quick lean of the spine from
/// frame 0 to 4, shown on frame 6 so the ponytail is still swinging after
/// it. The spine squashes as it leans, keeping its area, and the upper
/// arm, picked, shows the limits it may turn in.
fn dynamics_demo(editor: &mut Editor) {
    use inkframe_core::{
        Channel, Color, Dynamics, Element, Point, RotationLimits, Shape, Stroke, Style, Transform,
    };
    let Some(folder) = editor.selection else { return };
    let style = Style {
        fill: Some(Color::rgb(120, 70, 40)),
        stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 5.0)),
        gradient: None,
    };
    let tail = Element { shape: Shape::Rect { width: 170.0, height: 30.0, corner_radius: 15.0 }, style };
    let doc = &mut editor.doc;
    let ponytail = doc.add_layer("Ponytail", vec![tail], 0, Transform::at(Point::new(880.0, 420.0)));
    doc.set_folder(ponytail, Some(folder));
    // Behind the head and body: the bottom of the folder, where the body is.
    if let Some(body) = doc.layers.iter().position(|l| l.name == "Body") {
        doc.move_layer_to(ponytail, body);
    }
    if let Some(skeleton) = doc.layer_mut(folder).and_then(|l| l.skeleton.as_mut()) {
        let neck = 1;
        let top = skeleton.add_bone(Some(neck), Point::new(960.0, 420.0), Point::new(880.0, 420.0), 0);
        let tip = skeleton.add_bone(Some(top), Point::new(880.0, 420.0), Point::new(800.0, 420.0), 0);
        for (bone, name) in [(top, "Ponytail"), (tip, "Ponytail tip")] {
            skeleton.bones[bone].name = name.into();
            skeleton.bones[bone].dynamics = Some(Dynamics { spring: 0.15, damping: 0.1, limit: 70.0 });
            skeleton.bones[bone].strength = 30.0;
        }
        // The spine leans hard right over four frames, squashing as it goes.
        let spine = &mut skeleton.bones[0];
        spine.turn = Channel::new(0, -20.0);
        spine.turn.set(4, 25.0);
        let mut stretch = Channel::new(0, 1.0);
        stretch.set(4, 0.85);
        spine.stretch = Some(stretch);
        spine.squash = true;
        skeleton.bones[2].limits = Some(RotationLimits { min: -100.0, max: 20.0 });
    }
    editor.frame = 6;
    editor.selected_bone = Some(2);
}

/// Welded points: a house drawn as a wall, a roof and a door sharing
/// corners, picked with the Subselection tool so the welds show; and an
/// arm whose sleeve, pinned to the upper arm, shares its elbow edge with
/// the forearm, which bends with the forearm bone without coming apart.
fn welds_demo(editor: &mut Editor) {
    use inkframe_core::{
        Affine, BonePoints, Color, Element, Path, Point, Segment, Shape, Skeleton, Stroke, Style, TimelineId,
        Transform,
    };
    let ink = Stroke::new(Color::rgb(30, 30, 40), 4.0);
    let fill = |c: Color| Style { fill: Some(c), stroke: Some(ink), gradient: None };
    let rect = |w: f32, h: f32, x: f32, y: f32, c: Color| {
        Element { shape: Shape::Rect { width: w, height: h, corner_radius: 0.0 }, style: fill(c) }
            .transformed(Affine::translate(Point::new(x, y)))
            .unwrap()
    };
    let roof = Element {
        shape: Shape::Path(Path {
            start: Point::new(-150.0, -100.0),
            segments: vec![
                Segment::Line { to: Point::new(0.0, -220.0) },
                Segment::Line { to: Point::new(150.0, -100.0) },
            ],
            closed: true,
        }),
        style: fill(Color::rgb(230, 90, 100)),
    };
    // The roof's ends are the wall's top corners, and the door's bottom
    // left corner is the wall's.
    let wall = rect(300.0, 200.0, 0.0, 0.0, Color::rgb(250, 220, 150));
    let door = rect(70.0, 120.0, -115.0, 40.0, Color::rgb(140, 100, 70));
    let doc = &mut editor.doc;
    let house = doc.add_layer("House", vec![wall, roof, door], 0, Transform::at(Point::new(520.0, 600.0)));

    // The arm: a sleeve to the elbow and a forearm from it, sharing the edge.
    let sleeve = rect(180.0, 70.0, -90.0, 0.0, Color::rgb(90, 150, 230));
    let forearm = rect(180.0, 70.0, 90.0, 0.0, Color::rgb(250, 210, 160));
    let arm = doc.add_layer("Arm", vec![sleeve, forearm], 0, Transform::at(Point::new(1300.0, 540.0)));
    if let Some(layer) = doc.layer_mut(arm) {
        layer.drawings[0].bone_points.push(BonePoints {
            colour_art: false,
            element: 0,
            anchors: None,
            bone: 0,
        });
    }
    let Some(folder) = doc.add_folder(TimelineId::Main, &[arm]) else { return };
    let mut skeleton = Skeleton::default();
    let upper = skeleton.add_bone(None, Point::new(1120.0, 540.0), Point::new(1300.0, 540.0), 0);
    let fore = skeleton.add_bone(Some(upper), Point::new(1300.0, 540.0), Point::new(1480.0, 540.0), 0);
    skeleton.bones[fore].turn.set(0, 70.0);
    if let Some(layer) = doc.layer_mut(folder) {
        layer.name = "Rig".into();
        layer.skeleton = Some(skeleton);
    }
    editor.select(Some(house));
    editor.use_tool(Tool::Subselect);
}

/// Legs planted on target bones: hips (the root) with a thigh and shin on
/// each side, each shin reaching for a foot bone on the ground. The hips
/// drop on frame 10 and the knees bend to keep the feet where they are.
fn targets_demo(editor: &mut Editor) {
    use inkframe_core::{
        Affine, Color, Element, Point, Shape, Skeleton, Stroke, Style, TimelineId, Transform,
    };
    let ink = Stroke::new(Color::rgb(30, 30, 40), 5.0);
    let style = |c: Color| Style { fill: Some(c), stroke: Some(ink), gradient: None };
    let limb = |x: f32, top: f32, bottom: f32, width: f32, c: Color| {
        Element {
            shape: Shape::Rect { width, height: bottom - top, corner_radius: width / 2.0 },
            style: style(c),
        }
        .transformed(Affine::translate(Point::new(x, (top + bottom) / 2.0)))
        .unwrap()
    };
    let trousers = Color::rgb(90, 150, 230);
    let legs = vec![
        limb(-50.0, 0.0, 360.0, 60.0, trousers),
        limb(50.0, 0.0, 360.0, 60.0, trousers),
        Element {
            shape: Shape::Ellipse { radius_x: 110.0, radius_y: 70.0 },
            style: style(Color::rgb(230, 90, 100)),
        },
    ];
    let doc = &mut editor.doc;
    let layer = doc.add_layer("Legs", legs, 0, Transform::at(Point::new(960.0, 420.0)));
    let Some(folder) = doc.add_folder(TimelineId::Main, &[layer]) else { return };
    let mut skeleton = Skeleton::default();
    let hips = skeleton.add_bone(None, Point::new(960.0, 380.0), Point::new(960.0, 430.0), 0);
    skeleton.bones[hips].name = "Hips".into();
    for (side, x) in [("Left", 910.0), ("Right", 1010.0)] {
        let thigh = skeleton.add_bone(Some(hips), Point::new(x, 430.0), Point::new(x, 600.0), 0);
        let shin = skeleton.add_bone(Some(thigh), Point::new(x, 600.0), Point::new(x, 770.0), 0);
        // The feet on the ground under the hips, pointing ahead.
        let foot = skeleton.add_bone(None, Point::new(x, 770.0), Point::new(x + 60.0, 770.0), 0);
        skeleton.bones[thigh].name = format!("{side} thigh");
        skeleton.bones[shin].name = format!("{side} shin");
        skeleton.bones[foot].name = format!("{side} foot");
        skeleton.bones[shin].target = Some(foot);
        // Knees bend one way: forward, the shin swinging back under them.
        skeleton.bones[shin].limits = Some(inkframe_core::RotationLimits { min: 0.0, max: 150.0 });
        skeleton.bones[thigh].strength = 60.0;
        skeleton.bones[shin].strength = 60.0;
    }
    skeleton.move_bone(hips, Point::new(1000.0, 460.0), 10);
    if let Some(layer) = doc.layer_mut(folder) {
        layer.name = "Character".into();
        layer.skeleton = Some(skeleton);
    }
    editor.frame = 10;
    editor.select(Some(folder));
    editor.selected_bone = Some(2);
    editor.use_tool(Tool::Bone);
}

/// The guidebook's walk project (chapter "Project: A Walk Cycle"): a
/// character from the side, legs on a skeleton whose shins reach for
/// target bones under the feet. The feet are keyed through a walk on the
/// spot, sliding back while planted and lifting forward, a half cycle
/// apart, and the hips bob, lowest just after each contact and highest as
/// the legs pass. With `drawn`, just the layers, before the rig.
fn walk_demo(editor: &mut Editor, stage: &str) {
    use inkframe_core::{
        Affine, BoneBinding, Color, Element, Point, Shape, Skeleton, Stroke, Style, TimelineId, Transform,
    };
    let ink = Stroke::new(Color::rgb(30, 30, 40), 5.0);
    let style = |c: Color| Style { fill: Some(c), stroke: Some(ink), gradient: None };
    let placed = |shape: Shape, c: Color, at: Point| {
        Element { shape, style: style(c) }.transformed(Affine::translate(at)).unwrap()
    };
    // Shapes in stage coordinates, each layer at the origin.
    let leg = |x: f32, c: Color| {
        placed(Shape::Rect { width: 56.0, height: 360.0, corner_radius: 28.0 }, c, Point::new(x, 600.0))
    };
    let doc = &mut editor.doc;
    doc.duration = 24;
    let far = doc.add_layer("Far leg", vec![leg(962.0, Color::rgb(70, 115, 190))], 0, Transform::default());
    let near = doc.add_layer("Near leg", vec![leg(958.0, Color::rgb(90, 150, 230))], 0, Transform::default());
    let body = doc.add_layer(
        "Body",
        vec![
            placed(
                Shape::Rect { width: 130.0, height: 210.0, corner_radius: 50.0 },
                Color::rgb(240, 160, 70),
                Point::new(960.0, 330.0),
            ),
            placed(
                Shape::Ellipse { radius_x: 62.0, radius_y: 62.0 },
                Color::rgb(250, 205, 160),
                Point::new(968.0, 175.0),
            ),
        ],
        0,
        Transform::default(),
    );
    // The body goes in front of the far leg, behind the near one.
    doc.place_layer(body, Some(far), None);
    doc.place_layer(near, Some(body), None);
    editor.select(Some(near));
    if stage == "drawn" {
        return;
    }

    let doc = &mut editor.doc;
    let Some(folder) = doc.add_folder(TimelineId::Main, &[near, body, far]) else { return };
    let mut skeleton = Skeleton::default();
    let hips = skeleton.add_bone(None, Point::new(960.0, 380.0), Point::new(960.0, 430.0), 0);
    skeleton.bones[hips].name = "Hips".into();
    let mut feet = Vec::new();
    let mut thighs = Vec::new();
    for (side, x) in [("Near", 958.0), ("Far", 962.0)] {
        let thigh = skeleton.add_bone(Some(hips), Point::new(x, 430.0), Point::new(x, 600.0), 0);
        let shin = skeleton.add_bone(Some(thigh), Point::new(x, 600.0), Point::new(x, 770.0), 0);
        let foot = skeleton.add_bone(None, Point::new(x, 770.0), Point::new(x + 50.0, 770.0), 0);
        skeleton.bones[thigh].name = format!("{side} thigh");
        skeleton.bones[shin].name = format!("{side} shin");
        skeleton.bones[foot].name = format!("{side} foot");
        skeleton.bones[shin].target = Some(foot);
        skeleton.bones[shin].limits = Some(inkframe_core::RotationLimits { min: 0.0, max: 150.0 });
        skeleton.bones[thigh].strength = 40.0;
        skeleton.bones[shin].strength = 40.0;
        feet.push((foot, x));
        thighs.push(thigh);
    }
    // One foot's step, by frame of the cycle, from under its hip: planted
    // in front and sliding back over 12 frames, then lifted and swung
    // through. The far foot is half a cycle on.
    let step = [(0, 90.0, 0.0), (12, -90.0, 0.0), (15, -65.0, -28.0), (18, 0.0, -48.0), (21, 65.0, -22.0)];
    for (n, &(foot, hip)) in feet.iter().enumerate() {
        let mut keys: Vec<(u32, Point)> = step
            .iter()
            .map(|&(f, dx, dy)| ((f + n as u32 * 12) % 24, Point::new(hip + dx, 770.0 + dy)))
            .collect();
        keys.sort_by_key(|&(f, _)| f);
        // The cycle ends where it began, so it loops.
        keys.push((24, keys[0].1));
        for (frame, at) in keys {
            skeleton.move_bone(foot, at, frame);
        }
    }
    // The hips, low just after each contact and high as the legs pass.
    for (frame, drop) in
        [(0, 22.0), (3, 30.0), (6, 12.0), (9, 8.0), (12, 22.0), (15, 30.0), (18, 12.0), (21, 8.0), (24, 22.0)]
    {
        skeleton.move_bone(hips, Point::new(960.0, 380.0 + drop), frame);
    }
    let doc = &mut editor.doc;
    // The legs are drawn over each other, so each bends with its own limb
    // only; the body rides on the hips.
    for (layer, binding) in [
        (near, BoneBinding::Limb(thighs[0])),
        (far, BoneBinding::Limb(thighs[1])),
        (body, BoneBinding::Bone(hips)),
    ] {
        if let Some(layer) = doc.layer_mut(layer) {
            layer.bone_binding = binding;
        }
    }
    if let Some(layer) = doc.layer_mut(folder) {
        layer.name = "Walker".into();
        layer.skeleton = Some(skeleton);
    }
    editor.select(Some(folder));
    editor.selected_bone = Some(feet[0].0);
    if stage == "follows" {
        editor.select(Some(near));
    }
}

/// The guidebook's web banner project (chapter "Project: An Interactive
/// Web Banner"): a 970 × 250 stage with a background, a title that pops in
/// over the first frames, a line under it, and a Shop Now button that opens
/// a link, with a Stop on the last frame so the banner ends on the button.
fn banner_demo(editor: &mut Editor) {
    use inkframe_core::{Color, Element, Point, Shape, ShapeGradient, Style, Text, Transform, WebAction};
    let doc = &mut editor.doc;
    (doc.width, doc.height, doc.duration) = (970, 250, 36);
    let fill = |c: Color| Style { fill: Some(c), stroke: None, gradient: None };
    let mut sky = ShapeGradient::linear(Color::rgb(255, 214, 150), Color::rgb(240, 120, 110));
    (sky.from, sky.to) = (Point::new(0.0, 0.5), Point::new(1.0, 0.5));
    let background = doc.add_layer(
        "Background",
        vec![Element {
            shape: Shape::Rect { width: 970.0, height: 250.0, corner_radius: 0.0 },
            style: Style { fill: Some(Color::rgb(255, 214, 150)), stroke: None, gradient: Some(sky) },
        }],
        0,
        Transform::at(Point::new(485.0, 125.0)),
    );
    // Text is placed from its left edge.
    let text = |content: &str, size: f32, c: Color| Element {
        shape: Shape::Text(Text::new(content, size)),
        style: fill(c),
    };
    let title = doc.add_layer(
        "Title",
        vec![text("Spring Sale", 76.0, Color::rgb(60, 30, 50))],
        0,
        Transform::at(Point::new(60.0, 105.0)),
    );
    let line = doc.add_layer(
        "Tagline",
        vec![text("Everything 20% off this week", 30.0, Color::rgb(90, 50, 70))],
        6,
        Transform::at(Point::new(60.0, 175.0)),
    );
    let button = doc.add_layer(
        "Shop Now",
        vec![Element {
            shape: Shape::Rect { width: 220.0, height: 76.0, corner_radius: 38.0 },
            style: fill(Color::rgb(60, 30, 50)),
        }],
        12,
        Transform::at(Point::new(800.0, 125.0)),
    );
    // Its label rides on it, attached as a part of a rig is.
    let label = doc.add_layer(
        "Label",
        vec![text("Shop now", 30.0, Color::WHITE)],
        12,
        Transform::at(Point::new(734.0, 125.0)),
    );
    doc.set_parent(label, Some(button), 12);
    // Pop In, as the Motion Presets panel gives it: from nothing, a little
    // past full size, and settling.
    if let Some(layer) = doc.layer_mut(title) {
        let at = |scale: f32, opacity: f32| Transform {
            scale: Point::new(scale, scale),
            opacity,
            ..Transform::at(Point::new(60.0, 105.0))
        };
        layer.track.set_pose(0, at(0.2, 0.0));
        layer.track.set_pose(8, at(1.12, 1.0));
        layer.track.set_pose(12, at(1.0, 1.0));
    }
    if let Some(layer) = doc.layer_mut(line) {
        layer.track.set_pose(6, Transform { opacity: 0.0, ..Transform::at(Point::new(60.0, 175.0)) });
        layer.track.set_pose(14, Transform::at(Point::new(60.0, 175.0)));
    }
    if let Some(layer) = doc.layer_mut(button) {
        layer.track.set_pose(12, Transform::at(Point::new(1100.0, 125.0)));
        layer.track.set_pose(20, Transform::at(Point::new(800.0, 125.0)));
        layer.button = Some(WebAction::OpenLink("https://example.com/sale".into()));
    }
    doc.set_frame_action(35, Some(WebAction::Stop));
    let _ = background;
    editor.select(Some(button));
    editor.frame = 35;
}

/// Strokes drawn with art and pattern brushes (see `art_brush.rs` in the
/// core): three symbols to brush with, and a stroke drawn with each.
fn brushes_demo(editor: &mut Editor) {
    use inkframe_core::{
        BrushKind, Color, Element, Path, Point, Segment, Shape, Stroke, StrokeBrush, Style, TimelineId,
        Transform,
    };
    let fill = |c: Color| Style { fill: Some(c), stroke: None, gradient: None };
    let doc = &mut editor.doc;
    // A leaf on a short stem, pointing along the stroke.
    let leaf = Element {
        shape: Shape::Path(Path {
            start: Point::new(0.0, 0.0),
            segments: vec![
                Segment::Cubic {
                    ctrl1: Point::new(15.0, -22.0),
                    ctrl2: Point::new(45.0, -22.0),
                    to: Point::new(60.0, 0.0),
                },
                Segment::Cubic {
                    ctrl1: Point::new(45.0, 8.0),
                    ctrl2: Point::new(15.0, 8.0),
                    to: Point::new(0.0, 0.0),
                },
            ],
            closed: true,
        }),
        style: Style {
            fill: Some(Color::rgb(90, 180, 110)),
            stroke: Some(Stroke::new(Color::rgb(40, 100, 60), 2.0)),
            gradient: None,
        },
    };
    let stem = Element {
        shape: Shape::Rect { width: 60.0, height: 3.0, corner_radius: 1.5 },
        style: fill(Color::rgb(60, 120, 70)),
    }
    .transformed(inkframe_core::Affine::translate(Point::new(30.0, 6.0)))
    .unwrap();
    // A swoosh: thin, swelling in the middle, coming to a point.
    let swoosh = Element {
        shape: Shape::Path(Path {
            start: Point::new(0.0, 0.0),
            segments: vec![
                Segment::Cubic {
                    ctrl1: Point::new(60.0, -30.0),
                    ctrl2: Point::new(140.0, -25.0),
                    to: Point::new(200.0, 0.0),
                },
                Segment::Cubic {
                    ctrl1: Point::new(140.0, 10.0),
                    ctrl2: Point::new(60.0, 12.0),
                    to: Point::new(0.0, 0.0),
                },
            ],
            closed: true,
        }),
        style: fill(Color::rgb(60, 70, 160)),
    };
    let bead = Element {
        shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
        style: fill(Color::rgb(230, 170, 60)),
    };
    let link = Element {
        shape: Shape::Rect { width: 14.0, height: 4.0, corner_radius: 2.0 },
        style: fill(Color::rgb(160, 110, 40)),
    }
    .transformed(inkframe_core::Affine::translate(Point::new(16.0, 0.0)))
    .unwrap();
    let mut symbol = |name: &str, shapes: Vec<Element>| {
        let layer = doc.add_layer(name, shapes, 0, Transform::default());
        let symbol = doc.convert_layer_to_symbol(layer, 0);
        if let Some(symbol) = symbol.and_then(|id| doc.symbol_mut(id)) {
            symbol.name = name.into();
        }
        // The layer it was made from is just an instance now; it isn't wanted.
        doc.layers.retain(|l| l.id != layer);
        symbol
    };
    let (Some(leaf), Some(swoosh), Some(bead)) =
        (symbol("Leaf", vec![stem, leaf]), symbol("Swoosh", vec![swoosh]), symbol("Bead", vec![bead, link]))
    else {
        return;
    };
    let brushed = |kind: BrushKind, symbol, width: f32, fill_colour: Option<Color>| Style {
        fill: fill_colour,
        stroke: Some(Stroke {
            brush: Some(StrokeBrush { kind, symbol }),
            ..Stroke::new(Color::BLACK, width)
        }),
        gradient: None,
    };
    let vine = Element {
        shape: Shape::Path(Path {
            start: Point::new(-300.0, 0.0),
            segments: vec![
                Segment::Cubic {
                    ctrl1: Point::new(-150.0, -160.0),
                    ctrl2: Point::new(-50.0, 160.0),
                    to: Point::new(100.0, 0.0),
                },
                Segment::Cubic {
                    ctrl1: Point::new(200.0, -110.0),
                    ctrl2: Point::new(260.0, -40.0),
                    to: Point::new(300.0, 40.0),
                },
            ],
            closed: false,
        }),
        style: brushed(BrushKind::Pattern, leaf, 34.0, None),
    };
    let flourish = Element {
        shape: Shape::Path(Path {
            start: Point::new(-280.0, 40.0),
            segments: vec![Segment::Cubic {
                ctrl1: Point::new(-100.0, -90.0),
                ctrl2: Point::new(120.0, 120.0),
                to: Point::new(280.0, -30.0),
            }],
            closed: false,
        }),
        style: brushed(BrushKind::Art, swoosh, 46.0, None),
    };
    let card = Element {
        shape: Shape::Rect { width: 360.0, height: 220.0, corner_radius: 30.0 },
        style: brushed(BrushKind::Pattern, bead, 18.0, Some(Color::rgb(250, 240, 220))),
    };
    doc.add_layer("Vine", vec![vine], 0, Transform::at(Point::new(520.0, 330.0)));
    doc.add_layer("Swoosh", vec![flourish], 0, Transform::at(Point::new(520.0, 760.0)));
    let id = doc.add_layer("Card", vec![card], 0, Transform::at(Point::new(1380.0, 540.0)));
    let _ = TimelineId::Main;
    editor.select(Some(id));
}

/// One thick, wavy pencil line, four times over: flat, then with each of
/// the line textures, on layers of their own.
fn textures_demo(editor: &mut Editor) {
    use inkframe_core::{
        Color, Element, LineTexture, Path, Point, Segment, Shape, Style, TextureKind, Transform,
    };
    let wave = Path {
        start: Point::new(-380.0, 0.0),
        segments: vec![
            Segment::Cubic {
                ctrl1: Point::new(-250.0, -90.0),
                ctrl2: Point::new(-130.0, 90.0),
                to: Point::new(0.0, 0.0),
            },
            Segment::Cubic {
                ctrl1: Point::new(130.0, -90.0),
                ctrl2: Point::new(250.0, 90.0),
                to: Point::new(380.0, 0.0),
            },
        ],
        closed: false,
    };
    let line = |width: f32| Element {
        shape: Shape::Line { path: wave.clone(), widths: vec![width * 0.4, width, width * 0.4] },
        style: Style { fill: Some(Color::rgb(35, 35, 45)), stroke: None, gradient: None },
    };
    let rows = [
        (None, "Flat"),
        (Some(TextureKind::Pencil), "Pencil"),
        (Some(TextureKind::Charcoal), "Charcoal"),
        (Some(TextureKind::Chalk), "Chalk"),
    ];
    let mut last = None;
    for (i, (kind, name)) in rows.into_iter().enumerate() {
        let id = editor.doc.add_layer(
            name,
            vec![line(30.0)],
            0,
            Transform::at(Point::new(960.0, 200.0 + 220.0 * i as f32)),
        );
        if let Some(layer) = editor.doc.layer_mut(id) {
            layer.line_texture = kind.map(|kind| LineTexture { kind, amount: 0.8 });
        }
        last = Some(id);
    }
    editor.select(last);
}

/// A two-point perspective guide, and a block drawn to it: its near corner
/// upright, its two faces running off toward the vanishing points.
fn perspective_demo(editor: &mut Editor) {
    use inkframe_core::{
        Color, Element, Path, Perspective, PerspectiveKind, Point, Segment, Shape, Stroke, Style, Transform,
    };
    let (w, h) = (editor.doc.width as f32, editor.doc.height as f32);
    let guide = Perspective::new(PerspectiveKind::TwoPoint, w, h);
    let (left, right) = (guide.points[0], guide.points[1]);
    // The near corner, from the ground up; each face's far corner is part
    // way toward its vanishing point.
    let (foot, head) = (Point::new(960.0, 900.0), Point::new(960.0, 520.0));
    let toward = |p: Point, vp: Point, t: f32| p + (vp - p) * t;
    let face = |vp: Point, t: f32, colour: Color| {
        let (far_foot, far_head) = (toward(foot, vp, t), toward(head, vp, t));
        let points = [foot, far_foot, far_head, head];
        Element {
            shape: Shape::Path(Path {
                start: points[0],
                segments: points[1..].iter().map(|&to| Segment::Line { to }).collect(),
                closed: true,
            }),
            style: Style {
                fill: Some(colour),
                stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 4.0)),
                gradient: None,
            },
        }
    };
    let roof = {
        let (l, r) = (toward(head, left, 0.25), toward(head, right, 0.18));
        // The back corner: where the lines from each far corner to the other
        // vanishing point cross, near enough the sum of the two sides.
        let back = l + (r - head);
        Element {
            shape: Shape::Path(Path {
                start: head,
                segments: [l, back, r].iter().map(|&to| Segment::Line { to }).collect(),
                closed: true,
            }),
            style: Style {
                fill: Some(Color::rgb(230, 120, 100)),
                stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 4.0)),
                gradient: None,
            },
        }
    };
    let shapes =
        vec![face(left, 0.25, Color::rgb(250, 220, 150)), face(right, 0.18, Color::rgb(220, 190, 130)), roof];
    let id = editor.doc.add_layer("Block", shapes, 0, Transform::default());
    editor.doc.perspective = Some(guide);
    editor.select(Some(id));
}

/// Two key drawings, a square on frame 1 and a five-pointed star on frame
/// 9 off to the right, with the inbetweens drawn between them (eased out
/// and in), shown on frame 5 through onion skins either side.
fn inbetweens_demo(editor: &mut Editor) {
    use inkframe_core::{
        Color, Drawing, Easing, Element, Path, Point, Segment, Shape, Stroke, Style, Transform,
    };
    let style = Style {
        fill: Some(Color::rgb(250, 190, 80)),
        stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 5.0)),
        gradient: None,
    };
    let square = Element {
        shape: Shape::Rect { width: 160.0, height: 160.0, corner_radius: 0.0 },
        style: style.clone(),
    };
    let star: Vec<Point> = (0..10)
        .map(|i| {
            let angle = std::f32::consts::PI * i as f32 / 5.0 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 130.0 } else { 55.0 };
            Point::new(900.0 + r * angle.cos(), r * angle.sin())
        })
        .collect();
    let star = Element {
        shape: Shape::Path(Path {
            start: star[0],
            segments: star[1..].iter().map(|&to| Segment::Line { to }).collect(),
            closed: true,
        }),
        style,
    };
    let id = editor.doc.add_layer("Shape", vec![square], 0, Transform::at(Point::new(500.0, 540.0)));
    if let Some(layer) = editor.doc.layer_mut(id) {
        let key = layer.add_drawing(Drawing::new("2", vec![star]));
        layer.expose(8, key);
        layer.set_shape_tween(0, true);
        if let Some(tween) = layer.shape_tween_mut(0) {
            tween.easing = Easing::EaseInOut;
        }
        layer.make_inbetweens(0);
    }
    editor.select(Some(id));
    editor.frame = 4;
    editor.onion.enabled = true;
    editor.onion.before = 4;
    editor.onion.after = 4;
}

/// On top of `skeleton_demo`'s character: a correction on the spine (turned
/// 10°), keyed there, reshaping the body by hand into a pear that sags
/// toward the way it leans. The spine is picked, to show the key.
fn correction_demo(editor: &mut Editor) {
    use inkframe_core::{CorrectionKey, Element, Path, Point, Segment, Shape, SmartAction};
    let Some(folder) = editor.selection else { return };
    let Some(body) = editor.doc.layers.iter().find(|l| l.name == "Body").map(|l| l.id) else { return };
    let Some(style) = editor.doc.layer(body).map(|l| l.drawings[0].elements[0].style.clone()) else { return };
    // A pear: narrow at the chest, broad and low at the hips.
    let pear = Element {
        shape: Shape::Path(Path {
            start: Point::new(0.0, -170.0),
            segments: vec![
                Segment::Cubic {
                    ctrl1: Point::new(70.0, -170.0),
                    ctrl2: Point::new(150.0, 60.0),
                    to: Point::new(110.0, 140.0),
                },
                Segment::Cubic {
                    ctrl1: Point::new(70.0, 190.0),
                    ctrl2: Point::new(-90.0, 190.0),
                    to: Point::new(-110.0, 120.0),
                },
                Segment::Cubic {
                    ctrl1: Point::new(-130.0, 40.0),
                    ctrl2: Point::new(-70.0, -170.0),
                    to: Point::new(0.0, -170.0),
                },
            ],
            closed: true,
        }),
        style,
    };
    let key = CorrectionKey { turn: 10.0, colour_art: Vec::new(), line_art: vec![pear] };
    if let Some(skeleton) = editor.doc.layer_mut(folder).and_then(|l| l.skeleton.as_mut()) {
        skeleton.bones[0].actions.push(SmartAction::Correct { layer: body, drawing: 0, keys: vec![key] });
    }
    editor.selected_bone = Some(0);
}

/// A web page made interactive: a ball rolls across and stops on its last
/// frame (a frame action, marked "a" on the ruler), and two buttons, Replay
/// (back to the start, playing) and Pause. Replay is picked, to show its
/// Button setting.
fn buttons_demo(editor: &mut Editor) {
    use inkframe_core::{Color, Element, Point, Shape, Style, Transform, WebAction};
    let fill = |c: Color| Style { fill: Some(c), stroke: None, gradient: None };
    let doc = &mut editor.doc;
    let ball = Element {
        shape: Shape::Ellipse { radius_x: 60.0, radius_y: 60.0 },
        style: fill(Color::rgb(230, 90, 100)),
    };
    let id = doc.add_layer("Ball", vec![ball], 0, Transform::at(Point::new(200.0, 500.0)));
    if let Some(layer) = doc.layer_mut(id) {
        layer.track.set_pose(47, Transform::at(Point::new(1700.0, 500.0)));
    }
    let button = |c: Color| Element {
        shape: Shape::Rect { width: 220.0, height: 80.0, corner_radius: 20.0 },
        style: fill(c),
    };
    let replay = doc.add_layer(
        "Replay",
        vec![button(Color::rgb(80, 160, 90))],
        0,
        Transform::at(Point::new(760.0, 850.0)),
    );
    let pause = doc.add_layer(
        "Pause",
        vec![button(Color::rgb(90, 120, 200))],
        0,
        Transform::at(Point::new(1160.0, 850.0)),
    );
    for (layer, action) in [(replay, WebAction::GotoAndPlay(0)), (pause, WebAction::Stop)] {
        if let Some(layer) = doc.layer_mut(layer) {
            layer.button = Some(action);
        }
    }
    doc.set_frame_action(47, Some(WebAction::Stop));
    editor.select(Some(replay));
    editor.frame = 20;
}
