# Architecture

This guide is for people who want to change Inkframe. It explains how the
code is organized and why, so you know where a change belongs before you
start.

## The five crates

```
                 ┌──────────────────┐
                 │  inkframe-app    │  the editor window (egui / eframe)
                 └──┬──────┬─────┬──┘
                    │      │     │
                    │      ▼     │
                    │  ┌──────────────────┐
                    │  │ inkframe-export  │  PNG sequences, ffmpeg video
                    │  └──┬────────────┬──┘
                    ▼     ▼            ▼
     ┌──────────────────┐        ┌──────────────────┐
     │ inkframe-render  │        │  inkframe-audio  │  decode, resample, mix
     │ document → pixels│        └────────┬─────────┘
     └────────┬─────────┘                 │
              ▼                           ▼
            ┌───────────────────────────────┐
            │         inkframe-core         │  the document model
            └───────────────────────────────┘
```

(The app also uses `inkframe-audio` directly, for playback.)

Dependencies point one way only, downward. In particular, **`inkframe-core`
knows nothing about rendering or UI**. That keeps the model easy to test
and lets other front ends (a command-line renderer, a plugin host) reuse it.

### inkframe-core

The data model and the math behind animation.

| File | What's in it |
|---|---|
| `document.rs` | `Document` (stage settings plus layers), `Layer`, `Element`, loading and saving |
| `channel.rs` | `Channel<T>`, one property's sorted keys, and `sample()`, which tweens between them; `AnyChannel` for type-independent editing; `Bend` and `MotionCurve`, which curve position tweens and travel them at an even speed |
| `track.rs` | `Track`: a layer's four channels, auto-keying (`set_pose`), and loading older file formats |
| `transform.rs` | `Transform`, the animatable properties: position, rotation, scale, opacity |
| `easing.rs` | Easing curves, including a CSS-style cubic-Bézier solver |
| `shape.rs` | Rectangles, ellipses, Bézier paths, images, styles, and hit-testing |
| `freehand.rs` | Turns jittery pointer samples into a smooth path |
| `anchor.rs` | Anchor points with in/out handles; conversion between anchors, paths and shapes; nearest-point search and curve-preserving point insertion |
| `symbol.rs` | `Symbol` (library entry), `Instance` (a symbol placed on a layer) and its timing, Convert to Symbol, and symbol-aware bounds and hit-testing |
| `text.rs` | `Text`: content, `Font`, size, alignment; its measured extent, which the editor keeps up to date |
| `guide.rs` | Motion guides: `Document::pose`, a layer's transform following its guide (use it rather than `Layer::transform_at` wherever layers are drawn or edited) |
| `mask.rs` | Masks: which mask a layer is seen through, and Animate's "mask the layer below" |
| `font_file.rs` | `FontFile`: a font file linked to the project, and the families it provides |
| `image.rs` | `ImageAsset`: an image file the document links to, and its size in pixels |
| `audio.rs` | `AudioTrack`: a sound file laid against the timeline; how linked files are found |
| `clipboard.rs` | Copying layers (with the symbols, images and fonts they need) and keyframes, and pasting them into the same or another project |
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
`editor.layers()` and `editor.duration()` all refer to it.

`Editor::timeline_transform` maps the edited timeline onto the stage. A
symbol opened through an instance is edited in place: the transform is the
chain of instance transforms on the way in, and `Editor::surroundings` lists
the enclosing timelines to draw faded behind it (each skipping the instance
being edited). A symbol opened from the library is shown on its own,
mid-stage. The stage's `View` carries this transform, and every tool works
in the resulting "local" coordinates (`View::to_local`), so none of them need
to know whether, or how deeply, they're inside a symbol. `Document::can_place`
stops a symbol from being placed inside itself.

**Layer spans.** A layer appears from `start` to `end` (inclusive). An end
of `None` means "until the animation ends". Outside its span a layer isn't
drawn, exported or clickable: use `Layer::is_shown_at(frame)` rather than
checking `visible` alone. Keys outside the span are allowed; they still
shape the tween leading into the span.

**File format versions.** `FORMAT_VERSION` in `document.rs` is 11. Version 1
stored whole-transform keyframes; `TrackFormat` in `track.rs` converts those
on load, and `tests/format_migration.rs` checks an original version-1 file
still plays back identically. Version 3 added layer spans, version 4
symbols, version 5 audio tracks, version 6 images, version 7 text and
version 8 project font files, version 9 curved motion paths, version 10
motion guides and version 11 masks;
earlier files load with whole-length layers, an empty library, no audio
and no images.

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
other; the buffer covers only the pixels the layer can reach.

