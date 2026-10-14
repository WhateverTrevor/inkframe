# Handoff: where Inkframe stands

A checkpoint for picking development back up. Last updated 2026-10-14,
after the macOS app bundle.

## State

- **Branch:** `main`, clean. Every feature was built on its own branch,
  then fast-forward merged and the branch deleted.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo test --workspace` (183 tests)
  all pass.
- **File format:** version 8. Older versions load and upgrade automatically
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

## Not yet verified by hand

- Audio playback was checked by ear on 2026-10-14 and works.
- Both installers bundle a static ffmpeg 9.0.2 (macOS: martin-riedl.de,
  Windows: gyan.dev). CI downloads them with pinned SHA-256s, bundles them
  and runs the export tests through the bundled copies. Locally the macOS
  one lives in the git-ignored `vendor/ffmpeg/`.
- Pointer interactions are tested with simulated input (`egui_kittest`) and
  screenshots, not by clicking through the real app.

## Next up (roadmap order)

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

Smaller ideas noted along the way:

- Text is edited in the properties panel, not on the stage. On-stage
  editing (a caret drawn in the transformed text) would be the natural
  next step. Characters missing from the chosen font (emoji, CJK in a
  Latin font) show as the font's empty box; there's no per-character
  font fallback yet.
- Audio decoding is whole-file in memory (~23 MB per minute). Fine for
  dialogue and music, but long files might want streaming.
- A value graph (Animate's Motion Editor: each property's value plotted
  over the whole timeline, keys draggable up and down) would complement the
  per-tween easing curve editor.

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
  `_TOOL`, `_EXPAND`, `_EDIT` (symbol on its own), `_OPEN` (instance in place).
  It waits for audio to finish decoding before capturing.
- egui's default font lacks some glyphs (▸, ◆, ∅): check screenshots for
  empty boxes after adding symbols to UI text.
- `egui_kittest` defaults to a 0.25 s step; stage tests use 60 fps
  (`with_step_dt`) so timing-based input like double-clicks works.
