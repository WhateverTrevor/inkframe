//! The top-level application: window layout, menus, shortcuts and files.

mod areas;
mod commands;
mod files;
mod keys;
mod menus;

use std::path::{Path, PathBuf};

use inkframe_core::{Clipboard, Document, Preset, TimelineId};
use inkframe_render::IMAGE_EXTENSIONS;

use crate::adjust_dialog::{AdjustDialog, AdjustmentKind};
use crate::audio_engine::AudioEngine;
use crate::autosave::{Autosave, Recovery};
use crate::batch_dialog::BatchDialog;
use crate::editor::{Editor, Tool};
use crate::examples::ExamplesWindow;
use crate::export_dialog::ExportDialog;
use crate::guides::SafeAreas;
use crate::i18n::{tr, tr_args};
use crate::layout::{AreaId, Kind, Layout, Workspace};
use crate::library::LibraryView;
use crate::lip_sync_dialog::LipSyncDialog;
use crate::panels;
use crate::prefs::Prefs;
use crate::screenshot::AutoScreenshot;
use crate::swap_dialog::SwapDialog;
use crate::vectorise_dialog::VectoriseDialog;
use crate::xsheet::ExposureSheet;
use areas::{Views, drop_hint};

/// Everything the user can trigger from a menu or a keyboard shortcut.
/// Routing both through one enum keeps menus and shortcuts in sync.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Command {
    /// Opens Find a Command (see `command_finder.rs`).
    OpenFinder,
    /// Picks a tool (the command finder lists them).
    UseTool(Tool),
    New,
    Open,
    ImportImage,
    /// File › Import Sprite Sheet… (see `sprite_import_dialog.rs`).
    ImportSpriteSheet,
    ImportAudio,
    ImportFont,
    Save,
    SaveAs,
    Export,
    /// Opens the Batch Export window.
    BatchExport,
    /// Opens the gallery of example projects.
    OpenExample,
    /// Help › Take the Tour: the first-run tour again.
    StartTour,
    /// Help › What's New: this version's changes.
    WhatsNew,
    /// Help › Check for Updates… (see `updates.rs`).
    CheckForUpdates,
    /// Opens the Vectorise window for the selected layer.
    Vectorise,
    NewParticleLayer(inkframe_core::Preset),
    Undo,
    Redo,
    SelectAll,
    Deselect,
    /// Select › Inverse: the unselected pixels instead.
    InvertSelection,
    LayerViaCopy,
    LayerViaCut,
    MergeDown,
    /// The area under the pointer fills the window, or goes back.
    ToggleMaximize,
    /// Puts the window's areas back the way Inkframe starts.
    ResetLayout,
    Workspace(Workspace),
    /// Writes the current frame as a layered Photoshop file.
    ExportPsd,
    /// Opens the Adjustments window with this adjustment.
    Adjust(AdjustmentKind),
    Cut,
    Copy,
    /// Pastes layers in the middle of the view (or a keyframe at the playhead).
    Paste,
    /// Pastes layers exactly where they were copied from.
    PasteInPlace,
    /// Copies the selected layer's pose on the current frame.
    CopyPose,
    /// Pastes a copied pose onto the selected layer at the current frame.
    PastePose,
    /// Cut, Copy and Paste Frames: the keyframes on the frames picked in
    /// the timeline, pasted at the playhead (see `frame_commands.rs`).
    CutFrames,
    CopyFrames,
    PasteFrames,
    DuplicateLayer,
    DeleteLayer,
    ConvertToSymbol,
    /// Makes the selected layer a mask over the layer below it.
    MaskLayerBelow,
    /// Clips the selected layers to the layer beneath, or releases them
    /// (see `layer_masks.rs`).
    ToggleClipping,
    AddLayerMask,
    DeleteLayerMask,
    ApplyLayerMask,
    /// Turns the layer mask off or back on.
    ToggleLayerMask,
    /// Paints the layer mask, or the layer again.
    EditLayerMask,
    /// Opens the selected instance's symbol for editing.
    EditSymbol,
    /// Steps out of the symbol being edited.
    ExitSymbol,
    InsertKeyframe,
    RemoveKeyframe,
    LayerStartHere,
    LayerEndHere,
    /// Adds a blank drawing to the selected layer, shown from this frame.
    NewDrawing,
    /// Copies the drawing showing now into a new one, shown from this frame.
    DuplicateDrawing,
    /// Removes the drawing showing now.
    DeleteDrawing,
    PreviousDrawing,
    NextDrawing,
    /// Softer or harder brush edges (Shift+[ and Shift+]).
    SofterBrush,
    HarderBrush,
    /// Edit › Define Brush Preset: a brush tip from the selected pixels.
    DefineBrush,
    /// Edit › Define Pattern: a brush texture from the selected pixels.
    DefinePattern,
    ShowBrushSettings,
    ShowBrushes,
    /// Opens the Lip Sync window for the selected layer.
    LipSync,
    NewLayer,
    NewPaintLayer,
    // Frame commands (see `frame_commands.rs`).
    InsertFrames,
    RemoveFrames,
    InsertBlankKeyframe,
    ConvertToKeyframes,
    ConvertToBlankKeyframes,
    ReverseFrames,
    // Combining shapes (see `shape_ops.rs`).
    CombineShapes(inkframe_core::ShapeOp),
    BreakApart,
    // Distort and Envelope (see `envelope.rs`).
    StartLattice(crate::envelope::LatticeKind),
    // Skeletons (see `skeleton.rs`).
    NewSkeleton,
    FinishCorrection,
    CancelCorrection,
    /// Shows a smart bone previewed on its action timeline as animated again.
    EndPreview,
    NewSwitch,
    DeleteBone,
    FinishLattice,
    SwapSymbol,
    // Rulers and guides (see `stage/rulers.rs`).
    ToggleRulers,
    ToggleRigSetup,
    /// Lays a perspective guide of this kind over the stage, or takes it
    /// away (see `perspective.rs` in the core).
    SetPerspective(Option<inkframe_core::PerspectiveKind>),
    ToggleGuides,
    ToggleLockGuides,
    ToggleSnapToGuides,
    ClearGuides,
    NewFolder,
    CreateShapeTween,
    MakeInbetweens,
    AddCamera,
    RemoveShapeTween,
    AddShapeHint,
    RemoveShapeHints,
    PlayPause,
    StepBackward,
    StepForward,
    FirstFrame,
    LastFrame,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ZoomActualSize,
    /// Turns the view (not the drawing) 15° one way or the other, or back.
    RotateViewLeft,
    RotateViewRight,
    ResetRotation,
    /// Mirrors the view (not the drawing) left to right, or back.
    FlipView,
    /// A toolbar slot's key: its last-used tool, or with `cycle` the next
    /// tool in the slot.
    UseSlot {
        group: &'static [Tool],
        cycle: bool,
    },
    /// Ends the pen path being drawn, leaving it open.
    FinishPath,
    /// Removes the newest point of the pen path being drawn.
    RemovePathPoint,
    /// Deletes the anchor point picked with the Subselection tool.
    DeleteAnchor,
    /// Steps out of the layer being worked inside (see `inside.rs`).
    ExitLayer,
    DeleteShapes,
}

