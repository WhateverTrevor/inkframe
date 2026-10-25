# Symbols, the Library and Organising

As a project grows, the same artwork turns up again and again: a tree in a forest, a character in every shot, a walk cycle that repeats. Drawing it each time would be slow, and changing it later slower still. *Symbols* solve this, as they do in Animate: draw something once, and place it as often as you like. This chapter covers symbols and the Library that holds them, and the folders and templates that keep a project tidy.

In this chapter, we're going to cover the following main topics:

- Making and editing symbols
- The Library
- Swapping symbols
- Folders and templates

## Technical requirements

Open the *Bouncing ball* example (**File › Open Example…**). The section on folders uses the *Rig* example.

## Making and editing symbols

A *symbol* is a piece of artwork, with its own layers and its own timeline, stored once in the project. Each place it's used is an *instance*: a layer that shows the symbol, and that can be moved, scaled, tweened and faded like any other. Change the symbol, and every instance changes with it.

To make one, select one or more layers and press [[F8]] (**Modify › Convert to Symbol**). Their animation, rigs and masks come along, and the layers become a single instance in the timeline. Let's try it with the bouncing ball:

1. Press [[⌘A]] to select both the *Ball* and *Floor* layers.
2. Press [[F8]]. The two layers become one instance, *Symbol 1*, holding the whole bounce.
3. Double-click the instance on the stage to step inside it. The rest of the scene fades, and a bar above the stage shows where you are. Inside, the ball and floor are layers again, to change as you like.
4. Press [[Esc]] to step back out.

Each instance has its own playback settings in Properties. **Loop** repeats the symbol's timeline for as long as the instance lasts, so a 12-frame walk cycle keeps walking; **Play Once** plays it through and holds the last frame; **Single Frame** shows one frame only, handy for a symbol holding a set of poses. **Symbol length** sets how long the symbol's own timeline is, and **First frame** where each instance starts in it.

> Tip: You can also make a symbol from just some of a layer's shapes: double-click the layer, pick the shapes, and press [[F8]]. They go on a layer of their own just above.

## The Library

The **Library** (in any area's menu, under Project; it's a tab beside Properties in the default layout) holds everything a project reuses: symbols, imported pictures and sounds, each shown as a card with a picture.

![The Library, previewing the bouncing ball symbol, with pictures filed in a Reference folder](library)

With the Library, you can:

- **Place** a card by dragging it onto the stage, where it lands right there, or onto the timeline, where it starts on that frame.
- **Preview** a card by clicking it. A symbol plays in a loop at the top, a picture shows larger, and a sound shows its waveform with a button to hear it.
- **Edit** a symbol by double-clicking its card.
- **File** cards into folders: the folder button makes one, and you drag cards onto it.
- **Find** cards by name with the search box, and set the card size with the slider.
- **Rename**, **move** or **delete** a card by right-clicking it. A card still used on the stage can't be deleted.

Pictures (PNG, JPEG, GIF and WebP) come in with **File › Import to Stage** ([[⌘R]]), or by dragging them onto the window. Sounds come in with **File › Import Audio…**.

## Swapping symbols

**Modify › Swap Symbol…** (or **Swap…** in an instance's properties) replaces what an instance shows, keeping its animation, timing and playback. Choose **Whole layer** to swap it throughout, or **From frame … on** to swap it partway through a shot: a hand changing pose, or a head turning, while the rig carries on moving. The timeline marks each swap with the symbol's name.

**Duplicate Symbol** copies the chosen symbol first, so you can swap to the copy and change it without touching the original.

![Swap Symbol: the instance shows a copy of the symbol from frame 13 on](swap)

## Folders and templates

Characters soon need a dozen layers or more. **Folders** keep them in order: select some layers and choose **Modify › New Folder** (or the folder button in the timeline bar). Click a folder's triangle to close it up. Its eye and padlock hide and lock everything inside, and moving, copying or deleting it takes its contents along.

![The Rig's arm layers gathered in a folder](folder){crop=0,0.72,0.7,0.28}

**Templates** keep artwork for *every* project, not just this one: a rigged character, a walk cycle, a prop. Open the **Templates** editor (in any area's menu, under Project), select the layers, type a name and click **Save Selected**. Their animation, symbols, pictures and paint are saved with them. To use a template, double-click it (or click **Insert**), and it's added at the playhead, its animation starting there.

![Templates holding the Rig's arm and head, ready for any project](templates)

## Summary

In this chapter, we made symbols from layers and edited them in place, set how each instance plays, and used the Library to place, preview and file symbols, pictures and sounds. We swapped one symbol for another partway through a shot, gathered layers into folders, and saved artwork as templates for every project.

In the next chapter, we'll build characters you can pose: rigs.
