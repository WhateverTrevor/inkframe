# Drawing with Vectors

Most of what you draw in Inkframe is *vector* artwork: shapes made of points and the curves between them, rather than pixels. Vectors stay crisp at any size, they're light enough to animate by the hundred, and every point stays editable, so a drawing can be reshaped, bent by bones or morphed into another long after you drew it. This chapter covers the drawing tools, from the simple shapes to the Pen, and the ways to reshape, combine and style what they make.

In this chapter, we're going to cover the following main topics:

- Layers, and drawing into them
- Shapes, the Pencil and the Pen
- Reshaping with the Subselection and Width tools
- Working inside a layer, and combining shapes
- Fills, gradients and line styles
- Art brushes and line textures
- Rulers, guides and perspective

## Technical requirements

Start a new project with [[⌘N]]. A few sections open the *Bouncing ball* example (**File › Open Example…**).

## Layers, and drawing into them

A picture in Inkframe is a stack of *layers*, listed in the timeline with the top layer drawn in front. Each layer can be moved, hidden (with the eye beside its name), locked (with the padlock) and animated on its own, so anything that needs to move separately should have a layer of its own: a ball and the floor it bounces on, or a character's head and body.

What you draw goes into the *selected* layer, as in Animate. On an empty stage, the first shape you draw starts a layer. To start another, click **+** at the right of the timeline bar, or press [[⇧⌘N]]: an empty layer appears just above the selected one, ready to draw into. In {{fig:new-layer}}, a hat has been drawn into a new layer, above the ball.

![A new layer (Layer 1) above the ball, holding a freshly drawn hat](new-layer)

Some useful things to know about layers:

- **Rename** a layer in the **Layer** tab of Properties: its name is the box at the top.
- **Restack** a layer by pressing and holding its name until it lifts, then dragging it up or down. The ↑ and ↓ buttons move the selected one a place at a time.
- **Colour**: the small square at the right of each name sets the layer's colour, which marks it in the timeline and outlines it on the stage when it's selected.
- **Select several** with [[⇧]]-click or [[⌘]]-click on their names. Dragging any of them then moves them all.

> Tip: If you'd rather every shape got a layer of its own, untick **Draw into the selected layer** in the drawing tool's settings (**Properties › Tool**). Inkframe remembers your choice.

## Shapes, the Pencil and the Pen

The simplest drawing tools are the **Rectangle** ([[R]]) and **Oval** ([[O]]): drag on the stage to draw one, holding [[⇧]] for a square or a circle. They're drawn in the **Fill** and **Stroke** colours under the toolbar; click either to pick a colour, or ⊘ beside it for none.

The **Pencil** ([[Y]]) draws freehand lines. With a pen tablet, pressing harder draws a thicker line, and its ends taper as a brush's would. Both are options in its settings; with both off, it draws even strokes.

The **Text** tool ([[T]]) types on the stage: click and type, and press [[Esc]] when you're done. Text is real vector outlines, so it stays sharp at any size and animates like any other layer.

### The Pen

The **Pen** ([[P]]) draws precise paths, point by point:

1. **Click** to put down a corner point.
2. **Click and drag** to put down a smooth point, pulling out its curve handles as you drag.
3. To close the shape, click the first point. To leave the line open, press [[Enter]] or [[Esc]], or click the last point again.

While drawing, [[Backspace]] removes the last point and [[⇧]] keeps each segment at a 45° angle.

Between paths, the Pen also edits the selected shape's points, as in Animate. Click a point to show its handles, and drag the point or its handles. [[⌥]]-drag a corner point to pull curve handles out of it, turning the corner into a curve; [[⌥]]-click a point to switch it between corner and smooth. Click the outline between two points to add a point without changing the curve.

![A hill drawn with the Pen: its top corner turned into a curve by Alt-dragging handles out of it](pen-handles){crop=0.3,0.28,0.25,0.3}

## Reshaping with the Subselection and Width tools

The **Subselection** tool ([[A]]) edits the points of any shape, whichever tool drew it. Click a shape to see its points, then drag a point to move it, or drag a selected point's handles to change its curves. As with the Pen, [[⌥]]-drag a point to pull out new curve handles, [[⌥]]-click it to switch between corner and smooth, and hold [[⌥]] while dragging a handle to move it alone, for a sharp corner. [[Delete]] removes the selected point. Rectangles and ovals become ordinary paths the first time you edit their points.

The **Width** tool ([[⇧A]], sharing a slot with Subselection) shapes Pencil lines: drag sideways from any point on a line to make it wider or narrower there, for an ink line that swells and thins.

![Pencil lines shaped with the Width tool](pressure-lines){crop=0.44,0.34,0.3,0.26}

### Welded points

Where shapes in a layer meet at a point, such as a roof on a wall or a sleeve on an arm, the point is *welded*: shared between them, and marked with a dot by the Subselection tool and the Pen. Dragging a welded point moves every shape's copy of it, so they stay joined. Drag a point onto another shape's point to weld them; [[⌘]]-drag a point to pull it away on its own.

