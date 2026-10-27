# Changelog

What changed in each release of Inkframe. The newest is first. A release's
notes on GitHub are its section here.

## [0.8.0]

A painted example to start from, brush tips from your own pictures,
symmetry around any point and a Mixer Brush that samples every layer.
Skeleton characters can be made symbols, so a walk cycle loops, and
scripts make titles that draw whole. Underneath, the biggest source files
are split up to be easier to work on. Projects open in 0.7.0 and 0.6.0
too.

### Brushes

- A new example, Painted Landscape (File › Open Example…): a sunset
  painted with the ready-made brushes, the sky, clouds, mountains, hills,
  trees and grass each on its own paint layer, the clouds drifting.
- Import Tip… in the Brushes panel makes a brush tip from a picture file,
  dark as paint.
- Symmetry's centre can be dragged by its handle on the stage, and put
  back in the middle from the Symmetry menu.
- Sample All Layers for the Mixer Brush: it picks up colour from
  everything as seen, not only its own layer.

### Fixes

- A skeleton folder can be made a symbol (F8, or a script's
  `convertToSymbol`): the folder goes inside with its layers and bones,
  so the character still bends, and the instance keeps the folder's name.
  Before, the bones were left behind.
- Text added by a script, then exported or saved from the command line,
  was cut off or drawn as nothing: it's now measured, as the app does.
- Scripts can centre or right-align text (`align` in `addLayer`).

### For contributors

- The biggest source files are split into modules named for what they do:
  the stage's tools and overlays live in `stage/`, the Properties panel's
  sections in `inspector/`, and the app's menus, keys, commands, files and
  areas in `app/`. Nothing changes for people using Inkframe.

## [0.7.0]

More of Photoshop's painting: the Mixer Brush, the Pattern Stamp,
symmetry, Brush Pose and textures from your own pictures, with Photoshop
brushes bringing their patterns. Smudge, Blur and the other retouching
tools keep up at big sizes, groups slant together, and long sound files
load lighter. Projects open in 0.6.0, 0.5.1 and 0.4.0 too.

### Brushes

- The Mixer Brush, Photoshop's wet brush, shares the Brush's slot
  (Shift+B): it carries a reservoir of paint that mixes with the colours it
  passes (Mix), smears them (Wet), and runs dry over a stroke unless it's
  loaded with plenty (Load).
- Textures from pictures: import any picture as a brush's pattern, or make
  one from selected pixels with Edit › Define Pattern, light raised and
  dark hollow. Photoshop `.abr` files bring their own patterns, so
  imported brushes keep their textures.
- Define Brush Preset, Define Pattern, Brushes and Brush Settings are in
  Find a Command.
- Symmetry painting, from the context bar: mirrored across the canvas's
  middle (up and down, across, or both), or repeated round it (Radial, or
  Mandala, mirrored in each segment), with its lines shown on the stage.
  The copies are part of the one stroke, so they share its opacity.
- Brush Pose, a new Brush Settings section: a pen's tilt, turn and
  pressure for painting with a mouse, or to override the pen's.
- The Pattern Stamp shares the Clone Stamp's slot (Shift+S): it paints a
  pattern with the whole brush engine, its dark parts in the paint colour
  and its light in a second colour.

### Moving and transforming

- Several selected layers can be slanted together, by dragging an edge of
  their shared box, as one layer is.

### Sound

- Long sound files load quicker and take less memory: only as much as the
  animation can play is decoded (and a minute more), and more when the
  animation grows. A ten-minute song under a 30-second shot no longer
  holds all ten minutes in memory.

### Speed

- Smudge, Blur, Sharpen, Dodge, Burn and Clone Stamp keep up with big
  brushes: each dab's square of pixels is worked in bands across every
  processor core, changed in place rather than copied out and back, and
  blurred with running sums. At 2500 pixels the slowest moment of a
  stroke went from up to 130 ms (Blur) to about 12 ms.

## [0.6.0]

Photoshop's brushes, setting for setting, with brushes for painting
scenes and backgrounds and Photoshop brush import; the whole interface
ready to translate, with a language setting; and big brushes that keep up
with the pen. Projects open in 0.5.1, 0.5.0 and 0.4.0 too.

### Brushes

