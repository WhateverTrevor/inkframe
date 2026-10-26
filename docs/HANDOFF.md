# Handoff: where Inkframe stands

A checkpoint for picking development back up. Last updated 2026-10-19,
after the Harmony features (palettes to master controllers).

## State

- **Branch:** `main`, clean. Every feature is built on its own branch,
  then merged into `main` with a merge commit.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo test --workspace` (531 tests)
  all pass, on macOS locally and on macOS and Windows in CI. Clippy can
  also be run for Windows from the Mac:
  `cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets`.
- **File format:** version 30. Older versions load and upgrade automatically
  (see `crates/inkframe-core/tests/format_migration.rs`).
- **Git identity:** set for this repository only (`git config user.name` /
  `user.email`), since the Mac has no global identity.
- **Remote:** https://github.com/WhateverTrevor/inkframe (public). CI runs
  on macOS and Windows for every push and pull request, and both pass.
  Commits use the GitHub noreply address, not a personal email.

## Built so far (in order)

1. Core editor: shapes, pencil, layers, keyframe tweening, easing, timeline,
   onion skin, undo, PNG / ProRes / H.264 / HEVC-alpha export
2. Transform handles (scale, rotate)
3. Pen tool
4. Subselection tool (move anchors and handles)
5. Adding points with the Pen tool (curve-preserving split)
6. Corner/smooth conversion (Alt-click)
7. Per-property keyframes (format v2)
8. Layer start/end frames (format v3)
9. Symbols: library, instances, Convert to Symbol (format v4)
10. Editing inside symbols, with breadcrumbs
11. Edit in place (rotated/scaled instances, faded surroundings)
12. Audio tracks: import, waveforms, synced playback, scrubbing, export
    (format v5)
13. macOS packaging, part A: `cargo xtask bundle-macos [--dmg] [--ffmpeg]`
    builds an ad-hoc-signed `Inkframe.app`, with an app icon drawn in code
14. Windows installer: `cargo xtask bundle-windows` (Inno Setup), built and
    uploaded as an artifact by CI on every push, with gyan.dev's static
    ffmpeg 9.0.2 bundled (pinned URL and SHA-256 in ci.yml)
15. Selection bounds include stroke width and hug curves tightly
16. Easing curve editor: drag the Bézier handles in the properties panel
17. Faster renderer: translucent layers use scratch buffers sized to the
    layer, frames render in parallel bands, off-screen shapes are skipped
18. Image import (format v6): File › Import to Stage (⌘R) or drag and
    drop; linked files like audio, decoded once with mipmaps
19. Text (format v7): Text tool (T), edited in the properties panel;
    fontdb + harfrust + skrifa outlines, filled and stroked like shapes
20. Project fonts (format v8): File › Import Font… links font files,
    searched before installed fonts; a Refresh button rescans installed
    fonts; exports fail if a font file in use is missing
21. On-stage text editing: type directly on the stage with a cursor and
    selection that follow rotated/scaled text (`text_edit.rs`)
22. Motion paths, part 1 (format v9): position keys carry `Bend` handles;
    curved tweens are travelled by arc length; drag the path on the stage
    to bend it; Orient to path.
23. Motion guides (format v10): `Layer::guide` and `Layer::follow`; a
    follower runs the whole guide between its first and last position
    keys (middle keys pin to the nearest point). `Document::pose` resolves
    this; use it instead of `Layer::transform_at`. Guides aren't exported.
24. Masks (format v11): `Layer::mask` and `Layer::masked_by`; anything a mask
    draws is the window (soft masks via its opacity); Modify › Mask masks
    the layer below; clicks pass through masks unless one is selected.
25. Copy and paste: layers (with symbols, images, fonts; between projects
    via tagged JSON on the system clipboard), Paste in Center/in Place,
    Copy/Paste Frames (a layer's pose), and plain text as a text layer.
26. Multi-select: `Editor::selected_ids` (primary `selection` plus
    `also_selected`, which lapses when `selection` is set directly).
    Shift-click, marquee, timeline Shift/⌘-click, ⌘A. Moving, deleting,
    duplicating, copying and keyframing apply to all selected layers.
27. Group scale and rotate: handles around several selected layers,
    treated as one imaginary layer (`transform_group` in stage.rs).
28. Value graph: the timeline's Graph mode (`value_graph.rs`) plots a
    property over time on the timeline's frame axis; drag keys (value and
    frame) and easing handles in real values.
29. Font fallback: characters the chosen font lacks come from another
    font (preferred emoji/CJK/symbol fonts, then any installed face).
30. Input methods when typing on the stage: the stage reports its caret
    (`PlatformOutput::ime`), composing text is shown underlined and
    replaced on commit (`TextCursor::composing`).
31. Skew (format v12): a fifth animatable property (`Property::Skew`),
    applied as scale → skew → rotate; drag a selection box edge to slant.
    Uneven group stretching of rotated layers is now exact
    (`Transform::from_affine`).
32. Color emoji: the bundled Noto Color Emoji (COLRv1, OFL, in
    `assets/fonts/`, pinned to googlefonts/noto-emoji commit 1ffdd21) is
    painted as vector layers by `color_glyph.rs`; installers include its
    licence.
33. Parenting and movable pivots (format v13), the first step of the
    Harmony/OpenToonz "character" plan: `Layer::parent` and `Layer::pivot`,
    with `Document::layer_matrix` in `rig.rs` used everywhere a layer is
    drawn, hit or edited. A child's pose is in its parent's space; stage
    gestures convert the pointer into that space (`Handles::space`,
    `into_parent_space`). Option/Alt-drag moves the pivot (a plain drag
    still moves the layer). Selecting a parent and child together only
    transforms the parent (`Document::hangs_from`). The rest of the plan
    is drawing substitution (34), lip sync (35), bones/IK (36), deformers
    (37) and the exposure sheet (38).
34. Drawing substitution (format v14): `Layer::drawings` (never empty)
    and `Layer::exposure` hold keys, in `drawing.rs`. Everything that
    draws, hits or edits a layer reads `elements_at(frame)`; the editor's
    `element`/`element_mut` do that for the current frame. Inspector
    Drawings section, Modify menu items, `[`/`]` to flip, orange swap
    marks in the timeline. Drawing tools add into the shown drawing when
    the selected layer has more than one (`Editor::add_drawn_shape`),
    converting the shape to a path in the layer's coordinates.
    `examples/rig.inkf` has a mouth with four drawings.
35. Lip sync: `lip_sync.rs` in core (no file format change) and the Lip
    Sync window (`lip_sync_dialog.rs`, Modify › Lip Sync…). Loudness uses
    `AudioEngine::peaks`; timing files are Rhubarb TSV or Papagayo
    MohoSwitch1. The user chose "built-in, then Rhubarb": bundling Rhubarb
    (MIT) to run it from inside the app, like ffmpeg, is the agreed next
    step for lip sync, needing their OK for the download.
36. Bones and IK (format v15): the Bone tool (`B`, `Tool::Bone`) draws
    each rig's skeleton (`draw_skeleton` in stage.rs) and drags with
    `Gesture::Reach`, calling `Document::reach` in `ik.rs` (CCD, root
    joint first, nudging out of the straight-arm stall). Roots move the
    rig. `Layer::rotation_limits` is set in the inspector's rig section
    ("Bends").
37. Deformers (format v16): `Layer::deformer` (`deform.rs`), a bone
    deformer with rest joints, animated per-bone bends and smoothness.
    `Layer::shown_elements` applies it for render, hit testing and
    bounds; editing tools still edit the unbent drawing (`elements_at`),
    so Subselection points show where the drawing is at rest. Bone tool
    drags green joints (`Gesture::Bend`). Images and text don't bend.
    F6/⇧F6 and dragging summary keys include the deformer's keys.
38. Exposure sheet: `xsheet.rs`, a window opened from the Timeline menu
    (no format change). Columns for layers with several drawings and for
    sounds; double-click to type a drawing name (Enter moves down a row,
    a new name makes a blank drawing), right-click to pick or clear.
    `Layer::clear_exposure` and `Layer::drawing_named` in `drawing.rs`.
    This completes the Harmony/OpenToonz character plan.
39. Rhubarb Lip Sync 1.14.0 bundled (user approved the ~174 MB download
    of both zips; they're in the git-ignored `vendor/downloads/rhubarb/`,
    unpacked essentials in `vendor/rhubarb/`). `rhubarb.rs` runs it; it's
    the Lip Sync window's default source when found. xtask `--rhubarb`
    for both installers (macOS: `Contents/Resources/rhubarb/`, since only
    code may sit in `MacOS/`; `xattr -cr` before signing). CI downloads
    both pinned zips and sets `INKFRAME_REQUIRE_RHUBARB`. The macOS build
    is x86_64 (Rosetta); EBADARCH is explained to the user. The disk image
    grew from about 37 MB to 110 MB.
40. Painting, phase 1 of 4 (format v17): project files are zips
    (`container.rs`); paint layers hold tiled canvases (`paint.rs`);
    `brush.rs` is the brush engine; Brush (B), Eraser (E), Eyedropper (I)
    tools, with Bone moved to K. The brush panel shows in the properties
    panel while painting. `tablet.rs` reads pen pressure (Windows: egui
    touch force; macOS: NSEvent local monitor). `[`/`]` resize the brush
    while painting.
41. Painting, phase 2: Paint Bucket and Gradient (`fill.rs`), Smudge,
    Blur, Sharpen, Dodge, Burn and Clone Stamp (`retouch.rs`), brush
    presets and paper grain (`brush.rs`). Toolbar slots group related
    tools (`Tool::GROUPS`, `Editor::use_slot`): the key picks the slot's
    last-used tool, Shift+key cycles, right-click lists them. Keys: G
    bucket/gradient, U smudge/blur/sharpen, D dodge/burn, S clone stamp.
    The properties panel's tool section shows each tool's own settings.
42. Painting, phase 3 (format v18): selections (`selection.rs`; Marquee,
    Lasso, Polygonal Lasso, Magic Wand tools; Editor::pixel_selection,
    not saved or undone), limiting every painting tool, fill and
    adjustment; marching ants; moving selected pixels with the Selection
    tool (`Gesture::MovePixels`); Layer via Copy/Cut (core) and Merge Down
    (`inkframe_render::merge_down`); adjustments (`adjust.rs`, window in
    `adjust_dialog.rs`); layer blend modes (`blend.rs`). The toolbar goes
    to two columns when the window is too short.
43. Painting, phase 4: the stage view turns (`View::rotation`, about the
    stage's middle; Shift+Space drag, keys 4/5/6, View menu), with the
    stage drawn as a textured quad and fit-to-window allowing for the
    turn. PSD import/export (`psd.rs` in core, `export_psd` in render;
    File › Export PSD…, .psd accepted by Import to Stage). Verified
    against macOS's own PSD reader (sips reads our files) and 20 real
    Photoshop files shipped with macOS (all read). This completes the
    painting plan agreed with the user.
44. Layers restack by dragging their names in the timeline
    (`Drag::Reorder`, `Document::move_layer_to`). The window is now a
    Blender-style layout of areas (`layout.rs`): any editor kind in any
    area, split/join/swap from corners, resize borders, Ctrl+Space to
    maximise, saved to `~/Library/Application Support/Inkframe/
    layout.json` (Windows: `%APPDATA%\Inkframe`). Each stage, timeline and
    exposure-sheet area has its own view (`Views` in app.rs); commands act
    on the last stage pointed at. Screenshots always use the default
    layout (`INKFRAME_SCREENSHOT_LAYOUT=custom` shows a rearranged one).
45. The area header menu works (the header's right-click target had been
    added over it and took its clicks; a test now clicks through it) and
    lists ten editors in three groups, Blender-style. New kinds: Graph
    Editor (a timeline in graph mode; its Graph button swaps the area's
    kind), Tool Settings, Audio and Document. The timeline's layer
    buttons no longer overlap the transport bar in narrow areas.

46. UI round 1 (of 3, from the Animate comparison): `theme.rs` (Inter
    Regular 3.19 OTF bundled in assets/fonts, palette, spacing) and
    `icons.rs` (drawn icons replacing every emoji in the UI). Area headers
    are a flat icon + name + chevron button; the editor menu is drawn
    items. Panel areas get a margin; toolbar swatches use a red slash and
    a no-colour button. `INKFRAME_SCREENSHOT_AREA_MENU=1` opens the Stage
    header menu. Next: round 2, the layers panel and timeline (thumbnails,
    colour tags, folders, Animate-style cells); round 3, tabbed areas,
    workspace presets, Properties tabs, Swatches/Align/History panels.

47. UI round 2: layers panel and timeline (format v19). Layer folders
    (`folder.rs` in the core; a folder is a `Layer` with `folder` set,
    members name it in `in_folder` and sit directly below it), layer
    colours (`Layer::color` / `tag_color`), per-layer thumbnails
    (`layer_thumbnails.rs`, `render_layer_thumbnail`), variable row
    heights in the timeline's `Layout` (`tops`), Animate-style cells (dots
    on keyframes, tinted tweens with arrows, a box where a bar ends) and
    the selection outline on the stage in the layer's colour.
    `INKFRAME_SCREENSHOT_FOLDER=1` puts the rig's arm in a folder. Next:
    round 3 (tabbed areas, workspace presets, Properties tabs, Swatches /
    Align / History panels).

48. UI round 3: tabbed areas (`Node::Area::tabs`, `add_tab`, `move_tab`
    by dragging a tab to another header, icon-only tabs when they don't
    fit), workspace presets (`Workspace`; switcher at the menu bar's top
    right), Properties tabs (Tool / Layer / Document), and the Swatches,
    Align and History panels (`panels.rs`; core `align.rs`, history step
    names from `history::describe`). Document swatches are saved in the
    file without a format bump (older builds ignore them). Old saved
    layouts get a Library tab beside Properties (`Layout::upgraded`).
    `INKFRAME_SCREENSHOT_WORKSPACE=animator` (etc.) screenshots a workspace.
    Next, from the Animate comparison: shape tweens, a camera layer,
    filters and colour effects.

49. Shape tweens (format v20; `morph.rs`): `Layer::shape_tweens`
    (`ShapeTween { end, easing }`, keyed by the exposure swap it morphs
    into, dropped by `tidy_exposure` if that swap goes), `morph_at` inside
    `shown_elements`, and `Drawing::hints` (shape hints, paired by index).
    Timeline › Create/Remove Shape Tween, Modify › Add Shape Hint (⌘⇧H) /
    Remove All Hints, controls in the Drawings section, a green band in
    the timeline, lettered hints on the stage (Selection tool; yellow on
    the start drawing, green on the end; right-click removes).
    `INKFRAME_SCREENSHOT_SHAPE_TWEEN=1` adds a square-to-ellipse demo layer
    with one hint. Next from the Animate comparison: a camera layer, then
    filters and colour effects.

50. Camera (format v21; `camera.rs`): a `Layer` with `camera` set, kept
    on top of the main timeline; its pose (position looked at, rotation,
    `scale.x` zoom) is applied to every top-level main-timeline layer in
    `Document::parent_matrix`, so rendering, hit testing, handles and
    export all go through it. `Layer::depth` gives parallax
    (`F / (F + depth)`, `FOCAL_LENGTH` 1000). Camera tool (`C`): drag
    pans, Shift turns, Alt zooms, with an orange frame overlay; Camera
    section in Properties; Timeline › Add Camera and a timeline-bar
    button. One camera only (duplicates and pastes become plain layers;
    it can't become a symbol). `INKFRAME_SCREENSHOT_CAMERA=1` shows one.
    Next from the Animate comparison: filters and colour effects.

51. Colour effects and filters (format v22; `effects.rs`):
    `Layer::color_effect` and `Layer::filters`, applied in the renderer's
    scratch-buffer path; the buffer reaches `filter_margin` past the layer
    and past the band (a test checks a blur across a band edge is
    symmetric). Effects are fixed per layer, not animated: tweening them
    would need channels in `Track`. Properties has Colour Effect and
    Filters sections; the timeline labels such layers "fx".
    `INKFRAME_SCREENSHOT_EFFECTS=1` gives the rig a shadow, glow and tint.
    Also: layer thumbnails now only redraw on font/image loading for
    layers that show text or pictures, which fixed a flaky test.
    This completes the Animate comparison list.

52. Animated effects (format v23): `Track::effects`, a sixth channel
    (`Property::Effects`) whose value is `Effects { color, filters }`.
    `Animatable` now needs only `Clone`. `Effects::lerp` blends matching
    settings, fades a one-sided colour effect or filter from a neutral
    one, and holds where kinds differ. Format 22's fixed fields load into
    the channel (`Layer::unanimated_*`, emptied in `validate`). The graph
    editor leaves Effects out (`GRAPHED` in timeline.rs). The screenshot
    hook's head glow now grows in over frames 1–25.

53. Linked palettes (`palette.rs`): `Document::palette` replaces the
    plain `swatches` list (loaded into it). Swatches own their exact
    colour rather than shapes storing a link, so shapes, the renderer and
    the format are unchanged; swatch colours are kept distinct per
    version. Versions recolour the scene all at once. The Swatches panel
    lists the palette with names, a right-click colour picker that
    recolours live, and a version menu. `INKFRAME_SCREENSHOT_PALETTE=1`
    builds a palette of the rig's colours with a Night version on.

54. Ink and paint (format v24; `vector_fill.rs`): `Drawing::colour_art`
    beneath the line art (`Drawing::shown`, used by `shown_elements` and
    morphs); `Layer::fill_enclosed` and `enclosed_area` (grid flood,
    gap closing by thickening lines, grow back, trace, Douglas–Peucker).
    Areas inside a filled one aren't cut out of it; filling them adds a
    fill on top. The paint bucket fills vector layers (the selected one,
    or the one clicked) and only makes a paint layer when there's none.
    Editor options `draw_into_layer` and `colour_art` let the drawing
    tools draw into one drawing. `INKFRAME_SCREENSHOT_INK=1` shows a
    gapped, filled head.

55. Light table (`RenderOptions::light_table`): every top-level layer
    but the selected one draws at 30%; symbols' own layers go with their
    instance. View menu and a timeline-bar button. Also fixed: a selected
    mask is drawn faintly on the stage again (its `show_mask` had never
    been passed to the main stage render). `INKFRAME_SCREENSHOT_LIGHT_TABLE=1`
    turns it on with the rig's head selected. Note: screenshots need the
    display awake; with it asleep the window never draws and the hook
    never fires.

56. Free-form deformer (format v25; `warp.rs`): `Layer::warp`, a grid
    (2–8 points a side) of `Channel<Point>` offsets over the drawing's
    rest bounds, blended as a Bézier patch; applied after the bone
    deformer in `shown_elements`, its keys part of the layer's keys
    (F6, delete, drag). Bone tool drags its points (`Gesture::Warp`);
    Properties has a Free-Form Deformer section. Harmony's curve
    deformer is close enough to the existing bend deformer that it was
    left out; envelope deformers aren't done.

57. Master controllers (format v26; `controller.rs`):
    `Document::controllers`, each a set of layers with `ControllerPose`s
    (transforms and drawing indices per layer) at points along a 0–1
    slider. `set_controller` blends the bracketing poses and keys the
    layers with `set_pose` (and `expose` for drawings), so nothing is
    evaluated at render time. Deleted layers drop out
    (`tidy_controllers`, on delete and load). A Controllers panel
    (`Kind::Controllers`) lists them. Not done: Harmony's on-stage
    controller widgets (dials, 2D pads).

58. Node View (`node_view.rs`, `Kind::NodeView`): a visual editor of
    the existing model, not a new compositing engine (the user chose
    this). Built from the layers every frame; `connect`/`disconnect`
    map wires to `set_parent`, `mask`/`masked_by`, `place_layer` and
    effects edits. Node positions live in the view (per area, not
    saved); unmoved nodes are auto-laid out in rows, and the view fits
    the graph until zoomed or panned. `INKFRAME_SCREENSHOT_NODES=1`
    puts one in place of the timeline.

59. Template library (`template.rs` in the core, `templates.rs` in the
    app, `Kind::Templates`): a `Template` is `CopiedLayers` plus a name,
    the format version and the first frame, saved as `.inktemplate` JSON
    in `<settings>/Templates` (`layout::settings_dir`, shared with
    layout.json). `Layer::shift_time` / `AnyChannel::shift` move a
    layer's whole animation in time for inserting at the playhead.
    Thumbnails are rendered from the template, framed to its drawing.
    Layers whose parent wasn't saved with them come in unattached (as
    pasting does). `INKFRAME_SCREENSHOT_TEMPLATES=1` uses a temporary
    folder, never the real library.

60. Pressure-sensitive lines (format v27; `line.rs`): `Shape::Line {
    path, widths }` with a width per centreline anchor, filled with the
    element's fill. The Pencil records `tablet::current()` pressure per
    sample, densifies the fitted path (`with_anchors_every`) so widths can
    vary, and sets widths from the nearest sample's pressure and an end
    taper (`pencil_pressure`, `pencil_taper`). Width tool
    (`Tool::Width`, in the Subselection slot) drags a point's width.
    Subselection edits a line's outline (turning it into a plain filled
    shape), as Animate does with brush strokes. Still unverified with a
    real tablet, like the paint brush's pressure.

61. Motion blur: `FilterKind::MotionBlur { shutter (degrees), samples }`,
    animatable like other filters, but drawn by the renderer: it draws
    the layer at moments across the shutter (`Document::layer_matrix_at`,
    the fractional-frame placement, through parents and the camera) and
    averages them, into a buffer covering the whole sweep. Samples rise
    automatically so moments are at most ~2 px apart (up to 64); a still
    layer is drawn once. Only movement blurs (shapes changing within a
    drawing don't). `INKFRAME_SCREENSHOT_MOTION_BLUR=1` adds a fast ball.

62. Sprite sheet export: `ExportFormat::SpriteSheet` (no ffmpeg, no
    audio), in `inkframe-export/src/sprite_sheet.rs`. Frames are cropped
    to the union of their drawn areas (transparent background; with the
    background on nothing is cropped), identical frames share a cell, and
    `Grid::fit` packs them near-square under 8192 px or fails with
    `ExportError::SheetTooLarge`. Writes `name.png` + `name.json`
    (TexturePacker JSON hash with trim fields, `animations`, per-frame
    `duration`). The Export dialog gained a 25% size.
    `INKFRAME_SCREENSHOT_EXPORT=sprite` opens the Export window on a format.
    Not yet loaded into an actual engine.

63. Batch export: `inkframe_export::batch` (`BatchJob`, `outputs`, `run`
    with `BatchEvent`s) shared by File › Batch Export (`batch_dialog.rs`)
    and `inkframe --export` (`cli.rs`, handled in `main` before eframe
    starts; on Windows it calls `AttachConsole` so a GUI-subsystem
    release build can print). Jobs read projects from disk; the window
    warns when the open project has unsaved changes. Outputs default to
    `Exports/` beside the first project. `ExportFormat::slug`/`from_slug`
    name formats for files and the command line.
    `INKFRAME_SCREENSHOT_BATCH=1` queues the open project and its
    neighbours and starts. The queue isn't remembered between runs, and
    the Windows console output is unverified on a real PC.

64. Field guide and safe areas: `guides.rs` in the app (`Guides` on the
    Editor: `safe_areas: SafeAreas`, `field_guide: bool`), View menu
    toggles, drawn by `stage::draw_guides` on the stage rectangle via
    `View::stage_to_screen` (so they stay on the camera frame while
    editing inside a symbol). Field n is n/12 of the frame at its own
    aspect. Not saved with the document or between runs.
    `INKFRAME_SCREENSHOT_GUIDES=safe,field`.

65. Vectorise: `inkframe-core/src/vectorise.rs` (`trace`, `TraceOptions`,
    `TraceMode::{Ink, Colours}`, `Document::vectorise_layer`, which clones
    the layer via `duplicate_layer`, replaces Paint/Image elements with
    traced paths, names it "… (vector)" and hides the source). Shapes
    have no holes, so holes are spliced in with zero-width slits; this
    relies on `FillRule::Winding` and holes tracing the opposite way.
    Pixels under half opaque are left out (a very soft brush stroke traces
    thinner than it looks). App: `vectorise_dialog.rs` (preview traced and
    rendered on a thread), Modify › Vectorise…, `ImageLibrary::full_size`.
    A full-HD frame traces in ~20 ms (ink) to ~70 ms (colours).
    `INKFRAME_SCREENSHOT_VECTORISE=ink|colours[,apply]` (with `_PAINT`).
    No centreline tracing (lines become filled outlines, not pencil
    lines).

66. Particles (format v28; core `particles.rs`, render `particles.rs`):
    `Layer::particles: Option<Emitter>`, drawn in `draw_layer` instead of
    the drawing (which is the sprite when `Sprite::Drawing`). Stateless,
    seeded (SplitMix64 per particle and channel), world space via
    `Document::placement_without_camera`; capped at `MAX_PARTICLES`.
    Presets Sparks/Snow/Smoke/Fire (fire and sparks get blend Add).
    Modify › New Particle Layer, inspector Particles section, emitter
    outline and heading drawn on the stage when selected
    (`stage::draw_emitters`). Emitter settings don't animate, and motion
    blur doesn't smear individual particles. The four presets together
    render a full-HD frame in ~14 ms. `INKFRAME_SCREENSHOT_PARTICLES=1`.

67. 3D multiplane (format v29): `Property::Depth` / `Track::depth`
    (`depth_at`, `set_depth`, left out of files while a single 0 key);
    the old `Layer::depth` loads into it (`unanimated_depth`, moved in
    `validate`). The camera's own depth is its dolly: perspective is
    `F / (F + d - z)` in `camera_matrix_at`, and `camera_fade` (used by
    `render_layers`) fades layers within 0.25F of the camera. Inspector:
    Depth row in Transform (top-level main layers), Dolly in the camera
    section. `depth_view.rs` is a new area kind (Kind::DepthView). Depth
    `INKFRAME_SCREENSHOT_MULTIPLANE=1` (with `_FRAME`).

68. Stack by depth (format v30): `Document::stack_by_depth`, off by
    default. `Document::stacking_order(timeline, frame)` (camera.rs) is
    used by `render_timeline_into`, `export_psd` and `hit_test_in`;
    `render_layers` now takes `&[&Layer]`. Main timeline only (symbols
    keep their own order). Toggles: Depth View corner, Document
    properties. The timeline still lists layers in its own order; its bar
    shows a "Stacked by depth" chip while it applies (main timeline),
    which turns it off when clicked.
    `INKFRAME_SCREENSHOT_MULTIPLANE=stack`.

69. Remembered settings: `prefs.rs` (`Prefs`, `prefs.json` beside
    `layout.json`) keeps the guides and the Batch Export queue and
    folder. Saved like the layout (on change, no pointer down, never in
    screenshot runs). `replace_editor` now carries the guides over when
    opening or starting a project (they used to reset). Batch rows whose
    project is gone say "Project missing". To check loading:
    `HOME=<scratch> INKFRAME_SCREENSHOT_PREFS=1` with a prefs.json there.

70. Autosave and crash recovery: `autosave.rs` (`Autosave`, `Recovery`,
    `Recovery/<pid>-<nanos>.inkf` + `.json` note in the settings folder).
    Every 30 s while unsaved and changed since the last copy; written on
    a thread; heartbeat every 30 s; copies quiet for 120 s are offered at
    start-up (`recovery_window`, Recover / Discard / Discard All).
    Cleared on save (`save_to`) and on a normal quit. Off in screenshot
    runs; `INKFRAME_SCREENSHOT_RECOVERY=1` shows the window from a
    simulated crash in a temp folder. A real crash (killing the process
    mid-edit) hasn't been tried by hand.

71. Releases: pushing a tag `v<version>` runs CI as usual and then the
    `release` job, which checks the tag matches `Cargo.toml`'s version,
    takes that version's section of `CHANGELOG.md` as the notes, and
    creates a **draft** GitHub release with the `.dmg` and `-setup.exe`
    from the test jobs. To release: bump `version` in `Cargo.toml`, add a
    `## [x.y.z]` section to CHANGELOG.md, merge, `git tag vx.y.z && git
    push origin vx.y.z`, then review and publish the draft on GitHub.
    Builds are ad-hoc signed (macOS) and unsigned (Windows); signing and
    notarizing need Apple and Microsoft developer accounts.

72. Getting started: `docs/GETTING_STARTED.md` (a bouncing-ball
    tutorial and an examples gallery, images in `docs/images/`), linked
    from the README and Help › Getting Started Guide (Help › Report a
    Problem opens the issues). Samples `multiplane`, `particles` and
    `ink-and-paint` are built by `cargo xtask samples`
    (`xtask/src/samples.rs`); rerun it after a format change. The render
    test `tests/examples.rs` opens and draws every example.

73. Bundled examples: `cargo xtask bundle-macos` / `bundle-windows` copy
    `examples/*.inkf` into the package (`util::copy_examples`); the app's
    `examples.rs` gallery (File › Open Example…) finds them and opens one
    as an untitled copy. Add a line to `ABOUT` for a new example (a test
    fails otherwise). `INKFRAME_SCREENSHOT_EXAMPLES=1`.

74. Scripting spike: `crates/inkframe-script` (rquickjs 0.14 / QuickJS;
    `run(doc, source, limit) -> Result<Outcome, ScriptError>`, working on
    a clone, interrupt handler for the time limit, 256 MB memory cap).
    Not yet used by the app. Because QuickJS is C, `cargo clippy
    --target x86_64-pc-windows-msvc` on a Mac fails for it: use
    `--exclude inkframe-script` locally and rely on CI for Windows. Plan
    and results: docs/SCRIPTING_PLAN.md.

75. Scripting step 2: the API (docs/SCRIPTING.md) in `inkframe-script`
    (`native.rs`, `prelude.js`, `values.rs`, `files.rs`), the Scripts
    menu (`scripts.rs`) and `inkframe --script` (`cli_script.rs`).
    `shiftKeys` moves only keys; `shiftTime` moves the whole layer. The
    app now depends on the C-built QuickJS, so local Windows clippy must
    exclude both: `cargo clippy --workspace --exclude inkframe-script
    --exclude inkframe-app --target x86_64-pc-windows-msvc`.
    `INKFRAME_SCREENSHOT_SCRIPT=path.js` runs a script at start-up.
    Next (step 3): the Script Console area and example scripts.

76. Scripting step 3: Kind::ScriptConsole (Icon::Console) drawn by
    `Scripts::console`; console runs report into its log instead of the
    Script Output window. Example scripts in `examples/scripts`, built in
    (`EXAMPLES`, a test keeps the list complete) and written to the
    Scripts folder by `add_examples_if_new` when it doesn't exist yet.
    `inkframe-script/tests/examples.rs` runs each on the rig (the video one
    only with ffmpeg). `INKFRAME_SCREENSHOT_CONSOLE=path.js`.

77. Scripting step 4 (plan complete): symbols, rigs, controllers,
    effects, particles and camera in the API (natives in `native.rs`,
    wrappers in `prelude.js`, reference in docs/SCRIPTING.md). Effects and
    emitters round-trip through serde with camelCase keys and type names.
    `snowy-night.js` example (adds to an existing camera move rather than
    replacing it).
78. Pen-friendly timeline, brush preview and grouping: in the timeline,
    `Drag::Layer` with `LayerDrag::{Pending, Scrolling, Reordering}`: a
    name dragged at once scrolls the list (pens have no wheel), held for
    `HOLD_TO_REORDER` (0.35 s) it lifts to restack. `brush_preview` in
    inspector.rs paints a tapering sample stroke with the real
    `BrushStroke` (texture cached in `ui.data` by settings), and the stage
    cursor shows an inner hardness ring. `Document::convert_layers_to_symbol`
    (`ConvertError`) groups several layers, rigs and links kept, into one
    instance at their bounding-box centre; F8 uses it when several are
    selected. Non-adjacent layers collapse together in the stack.
    Screenshot hook `INKFRAME_SCREENSHOT_GROUP=1|open`.
79. Empty layers: `Editor::add_empty_layer` (timeline + button,
    Modify › New Layer, ⌘⇧N) adds an empty vector layer just above the
    selected one, in its folder, origin at the stage centre.
    `draw_into_layer` now defaults to on (Animate draws into the current
    layer); `Prefs::shape_per_layer` remembers turning it off. The
    getting-started guide uses New Layer for the floor.
80. Asset library (format v31): core `library.rs` (`Asset`, folder
    names on Symbol/ImageAsset/AudioTrack, `Document::library_folders`),
    app `library.rs` rewritten as a card grid (search, size slider,
    folders, context menus). Cards drag out as an `Asset` payload:
    `stage::asset_drop` places at the drop point
    (`Editor::place_asset_at`), `TimelineView::asset_drop` from that frame
    above that row (`place_asset_on_timeline`). Images dropped onto the
    Library area are added without placing (`add_library_image`). Image
    cards use `inkframe_render::render_image_thumbnail` (mipmapped).
    Screenshot hook `INKFRAME_SCREENSHOT_LIBRARY=1`. `Editor::place_symbol`
    was folded into `place_asset_at`. Next: the symbol previewer
    (`LibraryView::selected` is the card to preview).
81. Symbol previewer: the Library's Preview (always shown, so picking a
    card never moves the grid) plays the selected symbol in a loop with
    pause and a scrubber (`Preview { clock }`), shows images larger, and
    draws a sound's waveform with play/stop (`AudioEngine::preview`,
    `preview_position`, `duration`; a preview survives entering a
    symbol). Hovering an animated card plays it in the card. Thumbnails
    are fitted to `inkframe_render::symbol_extent` (all frames).
    `IconButton` now reports its tooltip as its accessible label.
82. Visual polish: a soft shadow under the stage (`draw_stage_shadow`,
    built from the corners so it turns with the view), an empty-stage
    hint (`draw_empty_stage_hint`), a pill playhead with a pointer and a
    tinted current-frame column, a light highlight on the timeline row
    under the pointer, and Properties section titles in small spaced
    capitals with a hairline (`inspector::section`, `section_text`).
    Further polish should follow the same approach: look at a
    screenshot, fix one rough edge at a time.
83. Shape selection inside a layer (the user chose "double-click to go
    inside" over Animate's click-picks-shape, to keep click-drag-to-key):
    `Editor::inside` (`inside.rs`: enter_layer, pick_shapes, shape_at,
    shapes_in, transform_shapes, delete_shapes, shapes_to_symbol,
    check_inside each frame), `stage/inside.rs` (`ShapeDrag` Move, Scale,
    Box; the pill note; cursor). Core `Element::transformed` /
    `is_reshapable` (anchor.rs) moves a shape by its points, turning
    rectangles and ovals into paths; `add_to_shown_drawing` uses it too.
    Inside, the light table fades the other layers and the layer's own
    box and handles hide. Keys: Esc → ExitLayer, Delete → DeleteShapes,
    F8 → shapes_to_symbol, ⌘A → pick_all_shapes. Then: picked shapes are
    `ShapeRef { colour_art, index }` (line art over colour art), turn with
    a knob (`ShapeDrag::Rotate`, reusing `handles::RotateDrag`), and copy,
    cut and paste as `Clipboard::Shapes(CopiedShapes)`, stored in timeline
    coordinates so they land in place in any layer (`paste_shapes` steps
    into the selected layer, or a new one).
84. Windows builds link the C runtime statically
    (`.cargo/config.toml`, `+crt-static` for x86_64-pc-windows-msvc), so
    inkframe.exe needs no Visual C++ redistributable; CI's "Check the
    Windows app needs no Visual C++ runtime" step fails if `dumpbin
    /dependents` shows vcruntime/msvcp/api-ms-win-crt.
85. Several layers in Properties (`multi_edit.rs`): the panel edits the
    last-picked layer, then `carry_settings` (checkboxes, blend, span),
    `carry_pose_change` (position as a delta in each layer's own parent
    space, other components set only if changed), `carry_depth` and
    `carry_key` carry it to `also_editing` (the other selected layers,
    not folders). `pose_differs` marks rows with •. Effects and easing
    are copied only from an "Apply to the other N selected layers"
    button (`copy_effects_to_selected`, `copy_easing_to_selected`); rig,
    instance and shape settings edit the last-picked layer only.
86. ⌘Z while typing on the stage: `text_edit::TypingHistory` (word-sized
    steps, redo with ⌘⇧Z/Ctrl+Y), reset in `begin_text_editing`. CHANGELOG
    has an Unreleased section; 0.1.0's is marked as never published.
87. Frame commands and Edit Multiple Frames: core `frames.rs`
    (`Layer::insert_frames/remove_frames/reverse_frames` over a private
    `retime`; `AnyChannel::retime`, `reverse_span` swaps bends and
    reverses easing; reversing keys both ends first). App
    `frame_commands.rs`: `Editor::frame_range` (picked in the timeline
    with Shift-click/drag, `Drag::Range`, drawn by `paint_frame_range`),
    F5/Shift+F5/F7 and Timeline menu items; `edit_multiple` with
    `MultiFrameMove` carried on `Gesture::Move { multi }`: animated layers
    shift position keys in range, static ones move the drawings shown in
    range. `new_drawing_on(id, frame, copy)` split out of `new_drawing`.
    Hooks `INKFRAME_SCREENSHOT_FRAMES=4-12`, `_MULTIPLE=1`. Not yet:
    dragging picked frames to move them, scale/rotate across frames.
88. Gradient fills and line styles (format v32): core `Style.gradient:
    Option<ShapeGradient>` (Style is no longer Copy; `.clone()` where
    needed), `Stroke { cap, join, dash }` with `Stroke::new`; `fill` stays
    the gradient's first colour for the bucket, palettes and hit tests.
    Shape tweens blend gradients stop by stop (`morph::blend_gradient`);
    `Element::transformed` scales dashes. Render: `gradient_paint`
    (tiny-skia Linear/RadialGradient), caps, joins, `StrokeDash`. App:
    one `style_editor` for tool and shapes (`gradient_stops`, `choice`);
    `stage/gradient_handles.rs` drags a picked shape's gradient ends
    (inside a layer only, so they don't clash with scale handles). Hook
    `INKFRAME_SCREENSHOT_STYLES=1`. Not yet: gradients in the script API,
    or a gradient on the paint bucket's fills.
89. GIF, SVG and Lottie export: `ExportFormat::{Gif, Svg, Lottie}`
    (`is_vector`), GIF through ffmpeg's palettegen/paletteuse, `svg.rs`
    and `lottie.rs` (see ARCHITECTURE.md). Render gained
    `shape_outline`, `canvas_image`, `render_layer_alone`. Verified in
    lottie-web 5.12.2 (cdnjs) in the browser pane against PNG renders:
    the rig, multiplane camera, ink and paint, a skewed layer, a looping
    symbol, gradients, dashes, a mask and a blurred layer. Particles
    embed a picture per frame (6.6 MB at quarter size for the example);
    the export window warns. Not done: Lottie text as real text (it's
    outlines), Lottie expressions or markers.
90. Boolean shapes: core `boolean.rs` (`ShapeOp`, `combine`, `break_apart`,
    `area_of`; written here rather than with a crate, see the module
    comment), app `shape_ops.rs` (`combine_shapes`, `break_apart`),
    Modify › Combine Shapes and Break Apart (⌘B). A shape is one path, so
    holes are slit in (`vectorise::join_hole`); a holed result with a
    stroke is split into a fill and stroke-only outlines so the slit
    doesn't show. Hook `INKFRAME_SCREENSHOT_COMBINE=1`. Not yet: combining
    shapes across layers, or fills and lines together.
91. Swap Symbol (format v33): `Layer::symbol_swaps: Vec<SymbolSwap>`,
    `symbol_at(frame)` (used by `instance_view`, so rendering, hit tests,
    SVG and editing follow it), `symbols_used` (use counts, nesting
    checks, clipboard), `Document::swap_symbol(layer, symbol, from)` and
    `duplicate_symbol` (layer ids remapped, rig relinked). Frame commands
    retime and reverse swaps; Lottie makes a precomp layer per run of one
    symbol. App `swap_dialog.rs` (requested from the inspector through
    egui memory, `swap_dialog::request`), swap marks in the timeline row
    (`paint_symbol_swaps`). Hook `INKFRAME_SCREENSHOT_SWAP=1` with
    `_GROUP=1`.
92. Motion presets: core `preset.rs` (`MotionPreset::from_layer`,
    `apply`, `built_in`; relative values per property, applying replaces
    keys in its span via `retime`), app `motion_presets.rs`
    (`Kind::MotionPresets`, `Icon::Motion`, presets as JSON
    `.inkpreset` files in settings/Motion Presets). Hook
    `INKFRAME_SCREENSHOT_PRESETS=1` (temp folder). Not yet: presets in
    the script API, effects in presets.
93. Rulers and guides (format v34): core `ruler_guides.rs`
    (`Document::ruler_guides`, `snap_point`, `snap_box`), app
    `stage/rulers.rs` (`handle_rulers` first in the stage's input,
    `draw_rulers`, `GuideDrag`), snapping in `Gesture::Move` (its new
    `bounds`) and `Gesture::DrawShape` via `snapping_guides` (main
    timeline only). Editor `show_rulers/show_guides/lock_guides/
    snap_to_guides`, remembered in prefs. Hook
    `INKFRAME_SCREENSHOT_RULERS=1`. Not yet: snapping the pen, pencil or
    scale handles.
94. Distort and Envelope: core `envelope.rs` (`Lattice`: the box's four
    sides as Bézier curves, the inside filled in with a Coons patch;
    Distort = 4 corners with straight sides (bilinear), Envelope = 8
    points (corners, side middles) each followed by the two handles of the
    curve to the next, 24 points clockwise from the top left;
    `move_point(index, to, smooth)` takes a point's handles with it and
    keeps a side-middle's handles in line unless Alt; `bend` cuts each
    segment with `deform::subdivide` and shares pencil-line widths along
    the pieces), app `envelope.rs` (`LatticeKind`, `LatticeEdit` in
    `Editor::lattice`: picked shapes inside a layer, or the whole drawing
    of the layer worked inside or selected, without stepping in;
    `move_lattice_point` re-bends the originals via
    `Editor::reshape_layer_shapes`; `check_lattice` ends it on
    frame/tool/target/shape-count changes; undo also ends it),
    `stage/lattice.rs` (`handle_lattice` after the rulers in the stage's
    input; a press off the handles finishes and falls through; the
    layer's own box and transform handles hide meanwhile). Modify ›
    Transform › Distort/Envelope; Enter/Escape finish. Hook
    `INKFRAME_SCREENSHOT_ENVELOPE=1`. Not yet: several layers at once;
    adding points to an Envelope's sides as Animate allows by
    subselecting.
95. Pen point editing: between paths (`StageView::pen` is `None`), the Pen
    picks the selected shape's points (`handle_pen` returns early on
    `anchor_hit`; the click block in `handle_tools` also runs for it) and
    drags points/handles through the Subselection's `EditAnchor` gesture
    (`begin_anchor_edit`, shared by both tools). `EditAnchor::pull`
    (Alt-drag on a point) uses `subselect::pull_handles` for symmetric new
    handles. Starting a new path drops the picked point. Hook
    `INKFRAME_SCREENSHOT_PEN_HANDLES=1` (`StageView::pick_anchor`). Not
    yet: Animate's minus cursor (click a point with the Pen to delete it;
    here Delete does that), Cmd as a temporary Subselection tool.
96. Skeletons, Moho rigging Phase A (format v35): core `skeleton.rs`
    (`Skeleton`/`Bone` with rest `start`/`end`, `strength`, animated
    `turn` (relative to parent) and `moved`; `pose` → per-bone
    rest-to-posed `Affine`s; `weights` = normalised 1/(d/strength)^4 to
    each rest segment; `carry`; `reach` = CCD IK; `move_bone`;
    `shift_rest`; `remove_bone` returns the index map), on
    `Layer::skeleton` (a folder) with `Layer::bone_binding` (Region default
    / Bone(i) / Unbound) on members; `Document::skeleton_layer_of`,
    `bone_space` (= folder's `parent_matrix`: parents + camera, not the
    folder's own transform), `layer_bend` → `LayerBend` (`bend`, `rigid`,
    `elements`), `posed_elements` (used by bounds/hit tests, SVG, Lottie,
    renderer). Renderer: `draw_canvas_bent`/`draw_image_bent` via
    `draw_bent` (16-unit triangle mesh, `triangle_map`, AA off so seams
    don't show); one-bone layers draw through one matrix. Skeleton keys
    join `Layer::key_frames`/insert/remove/move/shift/retime/reverse and
    show on the folder's timeline row. App: `skeleton.rs` (New Skeleton,
    `add_bone` hangs from `selected_bone`, `pose_bone`, `move_bone`,
    `delete_bone`), `Tool::AddBone` (with Bone, Shift+K), `stage/bones.rs`
    (`BoneDrag`, after the lattice in the stage's input; Add Bone shows
    bones at rest, Bone posed), inspector: folder "Add Skeleton", bone
    name/strength/turn/delete, member "Follows". Hook
    `INKFRAME_SCREENSHOT_SKELETON=1`. Phases next (agreed with the user):
    B point binding + smart bones + switch layers, C bone dynamics,
    squash/stretch, constraints (angle limits not yet on skeleton bones),
    D maybe shared points. Not yet:
    binding weights painted by hand.
97. Moho rigging Phase B (format v36): core `switch.rs` (`Layer::switch`,
    `switch_keys`, `folder_children`, `switch_choice` (smart bone >
    keys > topmost), `switched_off`, `shows` — used by render_layers,
    symbol bounds, SVG, Lottie, `hit_test_in`), `skeleton.rs`
    (`Bone::actions: Vec<SmartAction>`: `Morph {layer, drawing, turn}`
    applied in `posed_elements` via `morph::morph` before bending,
    `Switch {folder, layer, turn}` via `Document::smart_switch`;
    `amount` = turn/now clamped 0..1 by sign), pins
    (`Drawing::bone_points: Vec<BonePoints>` → `pins_of` → `ShapeBones`
    per shown shape; `bend_elements` takes `pins`; `bend_element` cuts per
    segment so pieces between same-bone points follow it; pencil lines
    pin only whole). `Drawing::shape_removed/shapes_inserted` keep pins on
    their shapes (called by delete_shapes, break_apart,
    replace_in_drawing). The renderer now takes shapes from
    `posed_elements`. App: `skeleton.rs` (`pin_shapes`, `pin_point`,
    `pinned_bone`, `shapes_pinned_bone`, `skeleton_members`,
    `new_switch`, delete_bone renumbers pins), `Editor::picked_anchor`
    (mirrored from the stage's anchor selection), inspector
    (`pin_controls`, `smart_bone_section`, folder `switch_controls`),
    pins marked by `draw_pins` with Add Bone, switch/skeleton keys on the
    folder row. Hook `INKFRAME_SCREENSHOT_SMART=1`. Not yet: reversing
    frames leaves switch keys; smart actions on layer transforms or bone
    angles; pins on pencil-line points; an action timeline as Moho's.
98. Moho rigging Phase C (format v37): `Bone::limits`
    (`RotationLimits`, clamped in `turn_at` and in `reach`), `stretch:
    Option<Channel<f32>>` + `squash` (across = 1/stretch, area kept),
    `dynamics: Option<Dynamics {spring, damping, limit}>`. `Skeleton::place`
    builds each bone: children are carried by the parent's rigid matrix
    translated to where its skin takes their start (`Pose::rigid`), so
    they follow a stretched end without stretching. `pose` simulates a
    tip spring (Verlet-ish, length-constrained, swing clamped) from
    `frame - SETTLE` (240) if any bone has dynamics; `is_at_rest` is false
    for such a skeleton unless nothing ever moves. App:
    `Editor::stretch_bone`, `BoneDrag::Stretch` (Shift with the Bone tool),
    `draw_limits` wedge for the picked bone, inspector rows (Stretch, Keep
    area, Limits, Dynamics + sliders). Hook `INKFRAME_SCREENSHOT_DYNAMICS=1`.
    Not yet: target bones, pose caching (dynamics re-simulate per call;
    fine at present sizes), Phase D (shared points), only if wanted.
99. Moho rigging Phase D, welded points (no format change): core
    `weld.rs` (welds by coincidence within `WELD_DISTANCE`; pencil lines
    and non-reshapable shapes excluded), `skeleton.rs` `weld_pins` (pins
    extend to welded points of other shapes unless pinned already). App
    stage: `Gesture::EditAnchor` gains `welds` (moved by the same delta
    when a point is dragged, not when handles are or with Alt-pull) and
    `snaps` (other shapes' points; a dragged point within ANCHOR_HIT on
    screen lands exactly on one, welding); `StageView::unweld` (⌘ held at
    drag start) skips the welds; welded points get a dot in
    `draw_anchor_overlay`. Hook `INKFRAME_SCREENSHOT_WELDS=1`. Not yet:
    moving whole picked shapes (inside a layer) doesn't drag welded
    neighbours' points; colour-art fills from the paint bucket aren't
    welded to the lines they fill; the Pen can't start a new path from an
    existing point (clicking one picks it).
100. Rig setup and painting on poses (no format change):
    `Document::rest_pose: RestPoseView` (serde skip, PartialEq always
    true) makes `layer_bend`, smart morphs and `smart_switch` stand down;
    the stage sets it each frame from `Editor::rig_setup || tool ==
    AddBone`; the export dialog's copy clears it. `LayerBend::unbend(p,
    area)` inverts through the mesh (`mesh_lines` aligned to `MESH_CELL`
    multiples, shared with the renderer's `draw_bent`, so it's exact where
    the two meshes overlap). Used by `canvas_point`/`canvas_to_screen`
    (all paint tools), `add_to_shown_drawing` (via `Element::mapped` over
    `unbend_area`), `fill_vector`. Bone tool in setup moves bones at rest.
    View › Rig Setup (⌥⌘B, `RIG_SETUP`), `draw_setup_note`. Hook
    `INKFRAME_SCREENSHOT_SETUP=1`. Not yet: Subselection edits on a posed
    frame still edit rest positions (they show the rest outline).
101. Bones panel: `bones_panel.rs` (`Kind::Bones`, `Icon::Bones`, in the
    Animation group; layout test now counts 20 kinds). `tree` lists bones
    depth-first from the roots; rows pick bones (selecting the skeleton);
    "Rigs" lists `skeleton_members` with a Follows combo and pin counts.
    Hook `INKFRAME_SCREENSHOT_BONES=1`.
102. Bent pixels in SVG/Lottie: `inkframe_export::bends_pixels` (a
    layer with Paint/Image shapes and a `layer_bend` on the frame) sends
    the layer down the existing "rendered" route (renderer draws it
    whole, embedded as PNG), per frame in SVG, for every frame in Lottie
    if any frame bends.
103. Folder keys: `Layer::move_key`/`remove_key` carry switch keys too;
    the timeline's press no longer grabs a folder's span edge (its bar
    spans its contents), so the folder row's keys (skeleton + switch, via
    `key_frames`) drag as any layer's do.
104. Target bones: `Bone::target: Option<usize>` (optional field, no
    format bump: older builds ignore it). `Skeleton::placing(frame)` gives
    every bone's (turn, move, stretch), then for targeted bones runs CCD
    (up to 24 rounds, stops within 0.05) over `target_chain` toward the
    target's posed start, respecting limits; `pose` (and the dynamics
    simulation, per frame) uses it. `remove_bone` renumbers targets.
    Inspector Target combo (bones it carries greyed out); the Bone tool
    draws a dotted line to the target. Hook `INKFRAME_SCREENSHOT_TARGETS=1`.
    (Roadmap item 5's other half, keyed corrections, is #110.)
105. Art/pattern brushes: core `art_brush.rs` (`Stroke::brush:
    Option<StrokeBrush{kind, symbol}>`, optional so no format bump;
    `Document::drawn_as` → fill-only copy + picture laid along the outline
    or a pencil line's centreline at its mean width; picture = symbol's
    frame-0 shapes in symbol space, brushes stripped so none recurse;
    Pattern tiles = round(length / natural tile)). Renderer, SVG and
    Lottie draw through `drawn_as`. `Layer::symbols_used` includes brush
    symbols so they can't be deleted while used. Inspector stroke row
    "Brush" (Plain/Art/Pattern + symbol). Hook
    `INKFRAME_SCREENSHOT_BRUSHES=1`. Not yet: a dedicated Paint Brush tool
    with a brush library panel; hit testing uses the plain line.
106. Line textures: `Layer::line_texture: Option<LineTexture{kind,
    amount}>` (optional, no bump). Renderer `texture.rs`: per-kind grain
    (OnceLock, 96-unit tile, hashed/value noise), `tile(color, texture)`
    → `paint(&tile)` (Repeat, identity transform: layer units). Render
    `Style::texture` set per layer in `draw_layer`; applies to Line fills
    and strokes, not area fills. Inspector "Lines" combo + amount. Hook
    `INKFRAME_SCREENSHOT_TEXTURES=1`. Not yet: textures in SVG/Lottie
    (they export flat).
107. Perspective guides: core `perspective.rs` (`Perspective{kind,
    points}` on `Document::perspective`, optional; `new` puts points on
    the stage; `move_point` keeps the horizon pair level; `directions`
    (toward each VP, plus across/up by kind); `snap`). App
    `stage/perspective.rs` (`handle_perspective` beside the rulers, in
    stage pixels, main timeline only; `draw_perspective` 36 rays per VP +
    horizon), Pen Shift uses `snap` via `PenPath::last_point`, View ›
    Perspective Guide (`Command::SetPerspective`). Hook
    `INKFRAME_SCREENSHOT_PERSPECTIVE=1`. Not yet: snapping the Pencil,
    Line or shape tools; several guides at once.
108. Assisted inbetweening: `Layer::make_inbetweens(frame)` in
    `morph.rs` (span from the key at/before `frame` to the next swap; a
    new numbered drawing per frame between, line art and colour art
    morphed separately with the hints, spaced by the span's shape tween
    easing, which is then removed). `Editor::make_inbetweens`, Timeline ›
    Draw Inbetweens (`Command::MakeInbetweens`). Hook
    `INKFRAME_SCREENSHOT_INBETWEENS=1`. Not yet: a choice of spacing
    (charts) when there's no shape tween; inbetweening a frame range at once.
109. Web page export: `ExportFormat::Html` (`html` slug, `.html`, silent,
    vector, no ffmpeg), `inkframe_export::html` (`page` dedupes frames
    into JSON in a `<script type=application/json>` with `</` escaped; a
    small player advances by elapsed time at the doc's fps, click to
    pause, arrows step when paused; the page is the doc's background).
    Checked in the browser (plays, pauses). Not yet: sound, and paint
    layers re-embed their PNG in every frame that differs. (Buttons and
    actions: #111.)
110. Smart bone corrections (format v38): `SmartAction::Correct{layer,
    drawing, keys: Vec<CorrectionKey{turn, colour_art, line_art}>}`
    (SmartAction no longer Copy), `corrected(rest, keys, now)` (rest is a
    key at 0; clamped past the ends; morph between neighbours), applied in
    `posed_elements` when that drawing shows and no shape tween runs.
    App: `Correcting` (action taken out of the bone, key shapes swapped
    into the drawing, rest stashed), `start_correction`/`add_correction`/
    `finish_correction`/`cancel_correction`, `settle`, `settled_doc` (used
    by save_to, autosave, export dialog), `check_correction` once a frame
    (finishes on frame/turn change); undo drops the state. Inspector
    correction row (key chips, Edit at, Remove, + Correction, Finish/Cancel
    banner), stage note, Enter/Esc. Hook `INKFRAME_SCREENSHOT_CORRECTION=1`.
111. Interactive web pages: core `web_actions.rs` (`WebAction` Play/
    Stop/GotoAndPlay/GotoAndStop/OpenLink, `FrameAction`,
    `Document::frame_actions` + `frame_action`/`set_frame_action`,
    `Layer::button`, all optional). SVG export marks button groups
    (`button_id` = layer id number); `html.rs` `Interaction` (frame
    actions re-counted from the export's first frame, buttons by id) in
    the page JSON; the player runs a frame's action as it enters it
    (tick-only, so a goto doesn't loop on itself), clicks run button
    actions, click-to-pause only when nothing's interactive; CSS hover/
    active brightness. App: inspector `web_action_editor` (Button row on
    layers), Timeline › Frame Action, "a" markers on the ruler. Checked
    in the browser via the ignored `write_interactive_sample` test
    (`target/web-check/buttons.html`): stops at the end, Replay replays,
    Pause holds. Not yet: frame actions don't move with Insert/Remove
    Frames; button over/down states as symbol frames; sound.
112. Discoverability pass: `context_bar.rs` (above each stage, drawn in
    `show_area`'s Kind::Stage before the stage; returns `Command`s run by
    `InkframeApp::execute`; per-tool actions, under-way Finish/Cancel,
    right-hand view switches), `command_finder.rs` (`CommandFinder` window,
    ⌘K `FIND_COMMAND`, menu bar search button, Help menu; `entries()`
    hand-written: keep it in step when adding menu commands; `Command`
    is now `pub(crate)` with `OpenFinder` and `UseTool`), `Tool::description`
    (toolbar tips list the slot's other tools), new icons (Search, Symbol,
    Distort, Envelope, Combine, BreakApart, RigSetup, Perspective, Ruler,
    StepOut, Inbetween, Check, Cross) and `icons::LabelledButton`. Hook
    `INKFRAME_SCREENSHOT_FINDER=query`. Ideas not done: icons in menus,
    collapsible Properties sections with icons, a first-run tour.
113. Guidebook: `docs/guide/` (chapters, `figures.txt`, `style.css`,
    README on extending it) built by `xtask/src/guide.rs` (`cargo xtask
    guide [--keep-shots]` → `target/guide/Inkframe-for-Artists.pdf`; own
    Markdown subset; headless Chrome `--print-to-pdf`, `CHROME` to
    override). Appendices come from `inkframe --reference`
    (`reference.rs`). When adding a tool or feature, add words to its
    chapter and a figure line if it needs a new scene. The LIPSYNC hook now
    makes up a line of dialogue (`screenshot::add_dialogue`) when the
    project has no sound. The TOC is numbered by printing twice
    (see #119); a sprite sheet must fit one 8192² image, so the
    batch figure shows the bouncing ball's sheet failing (a multi-sheet
    export would fix that).
114. First-run tour: `tour.rs` (`Tour` with 8 `STEPS`, each a `Target`;
    `Targets` noted each frame by `show_area` (Tools, Properties,
    Timeline, the Stage's context bar and stage) and `menu_bar` (Find a
    command button, Help menu); a Foreground shade with the target cut
    out takes the clicks; the card is placed beside it by `place`). The
    app skips its shortcuts while touring and waits for the recovery
    window. `Prefs::toured` saves that it was seen; `Command::StartTour`
    (Help › Take the Tour, Find a Command); Help links the guidebook
    (releases/latest). Hook `INKFRAME_SCREENSHOT_TOUR=n`; guidebook
    figure `tour` in chapter 1.
115. Multi-sheet sprite sheets: `sprite_sheet::pack` shares the distinct
    cells over as few `Sheet`s (a run of cells and a `Grid`) as fit
    `MAX_SIDE`, evenly; `write_sheets` (the limit as a parameter, for the
    unit test) writes `stem-N.png/.json` for a set, one sheet keeping the
    plain name; each JSON lists its own frames (numbered through the whole
    animation) and `meta.related_multi_packs`. `Grid::fit` falls back to
    the widest grid when the square one is too tall. `exported_file` names
    the first sheet for the Export and Batch windows' links.
116. Smart bone action timeline (no format change): core `Bone::preview:
    TurnPreview` (serde skip; PartialEq always true; Clone gives None, so
    history snapshots, `settled_doc` and exports never carry it) and
    `Bone::shown_turn` (used by `turn_at`, `posed_elements`,
    `smart_switch`, the app's correction start/check, the limits wedge and
    Bones panel). App `Editor::previewing` + `preview_turn`/`end_preview`/
    `previewed_turn`/`check_preview` (stage, before check_correction: other
    frame, bone or lost preview ends it, and a correction drawn at the
    preview is keyed as it moves on). `action_timeline.rs` strip (`span`,
    `show` -> `Asked::{Preview, EndPreview, Edit}`), drawn first in
    `smart_bone_section`; correction chips there became a key count.
    `Command::EndPreview` (context bar note, Esc). Hook
    `INKFRAME_SCREENSHOT_ACTIONS=deg`; guidebook figure `actions` (ch. 8).
117. Foldable Properties sections: `inspector::section(ui, editor, key,
    title, icon) -> bool` (arrow, icon, spaced capitals, hairline; click
    toggles `Editor::folded_sections` by key; labelled as a
    CollapsingHeader for accessibility/tests); each section returns early
    when folded (deformer, drawings, particles, shapes check their layer
    first, so the title only shows where the section would). New sections:
    "Parent and pivot" (rig_section), "Shapes"; Particles/Effects/Drawings
    titles converted. New icons Transform, Sparkle, Effects, Shapes (icon
    test now lists every icon). Saved as `Prefs::folded_sections`. Hook
    `INKFRAME_SCREENSHOT_FOLD=key,key`.
118. Saved Node View positions (no format bump: an optional field older
    builds ignore): core `node_places.rs` (`NodeRef` enum, now the app's
    `NodeKey` by re-export; `NodePlace{symbol: Option<SymbolId>, node,
    at}`; `Document::node_place`/`set_node_place`/`clear_node_places`),
    `Document::node_places`. The node view reads and writes them (a node
    drag is an ordinary document change, so it undoes and marks the
    project unsaved); Arrange clears the timeline's. Fixed: drags start
    from the press origin (nodes and wires no longer jump on grab), and
    dragging a node stops the auto-fit so the canvas doesn't slide.
119. Guidebook contents page numbers: `guide::page_numbers` parses Chrome's
    PDF (plain objects: catalog `/Pages` + `/Dests`; `/Dests` maps each
    heading anchor to `[page 0 R /XYZ …]`; page order from the tree,
    depth first) and the book is printed up to 4 times until the numbers
    it prints match what it reads. `Book::html(css, pages)` adds a dotted
    leader and number to each entry. No numbers found: prints without.
120. Guidebook projects + limb binding (format v39): chapters
    11-project-walk, 12-project-talking-head, 13-project-web-banner (preface
    and ch. 10 point to them). Hooks `INKFRAME_SCREENSHOT_WALK=drawn|rigged|
    follows` (walk_demo: overlapping legs on target feet, 24-frame cycle,
    hips bob) and `_BANNER=1` (banner_demo: 970×250, Pop In title, Open
    link button with a parented label, Stop on the last frame). Building
    the walk showed region binding can't keep overlapping legs apart, so
    core `BoneBinding::Limb(bone)` (`Skeleton::limb`, `carry_among`,
    `weights_among`) and the app's shared `inspector::binding_menu`/
    `binding_label` (Properties and Bones panel). Ideas: script API for
    skeletons, lip sync and buttons (the projects' scenes are built in
    Rust hooks because scripts can't yet).
121. Cut/Copy/Paste Frames: core `Track::copy_span`/`paste_span`/
    `clear_span` (keys at the span's ends frozen in; paste keys the frame
    after so what follows is unchanged) and `Clipboard::Frames(CopiedFrames
    {length, rows})`; app `Editor::copy_frames`/`cut_frames`/`paste_frames`
    in frame_commands.rs (frame_layers order, one row onto all, grows the
    timeline, picks the pasted span). ⌘X/⌘C/⌘V act on frames while
    `frame_range` is set; `Editor::select` of another layer clears it.
    Old pose copy renamed `Command::CopyPose`/`PastePose` (`COPY_POSE`,
    Edit › Copy Pose). Drawings and swaps come along for layers with >1 drawing: core `Layer::copy_span`/`paste_span`/`clear_span` (frames.rs) and `CopiedRow{track, drawings: Vec<(offset, Drawing)>}`; paste reuses a drawing with the same art (`drawing_like`), else adds it (renamed if taken), and re-keys what showed after the span.
122. Script API for characters and the web: `inkframe-script/src/rigging.rs`
    natives (addSkeleton, bones/addBone/setBone/poseBone by place,
    follows/setFollows, drawings/addDrawing/showDrawing, importAudio (sound
    extensions only, must exist), lipSync (loudness via inkframe_audio::decode
    + peaks_per_frame, or Rhubarb .tsv/.txt / Papagayo .dat), button/
    setButton, frameActions/setFrameAction), helpers in native.rs now
    pub(crate); layerGet reports `folder`; addLayer `text`+`size`. Prelude
    names bones (resolves names to places). Rhubarb itself isn't runnable
    from scripts (it lives in the app). Example `walk-cycle.js` (8 example
    scripts; tests/examples.rs runs it). Docs in SCRIPTING.md.
123. Speed check: `crates/inkframe-render/tests/heavy_scene.rs` (ignored;
    `cargo test --release -p inkframe-render --test heavy_scene -- --ignored
    --nocapture`; HEAVY_SCENE_PNG / HEAVY_SCENE_SAVE env vars) and app hook
    `INKFRAME_SCREENSHOT_BENCH=n` (InkframeApp::ui split into update_ui +
    step_bench). Profiling (macOS `sample`) showed Skeleton::pose recomputed
    per bound layer per band; fix: core `with_pose_cache` (thread-local,
    keyed by skeleton address + frame, scoped with a Drop guard, nested
    scopes share) around each `in_bands` worker. Results on the heavy scene:
    render 7.8 -> 5.6 ms (half size), 10.2 -> 7.0 ms (full); app update with
    a layer selected 56 -> 10 ms; with onion skins ~43 ms (5 renders: real
    work, left). Ideas: the stage's RenderKey clones the whole document each
    frame to compare; particles are simulated from scratch per frame.
124. Faster onion skins: stage `GhostCache` (per frame, the alpha coverage
    of an untinted ghost render, kept while `GhostKey` (timeline, to_stage,
    size, image/font generations) and the document hold; frames out of
    reach dropped) painted by `paint_coverage` (tint at coverage × opacity;
    overlapping shapes in one ghost now fade evenly rather than darker).
    `RenderKey` no longer holds a document copy: `rendered_doc` is compared
    in place and copied only on redraw. Heavy scene with onion: 43 -> 26 ms.
    Idea left: keep the main frame's coverage too (needs guides/light table
    off to match a ghost).
125. Properties search: `Editor::properties_search` (+ `properties_matches`
    counter), `inspector::search_box` at the top of `layer_tab`,
    `SECTION_WORDS` (each section key's searchable words; "layer" is the
    header block), `searched_for` used by `section()` (hidden unless
    matching; forced open while searching) and the layer header; inner
    Colour Effect/Filters headers open while searching. "No settings
    match" note. Hook `INKFRAME_SCREENSHOT_SEARCH=text`. Keep SECTION_WORDS
    in step when adding settings to a section.
126. Frame commands on the timeline's right-click menu: `TimelineView::show`
    (and `grid`, `context_menu`) take `frames_copied` and return an
    `Option<Command>` the app runs (it owns the clipboard). `Command::
    PasteFrames` now pastes only `Clipboard::Frames`.
127. Guidebook chapter 14, Automating with Scripts (`14-scripting.md`,
    figures `script-console` from `docs/guide/scripts/first-script.js` and
    `script-walk` from `examples/scripts/walk-cycle.js`). The guide's
    converter now sets fenced code blocks (``` lines) as `<pre><code>`.
    v0.5.0 tagged on a06720d; draft has dmg, setup.exe and the guidebook.
128. Rhubarb from scripts: `rhubarb.rs` moved from the app to
    `inkframe-audio` (`inkframe_audio::rhubarb`; adds `recognize`, the
    blocking run, and a hidden `write_hum` test helper; serde_json dep).
    Script `lipSync(sound, options)` takes `{ rhubarb, language, dialogue }`
    or `{ timing }` (options now passed as JSON). Only the bundled Rhubarb
    runs (`locate`).
129. Faster particles: profiling showed particle time was nearly all
    rasterising each soft dot as an anti-aliased gradient circle
    (particles_on_layer itself ~4%). Soft dots (softness > 0) now fill a
    unit square with the same radial gradient, anti-aliasing off; hard dots
    keep the AA circle. Heavy scene 5.0 -> 3.9 ms (half), 7.0 -> 5.5 ms
    (full); frames differ by at most 8/255 in ~0.01% of channels.
130. Quick start and named undo: `docs/GETTING_STARTED.md` cut to a one-page
    quick start pointing to the guidebook (Help › Quick Start (Five
    Minutes); README points at the guidebook first). Core
    `History::commit_as(doc, label)` and `next_labels()`; scripts.rs
    `finish` commits pending edits, applies the script's document, then
    `commit_as` "Run Script “name”" ("Script Console" for the console);
    Edit menu Undo/Redo show the next labels.
131. What's New: `whats_new.rs` (`notes(version)` parses CHANGELOG.md's
    `## [x.y.z]` section, built in; `WhatsNew` window with Got it / Read
    the guidebook). Shown at start-up when `Prefs::seen_version` differs
    from CARGO_PKG_VERSION and the tour isn't showing (new users get the
    tour; their version counts as seen); Help › What's New
    (`Command::WhatsNew`, Find a Command). A build between releases has no
    section, so nothing shows. Hook `INKFRAME_SCREENSHOT_WHATS_NEW=1`.
132. Script start-up: timed with `cargo run --release -p inkframe-script
    --example startup` (new example): a run, engine and prelude included,
    takes ~0.6 ms, so reusing the engine isn't worth it. The felt delay was
    the app: console runs requested no repaint (result appeared on the
    next input), menu runs waited for a 100 ms timer. `Scripts::ctx`
    (taken in `show`) is woken by the worker thread on finishing.
133. Profiling pass 2 (heavy scene, 18 cores): render workers re-posed every
    skeleton and re-bent every bound layer per band. Core `draw_cache.rs`
    (`DrawCache`: poses and posed shapes keyed by address+frame, one
    `OnceLock` per key so racing workers wait rather than duplicate;
    `with_draw_cache(&Arc<DrawCache>, f)` per thread, `with_pose_cache`
    kept as a fresh-cache wrapper) replaces the thread-local pose map;
    `Document::posed_shapes` -> `Posed` (borrowed or shared Arc). Render
    3.8 -> 3.1 ms half size, frames byte-identical. Stage texture upload
    now `bytemuck::cast_slice` into `ColorImage::new` (new direct dep,
    already in the lock): editor plain 7.7 -> 7.2 ms. Tried painting onion
    ghosts through a tiny-skia `Mask` + fill_rect: slower (21 -> 25.5 ms,
    no zero-skipping), reverted. Onion (~21 ms) is now mostly the three
    renders a step needs; thumbnails (~10%) are already budgeted.
134. Translation groundwork: app `i18n.rs` (gettext-style: English is the
    key; `tr(&'static str) -> &'static str`, `tr_args(text, &[(name,
    value)])` for `{name}`s, `tr_later` const marker for tables; language
    chosen once in main from INKFRAME_LANGUAGE or LANG; `BUILT_IN` list of
    include_str'd `locales/<code>.txt`, none yet; `pseudo` language).
    Converted: menu bar (all of `menu_bar`), tour.rs, whats_new.rs window.
    `cargo xtask strings` writes `locales/template.txt` (163 texts); xtask
    tests: template up to date (CRLF-tolerant), translations not stale.
    docs/TRANSLATING.md lists what's wrapped and what's left (toolbar,
    Properties, timeline, context bar, finder, dialogs, panels, names from
    enums). Keep wrapping new UI text in tr and rerun `cargo xtask strings`.
135. Translation round 2: tools (`Tool::name/description` marked
    tr_later, `label()`/`tip()` for display), toolbar, context bar,
    timeline, layout areas (`Kind::name` marked, `label()`; GROUPS
    marked), envelope lattice names. Area menu width now fits the widest
    label (pseudo showed it overflowing). Template: 334 texts. Converted
    mostly with a regex helper (widget calls; skips test modules), then by
    hand for format!s (tr_args) and tables. Next: Properties (inspector.rs,
    ~370 text lines), Find a Command, dialogs, panels.
136. Translation rounds 3 and 4: Properties (inspector.rs; section
    titles; `SECTION_WORDS` marked tr_later and searched in English and
    translated), Find a Command (entries' label/place/also marked
    tr_later; scored on translated and English words), app messages and
    file dialogs, panels, Library, lip sync, export, batch, scripts,
    presets, templates, vectorise, swap, Node View, action timeline,
    depth view, editing errors (skeleton, envelope, inside, shape ops),
    autosave ages, example notes. Plurals are separate whole texts (one /
    {count}), never pieced together. Template: 1113 texts. Left: names
    from enums (workspaces, adjustments, particle presets, perspective
    kinds, easings, blend modes, filters, texture kinds, safe areas),
    default layer names, undo step names, and the screenshot hooks and
    CLI (English on purpose). Helpers in the scratchpad: wrap.py
    (widget calls, continued literals) and wrap_list.py (given texts).
137. Translation round 5 and View › Language. Core/export names marked
    with `inkframe_core::i18n::tr_later` (blend, easing, filters, colour
    effects, adjustments, particle presets, playback, tone range, texture,
    perspective, text align, properties, export formats), app shows them
    via `tr(x.label())`; workspaces and safe areas likewise. The core's
    `i18n::set_translator` takes the app's `tr` at start-up, so
    `history::describe` (undo names, now whole phrases with {layer}) and
    new folder names translate. New layer names (Layer, Text, Paint, Oval,
    Rectangle, Path, Stroke, Line, Skeleton, Switch, "{layer} part")
    translate. `cargo xtask strings` now scans core and export too (1192
    texts). View › Language: radio list (Same as the System, English,
    built-ins named by their file's "# Name" line, pseudo), saved as
    `Prefs::language`, used from the next start (main loads prefs before
    `choose_language`; screenshot runs ignore it). Not clicked through in
    a running app: there's no app-level test harness.
## Not yet verified by hand

- Hearing a sound from the Library's preview (`AudioEngine::preview`)
  has only been exercised without an output device.

- The hold-to-reorder delay (0.35 s) and drag-to-scroll were tuned in
  tests, not with a real pen; adjust `HOLD_TO_REORDER` and
  `SCROLL_THRESHOLD` in timeline.rs if they feel wrong.

- Pen pressure has never been tried with a real tablet, on either
  platform. Tests cover the Windows path (touch events with force) with
  simulated input; the macOS event monitor is only known to compile and
  to leave mouse input working.

- Audio playback was checked by ear on 2026-10-14 and works.
- Both installers bundle a static ffmpeg 9.0.2 (macOS: martin-riedl.de,
  Windows: gyan.dev). CI downloads them with pinned SHA-256s, bundles them
  and runs the export tests through the bundled copies. Locally the macOS
  one lives in the git-ignored `vendor/ffmpeg/`.
- Pointer interactions are tested with simulated input (`egui_kittest`) and
  screenshots, not by clicking through the real app.

## Next up

Everything on the original "match Adobe Animate" list is built. The user
hasn't yet made a real animation end to end with it; suggest that before
more features, since real use will show what matters. Open items:

1. **Packaging, part B:** sign with a Developer ID (hardened runtime) and
   notarize, replacing `codesign` in `xtask/src/macos.rs`. Needs the user's
   Apple Developer account. Windows code signing needs a certificate.
2. **GPU stage preview** for very large scenes. Export should stay on the
   deterministic CPU renderer. The CPU renderer was sped up first
   (2026-10-14): 2,000 layers at 2880×1620 went from 216 ms to 28 ms a
   frame, and translucent scenes up to 100× faster. Measure with the
   `stage_benchmark` example before deciding the GPU path is still needed.
   The user chose to skip the GPU preview for now; revisit only if a real
   project feels slow.
4. **Verify by hand:** the Windows installer has never been run on a real
   PC; copy/paste between two windows; on-stage typing feel.

Smaller ideas noted along the way:

- With several layers selected, the properties panel edits only the
  last-picked layer. Groups scale and rotate but can't be skewed together.
- ⌘Z while typing on the stage does nothing; the whole typing session
  undoes as one step afterwards. Input methods have only been tested with
  simulated events, not with a real Chinese/Japanese/Korean keyboard.
- Audio decoding is whole-file in memory (~23 MB per minute). Fine for
  dialogue and music, but long files might want streaming.

## Working conventions

- The user directs and tests, and reads the code. Code should read like a
  careful human wrote it, with comments explaining *why* (the "meat"), not
  restating the code.
- Prefer one branch per feature, keep `fmt`, `clippy -D warnings` and tests
  green, update README (features, usage table, roadmap) and ARCHITECTURE.md
  with each feature, and add tests for both logic and interaction.
- macOS screen capture is blocked from the agent's shell. Use the built-in
  hook instead:
  `INKFRAME_SCREENSHOT=out.png` plus optional `_FRAME`, `_SELECT`, `_ONION`,
  `_TOOL`, `_EXPAND`, `_EDIT` (symbol on its own), `_OPEN` (instance in place),
  `_TYPE` (start typing into a text layer), `_GRAPH` (timeline in graph
  mode), `_GROUP` (all layers into a symbol; `=open` to step inside); `_SELECT` takes a comma-separated list. Launching it from a background
  shell sometimes never draws a frame; retry, it's not the app.
  It waits for audio to finish decoding before capturing.
- egui's default font lacks some glyphs (▸, ◆, ∅): check screenshots for
  empty boxes after adding symbols to UI text.
- `egui_kittest` defaults to a 0.25 s step; stage tests use 60 fps
  (`with_step_dt`) so timing-based input like double-clicks works. Hold
  modifiers in tests with `harness.event(Event::ModifiersChanged(..))`, and
  wait ~30 steps between clicks that mustn't count as a double-click.
- Use `Document::pose` (not `Layer::transform_at`) wherever layers are
  drawn, hit-tested or edited, and `Editor::selected_ids` for anything that
  should act on every selected layer.
- The user's workflow each round: build on a branch, show a screenshot,
  then "commit and merge, then start X". Push to GitHub after merging and
  check CI (`gh run watch`).
