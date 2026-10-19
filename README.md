# Inkframe

An open-source 2D vector animation editor, in the spirit of Adobe Animate.
Written in Rust. Runs natively on Apple Silicon Macs, with Windows support
planned (the code is already cross-platform and CI builds it on Windows).

![Inkframe editing a bouncing ball, with onion skinning on and per-property keyframes expanded in the timeline](docs/screenshot.png)

## What it does today

- **Draw** rectangles, ovals, Bézier paths with the Pen tool, and freehand
  pencil strokes on a stage of any size.
- **Text** in any installed font, or font files imported into the project
  (so titles look the same on every machine), with bold, italic, alignment
  and line spacing. Characters the font doesn't have, like emoji or
  Chinese in a Latin font, come from another font that does. Emoji are in
  full color and stay sharp at any size. It's real vector outlines,
  filled and stroked like shapes, so it stays sharp at any size and
  animates like any other layer.
- **Import images** (PNG, JPEG, GIF, WebP) to trace over or composite.
  They move, scale, rotate, fade and animate like any other layer.
- **Animate with per-property keyframes.** Go to a frame, move or change a
  layer, and a key is created for just the properties you changed. Position,
  rotation, scale and opacity each have their own keys and timing, so a
  layer can ease its movement while fading at a steady rate. Scale and
  rotate with on-stage handles, or type exact values in the properties panel.
- **Rig characters** by attaching layers to each other: a hand to the
  forearm, the forearm to the upper arm, the arm to the torso. Each part
  moves, turns and scales with what it hangs from, and turns around its own
  pivot (an elbow, a shoulder), which you can move anywhere on it. Pose
  a rig with the **Bone tool**: drag a hand and the elbow and shoulder
  turn to follow (inverse kinematics), with limits so elbows and knees
  bend only one way.
- **Bend drawings with deformers:** run a few bones through a single
  drawing (an arm, a tail, a scarf) and it bends smoothly along them,
  rubber-hose style, like Harmony's bone deformer.
- **Exposure sheet:** drawing swaps laid out frame by frame, as
  traditional animators time them, with the dialogue's loudness alongside.
- **Swap drawings** for mouths, hands and blinks: a layer can hold several
  named drawings ("closed", "A", "O") and show a different one from any
  frame on, the way Toon Boom Harmony's drawing substitution works.
