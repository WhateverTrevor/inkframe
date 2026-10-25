# Meet Inkframe

Every animation program is a desk with the same few things on it: somewhere to draw, something to draw with, and a way to see time. Inkframe arranges them the way Adobe Animate does, with a toolbar down the left, the picture in the middle and the timeline along the bottom, and lets you rearrange them the way Blender does. In this chapter, you'll find your way around that desk, learn to bend it to the way you work, and meet the one shortcut that finds everything else.

In this chapter, we're going to cover the following main topics:

- A tour of the window
- The first-run tour
- Areas, tabs and workspaces
- Finding any command
- Opening the examples

## Technical requirements

You'll need Inkframe installed and open. We'll use the *Bouncing ball* example, which comes with the app: choose **File › Open Example…** and click **Open** beside it.

## A tour of the window

When Inkframe opens, it gives you an empty stage 1920 × 1080 pixels in size, two seconds long at 24 frames a second. Open the *Bouncing ball* example and click the ball, and your window will look like {{fig:workspace}}.

![The Inkframe window, with the Bouncing ball example open and the ball selected](workspace){callouts=1@0.33,0.016 2@0.06,0.57 3@0.55,0.083 4@0.2,0.3 5@0.95,0.128 6@0.8,0.85 7@0.7,0.016 8@0.075,0.63 9@0.975,0.016}

Here's what each numbered part is for:

1. **The menu bar**: every command, grouped as in Animate: **File**, **Edit**, **Modify**, **Select**, **Timeline**, **View**, **Scripts** and **Help**.
2. **The toolbar**: the drawing, painting, rigging and camera tools. Tools that share a slot have a small triangle in their corner; right-click the slot, or press the tool's key with [[⇧]], to switch between them.
3. **The context bar**: the current tool, and buttons for what you can do right now. With a layer selected, it offers **Symbol**, **Distort**, **Envelope** and **Break apart**; with a bone tool, it offers to add bones or pose them. Its right end turns on the rulers, the perspective guide, onion skins, the light table and rig setup.
4. **The stage**: your picture. What's inside the frame is what gets exported; you can draw outside it too, for things that will move in.
5. **Properties**: settings for whatever you're working on, in three tabs: **Tool** (the current tool's settings), **Layer** (the selected layer's) and **Document** (the whole project's: its size, frame rate and length). A long panel is split into sections, such as Transform and Easing: click a section's title to fold it away, and again to open it. Inkframe remembers what you've folded.
6. **The timeline**: your layers, one per row, and their keyframes over time. The red marker is the *playhead*, the frame you're looking at.
7. **Find a Command**: a search box for every command and tool. More on this shortly.
8. **Fill and Stroke**: the colours new shapes are drawn in. The ⊘ beside each turns it off.
9. **The workspace menu**: ready-made arrangements of the window, for animating, designing and painting.

> Tip: Hover over any tool or button to see what it does. For tools, the tooltip also names the other tools sharing its slot and their keys.

### Moving around the stage

You'll spend most of your time on the stage, so it's worth learning to move around it early:

- **Scroll**, or hold [[Space]] and drag, to pan.
- **Pinch** on a trackpad, or hold [[⌘]] and scroll, to zoom. The zoom level is shown in the menu bar.
- [[⌘0]] fits the whole stage in view again.
- [[⇧]]+[[Space]] and drag turns the view, as you'd turn a sheet of paper to draw a curve comfortably. [[4]] and [[6]] turn it 15° at a time, and [[5]] puts it upright. Only your view turns, never the drawing.

### Playing and stepping through time

Press [[Enter]] to play the animation and [[Enter]] again to stop. The loop button among the play controls repeats it. [[,]] and [[.]] (or the arrow keys) step back and forward a frame at a time, and [[Home]] and [[End]] jump to the first and last frames. You can also click or drag along the numbers at the top of the timeline (the *ruler*) to *scrub*: to move through time by hand, watching the picture change.

## The first-run tour

