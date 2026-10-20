# Changelog

What changed in each release of Inkframe. The newest is first. A release's
notes on GitHub are its section here.

## [0.1.0]

The first release: a 2D animation editor in the spirit of Adobe Animate and
Toon Boom Harmony, for Apple Silicon Macs and 64-bit Windows.

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
