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
| `shape.rs` (styles) | `Style` has the flat `fill`, the `stroke` (with `LineCap`, `LineJoin` and an optional `Dash`) and an optional `ShapeGradient` (linear or radial, any number of `GradientStop`s, its ends as fractions of the shape's box so it follows the shape). Build strokes with `Stroke::new` |
| `art_brush.rs` | Art and pattern brushes: `Stroke::brush` (`StrokeBrush` of a `BrushKind` and a symbol). `Document::drawn_as(element)` is what a shape is drawn as: itself, or its fill and the symbol's first-frame picture laid along its outline (a `Track` of arc length and averaged normals; the picture's height spans the stroke width). Used by the renderer, SVG and Lottie. Brush symbols count as used (`Layer::symbols_used`) |
| `ruler_guides.rs` | `RulerGuide` (kept in `Document::ruler_guides`, stage pixels) and snapping to guides: `snap_point`, `snap_box` |
| `envelope.rs` | `Lattice`, Distort (four corners) and Envelope (eight points with curve handles): the box's sides as Bézier curves with the inside filled in by a Coons patch, bending shapes' points for good (`map`, `bend`), cutting segments first so straight edges can curve |
| `node_places.rs` | Where nodes have been dragged in the node view (`NodePlace`: a `NodeRef` and a point, per timeline) on `Document::node_places`; the graph itself is built from the layers each frame |
| `perspective.rs` | `Perspective` (a `PerspectiveKind` and its vanishing points, in stage pixels) on `Document::perspective`; `directions` and `snap` give the Pen's Shift constraint |
| `preset.rs` | `MotionPreset`: a stretch of a layer's keys kept relative to where it started (positions and angles as changes, scale and opacity as multiples), applied from any frame to any layer; the built-in presets |
| `boolean.rs` | Union, Intersect, Punch, Crop (`ShapeOp`, `combine`) and Break Apart: outlines flattened, edges split where they cross, kept where the result is on one side only (by winding number either side of each edge's middle), joined into loops, and fitted back into curves (`vectorise::fit_points`) |
| `frames.rs` | Editing a layer's time: `insert_frames`, `remove_frames` and `reverse_frames`, all through one `retime` that moves every key, deformer key, drawing swap and shape tween (and `AnyChannel::retime` / `reverse_span`, `Easing::reversed`) |
| `library.rs` | `Asset` (a symbol, image or sound) and the Library's one-level folders: each asset names its folder, `Document::library_folders` keeps empty ones; renaming, moving and deleting assets |
| `symbol.rs` | `Symbol` (library entry), `Instance` (a symbol placed on a layer) and its timing, symbol swaps partway through (`Layer::symbol_swaps`, `symbol_at`, `swap_symbol`), `duplicate_symbol`, Convert to Symbol (one layer, or several with their links via `convert_layers_to_symbol`), and symbol-aware bounds and hit-testing |
| `template.rs` | Templates: copied layers (with their symbols, pictures, fonts and canvases) saved as a file of their own with the frame they started on; `insert_template` moves their animation to the playhead (`Layer::shift_time`) and pastes them |
| `text.rs` | `Text`: content, `Font`, size, alignment; its measured extent, which the editor keeps up to date |
| `guide.rs` | Motion guides: `Document::pose`, a layer's transform following its guide (use it rather than `Layer::transform_at` wherever layers are drawn or edited) |
| `rig.rs` | Parenting and pivots: `Document::layer_matrix` (parents · pose · −pivot), the one map from a layer's drawing to its timeline that drawing, hit testing and editing all use; `set_parent` and `set_pivot`, which keep the layer where it is on screen |
| `ik.rs` | Inverse kinematics for the Bone tool: `Document::ik_chain` (a layer and its parents below the root) and `Document::reach`, a cyclic-coordinate-descent solver that keys only rotations and honours `Layer::rotation_limits` |
| `folder.rs` | Layer folders (a `Layer` with `folder` set; members name it in `in_folder` and always sit directly below it, restored by `tidy_folders`), `add_folder`, `place_layer` (a timeline drop), `move_among_siblings`, `folder_block`; layer colours (`Layer::tag_color`, `TAG_COLORS`) |
| `morph.rs` | Shape tweens: `ShapeTween` (keyed by the drawing swap it morphs into), `Layer::morph_at` (used by `shown_elements`, so drawing, bounds and hit testing all see the morph), and `morph`: shapes to anchors, segments split until the counts match (at least 16 on a closed shape), lined up by the best rotation and direction or by shape hints (`Drawing::hints`), then blended with colours and stroke widths |
| `palette.rs` | The colour palette: swatches that own their exact colour (every fill and stroke of it is theirs, so `set_swatch_color` recolours them all), in versions (day, night) switched with `use_palette_version`, which maps every swatch at once. `Palette` keeps swatch colours distinct within a version |
| `skeleton.rs` | Moho-style skeletons: a `Skeleton` (a tree of `Bone`s with rest ends, `strength`, and animated `turn`/`moved`) on a folder layer, which the layers in the folder bind to (`BoneBinding`: by region, by region among one limb's bones (`Limb`, via `carry_among` and `limb`), to one bone, or not). `pose` gives each bone's rest-to-posed matrix; `carry` blends them by nearness (linear blend skinning); `reach` is CCD inverse kinematics. `Document::posed_elements` is a layer's shapes as shown, bent by its own deformers and then the skeleton (use it for drawing, hit testing, bounds and export); `Document::layer_bend` is what the renderer uses to bend paint layers and pictures through a mesh. Shapes or single points can be pinned to one bone (`BonePoints` in `Drawing::bone_points`, as `ShapeBones` per shape when bending), and a bone's `SmartAction`s morph a layer toward another drawing (`Morph`) or pick a switch's layer (`Switch`) as it turns, or reshape it by hand-drawn keys at several turns (`Correct` with `CorrectionKey`s, morphed between by `corrected`; the app draws one by swapping the key's shapes into the drawing while the action is out of the bone, see `Correcting` in the app's `skeleton.rs`, and `Editor::settled_doc` is what's saved, autosaved and exported). Bones can have `limits` (clamped when posing and in IK), an animated `stretch` (with `squash` keeping area; children follow the stretched end but aren't stretched, via `Pose::rigid`), and `Dynamics`: a spring on the tip simulated from `SETTLE` frames before the one asked for, so swings lag and settle deterministically. `Document::rest_pose` (a `RestPoseView` that's never saved and never differs for undo) turns posing off for rig setup. `Bone::target` (Moho's target bones) is solved in `Skeleton::placing` by CCD over `target_chain` (the bone and up to two above, short of the root, empty if the target hangs from them) before any dynamics. `LayerBend::unbend` maps a point back through the same mesh the renderer draws with (`mesh_lines`, on multiples of `MESH_CELL`), so painting on a posed frame lands where it's seen |
| `weld.rs` | Welded points (Moho's shared points), kept by position rather than stored: points of different shapes within `WELD_DISTANCE` are one point. `welded_to`, `welded_at`, `welded_points`. Pencil lines don't weld (their points are their outline's). Pins carry across welds when bending (`skeleton.rs`'s `weld_pins`) |
| `switch.rs` | Switch folders: `Layer::switch` shows one direct child at a time, picked by a smart bone, else `switch_keys`, else the topmost (`switch_choice`). `Document::shows` (visible, on, and not switched off) is what drawing, hit testing and export check |
| `warp.rs` | The free-form deformer: a grid of animated control-point offsets over a drawing's rest bounds; each point of the drawing moves by a Bézier-patch blend (Bernstein weights) of them, applied after the bone deformer in `shown_elements` |
| `particles.rs` | Particle layers: `Emitter` (rate, emitter shape, direction and spread, speed, gravity, wind, drag, wander, lifetime, size/opacity/colour over life, spin, sprite) and `Preset`s. No simulation state: particle *i* is born at about *i*/rate and its whole life follows from the seed by formula (closed-form drag, a sine wobble), so any frame stands alone. Births are placed with `placement_without_camera` at the birth time, so particles stay in the scene when the emitter moves; `Document::particles_on_layer` maps them back onto the layer for drawing, and `particle_bounds` is the layer's bounds |
| `vectorise.rs` | Tracing pixels into shapes (`trace`, `Document::vectorise_layer`): labels pixels (ink by brightness, or colours found as histogram peaks and settled on nearby pixels only, so anti-aliasing doesn't drag them), merges patches under the minimum area into their surroundings, traces each patch's outline and holes along pixel edges (turning right at diagonal touches, so loops never cross), fits curves through the midpoints of the pixel stairs keeping long-run corners sharp, and joins holes to outlines by zero-width slits (holes run the opposite way, and fills are by winding) |
| `vector_fill.rs` | Ink and paint: a drawing's line art (`elements`) and colour art (`colour_art`, drawn beneath); `Layer::fill_enclosed` fills the area the lines enclose (grid flood with lines thickened to close gaps, grown back under them, outline traced and simplified) or recolours the fill already there |
| `paint.rs` | Painted pixels: `Canvas` (256×256 premultiplied RGBA tiles, shared between undo snapshots with `Arc`), `Shape::Paint` naming a canvas, `Document::element_bounds`/`element_hit` (paint counts only painted pixels), `add_paint_layer`, `separate_canvases` (a duplicated layer or drawing gets its own canvas) |
| `brush/` | The brush engine, Photoshop's section by section. `mod.rs`: `BrushSettings` (Brush Tip Shape plus each section, kept while switched off) and `BrushStroke`, which lays a `Stamp` per dab (size, angle, roundness, flips and scatter from the dynamics) into the stroke's own layer of colour and coverage, each dab raising coverage only towards its opacity ceiling (`deposit`), then redraws the touched tiles from how they were before the stroke with the texture, dual brush and wet edges applied; `rest` keeps Build-up flowing; `DabPath` places dabs for every stroke tool. `tip.rs`: round or sampled tips (`TipImage`, with halved copies for painting a big tip small, saved as PNG in JSON). `dynamics.rs`: `Control`, `DabInput` and the Shape Dynamics, Scattering, Color Dynamics and Transfer sections. `texture.rs`: `Texture`, `Pattern`s and the `Blend` modes Texture and Dual Brush share. `presets.rs`: the ready-made `BrushPreset`s, their tips drawn in code |
| `brush/` (speed) | A big dab spans dozens of tiles, so `in_parallel` shares a stamp's tiles (and the redraw's) across the cores, and a texture's heights are worked out once per tile per stroke. `cargo run --release -p inkframe-core --example brush_benchmark` times every ready-made brush and retouching tool from 100 to 2500 pixels (name one to time only it); a dab should stay well under a 16 ms frame |
| `abr.rs` | Reading Photoshop `.abr` brush files: versions 1–2 (round and sampled brushes) and 6–10 (`samp` tips, `patt` patterns, and the `desc` action descriptor mapped onto the brush sections, textures finding their pattern by id); PackBits rows through `psd.rs` |
| `i18n.rs` | The core's side of translation: `tr_later` marks names the app translates where it shows them (they stay English in the core, being data too), and `tr`/`tr_args`, for the text the core writes for people (undo steps' names), translate through the function the app hands `set_translator` at start-up |
| `selection.rs` | Pixel selections: a soft 0–255 mask per canvas pixel (`Selection`), from rectangles, ellipses and polygons (anti-aliased by 4×4 subsampling) or the magic wand; combining (`Combine`); `limit`/`limit_canvas` keep a change only where selected; `keep`/`clear`/`shifted` for moving and cutting; `outline` for marching ants |
| `adjust.rs` | Brightness/Contrast, Hue/Saturation, Levels and Curves (a monotone cubic), returning a new canvas so the app can preview from the original |
| `psd.rs` | Photoshop files: reading and writing 8-bit RGB layered images (PackBits or raw), and `Document::import_psd`; `inkframe_render::export_psd` draws each layer for writing |
| `blend.rs` | `BlendMode`, every layer's Photoshop-style blend mode; the renderer composites a non-Normal layer through a scratch buffer |
| `retouch.rs` | Smudge, Blur, Sharpen, Dodge, Burn, Clone Stamp and the Mixer Brush (a reservoir of paint, `RetouchStroke::reservoir`, that the colour under each dab mixes into, smeared like Smudge when wet and running dry with a light load): a `RetouchStroke` places dabs with the brush's `DabPath`, and each dab reads the square of pixels under it (`Canvas::read_region_into`, into buffers kept from dab to dab), reworks them in place in bands of rows across the cores (Blur and Sharpen with a two-pass running-sum box blur, the second pass fused with the mixing), and writes them back |
| `parallel.rs` | Sharing pixel work across the cores with scoped threads: `in_parallel` (a list of jobs, such as a dab's tiles) and `in_bands` (a slice in bands of whole rows), both staying on the calling thread when the work is small |
| `effects.rs` | Colour effects (`ColorEffect`: brightness, tint, advanced) and filters (`Filter`: drop shadow, blur, glow, adjust colour, motion blur; motion blur is drawn by the renderer, which averages the layer drawn at moments across the shutter using `Document::layer_matrix_at`), as pixel operations on premultiplied RGBA. `Effects` (both together) is the value of the track's sixth animated property, `Property::Effects`, blended setting by setting (`Animatable` asks only for `Clone`, so values like this can animate); blurs are three box passes standing in for a Gaussian. The renderer runs a layer with effects through its scratch buffer, widened by `filter_margin` (past the band being drawn, so bands meet seamlessly) |
| `fill.rs` | The Paint Bucket (`bucket_fill`, a scanline flood fill with a tolerance, or every matching pixel) and `gradient` (linear or radial, blended as straight colours) |
| `container.rs` | The project file: a zip of `document.json` and each canvas tile as `canvases/<id>/<column>_<row>.png`. Plain-JSON files from before version 17 still open |
| `controller.rs` | Master controllers: a slider over a set of layers with poses (transforms and drawings) recorded along it; `set_controller` blends the two poses either side and keys the layers on the frame, so what it poses animates like any edit |
| `deform.rs` | Bone deformers: a `Deformer`'s rest joints and animated bends, bending outlines by blending each point between nearby bones (subdividing segments first). `Layer::shown_elements(frame)` is the drawing as seen (drawing swapped, then bent): use it for drawing, hit testing and bounds. Also `Layer::key_frames`/`insert_key`/`remove_key`/`move_key`, covering the transform and the deformer together |
| `drawing.rs` | Drawing substitution: a layer's named `Drawing`s and its `Exposure` keys (which drawing shows from which frame). Use `Layer::elements_at(frame)` for what a layer shows, `all_elements` for "is this asset used anywhere" |
| `lip_sync.rs` | Lip sync: the nine standard `MouthShape`s, reading Rhubarb (`.tsv`) and Papagayo (`.dat`) timing (`parse_timing` tells them apart), mouth shapes from loudness, guessing which drawing is which shape (`MouthMap`), and `Layer::apply_lip_sync`. No audio code: the app passes in per-frame loudness |
| `line.rs` (textures) | `LineTexture` (`TextureKind` and amount), on `Layer::line_texture`; the renderer's `texture.rs` makes a tinted grain tile (hashed noise, cached per kind) to paint pencil lines and strokes through, in the layer's units |
| `line.rs` | Pressure-sensitive lines (`Shape::Line`): a centreline with a width per anchor, eased between anchors and drawn as the filled outline it sweeps out with round ends (`line_outline`). `to_anchors` gives that outline, so deformers, morphs, fills and hit tests treat a line as its shape; the Width tool edits the widths |
| `mask.rs` | Masks: which mask a layer is seen through, and Animate's "mask the layer below" |
| `font_file.rs` | `FontFile`: a font file linked to the project, and the families it provides |
| `image.rs` | `ImageAsset`: an image file the document links to, and its size in pixels |
| `audio.rs` | `AudioTrack`: a sound file laid against the timeline; how linked files are found |
| `clipboard.rs` | Copying layers (with the symbols, images and fonts they need) and keyframes, and pasting them into the same or another project |
| `history.rs` | Snapshot-based undo/redo; each step is named by comparing it with the one before (`describe`), for the History panel, which can `jump` several steps |
| `align.rs` | The Align panel's work: `stage_bounds`, `align_layers`, `distribute_layers`, `match_size`, moving layers in their parents' space |
| `geometry.rs` | `Point`, `Rect`, `Affine`, and `simplify` (Ramer–Douglas–Peucker), which freehand strokes and vector fills share |

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
(`Document::stack_by_depth`), version 31 library folders, version 32
gradients and line styles, version 33 symbol swaps, version 34 ruler
guides, version 38 smart bone corrections (`SmartAction::Correct`), version 35 skeletons (`Layer::skeleton` on a folder,
`Layer::bone_binding` on what's in it), and version 36 smart bones
(`Bone::actions`), pins (`Drawing::bone_points`) and switch folders
(`Layer::switch`, `switch_keys`), and version 37 bone limits, stretch
and dynamics;
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
before their outlines are built. All the workers drawing a picture
share one `inkframe_core::DrawCache` (`draw_cache.rs`), so each skeleton's
pose (IK for target bones, dynamics) and each bent layer's shapes
(`Document::posed_shapes`) are worked out once per frame, by whichever
worker asks first, rather than for every band. `tests/heavy_scene.rs` (ignored; run in release)
times a 500-layer scene with 24 rigged walkers and particles, and the app's
`INKFRAME_SCREENSHOT_BENCH=n` hook times its own updates while stepping
through frames.

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

### inkframe-script

Runs JavaScript against documents with QuickJS (`rquickjs`). `run(Script)`
evaluates `prelude.js`, which builds the `inkframe`, `Document`, `Layer`
and `console` API on the plain functions in `native.rs` (ids for documents
and layers, JSON for anything structured, frames from 1), then the user's
script, all against a copy of the document; only success returns the copy
(`Outcome`). An interrupt handler stops it on the `stop` flag or a time
limit. `values.rs` converts properties and easing; `files.rs` resolves
paths and allows only `.inkf` projects and exports with their format's
extension. The app runs scripts on a thread (`scripts.rs`) and the command
line with `inkframe --script` (`cli_script.rs`). QuickJS is C, so this
crate (and the app) can't be cross-checked for Windows from a Mac; CI's
Windows runner builds it.

Why it's built this way: JavaScript because Animate's JSFL and Harmony's
scripting are both JavaScript, so people's scripts and habits carry over
(QuickJS adds about 1.2 MB, and has no file or network access of its
own); a failing script changes nothing, as everything runs on a copy;
and frames count from 1, as the timeline shows them.

### inkframe-audio

Turns sound files into `Clip`s: interleaved stereo `f32` at 48 kHz, whatever
the source. `decode` uses symphonia (pure Rust: WAV, MP3, AAC, FLAC, Ogg…)
and resamples with rubato's band-limited sinc interpolation. `mix` lays
tracks against the timeline at their start frames; `mix_document` does that
for a whole document, decoding each file once. Documents only *reference*
audio files (`AudioTrack::path`), relative to the project file when
possible; `AudioTrack::resolve` finds them.

`rhubarb.rs` finds and runs Rhubarb Lip Sync: decoding the dialogue to a
temporary WAV, reading progress from its `--machineReadable` log and
parsing its TSV output. The app's Lip Sync window runs it on a background
thread (`Job`); scripts wait for it (`recognize`).

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

GIFs go through ffmpeg (`palettegen` from the frames, then `paletteuse`
with a fine ordered dither, `-loop 0`). SVG (`svg.rs`) and Lottie
(`lottie.rs`) are vector: both walk the layers as `render_layers` does
(`layer_matrix`, `shown_elements`, `instance_view`, `mask_for`) and take
outlines from `inkframe_render::shape_outline`, so they match the render.
What can't be vectors (filters, colour effects, particles) is drawn by
`inkframe_render::render_layer_alone` and embedded as PNG. Lottie writes
each layer as a null carrying its local matrix, decomposed every frame
into hold keys (so rigs, the camera and depth come out exact), with
shape layers per run of identical frames under it, symbols as
precompositions fitted to their extent (Lottie clips a precomp to its
box) and time-remapped, and masks as track mattes. Checked against
lottie-web by eye (`cargo test -p inkframe-export -- --ignored` writes a
sample to `target/lottie-check/features`).

The Web Page export (`html.rs`) is every frame as SVG (`svg::frame_svg`),
each different frame kept once, as JSON in one self-contained page whose
few lines of script swap them at the frame rate; a click pauses, the
arrow keys step while paused. `Document::frame_actions` and
`Layer::button` (`WebAction`s, core `web_actions.rs`) make it interactive:
the SVG marks button layers' groups (`class="inkframe-button"`,
`data-button`), and the page runs frame actions as it reaches them and
button actions on a click (links only `http(s):`/`mailto:`).

Sprite sheets (`sprite_sheet.rs`) render every frame, crop each to its
drawn pixels, and keep one copy of frames that come out identical (held
drawings). Every cell is the union of those areas, so frames stay aligned;
`Grid::fit` lays the cells out near-square within 8192 px with 2 px
padding. Cells too many for one sheet are shared out evenly over as few
as hold them (`pack`), written as `name-0.png`, `name-1.png`…, each with
its own JSON listing its frames and the others' JSON in
`meta.related_multi_packs` (TexturePacker's multipack). The JSON beside the PNG is TexturePacker's "JSON hash"
(`frames` with `trimmed`/`spriteSourceSize`/`sourceSize` recording the
crop, `animations`, `meta`), plus a per-frame `duration` as Aseprite
writes. No audio.

### inkframe-app

| File | What's in it |
|---|---|
| `autosave.rs` | Crash recovery: `Autosave::tick` copies the project every 30 s while it has unsaved changes (cloned, then zipped and written on a thread, to a temporary name renamed into place) into the settings folder's `Recovery/`, with a JSON note (original path, name, saved time, heartbeat). Saving or a normal quit `clear`s it. `left_behind` finds other sessions' copies whose heartbeat has been quiet for 2 minutes, so a second Inkframe that's still running isn't mistaken for a crash; `Recovery::open` reopens one as its project with changes unsaved (`Editor::recovered`). The window offering them is `App::recovery_window` |
| `app.rs` | Window layout, menus, the `Command` enum, file open/save |
| `editor.rs` | `Editor`: document, selection, current frame, tool, and the operations panels call |
| `stage.rs` | The canvas: zoom/pan, drawing tools, selection overlay and transform handles; `stage/rulers.rs` draws the rulers and guides and drags guides; `stage/inside.rs` handles the pointer while inside a layer (picking, moving and scaling shapes); `stage/lattice.rs` drags a Distort or Envelope's handles; `stage/bones.rs` draws skeletons and handles the Add Bone tool and posing with the Bone tool; `stage/perspective.rs` draws the perspective guide and drags its vanishing points |
| `frame_commands.rs` | Animate's frame commands on the frames picked in the timeline (Insert/Remove Frames, Blank Keyframe, Convert to Keyframes, Reverse Frames) and Edit Multiple Frames' move across them (`MultiFrameMove`) |
| `envelope.rs` | Modify › Transform › Distort and Envelope: the lattice edit under way (`Editor::lattice`), re-bending the picked shapes (or a whole layer's drawing) from their originals on every drag |
| `skeleton.rs` | Rigging with skeletons: New Skeleton and New Switch, adding, posing, moving and deleting bones (bindings and pins follow a deleted bone's renumbering), pinning picked shapes or the Subselection's point (`Editor::picked_anchor`) to a bone, and the picked bone (`Editor::selected_bone`) |
| `context_bar.rs` | The bar above the stage: the tool (`Tool::description`) and buttons for what can be done now (it returns a `Command` for the app to run; plain settings it flips itself), and view switches on the right |
| `command_finder.rs` | Find a Command (⌘K): a written-out catalogue of commands and tools (`entries`), each with its menu, icon, keys and other words, searched as you type |
| `action_timeline.rs` | A smart bone's actions laid out across its turns, as Moho's action timeline: scrubbing asks for a preview (`Bone::preview`, a `TurnPreview` that never saves, compares or survives a copy; `Editor::preview_turn`/`check_preview`), and lanes retime actions and correction keys |
| `i18n.rs` | Translation: `tr`/`tr_args`/`tr_later` take English text and give it in the language chosen at start-up (`INKFRAME_LANGUAGE` or `LANG`) from a built-in `locales/<code>.txt`, or the made-up `pseudo` language for checking what's wrapped; `cargo xtask strings` (`xtask/src/strings.rs`) lists every wrapped text in `locales/template.txt` |
| `whats_new.rs` | What's New: this version's section of the changelog (built in with `include_str!`, parsed into intro, headings and bullets), shown once after an update (`Prefs::seen_version`) and from Help |
| `tour.rs` | The first-run tour: cards that light up one part of the window at a time (the app notes where each part was drawn in `Targets` every frame), shown until `Prefs::toured` and from Help › Take the Tour |
| `bones_panel.rs` | The Bones area (`Kind::Bones`): each skeleton's bone tree (`tree`), what it rigs and how each layer follows, and the rig setup switch |
| `shape_ops.rs` | Combine Shapes and Break Apart on the picked shapes (text through `inkframe_render::shape_outline`) |
| `motion_presets.rs` | The Motion Presets area: built-in and saved presets (`.inkpreset` files in the settings folder's Motion Presets), animated previews, Apply and Save Movement |
| `swap_dialog.rs` | Modify › Swap Symbol: a picture of each symbol, swapping the selected instances for the whole layer or from the current frame (`Document::swap_symbol`), and Duplicate Symbol |
| `multi_edit.rs` | Carrying a change made in the properties panel to every selected layer: position by the same amount, other values set the same, only what changed |
| `inside.rs` | Working inside one layer: which shapes are picked (by their place in the showing drawing), hit-testing them, moving and scaling them by their points (`Element::transformed`), deleting them, and making a symbol of them |
| `handles.rs` | The math for scale and rotate handles (anchor pinning, multi-turn rotation) |
| `depth_view.rs` | The Depth View: the main timeline's top-level layers from above, as lines at their depths, with the camera's field of view. Dragging a line sets its depth, dragging the camera dollies and pans it (both through `Editor::set_depth` / `set_pose`, so they key the frame); scroll zooms, empty space pans, Fit fits. What's dragged is decided at the press point |
| `node_view.rs` | The Node View: the timeline being edited drawn as a graph built from the layers each frame (layer nodes with Parent and Mask inputs and Transform and Image outputs, effect and filter nodes in each layer's image chain, a Composite in stacking order, the Display). `connect` and `disconnect` turn wiring into the same edits the timeline and Properties make (`set_parent`, `masked_by`, `place_layer`, effects); only node positions are the view's own |
| `pen.rs` | State of the pen path being drawn, which lasts across many clicks |
| `subselect.rs` | The math for moving, deleting and converting anchors, and pulling new handles out of them (Alt-drag), with the Subselection tool or the Pen between paths |
| `timeline.rs` | Transport bar and the hand-painted layer × frame grid; dragging a layer name scrolls the list, holding it first restacks (pen-friendly) |
| `inspector.rs` | The properties panel, with Tool, Layer and Document tabs that follow the selection and tool |
| `brushes.rs` | The brush library: the ready-made brushes and the person's (`brushes.json` in the settings folder), importing brush sets (`.inkbrushes`) and `.abr` files, exporting brush sets |
| `brush_panel.rs` | The Brush Settings area (Photoshop's section list, or a menu when narrow; the angle-and-roundness dial; a live sample stroke), the Brushes area, and the quick menu that right-clicking the stage with a painting tool opens |
| `panels.rs` | The Swatches, Align, Controllers and History panels |
| `value_graph.rs` | The timeline's Graph mode: a property's values over time, with draggable keys and easing handles |
| `easing_editor.rs` | The easing curve in the properties panel, with draggable Bézier handles |
| `toolbar.rs` | Tool buttons with vector-drawn icons |
| `audio_engine.rs` | Plays the soundtrack in sync (the audio clock drives playback) and scrubs it; decodes files in the background |
| `library.rs` | The Library panel: cards for symbols, images and sounds with cached thumbnails, folders, search, and dragging cards out (an `Asset` payload in `egui::DragAndDrop`, dropped on the stage or timeline), and the preview of the selected card: symbols play through `inkframe_render::render_symbol_frame`, fitted to `symbol_extent` (the box around every frame, so playing doesn't jitter), sounds through `AudioEngine::preview` |
| `scripts.rs` | The Scripts menu and Script Console area: lists the Scripts folder's `.js` files (seeded with `EXAMPLES`, built in from `examples/scripts`, when the folder is first made), runs one on a thread against a copy of the project (Running window with Stop), applies the result as one undo step unless the project was edited meanwhile, and shows the log or error (Script Output) |
| `cli_script.rs` | `inkframe --script SCRIPT [PROJECT...] [--save]` |
| `examples.rs` | File › Open Example: finds the sample projects (bundled in `Contents/Resources/Examples` or an `Examples` folder beside the exe, else the checkout's `examples/`; only a folder holding `.inkf` files counts), lists them with a rendered picture and a note from `ABOUT`, and opens one as an untitled copy (`Editor::untitled_copy`) so saving never writes into the app |
| `export_dialog.rs` | Export settings and the background export thread |
| `guides.rs` | Safe areas (`SafeAreas`: current 93/90%, classic 90/80%) and the 12-field chart, as lines and labels in stage pixels; `stage.rs` draws them over the stage (the camera's frame), turned with the view |
| `batch_dialog.rs` | File › Batch Export: a queue of `BatchJob`s (project, format, size, transparency) run on a thread with `batch::run`, status per row |
| `cli.rs` | `inkframe --export …`: parses projects and options into `BatchJob`s and runs them without a window (attaching to the parent console on Windows) |
| `lip_sync_dialog.rs` | The Lip Sync window: picks the dialogue and timing source, maps drawings to mouth shapes, applies `Layer::apply_lip_sync` |
| `tablet.rs` | Pen pressure and tilt: from Windows Ink pens via egui's touch events with force, and on macOS from an AppKit local event monitor that notes each pen event's pressure and tilt. `samples` gives every pointer position since the last frame with the pen's state |
| `theme.rs` | The look: Inter as the interface font, a palette of greys with one blue accent (constants other modules draw with), spacing and corner radii |
| `icons.rs` | The interface's small icons (playback, layer toggles, area kinds), drawn as lines in the theme's colours, and `IconButton` |
| `layer_thumbnails.rs` | The timeline's per-layer thumbnails (`render_layer_thumbnail` in the render crate), cached and redrawn only when what shows in them changes, a few per UI frame |
| `layout.rs` | The window's areas, Blender-style: a binary tree of splits whose leaves are areas, each holding tabs (at most one per `Kind`; views are kept per area and kind, so a hidden tab keeps its state) and showing one `Kind` of editor, picked from a grouped menu in its header (`Kind::GROUPS`); the timeline and graph editor are one `TimelineView` whose mode follows the area's kind; resizing, splitting, joining, swapping and maximising, and saving the layout to the settings folder. `Workspace` presets build whole layouts. `app.rs` keeps a view per stage, timeline and exposure-sheet area (`Views`), menu commands acting on the stage last pointed at |
| `adjust_dialog.rs` | The Adjustments window: live preview from the canvas as it was, OK or Cancel, and the Curves editor |
| `xsheet.rs` | The Exposure Sheet window: `column` turns a layer's exposure into cells (starts and holds), `enter` exposes a typed drawing name, making the drawing if it's new |
| `shortcuts.rs` | Every keyboard shortcut in one place |
| `motion_path.rs` | The math for showing and bending a layer's motion path on the stage |
| `prefs.rs` | Settings kept for next time in the settings folder's `prefs.json`: the safe areas, the field guide, the brush and eraser, the language picked, and the Batch Export queue and folder (formats by their command-line slug, so a job in a format this build lacks is skipped). `app.rs` gathers them every frame and saves on a change, as it does the layout; screenshot runs neither read nor write them unless `INKFRAME_SCREENSHOT_PREFS` asks |
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
- `guide.rs` builds the guidebook PDF (`cargo xtask guide`): takes the
  screenshots `docs/guide/figures.txt` lists through the app's
  `INKFRAME_SCREENSHOT_*` hooks, turns the Markdown chapters in
  `docs/guide/chapters/` into one HTML page with its own small converter
  (figures with crops and callouts, numbered figure references, Tip and
  Important note boxes, keys), adds the tool and command tables from
  `inkframe --reference` (`crates/inkframe-app/src/reference.rs`, built
  from `Tool::GROUPS` and `command_finder::entries()`), and prints it
  with headless Chrome. The contents' page numbers take a second printing:
  `page_numbers` reads the PDF's named destinations (one per heading
  anchor) and page tree back, and the book is printed again until the
  numbers settle. See `docs/guide/README.md`.
- `strings.rs` collects every text the app wraps for translation into
  `locales/template.txt` (`cargo xtask strings`); its tests check the
  template is up to date and translations are of texts the app still has.
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
