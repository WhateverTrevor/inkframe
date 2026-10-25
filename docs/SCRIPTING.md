# Scripting Inkframe

Scripts are JavaScript files that change projects for you: setting up
scenes, changing many layers at once, building animation from numbers, or
exporting whole folders of projects with your own file names.

```js
// Fan out the selected layers' animation, two frames apart.
const layers = inkframe.document.selectedLayers;
layers.forEach((layer, i) => layer.shiftKeys(i * 2));
console.log(`Staggered ${layers.length} layers`);
```

## Running scripts

- **Scripts menu:** lists the `.js` files in your Scripts folder, which
  starts out holding the example scripts below. Choose **Scripts › Open
  Scripts Folder** to find it, and put your own scripts there. **Run
  Script…** runs any other `.js` file.
- **Script Console:** an area (in any area's menu, under Project) with a
  box to type or paste a script, **Run** (Cmd+Enter) and **Stop**, and
  what it prints underneath. Good for trying things out.
- **Command line:** `inkframe --script stagger.js walk.inkf run.inkf --save`
  runs a script on each project without opening a window, and `--save`
  saves the ones it changed. With no projects, it runs once on a new,
  empty project. What the script prints appears in the terminal.

A script works on a copy of the project. When it finishes, its copy
becomes the project, and **Edit › Undo** undoes the whole script in one
step. If it fails, nothing changes, and you see the error with its line
number. A **Stop** button appears while it runs. If you edit the project
while a script runs, the script's changes are set aside instead of
overwriting yours.

## What scripts can reach

A script sees only the `inkframe` object and `console`. It can change the
open project, open and save Inkframe projects (`.inkf` files), and export.
It can also bring sound files into a project (`importAudio`) and read
lip sync timing files (`.tsv`, `.txt` or `.dat`). It can't read or write
other files, use the network or run programs. A
relative path starts from the project's folder (from the command line,
for a new project, the current folder). `~/` means your home folder.

## Frames, values and colours

- **Frames** count from 1, as the timeline shows them.
- **Positions, scales and skews** are `[x, y]` pairs. **Scale** is a factor:
  1 is the original size, `[1.2, 0.8]` is squashed. **Rotation** and
  **skew** are in degrees. **Opacity** runs from 0 to 1. **Depth** is in
  stage pixels.
- **Colours** are text: `"#ff6633"`, `"#f63"`, or with transparency
  `"#ff663380"`.
- **Easing** is `"linear"`, `"ease-in"`, `"ease-out"`, `"ease-in-out"`,
  `"hold"`, or a curve `{ bezier: [x1, y1, x2, y2] }` as in CSS. A key's
  easing is how it travels on to the next key.

## `inkframe`

| | |
|---|---|
| `inkframe.document` | The project the script runs on |
| `inkframe.open(path)` | Opens another project and returns its `Document` |

## `Document`

| | |
|---|---|
| `name`, `path` | Its name, and its file (`null` if never saved) |
| `width`, `height` | Stage size in pixels; can be set |
| `frameRate`, `duration` | Frames per second and length in frames; can be set |
| `background` | Stage colour; can be set |
| `stackByDepth` | Whether layers are drawn far to near by depth; can be set |
| `layers` | The main timeline's layers, top first, as the timeline lists them |
| `layer(name)` | The first layer with that name, or `null` |
| `addLayer(name, options)` | Adds a layer on top and returns it (below) |
| `selectedLayers` | The layers selected in the editor |
| `select(layers)` | Selects these layers when the script finishes |
| `symbols` | The symbol library: `[{ name, duration, layers, uses }]` |
| `addInstance(symbol, options)` | Adds an instance of a symbol (by name); options `at` and `frame` |
| `addParticleLayer(preset, options)` | Adds particles: `"sparks"`, `"snow"`, `"smoke"` or `"fire"`; options `at` and `frame` |
| `addSkeleton(layers, name)` | Puts layers in a new folder with an empty skeleton, as Modify › New Skeleton does, and returns the folder |
| `importAudio(path, frame)` | Brings in a sound file, starting on a frame; returns the sound's name |
| `frameActions` | The frames with web page actions: `[{ frame, action }]` |
| `setFrameAction(frame, action)` | What a Web Page export does on reaching a frame (below), or nothing with `null` |
| `camera` | The camera layer, or `null` |
| `addCamera()` | The camera, added if there isn't one |
| `controllers` | The master controllers: `[{ name, value, poses }]` |
| `setController(controller, value, frame)` | Sets a controller (by name or number) from 0 to 1 on a frame, posing its layers |
| `swatches` | The palette: `[{ name, color }]` |
| `addSwatch(name, color)` | Adds a swatch |
| `setSwatch(index, color)` | Changes a swatch, recolouring everything painted in it |
| `save(path)` | Saves; `path` is needed the first time |
| `export(options)` | Exports, and returns the file written (below) |