For speed, each frame is split into fixed-height horizontal bands that are
drawn in parallel on every CPU core, and shapes outside a band are skipped
before their outlines are built.

Text is laid out in `text.rs`: fontdb finds the font, harfrust shapes the
string into positioned glyphs (kerning, ligatures, complex scripts) and
skrifa supplies each glyph's outline, which is then filled and stroked like
any path. Layouts are cached. Fonts live in one store shared by the whole
program: installed fonts, plus project font files (`sync_project_fonts`),
which are searched first so a project's copy of a font wins. `inkframe-core` can't lay out text, so the
editor stores each text's measured extent in `Text::measured`
(`Editor::remeasure_text`) for selection and hit-testing.

A masked layer is drawn into a scratch buffer like a translucent one; the
mask layer is drawn into a second buffer of the same size, whose coverage
becomes a stencil (`mask_stencil`) cut out of the first before it's
blended down. Mask layers themselves are never drawn, except faintly on the
stage while selected (`RenderOptions::show_mask`).

Images are drawn from an `ImageLibrary` (`images.rs`), which decodes each
file once and keeps half-size copies (mipmaps) so shrunken pictures don't
shimmer. Callers pass it in `RenderOptions::images`; without it, images
show as crossed-out placeholders. `cargo run --release -p inkframe-render
--example stage_benchmark` times scenes of 10 to 2,000 layers at Retina
stage size; run it before and after renderer changes.

### inkframe-audio

Turns sound files into `Clip`s: interleaved stereo `f32` at 48 kHz, whatever
the source. `decode` uses symphonia (pure Rust: WAV, MP3, AAC, FLAC, Ogg…)
and resamples with rubato's band-limited sinc interpolation. `mix` lays
tracks against the timeline at their start frames; `mix_document` does that
for a whole document, decoding each file once. Documents only *reference*
audio files (`AudioTrack::path`), relative to the project file when
possible; `AudioTrack::resolve` finds them.

### inkframe-export

`export(doc, settings, on_progress)` renders each frame and either writes a
PNG or streams raw RGBA to an `ffmpeg` child process. If the document has
audio, it's mixed, trimmed to the exported frames and passed to ffmpeg as a
temporary WAV (AAC in MP4/HEVC, 24-bit PCM with ProRes), or written next to a
PNG sequence. The ffmpeg arguments
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
| `easing_editor.rs` | The easing curve in the properties panel, with draggable Bézier handles |
| `toolbar.rs` | Tool buttons with vector-drawn icons |
| `audio_engine.rs` | Plays the soundtrack in sync (the audio clock drives playback) and scrubs it; decodes files in the background |
| `library.rs` | The Library panel, with cached symbol thumbnails |
| `export_dialog.rs` | Export settings and the background export thread |
| `shortcuts.rs` | Every keyboard shortcut in one place |
| `motion_path.rs` | The math for showing and bending a layer's motion path on the stage |
| `text_edit.rs` | Typing into text on the stage: the text cursor, selection and keys, kept separate from drawing so it can be tested |
| `screenshot.rs` | `INKFRAME_SCREENSHOT=out.png` captures the window and quits |

egui is an *immediate-mode* UI: each panel's `show()` function runs every
frame, draws its widgets, and handles input on the spot. There are no
callbacks or retained widget trees. Panels read and change `Editor` directly.

## Build tooling

`xtask/` is a small binary for release chores, run as `cargo xtask <task>`
(the alias is in `.cargo/config.toml`). It isn't part of the app.

- `icon.rs` draws the app icon with tiny-skia at any size. `cargo xtask icon`
  writes the PNGs and the Windows `.ico` in `assets/icon/`; the app embeds
  the 256-pixel PNG as its window icon.
- `macos.rs` assembles `Inkframe.app`, optionally with a bundled ffmpeg that
  `ffmpeg::locate` finds next to the executable, signs it ad hoc and builds
  a `.dmg`. See `packaging/macos/README.md`.
- `windows.rs` stages `inkframe.exe` (plus an optional ffmpeg) and runs Inno
  Setup on `packaging/windows/inkframe.iss` to make a `setup.exe`. The
  executable's icon is embedded by `crates/inkframe-app/build.rs`.
- `util.rs` holds the steps both platforms share (release build, licences,
  bundling ffmpeg).

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
