# Project: An Interactive Web Banner

Animation isn't only for video: it lives on web pages too, and there it can be clicked. In this project, you'll make a web banner whose title pops in, a line of text fades up, and a button slides in that opens a link when it's clicked, then stops and waits. It uses motion presets from *Chapter 4* and the Web Page export from *Chapter 10*.

In this chapter, we're going to cover the following main topics:

- Setting up the stage
- Adding the words
- Animating with motion presets
- Making a button
- Stopping at the end, and exporting

## Technical requirements

Start a new project with [[⌘N]]. You'll need a web browser to try the result.

## Setting up the stage

A banner is wide and short. In the **Document** tab of Properties, set the stage to **970** × **250**, a common banner size, and **Duration** to **36** frames: a second and a half.

Draw a rectangle covering the stage for the background, on a layer called *Background*. In its style, set **Fill** to **Linear** and pick two warm colours, so it blends from one to the other.

## Adding the words

Press [[T]] for the **Text** tool, click near the left of the stage and type the title, *Spring Sale*. Set its size in the **Layer** tab. Add a second text layer under it, *Tagline*, for a smaller line about the offer.

## Animating with motion presets

Rather than keying each layer by hand, use ready-made movements:

1. Open **Motion Presets** from any area's menu (under Animation).
2. Select the *Title* layer, put the playhead on frame **1**, pick **Pop In** and click **Apply**: the title grows in from nothing, a little past full size, and settles.
3. Select the *Tagline*, go to frame **7**, and apply **Fade In**.

Play it: the title pops, then the line fades up beneath it.

## Making a button

1. Draw a rounded rectangle on the right of the stage, on a layer called *Shop Now*.
2. Add a text layer on it saying *Shop now*, and set its **Parent** to *Shop Now*, so it moves with the button.
3. With the playhead on frame **13**, apply **Slide In from Right** to the *Shop Now* layer.
4. In the *Shop Now* layer's properties, set **Web button** to **Open link**, and type the address the banner should open.

The button now lights up under the pointer in the exported page, and opens the link when clicked.

![The banner on its last frame, with the button's Open link action in its properties](banner)

> Tip: A button can also **Play**, **Stop**, or **Go to** a frame, to make a banner the viewer controls: a Replay button that goes to frame 1, say.

## Stopping at the end, and exporting

Left to itself, a web page export loops. A banner should end on its button and wait. Go to the last frame, open **Timeline › Frame Action**, and set the frame's action to **Stop**. The ruler marks the frame with an "a".

Then export:

1. Press [[⌘E]] and set **Format** to **Web Page (.html)**.
2. Leave **Transparent** off, so the background is drawn.
3. Click **Export…** and save the page.

Open the page in a browser. The title pops, the line fades up, the button slides in and the banner stops. Hover over the button to see it light up, and click it to open the link. The page is one file with nothing else to load, so it can go straight onto a website.

![The Export window, set to export a web page](banner-export){crop=0,0,0.4,0.36}

## Summary

In this project, we set up a banner-sized stage, animated its words with motion presets, made a button that opens a link, stopped the animation on its last frame, and exported it as a web page to try in a browser.

That's the end of this book's projects. Appendix A lists every tool and its key, and Appendix B every command and where to find it. Now go and make something move.