- Photoshop's brush engine, setting for setting. Brush Settings edits a
  brush a section at a time: Brush Tip Shape (round or sampled tips, angle,
  roundness, flips, hardness, spacing), Shape Dynamics, Scattering,
  Texture, Dual Brush, Color Dynamics and Transfer, each with jitters and
  controls (pen pressure, pen tilt, fade, direction), plus Noise, Wet Edges
  and Build-up, with a live sample stroke.
- A Brushes panel of ready-made brushes for painting scenes and
  backgrounds (pencil, chalk, charcoal, bristle and flat brushes,
  watercolour, foliage, grass, clouds, rocks, spatter, stars) and your own:
  save the brush as a preset, rename, update and delete them, export them
  as a brush set and import brush sets or Photoshop `.abr` files.
- Define Brush Preset (Edit menu, or the Brushes panel) makes a tip from
  the selected pixels of a paint layer.
- A brush quick menu: right-click the stage with a painting tool for the
  size, hardness, opacity, flow and the brushes. Ctrl+Option-drag
  (Alt+right-drag on Windows) on the canvas resizes and softens the brush,
  and Shift+[ and Shift+] soften and harden it, as in Photoshop.
- The Tool tab's pressure switches, as in Photoshop's options bar:
  pressure for size and for opacity. The brush and eraser are remembered
  between sessions. The Painter workspace has the new panels.

### Translation

- The groundwork for translating Inkframe: the menus, the tools, the
  context bar, the timeline, Properties, Find a Command, the Library, the
  dialogs and panels, the window's areas, the first-run tour and What's
  New can be translated with a plain text file per language in
  `locales/` (see docs/TRANSLATING.md), chosen from the system's language
  or `INKFRAME_LANGUAGE`. `cargo xtask strings` lists every text to
  translate, and a pseudo-language shows what's been wrapped so far. The
  area menu now widens to fit longer names. Find a Command and the
  Properties search find things by their translated words and their
  English ones. View › Language picks the language Inkframe starts in,
  or follows the system's. The names of undo steps and new layers are
  translated too.

### Speed

- Drawing a frame shares each skeleton's pose and each bent layer's
  shapes across every part of the picture and every processor core,
  rather than working them out again for each, and the stage's picture is
  handed to the screen in one copy: on a 500-layer scene with rigged
  characters, drawing is about 20% quicker at the stage's size and the
  editor's updates about 7%.
- Big brushes keep up with the pen: a dab's tiles are painted and drawn
  on every processor core, and a texture is worked out once a stroke
  rather than at every redraw. The slowest moment of a 2500-pixel
  textured stroke went from about 100 ms to under 10 ms.

### Scripting

- A script's result shows the moment it finishes. A script run from the
  Script Console used to wait for the mouse to move before its changes
  and output appeared, and one from the Scripts menu up to a tenth of a
  second.

## [0.5.1]

What's new after an update, scripted Rhubarb lip sync, faster particles,
and a guidebook chapter on scripting. Projects open in 0.5.0 and 0.4.0
too.

### What's New

- After an update, a window shows what changed in the new version, from
  these notes; Help › What's New shows it again. Someone new gets the
  first-run tour instead.

### History

- A script's changes are one undo step named after it ("Run Script
  “walk-cycle”"), and Edit › Undo and Redo say what they'll undo and redo.

### Getting started

- The getting started guide is now a one-page quick start, pointing on to
  the guidebook for everything else; Help › Quick Start opens it.

### Speed

- Particles draw faster: a soft dot is filled as a square of its gradient,
  which fades to nothing at the edge anyway, rather than as a smoothed
  circle. A 500-layer scene with fire and snow draws about 20% faster.

### Scripting

- `lipSync` can have Rhubarb Lip Sync listen to the dialogue, as the Lip
  Sync window does: `mouth.lipSync("line", { rhubarb: true })`, with a
  `language` and the `dialogue` said.

### Guidebook

- A chapter on scripting: running the example scripts, a first script in
  the Script Console, and scripting rigs, mouths and web buttons.

### Timeline

- Cut, Copy and Paste Frames are on a row's right-click menu too, and
  Paste Frames only ever pastes copied frames (not layers copied since).

## [0.5.0]

Copying frames, scripting characters, and a quicker editor. Projects
saved by 0.5.0 open in 0.4.0 too.

### Timeline

