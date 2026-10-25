# Your First Animation

The bouncing ball is every animator's first exercise, and for good reason: in a few seconds of movement it teaches timing, spacing, weight and squash and stretch. In this chapter, you'll animate one from scratch, and on the way meet the keyframes, easing and motion tools you'll use in every animation after it.

In this chapter, we're going to cover the following main topics:

- Keyframes and tweens
- Giving movement weight with easing
- Squash and stretch
- Seeing movement: motion paths and onion skins
- The value graph
- Motion presets

## Technical requirements

Start a new project with [[⌘N]]. You can compare your work with the *Bouncing ball* example at any point (**File › Open Example…**).

## Keyframes and tweens

Animation in Inkframe works as it does in Animate and After Effects. A *keyframe* (or *key*) records a layer's position, rotation, scale or opacity on a frame. Between two keys, Inkframe works out every frame in between, a *tween*, so the layer moves smoothly from one to the other.

You rarely need to make keys by hand: Inkframe keys automatically. Go to a frame, change a layer, and a key appears there, for just the properties you changed.

Let's draw a ball and a floor, and make the ball fall:

1. Press [[O]] for the **Oval** tool, pick an orange **Fill**, and hold [[⇧]] while dragging near the top of the stage to draw a circle. A layer called *Oval 1* appears in the timeline. Rename it *Ball* in Properties.
2. Click **+** in the timeline bar ([[⇧⌘N]]) to add an empty layer, and rename it *Floor*.
3. Press [[R]] for the **Rectangle** tool, pick a darker colour, and draw a wide, flat floor across the bottom of the stage.
4. Press [[V]] for the **Selection** tool and click the ball.
5. Click frame **12** in the timeline's ruler.
6. Drag the ball down until it sits on the floor. A key appears on frame 12, and frames 1 to 12 show the ball falling.
7. Go to frame **24** and drag the ball back up to where it started.

Press [[Enter]] to play. The ball falls and rises again.

> Tip: To key a property without changing it, click the small diamond beside it in Properties. [[F6]] keys every property of the selected layers at once, and [[⇧F6]] clears the keys on the current frame.

### Reading the timeline

Each layer's row shows a bar for the frames it appears on, with a dot for each frame that has keys. Click the triangle beside a layer's name to *expand* it: a row appears for each property, each with its own keys, as in {{fig:bounce-expand}}. Drag a key along its row to retime it, and drag either end of a layer's bar to make the layer appear or disappear partway through.

![The ball's layer expanded, showing its Position and Scale keys on their own rows](bounce-expand){crop=0,0.72,0.7,0.28}

## Giving movement weight with easing

Play the animation again and look closely: the ball moves at the same speed all the way, as if it were sliding along a wire. A real ball speeds up as it falls and slows as it rises. That change of speed is *easing*.

1. Click any frame between 1 and 12 on the ball's row.
2. In Properties, under **Easing**, set **Position** to **Ease In**, so the fall gathers speed.
3. Click a frame between 12 and 24, and set Position to **Ease Out**, so the rise slows to a stop at the top.

Play it again, and the ball has weight. Each property is eased separately, so a layer can ease its movement while fading at a steady rate. The choices are **Linear**, **Ease In**, **Ease Out**, **Ease In-Out**, **Hold** (no tween: the value jumps at the next key) and a custom curve. The curve under the easing menu shows the timing, and you can drag its orange handles to shape it yourself: pull a handle above the top line to overshoot, or below the bottom to anticipate.

## Squash and stretch

A rubber ball squashes as it hits the ground. Exaggerating that makes the bounce feel springy:

1. Go to frame 12, where the ball hits the floor.
2. In Properties, set **Scale** to x **120%**, y **80%**: wider and flatter.
3. Go to frame 10 and click the diamond beside **Scale** to key it there, unsquashed, so the squash only happens at the moment of impact. Do the same on frame 14.

For extra life, stretch the ball a little as it falls fast, just before impact, with a scale of x **90%**, y **110%** on frame 10.

## Seeing movement: motion paths and onion skins

Select a moving layer, and its *motion path* appears on the stage as a dashed line, with a dot for each frame. The dots' spacing shows the timing: close together where the ball is slow, far apart where it's fast. Drag the path to curve it, and drag its round handles to fine-tune the curve. **Orient to path** (in Properties, under the transform) turns the layer to face the way it's going.

![The ball's motion path: the dots are close together at the top, where it's slow](bounce-path)

*Onion skinning* shows faint copies of the frames before and after the current one, so you can see the arc of the movement while you work on a single frame. Turn it on with its button in the context bar or the timeline bar; the frames before are tinted blue and those after green. The arrows beside the button set how many frames either side are shown.

![Onion skins show the frames before (blue) and after (green) the impact on frame 12](bounce-onion)

> Tip: For any shape of path (a loop, a winding road), draw it as a line, tick **Guide** in its layer's properties, and pick that layer under **Follow guide** on the layer that should travel along it. Guides never appear in exports.

## The value graph

Click the graph button in the timeline bar to swap the timeline for the **Graph Editor**. It shows the selected layer's animation as values over time: pick **Position**, **Rotation**, **Scale** or **Opacity** on the left. In {{fig:graph}}, the ball's height rises and falls in arcs, sharp at each bounce and round at the top, which is exactly what good bouncing-ball easing looks like.

![The Graph Editor showing the ball's position over time](graph){crop=0,0.7,0.7,0.3}

Drag a key's dot up or down to change its value and sideways to retime it, and drag the round handles to reshape the easing. Higher on the graph means a bigger number, so a ball falling down the stage draws as a rising curve, as in After Effects.

## Motion presets

Some movements come up again and again: a fade in, a pop, a slide onto the stage. **Motion Presets** (in any area's menu, under Animation) has them ready-made: **Fade In**, **Fade Out**, **Pop In**, **Slide In** from either side, **Bounce In**, **Spin**, **Pulse**, **Shake** and **Float**, each shown with a little square doing it.

Select a layer, pick a movement and click **Apply**. The movement starts at the playhead, from wherever the layer is, so a Slide In ends exactly where the layer already sits.

**Save Movement** keeps the selected layer's own animation as a preset, available in every project. Save your bouncing ball, and you can bounce anything.

![Motion Presets, with the ball's movement saved under Yours](presets)

## Summary

In this chapter, we animated a bouncing ball with keyframes and tweens, gave it weight with easing and life with squash and stretch, and checked its movement with motion paths, onion skins and the value graph. We also met motion presets, for movements you'll use again and again.

In the next chapter, we'll animate the traditional way: frame by frame.
