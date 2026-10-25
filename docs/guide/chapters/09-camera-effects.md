# The Camera, Depth and Effects

So far, the stage has been a fixed window onto the scene. A camera lets you move that window: pan across a landscape, push in on a face, turn for a dizzy moment. Give layers depth, and a camera move becomes a multiplane shot, with near things sliding past faster than far ones. This chapter covers the camera, depth, and the effects that finish a shot: filters, colour effects, motion blur and particles.

In this chapter, we're going to cover the following main topics:

- The camera
- Depth and the multiplane camera
- Safe areas and the field guide
- Filters and colour effects
- Motion blur
- Particles
- The Node View

## Technical requirements

Open the *Bouncing ball* example (**File › Open Example…**). The *Multiplane* and *Particles* examples are useful to explore alongside this chapter.

## The camera

Press [[C]] for the **Camera** tool and drag on the stage. The first drag adds a camera, and dragging pans it; the scene follows the pointer. [[⇧]]-drag turns the camera, and [[⌥]]-drag zooms. **Timeline › Add Camera** adds one too.

The camera is a layer at the top of the timeline, so it animates like any layer: go to another frame and pan again, and the camera moves between the two, with easing. Select it to type where it **Looks at**, its **Zoom** and **Rotation**, or **Reset** it. Hide the camera layer to turn it off.

![A camera zoomed in to 140% and turned -8°: the orange frame is what it sees](camera)

## Depth and the multiplane camera

Give a layer a **Depth** in its properties, and it sits further from the camera (or nearer, with a negative depth). Deeper layers look smaller, and move less as the camera pans. This is *parallax*: the effect Disney's multiplane camera gave its films, with near trees sliding past and far mountains barely moving.

The camera's **Dolly** moves it in among the layers: near ones grow faster than far ones, as with a real camera moving forward (zooming can't do that), and layers it passes fade away.

The **Depth View** (in any area's menu, under Animation) shows the scene from above, each layer a line at its depth, with the camera and what it sees. Drag a layer up or down to change its depth, or drag the camera to dolly and pan.

![A scene at several depths, with the Depth View showing it from above as the camera dollies in](multiplane)

> Tip: Depth animates like any property, so a layer can drift nearer or further. Tick **Stack by depth** and depth also decides what's in front, so a layer drifting back passes behind the others.

## Safe areas and the field guide

**View › Safe Areas** shows the *action-safe* and *title-safe* frames over the camera's view, the margins that televisions and phones may crop. **View › Field Guide** lays a traditional 12-field chart over it, for planning camera moves in fields as animation studios do. Neither shows in exports.

![The safe areas and field guide over the camera's view](guides){crop=0.095,0.14,0.664,0.558}

## Filters and colour effects

Every layer can have effects, in its properties:

- **Colour Effect** sets the layer's **Brightness**, a **Tint** (a colour and how much of it), or **Advanced** (each colour channel multiplied and offset).
- **Filters › Add Filter** adds a **Drop Shadow**, **Blur**, **Glow** (outer or inner), **Adjust Color** (brightness, contrast, saturation and hue) or **Motion Blur**.

Filters apply in order (↑ moves one earlier) and can be switched off with their checkbox. Effects animate like the transform: change them on another frame and they tween. Layers with effects are labelled *fx* in the timeline.

![The Rig with a drop shadow on the body and a tint on the tail](effects){crop=0.26,0.11,0.32,0.46}

## Motion blur

Fast movement looks smoother with *motion blur*, the smear a film camera's shutter leaves on anything moving while it's open. Add a **Motion Blur** filter to a moving layer, and it's smeared along its path. **Shutter** sets how long the shutter is open: 180° is the usual film look. Fast movement automatically gets more steps, so the smear stays smooth.

![A ball flying across the stage, smeared by motion blur](motion-blur){crop=0.12,0.4,0.6,0.22}

## Particles

**Modify › New Particle Layer** adds **Sparks**, **Snow**, **Smoke** or **Fire**. A particle layer sprays particles from its *emitter*: a point, a line, a rectangle or a circle. Its properties set the rate, direction and spread, speed, gravity, wind, drag and wander, and how each particle's size, opacity and colour change over its life.

Particles are soft dots, or copies of whatever is drawn in the layer: draw a leaf, and the layer sprays leaves. Move or animate the layer and the emitter leaves a trail. Each frame is worked out on its own, so scrubbing, playing backwards and exporting all match. **Seed › New** gives another spray just like it.

![The four particle presets: sparks, fire, smoke and falling snow](particles){crop=0.095,0.14,0.664,0.558}

## The Node View

For those who like to see how a scene is wired together, the **Node View** (in any area's menu, under General) shows the layers as a graph, as in Harmony. Each layer feeds its parent's children, its masks and its effects, and everything flows into the **Composite** and on to the **Display**. Drag from an output to an input to rewire it: attaching a layer, masking one with another, or restacking. It's the same scene as the timeline shows, so the two always agree.

![The Rig in the Node View: each part feeds the Composite, and the Composite the Display](nodes){crop=0.38,0.78,0.24,0.2}

## Summary

In this chapter, we added a camera and animated it, gave layers depth for multiplane parallax and dollied among them, and lined shots up with safe areas and the field guide. We added filters, colour effects and motion blur, sprayed particles, and saw the whole scene wired up in the Node View.

In the next chapter, we'll get the finished animation out of Inkframe.