- Cut, Copy and Paste Frames, as in Animate: with frames picked in the
  timeline, ⌘X, ⌘C and ⌘V cut, copy and paste their keyframes, pasted at
  the playhead onto the selected layers (also in the Timeline menu). On a
  layer drawn frame by frame, its drawings and swaps come along too. The
  pose commands (⌥⌘C, ⌥⌘V) are now called Copy Pose and Paste Pose.

### Properties search

- A search box at the top of the Layer tab: type a setting ("blur",
  "pivot", "dash") and only the sections that have it show, opened, until
  it's cleared.

### Speed

- Characters on skeletons draw faster: each skeleton's pose is worked out
  once a frame rather than for every layer it bends, which made a
  500-layer scene with 24 rigged walkers about 30% quicker to draw, and
  scrubbing it with a layer selected five times quicker.
- Onion skins draw each ghost frame once and keep it while nothing
  changes, so stepping through frames with them on is quicker (43 ms to
  26 ms an update on the same scene), and the stage no longer copies the
  whole document on every update to see whether it changed.

### Scripting

- Scripts can build characters: skeletons (`addSkeleton`, `addBone`,
  `setBone` for targets, limits and dynamics, `poseBone`), how layers
  follow them (`follows`, limbs included), drawings (`addDrawing`,
  `showDrawing`), sound (`importAudio`) and lip sync (`lipSync`, from
  loudness or a timing file); and web pages: `button` and
  `setFrameAction`. `addLayer` takes `text`. A new example script,
  `walk-cycle.js`, rigs a walking character.

## [0.4.0]

Rigging that bends the way characters do, an easier start, and a
guidebook with projects. Projects saved by 0.4.0 can use a newer file
format, which 0.3.0 can't open; 0.4.0 opens older projects as they were.

### Getting started

- The first time Inkframe opens, a tour points out each part of the
  window in turn: the toolbar, the context bar, the stage, Properties,
  the timeline, Find a Command and Help. Help › Take the Tour shows it
  again, and Help links to the guidebook.
- The guidebook, *Inkframe for Artists*, attached to this release, gains
  page numbers in its contents and three projects, start to finish: a
  walk cycle on a skeleton with target feet, a lip-synced talking head,
  and an interactive web banner.

### Rigging

- **Limbs:** a layer in a skeleton can follow a limb (**Thigh and
  below**), bending with that bone and the bones hanging from it only, so
  two legs drawn over each other, as in a walk from the side, each bend
  with their own bones. File format 39.
- **Action timeline:** each smart bone has a timeline of its own, as in
  Moho: a strip across its turns. Drag along it to preview the bone at
  any turn without keying anything, retime its actions and correction
  keys by dragging, and key corrections at a previewed turn.

### Workspace

- Every section of Properties folds away when its title is clicked, and
  what's folded is remembered.
- Nodes arranged in the Node View stay where they were put, saved with
  the project. Nodes and wires no longer jump when grabbed.

### Export

- A sprite sheet whose frames don't fit on one 8192 × 8192 image is shared
  out over as few as it needs, as TexturePacker's multipack does, rather
  than failing.

## [0.3.0]

Moho-style character rigging, Animate's drawing and export tools, an
easier-to-find interface, and a beginner's guidebook. Projects saved by
0.3.0 use a newer file format, so 0.2.0 can't open them; 0.3.0 opens
older projects as they were.

### Guidebook

- *Inkframe for Artists*, a beginner's guide in the style of a Packt
  book, is attached to this release as a PDF: ten chapters from the
  workspace to rigging, smart bones and export, with screenshots of every
  step and appendices of every tool and command. `cargo xtask guide`
  builds it, taking fresh screenshots, so it keeps up with the app.
- `inkframe --reference` prints every tool and command as Markdown tables.

### Finding your way

- A context bar above the stage: the current tool, buttons for what can
  be done now (Symbol, Distort, Envelope, New Skeleton…), and switches for
  rulers, perspective, onion skins, the light table and rig setup.
- Find a Command (⌘K): find any command or tool by name, or by what it's
  for ("bulge", "rig", "web page"), with its menu and keys.
- Toolbar tips say what each tool does and what shares its slot; new
  icons throughout.

### Rigging, Moho-style

