# Changelog

What changed in each release of Inkframe. The newest is first. A release's
notes on GitHub are its section here.

## Unreleased

### Drawing

- Picked shapes inside a layer turn with a knob above them (Shift snaps to
  15°), copy, cut and paste (landing where they were, in any layer), and
  the fills under the lines can be picked too.
- ⌘Z while typing on the stage undoes what's been typed, a word at a time,
  and ⌘⇧Z redoes it.

### Smart bone corrections

- A smart bone can reshape a layer by hand-drawn keys at several turns, as
  Moho's actions: turn the bone, Edit Correction, reshape with any tool,
  Finish. The drawing morphs between keys. File format 38.

### Web page export

- Export › Web Page (.html): one self-contained page that plays the
  animation as vectors, loops, and pauses on a click.

### Draw inbetweens

- Timeline › Draw Inbetweens fills the frames between two key drawings
  with drawings morphed between them, spaced by the shape tween's easing,
  to touch up by hand.

### Perspective guides

- View › Perspective Guide: one-, two- or three-point, with draggable
  vanishing points. Shift with the Pen snaps lines to it.

### Line textures

- A vector layer's lines can be inked with a paper grain (Pencil,
  Charcoal or Chalk), as Harmony's line textures.

### Art and pattern brushes

- Strokes can be drawn with a library symbol, stretched along the line
  (Art) or repeated along it (Pattern), as in Animate's Paint Brush. Older
  builds open such files and draw the strokes plain.

### Target bones

- A bone can reach for a target bone, the bones above it bending to get
  its end there, so feet stay planted while the body moves. File format
  stays 37 (an optional field).

### Folder keys

- A skeleton's poses and a switch's choices can be dragged along their
  folder's row in the timeline, and removed with Shift+F6.

### Bent paintings in SVG and Lottie

- Painted layers and pictures bent by a skeleton now export bent to SVG
  and Lottie too, drawn by the renderer and embedded as images.

### Bones panel

- A Bones area lists each skeleton's bones as a tree (smart, swinging,
  limited and stretching bones marked; click one to pick it) and the
  layers it rigs, with what each follows.

### Rig setup, and painting on poses

- View › Rig Setup (⌥⌘B) shows skeletons at rest to draw, paint and rig
  on. Off, the paint tools, the Paint Bucket and shapes drawn into a
  bent layer land exactly where they're seen on the pose.

### Welded points

- Points where shapes meet are shared: dragging one moves every shape's
  copy, a point dropped onto another welds to it, and ⌘-drag parts it.
  Pins to bones carry across welds, so joined shapes stay joined as a
  skeleton bends them. No file format change.

### Bone limits, stretch and dynamics

- Skeleton bones can have turn limits (respected by IK and shown as a
  wedge), stretch (Shift-drag with the Bone tool) with an option to keep
  their area as they squash, and dynamics: a spring that makes hair and
  tails swing after the body and settle. File format 37.

### Smart bones, pins and switches

- Shapes, or single points, can be pinned to one bone of a skeleton.
- Switch folders show one of their layers at a time, keyed per frame
  (Modify › New Switch).
- Smart bones: as a bone turns, it can morph a layer toward another of its
  drawings, or have a switch show another layer past an angle. File
  format 36.

### Skeletons

- Moho-style rigging: Modify › New Skeleton puts the selected layers in a
  folder with a skeleton. Draw bones with the new Add Bone tool (Shift+K),
  pose them with the Bone tool (inverse kinematics, keyed on the folder),
  and every layer in the folder bends with the bones near it: drawn
  shapes and painted layers and pictures alike. A layer can follow one
  bone instead, or none. File format 35.

### Pen editing

- Between paths, the Pen picks and drags the selected shape's existing
  points and their handles, instead of starting a new path on them.
- Alt-dragging a point (with the Pen or the Subselection tool) pulls new
  curve handles out of it, turning a corner into a curve, as in Animate.

### Envelope handles

- Envelope now works as Animate's does: a point at each corner and in the
  middle of each side, each with handles that shape the curve either side
  of it (Alt-drag one to kink the side). Distort and Envelope bend a whole
  layer without stepping inside it, so the other layers stay in view.

### Distort and Envelope

- Modify › Transform › Distort puts a handle at each corner of the picked
  shapes (or everything in the selected layer); drag them to lean, taper
  or put the shapes into perspective. Envelope has sixteen handles for
  bulging and bending. Curves stay smooth, and pencil lines keep their
  widths. Enter, Escape or clicking away finishes.

### Rulers and guides

- View › Rulers, and guides dragged out of them onto the stage, saved with
  the project. Moving layers and drawing rectangles and ovals snap to
  them; guides can be hidden, locked or cleared.

### Animation

- Motion Presets: give layers a ready-made movement (fade, pop, slide,
  bounce, spin, pulse, shake, float) from the playhead, starting from
  wherever each one is, or save a layer's own animation as a preset for
  every project.

### Symbols

- Modify › Swap Symbol: pick another symbol for the selected instances,
  keeping their animation, for the whole layer or from the current frame
  on (a hand's pose changed mid-shot, the arm still moving). Duplicate
  Symbol copies one to change separately.

### Shapes

- Modify › Combine Shapes: Union, Intersect, Punch and Crop on shapes
  picked inside a layer, keeping curves and sharp corners. Break Apart
  (⌘B) turns text into a shape per letter.

### Export

- Animated GIF, SVG (a file per frame) and Lottie, which plays as vectors
  on web pages and in apps. Both vector formats keep gradients, line
  styles, masks, blend modes and symbols; filters and particles go in as
  pictures.

### Styles

- Gradient fills on vector shapes, linear or radial, with as many colours
  as you like; drag their ends on the stage after stepping inside the
  shape's layer.
- Line ends (round, flat, square), corners (round, sharp, bevelled), and
  dashed or dotted lines.

### Timeline

- Animate's frame commands: pick frames with Shift-click or Shift-drag,
  then F5 inserts frames, Shift+F5 removes them, F7 adds a blank keyframe,
  and Timeline › Convert to Keyframes, Convert to Blank Keyframes and
  Reverse Frames work on the picked frames.
- Edit Multiple Frames: dragging on the stage moves layers across the
  picked frames at once, a whole motion or every drawing of a hand-drawn
  stretch.

### Properties

- With several layers selected, changes in the properties panel apply to
  all of them: position moves them together, and other values are set the
  same on each. A • marks a value they differ in. Effects and easing have
  an **Apply to the other selected layers** button, to give them all the
  same filters or the same curve.

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