![Welded points: the house's walls and roof share corners, and the sleeve stays joined to the forearm as the arm bends](welds){crop=0.18,0.28,0.45,0.28}

## Working inside a layer, and combining shapes

The **Selection** tool ([[V]]) moves, scales and turns whole layers: drag a layer to move it, drag a square handle to scale it ([[⇧]] keeps its proportions), or drag the round knob above it to turn it ([[⇧]] snaps to 15°). [[⌥]]-drag the ring in the middle to move the *pivot*, the point it turns and scales around.

To work on the individual shapes *in* a layer, double-click it. The other layers fade, and a click now picks just the shape under it: [[⇧]]-click to pick more, or [[⌘A]] for all of them. Picked shapes can be moved, scaled, turned, deleted, copied and pasted, just like layers. Press [[Esc]], or double-click empty stage, to step back out.

### Combining shapes

With two or more shapes picked inside a layer, **Modify › Combine Shapes** (or **Combine** in the context bar) joins them in one of four ways:

| Command | What it does |
|---|---|
| **Union** | Merges them into one shape, styled as the top one. |
| **Intersect** | Keeps only where they all overlap. |
| **Punch** | Cuts the top shape out of each shape beneath it. |
| **Crop** | Trims each shape beneath to the top shape. |

**Break Apart** ([[⌘B]]) turns text into a shape for each letter, ready to reshape or animate one by one, and turns a Pencil line into the filled shape it draws.

![Combined shapes: two circles in a union, a square with a circle punched out, and text broken apart into letters](combine){crop=0.095,0.14,0.664,0.558}

### Distort and Envelope

**Modify › Transform › Distort** puts a handle on each corner of the selected drawing; drag them to lean it, taper it or put it into perspective. **Envelope** goes further: it puts a point on each corner and in the middle of each side, each with curve handles like the Pen's, so you can bow, bulge and wave the whole drawing. Press [[Enter]] or click away to finish.

![A striped card distorted into perspective, and a word bent by an envelope, its handles still showing](envelope){crop=0.095,0.14,0.664,0.558}

## Fills, gradients and line styles

Select a shape's layer and its style appears in the **Layer** tab of Properties (or in the **Tool** tab, for shapes you're about to draw):

- **Fill** can be **Solid**, **Linear** or **Radial**. A gradient lists its colours, each at a place along it: **+ Colour** adds one, × takes one away, and **Reverse** flips them. To move where a gradient starts and ends, double-click the shape and click it: a round handle marks the start and a square one the end.
- **Stroke** sets the outline's colour and width, its **Ends** (round, flat or square), its **Corners** (round, sharp or bevelled), and its **Line**: solid, dashed or dotted.

![A sky and a sun filled with gradients (the sun's handles showing), and dashed and dotted outlines](styles){crop=0.095,0.14,0.664,0.558}

## Art brushes and line textures

Any stroke can be drawn with a symbol from the Library (we'll meet symbols in *Chapter 6*). In the stroke settings, set **Brush** to **Art** to stretch the symbol's picture along the whole line, for tapered strokes and ribbons, or to **Pattern** to repeat it along the line, for vines, chains and stitches. The picture bends round the line's curves, and the line keeps its points, so you can still reshape it.

![A vine of leaves (a pattern brush), a tapered swoosh (an art brush) and a card framed with beads](brushes){crop=0.095,0.14,0.664,0.558}

For a hand-drawn look, a layer's **Lines** setting (in its properties) inks its lines with a paper grain: **Pencil**, **Charcoal** or **Chalk**, with a slider for how strongly it shows. The grain is fixed to the layer, so it moves with the drawing instead of crawling over it as the drawing animates.

![The same line inked flat, then with the Pencil, Charcoal and Chalk textures](textures){crop=0.095,0.14,0.664,0.558}

## Rulers, guides and perspective

**View › Rulers** ([[⇧⌥⌘R]]) shows rulers along the top and left of the stage, in stage pixels. Drag down from the top ruler, or right from the left one, to place a *guide*; drag a guide back onto its ruler to remove it. Layers snap to guides as you move them near one, as do rectangles and ovals as you draw them. **View › Guides** shows, locks and clears them.

![The rulers, with three guides placed](rulers)

**View › Perspective Guide** lays a one-, two- or three-point perspective grid over the stage: a horizon, and lines radiating from each vanishing point. Drag the vanishing points to set the view. With the guide showing, hold [[⇧]] while drawing with the Pen and each segment snaps toward a vanishing point, so boxes, buildings and roads line up.

![A two-point perspective guide, with a block drawn to it](perspective){crop=0.08,0.11,0.69,0.6}

> Tip: Neither guides nor the perspective grid appear in exports, so leave them on as long as they're useful.

## Summary

In this chapter, we drew with the shape tools, the Pencil and the Pen, and reshaped drawings with the Subselection and Width tools. We stepped inside layers to work on their shapes, combined shapes, and bent drawings with Distort and Envelope. We styled shapes with gradients and line styles, drew with art and pattern brushes and line textures, and lined drawings up with rulers, guides and perspective.

In the next chapter, we'll leave vectors behind for a while and paint with pixels.
