# Handoff: where Inkframe stands

A checkpoint for picking development back up. Last updated 2026-10-19,
after the Harmony features (palettes to master controllers).

## State

- **Branch:** `main`, clean. Every feature is built on its own branch,
  then merged into `main` with a merge commit.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo test --workspace` (524 tests)
  all pass, on macOS locally and on macOS and Windows in CI. Clippy can
  also be run for Windows from the Mac:
  `cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets`.
- **File format:** version 26. Older versions load and upgrade automatically
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

## Not yet verified by hand

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
  mode); `_SELECT` takes a comma-separated list. Launching it from a background
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
