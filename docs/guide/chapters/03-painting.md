# Painting and Colour

Alongside its vector tools, Inkframe paints with pixels, much as Photoshop does: soft brushes, smudging, selections, adjustments and blend modes. Painted layers sit in the same timeline as drawn ones, so a painted background can sit behind vector characters, and both can be animated. This chapter covers painting, then Inkframe's tools for colouring line art and managing colour across a whole project.

In this chapter, we're going to cover the following main topics:

- Paint layers and the Brush
- Brushes and Brush Settings
- Filling and retouching
- Selections, adjustments and blend modes
- Clipping masks and layer masks
- Ink and paint: colouring line art
- Swatches and palettes
- Tracing a painting into vectors

## Technical requirements

Start a new project with [[⌘N]]. A pen tablet makes painting more natural, as its pressure thins and thickens the stroke, but a mouse works too. The section on palettes uses the *Rig* example.

## Paint layers and the Brush

Pixels live on *paint layers*. Pick the **Brush** ([[B]]) and paint on the stage: if the selected layer isn't a paint layer, your first stroke starts one (or use **Modify › New Paint Layer**). A paint layer's canvas is the size of the stage, pixel for pixel, and the layer can be moved, scaled, turned and animated like any other.

While a painting tool is in use, its settings show in the **Tool** tab of Properties, under a preview stroke:

- **Brush** picks a brush, ready-made or your own, from a hard round to foliage and clouds.
- **Size**, **Hardness**, **Opacity** and **Flow** set the brush's tip.
- **Spacing** and **Smoothing** shape how strokes are laid down.
- **Pressure** decides what pressing harder does with a tablet: a bigger mark, a more solid one, or both.

[[[]] and [[]]] shrink and grow the brush as you paint, and [[⇧[]] and [[⇧]]] make it softer and harder. The **Eraser** ([[E]]) rubs paint away (on a drawn layer it cuts lines and fills instead; see the previous chapter), and the **Eyedropper** ([[I]]), or [[⌥]]-click with the brush, picks up a colour from the stage.

Painters flip the canvas to see a drawing afresh: mistakes in proportion jump out mirrored. **View › Flip View Horizontally** ([[H]]) mirrors the view, not the painting, and [[H]] again puts it back; the stage says **Flipped** meanwhile.

![Strokes painted with a few brush presets, each pressed harder through its middle](paint){crop=0.095,0.14,0.664,0.558}

> Tip: For frame-by-frame painting, give the paint layer a new drawing on each frame (**Modify › New Drawing**). *Chapter 5* covers drawings in detail.

## Brushes and Brush Settings

Inkframe's brushes work as Photoshop's do, setting for setting, so a brush you know from there behaves the same here. Each stroke is a row of *dabs*, stamps of the brush's *tip*, and every dab can vary: bigger or smaller, turned, scattered, recoloured.

The **Brushes** panel lists the brushes, each by a sample stroke. Besides round brushes, the ready-made ones are a painter's set for scenes and backgrounds: pencil, chalk and charcoal; dry, bristle and flat brushes; watercolour; and stamping brushes for foliage, grass, clouds, rocks, spatter and stars. Click one to paint with it. The panel's buttons save the brush as it is now as a new preset, make a tip from a picture file (**Import Tip…**), **Import** brushes (a brush set, or a Photoshop *.abr* file), **Export** yours as a brush set to share, and **Define from Selection**: a new tip from the selected pixels of a paint layer, dark being paint, as Photoshop's **Edit › Define Brush Preset** does. Right-click a saved brush to rename, update or delete it.

![Every ready-made brush, one stroke each](brush-sheet){crop=0.095,0.14,0.76,0.70}

**Brush Settings** (**Edit › Brush Settings**, or the button in the Tool tab) holds every setting, a section at a time, picked from the list down its left. The checkbox beside a section turns it on and off, keeping its settings.

