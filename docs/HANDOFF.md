# Handoff: where Inkframe stands

A checkpoint for picking development back up. Last updated 2026-10-26,
after the Photoshop-style brushes and the translation work.

What each release added is in `CHANGELOG.md`, how the code is laid out in
`ARCHITECTURE.md`, and the story of each change in `git log` (every
feature is a merge commit with a description). This file holds only what
those don't: the state of things, what hasn't been checked, what's next,
and how the work is done.

## State

- **Branch:** `main`. Each feature is built on its own branch, then
  merged into `main` with a merge commit and pushed.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo test --workspace` (856 tests)
  pass, on macOS locally and on macOS and Windows in CI.
- **File format:** version 39. Older files load and upgrade automatically
  (`crates/inkframe-core/tests/format_migration.rs`).
- **Releases:** v0.2.0 to v0.5.1 are tagged. Their GitHub releases are
  drafts (with the .dmg, setup.exe and guidebook PDF) that the user
  publishes. Unreleased work is under *Unreleased* in the changelog.
- **Remote:** https://github.com/WhateverTrevor/inkframe (public). CI runs
  on macOS and Windows for every push. Commits use the GitHub noreply
  address; the git identity is set for this repository only.

## What's built

Everything on the original "match Adobe Animate" list, then Harmony's
and Moho's main features, and more:

- **Drawing:** vector tools (pen, pencil with pressure, shapes, text,
  Subselection, Distort and Envelope, combine shapes), ink-and-paint
  fills, art and pattern brushes, perspective guides, rulers and guides.
- **Painting:** paint layers with Photoshop's brush engine (tips,
  dynamics, texture, dual brush, presets, `.abr` import, Brush Settings
  and Brushes areas, a quick menu), retouching tools, selections,
  adjustments, blend modes, vectorising.
- **Animation:** keyframes with easing and a graph editor, motion
  guides, shape tweens, frame-by-frame drawings with an exposure sheet,
  onion skins, symbols with swaps, motion presets, templates.
- **Rigging:** parenting and IK, Moho-style skeletons with smart bones,
  dynamics, targets and corrections, deformers and free-form deformers,
  master controllers, lip sync (Rhubarb, Papagayo, loudness).
- **Scene:** camera with multiplane depth, effects and filters, motion
  blur, particles, node view, audio.
- **Output:** PNG sequences, sprite sheets, ffmpeg video, GIF, SVG,
  Lottie, web pages, PSD; batch export and a command line.
- **Around it:** JavaScript scripting (QuickJS) with a console, autosave
  and recovery, a first-run tour, What's New, Find a Command, workspaces,
  translation (every text wrapped; View › Language), a guidebook PDF.

## Not yet verified by hand

- **A real pen tablet,** on either platform: pressure (Windows' path is
  tested with simulated touches; the macOS monitor is only known to
  compile and leave the mouse working), tilt reaching the brush, and the
  timeline's hold-to-reorder delay (`HOLD_TO_REORDER`, `SCROLL_THRESHOLD`
  in timeline.rs).
- **Photoshop `.abr` files from the wild.** The reader is tested on files
  built in its tests; the `desc` key names and the `patt` layout are from
  memory of the format.
- **The Mixer Brush** feel (Wet, Load, Mix) is tuned by tests, not by a
  painter; `MIX_PER_DAB` and `DRY_PER_DAB` in retouch.rs set it.
- **View › Language** hasn't been clicked through in a running app (no
  app-level test harness; screenshots ignore the setting by design).
- **The Windows installer** on a real PC, copy and paste between two
  windows, on-stage typing with a real input method, and hearing the
  Library's sound preview with an output device.

## Next up

The user hasn't yet made a real animation end to end; suggest that, since
real use will show what matters. Open items:

1. **Signing:** a Developer ID (hardened runtime) and notarizing on macOS,
   replacing `codesign` in `xtask/src/macos.rs`, and a Windows
   certificate. Needs the user's accounts.
2. **A first real translation,** checked by someone fluent.
3. **A GPU stage preview** only if a real project feels slow (the CPU
   renderer is fast; measure with the `stage_benchmark` example first).
4. Smaller ideas: Properties edits only the last-picked of several
   layers; groups can't be skewed together; ⌘Z while typing on the stage
   does nothing until typing ends; audio is decoded whole into memory.

## Working conventions

- The user directs and tests, and reads the code. Code should read like a
  careful human wrote it, with comments explaining *why*, not restating
  the code.
- One branch per feature; keep `fmt`, `clippy -D warnings` and tests
  green; update the README, ARCHITECTURE.md, the changelog and the
  guidebook with each feature, and add tests for logic and interaction.
- The user's rhythm: build on a branch, show a screenshot, wait for
  "commit and merge", then push and check CI (`gh run watch`). Give the
  rebuilt app's path at each check-in: `target/release/Inkframe.app`, from
  `cargo xtask bundle-macos --ffmpeg vendor/ffmpeg/ffmpeg`.
- **Screenshots:** macOS screen capture is blocked from the agent's
  shell, so the app takes its own: `INKFRAME_SCREENSHOT=out.png` plus the
  hooks listed at the top of `crates/inkframe-app/src/screenshot.rs`.
  Run it in the foreground (macOS has no `timeout` command; a background
  run can stall without drawing a frame).
- **Text people see** is wrapped for translation (`tr`, `tr_args`,
  `tr_later`; see `docs/TRANSLATING.md`), and `cargo xtask strings` is
  rerun after adding any; a test fails otherwise. Plurals are whole texts
  ("1 layer", "{count} layers"), never pieced together.
- **Windows from a Mac:** `cargo clippy --target x86_64-pc-windows-msvc`
  can't build QuickJS (C), so leave out `inkframe-script` and the app
  locally and rely on CI.
- egui's default font lacks some glyphs (▸, ◆, ∅): check screenshots for
  empty boxes after adding symbols to text.
- `egui_kittest` defaults to a 0.25 s step; stage tests use 60 fps
  (`with_step_dt`) so double-clicks work. Hold modifiers with
  `harness.event(Event::ModifiersChanged(..))`, and wait ~30 steps
  between clicks that mustn't count as a double-click.
- Use `Document::pose` (not `Layer::transform_at`) wherever layers are
  drawn, hit-tested or edited, `Document::posed_elements` for shapes as
  shown, and `Editor::selected_ids` for anything that acts on every
  selected layer. `Editor::settled_doc` is what's saved and exported.
- After a file format change, rerun `cargo xtask samples` so the examples
  are in the current format.