**`addLayer` options**, all optional: a shape (`rect: [width, height]`
with `radius` for rounded corners, `ellipse: [width, height]`, `text`
with a font `size`, or
`polygon: [[x, y], ...]`), `fill`, `stroke`, `strokeWidth`, `at: [x, y]`
on the stage (the middle if left out), and the first `frame` it shows on.
A layer with no shape is empty, ready to animate or parent things to.

```js
const ball = inkframe.document.addLayer("Ball", {
  ellipse: [120, 120], fill: "#e8543c", at: [960, 200],
});
```

**`export` options:** `format` (`"png"`, `"sheet"`, `"gif"`, `"svg"`,
`"lottie"`, `"prores4444"`, `"prores4444xq"`, `"prores422hq"`, `"h264"`, or
`"hevc-alpha"` on macOS) and
`to`, the file to write, ending in the format's extension. Optional:
`scale` (0.5 for half size), `transparent` (default `true`) and
`frames: [first, last]`.

```js
doc.export({ format: "h264", to: `renders/${doc.name}_v001.mp4`, scale: 0.5 });
```

## `Layer`

| | |
|---|---|
| `name`, `visible`, `locked` | Can be set |
| `kind` | `"vector"`, `"paint"`, `"symbol"`, `"particles"`, `"folder"` or `"camera"` |
| `start`, `end` | The first and last frame it shows on (`end` is `null` to stay to the end); can be set |
| `parent` | The layer it's attached to in a rig, or `null`; can be set |
| `children` | The layers attached to it |
| `pivot` | The point it turns and scales around, in its own coordinates; can be set |
| `symbol`, `playback`, `firstFrame` | For an instance: its symbol's name, how it plays (`"loop"`, `"play-once"`, `"single-frame"`) and the symbol frame it starts on; the last two can be set |
| `convertToSymbol(frame)` | Turns the layer into a symbol, the layer becoming its instance; returns the symbol's name |
| `effectsAt(frame)`, `setEffects(frame, effects)` | Its colour effect and filters on a frame (below) |
| `particles` | A particle layer's emitter settings (below); set some to change just those |
| `setKey(frame, values, { easing })` | Keys some properties: `setKey(12, { position: [960, 880], rotation: 15 })` |
| `keys(property)` | Every key of one property: `[{ frame, value, easing }]` |
| `valueAt(property, frame)` | A property's value on a frame, tweens included |
| `removeKey(frame, property)` | Removes one property's key, or all of them on that frame without `property` |
| `shiftKeys(frames)` | Moves its keys later (earlier if negative); the layer stays put |
| `shiftTime(frames)` | Moves the whole layer in time: its span, drawings and keys |
| `bones` | A skeleton folder's bones: `[{ name, parent, start, end, strength, target, limits, dynamics, squash }]` |
| `addBone(name, start, end, { parent })` | Adds a bone at rest, hanging from a bone (by name) or a root of its own |
| `setBone(name, settings)` | Changes a bone: `name`, `strength`, `target` (a bone's name), `limits: [min, max]`, `squash`, `dynamics: { spring, damping, swing }` (`null` turns one off) |
| `poseBone(name, frame, pose)` | Keys a bone: `turn` (degrees from rest), `moveTo: [x, y]` (where its start goes, as dragging a root does) and `stretch` |
| `follows` | How a layer in a skeleton's folder follows the bones: `"nearby"`, `"none"`, `{ bone: name }` or `{ limb: name }`; can be set |
| `drawings`, `addDrawing(name, { copy })`, `showDrawing(name, frame)` | The layer's drawings by name; adds one (blank, or a copy of another); shows one from a frame on |
| `lipSync(sound, { timing })` | Lip-syncs a mouth layer to a sound (by name), its drawings matched to the mouth shapes by their names: from the sound's loudness, or a Rhubarb (`.tsv`) or Papagayo (`.dat`) timing file. Returns how many mouth changes it made |
| `button` | What clicking the layer does in a Web Page export (below), or `null`; can be set |
| `moveUp(steps)`, `moveDown(steps)` | Restacks it |
| `remove()` | Deletes it |

The properties are `position`, `rotation`, `scale`, `skew`, `opacity` and
`depth`. For the camera, `position` is the point it looks at, `rotation`
turns the view, `scale` zooms, and `depth` is how far it has dollied in.

**Effects** are `{ color, filters }`. `color` is `{ type: "none" }`,
`{ type: "brightness", amount }`, `{ type: "tint", color, amount }` or
`{ type: "advanced", ... }`. Each filter has a `type` (`"dropShadow"`,
`"blur"`, `"glow"`, `"adjustColor"`, `"motionBlur"`), `enabled`, and its
settings. Read a layer's with `effectsAt` to see the exact form, change
it, and give it back to `setEffects`. Colours here are full `"#rrggbbaa"`.

```js
const fx = layer.effectsAt(1);
fx.filters.push({ type: "blur", enabled: true, x: 6, y: 6 });
layer.setEffects(1, fx);
```

**Particle settings** are the ones in the Particles section of a layer's
Properties, in camelCase: `rate`, `startFull`, `shape`, `direction`,
`spread`, `speed`, `speedVariance`, `gravity`, `wind`, `drag`,
`turbulence`, `turbulenceFrequency`, `lifetime`, `lifetimeVariance`,
`sizeStart`, `sizeEnd`, `sizeVariance`, `opacityStart`, `opacityEnd`,
`colorStart`, `colorEnd`, `spin`, `spinVariance`, `sprite` and `seed`.

```js
const snow = doc.addParticleLayer("snow");
snow.particles = { rate: 80, sizeStart: 6 };
```

```js
// A bouncing ball.
const doc = inkframe.document;
const ball = doc.addLayer("Ball", { ellipse: [120, 120], fill: "#e8543c" });
ball.setKey(1,  { position: [960, 200] }, { easing: "ease-in" });   // falls, speeding up
ball.setKey(12, { position: [960, 880], scale: [1.2, 0.8] }, { easing: "ease-out" });
ball.setKey(24, { position: [960, 200], scale: [1, 1] });
```

**Web actions**, for `button` and `setFrameAction`, are `"play"`, `"stop"`,
`{ gotoAndPlay: frame }`, `{ gotoAndStop: frame }` or `{ openLink: url }`.

```js
// Rigging a leg, as the walk-cycle example does in full.
const leg = doc.addLayer("Leg", { rect: [50, 340], at: [960, 600] });
const rig = doc.addSkeleton([leg], "Walker");
rig.addBone("Thigh", [960, 430], [960, 600])
   .addBone("Shin", [960, 600], [960, 770], { parent: "Thigh" })
   .addBone("Foot", [960, 770], [1010, 770])
   .setBone("Shin", { target: "Foot", limits: [0, 150] })
   .poseBone("Foot", 12, { moveTo: [1050, 770] });   // the leg reaches for it
leg.follows = { limb: "Thigh" };
```

## `console`

`console.log(...)` prints its arguments, separated by spaces: to the
Script Output window in the app, or the terminal from the command line.

## Example scripts

These come in your Scripts folder, and in `examples/scripts`. Each starts
with a comment saying what it does and what to change.

| | |
|---|---|
| `bouncing-ball.js` | Builds a bouncing ball with squash and stretch: the one to read first |
| `stagger-selected.js` | Delays each selected layer's animation a little more than the last |
| `fade-in-out.js` | Fades the selected layers (or all) in and out at their ends |
| `number-layers.js` | Renames layers to a prefix and a number, top to bottom |
| `confetti.js` | Generates 40 tumbling pieces of confetti from random numbers |
| `export-review.js` | Exports a half-size H.264 beside the project, named with today's date |
| `walk-cycle.js` | Draws a character from the side and rigs a 24-frame walk: a skeleton with target feet, limbs and bobbing hips |
| `snowy-night.js` | Adds snow, tints every layer blue and pushes the camera in: particles, effects and the camera |