| Section | What it does |
|---|---|
| **Brush Tip Shape** | The tip, round or sampled; its size, angle, roundness and flips; the hardness of a round tip; and the spacing between dabs. Drag the arrow in the circle to turn the tip, or a dot to squash it. |
| **Shape Dynamics** | How each dab's size, angle and roundness vary: a *jitter* (at random) and a *control* (pen pressure, pen tilt, a fade over some dabs, or, for the angle, the stroke's direction). |
| **Scattering** | Throws dabs off the path, across it or both ways, several at a time. |
| **Texture** | Presses a pattern (paper, canvas, grain…, or a picture: **Import…**, or **From Selection** as Photoshop's **Edit › Define Pattern** makes one) into the paint. **Height** catches paint on the pattern's peaks, like chalk on paper; **Multiply** and the others darken paint into its hollows. |
| **Dual Brush** | A second tip along the same path: the paint shows only where the two meet, for broken, bristly edges. |
| **Color Dynamics** | The colour wanders, per dab or per stroke: towards the second colour, or in hue, saturation and brightness. |
| **Transfer** | Each dab's opacity and flow, by chance or by pressure. |
| **Brush Pose** | A pen's tilt, turn and pressure, for painting with a mouse as if with a pen held so; **Override** uses them even with a pen. |
| **Noise**, **Wet Edges**, **Build-up** | Grit in soft edges; paint pooling at the rim like watercolour; paint that keeps flowing while the pen rests, like an airbrush. |

A stroke along the foot of the panel shows the brush as it stands.

![The Brushes panel above Brush Settings at Shape Dynamics, for the Foliage brush](brush-settings){crop=0.76,0.035,1.0,0.82}

**Symmetry**, in the bar above the stage while the Brush, Eraser or Pattern Stamp is picked, repeats every stroke as you paint: mirrored across the canvas's middle (**Vertical**, **Horizontal** or both), or turned round it (**Radial**, or **Mandala**, mirrored within each segment, with the number of **Segments** to choose). Its lines show faintly on the stage; drag the handle where they meet to move the centre, and **Centre on the Canvas** in the menu puts it back.

![A mandala painted with symmetry, mirrored foliage, and a band of Pattern Stamp](symmetry){crop=0.095,0.14,0.76,0.70}

For a quick change without leaving the canvas, right-click the stage with a painting tool: a small panel opens with the size and hardness over the brushes. Or hold [[⌃⌥]] and drag on the canvas (Alt and the right button on Windows): left and right resize the brush, up and down soften and harden it.

![The brush quick menu, opened by right-clicking the stage](quick-brush){crop=0.095,0.14,0.76,0.75}

> Tip: Painting scenery, start with big, soft shapes in the Soft Round, then break them up with the stamping brushes: Clouds for the sky, Foliage and Sponge for trees, Grass and Rocks for the ground. Color Dynamics varies their greens and browns, so a few strokes look like many leaves.

The *Painted landscape* example (**File › Open Example…**) is a scene painted that way, with a paint layer for each part: the sky, clouds, mountains, hills, trees and grass. Hide the layers one by one to see how it's built up, or paint over it.

## Filling and retouching

The rest of the painting tools work as they do in Photoshop:

| Tool | Key | What it does |
|---|---|---|
| **Paint Bucket** | [[G]] | Fills the area of similar colour you click. **Tolerance** sets how similar; **Contiguous** keeps the fill to connected pixels. |
| **Gradient** | [[⇧G]] | Drag to blend from the paint colour to a second colour (or to transparent), linear or radial. |
| **Lasso Fill** | [[⇧G]] | Draw round an area; it fills with the paint colour when you let go, the quickest way to block in colour. On a drawn layer it adds a fill under the lines. |
| **Mixer Brush** | [[⇧B]] | Paints wet, as Photoshop's does: **Wet** picks up and smears the paint under the brush, **Load** is how much paint it holds before running dry, and **Mix** how much of what it picks up blends into its paint. **Sample All Layers** picks colour up from everything as seen. |
| **Smudge** | [[U]] | Pushes paint around, as with a finger. |
| **Blur** and **Sharpen** | [[⇧U]] | Soften or crisp up the paint under the brush. |
| **Liquify** | [[⇧U]] | Moves the paint rather than adding any, as Photoshop's Liquify: **Push** drags it along the stroke, and **Bloat**, **Pucker** and the two **Twirls** swell it, shrink it or turn it round the brush's middle, working on while the brush is held still. |
| **Dodge** and **Burn** | [[D]], [[⇧D]] | Lighten or darken it. **Range** picks the shadows, midtones or highlights. |
| **Clone Stamp** | [[S]] | [[⌥]]-click where to copy from, then paint a copy elsewhere. |
| **Pattern Stamp** | [[⇧S]] | Paints a pattern with the whole brush: its dark parts in the paint colour, its light parts in the **Light colour**. Pick the pattern, or **Import…** a picture, in the Tool tab. |

![The same strokes over a gradient sky, smudged, blurred, dodged and burned, with a filled circle](paint-tools){crop=0.095,0.14,0.664,0.558}

## Selections, adjustments and blend modes

A *selection* limits what the painting tools change. Make one with the **Rectangular** or **Elliptical Marquee** ([[M]]), the **Lasso** or **Polygonal Lasso** ([[L]]), or the **Magic Wand** ([[W]]), which picks an area of one colour. Hold [[⇧]] to add to a selection and [[⌥]] to take away from it. With a painting or selection tool, [[⌘A]] selects everything, [[⇧⌘A]] nothing, and [[⇧⌘I]] inverts the selection; [[Delete]] clears what's selected.

Drag inside a selection with the Selection tool to move those pixels. **Select › Layer via Copy** ([[⌘J]]) puts them on a layer of their own, to scale and turn freely.

**Modify › Adjustments** changes the colours of the selected paint layer (or just the selection): **Brightness/Contrast**, **Hue/Saturation**, **Levels** and **Curves**, each previewed live as you drag.

To grade a whole scene and keep changing your mind, use **Modify › New Adjustment Layer** instead: the same four adjustments as a layer of their own, which change everything beneath it as it's drawn, painting and drawings alike. Its settings stay in its properties, so the grade can be changed, faded with the layer's opacity, kept to part of the picture with a layer mask, or hidden, at any time. Clipped to a layer (see *Clipping masks and layer masks*, below), it changes only that layer. In {{fig:adjustment}}, a Hue/Saturation layer on top warms the whole painted landscape.

![A Hue/Saturation adjustment layer warming the Painted Landscape, its settings in Properties](adjustment)

Every layer also has a **Blend** mode in its properties, which sets how it mixes with what's beneath it: **Multiply** for shading, **Screen** for glows, and the rest of Photoshop's modes. In {{fig:paint-select}}, an elliptical selection's hue has been shifted, and the layer set to Multiply.

![An elliptical selection, its hue shifted, on a layer blended with Multiply](paint-select)

## Clipping masks and layer masks

Two of Photoshop's ways of keeping paint where it belongs, without erasing anything.

A *clipping mask* shows a layer only where the layer beneath it has paint. Paint a figure, add a layer above it, and choose **Modify › Create Clipping Mask** ([[⌥⌘G]]), or tick **Clipped** in its properties: now shading painted over the figure, however loosely, stays on the figure. Several layers in a row can clip to the same layer, their *base*; the base's opacity and blend mode apply to them all, and hiding it hides them too. The timeline indents a clipped layer with an arrow pointing down to its base.

A *layer mask* hides parts of a layer and keeps them. **Modify › Layer Mask › Add** (or **Add** beside **Layer mask** in the properties) gives the layer a mask, and starts you painting it: the brushes, eraser and other painting tools paint the mask rather than the layer, dark hiding it and light showing it, so a soft brush gives a soft edge and erasing brings the layer back. The mask's picture sits beside the layer's in the timeline; click it to paint the mask, and click the layer's own picture to paint the layer again. The bar above the stage says which you're painting. From the same menu, **Turn Off** shows the whole layer while keeping the mask, **Unlink** leaves the mask where it is on the stage as the layer moves (and **Link** has it move with the layer again), **Apply** erases what it hides from a paint layer for good, and **Delete** removes it.

In {{fig:masks}}, the trees on the right are hidden by the Trees layer's mask, picked for painting, and evening shade painted right across the stage on a layer clipped to the Hills darkens only the hills.

![A layer mask hiding the trees on the right, and shade on a layer clipped to the hills](masks)

> Tip: **File › Export PSD…** saves the current frame as a layered Photoshop file, and Photoshop files can be imported with **File › Import to Stage** ([[⌘R]]), each layer arriving as a paint layer.

## Ink and paint: colouring line art

Colouring a character drawn in lines is a job for the **Paint Bucket** on a *vector* layer, as in Toon Boom Harmony:

1. Draw the character's outlines with the Pencil or Pen. In the tool's settings, make sure they're drawn as **Line Art**.
2. Pick the Paint Bucket ([[G]]) and a colour.
3. Click between the lines. The area they enclose fills with the colour, *underneath* the lines, so the line work stays crisp on top.

Lines rarely meet perfectly. **Close gaps** (**Small**, **Medium** or **Large**) in the Paint Bucket's settings lets lines that don't quite touch still hold the paint, as the gap in the face's outline in {{fig:ink}} does. Click a fill again to recolour it, or click inside a filled area (an eye in a face) to give it its own colour.

![An inked face filled with the Paint Bucket: the fill holds despite the gap in the outline](ink){crop=0.45,0.3,0.27,0.3}

The *Ink and paint* example (**File › Open Example…**) is a flower coloured this way.

## Swatches and palettes

The **Swatches** editor (in any area's menu, under Design; the *Designer* workspace shows it beside Properties) holds the project's palette. Click a swatch to use it, or **+** to add the current colour, and name it in its row.

A palette swatch *owns* everything painted with it. Right-click a swatch and change its colour, and every shape of that colour in the project changes with it. That makes it easy to try out a character's colours, and to keep them consistent across hundreds of drawings.

A palette can also have *versions*. **New Version** in the palette menu copies the palette, for a night-time version, say; recolour its swatches, then switch between versions to recolour the whole scene at once.

![The Rig character recoloured by switching its palette to a darker Night version](palette)

## Tracing a painting into vectors

**Modify › Vectorise…** traces the selected paint or picture layer into vector shapes, as Animate's Trace Bitmap does:

- **Line art** keeps the pixels darker than the **Threshold**, so a scanned pencil drawing becomes filled ink lines, ready for the Paint Bucket.
- **Colours** finds the picture's main colours and makes shapes of each.

**Minimum area** drops specks and fills pinholes, and **Smoothing** trades closeness for fewer points. A preview updates as you go. The tracing goes into a new layer above the original, which is hidden rather than deleted.

![Vectorise tracing painted strokes into line art](vectorise){crop=0,0,0.7,0.6}

## Summary

In this chapter, we painted on paint layers with the Brush and its presets, built brushes of our own in Brush Settings, filled and retouched with the bucket, gradient, smudge and dodge tools, and limited them with selections. We adjusted colours and blended layers, kept paint in its place with clipping masks and layer masks, coloured line art Harmony-style with the Paint Bucket, managed colour with palettes and their versions, and traced paintings into vectors.

In the next chapter, we'll make something move.
