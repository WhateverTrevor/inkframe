# Scripting: a plan

A proposal for letting people automate Inkframe with scripts, as Animate
(JSFL), Harmony and Blender allow. Nothing here is built yet. The
decisions marked **Decide** have been made; see the end.

## What scripts are for

Animators and studios script the work that's the same every time:

- Setting up a scene: layers named and stacked to a studio's template, a
  camera, a palette.
- Bulk changes: rename, recolour or retime many layers at once; stagger a
  set of layers' animation by a few frames each.
- Generating animation: a crowd of particles placed from data, a
  procedural walk, text animated letter by letter.
- Pipelines: open every project in a folder, export each with the
  studio's file names, report what's missing.

## Language

**Decide: which language.** The candidates, each with a maintained Rust
library for embedding it:

| | JavaScript (QuickJS, via `rquickjs`) | Lua (via `mlua`) | Rhai |
|---|---|---|---|
| Who knows it | Most people who've scripted anything. Animate's JSFL and Harmony's scripting are both JavaScript, so their users' scripts and habits carry over | Game and tool people (Aseprite, Roblox, Neovim) | Almost nobody outside Rust projects |
| Building it | A small C library compiled in; needs checking on both CI platforms | A C library compiled in (well proven on macOS and Windows) | Pure Rust, nothing extra |
| Size | About 1 MB | About 0.5 MB | About 1 MB |
| Speed | Fast enough for anything a script would do here | Fastest | Slowest, still fine for document edits |
| Safety | Has no file or network access of its own: scripts get only what Inkframe hands them | Its standard file and OS functions have to be removed | No file or network access unless given |

**Recommendation: JavaScript.** It's the language people coming from
Animate and Harmony already script in, and it's the most widely known.
QuickJS keeps it small and starts with no access to files or the network,
which makes safety straightforward. The first step (below) is a short
spike to confirm it builds and runs on both platforms before committing.

## What a script looks like

A first sketch of the API, to show its shape (names will firm up as it's
built):

```js
// Stagger the selected layers' animation, two frames apart.
const layers = inkframe.document.selectedLayers;
layers.forEach((layer, i) => layer.shiftKeys(i * 2));

// Build a scene.
const doc = inkframe.document;
const ball = doc.addLayer("Ball", { ellipse: [80, 80], fill: "#ff6633" });
ball.setKey(1,  { position: [960, 200] }, { easing: "ease-in" });
ball.setKey(12, { position: [960, 880] }, { easing: "ease-out" });
ball.setKey(24, { position: [960, 200] });

// Export with a studio's naming.
inkframe.export({ format: "h264", to: `renders/${doc.name}_v001.mp4` });
console.log(`Exported ${doc.name}`);
```

The first version would cover:

- **The document:** size, frame rate, duration, background, its name and
  file.
- **Layers:** list, find by name, add, remove, rename, reorder, show, hide
  and lock; selection.
- **Animation:** read, set and remove keys per property (position,
  rotation, scale, skew, opacity, depth), with easing; shift keys in time.
- **Drawing:** add simple shapes (rectangles, ovals, paths) with fill and
  stroke.
- **Colour:** the palette's swatches.
- **Files:** open and save projects, and export in any format.
- **Output:** `console.log` to a Script Console.

Symbols, rigs, effects, particles and the camera would follow in a second
round.

## How scripts run

- **Scripts menu:** lists the `.js` files in a Scripts folder (in the
  settings folder, like the template library), plus *Run Script…* for any
  file and *Open Scripts Folder*.
- **Script Console:** a new area kind with a box to type or paste a script,
  a Run button, and the output and any errors with their line numbers.
- **Command line:** `inkframe --script stagger.js walk.inkf` runs a script
  without a window, for pipelines, beside the existing `--export`.

**One undo step.** A script's changes undo all at once with Cmd+Z, however
much it did.

**Decide: if a script fails partway, roll back?** Recommended: yes. An
error undoes everything the script had changed and shows where it went
wrong, so a broken script never leaves a project half-edited.

