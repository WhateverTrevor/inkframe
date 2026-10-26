# Automating with Scripts

Some jobs are the same few clicks over and over: delaying forty layers by a frame each, renaming a hundred drawings, rigging the fifth character just like the first four, exporting every shot with the date in its name. A *script* does such jobs for you. Inkframe's scripts are short JavaScript files, the language of web pages, and you don't need to be a programmer to use the ones that come with it or to change them a little. This chapter is a gentle start.

In this chapter, we're going to cover the following main topics:

- Running scripts from the Scripts menu
- Trying things out in the Script Console
- Making and animating layers
- Rigging a character from a script
- Mouths and web buttons
- Scripts from the command line

## Technical requirements

Start a new project with [[⌘N]]. A plain text editor is handy for keeping scripts you write, such as TextEdit set to plain text, Notepad, or a code editor if you have one.

## Running scripts from the Scripts menu

The **Scripts** menu lists the scripts in Inkframe's Scripts folder. The first time Inkframe runs, it puts its example scripts there for you, each beginning with a comment that says what it does and what you might change:

| Script | What it does |
|---|---|
| `bouncing-ball.js` | Builds a bouncing ball with squash and stretch: the one to read first |
| `walk-cycle.js` | Draws a character and rigs a 24-frame walk on a skeleton |
| `stagger-selected.js` | Delays each selected layer's animation a little more than the last |
| `fade-in-out.js` | Fades the selected layers in and out at their ends |
| `number-layers.js` | Renames layers to a prefix and a number |
| `confetti.js` | Scatters forty tumbling pieces of confetti |
| `snowy-night.js` | Adds snow, tints the scene blue and pushes the camera in |
| `export-review.js` | Exports a half-size video beside the project, named with the date |

Choose one, and it runs on the open project. When it finishes, a window shows anything it printed.

> Important note: A script's changes undo in one step, with [[⌘Z]], however many it made. A script that fails part way through changes nothing at all, and says on which line it stopped.

**Scripts › Open Scripts Folder** shows the folder, so you can add scripts of your own or keep copies of the examples you've changed.

## Trying things out in the Script Console

To write a script, it's easiest to start in the **Script Console**. Choose it from any area's menu (under Project). Type a script in the box at the top and click **Run**: it runs on the open project straight away, and what it prints with `console.log` appears underneath.

Type this, and click **Run**:

```
const doc = inkframe.document;
const ball = doc.addLayer("Ball", { ellipse: [120, 120], fill: "#e8543c", at: [960, 200] });
ball.setKey(1, { position: [960, 200] }, { easing: "ease-in" });
ball.setKey(12, { position: [960, 880] }, { easing: "ease-out" });
ball.setKey(24, { position: [960, 200] });
```

A ball appears, and pressing [[Enter]] plays it falling and bouncing back.

![The Script Console: a script that makes and animates a ball, and what it printed](script-console)

Here's what each line does:

- `inkframe.document` is the open project. Giving it a short name, `doc`, saves typing.
- `doc.addLayer(name, options)` adds a layer: here an ellipse 120 pixels wide and tall, filled orange, at a point on the stage.
- `ball.setKey(frame, values, options)` keys properties on a frame, just as moving the ball there would, with the easing to the next key.

Frames count from 1, as the timeline shows them. Positions are in stage pixels, from the top-left corner.

> Tip: Scripts can change many layers at once. `doc.layers` is every layer, top first, and `doc.selectedLayers` the selected ones: `doc.selectedLayers.forEach((layer, i) => layer.shiftKeys(i * 2))` staggers the selected layers' animation by two frames each.

## Rigging a character from a script

Scripts can build the skeletons of *Chapter 7*, too. The `walk-cycle.js` example builds the whole character of *Chapter 11*: run it from the Scripts menu, and a character walks on the spot. Here's how it rigs a leg, in a few lines:

```
const leg = doc.addLayer("Leg", { rect: [50, 340], at: [960, 600] });
const rig = doc.addSkeleton([leg], "Walker");
rig.addBone("Thigh", [960, 430], [960, 600])
   .addBone("Shin", [960, 600], [960, 770], { parent: "Thigh" })
   .addBone("Foot", [960, 770], [1010, 770])
   .setBone("Shin", { target: "Foot", limits: [0, 150] })
   .poseBone("Foot", 12, { moveTo: [1050, 770] });
leg.follows = { limb: "Thigh" };
```

- `addSkeleton` puts layers in a folder with a skeleton, as **Modify › New Skeleton** does.
- `addBone` draws a bone from one point to another, hanging from a `parent` or as a root of its own.
- `setBone` sets a bone's **Target**, **Limits**, strength or dynamics.
- `poseBone` keys a pose: moving the foot on frame 12, and the leg bends to reach it.
- `follows` binds a layer to its limb, as in *Chapter 11*.

![A character the walk-cycle script drew and rigged, mid-stride](script-walk)

## Mouths and web buttons

The talking head and the banner of *Chapter 12* and *Chapter 13* can be scripted as well:

- `layer.addDrawing("A", { copy: "closed" })` adds a drawing, and `layer.showDrawing("A", 12)` shows it from frame 12.
- `doc.importAudio("line.wav")` brings in a sound file, and `mouth.lipSync("line")` times the mouth's drawings to it from its loudness. For mouths that follow the actual words, have Rhubarb listen, as the Lip Sync window does: `mouth.lipSync("line", { rhubarb: true, dialogue: "Hello there!" })`.
- `layer.button = { openLink: "https://example.com" }` makes a layer a web page button, and `doc.setFrameAction(36, "stop")` stops the page on frame 36.

> Important note: A script can only reach the files you name in it: projects it opens and saves, exports, sound files and lip sync timing files. It can't read other files, use the internet or run programs (beyond the Rhubarb Lip Sync that comes with Inkframe), so a script from someone else can't do more than change your project.

**docs/SCRIPTING.md**, in Inkframe's source, lists everything scripts can do.

## Scripts from the command line

Scripts don't need the window at all. In a terminal, this runs a script on a project and saves it:

```
inkframe --script stagger.js shot.inkf --save
```

It's handy for jobs across many projects, such as exporting every shot of a film overnight, run by a scheduler or another tool.

## Summary

In this chapter, we ran the example scripts from the Scripts menu, wrote a first script in the Script Console, and made and animated layers with it. We saw how scripts rig characters with skeletons, time mouths to dialogue and make web buttons, what they can and can't reach, and how to run them without opening the window.

That's the end of the book. Appendix A lists every tool and its key, and Appendix B every command and where to find it. Now go and make something move.
