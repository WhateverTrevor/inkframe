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

- **Scripts menu:** lists the `.js` files in your Scripts folder. Choose
  **Scripts › Open Scripts Folder** to find it, and put scripts there.
  **Run Script…** runs any other `.js` file.
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
It can't read or write other files, use the network or run programs. A
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
| `layers` | The main timeline's layers, top first, as the timeline lists them |
| `layer(name)` | The first layer with that name, or `null` |
| `addLayer(name, options)` | Adds a layer on top and returns it (below) |
| `selectedLayers` | The layers selected in the editor |
| `select(layers)` | Selects these layers when the script finishes |
| `swatches` | The palette: `[{ name, color }]` |
| `addSwatch(name, color)` | Adds a swatch |
| `setSwatch(index, color)` | Changes a swatch, recolouring everything painted in it |
| `save(path)` | Saves; `path` is needed the first time |
| `export(options)` | Exports, and returns the file written (below) |

**`addLayer` options**, all optional: a shape (`rect: [width, height]`
with `radius` for rounded corners, `ellipse: [width, height]`, or
`polygon: [[x, y], ...]`), `fill`, `stroke`, `strokeWidth`, `at: [x, y]`
on the stage (the middle if left out), and the first `frame` it shows on.
A layer with no shape is empty, ready to animate or parent things to.

```js
const ball = inkframe.document.addLayer("Ball", {
  ellipse: [120, 120], fill: "#e8543c", at: [960, 200],
});
```

**`export` options:** `format` (`"png"`, `"sheet"`, `"prores4444"`,
`"prores4444xq"`, `"prores422hq"`, `"h264"`, or `"hevc-alpha"` on macOS) and
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
| `setKey(frame, values, { easing })` | Keys some properties: `setKey(12, { position: [960, 880], rotation: 15 })` |
| `keys(property)` | Every key of one property: `[{ frame, value, easing }]` |
| `valueAt(property, frame)` | A property's value on a frame, tweens included |
| `removeKey(frame, property)` | Removes one property's key, or all of them on that frame without `property` |
| `shiftKeys(frames)` | Moves its keys later (earlier if negative); the layer stays put |
| `shiftTime(frames)` | Moves the whole layer in time: its span, drawings and keys |
| `moveUp(steps)`, `moveDown(steps)` | Restacks it |
| `remove()` | Deletes it |

The properties are `position`, `rotation`, `scale`, `skew`, `opacity` and
`depth`.

```js
// A bouncing ball.
const doc = inkframe.document;
const ball = doc.addLayer("Ball", { ellipse: [120, 120], fill: "#e8543c" });
ball.setKey(1,  { position: [960, 200] }, { easing: "ease-in" });   // falls, speeding up
ball.setKey(12, { position: [960, 880], scale: [1.2, 0.8] }, { easing: "ease-out" });
ball.setKey(24, { position: [960, 200], scale: [1, 1] });
```

## `console`

`console.log(...)` prints its arguments, separated by spaces: to the
Script Output window in the app, or the terminal from the command line.

## Coming next

The plan (in `SCRIPTING_PLAN.md`) adds a Script Console to type and run
scripts in, example scripts, and then symbols, rigs, effects, particles
and the camera.