pub(crate) const FILE_EXTENSION: &str = "inkf";
/// The guidebook PDF is attached to each release.
pub(crate) const GUIDEBOOK_URL: &str = "https://github.com/WhateverTrevor/inkframe/releases/latest";
pub(crate) const SCRIPTING_URL: &str =
    "https://github.com/WhateverTrevor/inkframe/blob/main/docs/SCRIPTING.md";

/// Opens a web page in the default browser.
pub(crate) fn open_in_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        // `start` is built into cmd; the empty title keeps it from taking
        // the URL as the window's title.
        std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(err) = result {
        eprintln!("couldn't open {url}: {err}");
    }
}

pub struct InkframeApp {
    editor: Editor,
    /// How the window is divided into areas (see `layout.rs`), and the
    /// layout as last saved, to save again only when it changes.
    layout: Layout,
    saved_layout: Layout,
    /// The settings last saved for next time (see `prefs.rs`).
    saved_prefs: Prefs,
    /// Copies unsaved work for crash recovery; `None` in screenshot runs.
    autosave: Option<Autosave>,
    /// Work left behind by an Inkframe that didn't close properly, offered
    /// back when Inkframe starts.
    recoveries: Vec<Recovery>,
    /// The editors in the areas that keep state of their own.
    views: Views,
    /// The area under the pointer last frame, for Ctrl+Space.
    hovered_area: Option<AreaId>,
    export: ExportDialog,
    batch: BatchDialog,
    vectorise: VectoriseDialog,
    swap: SwapDialog,
    examples: ExamplesWindow,
    scripts: crate::scripts::Scripts,
    lip_sync: LipSyncDialog,
    sprite_import: crate::sprite_import_dialog::SpriteImportDialog,
    xsheet: ExposureSheet,
    adjust: AdjustDialog,
    library: LibraryView,
    panels: panels::PanelState,
    brush_panels: crate::brush_panel::BrushPanels,
    templates: crate::templates::TemplateLibrary,
    presets: crate::motion_presets::PresetLibrary,
    audio: AudioEngine,
    /// The window title we last set, so we only send changes.
    title: String,
    /// Set once the user has confirmed quitting with unsaved changes.
    allow_close: bool,
    screenshot: Option<AutoScreenshot>,
    /// What was last copied. Kept here as well as on the system clipboard,
    /// because egui only reports a paste when the system clipboard has
    /// text, and the Edit menu has no other way to read it.
    clipboard: Option<Clipboard>,
    ctx: egui::Context,
    /// Find a Command (see `command_finder.rs`).
    finder: crate::command_finder::CommandFinder,
    /// A tool picked by holding its key (see `keys.rs`).
    held_tool: Option<keys::HeldTool>,
    /// The first-run tour (see `tour.rs`), where the parts of the window
    /// it points at were drawn this frame, and whether it's been seen.
    tour: crate::tour::Tour,
    tour_targets: crate::tour::Targets,
    /// For the screenshot hook's bench: frames left to step through, and
    /// how long each update took (see `AutoScreenshot::bench`).
    bench: Option<(u32, Vec<std::time::Duration>)>,
    toured: bool,
    /// What changed in this version, shown once after an update.
    whats_new: crate::whats_new::WhatsNew,
    updates: crate::updates::Updates,
    update_prefs: crate::updates::UpdatePrefs,
    /// The newest version whose What's New has been seen, kept with the
    /// settings: `None` before this one first ran.
    seen_version: Option<String>,
    /// The language picked in View › Language (none: the system's). It's
    /// used from the next start; see `i18n.rs`.
    language: Option<String>,
}

