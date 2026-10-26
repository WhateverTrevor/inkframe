# Frame by Frame

Keyframes and tweens move a drawing around; they don't change it. For a character to blink, talk or turn its head, you need a new drawing: the hand-drawn, frame-by-frame animation of traditional studios. Inkframe gives every layer a set of *drawings* to swap between, and the tools to make them quickly: inbetweens drawn for you, shape tweens, a light table, an exposure sheet and automatic lip sync.

In this chapter, we're going to cover the following main topics:

- Drawings and drawing swaps
- Frame commands
- Inbetweens and shape tweens
- The light table
- The exposure sheet
- Lip sync

## Technical requirements

Start a new project with [[⌘N]]. The later sections use the *Rig* example (**File › Open Example…**).

## Drawings and drawing swaps

A layer can hold several named *drawings*, and show a different one from any frame on, the way Toon Boom Harmony's drawing substitution works. A mouth layer might hold drawings called *closed*, *A* and *O*; a hand, *open* and *fist*.

Select a layer, and its drawings are listed in the **Drawings** section of Properties:

- **New** adds a blank drawing from the current frame on. **Duplicate** adds a copy of the one showing, to change, which is usually quicker.
- Click a drawing's name to show it from the current frame on.
- [[[]] and [[]]] flip to the previous and next drawing.

Whatever you draw goes into the drawing that's showing. The timeline marks each swap with the drawing's name, as on the Rig's mouth in {{fig:light-table}}.

To animate frame by frame, then:

1. Draw the first pose.
2. Move a few frames on (two frames per drawing, "on twos", is traditional) and click **Duplicate**.
3. Change the copy into the next pose, and repeat.

## Frame commands

The frame commands work as they do in Animate. [[⇧]]-click or [[⇧]]-drag along a layer's row to pick frames first, or they apply to the current frame:

| Command | Key | What it does |
|---|---|---|
| **Insert Frames** | [[F5]] | Holds what shows there longer, pushing what follows later. |
| **Remove Frames** | [[⇧F5]] | Brings what follows earlier. |
| **Insert Blank Keyframe** | [[F7]] | Starts an empty drawing on the frame. |
| **Reverse Frames** | | Plays the picked frames backwards. |
| **Cut Frames** | [[⌘X]] | Takes the picked frames' keyframes out, to paste elsewhere. |
| **Copy Frames** | [[⌘C]] | Copies the picked frames' keyframes. |
| **Paste Frames** | [[⌘V]] | Pastes them at the playhead, on the selected layers. |

Cut, Copy and Paste act on frames while frames are picked, as in Animate, and on layers otherwise; they're also on a row's right-click menu. On a layer drawn frame by frame, the frames' drawings come along too, so a stretch of mouth shapes or a blink can be copied to another mouth or another time; a drawing the target already has is reused. Copied frames paste onto the same number of layers, row for row, or a single row onto every selected layer, replacing the keys they land on.

**Timeline › Edit Multiple Frames** (or its button beside the onion skin) lets you move a drawing on many frames at once: dragging a layer then moves it on every picked frame, rather than just the current one.

## Inbetweens and shape tweens

In a traditional studio, a lead animator draws the *key* drawings and an assistant draws the *inbetweens*. Inkframe can be your assistant. Put the playhead between a key drawing and the next one it swaps to, and choose **Timeline › Draw Inbetweens** (or **Inbetweens** in the context bar). A new drawing appears on every frame between, each the first key morphed that much further toward the next. They're ordinary drawings, so touch each one up by hand.

![Inbetweens drawn between a square and a star, seen through onion skins](inbetweens)

A *shape tween* does the same morph live, without making drawings, as Animate's shape tweens do. Give a layer a second drawing on a later frame, then choose **Timeline › Create Shape Tween** on any frame before the swap (or tick **Shape tween into the next drawing** in the Drawings section). The span turns green in the timeline, and the drawing morphs smoothly into the next: shapes change, colours blend, and shapes with no partner shrink away.

If a morph twists in an odd way, add *shape hints* with **Modify › Add Shape Hint** ([[⇧⌘H]]). Drag hint **a** to a point on the first drawing, then, on the swap's frame, to the matching point on the second; those points will meet.

![A square shape-tweening into a circle, with a shape hint (a) on its corner](shape-tween)

## The light table

When you're drawing one part of a scene, everything else can get in the way. **View › Light Table**, or the lamp button in the timeline bar, fades every layer but the selected one, so the drawing you're working on stands out, as on Harmony's light table.

![The light table: only the selected head is shown at full strength](light-table){crop=0.095,0.14,0.664,0.558}

## The exposure sheet

Traditional animators time their drawings on an *exposure sheet* (or X-sheet): a table with a row for each frame and a column for each layer. **Timeline › Exposure Sheet** shows one, for every layer that swaps drawings, plus each sound's loudness to time dialogue against.

A name marks where a drawing starts, and a line shows it held. Click a cell to go to that frame. Double-click a cell and type a drawing's name, then [[Enter]], to show that drawing there and move down a row; a new name makes a new, blank drawing. Right-click a cell for a list of the layer's drawings.

![The exposure sheet for the Rig's mouth: closed, then A on frame 7, E on 13 and O on 17](xsheet){crop=0,0,0.4,0.65}

## Lip sync

Matching a mouth to dialogue by hand is slow work. Inkframe can do it for you:

1. Give the mouth layer a drawing for each mouth shape, named with the standard letters: **A** (closed: M, B, P), **B**, **C**, **D** (wide open), **E**, **F**, **G**, **H** and **X** (at rest). A few are enough; shapes without a drawing borrow a similar one's.
2. Import the dialogue with **File › Import Audio…**. Sounds show as waveforms in the timeline; drag one to retime it.
3. Select the mouth layer and choose **Modify › Lip Sync…**.
4. Pick the sound and where the mouth shapes come from: **Rhubarb**, which listens for the actual sounds of speech; **Loudness**, which opens the mouth wider as the voice gets louder; or a timing file from Rhubarb or Papagayo.
5. Check which drawing is used for each shape, and click **Apply**.

The drawing swaps land under the sound, ready to play.

![The Lip Sync window: Rhubarb listens for the sounds, and each mouth shape is matched to a drawing](lipsync){crop=0,0,0.45,0.75}

> Tip: For Rhubarb, typing what's said into the box helps it with unclear words. Choose the **Other** language for speech that isn't English: it then listens for sounds rather than words.

## Summary

In this chapter, we animated frame by frame with drawings and drawing swaps, and used Animate's frame commands. We let Inkframe draw inbetweens and shape tweens, isolated our work on the light table, timed drawings on the exposure sheet, and lip-synced a mouth to dialogue.

In the next chapter, we'll learn to reuse artwork with symbols, and keep a growing project organised.
