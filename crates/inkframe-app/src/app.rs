//! The top-level application: window layout, menus, shortcuts and files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use inkframe_core::{Clipboard, Document, SymbolId, TimelineId};
use inkframe_render::IMAGE_EXTENSIONS;

use crate::adjust_dialog::{AdjustDialog, AdjustmentKind};
use crate::audio_engine::AudioEngine;
use crate::autosave::{Autosave, Recovery};
use crate::batch_dialog::BatchDialog;
use crate::editor::{Editor, Tool};
use crate::examples::ExamplesWindow;
use crate::export_dialog::ExportDialog;
use crate::guides::SafeAreas;
use crate::layout::{AreaId, Kind, Layout, Workspace};
use crate::library::LibraryView;
use crate::lip_sync_dialog::LipSyncDialog;
use crate::panels;
use crate::prefs::Prefs;
use crate::screenshot::AutoScreenshot;
use crate::stage::StageView;
use crate::swap_dialog::SwapDialog;
use crate::timeline::TimelineView;
use crate::vectorise_dialog::VectoriseDialog;
use crate::xsheet::ExposureSheet;
use crate::{inspector, shortcuts, toolbar};
use inkframe_core::Preset;

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
    CopyFrames,
    /// Pastes a copied pose onto the selected layer at the current frame.
    PasteFrames,
    DuplicateLayer,
    DeleteLayer,
    ConvertToSymbol,
    /// Makes the selected layer a mask over the layer below it.
    MaskLayerBelow,
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
/// The guide that walks through a first animation, and where problems are
/// reported (Help menu).
/// The guidebook PDF is attached to each release.
const GUIDEBOOK_URL: &str = "https://github.com/WhateverTrevor/inkframe/releases/latest";
const GUIDE_URL: &str = "https://github.com/WhateverTrevor/inkframe/blob/main/docs/GETTING_STARTED.md";
const ISSUES_URL: &str = "https://github.com/WhateverTrevor/inkframe/issues";
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
/// How much `[` and `]` shrink or grow the brush.
const BRUSH_STEP: f32 = 1.2;
/// Font files Inkframe can import.
const FONT_EXTENSIONS: [&str; 3] = ["ttf", "otf", "ttc"];
/// Sound files Inkframe can import (see `inkframe-audio`).
const AUDIO_EXTENSIONS: [&str; 9] = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "aif", "aiff", "caf"];

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
    xsheet: ExposureSheet,
    adjust: AdjustDialog,
    library: LibraryView,
    panels: panels::PanelState,
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
    /// The first-run tour (see `tour.rs`), where the parts of the window
    /// it points at were drawn this frame, and whether it's been seen.
    tour: crate::tour::Tour,
    tour_targets: crate::tour::Targets,
    toured: bool,
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
                show_error(&format!("Couldn't open {}", path.display()), &err.to_string());
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
            xsheet: ExposureSheet::default(),
            adjust: AdjustDialog::default(),
            library: LibraryView::default(),
            panels: panels::PanelState::default(),
            templates: crate::templates::TemplateLibrary::default(),
            presets: crate::motion_presets::PresetLibrary::default(),
            audio: AudioEngine::default(),
            title: String::new(),
            allow_close: false,
            screenshot: AutoScreenshot::from_env(),
            clipboard: None,
            ctx: cc.egui_ctx.clone(),
            finder: Default::default(),
            tour: Default::default(),
            tour_targets: Default::default(),
            toured: false,
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
            app.editor.guides = prefs.guides();
            app.editor.draw_into_layer = !prefs.shape_per_layer;
            app.editor.show_rulers = prefs.show_rulers;
            app.editor.show_guides = !prefs.hide_guides;
            app.editor.lock_guides = prefs.lock_guides;
            app.editor.snap_to_guides = !prefs.no_guide_snapping;
            app.batch.restore(prefs.batch_jobs(), prefs.batch_folder.clone());
            app.toured = prefs.toured;
        }
        // The tour shows the first time Inkframe opens, and never in a
        // screenshot unless asked for.
        if app.screenshot.is_none() && !app.toured {
            app.tour.start();
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

    fn execute(&mut self, command: Command) {
        let editor = &mut self.editor;
        match command {
            Command::OpenFinder => self.finder.open(),
            Command::UseTool(tool) => editor.use_tool(tool),
            Command::New => {
                if self.confirm_discard_changes() {
                    self.replace_editor(Editor::new(Document::default()));
                }
            }
            Command::Open => self.open(),
            Command::ImportImage => self.import_image(),
            Command::ImportAudio => self.import_audio(),
            Command::ImportFont => self.import_fonts(),
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
            Command::Export => self.export.open(&self.editor.doc),
            Command::BatchExport => self.batch.open(),
            Command::OpenExample => self.examples.open(&self.ctx),
            Command::StartTour => self.tour.start(),
            Command::Vectorise => self.vectorise.open(&self.editor),
            Command::ExportPsd => self.export_psd(),
            Command::ToggleMaximize => {
                if let Some(area) = self.hovered_area {
                    self.layout.toggle_maximized(area);
                }
            }
            Command::ResetLayout => self.layout = Layout::workspace(self.layout.workspace_name()),
            Command::Workspace(workspace) => self.layout = Layout::workspace(workspace),
            // With a painting or selection tool, Select All and Deselect
            // work on pixels, as in Photoshop; otherwise on layers.
            Command::SelectAll if editor.tool.on_pixels() && editor.paint_target().is_some() => {
                editor.select_all_pixels()
            }
            Command::Deselect if editor.tool.on_pixels() => editor.deselect_pixels(),
            // Inside a layer, Select All picks every shape in it.
            Command::SelectAll if editor.inside_layer().is_some() => editor.pick_all_shapes(),
            Command::SelectAll => editor.select_all(),
            Command::Deselect => editor.select(None),
            Command::InvertSelection => editor.invert_pixel_selection(),
            Command::LayerViaCopy => editor.layer_via_copy(false),
            Command::LayerViaCut => editor.layer_via_copy(true),
            Command::MergeDown => {
                editor.merge_down();
            }
            Command::Adjust(adjustment) => self.adjust.open(editor, adjustment),
            Command::Cut => {
                self.copy(false);
                // Inside a layer with shapes picked, they're what was cut.
                if !self.editor.delete_shapes() {
                    self.editor.delete_selected_layer();
                }
            }
            Command::Copy => self.copy(false),
            Command::CopyFrames => self.copy(true),
            Command::Paste => {
                let center = self.views.stage().visible_center(&self.editor);
                if let Some(clipboard) = &self.clipboard {
                    self.editor.paste(clipboard, Some(center));
                }
            }
            Command::PasteInPlace | Command::PasteFrames => {
                if let Some(clipboard) = &self.clipboard {
                    self.editor.paste(clipboard, None);
                }
            }
            Command::Undo => editor.undo(),
            Command::Redo => editor.redo(),
            Command::DuplicateLayer => editor.duplicate_selected_layer(),
            // Delete with selected pixels clears them, not the whole layer.
            Command::DeleteLayer if editor.active_selection().is_some() => {
                editor.clear_selected_pixels();
            }
            Command::DeleteLayer => editor.delete_selected_layer(),
            // Inside a layer with shapes picked, F8 makes a symbol of just them.
            Command::ConvertToSymbol if !editor.picked_shapes().is_empty() => {
                if let Err(problem) = editor.shapes_to_symbol() {
                    show_error("Couldn't make a symbol", &problem);
                }
            }
            Command::ConvertToSymbol => {
                if let Err(problem) = editor.convert_selected_to_symbol() {
                    show_error("Couldn't make a symbol", &problem);
                }
            }
            Command::MaskLayerBelow => {
                if let Some(id) = editor.selection {
                    editor.doc.mask_layer_below(id);
                }
            }
            Command::EditSymbol => {
                if let Some(id) = editor.selection {
                    editor.enter_instance(id);
                }
            }
            Command::ExitSymbol => editor.exit_symbol(),
            Command::InsertKeyframe => editor.insert_keyframe(),
            Command::RemoveKeyframe => editor.remove_keyframe(),
            Command::LayerStartHere => editor.set_layer_start_here(),
            Command::LayerEndHere => editor.set_layer_end_here(),
            Command::NewDrawing => editor.new_drawing(false),
            Command::DuplicateDrawing => editor.new_drawing(true),
            Command::DeleteDrawing => editor.delete_shown_drawing(),
            // While painting, [ and ] change the brush size instead, as in
            // Photoshop.
            Command::PreviousDrawing if editor.tool.uses_brush() => editor.resize_brush(1.0 / BRUSH_STEP),
            Command::NextDrawing if editor.tool.uses_brush() => editor.resize_brush(BRUSH_STEP),
            Command::PreviousDrawing => editor.cycle_drawing(-1),
            Command::NextDrawing => editor.cycle_drawing(1),
            Command::CombineShapes(how) => {
                if let Err(problem) = editor.combine_shapes(how) {
                    show_error("Couldn't combine the shapes", &problem);
                }
            }
            Command::SwapSymbol => self.swap.open(editor),
            Command::ToggleRulers => editor.show_rulers = !editor.show_rulers,
            Command::ToggleRigSetup => editor.rig_setup = !editor.rig_setup,
            Command::SetPerspective(kind) => {
                let (w, h) = (editor.doc.width as f32, editor.doc.height as f32);
                editor.doc.perspective = match (kind, editor.doc.perspective.take()) {
                    (None, _) => None,
                    (Some(kind), Some(mut guide)) => {
                        guide.set_kind(kind, w, h);
                        Some(guide)
                    }
                    (Some(kind), None) => Some(inkframe_core::Perspective::new(kind, w, h)),
                };
            }
            Command::ToggleGuides => editor.show_guides = !editor.show_guides,
            Command::ToggleLockGuides => editor.lock_guides = !editor.lock_guides,
            Command::ToggleSnapToGuides => editor.snap_to_guides = !editor.snap_to_guides,
            Command::ClearGuides => editor.doc.ruler_guides.clear(),
            Command::StartLattice(kind) => {
                if let Err(problem) = editor.start_lattice(kind) {
                    show_error(&format!("Couldn't start {}", kind.name()), &problem);
                }
            }
            Command::FinishLattice => editor.finish_lattice(),
            Command::NewSkeleton => editor.new_skeleton(),
            Command::FinishCorrection => editor.finish_correction(),
            Command::CancelCorrection => editor.cancel_correction(),
            Command::EndPreview => editor.end_preview(),
            Command::NewSwitch => editor.new_switch(),
            Command::DeleteBone => {
                editor.delete_bone();
            }
            Command::BreakApart => {
                if editor.break_apart() == 0 {
                    show_error(
                        "Nothing to break apart",
                        "Break Apart turns text into a shape for each letter, and pencil lines into filled shapes. \
                         Select a text layer, or double-click a layer and pick its text or lines.",
                    );
                }
            }
            Command::InsertFrames => editor.insert_frames(),
            Command::RemoveFrames => editor.remove_frames(),
            Command::InsertBlankKeyframe => editor.insert_blank_keyframe(),
            Command::ConvertToKeyframes => editor.convert_to_keyframes(false),
            Command::ConvertToBlankKeyframes => editor.convert_to_keyframes(true),
            Command::ReverseFrames => {
                if !editor.reverse_frames() {
                    show_error(
                        "Pick some frames first",
                        "Reverse Frames plays a stretch backwards. Shift-click or Shift-drag along a layer's row in \
                         the timeline to pick the frames.",
                    );
                }
            }
            Command::NewLayer => {
                editor.add_empty_layer();
            }
            Command::NewPaintLayer => {
                editor.add_paint_layer();
            }
            Command::NewParticleLayer(preset) => {
                editor.add_particle_layer(preset);
            }
            Command::NewFolder => editor.new_folder(),
            Command::CreateShapeTween => editor.set_shape_tween(true),
            Command::MakeInbetweens => {
                if editor.make_inbetweens() == 0 {
                    show_error(
                        "No inbetweens to draw",
                        "Inbetweens are drawn between two key drawings: put the playhead between a drawing and the next one it swaps to, with frames to fill between them.",
                    );
                }
            }
            Command::AddCamera => editor.add_camera(),
            Command::RemoveShapeTween => editor.set_shape_tween(false),
            Command::AddShapeHint => editor.add_shape_hint(),
            Command::RemoveShapeHints => editor.clear_shape_hints(),
            Command::LipSync => self.lip_sync.open(editor),
            Command::PlayPause => editor.toggle_playback(),
            Command::StepBackward => editor.step(-1),
            Command::StepForward => editor.step(1),
            Command::FirstFrame => editor.set_frame(0),
            Command::LastFrame => editor.set_frame(editor.last_frame()),
            Command::ZoomIn => self.views.stage().zoom_by(&editor.doc, 1.25),
            Command::ZoomOut => self.views.stage().zoom_by(&editor.doc, 0.8),
            Command::ZoomFit => self.views.stage().zoom_to_fit(),
            Command::ZoomActualSize => self.views.stage().zoom_to_actual_size(&editor.doc),
            Command::RotateViewLeft => self.views.stage().rotate_view(-15.0),
            Command::RotateViewRight => self.views.stage().rotate_view(15.0),
            Command::ResetRotation => self.views.stage().reset_rotation(),
            Command::UseSlot { group, cycle } => editor.use_slot(group, cycle),
            Command::FinishPath => self.views.stage().finish_path(editor, false),
            Command::RemovePathPoint => self.views.stage().remove_last_path_point(),
            Command::DeleteAnchor => self.views.stage().delete_selected_anchor(editor),
            Command::ExitLayer => editor.exit_layer(),
            Command::DeleteShapes => {
                editor.delete_shapes();
            }
        }
    }

    // ------------------------------------------------------------ shortcuts

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use shortcuts::*;
        // Find a Command has the keyboard while it's open.
        if self.finder.is_open() {
            return;
        }

        // `consume_shortcut` ignores extra Shift, so ⌘⇧S would also match ⌘S.
        // Check the shifted variants first so they win.
        let file_shortcuts = [
            (SAVE_AS, Command::SaveAs),
            (SAVE, Command::Save),
            (OPEN, Command::Open),
            // Before ⌘N, which would otherwise match it too.
            (NEW_LAYER, Command::NewLayer),
            (NEW, Command::New),
            (EXPORT, Command::Export),
            (IMPORT, Command::ImportImage),
            (FIND_COMMAND, Command::OpenFinder),
        ];
        let editing_shortcuts = [
            (REDO, Command::Redo),
            (UNDO, Command::Undo),
            (REDO_ALT, Command::Redo),
            (DUPLICATE, Command::DuplicateLayer),
            // Deselect (⌘⇧A) before Select All (⌘A), since shortcuts
            // ignore extra Shift.
            (DESELECT_ALL, Command::Deselect),
            (SELECT_ALL, Command::SelectAll),
            (INVERT_SELECTION, Command::InvertSelection),
            (LAYER_VIA_CUT, Command::LayerViaCut),
            (LAYER_VIA_COPY, Command::LayerViaCopy),
            (CONVERT_TO_SYMBOL, Command::ConvertToSymbol),
            (BREAK_APART, Command::BreakApart),
            // The modified ones first: shortcuts ignore extra Shift and Alt.
            (RULERS, Command::ToggleRulers),
            (RIG_SETUP, Command::ToggleRigSetup),
            (SNAP_TO_GUIDES, Command::ToggleSnapToGuides),
            (LOCK_GUIDES, Command::ToggleLockGuides),
            (SHOW_GUIDES, Command::ToggleGuides),
            (ADD_SHAPE_HINT, Command::AddShapeHint),
            (ZOOM_FIT, Command::ZoomFit),
            (ZOOM_100, Command::ZoomActualSize),
            (ZOOM_IN, Command::ZoomIn),
            (ZOOM_OUT, Command::ZoomOut),
            (REMOVE_KEYFRAME, Command::RemoveKeyframe),
            // Shift+F5 before F5, since shortcuts ignore extra Shift.
            (REMOVE_FRAMES, Command::RemoveFrames),
            (INSERT_FRAMES, Command::InsertFrames),
            (INSERT_BLANK_KEYFRAME, Command::InsertBlankKeyframe),
            (LAYER_START_HERE, Command::LayerStartHere),
            (LAYER_END_HERE, Command::LayerEndHere),
            // After the ⌥ versions above, so those don't also swap.
            (PREVIOUS_DRAWING, Command::PreviousDrawing),
            (NEXT_DRAWING, Command::NextDrawing),
            (INSERT_KEYFRAME, Command::InsertKeyframe),
            (PLAY, Command::PlayPause),
        ];

        let mut commands = Vec::new();
        let mut pasted_text = None;
        // While a text field has focus, keys like ⌘Z and Delete belong to
        // the text field. File shortcuts still work everywhere.
        let typing = ctx.egui_wants_keyboard_input();
        let drawing_path = self.views.stage().is_drawing_path();
        let anchor_selected = self.views.stage().has_anchor_selection();
        let in_symbol = self.editor.timeline() != TimelineId::Main;
        let inside_layer = self.editor.inside_layer().is_some();
        let shapes_picked = !self.editor.picked_shapes().is_empty();
        let bending = self.editor.lattice.is_some();
        let correcting = self.editor.correcting.is_some();
        let bone_picked = self.editor.tool == Tool::AddBone && self.editor.selected_bone.is_some();
        ctx.input_mut(|input| {
            for (shortcut, command) in file_shortcuts {
                if input.consume_shortcut(&shortcut) {
                    commands.push(command);
                }
            }
            if typing {
                return;
            }
            // Copy, cut and paste come from the system as clipboard events,
            // not keys. ⌥ makes them act on keyframes, and ⇧ pastes in place.
            for event in &input.events {
                let (alt, shift) = (input.modifiers.alt, input.modifiers.shift);
                match event {
                    egui::Event::Copy => commands.push(if alt { Command::CopyFrames } else { Command::Copy }),
                    egui::Event::Cut => commands.push(Command::Cut),
                    egui::Event::Paste(text) => match Clipboard::from_text(text) {
                        // Inkframe data, perhaps from another window: paste
                        // that, and remember it for the Edit menu.
                        Some(clipboard) => {
                            let command = match (&clipboard, shift) {
                                (Clipboard::Keyframe(_), _) => Command::PasteFrames,
                                (Clipboard::Layers(_) | Clipboard::Shapes(_), true) => Command::PasteInPlace,
                                (Clipboard::Layers(_) | Clipboard::Shapes(_), false) => Command::Paste,
                            };
                            self.clipboard = Some(clipboard);
                            commands.push(command);
                        }
                        // Text from another app becomes a text layer.
                        None => pasted_text = Some(text.clone()),
                    },
                    _ => {}
                }
            }
            // Some keys mean something different while the stage is busy:
            // with a pen path in progress, Enter and Escape finish it and
            // Backspace takes back a point; with an anchor picked, Backspace
            // deletes that point rather than the whole layer. Checked first
            // so they win over the usual meanings (play, delete layer).
            let stage_keys: &[(Key, Command)] = if drawing_path {
                &[
                    (Key::Enter, Command::FinishPath),
                    (Key::Escape, Command::FinishPath),
                    (Key::Backspace, Command::RemovePathPoint),
                    (Key::Delete, Command::RemovePathPoint),
                ]
            } else if correcting && !shapes_picked && !anchor_selected {
                // Enter keys the correction being drawn; Escape drops it.
                &[(Key::Enter, Command::FinishCorrection), (Key::Escape, Command::CancelCorrection)]
            } else if bending {
                // Enter or Escape leaves a Distort or Envelope's shapes bent.
                &[(Key::Enter, Command::FinishLattice), (Key::Escape, Command::FinishLattice)]
            } else if bone_picked && self.editor.previewing.is_some() {
                // Escape puts a bone previewed at another turn back.
                &[
                    (Key::Backspace, Command::DeleteBone),
                    (Key::Delete, Command::DeleteBone),
                    (Key::Escape, Command::EndPreview),
                ]
            } else if bone_picked {
                &[(Key::Backspace, Command::DeleteBone), (Key::Delete, Command::DeleteBone)]
            } else if self.editor.previewing.is_some() {
                &[(Key::Escape, Command::EndPreview)]
            } else if anchor_selected {
                &[(Key::Backspace, Command::DeleteAnchor), (Key::Delete, Command::DeleteAnchor)]
            } else if shapes_picked {
                // Inside a layer, Delete takes out the picked shapes, not
                // the layer, and Escape steps back out.
                &[
                    (Key::Backspace, Command::DeleteShapes),
                    (Key::Delete, Command::DeleteShapes),
                    (Key::Escape, Command::ExitLayer),
                ]
            } else if inside_layer {
                &[(Key::Escape, Command::ExitLayer)]
            } else if in_symbol {
                // Escape steps back out of a symbol, as in Animate.
                &[(Key::Escape, Command::ExitSymbol)]
            } else {
                &[]
            };
            for &(key, command) in stage_keys {
                if input.consume_key(Modifiers::NONE, key) {
                    commands.push(command);
                }
            }
            for (shortcut, command) in editing_shortcuts {
                if input.consume_shortcut(&shortcut) {
                    commands.push(command);
                }
            }

            let plain_keys = [
                (Key::Comma, Command::StepBackward),
                (Key::ArrowLeft, Command::StepBackward),
                (Key::Period, Command::StepForward),
                (Key::ArrowRight, Command::StepForward),
                (Key::Home, Command::FirstFrame),
                (Key::End, Command::LastFrame),
                (Key::Delete, Command::DeleteLayer),
                // Krita's keys for turning the view: 4 and 6 either way, 5
                // back upright.
                (Key::Num4, Command::RotateViewLeft),
                (Key::Num6, Command::RotateViewRight),
                (Key::Num5, Command::ResetRotation),
                (Key::Backspace, Command::DeleteLayer),
            ];
            // Ctrl+Space: the area under the pointer fills the window, or
            // goes back, as in Blender.
            if input.consume_key(Modifiers::CTRL, Key::Space) {
                commands.push(Command::ToggleMaximize);
            }
            for (key, command) in plain_keys {
                if input.consume_key(Modifiers::NONE, key) {
                    commands.push(command);
                }
            }
            for group in Tool::GROUPS {
                let key = tool_key(group[0]);
                if input.consume_key(Modifiers::SHIFT, key) {
                    commands.push(Command::UseSlot { group, cycle: true });
                } else if input.consume_key(Modifiers::NONE, key) {
                    commands.push(Command::UseSlot { group, cycle: false });
                }
            }
        });

        for command in commands {
            self.execute(command);
        }
        if let Some(text) = pasted_text {
            let center = self.views.stage().visible_center(&self.editor);
            self.editor.paste_text(&text, center);
        }
    }

    /// Copies the selected layer, or with `keyframe` its pose on this frame,
    /// to the system clipboard (so it can be pasted into another window)
    /// and to our own.
    fn copy(&mut self, keyframe: bool) {
        let copied = if keyframe { self.editor.copy_keyframe() } else { self.editor.copy_selection() };
        if let Some(clipboard) = copied {
            self.ctx.copy_text(clipboard.to_text());
            self.clipboard = Some(clipboard);
        }
    }

    // ---------------------------------------------------------------- menus

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        // The View menu's toggles, kept apart from `chosen`, which `item`
        // holds onto for the whole menu bar.
        let mut toggled = None;
        egui::MenuBar::new().ui(ui, |ui| {
            let ctx = ui.ctx().clone();
            let mut item = |ui: &mut egui::Ui,
                            label: &str,
                            shortcut: Option<egui::KeyboardShortcut>,
                            command,
                            enabled: bool| {
                let mut button = egui::Button::new(label);
                if let Some(shortcut) = shortcut {
                    button = button.shortcut_text(ctx.format_shortcut(&shortcut));
                }
                if ui.add_enabled(enabled, button).clicked() {
                    chosen = Some(command);
                    ui.close();
                }
            };
            let has_selection = self.editor.selection.is_some();

            ui.menu_button("File", |ui| {
                item(ui, "New", Some(shortcuts::NEW), Command::New, true);
                item(ui, "Open…", Some(shortcuts::OPEN), Command::Open, true);
                item(ui, "Open Example…", None, Command::OpenExample, true);
                item(ui, "Import to Stage…", Some(shortcuts::IMPORT), Command::ImportImage, true);
                item(ui, "Import Audio…", None, Command::ImportAudio, true);
                item(ui, "Import Font…", None, Command::ImportFont, true);
                ui.separator();
                item(ui, "Save", Some(shortcuts::SAVE), Command::Save, true);
                item(ui, "Save As…", Some(shortcuts::SAVE_AS), Command::SaveAs, true);
                ui.separator();
                item(ui, "Export…", Some(shortcuts::EXPORT), Command::Export, true);
                item(ui, "Export PSD…", None, Command::ExportPsd, true);
                item(ui, "Batch Export…", None, Command::BatchExport, true);
            });
            ui.menu_button("Edit", |ui| {
                item(ui, "Undo", Some(shortcuts::UNDO), Command::Undo, self.editor.history.can_undo());
                item(ui, "Redo", Some(shortcuts::REDO), Command::Redo, self.editor.history.can_redo());
                ui.separator();
                item(
                    ui,
                    "Duplicate Layer",
                    Some(shortcuts::DUPLICATE),
                    Command::DuplicateLayer,
                    has_selection,
                );
                item(ui, "Delete Layer", None, Command::DeleteLayer, has_selection);
                ui.separator();
                let layers = matches!(self.clipboard, Some(Clipboard::Layers(_) | Clipboard::Shapes(_)));
                let keyframe = matches!(self.clipboard, Some(Clipboard::Keyframe(_)));
                item(ui, "Select All", Some(shortcuts::SELECT_ALL), Command::SelectAll, true);
                item(ui, "Deselect All", Some(shortcuts::DESELECT_ALL), Command::Deselect, has_selection);
                ui.separator();
                item(ui, "Cut", Some(shortcuts::CUT), Command::Cut, has_selection);
                item(ui, "Copy", Some(shortcuts::COPY), Command::Copy, has_selection);
                item(ui, "Paste in Center", Some(shortcuts::PASTE), Command::Paste, layers);
                item(ui, "Paste in Place", Some(shortcuts::PASTE_IN_PLACE), Command::PasteInPlace, layers);
                ui.separator();
                item(ui, "Copy Frames", Some(shortcuts::COPY_FRAMES), Command::CopyFrames, has_selection);
                item(
                    ui,
                    "Paste Frames",
                    Some(shortcuts::PASTE_FRAMES),
                    Command::PasteFrames,
                    keyframe && has_selection,
                );
            });
            ui.menu_button("Modify", |ui| {
                item(
                    ui,
                    "Convert to Symbol",
                    Some(shortcuts::CONVERT_TO_SYMBOL),
                    Command::ConvertToSymbol,
                    has_selection,
                );
                // On shapes picked inside a layer (see `inside.rs`).
                let picked = self.editor.picked_shapes().len();
                ui.menu_button("Combine Shapes", |ui| {
                    use inkframe_core::ShapeOp;
                    for (label, how) in [
                        ("Union", ShapeOp::Union),
                        ("Intersect", ShapeOp::Intersect),
                        ("Punch", ShapeOp::Punch),
                        ("Crop", ShapeOp::Crop),
                    ] {
                        item(ui, label, None, Command::CombineShapes(how), picked >= 2);
                    }
                    if picked < 2 {
                        ui.weak("Double-click a layer, then pick two or more of its shapes.");
                    }
                });
                item(ui, "Break Apart", Some(shortcuts::BREAK_APART), Command::BreakApart, has_selection);
                ui.menu_button("Transform", |ui| {
                    use crate::envelope::LatticeKind;
                    let bendable = has_selection && self.editor.lattice.is_none();
                    for kind in [LatticeKind::Distort, LatticeKind::Envelope] {
                        item(ui, kind.name(), None, Command::StartLattice(kind), bendable);
                    }
                    ui.weak("On the picked shapes, or everything in the selected layer.");
                });
                ui.separator();
                let instance_selected = self.editor.selected_layer().is_some_and(|l| l.instance.is_some());
                item(ui, "Edit Symbol", None, Command::EditSymbol, instance_selected);
                item(ui, "Swap Symbol…", None, Command::SwapSymbol, SwapDialog::can_open(&self.editor));
                let in_symbol = self.editor.timeline() != TimelineId::Main;
                item(ui, "Exit Symbol", None, Command::ExitSymbol, in_symbol);
                ui.separator();
                // Only offered when there's a layer below to mask.
                let below = self
                    .editor
                    .selection
                    .is_some_and(|id| self.editor.layers().first().is_some_and(|bottom| bottom.id != id));
                item(ui, "Mask", None, Command::MaskLayerBelow, below);
                ui.separator();
                let traceable = VectoriseDialog::can_vectorise(&self.editor);
                item(ui, "Vectorise…", None, Command::Vectorise, traceable);
                item(ui, "New Layer", Some(shortcuts::NEW_LAYER), Command::NewLayer, true);
                item(ui, "New Paint Layer", None, Command::NewPaintLayer, true);
                ui.menu_button("New Particle Layer", |ui| {
                    for preset in inkframe_core::Preset::ALL {
                        item(ui, preset.name(), None, Command::NewParticleLayer(preset), true);
                    }
                });
                item(ui, "New Folder", None, Command::NewFolder, true);
                item(ui, "New Skeleton", None, Command::NewSkeleton, true);
                item(ui, "New Switch", None, Command::NewSwitch, true);
                ui.separator();
                let hints = self.editor.shape_tween_drawings().is_some();
                item(ui, "Add Shape Hint", Some(shortcuts::ADD_SHAPE_HINT), Command::AddShapeHint, hints);
                item(ui, "Remove All Hints", None, Command::RemoveShapeHints, hints);
                let below_is_paint = self.editor.selection.is_some_and(|id| {
                    let Some(timeline) = self.editor.doc.timeline_of(id) else { return false };
                    let layers = self.editor.doc.layers_of(timeline);
                    let index = layers.iter().position(|l| l.id == id).unwrap_or(0);
                    index > 0
                        && self.editor.doc.paint_canvas_at(layers[index - 1].id, self.editor.frame).is_some()
                });
                item(ui, "Merge Down", None, Command::MergeDown, below_is_paint);
                let paint = self.editor.paint_target().is_some();
                ui.menu_button("Adjustments", |ui| {
                    for kind in AdjustmentKind::ALL {
                        let label = format!("{}…", kind.neutral().name());
                        item(ui, &label, None, Command::Adjust(kind), paint);
                    }
                });
                // Drawing substitution: only for layers with their own
                // artwork, not symbol instances.
                let drawn = self.editor.selected_layer().filter(|l| l.instance.is_none());
                let swaps = drawn.is_some_and(|l| l.drawings.len() > 1);
                item(ui, "New Drawing", None, Command::NewDrawing, drawn.is_some());
                item(ui, "Duplicate Drawing", None, Command::DuplicateDrawing, drawn.is_some());
                item(ui, "Delete Drawing", None, Command::DeleteDrawing, swaps);
                item(
                    ui,
                    "Previous Drawing",
                    Some(shortcuts::PREVIOUS_DRAWING),
                    Command::PreviousDrawing,
                    swaps,
                );
                item(ui, "Next Drawing", Some(shortcuts::NEXT_DRAWING), Command::NextDrawing, swaps);
                item(ui, "Lip Sync…", None, Command::LipSync, swaps);
            });
            ui.menu_button("Select", |ui| {
                let paint = self.editor.paint_target().is_some();
                let selected = self.editor.active_selection().is_some();
                item(ui, "All Pixels", Some(shortcuts::SELECT_ALL), Command::SelectAll, paint);
                item(ui, "Deselect", Some(shortcuts::DESELECT_ALL), Command::Deselect, selected);
                item(ui, "Inverse", Some(shortcuts::INVERT_SELECTION), Command::InvertSelection, paint);
                ui.separator();
                item(ui, "Layer via Copy", Some(shortcuts::LAYER_VIA_COPY), Command::LayerViaCopy, selected);
                item(ui, "Layer via Cut", Some(shortcuts::LAYER_VIA_CUT), Command::LayerViaCut, selected);
                ui.separator();
                ui.weak(
                    "All Pixels and Deselect use ⌘A and ⌘⇧A while a\npainting or selection tool is in use.",
                );
            });
            ui.menu_button("Timeline", |ui| {
                let play = if self.editor.playback.playing { "Pause" } else { "Play" };
                item(ui, play, Some(shortcuts::PLAY), Command::PlayPause, true);
                ui.separator();
                item(
                    ui,
                    "Insert Keyframe",
                    Some(shortcuts::INSERT_KEYFRAME),
                    Command::InsertKeyframe,
                    has_selection,
                );
                item(
                    ui,
                    "Remove Keyframe",
                    Some(shortcuts::REMOVE_KEYFRAME),
                    Command::RemoveKeyframe,
                    has_selection,
                );
                ui.separator();
                // Animate's frame commands, on the picked frames (or the
                // current one); see `frame_commands.rs`.
                item(
                    ui,
                    "Insert Frames",
                    Some(shortcuts::INSERT_FRAMES),
                    Command::InsertFrames,
                    has_selection,
                );
                item(
                    ui,
                    "Remove Frames",
                    Some(shortcuts::REMOVE_FRAMES),
                    Command::RemoveFrames,
                    has_selection,
                );
                item(
                    ui,
                    "Insert Blank Keyframe",
                    Some(shortcuts::INSERT_BLANK_KEYFRAME),
                    Command::InsertBlankKeyframe,
                    has_selection,
                );
                let picked = self.editor.frame_range.is_some();
                item(ui, "Convert to Keyframes", None, Command::ConvertToKeyframes, has_selection && picked);
                item(
                    ui,
                    "Convert to Blank Keyframes",
                    None,
                    Command::ConvertToBlankKeyframes,
                    has_selection && picked,
                );
                item(ui, "Reverse Frames", None, Command::ReverseFrames, has_selection && picked);
                if ui
                    .checkbox(&mut self.editor.edit_multiple, "Edit Multiple Frames")
                    .on_hover_text(
                        "Dragging on the stage moves layers across the picked frames (or all of them)",
                    )
                    .clicked()
                {
                    ui.close();
                }
                ui.separator();
                item(
                    ui,
                    "Start Layer Here",
                    Some(shortcuts::LAYER_START_HERE),
                    Command::LayerStartHere,
                    has_selection,
                );
                item(
                    ui,
                    "End Layer Here",
                    Some(shortcuts::LAYER_END_HERE),
                    Command::LayerEndHere,
                    has_selection,
                );
                ui.separator();
                // Morphing into the next drawing swap; see `morph.rs`.
                let tween = self.editor.shape_tween_here();
                item(ui, "Create Shape Tween", None, Command::CreateShapeTween, tween == Some(false));
                item(ui, "Remove Shape Tween", None, Command::RemoveShapeTween, tween == Some(true));
                item(ui, "Draw Inbetweens", None, Command::MakeInbetweens, tween.is_some());
                ui.separator();
                // What a Web Page export does on reaching this frame.
                let main = self.editor.timeline() == TimelineId::Main;
                ui.add_enabled_ui(main, |ui| {
                    ui.menu_button("Frame Action", |ui| {
                        let (frame, last) = (self.editor.frame, self.editor.last_frame());
                        let mut action = self.editor.doc.frame_action(frame).cloned();
                        let before = action.clone();
                        ui.horizontal(|ui| {
                            ui.label(format!("On frame {}:", frame + 1));
                            crate::inspector::web_action_editor(ui, "frame action", &mut action, last);
                        });
                        if action != before {
                            self.editor.doc.set_frame_action(frame, action);
                        }
                        ui.weak("What a Web Page export does on reaching this frame,\nas Animate's stop() and gotoAndPlay().");
                    });
                });
                ui.separator();
                let label =
                    if self.editor.doc.camera_layer().is_some() { "Select Camera" } else { "Add Camera" };
                item(ui, label, None, Command::AddCamera, true);
                ui.separator();
                item(ui, "First Frame", None, Command::FirstFrame, true);
                item(ui, "Last Frame", None, Command::LastFrame, true);
                ui.separator();
                ui.checkbox(&mut self.xsheet.open, "Exposure Sheet");
            });
            ui.menu_button("View", |ui| {
                item(ui, "Zoom In", Some(shortcuts::ZOOM_IN), Command::ZoomIn, true);
                item(ui, "Zoom Out", Some(shortcuts::ZOOM_OUT), Command::ZoomOut, true);
                item(ui, "Fit Stage", Some(shortcuts::ZOOM_FIT), Command::ZoomFit, true);
                item(ui, "Actual Size", Some(shortcuts::ZOOM_100), Command::ZoomActualSize, true);
                ui.separator();
                item(ui, "Rotate View Left (4)", None, Command::RotateViewLeft, true);
                item(ui, "Rotate View Right (6)", None, Command::RotateViewRight, true);
                item(ui, "Reset Rotation (5)", None, Command::ResetRotation, true);
                ui.weak("Or Shift+Space and drag to turn it freely.");
                ui.separator();
                ui.checkbox(&mut self.editor.onion.enabled, "Onion Skin");
                ui.checkbox(&mut self.editor.light_table, "Light Table")
                    .on_hover_text("Fade every layer but the selected one, to draw on it");
                ui.separator();
                ui.menu_button("Safe Areas", |ui| {
                    for kind in SafeAreas::ALL {
                        ui.radio_value(&mut self.editor.guides.safe_areas, kind, kind.label());
                    }
                    ui.weak("Keep action inside the outer line\nand text inside the inner one.");
                });
                ui.checkbox(&mut self.editor.guides.field_guide, "Field Guide")
                    .on_hover_text("A 12-field chart over the camera's frame, for planning moves in fields");
                ui.separator();
                // On or off, shown as a highlighted item, with its keys.
                let toggle = |ui: &mut egui::Ui, on: bool, label: &str, keys: egui::KeyboardShortcut| {
                    let button =
                        egui::Button::new(label).selected(on).shortcut_text(ctx.format_shortcut(&keys));
                    let clicked = ui.add(button).clicked();
                    if clicked {
                        ui.close();
                    }
                    clicked
                };
                if toggle(ui, self.editor.show_rulers, "Rulers", shortcuts::RULERS) {
                    toggled = Some(Command::ToggleRulers);
                }
                if toggle(ui, self.editor.rig_setup, "Rig Setup (Skeletons at Rest)", shortcuts::RIG_SETUP) {
                    toggled = Some(Command::ToggleRigSetup);
                }
                ui.menu_button("Perspective Guide", |ui| {
                    let current = self.editor.doc.perspective.as_ref().map(|g| g.kind);
                    let mut options = vec![(None, "None")];
                    options.extend(inkframe_core::PerspectiveKind::ALL.map(|k| (Some(k), k.label())));
                    for (kind, label) in options {
                        if ui.add(egui::Button::new(label).selected(current == kind)).clicked() {
                            toggled = Some(Command::SetPerspective(kind));
                            ui.close();
                        }
                    }
                    ui.weak("Drag its vanishing points on the stage.\nShift with the Pen snaps lines to it.");
                });
                ui.menu_button("Guides", |ui| {
                    let editor = &self.editor;
                    let toggles = [
                        (editor.show_guides, "Show Guides", shortcuts::SHOW_GUIDES, Command::ToggleGuides),
                        (
                            editor.lock_guides,
                            "Lock Guides",
                            shortcuts::LOCK_GUIDES,
                            Command::ToggleLockGuides,
                        ),
                        (
                            editor.snap_to_guides,
                            "Snap to Guides",
                            shortcuts::SNAP_TO_GUIDES,
                            Command::ToggleSnapToGuides,
                        ),
                    ];
                    for (on, label, keys, command) in toggles {
                        if toggle(ui, on, label, keys) {
                            toggled = Some(command);
                        }
                    }
                    if ui
                        .add_enabled(!editor.doc.ruler_guides.is_empty(), egui::Button::new("Clear Guides"))
                        .clicked()
                    {
                        toggled = Some(Command::ClearGuides);
                        ui.close();
                    }
                    ui.weak("Drag from a ruler onto the stage for a guide;\ndrag it back to take it away.");
                });
                ui.separator();
                ui.menu_button("Workspace", |ui| {
                    for workspace in Workspace::ALL {
                        item(ui, workspace.name(), None, Command::Workspace(workspace), true);
                    }
                });
                item(ui, "Reset Workspace", None, Command::ResetLayout, true);
                ui.weak("Drag an area's corner to split or join it,\nCtrl+Space to fill the window with it.");
            });
            ui.menu_button("Scripts", |ui| self.scripts.menu(ui, &self.editor));
            let help = ui.menu_button("Help", |ui| {
                item(ui, "Find a Command…", Some(shortcuts::FIND_COMMAND), Command::OpenFinder, true);
                item(ui, "Take the Tour", None, Command::StartTour, true);
                ui.separator();
                if ui.button("Inkframe for Artists (Guidebook)").clicked() {
                    open_in_browser(GUIDEBOOK_URL);
                    ui.close();
                }
                if ui.button("Getting Started Guide").clicked() {
                    open_in_browser(GUIDE_URL);
                    ui.close();
                }
                if ui.button("Report a Problem…").clicked() {
                    open_in_browser(ISSUES_URL);
                    ui.close();
                }
                ui.weak(format!("Inkframe {}", env!("CARGO_PKG_VERSION")));
            });
            crate::tour::Targets::note(&mut self.tour_targets.help, help.response.rect);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // The workspace switcher, at the top right as in Animate.
                let current = self.layout.workspace_name();
                egui::ComboBox::from_id_salt("workspace").selected_text(current.name()).width(110.0).show_ui(
                    ui,
                    |ui| {
                        for workspace in Workspace::ALL {
                            if ui.selectable_label(workspace == current, workspace.name()).clicked() {
                                chosen = Some(Command::Workspace(workspace));
                            }
                        }
                        ui.separator();
                        if ui.button(format!("Reset {}", current.name())).clicked() {
                            chosen = Some(Command::ResetLayout);
                        }
                    },
                );
                ui.add_space(8.0);
                ui.weak(format!("{:.0}%", self.views.stage().zoom_percent(&self.editor.doc)));
                // Find a Command, where it's easy to see.
                ui.add_space(8.0);
                let keys = ctx.format_shortcut(&shortcuts::FIND_COMMAND);
                let find = ui
                    .add(crate::icons::LabelledButton::new(crate::icons::Icon::Search, &format!("Find a command  {keys}")))
                    .on_hover_text("Search every command and tool by name");
                crate::tour::Targets::note(&mut self.tour_targets.finder, find.rect);
                if find.clicked() {
                    chosen = Some(Command::OpenFinder);
                }
            });
        });

        if let Some(command) = chosen.or(toggled) {
            self.execute(command);
        }
    }

    // ---------------------------------------------------------------- files

    /// Notes that the tour has been seen, so it doesn't open by itself
    /// again, and does what its last card asked.
    fn tour_ended(&mut self, ended: crate::tour::Ended) {
        self.toured = true;
        if ended == crate::tour::Ended::OpenExamples {
            self.execute(Command::OpenExample);
        }
    }

    /// The settings to keep for next time, as they are now.
    fn current_prefs(&self) -> Prefs {
        Prefs {
            shape_per_layer: !self.editor.draw_into_layer,
            show_rulers: self.editor.show_rulers,
            hide_guides: !self.editor.show_guides,
            lock_guides: self.editor.lock_guides,
            no_guide_snapping: !self.editor.snap_to_guides,
            toured: self.toured,
            ..Prefs::gather(self.editor.guides, self.batch.jobs(), self.batch.chosen_folder())
        }
    }

    fn replace_editor(&mut self, editor: Editor) {
        // The tool and guides are the person's, not the document's.
        let (tool, guides) = (self.editor.tool, self.editor.guides);
        self.editor = editor;
        self.editor.tool = tool;
        self.editor.guides = guides;
        self.views.stage().zoom_to_fit();
    }

    fn open(&mut self) {
        if !self.confirm_discard_changes() {
            return;
        }
        let Some(path) =
            rfd::FileDialog::new().add_filter("Inkframe document", &[FILE_EXTENSION]).pick_file()
        else {
            return;
        };
        match Editor::open(&path) {
            Ok(editor) => self.replace_editor(editor),
            Err(err) => show_error(&format!("Couldn't open {}", file_name(&path)), &err.to_string()),
        }
    }

    /// Opens an example as a new project, so saving it asks where to.
    fn open_example(&mut self, path: &Path) {
        if !self.confirm_discard_changes() {
            return;
        }
        match Document::load(path) {
            Ok(doc) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.replace_editor(Editor::untitled_copy(doc, name));
            }
            Err(err) => show_error(&format!("Couldn't open {}", file_name(path)), &err.to_string()),
        }
    }

    fn import_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import to stage")
            .add_filter("Images and Photoshop files", &[&IMAGE_EXTENSIONS[..], &["psd"]].concat())
            .pick_file()
        else {
            return;
        };
        if let Err(message) = self.editor.import_image(&path) {
            show_error("Couldn't import the image", &message);
        }
    }

    /// File › Export PSD: the current frame, layer by layer.
    fn export_psd(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export Photoshop file")
            .add_filter("Photoshop file", &["psd"])
            .set_file_name(format!("{}.psd", self.editor.display_name()))
            .save_file()
        else {
            return;
        };
        if let Err(message) = self.editor.export_psd(&path) {
            show_error("Couldn't export the Photoshop file", &message);
        }
    }

    /// Imports image and sound files dropped onto the window, the same as
    /// choosing them from the File menu. Images dropped onto the Library go
    /// into it without being placed on the stage.
    fn import_dropped_files(&mut self, ctx: &egui::Context) {
        let onto_library =
            self.hovered_area.is_some_and(|area| self.layout.kind_of(area) == Some(Kind::Library));
        // A file dropped without a path on disk can't be linked to, so skip it.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if onto_library && IMAGE_EXTENSIONS.contains(&extension.as_str()) {
                if let Err(message) = self.editor.add_library_image(&path) {
                    show_error("Couldn't import the image", &message);
                }
            } else if IMAGE_EXTENSIONS.contains(&extension.as_str()) || extension == "psd" {
                if let Err(message) = self.editor.import_image(&path) {
                    show_error("Couldn't import the image", &message);
                }
            } else if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
                self.editor.import_audio(&path);
            } else if FONT_EXTENSIONS.contains(&extension.as_str()) {
                self.import_font(&path);
            } else {
                show_error(
                    "Can't import that file",
                    &format!(
                        "{} isn't an image, Photoshop, sound or font file Inkframe can read.",
                        file_name(&path)
                    ),
                );
            }
        }
    }

    /// Several at once, since a family often comes as separate files for
    /// regular, bold and italic.
    fn import_fonts(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import fonts")
            .add_filter("Fonts", &FONT_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        for path in paths {
            self.import_font(&path);
        }
    }

    fn import_font(&mut self, path: &Path) {
        if let Err(message) = self.editor.import_font(path) {
            show_error("Couldn't import the font", &message);
        }
    }

    fn import_audio(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import audio")
            .add_filter("Audio", &AUDIO_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        self.editor.import_audio(&path);
    }

    /// Saves to the current file, asking for a location if there isn't one.
    /// Returns whether the document was saved.
    fn save(&mut self) -> bool {
        match self.editor.path.clone() {
            Some(path) => self.save_to(&path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Inkframe document", &[FILE_EXTENSION])
            .set_file_name(format!("{}.{FILE_EXTENSION}", self.editor.display_name()))
            .save_file()
        else {
            return false;
        };
        self.save_to(&path)
    }

    fn save_to(&mut self, path: &Path) -> bool {
        match self.editor.save_to(path) {
            Ok(()) => {
                // Saved, so there's nothing left to recover.
                if let Some(autosave) = &mut self.autosave {
                    autosave.clear();
                }
                true
            }
            Err(err) => {
                show_error(&format!("Couldn't save {}", file_name(path)), &err.to_string());
                false
            }
        }
    }

    /// If there are unsaved changes, asks whether to save them. Returns
    /// `true` if it's OK to continue (saved or discarded), `false` to stay put.
    /// Offers back work left behind by an Inkframe that didn't close
    /// properly, newest first.
    fn recovery_window(&mut self, ctx: &egui::Context) {
        if self.recoveries.is_empty() {
            return;
        }
        let mut recover = None;
        let mut discard = Vec::new();
        egui::Window::new("Recover Unsaved Work")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    "Inkframe didn't close properly last time. This work was autosaved but never saved:",
                );
                ui.add_space(6.0);
                egui::Grid::new("recoveries").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                    for (i, found) in self.recoveries.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_min_width(320.0);
                            ui.strong(&found.name);
                            let place = found
                                .original
                                .as_ref()
                                .map_or_else(|| "Never saved".to_owned(), |p| p.display().to_string());
                            ui.add(egui::Label::new(egui::RichText::new(place).weak()).truncate());
                            ui.weak(format!("Autosaved {}", found.when()));
                        });
                        if ui.button("Recover").clicked() {
                            recover = Some(i);
                        }
                        if ui.button("Discard").on_hover_text("Delete this autosaved copy").clicked() {
                            discard.push(i);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                if self.recoveries.len() > 1 && ui.button("Discard All").clicked() {
                    discard.extend(0..self.recoveries.len());
                }
            });
        if let Some(i) = recover
            && self.confirm_discard_changes()
        {
            match self.recoveries[i].open() {
                Ok(editor) => {
                    self.replace_editor(editor);
                    // It's this session's unsaved work now, autosaved as such.
                    discard.push(i);
                }
                Err(err) => show_error(&format!("Couldn't recover {}", self.recoveries[i].name), &err),
            }
        }
        discard.sort_unstable();
        discard.dedup();
        for i in discard.into_iter().rev() {
            self.recoveries.remove(i).discard();
        }
    }

    fn confirm_discard_changes(&mut self) -> bool {
        if !self.editor.has_unsaved_changes() {
            return true;
        }
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Unsaved changes")
            .set_description(format!("Save changes to “{}” first?", self.editor.display_name()))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match answer {
            rfd::MessageDialogResult::Yes => self.save(),
            rfd::MessageDialogResult::No => true,
            _ => false,
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        // A screenshot run quits without asking: nobody is there to
        // answer, and an unanswered dialog can take its default and save
        // the file that was opened.
        if self.screenshot.is_some() {
            self.allow_close = true;
            return;
        }
        let export_ok = !(self.export.is_running() || self.batch.is_running() || self.scripts.is_running())
            || {
                let answer = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Still working")
                .set_description(
                    "Quitting now will stop the export or script, which may leave an incomplete file. Quit anyway?",
                )
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show();
                answer == rfd::MessageDialogResult::Ok
            };
        if export_ok && self.confirm_discard_changes() {
            self.allow_close = true;
            // A normal quit: whatever wasn't saved was let go on purpose.
            if let Some(autosave) = &mut self.autosave {
                autosave.clear();
            }
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    /// One area's editor.
    fn show_area(&mut self, ui: &mut egui::Ui, area: AreaId, kind: Kind) {
        // Where the tour can find this area.
        let targets = &mut self.tour_targets;
        match kind {
            Kind::Tools => crate::tour::Targets::note(&mut targets.tools, ui.max_rect()),
            Kind::Properties => crate::tour::Targets::note(&mut targets.properties, ui.max_rect()),
            Kind::Timeline => crate::tour::Targets::note(&mut targets.timeline, ui.max_rect()),
            _ => {}
        }
        match kind {
            Kind::Stage => {
                // What can be done now, above the stage (see `context_bar.rs`).
                let top = ui.cursor().top();
                let asked = crate::context_bar::show(ui, &mut self.editor);
                let full = ui.max_rect();
                let bar = egui::Rect::from_x_y_ranges(full.x_range(), top..=ui.cursor().top());
                crate::tour::Targets::note(&mut self.tour_targets.context_bar, bar);
                crate::tour::Targets::note(
                    &mut self.tour_targets.stage,
                    egui::Rect::from_x_y_ranges(full.x_range(), bar.max.y..=full.max.y),
                );
                if self.editor.timeline() != TimelineId::Main {
                    breadcrumbs(ui, &mut self.editor);
                }
                if let Some(stage) = self.views.stages.get_mut(&area) {
                    stage.show(ui, &mut self.editor);
                }
                if let Some(command) = asked {
                    self.execute(command);
                }
            }
            Kind::Timeline | Kind::GraphEditor => {
                if let Some(timeline) = self.views.timelines.get_mut(&area) {
                    // The area's kind says which mode; the Graph button
                    // in the transport bar switches the kind.
                    let graph = kind == Kind::GraphEditor;
                    timeline.set_graph_mode(graph);
                    timeline.show(ui, &mut self.editor, &mut self.audio);
                    if timeline.graph_mode() != graph {
                        let other = if graph { Kind::Timeline } else { Kind::GraphEditor };
                        self.views.retype = Some((area, other));
                    }
                }
            }
            Kind::Properties => {
                if let Some(inspector::AudioAction::Import) =
                    inspector::show(ui, &mut self.editor, &mut self.audio)
                {
                    self.import_audio();
                }
            }
            Kind::Tools => {
                egui::ScrollArea::vertical()
                    .id_salt(("tools", area))
                    .show(ui, |ui| toolbar::show(ui, &mut self.editor));
            }
            // The Library scrolls its cards itself, under the preview.
            Kind::Library => self.library.show(ui, &mut self.editor, &mut self.audio),
            Kind::ExposureSheet => {
                if let Some(sheet) = self.views.sheets.get_mut(&area) {
                    sheet.contents(ui, &mut self.editor, &mut self.audio);
                }
            }
            Kind::ToolSettings => {
                egui::ScrollArea::vertical()
                    .id_salt(("tool_settings", area))
                    .show(ui, |ui| inspector::tool_settings(ui, &mut self.editor));
            }
            Kind::Audio => {
                egui::ScrollArea::vertical().id_salt(("audio", area)).show(ui, |ui| {
                    if let Some(inspector::AudioAction::Import) =
                        inspector::audio_section(ui, &mut self.editor, &mut self.audio)
                    {
                        self.import_audio();
                    }
                });
            }
            Kind::Document => {
                egui::ScrollArea::vertical()
                    .id_salt(("document", area))
                    .show(ui, |ui| inspector::document(ui, &mut self.editor));
            }
            Kind::Swatches => {
                egui::ScrollArea::vertical()
                    .id_salt(("swatches", area))
                    .show(ui, |ui| panels::swatches(ui, &mut self.editor, &mut self.panels));
            }
            Kind::Align => {
                egui::ScrollArea::vertical()
                    .id_salt(("align", area))
                    .show(ui, |ui| panels::align(ui, &mut self.editor, &mut self.panels));
            }
            Kind::Bones => {
                egui::ScrollArea::vertical()
                    .id_salt(("bones", area))
                    .show(ui, |ui| crate::bones_panel::show(ui, &mut self.editor));
            }
            Kind::MotionPresets => {
                egui::ScrollArea::vertical()
                    .id_salt(("presets", area))
                    .show(ui, |ui| self.presets.show(ui, &mut self.editor));
            }
            Kind::Templates => {
                egui::ScrollArea::vertical()
                    .id_salt(("templates", area))
                    .show(ui, |ui| self.templates.show(ui, &mut self.editor));
            }
            Kind::NodeView => {
                if let Some(view) = self.views.nodes.get_mut(&area) {
                    view.show(ui, &mut self.editor);
                }
            }
            Kind::ScriptConsole => self.scripts.console(ui, &mut self.editor),
            Kind::DepthView => {
                if let Some(view) = self.views.depths.get_mut(&area) {
                    view.show(ui, &mut self.editor);
                }
            }
            Kind::Controllers => {
                egui::ScrollArea::vertical()
                    .id_salt(("controllers", area))
                    .show(ui, |ui| panels::controllers(ui, &mut self.editor));
            }
            Kind::History => {
                egui::ScrollArea::vertical()
                    .id_salt(("history", area))
                    .stick_to_bottom(true)
                    .show(ui, |ui| panels::history(ui, &mut self.editor));
            }
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
        self.xsheet.show(&ctx, &mut self.editor, &mut self.audio);
        self.adjust.show(&ctx, &mut self.editor);
        self.vectorise.show(&ctx, &mut self.editor);
        self.swap.show(&ctx, &mut self.editor);
        self.scripts.show(&ctx, &mut self.editor);
        if let Some(path) = self.examples.show(&ctx) {
            self.open_example(&path);
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
            screenshot.update(&ctx, self.audio.is_busy() || self.lip_sync.is_busy());
        }
    }
}

/// The "Scene ▸ Symbol ▸ …" bar shown while editing a symbol. Clicking an
/// earlier step goes back out to it.
/// The editors that keep state of their own, one per area showing them, and
/// which stage menu commands act on.
#[derive(Default)]
struct Views {
    stages: BTreeMap<AreaId, StageView>,
    timelines: BTreeMap<AreaId, TimelineView>,
    sheets: BTreeMap<AreaId, ExposureSheet>,
    nodes: BTreeMap<AreaId, crate::node_view::NodeView>,
    depths: BTreeMap<AreaId, crate::depth_view::DepthView>,
    /// The stage last pointed at.
    active_stage: Option<AreaId>,
    /// An area to change to another kind once the layout has drawn: a
    /// timeline whose Graph button was pressed, say.
    retype: Option<(AreaId, Kind)>,
    /// Stands in when no area shows the stage, so commands still have one.
    spare_stage: StageView,
}

impl Views {
    /// Makes views for areas that need one, and forgets those of areas
    /// that have gone or changed kind.
    fn prepare(&mut self, layout: &Layout) {
        // Every tab, showing or not, so a hidden tab keeps its view.
        let areas = layout.all_tabs();
        let of = |kind: Kind| areas.iter().filter(move |a| a.1 == kind).map(|a| a.0).collect::<Vec<_>>();
        let (stages, sheets, graphs) = (of(Kind::Stage), of(Kind::ExposureSheet), of(Kind::NodeView));
        let timelines: Vec<AreaId> = areas.iter().filter(|a| a.1.is_timeline()).map(|a| a.0).collect();
        self.stages.retain(|id, _| stages.contains(id));
        self.timelines.retain(|id, _| timelines.contains(id));
        self.sheets.retain(|id, _| sheets.contains(id));
        self.nodes.retain(|id, _| graphs.contains(id));
        let depths = of(Kind::DepthView);
        self.depths.retain(|id, _| depths.contains(id));
        for id in depths {
            self.depths.entry(id).or_default();
        }
        for id in graphs {
            self.nodes.entry(id).or_default();
        }
        for id in stages {
            self.stages.entry(id).or_default();
        }
        for id in timelines {
            self.timelines.entry(id).or_default();
        }
        for id in sheets {
            self.sheets.entry(id).or_default();
        }
        if self.active_stage.is_none_or(|id| !self.stages.contains_key(&id)) {
            self.active_stage = self.stages.keys().next().copied();
        }
    }

    /// The stage menu commands act on.
    fn stage(&mut self) -> &mut StageView {
        match self.active_stage.and_then(|id| self.stages.get_mut(&id)) {
            Some(stage) => stage,
            None => &mut self.spare_stage,
        }
    }

    /// Whether a gesture is under way in any stage or timeline, which
    /// holds off recording an undo step.
    fn is_busy(&self) -> bool {
        self.stages.values().any(StageView::is_busy) || self.timelines.values().any(TimelineView::is_busy)
    }
}

fn breadcrumbs(ui: &mut egui::Ui, editor: &mut Editor) {
    let path: Vec<(SymbolId, String)> = editor
        .open_symbols()
        .map(|id| (id, editor.doc.symbol(id).map_or_else(|| "?".to_owned(), |s| s.name.clone())))
        .collect();
    let mut exit_to = None;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).fill(ui.visuals().panel_fill).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui
                    .link("Scene")
                    .on_hover_text("Back to the main timeline (Esc steps out one level)")
                    .clicked()
                {
                    exit_to = Some(0);
                }
                for (depth, (_, name)) in path.iter().enumerate() {
                    ui.weak("›");
                    // The last step is where we are, so it isn't a link.
                    if depth + 1 == path.len() {
                        ui.strong(name);
                    } else if ui.link(name).clicked() {
                        exit_to = Some(depth + 1);
                    }
                }
            });
        },
    );
    if let Some(depth) = exit_to {
        editor.exit_to(depth);
    }
}

/// Shown over the window while files are dragged over it.
fn drop_hint(ctx: &egui::Context) {
    let area = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_hint")));
    painter.rect_filled(area, 0.0, egui::Color32::from_black_alpha(140));
    painter.text(
        area.center(),
        egui::Align2::CENTER_CENTER,
        "Drop images, sounds or fonts to import them",
        egui::FontId::proportional(20.0),
        egui::Color32::WHITE,
    );
}

pub(crate) fn show_error(title: &str, detail: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(detail)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}