The first time Inkframe opens, it offers a tour of its own: a card for each part of the window, with that part lit up and the rest dimmed. Click **Next** (or press [[Enter]]) to move on, **Back** (or [[←]]) to go back, and **Skip tour** (or [[Esc]]) to leave it. Take it again at any time with **Help › Take the Tour**.

![The tour's card for the toolbar, with the rest of the window dimmed](tour)

## Areas, tabs and workspaces

Every part of the window is an *area*, and every area can show any *editor*: the stage, the timeline, Properties, the Library and a dozen more. Click the name in an area's top-left corner and a menu of every editor appears, grouped by what it's for, as {{fig:area-menu}} shows.

![Every area's menu lists all the editors it can show](area-menu){crop=0,0,0.62,0.36}

You can change an area in several ways:

- **Pick another editor** from its menu, or click **+** beside its name to add it as a *tab*, so the area holds both.
- **Resize** areas by dragging the border between them.
- **Split** an area by dragging from its corner into it, or **join** two by dragging from a corner into the neighbour, as in Blender.
- **Move a tab** by dragging it onto another area's header.
- [[Ctrl]]+[[Space]] fills the whole window with the area under the pointer, and again puts it back.

Inkframe remembers your arrangement between sessions. If you get lost, **View › Reset Workspace** puts everything back.

### Workspaces

Rather than arranging areas yourself, you can pick a ready-made *workspace* from the menu at the top right of the window (or **View › Workspace**):

- **Essentials**: the layout you've seen so far.
- **Animator**: a taller timeline, with the **Graph Editor** and **Exposure Sheet** as tabs beside it.
- **Designer**: **Swatches** and **Align** beside Properties, for laying out artwork.
- **Painter**: tool settings, swatches and history up front, for painting.

![The Animator workspace gives the timeline more room, with the Graph Editor and Exposure Sheet a click away](workspace-anim)

## Finding any command

Inkframe has a lot of commands, and nobody remembers where they all live. You don't have to: press [[⌘K]], or click **Find a command** in the menu bar, and type what you want. It finds commands and tools by their names, and also by what they're *for*. Type "bulge" and it suggests **Envelope**, the command that bulges and bends a drawing, as {{fig:finder}} shows.

![Find a Command finds Envelope from the word "bulge", and shows where it lives in the menus](finder){crop=0.22,0,0.56,0.3}

Use the arrow keys to choose a result and [[Enter]] to run it. Each result shows which menu the command is in and its shortcut, so you'll pick up the menus as you go.

> Tip: Try typing what you want to do in plain words: "rig", "web page", "fill", "mouth". Find a Command is the quickest way to learn Inkframe.

## Opening the examples

**File › Open Example…** shows the projects that come with Inkframe, each with a picture and a line on what it shows off. Each one opens as a new, untitled project, so you can change it freely and save your own copy wherever you like.

![File › Open Example lists the example projects](examples){crop=0.27,0.24,0.46,0.52}

| Example | What it shows |
|---|---|
| **Bouncing ball** | Keyframes, easing and squash and stretch. You'll build it yourself in *Chapter 4*. |
| **Rig** | A character built from parented parts, with lip sync on the mouth. |
| **Ink and paint** | Line art coloured with the paint bucket from palette swatches, as in Harmony. |
| **Multiplane** | Scenery at different depths, with the camera dollying in. |
| **Particles** | A campfire of fire, smoke and sparks under falling snow. |

### Saving your work

Press [[⌘S]] to save. Inkframe projects are `.inkf` files. While a project has unsaved changes, Inkframe also keeps a recovery copy every 30 seconds. If it ever closes without saving, the next time it starts it offers the work back.

> Important note: [[⌘Z]] undoes anything, and [[⇧⌘Z]] redoes it. The **History** editor (in any area's menu, under Project) lists every change by name, so you can jump back several steps at once.

## Summary

In this chapter, we toured Inkframe's window (as its own first-run tour does): the menu bar, toolbar, context bar, stage, Properties and timeline. We saw how areas can show any editor and be split, joined and rearranged, and how workspaces set them up for different jobs. Most usefully, we met Find a Command ([[⌘K]]), which finds any command by name or by what it does.

In the next chapter, we'll start drawing.