impl InkframeApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open_path: Option<PathBuf>) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        // Indexing the system's fonts takes a moment; do it now, off the UI
        // thread, so the first text drawn doesn't pause the app.
        std::thread::spawn(inkframe_render::load_fonts);
        crate::tablet::install();

        let editor = match open_path {
            Some(path) => Editor::open(&path).unwrap_or_else(|err| {
                show_error(
                    &tr_args("Couldn't open {file}", &[("file", &path.display().to_string())]),
                    &err.to_string(),
                );
                Editor::new(Document::default())
            }),
            None => Editor::new(Document::default()),
        };

        let mut app = Self {
            editor,
            layout: Layout::load(),
            saved_layout: Layout::default(),
            saved_prefs: Prefs::default(),
            autosave: None,
            recoveries: Vec::new(),
            views: Views::default(),
            hovered_area: None,
            export: ExportDialog::default(),
            batch: BatchDialog::default(),
            vectorise: VectoriseDialog::default(),
            swap: SwapDialog::default(),
            examples: ExamplesWindow::default(),
            scripts: crate::scripts::Scripts::default(),
            lip_sync: LipSyncDialog::default(),
            sprite_import: Default::default(),
            xsheet: ExposureSheet::default(),
            adjust: AdjustDialog::default(),
            library: LibraryView::default(),
            panels: panels::PanelState::default(),
            brush_panels: Default::default(),
            templates: crate::templates::TemplateLibrary::default(),
            presets: crate::motion_presets::PresetLibrary::default(),
            audio: AudioEngine::default(),
            title: String::new(),
            allow_close: false,
            screenshot: AutoScreenshot::from_env(),
            clipboard: None,
            ctx: cc.egui_ctx.clone(),
            finder: Default::default(),
            held_tool: None,
            tour: Default::default(),
            tour_targets: Default::default(),
            bench: AutoScreenshot::bench().map(|frames| (frames, Vec::new())),
            toured: false,
            whats_new: Default::default(),
            updates: Default::default(),
            update_prefs: Default::default(),
            seen_version: None,
            language: None,
        };
        // Screenshots always use the usual layout, so they look the same
        // on every machine.
        if app.screenshot.is_some() {
            app.layout = AutoScreenshot::workspace().map_or_else(Layout::default, Layout::workspace);
            if AutoScreenshot::custom_layout() {
                app.layout.arrange_for_screenshot();
            }
        }
        app.saved_layout = app.layout.clone();
        // Screenshots start from a clean slate and never touch the
        // person's settings.
        if app.screenshot.is_none() || AutoScreenshot::load_prefs() {
            let prefs = Prefs::load();
            app.editor.brushes = crate::brushes::BrushLibrary::load();
            if let Some(brush) = prefs.brush.clone() {
                app.editor.brush = brush;
            }
            if let Some(eraser) = prefs.eraser.clone() {
                app.editor.eraser = eraser;
            }
            app.editor.brushes.picked = prefs.brush_name.clone();
            app.editor.symmetry = prefs.symmetry;
            app.editor.smoothing = prefs.smoothing.unwrap_or(crate::editor::DEFAULT_SMOOTHING);
            app.editor.pressure_feel = prefs.pressure_feel;
            crate::color_picker::set_recent(prefs.recent_colors.clone());
            app.update_prefs = crate::updates::UpdatePrefs {
                automatic: !prefs.no_update_check,
                checked_at: prefs.update_checked_at,
                skipped: prefs.skipped_version.clone(),
            };
            app.editor.guides = prefs.guides();
            app.editor.draw_into_layer = !prefs.shape_per_layer;
            app.editor.show_rulers = prefs.show_rulers;
            app.editor.show_guides = !prefs.hide_guides;
            app.editor.lock_guides = prefs.lock_guides;
            app.editor.snap_to_guides = !prefs.no_guide_snapping;
            app.batch.restore(prefs.batch_jobs(), prefs.batch_folder.clone());
            app.toured = prefs.toured;
            app.seen_version = prefs.seen_version.clone();
            app.language = prefs.language.clone();
            app.editor.folded_sections = prefs.folded_sections.iter().cloned().collect();
        }
        // The tour shows the first time Inkframe opens, and never in a
        // screenshot unless asked for.
        if app.screenshot.is_none() && !app.toured {
            app.tour.start();
        } else if app.screenshot.is_none() && app.seen_version.as_deref() != Some(env!("CARGO_PKG_VERSION")) {
            // Updated since last time: what changed. (Someone new gets the
            // tour instead, and this version's notes count as seen.)
            app.whats_new.open();
        }
        app.seen_version = Some(env!("CARGO_PKG_VERSION").to_owned());
        if AutoScreenshot::whats_new() {
            app.whats_new.open();
        }
        if let Some(card) = AutoScreenshot::tour() {
            app.tour.start_at(card);
        }
        app.saved_prefs = app.current_prefs();
        if app.screenshot.is_none() {
            crate::scripts::add_examples_if_new();
            let autosave = Autosave::new();
            app.recoveries = autosave.left_behind();
            app.autosave = Some(autosave);
        } else if AutoScreenshot::recovery() {
            // Work "left behind" in a scratch folder, to show the window.
            let dir = std::env::temp_dir().join("inkframe-recovery-screenshot");
            app.recoveries = Autosave::simulate_crash(&app.editor, dir).left_behind();
        }
        app.views.prepare(&app.layout);
        if app.screenshot.is_some() {
            AutoScreenshot::set_up_scene(&mut app.editor);
            if let Some(name) = AutoScreenshot::type_into() {
                app.views.stage().type_into(&mut app.editor, &name);
            }
            if AutoScreenshot::pen_handles() {
                app.views.stage().pick_anchor(&app.editor, 0, 1);
            }
            if AutoScreenshot::graph() {
                app.layout.timelines_to_graphs();
            }
            if AutoScreenshot::motion_blur() {
                use inkframe_core::{
                    Color, Effects, Element, Filter, FilterKind, Point, Shape, Stroke, Style, Transform,
                };
                let ball = Element {
                    shape: Shape::Ellipse { radius_x: 70.0, radius_y: 70.0 },
                    style: Style {
                        fill: Some(Color::rgb(0xe8, 0x4a, 0x5f)),
                        stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 6.0)),
                        gradient: None,
                    },
                };
                let doc = &mut app.editor.doc;
                let id = doc.add_layer("Ball", vec![ball], 0, Transform::at(Point::new(300.0, 760.0)));
                if let Some(layer) = doc.layer_mut(id) {
                    layer.track.set_pose(8, Transform::at(Point::new(1700.0, 760.0)));
                    let blur = Filter {
                        enabled: true,
                        kind: FilterKind::MotionBlur { shutter: 180.0, samples: 16 },
                    };
                    layer.track.set_effects(0, Effects { filters: vec![blur], ..Default::default() });
                }
                app.editor.select(Some(id));
            }
            if AutoScreenshot::pressure_lines() {
                use inkframe_core::{Color, Element, Path, Point, Segment, Shape, Style, Transform};
                // Three wavy strokes, pressed hard in the middle and light at
                // the ends, as a pen would draw them.
                let strokes: Vec<Element> = (0..3)
                    .map(|k| {
                        let points: Vec<Point> = (0..=40)
                            .map(|i| {
                                let t = i as f32 / 40.0;
                                Point::new(
                                    -300.0 + 600.0 * t,
                                    k as f32 * 90.0 - 90.0 + 30.0 * (t * 9.0).sin(),
                                )
                            })
                            .collect();
                        let widths = (0..=40)
                            .map(|i| {
                                let t = i as f32 / 40.0;
                                (2.0 + 22.0 * (t * std::f32::consts::PI).sin()) * (1.0 + k as f32 * 0.4)
                            })
                            .collect();
                        let path = Path {
                            start: points[0],
                            segments: points[1..].iter().map(|&to| Segment::Line { to }).collect(),
                            closed: false,
                        };
                        Element {
                            shape: Shape::Line { path, widths },
                            style: Style { fill: Some(Color::rgb(30, 30, 40)), stroke: None, gradient: None },
                        }
                    })
                    .collect();
                let id =
                    app.editor.doc.add_layer("Ink", strokes, 0, Transform::at(Point::new(1450.0, 650.0)));
                app.editor.select(Some(id));
                app.editor.tool = crate::editor::Tool::Width;
            }
            if AutoScreenshot::templates() {
                let folder = std::env::temp_dir()
                    .join(format!("inkframe-screenshot-templates-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&folder);
                app.templates.use_folder(folder);
                for (name, layers) in
                    [("Arm", &["Upper arm", "Forearm", "Hand"][..]), ("Head", &["Head", "Mouth"][..])]
                {
                    let ids: Vec<_> = app
                        .editor
                        .layers()
                        .iter()
                        .filter(|l| layers.contains(&l.name.as_str()))
                        .map(|l| l.id)
                        .collect();
                    app.editor.select_many(&ids);
                    app.templates.save_selected(&app.editor, name);
                }
                app.layout.add_tab_beside(Kind::Library, Kind::Templates);
            }
            if AutoScreenshot::node_view() {
                app.layout.replace_kind(Kind::Timeline, Kind::NodeView);
            }
            if AutoScreenshot::light_table() {
                app.editor.light_table = true;
                let head = app.editor.layers().iter().find(|l| l.name == "Head").map(|l| l.id);
                app.editor.select(head);
            }
            if AutoScreenshot::ink_and_paint() {
                use inkframe_core::{Color, Element, Path, Point, Segment, Shape, Stroke, Style, Transform};
                let ink = Style {
                    fill: None,
                    stroke: Some(Stroke::new(Color::rgb(30, 30, 40), 6.0)),
                    gradient: None,
                };
                // An outline as straight pieces round an ellipse, leaving
                // out the last few: a gap the bucket has to close.
                let ring = |rx: f32, ry: f32, cx: f32, cy: f32, gap: usize| {
                    let points: Vec<Point> = (0..=(96 - gap))
                        .map(|i| {
                            let a = i as f32 / 96.0 * std::f32::consts::TAU;
                            Point::new(cx + rx * a.cos(), cy + ry * a.sin())
                        })
                        .collect();
                    Element {
                        shape: Shape::Path(Path {
                            start: points[0],
                            segments: points[1..].iter().map(|&to| Segment::Line { to }).collect(),
                            closed: false,
                        }),
                        style: ink.clone(),
                    }
                };
                let lines = vec![
                    ring(220.0, 180.0, 0.0, 0.0, 1),
                    ring(40.0, 50.0, -70.0, -30.0, 0),
                    ring(40.0, 50.0, 70.0, -30.0, 0),
                ];
                let doc = &mut app.editor.doc;
                let id = doc.add_layer("Ink", lines, 0, Transform::at(Point::new(1450.0, 600.0)));
                if let Some(layer) = doc.layer_mut(id) {
                    layer.fill_enclosed(0, Point::new(0.0, 100.0), Color::rgb(0xf2, 0xb8, 0x8a), 14.0);
                    layer.fill_enclosed(0, Point::new(-70.0, -30.0), Color::rgb(0xff, 0xff, 0xff), 14.0);
                    layer.fill_enclosed(0, Point::new(70.0, -30.0), Color::rgb(0xff, 0xff, 0xff), 14.0);
                }
                app.editor.select(Some(id));
                app.editor.tool = crate::editor::Tool::PaintBucket;
            }
            if AutoScreenshot::palette() {
                let doc = &mut app.editor.doc;
                let mut colours: Vec<inkframe_core::Color> = Vec::new();
                for element in doc.all_layers().flat_map(|l| l.all_elements()) {
                    let used = element.style.fill.into_iter().chain(element.style.stroke.map(|s| s.color));
                    for colour in used {
                        if !colours.contains(&colour) {
                            colours.push(colour);
                        }
                    }
                }
                for (i, colour) in colours.iter().enumerate() {
                    doc.add_swatch(format!("Colour {}", i + 1), *colour);
                }
                let night = doc.add_palette_version("Night");
                for swatch in &mut doc.palette.swatches {
                    let c = swatch.colors[0];
                    let dim = |v: u8, k: f32| (v as f32 * k) as u8;
                    swatch.colors[night] =
                        inkframe_core::Color::rgb(dim(c.r, 0.45), dim(c.g, 0.5), dim(c.b, 0.8));
                }
                doc.use_palette_version(night);
            }
            if AutoScreenshot::effects() {
                use inkframe_core::{Color, ColorEffect, Effects, Filter, FilterKind};
                let find = |editor: &crate::editor::Editor, name: &str| {
                    editor.layers().iter().find(|l| l.name == name).map(|l| l.id)
                };
                let shadow = FilterKind::DropShadow {
                    distance: 18.0,
                    angle: 45.0,
                    blur: 14.0,
                    color: Color::rgba(0, 0, 0, 160),
                    strength: 1.0,
                };
                let glow = FilterKind::Glow {
                    blur: 16.0,
                    color: Color::rgb(0xff, 0xd0, 0x40),
                    strength: 1.5,
                    inner: false,
                };
                let tint = ColorEffect::Tint { color: Color::rgb(0x40, 0xc0, 0x80), amount: 0.6 };
                let looks = [
                    (
                        "Torso",
                        Effects {
                            filters: vec![Filter { enabled: true, kind: shadow }],
                            ..Default::default()
                        },
                    ),
                    (
                        "Head",
                        Effects { filters: vec![Filter { enabled: true, kind: glow }], ..Default::default() },
                    ),
                    ("Tail", Effects { color: tint, filters: Vec::new() }),
                ];
                // The head's glow grows in over the first second; the rest
                // stay as they are.
                for (name, effects) in looks {
                    let frame = if name == "Head" { 24 } else { 0 };
                    if let Some(layer) = find(&app.editor, name).and_then(|id| app.editor.doc.layer_mut(id)) {
                        layer.track.set_effects(frame, effects);
                    }
                }
                app.editor.select(find(&app.editor, "Head"));
            }
            if AutoScreenshot::camera() {
                let camera = app.editor.doc.add_camera();
                let pose = inkframe_core::Transform {
                    rotation: -8.0,
                    scale: inkframe_core::Point::new(1.4, 1.4),
                    ..inkframe_core::Transform::at(inkframe_core::Point::new(900.0, 500.0))
                };
                app.editor.doc.set_pose(camera, 0, pose);
                app.editor.select(Some(camera));
                app.editor.tool = crate::editor::Tool::Camera;
            }
            if AutoScreenshot::shape_tween() {
                use inkframe_core::{Color, Drawing, Element, Point, Shape, Style, Transform};
                let fill = |r, g, b| Style { fill: Some(Color::rgb(r, g, b)), stroke: None, gradient: None };
                let square = Element {
                    shape: Shape::Rect { width: 300.0, height: 300.0, corner_radius: 0.0 },
                    style: fill(0xe8, 0x4a, 0x5f),
                };
                let circle = Element {
                    shape: Shape::Ellipse { radius_x: 220.0, radius_y: 140.0 },
                    style: fill(0x3d, 0x8b, 0xfd),
                };
                let doc = &mut app.editor.doc;
                let id = doc.add_layer("Morph", vec![square], 0, Transform::at(Point::new(1500.0, 760.0)));
                if let Some(layer) = doc.layer_mut(id) {
                    let circle = layer.add_drawing(Drawing::new("circle", vec![circle]));
                    layer.expose(24, circle);
                    layer.set_shape_tween(0, true);
                }
                app.editor.select(Some(id));
                app.editor.add_shape_hint();
            }
            if AutoScreenshot::folder() {
                let arm: Vec<_> = ["Upper arm", "Forearm", "Hand"]
                    .iter()
                    .filter_map(|name| app.editor.layers().iter().find(|l| l.name == *name).map(|l| l.id))
                    .collect();
                app.editor.doc.add_folder(TimelineId::Main, &arm);
            }
            app.xsheet.open = AutoScreenshot::exposure_sheet();
            if let Some(format) = AutoScreenshot::export() {
                app.export.open_with(&app.editor.doc, format);
            }
            if let Some(guides) = AutoScreenshot::guides() {
                app.editor.guides.field_guide = guides.contains("field");
                if guides.contains("safe") {
                    app.editor.guides.safe_areas = SafeAreas::Current;
                }
            }
            if AutoScreenshot::load_prefs() {
                app.batch.open();
            }
            if AutoScreenshot::batch()
                && let Some(path) = app.editor.path.clone()
            {
                // This project, then the other projects beside it, in a
                // few formats, exporting into a temporary folder.
                let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let mut others: Vec<PathBuf> = std::fs::read_dir(&folder)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|p| p != &path && p.extension().is_some_and(|e| e == FILE_EXTENSION))
                    .collect();
                others.sort();
                app.batch.open();
                use inkframe_export::ExportFormat;
                let jobs = [(path.clone(), ExportFormat::SpriteSheet), (path, ExportFormat::PngSequence)]
                    .into_iter()
                    .chain(others.into_iter().map(|other| (other, ExportFormat::H264)));
                for (project, format) in jobs {
                    app.batch.add(project);
                    app.batch.set_last_format(format);
                }
                app.batch.set_folder(std::env::temp_dir().join("inkframe-batch-screenshot"));
                app.batch.start(&cc.egui_ctx);
            }
            if let Some(degrees) = AutoScreenshot::view_rotation() {
                app.views.stage().rotate_view(degrees);
            }
            if let Some(section) = AutoScreenshot::brush_panel() {
                app.layout = crate::layout::Layout::workspace(crate::layout::Workspace::Painter);
                app.layout.reveal(Kind::BrushSettings);
                app.brush_panels.show_section(&section);
            }
            if let Some(script) = AutoScreenshot::console() {
                app.layout.replace_kind(Kind::Properties, Kind::ScriptConsole);
                let source = std::fs::read_to_string(script).unwrap_or_default();
                app.scripts.run_in_console(source, &app.editor);
            }
            if let Some(script) = AutoScreenshot::script() {
                app.scripts.run_file(&script, &app.editor);
            }
            if AutoScreenshot::examples() {
                app.examples.open(&cc.egui_ctx);
            }
            if let Some(open) = AutoScreenshot::group() {
                // Every layer but the camera into one symbol, as selecting
                // them all and pressing F8 would.
                let ids: Vec<_> = app.editor.doc.layers.iter().filter(|l| !l.camera).map(|l| l.id).collect();
                app.editor.select_many(&ids);
                match app.editor.convert_selected_to_symbol() {
                    Ok(symbol) => {
                        if let Some(symbol) = app.editor.doc.symbol_mut(symbol) {
                            symbol.duration = 24;
                        }
                        // The scene setup ran before the symbol existed, so
                        // opening it is done here rather than by `_OPEN`.
                        if open {
                            if let Some(instance) = app.editor.selected_layer().map(|l| l.id) {
                                app.editor.enter_instance(instance);
                            }
                        }
                    }
                    Err(problem) => eprintln!("Couldn't group for the screenshot: {problem}"),
                }
            }
            if let Some(query) = AutoScreenshot::finder() {
                app.finder.open_with(&query);
            }
            if AutoScreenshot::bones_panel() {
                app.layout.replace_kind(Kind::Properties, Kind::Bones);
            }
            if AutoScreenshot::presets() {
                app.presets.use_folder(std::env::temp_dir().join("inkframe-screenshot-presets"));
                app.presets.screenshot_setup(&app.editor);
                app.layout.replace_kind(Kind::Properties, Kind::MotionPresets);
            }
            if AutoScreenshot::swap()
                && let Some(layer) = app.editor.selection
                && let Some(symbol) = app.editor.doc.layer(layer).and_then(|l| l.symbol_at(0))
                && let Some(copy) = app.editor.doc.duplicate_symbol(symbol)
            {
                app.editor.doc.swap_symbol(layer, copy, Some(12));
                app.swap.open(&app.editor);
            }
            if AutoScreenshot::library() {
                crate::screenshot::library_demo(&mut app.editor);
                app.layout.replace_kind(Kind::Properties, Kind::Library);
                app.library.selected = app.editor.doc.library_assets().first().copied();
            }
            if AutoScreenshot::multiplane() {
                use inkframe_core::{Color, Element, Point, Shape, Style, Transform};
                let doc = &mut app.editor.doc;
                doc.layers.retain(|l| l.camera);
                doc.background = Color::rgb(150, 200, 235);
                let block = |w: f32, h: f32, color: Color, round: f32| Element {
                    shape: Shape::Rect { width: w, height: h, corner_radius: round },
                    style: Style { fill: Some(color), stroke: None, gradient: None },
                };
                let blob = |r: f32, color: Color| Element {
                    shape: Shape::Ellipse { radius_x: r, radius_y: r * 0.8 },
                    style: Style { fill: Some(color), stroke: None, gradient: None },
                };
                // Far to near, each at its depth.
                let scene: Vec<(&str, Vec<Element>, Point, f32)> = vec![
                    ("Sun", vec![blob(120.0, Color::rgb(255, 230, 150))], Point::new(1300.0, 300.0), 4000.0),
                    (
                        "Mountains",
                        vec![block(1800.0, 500.0, Color::rgb(110, 130, 170), 200.0)],
                        Point::new(960.0, 760.0),
                        1500.0,
                    ),
                    (
                        "Hills",
                        vec![block(2200.0, 360.0, Color::rgb(90, 160, 110), 160.0)],
                        Point::new(960.0, 900.0),
                        600.0,
                    ),
                    (
                        "Tree",
                        vec![block(140.0, 360.0, Color::rgb(40, 110, 60), 70.0)],
                        Point::new(620.0, 800.0),
                        150.0,
                    ),
                    ("Bush", vec![blob(260.0, Color::rgb(30, 80, 40))], Point::new(250.0, 1020.0), -350.0),
                ];
                for (name, elements, at, depth) in scene {
                    let id = doc.add_layer(name, elements, 0, Transform::at(at));
                    if let Some(layer) = doc.layer_mut(id) {
                        layer.track.set_depth(0, depth);
                    }
                }
                let camera = doc.add_camera();
                if let Some(c) = doc.layer_mut(camera) {
                    c.track.set_depth(47, 700.0);
                }
                // `=stack`: the tree drifts back past the hills, and
                // stacking by depth puts it behind them.
                if std::env::var("INKFRAME_SCREENSHOT_MULTIPLANE").is_ok_and(|v| v == "stack") {
                    doc.stack_by_depth = true;
                    let tree = doc.layers.iter().find(|l| l.name == "Tree").map(|l| l.id);
                    if let Some(tree) = tree.and_then(|id| doc.layer_mut(id)) {
                        tree.track.set_depth(47, 1000.0);
                    }
                }
                if std::env::var_os("INKFRAME_SCREENSHOT_FRAME").is_none() {
                    app.editor.frame = 36;
                }
                app.editor.select(Some(camera));
                app.layout.replace_kind(Kind::Properties, Kind::DepthView);
            }
            if AutoScreenshot::particles() {
                app.editor.doc.background = inkframe_core::Color::rgb(24, 28, 44);
                for preset in [Preset::Snow, Preset::Smoke, Preset::Sparks, Preset::Fire] {
                    app.editor.add_particle_layer(preset);
                }
                let stage = app.editor.doc.stage_rect();
                // Spread out, with the fire (selected, last) at the bottom.
                for (name, at) in [
                    ("Smoke", inkframe_core::Point::new(stage.max.x * 0.75, stage.max.y * 0.85)),
                    ("Sparks", inkframe_core::Point::new(stage.max.x * 0.25, stage.max.y * 0.65)),
                    ("Fire", inkframe_core::Point::new(stage.center().x, stage.max.y * 0.88)),
                ] {
                    let id = app.editor.doc.layers.iter().find(|l| l.name == name).map(|l| l.id);
                    if let Some(id) = id {
                        app.editor.set_pose(id, inkframe_core::Transform::at(at));
                    }
                }
                app.editor.frame = 40;
            }
            if let Some(how) = AutoScreenshot::vectorise() {
                if how.contains("colours") {
                    app.vectorise
                        .set_mode(inkframe_core::TraceMode::Colours { colours: 8, skip_white: true });
                }
                app.vectorise.open(&app.editor);
                if how.contains("apply") {
                    app.vectorise.apply(&mut app.editor);
                }
            }
            if AutoScreenshot::update_available() {
                app.updates.found(crate::updates::Release {
                    version: "9.9.0".into(),
                    page: "https://github.com/WhateverTrevor/inkframe/releases".into(),
                    installer: None,
                    notes: "A colour wheel for painting, tools that come back when their key is let go, \
                            sprite sheets imported to edit, and this: Inkframe telling you when there's \
                            a new version."
                        .into(),
                });
            }
            if let Some(sheet) = AutoScreenshot::sprite_sheet()
                && let Err(message) = app.sprite_import.open(&sheet, &app.editor)
            {
                eprintln!("INKFRAME_SCREENSHOT_SPRITES: {message}");
            }
            if AutoScreenshot::lip_sync() {
                crate::screenshot::add_dialogue(&mut app.editor);
                app.lip_sync.open(&app.editor);
            }
            if AutoScreenshot::expand_timeline() {
                for layer in app.editor.doc.all_layers() {
                    for timeline in app.views.timelines.values_mut() {
                        timeline.set_expanded(layer.id, true);
                    }
                }
            }
        }
        app
    }

    // ------------------------------------------------------------ shortcuts

    // ---------------------------------------------------------------- menus

    // ---------------------------------------------------------------- files

    /// What the brush panels asked for: file dialogs, a new tip, a panel.
    fn brush_requests(&mut self) {
        use crate::brush_panel::Asked;
        match self.brush_panels.asked.take().or(self.editor.brush_request.take()) {
            Some(Asked::ShowBrushSettings) => self.layout.reveal(Kind::BrushSettings),
            Some(Asked::ShowBrushes) => self.layout.reveal(Kind::Brushes),
            Some(Asked::DefineBrush) => {
                if let Err(problem) = self.editor.define_brush() {
                    show_error(tr("Couldn't define a brush"), &problem);
                }
            }
            Some(Asked::DefinePattern) => {
                if let Err(problem) = self.editor.define_pattern() {
                    show_error(tr("Couldn't define a pattern"), &problem);
                }
            }
            Some(Asked::ImportTip) => {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr("Import a picture as a brush tip"))
                    .add_filter(tr("Images"), &IMAGE_EXTENSIONS)
                    .pick_file()
                else {
                    return;
                };
                let name = path
                    .file_stem()
                    .map_or_else(|| tr("Custom Tip").to_owned(), |s| s.to_string_lossy().into_owned());
                let made = inkframe_render::read_rgba(&path).and_then(|(width, height, rgba)| {
                    self.editor.brush_from_picture(&name, width, height, &rgba)
                });
                if let Err(problem) = made {
                    show_error(&tr_args("Couldn't import {file}", &[("file", &file_name(&path))]), &problem);
                }
            }
            Some(Asked::ImportPattern) => {
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr("Import a picture as a pattern"))
                    .add_filter(tr("Images"), &IMAGE_EXTENSIONS)
                    .pick_file()
                else {
                    return;
                };
                match inkframe_render::read_rgba(&path) {
                    Ok((width, height, rgba)) => {
                        let name = path
                            .file_stem()
                            .map_or_else(|| tr("Pattern").to_owned(), |s| s.to_string_lossy().into_owned());
                        self.editor.use_pattern(inkframe_core::TipImage::pattern_from_picture(
                            name, width, height, &rgba,
                        ));
                    }
                    Err(problem) => show_error(
                        &tr_args("Couldn't import {file}", &[("file", &file_name(&path))]),
                        &problem,
                    ),
                }
            }
            Some(Asked::ImportBrushes) => {
                let Some(paths) = rfd::FileDialog::new()
                    .set_title(tr("Import brushes"))
                    .add_filter(tr("Brushes"), &[crate::brushes::BRUSH_SET_EXTENSION, "abr"])
                    .pick_files()
                else {
                    return;
                };
                for path in paths {
                    if let Err(problem) = self.editor.brushes.import(&path) {
                        show_error(
                            &tr_args("Couldn't import {file}", &[("file", &file_name(&path))]),
                            &problem,
                        );
                    }
                }
            }
            Some(Asked::ExportBrushes(only)) => {
                let name = only
                    .and_then(|i| self.editor.brushes.saved.get(i))
                    .map_or_else(|| tr("My Brushes").to_owned(), |p| p.name.clone());
                let Some(path) = rfd::FileDialog::new()
                    .set_title(tr("Export brushes"))
                    .add_filter(tr("Brush set"), &[crate::brushes::BRUSH_SET_EXTENSION])
                    .set_file_name(format!("{name}.{}", crate::brushes::BRUSH_SET_EXTENSION))
                    .save_file()
                else {
                    return;
                };
                if let Err(problem) = self.editor.brushes.export(&path, only) {
                    show_error(tr("Couldn't export the brushes"), &problem);
                }
            }
            None => {}
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.editor.has_unsaved_changes() { " •" } else { "" };
        let title = format!("{}{dirty} — Inkframe", self.editor.display_name());
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for InkframeApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let started = std::time::Instant::now();
        self.update_ui(ui, frame);
        self.step_bench(ui.ctx(), started.elapsed());
    }
}

