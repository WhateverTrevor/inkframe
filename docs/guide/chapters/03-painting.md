# Painting and Colour

Alongside its vector tools, Inkframe paints with pixels, much as Photoshop does: soft brushes, smudging, selections, adjustments and blend modes. Painted layers sit in the same timeline as drawn ones, so a painted background can sit behind vector characters, and both can be animated. This chapter covers painting, then Inkframe's tools for colouring line art and managing colour across a whole project.

In this chapter, we're going to cover the following main topics:

- Paint layers and the Brush
- Filling and retouching
- Selections, adjustments and blend modes
- Ink and paint: colouring line art
- Swatches and palettes
- Tracing a painting into vectors

## Technical requirements

Start a new project with [[⌘N]]. A pen tablet makes painting more natural, as its pressure thins and thickens the stroke, but a mouse works too. The section on palettes uses the *Rig* example.

## Paint layers and the Brush

Pixels live on *paint layers*. Pick the **Brush** ([[B]]) and paint on the stage: if the selected layer isn't a paint layer, your first stroke starts one (or use **Modify › New Paint Layer**). A paint layer's canvas is the size of the stage, pixel for pixel, and the layer can be moved, scaled, turned and animated like any other.

While a painting tool is in use, its settings show in the **Tool** tab of Properties, under a preview stroke:

- **Size**, **Hardness**, **Opacity** and **Flow** set the brush's tip.
- **Spacing**, **Smoothing** and **Paper grain** shape how strokes are laid down.
- **Presets** set them all at once, from an inking pen to chalk and an airbrush.
- **Pressure** decides what pressing harder does with a tablet: a thicker line, or a denser one.

[[[]] and [[]]] shrink and grow the brush as you paint. The **Eraser** ([[E]]) rubs paint away, and the **Eyedropper** ([[I]]), or [[⌥]]-click with the brush, picks up a colour from the stage.

![Strokes painted with a few brush presets, each pressed harder through its middle](paint){crop=0.095,0.14,0.664,0.558}

> Tip: For frame-by-frame painting, give the paint layer a new drawing on each frame (**Modify › New Drawing**). *Chapter 5* covers drawings in detail.

## Filling and retouching

The rest of the painting tools work as they do in Photoshop:

| Tool | Key | What it does |
|---|---|---|
| **Paint Bucket** | [[G]] | Fills the area of similar colour you click. **Tolerance** sets how similar; **Contiguous** keeps the fill to connected pixels. |
| **Gradient** | [[⇧G]] | Drag to blend from the paint colour to a second colour (or to transparent), linear or radial. |
| **Smudge** | [[U]] | Pushes paint around, as with a finger. |
| **Blur** and **Sharpen** | [[⇧U]] | Soften or crisp up the paint under the brush. |
| **Dodge** and **Burn** | [[D]], [[⇧D]] | Lighten or darken it. **Range** picks the shadows, midtones or highlights. |
| **Clone Stamp** | [[S]] | [[⌥]]-click where to copy from, then paint a copy elsewhere. |

![The same strokes over a gradient sky, smudged, blurred, dodged and burned, with a filled circle](paint-tools){crop=0.095,0.14,0.664,0.558}

## Selections, adjustments and blend modes

A *selection* limits what the painting tools change. Make one with the **Rectangular** or **Elliptical Marquee** ([[M]]), the **Lasso** or **Polygonal Lasso** ([[L]]), or the **Magic Wand** ([[W]]), which picks an area of one colour. Hold [[⇧]] to add to a selection and [[⌥]] to take away from it. With a painting or selection tool, [[⌘A]] selects everything, [[⇧⌘A]] nothing, and [[⇧⌘I]] inverts the selection; [[Delete]] clears what's selected.

Drag inside a selection with the Selection tool to move those pixels. **Select › Layer via Copy** ([[⌘J]]) puts them on a layer of their own, to scale and turn freely.

**Modify › Adjustments** changes the colours of the selected paint layer (or just the selection): **Brightness/Contrast**, **Hue/Saturation**, **Levels** and **Curves**, each previewed live as you drag.

Every layer also has a **Blend** mode in its properties, which sets how it mixes with what's beneath it: **Multiply** for shading, **Screen** for glows, and the rest of Photoshop's modes. In {{fig:paint-select}}, an elliptical selection's hue has been shifted, and the layer set to Multiply.

![An elliptical selection, its hue shifted, on a layer blended with Multiply](paint-select)

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

In this chapter, we painted on paint layers with the Brush and its presets, filled and retouched with the bucket, gradient, smudge and dodge tools, and limited them with selections. We adjusted colours and blended layers, coloured line art Harmony-style with the Paint Bucket, managed colour with palettes and their versions, and traced paintings into vectors.

In the next chapter, we'll make something move.
