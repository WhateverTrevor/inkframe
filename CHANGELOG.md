# Changelog

What changed in each release of Inkframe. The newest is first. A release's
notes on GitHub are its section here.

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