**Responsive while running.** A script works on its own copy of the
project on a background thread, with a Stop button. When it finishes, its
copy becomes the project, as one undo step. Inkframe never freezes, and
stopping a runaway loop is always possible.

**Decide: frame numbers from 1 or 0?** Recommended: from 1, as the
timeline shows them (and as Harmony's scripting counts). Animate's JSFL
counts from 0, so its scripts would need adjusting either way.

## Safety

Scripts are programs, and someone may run one they downloaded. In the
first version, a script can:

- change the open project (which undo reverses);
- open, save and export projects, to paths it names;
- write to the Script Console.

It can't read or write any other files, use the network, or run other
programs. That keeps the worst a bad script can do to overwriting a
project or export it was pointed at. Wider file access (reading a CSV of
dialogue, say) can come later behind an explicit setting.

## Steps

1. **Spike (small).** Add QuickJS, run a one-line script against a
   document, and check it builds on macOS and Windows in CI and what it
   adds to the app's size. Stop here and switch to Lua if it doesn't
   work out.
2. **The first API and the Scripts menu.** The list above, with the
   one-undo-step and roll-back behaviour, `--script` on the command line,
   tests that run scripts against documents, and `docs/SCRIPTING.md` as
   the reference.
3. **The Script Console** and three or four example scripts (stagger
   layers, rename by pattern, batch export with names, build a scene).
4. **The second round of API:** symbols, rigs, effects, particles,
   camera.

Each step is its own branch, reviewed and merged like the features before.

## Decided (2026-10-20)

1. **Language:** JavaScript, embedded with QuickJS, subject to the spike
   in step 1.
2. **On an error:** everything the script changed is rolled back, and the
   error is shown with its line.
3. **Frame numbers:** from 1, as the timeline shows them.

Still open: the scope of the first version (the list under *What a script
looks like*), which can be adjusted as step 2 goes.

## Spike results (2026-10-20)

The spike is the `inkframe-script` crate: `run(doc, source, limit)` runs
a script against a copy of the document, with `console.log`, an
`inkframe` object that can read the document and add a keyed layer, and
a time limit.

- **It works on macOS:** a script reads the document, changes it, and
  logs. A failing script leaves the document untouched and reports its
  line, syntax errors included. A runaway loop is stopped at the time
  limit. `std`, `os`, `require` and `fetch` don't exist, so scripts have
  no file or network access.
- **Size:** QuickJS adds 1.2 MB to the app (29.6 MB to 30.8 MB), and
  compiles in about six seconds.
- **Windows:** QuickJS is C, so the crate can no longer be checked for
  Windows from a Mac (that needs Microsoft's C compiler). CI's Windows
  runner builds and tests it for real. Local Windows checks now leave the
  crate out (`--exclude inkframe-script`), and once the app uses it they
  will have to leave the app out too, relying on CI.

Pending: the CI run on Windows, which happens once the crate is merged.

The Windows CI run passed (QuickJS builds and its tests run there), so
JavaScript stays.

## Step 2 (2026-10-20)

Built: the API in `docs/SCRIPTING.md` (documents, layers, keys with
easing, simple shapes, swatches, open, save, export, console), the
Scripts menu with a Stop button and one undo step, and
`inkframe --script`. The natives in `native.rs` pass plain values and JSON;
`prelude.js` builds the `Document` and `Layer` classes on them, which
avoided pulling in rquickjs's macro crates.

## Step 3 (2026-10-20)

Built: the Script Console area (`scripts.rs`, Run with Cmd+Enter, Stop,
output), and six example scripts in `examples/scripts`, built into the app
and copied into the Scripts folder the first time it's made. A test runs
every example on the rig sample.

## Step 4 (2026-10-20)

Built: symbols (library, convert, instances and their playback), rigs
(pivots, children, master controllers), effects (`effectsAt` /
`setEffects`), particles (`addParticleLayer`, `particles`) and the camera
(`camera`, `addCamera`, `stackByDepth`). Effects and emitters use their
saved-file form with keys and type names in camelCase (`values::to_script`
and `from_script`). A seventh example, `snowy-night.js`, uses them. This
completes the plan.

