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
                    │  │ inkframe-export  │  PNG sequences, sprite sheets, ffmpeg video
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
| `camera.rs` | The camera: a layer with `camera` set, kept on top of the main timeline (`keep_camera_on_top`), whose pose is the view (`camera_matrix`). Top-level main-timeline layers are placed through it in `parent_matrix`, so everything that draws or edits sees the camera's view. A multiplane: each layer's Depth property (`Track::depth`, animatable) against the camera's own Depth (its dolly) gives perspective `F / (F + d - z)` (`FOCAL_LENGTH`), and `camera_fade` fades layers the camera reaches. `stacking_order` is the order layers are drawn and clicked in: the timeline's, or with `stack_by_depth` far to near (a stable sort on the depth of each layer's top parent) |
| `channel.rs` | `Channel<T>`, one property's sorted keys, and `sample()`, which tweens between them; `AnyChannel` for type-independent editing; `Bend` and `MotionCurve`, which curve position tweens and travel them at an even speed |
| `track.rs` | `Track`: a layer's four channels, auto-keying (`set_pose`), and loading older file formats |
| `transform.rs` | `Transform`, the animatable properties: position, rotation, scale, skew, opacity; `from_affine` turns any matrix back into a pose |
| `easing.rs` | Easing curves, including a CSS-style cubic-Bézier solver |
| `shape.rs` | Rectangles, ellipses, Bézier paths, images, styles, and hit-testing |
| `freehand.rs` | Turns jittery pointer samples into a smooth path |
| `anchor.rs` | Anchor points with in/out handles; conversion between anchors, paths and shapes; nearest-point search and curve-preserving point insertion |
| `symbol.rs` | `Symbol` (library entry), `Instance` (a symbol placed on a layer) and its timing, Convert to Symbol, and symbol-aware bounds and hit-testing |
| `template.rs` | Templates: copied layers (with their symbols, pictures, fonts and canvases) saved as a file of their own with the frame they started on; `insert_template` moves their animation to the playhead (`Layer::shift_time`) and pastes them |
| `text.rs` | `Text`: content, `Font`, size, alignment; its measured extent, which the editor keeps up to date |
| `guide.rs` | Motion guides: `Document::pose`, a layer's transform following its guide (use it rather than `Layer::transform_at` wherever layers are drawn or edited) |
| `rig.rs` | Parenting and pivots: `Document::layer_matrix` (parents · pose · −pivot), the one map from a layer's drawing to its timeline that drawing, hit testing and editing all use; `set_parent` and `set_pivot`, which keep the layer where it is on screen |
| `ik.rs` | Inverse kinematics for the Bone tool: `Document::ik_chain` (a layer and its parents below the root) and `Document::reach`, a cyclic-coordinate-descent solver that keys only rotations and honours `Layer::rotation_limits` |
| `folder.rs` | Layer folders (a `Layer` with `folder` set; members name it in `in_folder` and always sit directly below it, restored by `tidy_folders`), `add_folder`, `place_layer` (a timeline drop), `move_among_siblings`, `folder_block`; layer colours (`Layer::tag_color`, `TAG_COLORS`) |
| `morph.rs` | Shape tweens: `ShapeTween` (keyed by the drawing swap it morphs into), `Layer::morph_at` (used by `shown_elements`, so drawing, bounds and hit testing all see the morph), and `morph`: shapes to anchors, segments split until the counts match (at least 16 on a closed shape), lined up by the best rotation and direction or by shape hints (`Drawing::hints`), then blended with colours and stroke widths |
| `palette.rs` | The colour palette: swatches that own their exact colour (every fill and stroke of it is theirs, so `set_swatch_color` recolours them all), in versions (day, night) switched with `use_palette_version`, which maps every swatch at once. `Palette` keeps swatch colours distinct within a version |
| `warp.rs` | The free-form deformer: a grid of animated control-point offsets over a drawing's rest bounds; each point of the drawing moves by a Bézier-patch blend (Bernstein weights) of them, applied after the bone deformer in `shown_elements` |
| `particles.rs` | Particle layers: `Emitter` (rate, emitter shape, direction and spread, speed, gravity, wind, drag, wander, lifetime, size/opacity/colour over life, spin, sprite) and `Preset`s. No simulation state: particle *i* is born at about *i*/rate and its whole life follows from the seed by formula (closed-form drag, a sine wobble), so any frame stands alone. Births are placed with `placement_without_camera` at the birth time, so particles stay in the scene when the emitter moves; `Document::particles_on_layer` maps them back onto the layer for drawing, and `particle_bounds` is the layer's bounds |
| `vectorise.rs` | Tracing pixels into shapes (`trace`, `Document::vectorise_layer`): labels pixels (ink by brightness, or colours found as histogram peaks and settled on nearby pixels only, so anti-aliasing doesn't drag them), merges patches under the minimum area into their surroundings, traces each patch's outline and holes along pixel edges (turning right at diagonal touches, so loops never cross), fits curves through the midpoints of the pixel stairs keeping long-run corners sharp, and joins holes to outlines by zero-width slits (holes run the opposite way, and fills are by winding) |
| `vector_fill.rs` | Ink and paint: a drawing's line art (`elements`) and colour art (`colour_art`, drawn beneath); `Layer::fill_enclosed` fills the area the lines enclose (grid flood with lines thickened to close gaps, grown back under them, outline traced and simplified) or recolours the fill already there |
| `paint.rs` | Painted pixels: `Canvas` (256×256 premultiplied RGBA tiles, shared between undo snapshots with `Arc`), `Shape::Paint` naming a canvas, `Document::element_bounds`/`element_hit` (paint counts only painted pixels), `add_paint_layer`, `separate_canvases` (a duplicated layer or drawing gets its own canvas) |
| `brush.rs` | The brush engine (and `BrushSettings::presets`, the paper `grain`, and `DabPath`, which every stroke tool uses to place its dabs): a `BrushStroke` lays round dabs along the pen's path into its own coverage mask, then redraws the touched tiles from how they were before the stroke, so opacity caps a whole stroke and flow builds up within it, as in Photoshop |
| `selection.rs` | Pixel selections: a soft 0–255 mask per canvas pixel (`Selection`), from rectangles, ellipses and polygons (anti-aliased by 4×4 subsampling) or the magic wand; combining (`Combine`); `limit`/`limit_canvas` keep a change only where selected; `keep`/`clear`/`shifted` for moving and cutting; `outline` for marching ants |
| `adjust.rs` | Brightness/Contrast, Hue/Saturation, Levels and Curves (a monotone cubic), returning a new canvas so the app can preview from the original |
| `psd.rs` | Photoshop files: reading and writing 8-bit RGB layered images (PackBits or raw), and `Document::import_psd`; `inkframe_render::export_psd` draws each layer for writing |
| `blend.rs` | `BlendMode`, every layer's Photoshop-style blend mode; the renderer composites a non-Normal layer through a scratch buffer |
| `retouch.rs` | Smudge, Blur, Sharpen, Dodge, Burn and Clone Stamp: a `RetouchStroke` places dabs with the brush's `DabPath`, and each dab reads the square of pixels under it (`Canvas::read_region`), reworks them, and writes them back |
| `effects.rs` | Colour effects (`ColorEffect`: brightness, tint, advanced) and filters (`Filter`: drop shadow, blur, glow, adjust colour, motion blur; motion blur is drawn by the renderer, which averages the layer drawn at moments across the shutter using `Document::layer_matrix_at`), as pixel operations on premultiplied RGBA. `Effects` (both together) is the value of the track's sixth animated property, `Property::Effects`, blended setting by setting (`Animatable` asks only for `Clone`, so values like this can animate); blurs are three box passes standing in for a Gaussian. The renderer runs a layer with effects through its scratch buffer, widened by `filter_margin` (past the band being drawn, so bands meet seamlessly) |
| `fill.rs` | The Paint Bucket (`bucket_fill`, a scanline flood fill with a tolerance, or every matching pixel) and `gradient` (linear or radial, blended as straight colours) |
| `container.rs` | The project file: a zip of `document.json` and each canvas tile as `canvases/<id>/<column>_<row>.png`. Plain-JSON files from before version 17 still open |
| `controller.rs` | Master controllers: a slider over a set of layers with poses (transforms and drawings) recorded along it; `set_controller` blends the two poses either side and keys the layers on the frame, so what it poses animates like any edit |
| `deform.rs` | Bone deformers: a `Deformer`'s rest joints and animated bends, bending outlines by blending each point between nearby bones (subdividing segments first). `Layer::shown_elements(frame)` is the drawing as seen (drawing swapped, then bent): use it for drawing, hit testing and bounds. Also `Layer::key_frames`/`insert_key`/`remove_key`/`move_key`, covering the transform and the deformer together |
| `drawing.rs` | Drawing substitution: a layer's named `Drawing`s and its `Exposure` keys (which drawing shows from which frame). Use `Layer::elements_at(frame)` for what a layer shows, `all_elements` for "is this asset used anywhere" |
| `lip_sync.rs` | Lip sync: the nine standard `MouthShape`s, reading Rhubarb (`.tsv`) and Papagayo (`.dat`) timing, mouth shapes from loudness, guessing which drawing is which shape (`MouthMap`), and `Layer::apply_lip_sync`. No audio code: the app passes in per-frame loudness |
| `line.rs` | Pressure-sensitive lines (`Shape::Line`): a centreline with a width per anchor, eased between anchors and drawn as the filled outline it sweeps out with round ends (`line_outline`). `to_anchors` gives that outline, so deformers, morphs, fills and hit tests treat a line as its shape; the Width tool edits the widths |
| `mask.rs` | Masks: which mask a layer is seen through, and Animate's "mask the layer below" |
| `font_file.rs` | `FontFile`: a font file linked to the project, and the families it provides |
| `image.rs` | `ImageAsset`: an image file the document links to, and its size in pixels |
| `audio.rs` | `AudioTrack`: a sound file laid against the timeline; how linked files are found |
| `clipboard.rs` | Copying layers (with the symbols, images and fonts they need) and keyframes, and pasting them into the same or another project |
| `history.rs` | Snapshot-based undo/redo; each step is named by comparing it with the one before (`describe`), for the History panel, which can `jump` several steps |
| `align.rs` | The Align panel's work: `stage_bounds`, `align_layers`, `distribute_layers`, `match_size`, moving layers in their parents' space |
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

**File format versions.** `FORMAT_VERSION` in `document.rs` is 30. Version 1
stored whole-transform keyframes; `TrackFormat` in `track.rs` converts those
on load, and `tests/format_migration.rs` checks an original version-1 file
still plays back identically. Version 3 added layer spans, version 4
symbols, version 5 audio tracks, version 6 images, version 7 text and
version 8 project font files, version 9 curved motion paths, version 10
motion guides, version 11 masks, version 12 skew, version 13 parents
and pivots, version 14 drawings (a layer's `elements` became its first
drawing; `drawing::drawings_or_elements` reads either), version 15
joint rotation limits, version 16 deformers, version 17 paint layers,
when project files became zips (see `container.rs`), version 18
layer blend modes, version 19 layer folders and colours, and version 20
shape tweens and shape hints, version 21 the camera and layer depth, version 22 colour effects
and filters, and version 23 animated effects (format 22's fixed ones
move into the Effects property on load), version 24 colour art in
drawings, version 25 the free-form deformer, version 26 master
controllers, version 27 pressure-sensitive pencil lines, and version 28
particle layers (`Layer::particles`), and version 29 animatable depth (the
track's `depth`; older fixed depths move into it on load), and version 30 stacking by depth
(`Document::stack_by_depth`);
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
any path. Glyphs with colors of their own (emoji, from the bundled Noto
Color Emoji in the vector COLR format) are painted by `color_glyph.rs`,
which carries out the font's layers, gradients and clips with tiny-skia.
Characters the chosen font lacks are looked up in other fonts
(`Fonts::fallback_for`, cached per character) and shaped as separate runs
on the same line. Layouts are cached. Fonts live in one store shared by the whole
program: installed fonts, plus project font files (`sync_project_fonts`),
which are searched first so a project's copy of a font wins. `inkframe-core` can't lay out text, so the
editor stores each text's measured extent in `Text::measured`
(`Editor::remeasure_text`) for selection and hit-testing.

A masked layer is drawn into a scratch buffer like a translucent one; the
mask layer is drawn into a second buffer of the same size, whose coverage
becomes a stencil (`mask_stencil`) cut out of the first before it's
blended down. Mask layers themselves are never drawn, except faintly on the
stage while selected (`RenderOptions::show_mask`).

A particle layer draws its particles instead of its drawing
(`particles.rs`): dots as circles with a radial-gradient edge, or, when the
layer's drawing is the particle, that drawing rasterised once at the
largest particle's size and stamped at each particle with its opacity.
Since this happens in `draw_layer`, opacity, blend modes, masks and
filters apply to the particles as a whole. Motion blur blurs only the
emitter's own movement, not each particle's.

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

Batches (`batch.rs`) are lists of `BatchJob`s, each opening its project
from disk and exporting the whole animation. `batch::outputs` names files
after projects, adding the format's slug where two would clash and giving
PNG sequences their own folder; `batch::run` keeps going past a failed
job and stops everything when its callback breaks.

Sprite sheets (`sprite_sheet.rs`) render every frame, crop each to its
drawn pixels, and keep one copy of frames that come out identical (held
drawings). Every cell is the union of those areas, so frames stay aligned;
`Grid::fit` lays the cells out near-square within 8192 px with 2 px
padding. The JSON beside the PNG is TexturePacker's "JSON hash"
(`frames` with `trimmed`/`spriteSourceSize`/`sourceSize` recording the
crop, `animations`, `meta`), plus a per-frame `duration` as Aseprite
writes. No audio.

### inkframe-app

| File | What's in it |
|---|---|
| `autosave.rs` | Crash recovery: `Autosave::tick` copies the project every 30 s while it has unsaved changes (cloned, then zipped and written on a thread, to a temporary name renamed into place) into the settings folder's `Recovery/`, with a JSON note (original path, name, saved time, heartbeat). Saving or a normal quit `clear`s it. `left_behind` finds other sessions' copies whose heartbeat has been quiet for 2 minutes, so a second Inkframe that's still running isn't mistaken for a crash; `Recovery::open` reopens one as its project with changes unsaved (`Editor::recovered`). The window offering them is `App::recovery_window` |
| `app.rs` | Window layout, menus, the `Command` enum, file open/save |
| `editor.rs` | `Editor`: document, selection, current frame, tool, and the operations panels call |
| `stage.rs` | The canvas: zoom/pan, drawing tools, selection overlay and transform handles |
| `handles.rs` | The math for scale and rotate handles (anchor pinning, multi-turn rotation) |
| `depth_view.rs` | The Depth View: the main timeline's top-level layers from above, as lines at their depths, with the camera's field of view. Dragging a line sets its depth, dragging the camera dollies and pans it (both through `Editor::set_depth` / `set_pose`, so they key the frame); scroll zooms, empty space pans, Fit fits. What's dragged is decided at the press point |
| `node_view.rs` | The Node View: the timeline being edited drawn as a graph built from the layers each frame (layer nodes with Parent and Mask inputs and Transform and Image outputs, effect and filter nodes in each layer's image chain, a Composite in stacking order, the Display). `connect` and `disconnect` turn wiring into the same edits the timeline and Properties make (`set_parent`, `masked_by`, `place_layer`, effects); only node positions are the view's own |
| `pen.rs` | State of the pen path being drawn, which lasts across many clicks |
| `subselect.rs` | The math for moving, deleting and converting anchors with the Subselection tool |
| `timeline.rs` | Transport bar and the hand-painted layer × frame grid |
| `inspector.rs` | The properties panel, with Tool, Layer and Document tabs that follow the selection and tool |
| `panels.rs` | The Swatches, Align and History panels |
| `value_graph.rs` | The timeline's Graph mode: a property's values over time, with draggable keys and easing handles |
| `easing_editor.rs` | The easing curve in the properties panel, with draggable Bézier handles |
| `toolbar.rs` | Tool buttons with vector-drawn icons |
| `audio_engine.rs` | Plays the soundtrack in sync (the audio clock drives playback) and scrubs it; decodes files in the background |
| `library.rs` | The Library panel, with cached symbol thumbnails |
| `examples.rs` | File › Open Example: finds the sample projects (bundled in `Contents/Resources/Examples` or an `Examples` folder beside the exe, else the checkout's `examples/`; only a folder holding `.inkf` files counts), lists them with a rendered picture and a note from `ABOUT`, and opens one as an untitled copy (`Editor::untitled_copy`) so saving never writes into the app |
| `export_dialog.rs` | Export settings and the background export thread |
| `guides.rs` | Safe areas (`SafeAreas`: current 93/90%, classic 90/80%) and the 12-field chart, as lines and labels in stage pixels; `stage.rs` draws them over the stage (the camera's frame), turned with the view |
| `batch_dialog.rs` | File › Batch Export: a queue of `BatchJob`s (project, format, size, transparency) run on a thread with `batch::run`, status per row |
| `cli.rs` | `inkframe --export …`: parses projects and options into `BatchJob`s and runs them without a window (attaching to the parent console on Windows) |
| `lip_sync_dialog.rs` | The Lip Sync window: picks the dialogue and timing source, maps drawings to mouth shapes, applies `Layer::apply_lip_sync` |
| `tablet.rs` | Pen pressure: from Windows Ink pens via egui's touch events with force, and on macOS from an AppKit local event monitor that notes each pen event's pressure and tilt. `samples` gives every pointer position since the last frame with its pressure |
| `rhubarb.rs` | Finding and running Rhubarb Lip Sync on a background thread (`Job`): decodes the dialogue to a temporary WAV, reads progress from its `--machineReadable` log, parses its TSV output |
| `theme.rs` | The look: Inter as the interface font, a palette of greys with one blue accent (constants other modules draw with), spacing and corner radii |
| `icons.rs` | The interface's small icons (playback, layer toggles, area kinds), drawn as lines in the theme's colours, and `IconButton` |
| `layer_thumbnails.rs` | The timeline's per-layer thumbnails (`render_layer_thumbnail` in the render crate), cached and redrawn only when what shows in them changes, a few per UI frame |
| `layout.rs` | The window's areas, Blender-style: a binary tree of splits whose leaves are areas, each holding tabs (at most one per `Kind`; views are kept per area and kind, so a hidden tab keeps its state) and showing one `Kind` of editor, picked from a grouped menu in its header (`Kind::GROUPS`); the timeline and graph editor are one `TimelineView` whose mode follows the area's kind; resizing, splitting, joining, swapping and maximising, and saving the layout to the settings folder. `Workspace` presets build whole layouts. `app.rs` keeps a view per stage, timeline and exposure-sheet area (`Views`), menu commands acting on the stage last pointed at |
| `adjust_dialog.rs` | The Adjustments window: live preview from the canvas as it was, OK or Cancel, and the Curves editor |
| `xsheet.rs` | The Exposure Sheet window: `column` turns a layer's exposure into cells (starts and holds), `enter` exposes a typed drawing name, making the drawing if it's new |
| `shortcuts.rs` | Every keyboard shortcut in one place |
| `motion_path.rs` | The math for showing and bending a layer's motion path on the stage |
| `prefs.rs` | Settings kept for next time in the settings folder's `prefs.json`: the safe areas, the field guide, and the Batch Export queue and folder (formats by their command-line slug, so a job in a format this build lacks is skipped). `app.rs` gathers them every frame and saves on a change, as it does the layout; screenshot runs neither read nor write them unless `INKFRAME_SCREENSHOT_PREFS` asks |
| `templates.rs` | The Templates panel: the template library in the settings folder's Templates folder (`layout::settings_dir`), saving the selected layers and inserting one at the playhead, with thumbnails drawn from each template |
| `text_edit.rs` | Typing into text on the stage: the text cursor, selection and keys, kept separate from drawing so it can be tested |
| `screenshot.rs` | `INKFRAME_SCREENSHOT=out.png` captures the window and quits |

egui is an *immediate-mode* UI: each panel's `show()` function runs every
frame, draws its widgets, and handles input on the spot. There are no
callbacks or retained widget trees. Panels read and change `Editor` directly.

## Build tooling

`xtask/` is a small binary for release chores, run as `cargo xtask <task>`
(the alias is in `.cargo/config.toml`). It isn't part of the app.

- `samples.rs` builds the sample projects in `examples/` from code with
  `inkframe-core` (`cargo xtask samples`), so they can be rebuilt in the
  current file format; `inkframe-render`'s `tests/examples.rs` checks they
  all open and draw.
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
