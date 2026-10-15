# Handoff: where Inkframe stands

A checkpoint for picking development back up. Last updated 2026-10-14
(end of the second long session), after the value graph.

## State

- **Branch:** `main`, clean. Every feature was built on its own branch,
  then fast-forward merged and the branch deleted.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo test --workspace` (273 tests)
  all pass, on macOS locally and on macOS and Windows in CI. Clippy can
  also be run for Windows from the Mac:
  `cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets`.
- **File format:** version 12. Older versions load and upgrade automatically
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
    while painting. Planned next, as agreed with the user: phase 2 (bucket,
    gradient, smudge, blur/sharpen, dodge/burn, clone stamp, brush
    presets), phase 3 (marquee, lasso, magic wand, adjustments, blend
    modes), phase 4 maybe (canvas rotation, PSD).

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
