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
- **Symbols:** turn any layer, or several at once with their animation and
  rigs, into a reusable symbol (F8), as Animate does. Place it as many times as you like from the Library; each instance
  can be moved, scaled, tweened and offset in time, and changing the symbol
  changes them all. Double-click an instance to edit the symbol in place,
  right where it sits (even rotated or scaled), with the rest of the scene
  faded around it; a breadcrumb bar shows where you are. Symbols can contain
  other symbols.
- **Audio:** import WAV, MP3, AAC/M4A, FLAC, Ogg or AIFF files, see their
  waveforms in the timeline, slide them to the right frame, and hear them
  in sync while playing, or a frame at a time while scrubbing (for lip
  sync). Exports include the mixed sound.
- **Autosave and crash recovery.** While a project has unsaved changes,
  a copy is kept every 30 seconds (in the background, so drawing never
  stutters). If Inkframe doesn't close properly, the next start offers the
  work back, as the project it was, ready to save. Saving or quitting
  normally clears the copy.
- **Export** with or without transparency:

  | Format | Alpha | Use it for |
  |---|---|---|
  | PNG sequence | ✔ | Lossless frames for any tool |
  | Sprite sheet (PNG + JSON) | ✔ | Games: every frame packed onto one image (or, past 8192 × 8192, several numbered ones, as TexturePacker's multipack), cropped to the drawing, held frames packed once, with TexturePacker-style data for Phaser, PixiJS, Godot and Unity |
  | Apple ProRes 4444 / 4444 XQ | ✔ | Masters for editing and compositing |
  | Apple ProRes 422 HQ | — | High quality, smaller files |
  | H.264 MP4 | — | Review copies and the web |
  | HEVC with alpha (macOS) | ✔ | Small files with alpha for Apple devices and Safari |
  | Animated GIF | ✔ (on/off per pixel) | Sharing anywhere: loops forever, 256 colours |
  | SVG | ✔ | Vectors, one file per frame (or one for one frame), for the web, print and drawing programs |
  | Lottie (JSON) | ✔ | A vector animation that plays on web pages (lottie-web) and in iOS and Android apps |
  | Web Page (HTML) | ✔ | One self-contained page that plays the animation as vectors in any browser, as Animate's HTML5 Canvas export: it fits the window, loops, pauses on a click and steps with the arrow keys while paused. Nothing else to load. It can be interactive: any layer can be a **Button** (in its properties: Play, Stop, Go to a frame and play or stop, or Open a link), lighting up under the pointer, and Timeline › Frame Action puts Stop or Go to on a frame, as Animate's `stop()` and `gotoAndPlay()` (marked "a" on the ruler) |

  SVG and Lottie keep shapes as vectors, with gradients, line styles,
  masks, blend modes and symbols. A layer with filters, colour effects or
  particles goes in as pictures instead (in a Lottie file, one per frame,
  so a smaller size keeps it small). Lottie files play back exactly what
  Inkframe shows: every layer's placement (rigs, the camera, depth) is
  written for every frame.

  **File › Batch Export** queues several projects, or one project in
  several formats, and exports them one after another into one folder,
  each named after its project. A job that fails is marked and the rest
  carry on. The queue and folder are remembered for next time, and a
  project that has since moved is flagged. The same works without opening a window (see
  [Exporting from the command line](#exporting-from-the-command-line)).

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

New to Inkframe? [Getting started](docs/GETTING_STARTED.md) walks through
a first animation, and the [examples](examples), also in the app under
File › Open Example, show off the bigger features. *Inkframe for Artists*,
a beginner's guidebook with a chapter for each group of tools and
screenshots of every step, is built as a PDF with `cargo xtask guide`
(see [docs/guide](docs/guide/README.md)).

## Download

Installers for Apple Silicon Macs (`.dmg`) and 64-bit Windows
(`-setup.exe`) are on the
[releases page](https://github.com/WhateverTrevor/inkframe/releases), with
what's new in each in [CHANGELOG.md](CHANGELOG.md). They aren't signed with
a developer certificate yet, so macOS and Windows warn the first time; the
release notes say how to open Inkframe anyway.

## Getting started

You need [Rust](https://rustup.rs) (1.85 or newer) and, for video export,
[ffmpeg](https://ffmpeg.org):

```sh
brew install ffmpeg            # macOS; PNG sequences and sprite sheets work without it
git clone <this repository> inkframe && cd inkframe
cargo run --release -- examples/bouncing-ball.inkf
```

If ffmpeg is somewhere unusual, point Inkframe at it with the
`INKFRAME_FFMPEG` environment variable.

### Exporting from the command line

`inkframe --export` exports saved projects without opening a window, for
scripts and render machines. Every project is exported in every format
listed:

```sh
inkframe --export walk.inkf run.inkf --format h264,prores4444 --to renders
```

Formats are `png`, `sheet`, `prores4444`, `prores4444xq`, `prores422hq`,
`h264` and `hevc-alpha` (macOS). `--scale 0.5` exports at half size,
`--opaque` draws the stage colour instead of transparency, and `--help`
lists everything. It exits with 1 if any export failed.

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
| Type on the stage | Text tool (`T`), click to start, or double-click existing text with the Selection tool. `⌘Z` while typing undoes what's been typed a word at a time, `⌘⇧Z` redoes; once you click away, the whole edit is one step in Edit › Undo |
| Pick shapes inside a layer | Double-click a layer on the stage to step inside it, as with a symbol: the other layers fade, and a click picks just the shape under it (Shift-click adds, a box over empty stage picks what it touches, `⌘A` picks them all). Drag picked shapes to move them, drag a corner handle to scale them (Shift keeps proportions), drag the knob above them to turn them (Shift snaps to 15°), press Delete to remove them, or F8 to make a symbol of just them, which goes on a layer of its own just above, animated as the layer was. Copy, cut and paste work on picked shapes too: paste into another layer (step inside it, or just select it) and they land where they were on the stage, or with Paste in Center, in the middle of the view. Fills under the lines (colour art) are picked by clicking beside the lines. The layer itself isn't moved or keyed. Escape, or double-clicking empty stage, steps back out. Pictures and text move with their layer, so they can't be picked |
| Select several | Shift-click shapes, or drag a box over empty stage; Shift- or ⌘-click layer names in the timeline; `⌘A` selects all, `⌘⇧A` none. Dragging any of them moves them all, the handles around them scale and rotate them as a group, and delete, duplicate, copy/cut and F6 keyframes apply to every selected layer. The properties panel shows the last one picked, and changes there apply to them all: position moves them together, while rotation, scale, skew, opacity, depth, blend, the checkboxes, the frame span and the key diamonds are set the same on each. A • beside a property means the selected layers differ in it. Effects and easing aren't changed on the others as you go (each layer may have its own); press **Apply to the other selected layers** under them to give the rest the same |
| Scale | Drag a square handle. Shift keeps proportions, Option/Alt scales from the pivot |
| Rotate | Drag the round knob, or just outside a corner. Shift snaps to 15° |
| Skew | Drag an edge of the selection box between its handles to slant the layer, the opposite edge staying put; or type skew angles in the properties panel. Skew animates like any property |
| Move the pivot | Option/Alt-drag the ring in the middle of the selected layer. It snaps to the corners, edge midpoints and center; or type it in the properties panel. The layer stays where it is, and from then on turns and scales around the new spot |
| Attach to a parent | Pick a **Parent** in the properties panel. The layer stays where it is and from then on follows its parent; the timeline indents it under the parent's name. Deleting a parent hands its children to the parent above |
| Pose with bones (IK) | Bone tool (`K`) shows the rigs as bones between their pivots. Drag a layer and the joints above it turn so the spot you grabbed follows the pointer, keying their rotations on the current frame; the top of the rig (the root, drawn as a square) stays put, and dragging it moves the whole rig. To stop a joint bending the wrong way, tick **Bends** in its properties and set the lowest and highest rotation |
| Skeletons (Moho-style rigging) | Select a character's layers and choose Modify › New Skeleton: they go in a folder with a skeleton, and the **Add Bone** tool (`Shift+K`) is picked. Drag to draw a bone; the next one hangs from it, and starting on its end joins them, so an arm is a few drags from the shoulder out. Click empty stage to start a new root; drag a bone or its end to move it at rest; Delete removes the picked one. Then pose with the **Bone** tool (`K`): drag a bone and the bones above it turn so its end follows (Alt turns just that one), or drag a root's square start to move the whole character; every pose is keyed on the folder. Every layer in the folder bends with the bones near it, **painted layers and pictures included** (bent through a fine mesh), as do drawn shapes. In a layer's properties, **Follows** binds it to one bone instead (a hand, a hat), to a limb (**Thigh and below**: it bends with that bone and those hanging from it only, so legs drawn over each other each follow their own bones), or to none; a bone's **Strength** sets how far it reaches |
| Pins, smart bones and switches | **Pins:** pick shapes inside a layer (double-click it), or one point with the Subselection tool, and in the layer's properties **Pin to** a bone: they follow that bone alone, so an eye stays on the head while the face bends; with the Add Bone tool, the picked bone's pins are marked. **Switches:** Modify › New Switch (or tick **Switch** on a folder) shows one of the folder's layers at a time, a hand's poses or a mouth's shapes; pick which in **Showing**, keyed per frame. **Smart bones:** pick a bone and add **+ Reshape** (a layer morphs toward another of its drawings as the bone turns, all the way at the angle set: a corrected elbow, a smile on a dial bone) or **+ Switch** (a switch shows another layer once the bone's turned past an angle: a fist as the wrist bends). Pose the bone first and the new action takes its angle |
| Bone limits, stretch and dynamics | In a picked bone's properties: **Limits** keep it within a range of turns (IK stops there and the bones above make up the rest; the Bone tool shows the range as a wedge). **Stretch** makes it longer or shorter, keyed (or Shift-drag it with the Bone tool); **Keep area** makes it thinner as it stretches and fatter as it squashes. **Dynamics** make it swing after the bones it hangs from, overshoot and settle, for hair, tails and antennae, with **Spring**, **Damping** and how far it may **Swing** |
| Welded points | Where shapes in a layer meet at a point (an outline and its sleeve, a roof on a wall), that point is shared, marked with a dot by the Subselection tool and the Pen. Dragging it moves every shape's copy, so they stay joined; drag a point onto another shape's point and it snaps there and welds; ⌘-drag pulls a point away on its own. Pinning a point to a bone pins the points welded to it too, so joined shapes don't crack apart as a skeleton bends them |
| Rig setup, and painting on poses | View › Rig Setup (`⌥⌘B`) shows every skeleton at rest, as Moho's frame 0: draw, paint, pin and move bones there (the Bone tool moves them at rest too, and Add Bone always works at rest). Off, everything is posed, and the paint tools, the Paint Bucket and shapes drawn into a layer all land where you see them on the bent drawing, so you can touch up a pose as it is. Exports always show the poses |
| Bones panel | Window › (any area's menu) › Bones lists every skeleton in the timeline: its bones as a tree, marked smart, swings, limited or stretches, with each one's turn on this frame (click one to pick it), and the layers it rigs, with what each follows and how many points are pinned. Rig setup switches on and off here too |
| Target bones | Give a bone a **Target** (in the picked bone's properties) and its end reaches for that bone's start, the bones above it (up to two, short of the root) bending to get it there, as Moho's target bones do. Plant feet on target bones and the legs bend to keep them there as the hips move; move a target to step. Limits on the shins keep the knees bending the right way. The Bone tool draws a dotted line to each target |
| Art and pattern brushes | Any stroke can be drawn with a library symbol, as Animate's Paint Brush art and pattern brushes are: in the stroke settings, set **Brush** to **Art** (the symbol's picture stretched along the whole line: tapered strokes, ribbons) or **Pattern** (repeated along it: vines, chains, stitches), and pick the symbol. The picture bends with the line round curves and corners and spans its width. The line keeps its points, so it edits as any other; SVG and Lottie export it as drawn |
| Line textures | In a vector layer's properties, **Lines** inks its pencil lines and strokes with a paper grain, as Harmony's line textures: Pencil (fine), Charcoal (streaky) or Chalk (speckled), with how strongly it shows. The grain is fixed to the layer, so it moves with the drawing rather than crawling over it; fills stay flat |
| Draw inbetweens | Timeline › Draw Inbetweens, with the playhead between a key drawing and the next one it swaps to: a new drawing on every frame between, each the key drawing morphed that far toward the next (using the shape hints, and spaced by the span's shape tween easing if it has one, so ease in and out to slow into the keys). They're ordinary drawings, so touch each one up by hand, as with an assistant's inbetweens |
| Action timeline | A smart bone's actions on a timeline of their own, as Moho's, at the top of its Smart bone section: a strip across the bone's turns. Drag along it to preview the bone at any turn, with everything its actions do, keying nothing (the context bar's **Back to the animation**, Esc, another frame or another bone puts the animation back). Each action has a lane: drag a reshape's or switch's handle to change its turn, and a correction's keys (diamonds) to rekey them; double-click a key to redraw it, right-click to remove it. Edit at …° keys a correction at the previewed turn, and scrubbing on keys what was drawn |
| Smart bone corrections | Moho's smart bone actions: pick a bone, turn it, and click **+ Correction** (or **Edit at …°** on one already there). The layer's drawing is now yours to reshape with any tool, as it should look at that turn: a bulging bicep, a clean elbow, a smile. **Finish** (or Enter) keys it there; Esc cancels. Key as many turns as you like, either way round; between them, and back to rest, the drawing morphs smoothly. Each key shows on the action timeline, to move or take out. Saving or exporting mid-correction keys it first |
| Bend a drawing | Select the layer and click **Add Bend Deformer** in the properties panel: bones are laid along the drawing's long side (set how many, and how soft the bends are). With the Bone tool, drag a green joint to bend the drawing there; bends are keyed on the current frame and tween like anything else, or type them in the panel. Shapes bend; images and text stay as they are |
| Exposure sheet | Timeline › Exposure Sheet. One row per frame, one column per layer that swaps drawings, plus each sound's loudness. A name marks where a drawing starts and a line shows it held. Click a cell to go there; double-click and type a drawing's name, then Enter, to expose it and move down a row (a new name makes a new, blank drawing); right-click for a list of drawings, or to hold the one above |
| Paint | Brush (`B`) and Eraser (`E`) paint on the selected paint layer; if the selected layer isn't one, a stroke starts a new one (or use Modify › New Paint Layer). The brush's settings show in the properties panel while it's in use, under a preview stroke that tapers as a pen would; `[` and `]` shrink and grow it. With a tablet, pressing harder paints a thicker line (or a denser one: Pressure › Flow). Eyedropper (`I`), or Alt/Option-click with the brush, picks a colour from the stage. A paint layer's canvas is the stage's size, pixel for pixel; it moves, scales and turns like any layer. For frame-by-frame painting, give it a new drawing on each frame (Modify › New Drawing) |
| Fill, retouch | Paint Bucket and Gradient share a toolbar slot (`G`; `Shift+G` switches, or right-click the slot): the bucket fills the area of similar colour you click (Tolerance, Contiguous), the gradient blends from the paint colour to a second colour (or to transparent) along the line you drag, linear or radial. Smudge, Blur and Sharpen share `U`; Dodge and Burn share `D` (Range picks shadows, midtones or highlights). Clone Stamp (`S`): Option/Alt-click where to copy from, then paint; later strokes keep the same distance to the source, so a picture can be copied over in several strokes |
| Select pixels | Marquee (`M`, rectangle or ellipse), Lasso (`L`, freehand or polygonal: click corners, then click the first, double-click or press Enter) and Magic Wand (`W`, Tolerance and Contiguous in its options). Shift adds to the selection, Option/Alt takes away, both keep the overlap; a click with the marquee deselects. Every painting tool, fill and adjustment then changes only what's selected. With a painting or selection tool, `⌘A` selects all pixels, `⌘⇧A` deselects, `⌘⇧I` inverts, and Delete clears what's selected |
| Move, transform pixels | Drag inside a selection with the Selection tool (`V`) to move those pixels. Select › Layer via Copy (`⌘J`) or via Cut (`⌘⇧J`) puts them on a layer of their own, to scale, rotate and skew with its handles; Modify › Merge Down paints a layer into the paint layer beneath (which also turns vector drawing into pixels) |
| Adjust, blend | Modify › Adjustments: Brightness/Contrast, Hue/Saturation, Levels or Curves on the selected paint layer, previewed live (Curves: click to add points, drag them, right-click to remove). Any layer's Blend menu (in its properties) sets how it mixes with what's beneath: Multiply for shading, Screen for glows, and the rest of Photoshop's modes |
| Turn the view | Shift+Space and drag turns the canvas view around its middle, to draw at a comfortable angle, as on paper; `4` and `6` turn it 15° either way and `5` puts it upright (also in the View menu). Only the view turns, not the drawing |
| Photoshop files | File › Export PSD… saves the current frame as a layered .psd: each layer with its name, opacity, visibility and blend mode, over a Background layer. File › Import to Stage (or dropping a .psd on the window) brings each layer of an 8-bit RGB Photoshop file in as a paint layer; groups are flattened, and adjustment layers, masks and effects are skipped |
| Frame commands | Shift-click or Shift-drag along a layer's row to pick frames (on every selected layer). As in Animate: `F5` inserts frames (as many as are picked, or one), holding what shows there longer; `Shift+F5` removes them, bringing what follows earlier; `F7` puts a blank drawing on the current frame (or on every picked one). Timeline › Convert to Keyframes gives each picked frame a copy of its drawing and keys any tween through it, so each can be changed by itself; Convert to Blank Keyframes gives each an empty drawing; Reverse Frames plays the picked frames backwards, ease-ins turning into ease-outs. The timeline grows if frames go past its end |
| Edit multiple frames | The frames button by the onion skin (or Timeline › Edit Multiple Frames): dragging layers on the stage then moves them across the picked frames (or the whole timeline, with none picked), not just the current one. An animated layer's position keys there all move together; a layer drawn frame by frame has every drawing in the range moved. A blue band above the frames shows the range |
| Restack layers | Press and hold a layer's name for a moment until it lifts, then drag it up or down; a line shows where it will land. The ↑ and ↓ buttons move the selected layer one place |
| Scroll the layers | Drag a layer's name up or down straight away (handy with a pen, which has no scroll wheel); a plain click still selects it |
| Folders | Modify › New Folder (or the folder button at the right of the timeline bar, or right-click › New Folder) puts the selected layers in a folder. Drop a layer just under a folder's name, or between layers in it, to put it inside; right-click › Remove from Folder takes it out. Click the folder's triangle to close it up. Its eye and lock show, hide, lock and unlock everything in it; deleting, copying or moving it takes its contents along |
| Layer colours | Each layer has a colour, shown down the left of its row, along its bar and around it on the stage when it's selected. Click the small square at the right of a layer's name (or right-click › Layer Colour) to choose another. Each row also shows a thumbnail of the layer on the current frame |
| Arrange the window | Like Blender: every part of the window is an area, and each area holds tabs. Click the showing tab for a menu of every editor (in groups: Stage, Tools, Tool Settings and Properties; Timeline, Graph Editor and Exposure Sheet; Swatches and Align; Library, Audio, Document and History), **+** to add a tab, middle-click or right-click › Close Tab to close one, and drag a tab onto another area's header to move it there. Drag the border between areas to resize them. Drag from an area's corner into it to split it (across for side by side, down for one above the other), into its neighbour to join them, or with Ctrl onto any area to swap them. Right-click a header for the same, and Ctrl+Space fills the window with the area under the pointer. The layout is remembered between sessions |
| Workspaces | The menu at the top right (or View › Workspace) switches between ready-made arrangements: Essentials, Animator (a tall timeline with the graph editor and exposure sheet as tabs), Designer (Swatches and Align beside Properties) and Painter (tool settings, swatches and history up front). Reset puts the current one back as it started |
| Properties | Three tabs: Tool (the current tool's settings, including fill, stroke and line width for the drawing tools), Layer and Document. Picking a layer or a painting or selection tool brings its tab forward. Each section (Transform, Easing, Effects, Drawings, Smart bone…) has a title with an icon: click it to fold the section away, or open it again; what's folded is remembered for next time |
| Free-form deformer | In a layer's properties, **Add 3×3 Grid** (or 4×4) puts a grid of points over the drawing. Drag them with the Bone tool (`K`) to squash, stretch, bulge or sway it smoothly; each drag keys that point on the current frame, so the warp animates. **Straighten** puts the grid back on this frame. It works on top of a bend deformer |
| Templates | Open a Templates panel (from an area's menu, under Project). Select layers (a rigged character, a walk cycle, a prop; a folder brings everything in it), type a name and click **Save Selected**: they're saved with their animation, symbols, pictures and paint to Inkframe's own Templates folder, for every project. Double-click a template (or **Insert**) to add it at the playhead: its animation starts there. Right-click one to delete it |
| Node View | Choose **Node View** from an area's menu to see the layers as a graph, as in Harmony: each layer's transform output feeds the Parent input of the layers attached to it, a mask feeds the Mask inputs it cuts, and each layer's image runs through its colour effect and filters into the Composite (in stacking order) and on to the Display. Drag from an output to an input to rewire: attaching a layer, making one a mask of another, or dropping a layer into a different place in the Composite to restack it. Right-click an input to unplug it, a layer to add a filter, a filter to switch it off or remove it. Drag nodes to arrange them, empty space to pan, pinch or ⌘-scroll to zoom; Where you put the nodes is saved with the project, so the view opens as you left it; **Arrange** lays it all out again. It's the same scene as the timeline shows, so the two always agree |
| Master controllers | Open a Controllers panel (from any area's menu, under Animation). Select the layers a pose involves (a face's features, a hand's fingers) and click **New Controller from Selected Layers**: their current pose is recorded at the slider's start. Move the slider to the other end, pose the layers differently on the stage, and **Record Pose** there; add more poses in between if you like. Then moving the slider blends the layers between the poses (drawings switch at the halfway point), keying them on the current frame, so set it on one frame, again on another, and the pose animates. Click a pose's name to jump to it |
| Light table | View › Light Table, or the lamp button in the timeline bar: every layer but the selected one fades, so the drawing you're working on stands out, as in Harmony |
| Particles | Modify › New Particle Layer adds Sparks, Snow, Smoke or Fire. A particle layer sprays particles from its emitter (a point, line, rectangle or circle), and its Properties set the rate, direction and spread, speed, gravity, wind, drag and wander, and how size, opacity and colour change over each particle's life, with spin. Particles are dots with a soft edge, or copies of whatever is drawn in the layer. Move or animate the layer and the emitter leaves a trail; particles stay in the scene as the camera moves. Every frame is worked out on its own, so scrubbing, playing backwards and exporting all match, and Seed › New gives another spray just like it |
| Scripting | Scripts are JavaScript files that change projects for you: build scenes, rename or retime many layers at once, export with your own file names. Run them from the **Scripts** menu, which comes with six example scripts, try them out in the **Script Console** area, or use `inkframe --script` on the command line. A script's changes undo in one step, a failing one changes nothing, and scripts can't reach files beyond the projects and exports they name. See [docs/SCRIPTING.md](docs/SCRIPTING.md) |
| Vectorise | Modify › Vectorise… traces the selected paint or image layer into vector shapes, as Animate's Trace Bitmap does. **Line art** keeps pixels darker than the threshold, so a scanned drawing becomes filled ink lines ready for the paint bucket; **Colours** finds the picture's main colours and makes each its own shapes. Minimum area drops specks and fills pinholes, Smoothing trades closeness for fewer points, and a preview updates as you go. Every drawing of the layer is traced into a new layer above it, lined up and animated the same, and the original is hidden rather than deleted |
| Safe areas, field guide | View › Safe Areas shows action-safe and title-safe frames over the camera's view: Current (93% / 90%, the SMPTE HD figures) or Classic (90% / 80%). View › Field Guide lays a 12-field chart over it, fields 1F–12F with a grid at every field step and the N/E/S/W axes, for planning camera moves in fields. Neither shows in exports. Both are remembered for next time |
| Ink and paint | Strokes go into the selected layer's drawing; choose **Line Art** or **Colour Art** (which sits beneath the lines) in the Pencil, Pen, Rectangle or Oval tool's settings (Properties › Tool). Then, with the Paint Bucket on that layer, click between the lines to fill the area they enclose with the paint colour; the fill runs under the lines, and **Close gaps** (Small, Medium, Large) lets lines that don't quite meet still hold the paint. Click a fill again to recolour it; fill an area inside a filled one (an eye in a face) to give it its own colour |
| Swatches, palettes | Click a colour to use it: the paint colour for the paint tools; otherwise the fill (or, with Stroke chosen, the stroke) for new shapes and for the selected layers' shapes. **+** adds the current colour to the document's palette; name it in its row. A palette colour owns everything painted with it: right-click it and change the colour, and every shape of that colour changes too, as in Harmony. **New Version** in the palette menu copies the palette (for a night version, say); recolour its swatches, and switching versions recolours the whole scene |
| Align | Line the selected layers up by their edges or centres, space them evenly, or make them the same width or height. One layer, or **Align to stage**, lines up with the stage |
| History | Every change so far, named for what it did (“Add “Ball””, “Paint on “Sky””). Click a step to go back to it, or a greyed one to go forward again |
| Swap drawings | In the properties panel's **Drawings** section, **New** adds a blank drawing (or **Duplicate** a copy of the one showing) from the current frame on. Click a drawing's name to show it from the current frame on, or press `[` and `]` to flip through them. With a layer that has several drawings selected, new shapes are drawn into the drawing that's showing. The timeline marks each swap with the drawing's name |
| Camera | Pick the Camera tool (`C`) and drag on the stage: the first drag adds a camera, and dragging pans it (the scene follows the pointer); Shift-drag turns it and Alt-drag zooms. Or use Timeline › Add Camera, or the camera button at the right of the timeline bar. The camera is a layer at the top of the timeline, so move to another frame and pan again to animate it, with tweens and easing like any layer. Select it to type where it looks, its zoom and rotation, or Reset it. Give layers a **Depth** (in their properties) for parallax: deeper layers look smaller and move less as the camera pans. Hide the camera layer to turn it off |
| 3D multiplane | Depth animates like any property, with keys, tweens and its own row and curve in the timeline, so layers can drift nearer or further. The camera's **Dolly** moves it in among the layers: near ones grow faster than far ones, as with a real multiplane camera (zoom can't do that), and layers it reaches fade away. The **Depth View** (an area kind, under Animation) shows the scene from above: each layer a line at its depth, the camera and what it sees. Drag a layer up or down to set its depth, or the camera to dolly and pan; scroll to zoom the view, and Fit to see everything. Tick **Stack by depth** (in the Depth View or the Document properties) and depth also decides what's in front: layers are drawn far to near on each frame, so a layer drifting back passes behind the others (layers at the same depth keep the timeline's order, and a rig's parts stay with it); the timeline bar shows **Stacked by depth** while it's on, and clicking that turns it off. Off by default, so older scenes look as they always have |
| Colour effects, filters | In a layer's properties: **Colour Effect** sets Brightness, Tint (a colour and how much) or Advanced (each channel multiplied and offset); **Filters › Add Filter** adds a Drop Shadow, Blur, Glow (outer or inner), Adjust Color (brightness, contrast, saturation, hue) or Motion Blur, which smears a moving layer along its path as a film camera would (Shutter 180° is the usual film look; fast movement automatically gets more steps so the smear stays smooth). Filters apply in order (↑ moves one earlier), can be switched off with their checkbox, and show in exports. Effects animate like the transform: change them on another frame and they tween, a filter added on a later key growing in from nothing; the ◇ beside **Effects** keys them, and they have their own row under an expanded layer in the timeline. Layers with effects are labelled "fx" |
| Shape tweens | Morph one drawing into the next, as Animate's shape tweens do. Give a layer a second drawing from a later frame (**Duplicate** in the Drawings section, then change it), then on any frame before the swap choose Timeline › Create Shape Tween (or right-click the frame, or tick **Shape tween into the next drawing** in the Drawings section). The span turns green in the timeline. Rectangles, ellipses and paths all morph, colours and line widths blend, and shapes with no partner shrink away. Pick an easing in the Drawings section. If the morph twists, add shape hints (Modify › Add Shape Hint, ⌘⇧H): drag hint **a** to a point on the first drawing (on the tween's first frame) and to the matching point on the second (on the swap's frame), and those points will meet. Right-click a hint to remove it |
| Lip sync | Give a mouth layer a drawing per mouth shape, named with Rhubarb's letters (A closed, B, C, D wide open, E, F, G, H, X at rest) or Papagayo's (MBP, etc, E, AI, O, U, FV, L, rest); a few are enough, the rest borrow a similar shape's. Import the dialogue, select the mouth, then Modify › Lip Sync…: pick the sound and where the shapes come from (Rhubarb, which listens for the sounds and takes a few seconds; loudness; or a timing file), check the drawings, and Apply. For Rhubarb, typing what's said helps with unclear words, and "Other" language listens for sounds rather than English words. The swaps land under the sound, replacing any there |
| Combine shapes | Double-click a layer, pick two or more of its shapes, and choose Modify › Combine Shapes: **Union** merges them into one (styled as the top one), **Intersect** keeps only where they all overlap, **Punch** cuts the top shape out of each one beneath, and **Crop** trims each one beneath to the top shape. Curves stay curves and corners stay sharp. A result with a hole and an outline keeps its fill and its outlines as separate pieces, as Animate does. **Break Apart** (`⌘B`) turns text into a shape for each letter, and a pencil line into the filled shape it draws |
| Gradients, line styles | In a shape's style (Properties › Layer for a shape, or Properties › Tool for new ones), Fill can be **Solid**, **Linear** or **Radial**. A gradient lists its colours, each at a place along it: add one with **+ Colour**, take one away with ×, or **Reverse** them. To move where it starts and ends, double-click the shape on the stage and click it: a round handle marks the start (a radial gradient's middle) and a square one the end. The gradient goes with the shape when it's moved or resized. Strokes have **Ends** (round, flat, square), **Corners** (round, sharp, bevelled) and **Line** (solid, dashed with dash and gap lengths, or dotted) |
| Draw | Rectangle (`R`), Oval (`O`), Pencil (`Y`). Hold Shift for squares and circles. What you draw goes into the selected layer, as in Animate; with nothing selected (or a locked layer, a symbol or a paint layer), it starts a new one. To give every shape a layer of its own instead, untick **Draw into the selected layer** in the tool's settings (remembered between runs) |
| New layer | **+** in the timeline (or Modify › New Layer, `⌘⇧N`) adds an empty layer just above the selected one, in the same folder, ready to draw into |
| Pressure lines | The Pencil follows a pen's pressure (press harder for a thicker line) and tapers its lines' ends; both are options in its settings, and with both off it draws plain strokes. Shape a line afterwards with the **Width** tool (`Shift+A` from the Subselection tool): drag sideways from any point to make the line wider or narrower there. A line's width can be scaled as a whole in its properties |
| Reshape | Subselection tool (`A`): drag a shape's points, or a selected point's handles. Alt/Option moves one handle alone for a sharp corner; Alt/Option-drag a point to pull new curve handles out of it, as in Animate; Alt/Option-click a point to switch it between corner and smooth; Delete removes the point. Rectangles and ovals become editable paths on the first edit |
| Pen (`P`) | Click for corners, drag for curves. Click the first point to close the shape; Enter, Esc or clicking the last point ends an open line. Backspace removes the last point, Shift snaps to 45°. Click a selected shape's outline to add a point without changing its curve. Between paths, the Pen edits the selected shape's points as the Subselection tool does: click one to show its handles, drag it or its handles, Alt/Option-drag it to pull out curve handles (turning a corner into a curve), Alt/Option-click it to switch corner and smooth |
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
| Symbols | Select a layer, or several (their rigs, masks and animation come with them), and press `F8` (Modify ▸ Convert to Symbol); the layers become one instance on the timeline. Double-click it to step inside and work on each layer on its own. The instance's **Symbol length** sets how long the symbol's own timeline is: on Loop it repeats every that many frames, so a 12-frame walk cycle keeps walking for as long as the instance lasts. Drag it from the Library to add more instances; set each instance's playback (Loop, Play Once, Single Frame) and first frame in the properties panel. Double-click an instance (or its card in the Library) to change the symbol; `Esc` or the breadcrumb bar takes you back out |
| Text | Text tool (`T`): click the stage and type. Click existing text with the Text tool, or double-click it with the Selection tool, to type into it; click or drag inside it to move the cursor or select, and press Esc or click elsewhere to finish. The usual text keys work (arrows, Shift to select, ⌥/Ctrl+arrows by word, ⌘C/⌘V), and so do input methods for Chinese, Japanese and Korean. The properties panel has a Text box too. Font, size, alignment and fill/stroke are in the same panel. File › Import Font… (or dropping `.ttf`/`.otf` files on the window) links font files to the project; they're listed first in the font menu |
| Distort and Envelope | Modify › Transform › Distort puts a handle on each corner of the selected layer's drawing (or of the shapes picked inside a layer); drag them to lean, taper or put it into perspective. Envelope puts a point on each corner and in the middle of each side, each with handles that shape the curve either side of it, as the pen's do: drag a middle out to bow a side, or a handle to swing the curve; Alt-drag a middle's handle to kink the side there. Straight edges curve smoothly and pencil lines keep their widths. Enter, Escape or clicking away from the handles finishes |
| Perspective guides | View › Perspective Guide lays a one-, two- or three-point perspective over the stage, as Harmony's: a horizon and lines radiating from each vanishing point. Drag the points (the two on the horizon stay level). With the guide showing, Shift with the Pen snaps each new segment toward a vanishing point, or straight across or up where the perspective has those. Saved with the project |
| Finding things | **Above the stage**, a bar shows the current tool, with a tip on hover, and buttons for what can be done right now: Symbol, Distort, Envelope and Break Apart with a layer selected; Combine with shapes picked inside a layer; New Skeleton, Add Bones and Pose with the bone tools; Finish and Cancel while a Distort or a correction is under way. Its right end switches rulers, the perspective guide, onion skins, the light table and rig setup. The first time Inkframe opens, a short **tour** points out each part of the window in turn (Help › Take the Tour shows it again). **Find a Command** (`⌘K`, or the search box in the menu bar) finds any command or tool by name or by what it's for ("bulge", "rig", "web page"), and shows which menu it's in and its keys. Hover a tool in the toolbar to see what it does and what else shares its slot |
| Rulers and guides | View › Rulers (`⌥⇧⌘R`) shows rulers along the top and left of the stage, in stage pixels, the pointer's place marked on both. Drag down from the top ruler (or right from the left one) to put a guide on the stage; drag a guide to move it, or back onto a ruler to take it away. Guides are saved with the project. View › Guides: Show (`⌘;`), Lock (`⌥⌘;`), Snap to Guides (`⇧⌘;`) and Clear. Moving layers snaps an edge or their middle onto a guide they come near, and so do the corners of rectangles and ovals as they're drawn. With the view turned, the rulers hide; the guides turn with the stage |
| Motion Presets | Choose **Motion Presets** from an area's menu (under Animation). Pick a movement, select layers, and **Apply**: from the playhead, each layer does it starting from wherever it is, so a Slide In ends where the layer already sits. Built in: Fade In, Fade Out, Pop In, Slide In from Left or Right, Bounce In, Spin, Pulse, Shake and Float; each shows a square doing it. **Save Movement** keeps the selected layer's animation (the frames picked in the timeline, or all its keys) as a preset of your own, available in every project; easing and curved paths come along. A preset replaces the layer's keys over its frames only for what it animates |
| Swap Symbol | Modify › Swap Symbol… (or **Swap…** in an instance's properties) shows every symbol as a picture: pick one and Swap, and the selected instances show it instead, keeping their animation, timing and playback. Swap for the **whole layer**, or **from the current frame on**, to change a hand's pose or a head's turn partway through a shot while the rig keeps moving; the timeline marks each swap with the symbol's name. **Duplicate Symbol** copies the chosen one first, to swap to and change without changing the original |
| Library | Symbols, imported images and sounds, as cards with a picture of each. Drag a card onto the stage to place it right there, or onto the timeline to place it from that frame, just above that layer (a sound starts on that frame). Double-click a symbol's card to edit it, or any other card to place it mid-stage. Right-click a card to rename, move or delete it (only once nothing on the stage uses it). The folder button makes a folder: drag cards onto it (or onto a card inside it) to file them, onto empty space to take them out, and right-click it to rename or delete it. Search finds cards by name, inside folders too, and the slider sets the card size. **Preview**, at the top, shows the card last clicked: a symbol plays in a loop (pause it, or drag the slider to step through its frames), an image shows larger with its size, and a sound shows its waveform with a button to hear it on its own. Hovering an animated symbol's card plays it in the card. Image files dropped onto the Library from Finder or Explorer are added to it without being placed |
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
  inkframe-export/  PNG sequences, sprite sheets, ffmpeg video and GIF, SVG and Lottie
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