impl InkframeApp {
    /// The screenshot hook's bench: each update moves on a frame, as
    /// scrubbing does, and is timed; once through, the times are printed.
    fn step_bench(&mut self, ctx: &egui::Context, took: std::time::Duration) {
        let Some((left, times)) = &mut self.bench else { return };
        if *left == 0 {
            return;
        }
        times.push(took);
        *left -= 1;
        let last = self.editor.last_frame();
        self.editor.set_frame((self.editor.frame + 1) % (last + 1));
        ctx.request_repaint();
        if *left == 0 {
            // The first update builds caches (thumbnails, text layout), so
            // it's left out of the average.
            let steady = &times[1.min(times.len())..];
            let ms = |d: &std::time::Duration| d.as_secs_f64() * 1000.0;
            let average = steady.iter().map(ms).sum::<f64>() / steady.len().max(1) as f64;
            let worst = steady.iter().map(ms).fold(0.0, f64::max);
            let layers = self.editor.doc.all_layers().count();
            println!(
                "Bench: {layers} layers, {} updates, {average:.1} ms on average, {worst:.1} ms at worst (first {:.1} ms)",
                steady.len(),
                times.first().map_or(0.0, ms)
            );
        }
    }

    fn update_ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.tour_targets = Default::default();
        // The tour waits for the Recover Unsaved Work window, and has the
        // keyboard while it's open.
        let touring = self.tour.is_open() && self.recoveries.is_empty();
        if touring {
            if let Some(ended) = self.tour.keys(&ctx) {
                self.tour_ended(ended);
            }
        } else {
            self.handle_shortcuts(&ctx);
        }
        self.import_dropped_files(&ctx);
        // Decode any image that's new (just imported, or brought back by
        // undo) before the stage draws.
        let project_dir = self.editor.project_dir();
        self.editor.images.sync(&self.editor.doc, project_dir.as_deref());
        self.editor.font_problems =
            inkframe_render::sync_project_fonts(&self.editor.doc, project_dir.as_deref());
        self.editor.remeasure_text();
        crate::tablet::set_feel(self.editor.pressure_feel);

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        if let Some(command) = self.finder.show(&ctx, &self.editor) {
            self.execute(command);
        }
        // The rest of the window is the layout's areas (see `layout.rs`).
        // It's taken out while it draws, since the areas' editors need the
        // rest of the app.
        let mut layout = std::mem::take(&mut self.layout);
        self.views.prepare(&layout);
        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.hovered_area = layout.show(ui, |ui, area, kind| self.show_area(ui, area, kind));
        });
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            drop_hint(&ctx);
        }
        self.layout = layout;
        if let Some((area, kind)) = self.views.retype.take() {
            self.layout.set_kind(area, kind);
        }
        // The last stage pointed at is the one menu commands (zoom, turn)
        // act on.
        if let Some(area) = self.hovered_area
            && self.layout.kind_of(area) == Some(Kind::Stage)
        {
            self.views.active_stage = Some(area);
        }
        if self.layout != self.saved_layout
            && !ctx.input(|i| i.pointer.any_down())
            && self.screenshot.is_none()
        {
            self.layout.save();
            self.saved_layout = self.layout.clone();
        }
        if let Some(autosave) = &mut self.autosave {
            autosave.tick(&self.editor);
        }
        self.recovery_window(&ctx);
        // Screenshot runs stay off the network.
        if self.screenshot.is_none() {
            self.updates.check_if_due(&mut self.update_prefs);
        }
        self.updates.show(&ctx, &mut self.update_prefs);
        let prefs = self.current_prefs();
        if prefs != self.saved_prefs && !ctx.input(|i| i.pointer.any_down()) && self.screenshot.is_none() {
            prefs.save();
            self.saved_prefs = prefs;
        }

        let name = self.editor.display_name();
        self.export.show(&ctx, &self.editor.settled_doc(), &name, self.editor.project_dir());
        let unsaved = self.editor.has_unsaved_changes();
        self.batch.show(&ctx, self.editor.path.as_deref().map(|path| (path, unsaved)));
        self.lip_sync.show(&ctx, &mut self.editor, &mut self.audio);
        self.sprite_import.show(&ctx, &mut self.editor);
        self.xsheet.show(&ctx, &mut self.editor, &mut self.audio);
        self.adjust.show(&ctx, &mut self.editor);
        self.vectorise.show(&ctx, &mut self.editor);
        self.swap.show(&ctx, &mut self.editor);
        crate::brush_panel::quick_menu(&ctx, &mut self.editor, &mut self.brush_panels);
        self.brush_requests();
        self.scripts.show(&ctx, &mut self.editor);
        if let Some(path) = self.examples.show(&ctx) {
            self.open_example(&path);
        }
        // After the recovery window and the tour, so it's not in their way.
        if self.recoveries.is_empty() && !self.tour.is_open() {
            self.whats_new.show(&ctx);
        }
        if self.recoveries.is_empty()
            && let Some(ended) = self.tour.show(&ctx, &self.tour_targets)
        {
            self.tour_ended(ended);
        }

        let dt = ctx.input(|i| i.stable_dt) as f64;
        let project_dir = self.editor.project_dir();
        self.audio.sync(&ctx, &mut self.editor, project_dir.as_deref(), dt);
        if self.editor.playback.playing {
            ctx.request_repaint();
        }

        // Record an undo step once the user has let go of the mouse and
        // finished typing, so each gesture or edit is a single step.
        let interacting =
            self.views.is_busy() || ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input();
        if !interacting {
            self.editor.commit_history();
        }

        self.update_title(&ctx);
        self.handle_close_request(&ctx);
        if let Some(screenshot) = &mut self.screenshot {
            // Once the sound is ready, so loudness can be measured.
            if AutoScreenshot::lip_sync_apply() && !self.audio.is_busy() {
                self.lip_sync.apply_once(&mut self.editor, &mut self.audio);
            }
            let benching = self.bench.as_ref().is_some_and(|(left, _)| *left > 0);
            screenshot.update(&ctx, self.audio.is_busy() || self.lip_sync.is_busy() || benching);
        }
    }
}

pub(crate) fn show_error(title: &str, detail: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(detail)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}