- **Lip sync** a mouth to dialogue: the bundled
  [Rhubarb Lip Sync](https://github.com/DanielSWolf/rhubarb-lip-sync)
  listens for the actual sounds, or use the sound's loudness, or a Rhubarb
  or Papagayo timing file. Its drawings are matched to the standard mouth
  shapes by name.
- **Motion paths:** a moving layer's path is drawn on the stage, with a dot
  per frame showing its spacing. Drag the path to curve it, then fine-tune
  the curve handles; "Orient to path" turns the layer to face where it's going.
  For any shape of path (loops, winding roads), draw it, tick **Guide**, and
  have another layer **Follow guide**. Guides never appear in exports.
- **Masks:** any layer can be a mask, a window that other layers only show
  through. Masks can be animated (a moving spotlight) and soft (a faded
  mask lets part of the layer through).
- **Ease** each property's tweens separately with Linear, Ease In/Out/In-Out,
  Hold, or a custom curve: drag the curve's handles in the properties panel
  to shape it, with the playhead's position shown on the curve.
- **Timeline** with layers that expand into one row per property, visibility
  and lock toggles, draggable keys, scrubbing, looped playback and onion
  skinning. Each layer's bar shows the frames it appears on; drag its ends
  to make it appear or disappear partway through.
- **Symbols:** turn any layer, animation included, into a reusable symbol
  (F8). Place it as many times as you like from the Library; each instance
  can be moved, scaled, tweened and offset in time, and changing the symbol
  changes them all. Double-click an instance to edit the symbol in place,
  right where it sits (even rotated or scaled), with the rest of the scene
  faded around it; a breadcrumb bar shows where you are. Symbols can contain
  other symbols.
- **Audio:** import WAV, MP3, AAC/M4A, FLAC, Ogg or AIFF files, see their
  waveforms in the timeline, slide them to the right frame, and hear them
  in sync while playing, or a frame at a time while scrubbing (for lip
  sync). Exports include the mixed sound.
- **Export** with or without transparency:

  | Format | Alpha | Use it for |
  |---|---|---|
  | PNG sequence | ✔ | Lossless frames for any tool |
  | Apple ProRes 4444 / 4444 XQ | ✔ | Masters for editing and compositing |
  | Apple ProRes 422 HQ | — | High quality, smaller files |
  | H.264 MP4 | — | Review copies and the web |
  | HEVC with alpha (macOS) | ✔ | Small files with alpha for Apple devices and Safari |

- **Paint** like in Photoshop, alongside the vector drawing: paint layers
  hold pixels, painted with a soft or hard round brush (size, hardness,
  opacity, flow, spacing, paper grain, smoothing, and presets from inking
  pen to chalk and airbrush), erased with the eraser, coloured with the
  eyedropper. The paint bucket, gradient, smudge, blur, sharpen, dodge,
  burn and clone stamp work as in Photoshop, as do selections (marquee,
  lasso, magic wand), adjustments (Brightness/Contrast, Hue/Saturation,
  Levels, Curves) and layer blend modes (Multiply, Screen, Overlay…).
  Turn the canvas view to draw at a comfortable angle, and trade layered
  paintings with Photoshop and other apps as .psd files. A pen tablet's pressure thins and thickens the stroke
  (Windows Ink pens and macOS tablets such as Wacom). Paint layers animate
  like any other layer, and frame by frame with drawing swaps.
- **Undo/redo** for every change. Project files (`.inkf`) are zip files
  holding the document as readable JSON and painted pixels as PNG images.

## Getting started

You need [Rust](https://rustup.rs) (1.85 or newer) and, for video export,
[ffmpeg](https://ffmpeg.org):

```sh
brew install ffmpeg            # macOS; PNG sequences work without it
git clone <this repository> inkframe && cd inkframe
cargo run --release -- examples/bouncing-ball.inkf
```

If ffmpeg is somewhere unusual, point Inkframe at it with the
`INKFRAME_FFMPEG` environment variable.

Lip sync from Rhubarb needs
[Rhubarb Lip Sync](https://github.com/DanielSWolf/rhubarb-lip-sync/releases)
unpacked into `vendor/rhubarb/` (or pointed to with `INKFRAME_RHUBARB`); the
installers include it. Without it, lip sync from loudness and timing files
still works.

To build a double-clickable `Inkframe.app` (and a `.dmg` to install it from):

```sh
cargo xtask bundle-macos --dmg
```

[packaging/macos/README.md](packaging/macos/README.md) covers bundling
ffmpeg inside the app and what still needs an Apple Developer account. On
Windows, `cargo xtask bundle-windows` builds a `setup.exe` installer with
[Inno Setup](https://jrsoftware.org/isinfo.php); see
[packaging/windows/README.md](packaging/windows/README.md).

## Using the editor

| Action | How |
|---|---|
| Select / move | Selection tool (`V`), click or drag a shape. Hold Shift to lock to one axis |
| Select several | Shift-click shapes, or drag a box over empty stage; Shift- or ⌘-click layer names in the timeline; `⌘A` selects all, `⌘⇧A` none. Dragging any of them moves them all, the handles around them scale and rotate them as a group, and delete, duplicate, copy/cut and F6 keyframes apply to every selected layer |
| Scale | Drag a square handle. Shift keeps proportions, Option/Alt scales from the pivot |
| Rotate | Drag the round knob, or just outside a corner. Shift snaps to 15° |
| Skew | Drag an edge of the selection box between its handles to slant the layer, the opposite edge staying put; or type skew angles in the properties panel. Skew animates like any property |
| Move the pivot | Option/Alt-drag the ring in the middle of the selected layer. It snaps to the corners, edge midpoints and center; or type it in the properties panel. The layer stays where it is, and from then on turns and scales around the new spot |
| Attach to a parent | Pick a **Parent** in the properties panel. The layer stays where it is and from then on follows its parent; the timeline indents it under the parent's name. Deleting a parent hands its children to the parent above |
| Pose with bones (IK) | Bone tool (`K`) shows the rigs as bones between their pivots. Drag a layer and the joints above it turn so the spot you grabbed follows the pointer, keying their rotations on the current frame; the top of the rig (the root, drawn as a square) stays put, and dragging it moves the whole rig. To stop a joint bending the wrong way, tick **Bends** in its properties and set the lowest and highest rotation |
| Bend a drawing | Select the layer and click **Add Bend Deformer** in the properties panel: bones are laid along the drawing's long side (set how many, and how soft the bends are). With the Bone tool, drag a green joint to bend the drawing there; bends are keyed on the current frame and tween like anything else, or type them in the panel. Shapes bend; images and text stay as they are |
| Exposure sheet | Timeline › Exposure Sheet. One row per frame, one column per layer that swaps drawings, plus each sound's loudness. A name marks where a drawing starts and a line shows it held. Click a cell to go there; double-click and type a drawing's name, then Enter, to expose it and move down a row (a new name makes a new, blank drawing); right-click for a list of drawings, or to hold the one above |
| Paint | Brush (`B`) and Eraser (`E`) paint on the selected paint layer; if the selected layer isn't one, a stroke starts a new one (or use Modify › New Paint Layer). The brush's settings show in the properties panel while it's in use; `[` and `]` shrink and grow it. With a tablet, pressing harder paints a thicker line (or a denser one: Pressure › Flow). Eyedropper (`I`), or Alt/Option-click with the brush, picks a colour from the stage. A paint layer's canvas is the stage's size, pixel for pixel; it moves, scales and turns like any layer. For frame-by-frame painting, give it a new drawing on each frame (Drawings › New) |
| Fill, retouch | Paint Bucket and Gradient share a toolbar slot (`G`; `Shift+G` switches, or right-click the slot): the bucket fills the area of similar colour you click (Tolerance, Contiguous), the gradient blends from the paint colour to a second colour (or to transparent) along the line you drag, linear or radial. Smudge, Blur and Sharpen share `U`; Dodge and Burn share `D` (Range picks shadows, midtones or highlights). Clone Stamp (`S`): Option/Alt-click where to copy from, then paint; later strokes keep the same distance to the source, so a picture can be copied over in several strokes |
| Select pixels | Marquee (`M`, rectangle or ellipse), Lasso (`L`, freehand or polygonal: click corners, then click the first, double-click or press Enter) and Magic Wand (`W`, Tolerance and Contiguous in its options). Shift adds to the selection, Option/Alt takes away, both keep the overlap; a click with the marquee deselects. Every painting tool, fill and adjustment then changes only what's selected. With a painting or selection tool, `⌘A` selects all pixels, `⌘⇧A` deselects, `⌘⇧I` inverts, and Delete clears what's selected |
| Move, transform pixels | Drag inside a selection with the Selection tool (`V`) to move those pixels. Select › Layer via Copy (`⌘J`) or via Cut (`⌘⇧J`) puts them on a layer of their own, to scale, rotate and skew with its handles; Modify › Merge Down paints a layer into the paint layer beneath (which also turns vector drawing into pixels) |
| Adjust, blend | Modify › Adjustments: Brightness/Contrast, Hue/Saturation, Levels or Curves on the selected paint layer, previewed live (Curves: click to add points, drag them, right-click to remove). Any layer's Blend menu (in its properties) sets how it mixes with what's beneath: Multiply for shading, Screen for glows, and the rest of Photoshop's modes |
| Turn the view | Shift+Space and drag turns the canvas view around its middle, to draw at a comfortable angle, as on paper; `4` and `6` turn it 15° either way and `5` puts it upright (also in the View menu). Only the view turns, not the drawing |
| Photoshop files | File › Export PSD… saves the current frame as a layered .psd: each layer with its name, opacity, visibility and blend mode, over a Background layer. File › Import to Stage (or dropping a .psd on the window) brings each layer of an 8-bit RGB Photoshop file in as a paint layer; groups are flattened, and adjustment layers, masks and effects are skipped |
| Restack layers | Drag a layer's name up or down the timeline; a line shows where it will land. The ↑ and ↓ buttons move the selected layer one place |
| Folders | Modify › New Folder (or the folder button at the right of the timeline bar, or right-click › New Folder) puts the selected layers in a folder. Drop a layer just under a folder's name, or between layers in it, to put it inside; right-click › Remove from Folder takes it out. Click the folder's triangle to close it up. Its eye and lock show, hide, lock and unlock everything in it; deleting, copying or moving it takes its contents along |
| Layer colours | Each layer has a colour, shown down the left of its row, along its bar and around it on the stage when it's selected. Click the small square at the right of a layer's name (or right-click › Layer Colour) to choose another. Each row also shows a thumbnail of the layer on the current frame |
| Arrange the window | Like Blender: every part of the window is an area, and each area holds tabs. Click the showing tab for a menu of every editor (in groups: Stage, Tools, Tool Settings and Properties; Timeline, Graph Editor and Exposure Sheet; Swatches and Align; Library, Audio, Document and History), **+** to add a tab, middle-click or right-click › Close Tab to close one, and drag a tab onto another area's header to move it there. Drag the border between areas to resize them. Drag from an area's corner into it to split it (across for side by side, down for one above the other), into its neighbour to join them, or with Ctrl onto any area to swap them. Right-click a header for the same, and Ctrl+Space fills the window with the area under the pointer. The layout is remembered between sessions |
| Workspaces | The menu at the top right (or View › Workspace) switches between ready-made arrangements: Essentials, Animator (a tall timeline with the graph editor and exposure sheet as tabs), Designer (Swatches and Align beside Properties) and Painter (tool settings, swatches and history up front). Reset puts the current one back as it started |
| Properties | Three tabs: Tool (the current tool's settings, including fill, stroke and line width for the drawing tools), Layer and Document. Picking a layer or a painting or selection tool brings its tab forward |
| Free-form deformer | In a layer's properties, **Add 3×3 Grid** (or 4×4) puts a grid of points over the drawing. Drag them with the Bone tool (`K`) to squash, stretch, bulge or sway it smoothly; each drag keys that point on the current frame, so the warp animates. **Straighten** puts the grid back on this frame. It works on top of a bend deformer |
| Light table | View › Light Table, or the lamp button in the timeline bar: every layer but the selected one fades, so the drawing you're working on stands out, as in Harmony |
| Ink and paint | Tick **Draw into the selected layer** in the Pencil, Pen, Rectangle or Oval tool's settings (Properties › Tool) to draw strokes into one drawing, as in Harmony, and choose **Line Art** or **Colour Art** (which sits beneath the lines). Then, with the Paint Bucket on that layer, click between the lines to fill the area they enclose with the paint colour; the fill runs under the lines, and **Close gaps** (Small, Medium, Large) lets lines that don't quite meet still hold the paint. Click a fill again to recolour it; fill an area inside a filled one (an eye in a face) to give it its own colour |
| Swatches, palettes | Click a colour to use it: the paint colour for the paint tools; otherwise the fill (or, with Stroke chosen, the stroke) for new shapes and for the selected layers' shapes. **+** adds the current colour to the document's palette; name it in its row. A palette colour owns everything painted with it: right-click it and change the colour, and every shape of that colour changes too, as in Harmony. **New Version** in the palette menu copies the palette (for a night version, say); recolour its swatches, and switching versions recolours the whole scene |
| Align | Line the selected layers up by their edges or centres, space them evenly, or make them the same width or height. One layer, or **Align to stage**, lines up with the stage |
| History | Every change so far, named for what it did (“Add “Ball””, “Paint on “Sky””). Click a step to go back to it, or a greyed one to go forward again |
| Swap drawings | In the properties panel's **Drawings** section, **New** adds a blank drawing (or **Duplicate** a copy of the one showing) from the current frame on. Click a drawing's name to show it from the current frame on, or press `[` and `]` to flip through them. With a layer that has several drawings selected, new shapes are drawn into the drawing that's showing. The timeline marks each swap with the drawing's name |
| Camera | Pick the Camera tool (`C`) and drag on the stage: the first drag adds a camera, and dragging pans it (the scene follows the pointer); Shift-drag turns it and Alt-drag zooms. Or use Timeline › Add Camera, or the camera button at the right of the timeline bar. The camera is a layer at the top of the timeline, so move to another frame and pan again to animate it, with tweens and easing like any layer. Select it to type where it looks, its zoom and rotation, or Reset it. Give layers a **Depth** (in their properties) for parallax: deeper layers look smaller and move less as the camera pans. Hide the camera layer to turn it off |
| Colour effects, filters | In a layer's properties: **Colour Effect** sets Brightness, Tint (a colour and how much) or Advanced (each channel multiplied and offset); **Filters › Add Filter** adds a Drop Shadow, Blur, Glow (outer or inner) or Adjust Color (brightness, contrast, saturation, hue). Filters apply in order (↑ moves one earlier), can be switched off with their checkbox, and show in exports. Effects animate like the transform: change them on another frame and they tween, a filter added on a later key growing in from nothing; the ◇ beside **Effects** keys them, and they have their own row under an expanded layer in the timeline. Layers with effects are labelled "fx" |
| Shape tweens | Morph one drawing into the next, as Animate's shape tweens do. Give a layer a second drawing from a later frame (**Duplicate** in the Drawings section, then change it), then on any frame before the swap choose Timeline › Create Shape Tween (or right-click the frame, or tick **Shape tween into the next drawing** in the Drawings section). The span turns green in the timeline. Rectangles, ellipses and paths all morph, colours and line widths blend, and shapes with no partner shrink away. Pick an easing in the Drawings section. If the morph twists, add shape hints (Modify › Add Shape Hint, ⌘⇧H): drag hint **a** to a point on the first drawing (on the tween's first frame) and to the matching point on the second (on the swap's frame), and those points will meet. Right-click a hint to remove it |
| Lip sync | Give a mouth layer a drawing per mouth shape, named with Rhubarb's letters (A closed, B, C, D wide open, E, F, G, H, X at rest) or Papagayo's (MBP, etc, E, AI, O, U, FV, L, rest); a few are enough, the rest borrow a similar shape's. Import the dialogue, select the mouth, then Modify › Lip Sync…: pick the sound and where the shapes come from (Rhubarb, which listens for the sounds and takes a few seconds; loudness; or a timing file), check the drawings, and Apply. For Rhubarb, typing what's said helps with unclear words, and "Other" language listens for sounds rather than English words. The swaps land under the sound, replacing any there |
| Draw | Rectangle (`R`), Oval (`O`), Pencil (`Y`). Hold Shift for squares and circles |
| Reshape | Subselection tool (`A`): drag a shape's points, or a selected point's handles. Alt/Option moves one handle alone for a sharp corner; Alt/Option-click a point to switch it between corner and smooth; Delete removes the point. Rectangles and ovals become editable paths on the first edit |
| Pen (`P`) | Click for corners, drag for curves. Click the first point to close the shape; Enter, Esc or clicking the last point ends an open line. Backspace removes the last point, Shift snaps to 45°. Click a selected shape's outline to add a point without changing its curve |
| Animate | Move the playhead to a later frame, then drag the shape or edit its transform |
| Layer start / end | Drag either end of a layer's bar in the timeline, press `⌥[` / `⌥]` to start or end it at the playhead, or type frames in the properties panel. New drawings start on the current frame |
| Shape easing | In the properties panel's Easing section, pick a property, then drag the curve's orange handles. Pull a handle above the top line to overshoot, or below the bottom to anticipate |
| Motion paths | Select a layer that moves: its path appears as a dashed line. Drag the path to bend that tween; drag the round handles to fine-tune. **Orient to path** and **Straighten path** are under the transform in the properties panel |
| Motion guides | Draw a path (Pencil, Pen, or a shape) and tick **Guide** in its properties. Select the layer that should travel and pick the guide under **Follow guide**: between its first and last position keyframes it runs the whole guide, with its easing setting the pace. A middle keyframe pins it to the nearest point on the guide |
| Masks | Select a layer and choose Modify › Mask to make it mask the layer below, or tick **Mask** on a layer and pick it under **Masked by** on any others. The mask isn't drawn; select it to see it faintly and drag it into place |
| Copy and paste | `⌘C` / `⌘X` copy or cut the selected layer with its animation, symbols, images and fonts; `⌘V` pastes it in the middle of the view, `⌘⇧V` in place. Works between windows and projects. `⌥⌘C` / `⌥⌘V` copy a layer's pose on the current frame and paste it onto another frame or layer. Text copied from another app pastes as a text layer |
| Value graph | Click **Graph** in the timeline bar to see the selected layer's animation as values over time; pick Position, Rotation, Scale or Opacity on the left. Drag a key's dot up or down to change its value, sideways to move it, and the round handles to reshape the easing. Up means a bigger number, so a falling layer (y growing) draws as a rising curve, as in After Effects |
| Insert / remove keyframe | `F6` / `Shift+F6` keys every property; right-click a property row in the timeline, or click the ◇ next to a property in the properties panel, to key just that one |
| Play / pause | `Enter` |
| Step frames | `,` and `.` (or ← →), `Home` / `End` |
| Pan / zoom | Scroll or Space-drag to pan, pinch or ⌘-scroll to zoom, `⌘0` to fit |
| Symbols | Select a layer and press `F8` (Modify ▸ Convert to Symbol). Use **Place** in the Library to add more instances; set each instance's playback (Loop, Play Once, Single Frame) and first frame in the properties panel. Double-click an instance (or **Edit** in the Library) to change the symbol; `Esc` or the breadcrumb bar takes you back out |
| Text | Text tool (`T`): click the stage and type. Click existing text with the Text tool, or double-click it with the Selection tool, to type into it; click or drag inside it to move the cursor or select, and press Esc or click elsewhere to finish. The usual text keys work (arrows, Shift to select, ⌥/Ctrl+arrows by word, ⌘C/⌘V), and so do input methods for Chinese, Japanese and Korean. The properties panel has a Text box too. Font, size, alignment and fill/stroke are in the same panel. File › Import Font… (or dropping `.ttf`/`.otf` files on the window) links font files to the project; they're listed first in the font menu |
| Images | File › Import to Stage (`⌘R`), or drag image files onto the window. Each lands on a new layer at the playhead, centered on the stage. **Actual size** in the properties panel resets its size to the file's own pixels |
| Audio | File › Import Audio adds a sound at the playhead (or drag sound files onto the window). Drag its waveform to re-time it; click the speaker to mute. Volume and start frame are in the properties panel's Audio section |
| Export | `⌘E` |

Shortcuts follow Adobe Animate where possible. On Windows, use Ctrl in
place of ⌘.

## Project layout

```
crates/
  inkframe-core/    document model, keyframes, easing, undo (no UI)
  inkframe-render/  draws documents to pixels (tiny-skia)
  inkframe-audio/   decodes, resamples and mixes sound files
  inkframe-export/  PNG sequences and ffmpeg video encoding
  inkframe-app/     the desktop editor (egui)
xtask/              build tooling: `cargo xtask icon`, `bundle-macos`, `bundle-windows`
packaging/          per-platform packaging files
assets/icon/        the app icon (drawn in code by xtask)
```

[ARCHITECTURE.md](ARCHITECTURE.md) explains how the pieces fit together and
where to start reading. [CONTRIBUTING.md](CONTRIBUTING.md) covers the
development workflow.

## Roadmap

Roughly in priority order:

- Signed and notarized macOS app (the unsigned `.app` and `.dmg` already
  build with `cargo xtask bundle-macos`)
- Code-signed Windows installer (the unsigned one already builds with
  `cargo xtask bundle-windows`)
- GPU stage preview, if very large scenes ever outgrow the CPU renderer
  (2,000 layers currently redraw in about 30 ms)

## License

Dual-licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option. This is the standard licensing for Rust projects. Unless
you state otherwise, any contribution you submit is dual-licensed the same way.

Inkframe includes [Noto Color Emoji](https://github.com/googlefonts/noto-emoji)
(`assets/fonts/`), © Google, under the
[SIL Open Font License 1.1](assets/fonts/Noto-COLRv1-LICENSE.txt), and the
[Inter](https://github.com/rsms/inter) typeface for its interface, © The
Inter Project Authors, under the same
[licence](assets/fonts/Inter-LICENSE.txt). The
installers also include ffmpeg (under its own licence; see
[packaging/macos/README.md](packaging/macos/README.md#licensing)) and
Rhubarb Lip Sync, © Daniel Wolf, MIT licensed, as separate programs.
