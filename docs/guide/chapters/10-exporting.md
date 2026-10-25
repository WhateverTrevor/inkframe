# Exporting Your Work

An animation isn't finished until someone else can watch it. Inkframe exports to video for sharing and editing, to image sequences and sprite sheets for games, to vector formats for the web and apps, and to interactive web pages with buttons. This chapter covers each, and how to export many projects at once.

In this chapter, we're going to cover the following main topics:

- The Export window
- Choosing a format
- Interactive web pages
- Batch export

## Technical requirements

Open the *Bouncing ball* example (**File › Open Example…**), or any animation of your own.

## The Export window

Press [[⌘E]] (**File › Export…**) to open the Export window:

- **Format**: what to export. More on the choices next.
- **Background**: tick **Transparent** to leave the stage's background out, for formats that can keep transparency.
- **Resolution**: **25%**, **50%**, **100%** or **200%** of the stage's size.
- **Frames**: **All**, or a **Range** of them.

Click **Export…**, choose a name and a folder, and the export runs, with a progress bar.

![The Export window, set to export an MP4 video](export){crop=0,0,0.4,0.36}

## Choosing a format

Each format suits a different destination:

| Format | Transparency | Use it for |
|---|---|---|
| **H.264 (.mp4)** | No | Videos that play anywhere: review copies, social media, the web. |
| **Apple ProRes 4444 / 4444 XQ** | Yes | High-quality masters for video editing and compositing. |
| **Apple ProRes 422 HQ** | No | High quality, with smaller files than 4444. |
| **HEVC with alpha** (macOS) | Yes | Small files with transparency, for Apple devices and Safari. |
| **Animated GIF** | On or off per pixel | Sharing anywhere: it loops forever, in 256 colours. |
| **PNG sequence** | Yes | A lossless image for every frame, for any other tool. |
| **Sprite sheet** (.png + .json) | Yes | Games: every frame packed onto an image, with data for Phaser, PixiJS, Godot and Unity. |
| **SVG** | Yes | Vectors, a file per frame, for the web, print and drawing programs. |
| **Lottie** (.json) | Yes | A vector animation that plays on web pages and in iOS and Android apps. |
| **Web Page** (.html) | Yes | One page that plays the animation in any browser. More on this shortly. |

A sprite sheet crops each frame to what's drawn, and packs frames that are the same only once, so a game loads as little as possible. Game engines can't load images bigger than 8192 × 8192 pixels, so when the frames don't all fit on one, Inkframe shares them out over as few sheets as they need: `walk-0.png` with `walk-0.json`, `walk-1.png` with `walk-1.json`, and so on. Each data file names the others, so engines that read TexturePacker's "multipack" sheets, such as PixiJS, load them together.

![The Export window set to a sprite sheet, with transparency](export-sprite){crop=0,0,0.4,0.36}

> Tip: **File › Export PSD…** saves the current frame as a layered Photoshop file, for painting over or printing.

## Interactive web pages

The **Web Page** format writes a single, self-contained HTML file that plays the animation as vectors in any browser, much as Animate's HTML5 Canvas export does. It fits the browser window, loops, pauses on a click and steps a frame at a time with the arrow keys while paused.

It can also be *interactive*. Any layer can be a button: in its properties, set **Web button** to what clicking it does: **Play**, **Stop**, **Go to & play** a frame, **Go to & stop** at one, or **Open link**. Buttons light up under the pointer. And **Timeline › Frame Action** gives a frame an action, such as **Stop** or **Go to & play**, as Animate's `stop()` and `gotoAndPlay()` do; frames with an action are marked "a" on the ruler.

Here's how to make the ball bounce once, stop, and offer a Replay button:

1. Go to the last frame, open **Timeline › Frame Action**, and set the frame's action to **Stop**.
2. Draw a button shape on a new layer, and name the layer *Replay*.
3. In its properties, set **Web button** to **Go to & play**, and the frame to **1**.
4. Export as **Web Page**, and open the file in a browser.

![A ball that stops on its last frame, with Replay and Pause buttons: Replay goes to frame 1 and plays](buttons)

## Batch export

**File › Batch Export…** queues several projects, or one project in several formats, and exports them one after another into one folder, each named after its project. Click **Add Projects…** or **Add This Project**, set each job's format and size, choose where to **Save to**, and start.

Each finished job is ticked, with a link that shows its file in Finder or Explorer; in {{fig:batch}}, the bouncing ball's sprite sheets are done, and the next job is under way. A job that fails is marked (hover over it to see why), and the rest carry on. The queue and folder are remembered for next time.

![Batch Export working through six jobs: the sprite sheets are done, and the PNG sequence is under way](batch){crop=0,0,0.62,0.46}

> Tip: Inkframe can also export from the command line, without opening a window, for render machines and scripts: `inkframe --export walk.inkf --format h264 --to renders`. Run `inkframe --help` for everything it can do.

## Summary

In this chapter, we exported animations from the Export window in the right format for each destination: video, image sequences, sprite sheets, SVG, Lottie and web pages. We made a web page interactive with buttons and frame actions, and exported many projects at once with Batch Export.

That's the end of the tour of Inkframe's tools. The next three chapters are projects that put them together, start to finish: a walk cycle, a talking head and an interactive web banner.