- **Skeletons:** Modify › New Skeleton puts the selected layers in a
  folder with a skeleton. Draw bones with the Add Bone tool (Shift+K) and
  pose them with the Bone tool (inverse kinematics, keyed on the folder).
  Every layer in the folder bends with the bones near it, painted layers
  and pictures included; a layer can follow one bone instead, or none.
- **Pins and welded points:** shapes or single points can be pinned to a
  bone, so an eye stays on the head as the face bends. Points where shapes
  meet are shared, so joined shapes stay joined as they're edited or bent.
- **Switches** show one of a folder's layers at a time, keyed per frame
  (Modify › New Switch).
- **Smart bones** morph a layer toward another drawing, or have a switch
  show another layer, as they turn; **corrections** let you reshape a
  drawing by hand at any turn, morphing smoothly between them.
- **Limits, stretch and dynamics:** turn limits respected by IK, stretch
  that can keep a drawing's area as it squashes, and springy dynamics for
  hair and tails.
- **Target bones** keep feet planted while the body moves.
- **Rig setup** (View › Rig Setup, ⌥⌘B) shows skeletons at rest to draw,
  paint and rig on; off, painting and drawing land where they're seen on
  the pose.
- A **Bones** panel lists each skeleton's bones and the layers it rigs. A
  skeleton's poses and a switch's choices can be dragged along their
  folder's row in the timeline.

### Drawing

- The Pen edits existing points between paths, and Alt-dragging a point
  (with the Pen or Subselection tool) pulls curve handles out of it, as in
  Animate.
- Modify › Transform › Distort and Envelope, Animate-style, bend a whole
  layer or the shapes picked inside it.
- Modify › Combine Shapes: Union, Intersect, Punch and Crop, keeping
  curves and corners. Break Apart (⌘B) turns text into a shape per letter.
- Gradient fills, linear or radial; line ends, corners, and dashed or
  dotted lines.
- Art and pattern brushes draw strokes with a library symbol, stretched
  along the line or repeated along it.
- Line textures ink a layer's lines with a Pencil, Charcoal or Chalk grain.
- Rulers and guides, with snapping, and one-, two- and three-point
  perspective guides that the Pen snaps to.
- Picked shapes turn with a knob, and copy, cut and paste into any layer;
  ⌘Z while typing undoes a word at a time.

### Animation

- Timeline › Draw Inbetweens fills the frames between two key drawings
  with drawings morphed between them, to touch up by hand.
- Animate's frame commands (F5, Shift+F5, F7, Convert to Keyframes,
  Reverse Frames) and Edit Multiple Frames.
- Motion Presets: ready-made movements (fade, pop, slide, bounce, spin and
  more), or save a layer's own animation as a preset for every project.
- Swap Symbol replaces an instance's symbol for the whole layer or from a
  frame on, keeping its animation.
- With several layers selected, the properties panel changes all of them.

### Export

- Web Page (.html): one self-contained page that plays the animation as
  vectors in any browser. Layers can be buttons (Play, Stop, Go to a
  frame, Open a link) and frames can have actions, as Animate's `stop()`
  and `gotoAndPlay()`.
- Animated GIF, SVG and Lottie. The vector formats keep gradients, line
  styles, masks, blend modes and symbols, and bent paintings export bent.

## [0.2.0]

Drawing, the Library and the timeline, reworked after first use. Projects
saved by 0.2.0 use a newer file format, so 0.1.0 can't open them; 0.2.0
opens 0.1.0's projects as they were.

### Drawing

- What you draw goes into the selected layer, as in Animate, rather than
  each shape getting a layer of its own. **+** in the timeline (or
  `⌘⇧N` / `Ctrl+Shift+N`) adds an empty layer to draw into. The old way is
  one checkbox away in the drawing tools' settings.
- Double-click a layer on the stage to step inside it and pick out single
  shapes: move them, scale them, delete them, or turn them into a symbol
  with F8, without moving or keying the layer.
- The Brush and Eraser settings show a preview stroke, tapering as a pen
  would, and the brush cursor marks where a soft brush's edge begins.

### Symbols and the Library

- Select several layers and press F8 to group them into one symbol, rigs
  and animation included, then double-click it to work on each layer
  inside. A symbol's length sets how often a looping instance repeats, for
  walk cycles and other loops.
- The Library is now a grid of cards for symbols, imported images and
  sounds, with folders, search and a size slider. Drag a card onto the
  stage to place it where you let go, or onto the timeline to place it from
  that frame, above that layer.
