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
| `channel.rs` | `Channel<T>`, one property's sorted keys, and `sample()`, which tweens between them; `AnyChannel` for type-independent editing |
| `track.rs` | `Track`: a layer's four channels, auto-keying (`set_pose`), and loading older file formats |
| `transform.rs` | `Transform`, the animatable properties: position, rotation, scale, opacity |
| `easing.rs` | Easing curves, including a CSS-style cubic-Bézier solver |
| `shape.rs` | Rectangles, ellipses, Bézier paths, styles, and hit-testing |
| `freehand.rs` | Turns jittery pointer samples into a smooth path |
| `anchor.rs` | Anchor points with in/out handles; conversion between anchors, paths and shapes; nearest-point search and curve-preserving point insertion |
| `symbol.rs` | `Symbol` (library entry), `Instance` (a symbol placed on a layer) and its timing, Convert to Symbol, and symbol-aware bounds and hit-testing |
| `history.rs` | Snapshot-based undo/redo |
| `geometry.rs` | `Point`, `Rect`, `Affine` |

**The animation model.** A layer is a group of shapes that animate together,
like a tween layer holding one symbol in Animate. Each layer has a `Track`
made of one `Channel` per property (position, rotation, scale, opacity). A
key stores one value plus the easing used to travel to that channel's *next*
key, so every property has independent timing. `Track::sample` combines the
channels into a `Transform` for rendering. `Track::set_pose` is auto-keying:
it only adds keys for properties whose value changed. Shapes are defined
around the layer's local origin, which is also the pivot for rotation and
scaling.

**Symbols.** The document has a library of `Symbol`s, each with its own
layers and length. A layer whose `instance` is set draws that symbol through
the layer's transform (instance layers normally have no elements). Timing
follows Animate's *graphic* symbols: `Instance::local_frame` derives the
symbol's frame from the main timeline, so scrubbing and export are exact.
Rendering, bounds and hit-testing recurse into symbols; use
`Document::layer_bounds` and `Document::layer_hit` rather than looking at a
layer's elements alone. Layer ids are unique across the whole document,
symbols included. Loading rejects a symbol that contains itself, and
`MAX_SYMBOL_DEPTH` is a backstop against runaway nesting.

**Editing inside symbols.** Every timeline, the main one or a symbol's, is
named by a `TimelineId`. `Document` operations that add layers or hit-test
take one; operations on an existing layer (`layer_mut`, `remove_layer`,
`move_layer`…) find its timeline by id. The editor keeps a stack of open
symbols: `Editor::timeline()` is the one being edited, and `editor.frame`,
`editor.layers()` and `editor.duration()` all refer to it. On the stage, a
symbol's registration point sits mid-stage (`Editor::timeline_origin`); the
stage's `View::offset_by` shifts coordinates so every tool works in the
symbol's own space without knowing it's inside one. `Document::can_place`
stops a symbol from being placed inside itself.

**Layer spans.** A layer appears from `start` to `end` (inclusive). An end
of `None` means "until the animation ends". Outside its span a layer isn't
drawn, exported or clickable: use `Layer::is_shown_at(frame)` rather than
checking `visible` alone. Keys outside the span are allowed; they still
shape the tween leading into the span.

**File format versions.** `FORMAT_VERSION` in `document.rs` is 4. Version 1
stored whole-transform keyframes; `TrackFormat` in `track.rs` converts those
on load, and `tests/format_migration.rs` checks an original version-1 file
still plays back identically. Version 3 added layer spans and version 4
added symbols; earlier files load with whole-length layers and an empty
library.

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
| `pen.rs` | State of the pen path being drawn, which lasts across many clicks |
| `subselect.rs` | The math for moving, deleting and converting anchors with the Subselection tool |
| `timeline.rs` | Transport bar and the hand-painted layer × frame grid |
| `inspector.rs` | The properties panel |
| `toolbar.rs` | Tool buttons with vector-drawn icons |
| `library.rs` | The Library panel, with cached symbol thumbnails |
| `export_dialog.rs` | Export settings and the background export thread |
| `shortcuts.rs` | Every keyboard shortcut in one place |
| `screenshot.rs` | `INKFRAME_SCREENSHOT=out.png` captures the window and quits |

egui is an *immediate-mode* UI: each panel's `show()` function runs every
frame, draws its widgets, and handles input on the spot. There are no
callbacks or retained widget trees. Panels read and change `Editor` directly.

## Common changes

**Add an animatable property** (for example, skew): add the field to
`Transform` and apply it in `Transform::to_affine`; add a `Property` variant
and a `Channel` field to `Track`, and wire it into `Track::new`, `sample`,
`set_pose`, `channel` and `channel_mut`; then add a row in
`inspector::transform_section`. Files saved before the property existed
won't have its channel, so give it a default in `TrackFormat::Current`
(a single key holding the identity value).

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
