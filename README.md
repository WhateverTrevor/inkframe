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
  and line spacing. It's real vector outlines, filled and stroked like shapes, so
  it stays sharp at any size and animates like any other layer.
- **Import images** (PNG, JPEG, GIF, WebP) to trace over or composite.
  They move, scale, rotate, fade and animate like any other layer.
- **Animate with per-property keyframes.** Go to a frame, move or change a
  layer, and a key is created for just the properties you changed. Position,
  rotation, scale and opacity each have their own keys and timing, so a
  layer can ease its movement while fading at a steady rate. Scale and
  rotate with on-stage handles, or type exact values in the properties panel.
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

- **Undo/redo** for every change, and plain-JSON project files (`.inkf`) that
  diff cleanly in version control.

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
| Select several | Shift-click shapes, or drag a box over empty stage; Shift- or ⌘-click layer names in the timeline; `⌘A` selects all, `⌘⇧A` none. Dragging any of them moves them all, and delete, duplicate, copy/cut and F6 keyframes apply to every selected layer |
| Scale | Drag a square handle. Shift keeps proportions, Option/Alt scales from the pivot |
| Rotate | Drag the round knob, or just outside a corner. Shift snaps to 15° |
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
| Insert / remove keyframe | `F6` / `Shift+F6` keys every property; right-click a property row in the timeline, or click the ◇ next to a property in the properties panel, to key just that one |
| Play / pause | `Enter` |
| Step frames | `,` and `.` (or ← →), `Home` / `End` |
| Pan / zoom | Scroll or Space-drag to pan, pinch or ⌘-scroll to zoom, `⌘0` to fit |
| Symbols | Select a layer and press `F8` (Modify ▸ Convert to Symbol). Use **Place** in the Library to add more instances; set each instance's playback (Loop, Play Once, Single Frame) and first frame in the properties panel. Double-click an instance (or **Edit** in the Library) to change the symbol; `Esc` or the breadcrumb bar takes you back out |
| Text | Text tool (`T`): click the stage and type. Click existing text with the Text tool, or double-click it with the Selection tool, to type into it; click or drag inside it to move the cursor or select, and press Esc or click elsewhere to finish. The usual text keys work (arrows, Shift to select, ⌥/Ctrl+arrows by word, ⌘C/⌘V). The properties panel has a Text box too. Font, size, alignment and fill/stroke are in the same panel. File › Import Font… (or dropping `.ttf`/`.otf` files on the window) links font files to the project; they're listed first in the font menu |
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