- A preview at the top of the Library plays the selected symbol in a loop
  (with pause and a frame slider), shows images larger, and plays sounds on
  their own. Hovering an animated card plays it in the card.

### Timeline

- Works with a pen: drag a layer's name to scroll the list, or press and
  hold it for a moment to pick it up and restack it.
- A clearer playhead, the current frame's column tinted down every row,
  and the row under the pointer lit.

### Scripting

- JavaScript scripts that change projects: set up scenes, change many
  layers at once, build animation from numbers, or export folders of
  projects. Run them from the Scripts menu, the Script Console, or
  `inkframe --script` on the command line. A script's changes undo in one
  step, and nothing changes if it fails. See docs/SCRIPTING.md, and the
  example scripts in your Scripts folder. (This was in the 0.1.0 builds but
  missing from their notes.)

### Look

- A soft shadow under the stage, matching section titles in Properties,
  and hints on an empty project for how to start.

### Before you install

These builds aren't signed with an Apple or Microsoft developer certificate
yet, so each system warns the first time:

- **macOS:** open the disk image and drag Inkframe to Applications. If macOS
  says it can't check the app for malicious software, open **System
  Settings › Privacy & Security** and click **Open Anyway** beside the
  message about Inkframe.
- **Windows:** if SmartScreen says it protected your PC, click **More info**,
  then **Run anyway**.

Not yet tried by hand: pressure and the timeline's hold-to-restack from a
real drawing tablet, the Windows installer on a physical PC, Photoshop
opening the exported PSDs, and a game engine loading the sprite sheets.
Reports are welcome in the issues.

## [0.1.0]

Prepared but never published; 0.2.0 is the first public release, and has
everything below too.

A 2D animation editor in the spirit of Adobe Animate and Toon Boom Harmony,
for Apple Silicon Macs and 64-bit Windows.

### Drawing and painting

- Vector shapes, the Pen, freehand Pencil lines that follow pen pressure
  (with a Width tool to reshape them), and text with any installed or
  project font.
- Paint layers with Photoshop-style brushes, erasers, bucket and gradient
  fills, retouching tools, selections and adjustments (Levels, Curves,
  Hue/Saturation and more).
- Ink and paint as in Harmony: line art and colour art, a gap-closing
  paint bucket, and palettes whose colours update every drawing.
- Vectorise: trace a scan or a painting into vector shapes.

### Animation

- A timeline with keyframes per property, easing, a graph editor, motion
  guides, onion skin and a light table, plus an exposure sheet.
- Symbols, shape tweens, character rigs with parents, bones and inverse
  kinematics, deformers, free-form warps and master controllers.
- Lip sync from Rhubarb, Papagayo files or loudness.
- A camera that pans, zooms, turns and dollies through layers at depth (a
  multiplane), with a Depth View to arrange them and optional stacking by
  depth.
- Colour effects and filters (shadow, blur, glow, adjust colour, motion
  blur) that animate, and particle layers for sparks, snow, smoke and fire.
- A node view of how layers combine, a template library, safe areas and a
  field guide.

### Export

- PNG sequences, ProRes 4444 / 4444 XQ / 422 HQ, H.264, HEVC with alpha
  (macOS), sprite sheets for game engines, and layered PSDs. The installers
  include ffmpeg, so video export works straight away.
- Batch export of many projects and formats, in a window or with
  `inkframe --export` on the command line.

### Working

- Blender-style areas you can split, join and change, with workspaces.
- Autosave and crash recovery.
- A getting-started guide (Help › Getting Started Guide) and five example
  projects to open from File › Open Example: a bouncing ball, a character
  rig, ink and paint, a multiplane camera move, and particles.

### Before you install

These builds aren't signed with an Apple or Microsoft developer certificate
yet, so each system warns the first time:

- **macOS:** open the disk image and drag Inkframe to Applications. If macOS
  says it can't check the app for malicious software, open **System
  Settings › Privacy & Security** and click **Open Anyway** beside the
  message about Inkframe.
- **Windows:** if SmartScreen says it protected your PC, click **More info**,
  then **Run anyway**.

Not yet tried by hand: pressure from a real drawing tablet, the Windows
installer on a physical PC, Photoshop opening the exported PSDs, and a game
engine loading the sprite sheets. Reports are welcome in the issues.
