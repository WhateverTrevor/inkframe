# Architecture

This guide is for people who want to change Inkframe. It explains how the
code is organized and why, so you know where a change belongs before you
start.

## The four crates

```
            ┌──────────────────┐
            │  inkframe-app    │  the editor window (egui / eframe)
            └──┬─────┬──────┬──┘
               │     │      │
               │     ▼      ▼
               │  ┌───────────────────┐
               │  │ inkframe-export   │  PNG sequences, ffmpeg video
               │  └──┬────────────────┘
               ▼     ▼
            ┌──────────────────┐
            │ inkframe-render  │  document → pixels (tiny-skia)
            └────────┬─────────┘
                     ▼
            ┌──────────────────┐
            │  inkframe-core   │  the document model, no dependencies on the above
            └──────────────────┘
```

Dependencies point one way only, downward. In particular, **`inkframe-core`
knows nothing about rendering or UI**. That keeps the model easy to test
and lets other front ends (a command-line renderer, a plugin host) reuse it.

### inkframe-core

The data model and the math behind animation.

| File | What's in it |
|---|---|
| `document.rs` | `Document` (stage settings plus layers), `Layer`, `Element`, loading and saving |
| `track.rs` | `Track`, a layer's sorted list of `Keyframe`s, and `sample()`, which tweens between them |
| `transform.rs` | `Transform`, the animatable properties: position, rotation, scale, opacity |
| `easing.rs` | Easing curves, including a CSS-style cubic-Bézier solver |
| `shape.rs` | Rectangles, ellipses, Bézier paths, styles, and hit-testing |
| `freehand.rs` | Turns jittery pointer samples into a smooth path |
| `history.rs` | Snapshot-based undo/redo |
| `geometry.rs` | `Point`, `Rect`, `Affine` |

**The animation model.** A layer is a group of shapes that animate together,
like a tween layer holding one symbol in Animate. Each layer has one `Track`.
A keyframe stores a full `Transform` plus the easing used to travel to the
*next* keyframe. Shapes are defined around the layer's local origin, which is
also the pivot for rotation and scaling.

**Coordinates.** Stage pixels, with y pointing down. Rotation is in degrees,
clockwise. Frames are 0-based internally and shown 1-based in the UI.

**Undo.** `History` stores whole-document snapshots instead of individual
commands. Documents are small, so this is cheap. More importantly, any new
feature that edits the document is undoable automatically. The editor
commits a snapshot when the user isn't mid-drag or mid-typing, so one
gesture becomes one undo step.

### inkframe-render

`render_frame_into(pixmap, doc, frame, options)` draws one frame. The stage
preview and every export use this same function, so what you see is exactly
what you get. Translucent layers render into a scratch buffer and are then
composited, so overlapping shapes inside a layer don't show through each
other.

### inkframe-export

`export(doc, settings, on_progress)` renders each frame and either writes a
PNG or streams raw RGBA to an `ffmpeg` child process. The ffmpeg arguments
for every format are in `ffmpeg::build_args`, with comments explaining each
choice (color matrix, pixel format, padding to even sizes). The progress
callback returns `ControlFlow::Break` to cancel.

### inkframe-app

| File | What's in it |
|---|---|
| `app.rs` | Window layout, menus, the `Command` enum, file open/save |
| `editor.rs` | `Editor`: document, selection, current frame, tool, and the operations panels call |
| `stage.rs` | The canvas: zoom/pan, drawing tools, selection overlay and transform handles |
| `handles.rs` | The math for scale and rotate handles (anchor pinning, multi-turn rotation) |
| `timeline.rs` | Transport bar and the hand-painted layer × frame grid |
| `inspector.rs` | The properties panel |
| `toolbar.rs` | Tool buttons with vector-drawn icons |
| `export_dialog.rs` | Export settings and the background export thread |
| `shortcuts.rs` | Every keyboard shortcut in one place |
| `screenshot.rs` | `INKFRAME_SCREENSHOT=out.png` captures the window and quits |

egui is an *immediate-mode* UI: each panel's `show()` function runs every
frame, draws its widgets, and handles input on the spot. There are no
callbacks or retained widget trees. Panels read and change `Editor` directly.

## Common changes

**Add an animatable property** (for example, skew): add the field to
`Transform`, interpolate it in `Transform::lerp`, apply it in
`Transform::to_affine`, then add a control in `inspector::transform_section`.
Old files keep loading because `Transform` uses `#[serde(default)]`.

**Add a shape type:** add a variant to `Shape` with `bounds()` and
`hit_test()`, a case in `inkframe-render/src/path.rs`, and a tool in
`stage.rs`.

**Add an export format:** add a variant to `ExportFormat` and a branch in
`ffmpeg::build_args`, then add a test in `crates/inkframe-export/tests/formats.rs`
that checks the output with ffprobe.

**Change the file format incompatibly:** bump `FORMAT_VERSION` in
`document.rs` and migrate older versions in `Document::from_json`.

## Testing

- Core logic has unit tests next to the code (`cargo test -p inkframe-core`).
- Export tests encode real files and inspect them with ffprobe. They skip
  themselves if ffmpeg isn't installed.
- Stage tools are tested with simulated mouse drags through `egui_kittest`
  (see the bottom of `stage.rs`).
