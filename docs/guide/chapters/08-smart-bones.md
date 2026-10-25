# Smart Bones, Corrections and Dynamics

A plain skeleton bends drawings, but real bodies do more than bend. A fist closes as the wrist turns, a bicep bulges as the elbow bends, and a ponytail swings after the head stops. Moho calls bones that do such things *smart bones*. This chapter adds them to the skeleton from *Chapter 7*, along with the limits, stretch and dynamics that make a rig feel alive.

In this chapter, we're going to cover the following main topics:

- Switches
- Smart bones: reshape and switch actions
- Corrections: drawing how a pose should look
- Limits and stretch
- Dynamics

## Technical requirements

You'll need a character rigged with a skeleton, as in *Chapter 7*.

## Switches

A *switch* is a folder that shows just one of its layers at a time: a hand's poses, or a mouth's shapes. Make one with **Modify › New Switch**, or tick **Switch** on an existing folder. Pick which layer shows under **Showing** in its properties; that's keyed per frame like anything else.

## Smart bones: reshape and switch actions

A smart bone changes the drawing as it turns, rather than just bending it. Pick a bone (with the Bone or Add Bone tool) and add one of these *actions* in the **Smart bone** section of its properties:

- **+ Reshape** morphs a layer toward another of its drawings as the bone turns, reaching the other drawing fully at the angle you set. Use it for a cleaner elbow, or a smile on a dial bone.
- **+ Switch** makes a switch show another layer once the bone has turned past an angle: an open hand becomes a fist as the wrist bends.

Pose the bone first, and the new action takes its current angle. In {{fig:smart}}, the forearm is a smart bone: past -50°, the hand switch shows the fist.

![The forearm as a smart bone: past -50°, its switch action shows the fist](smart){crop=0.3,0.05,0.7,0.68}

## Corrections: drawing how a pose should look

Sometimes bending isn't enough, and only redrawing will do: a shoulder that pinches, a body that should bulge as it leans. A *correction* lets you draw the fix by hand, at the pose that needs it:

1. Pick the bone and turn it to the pose that needs fixing.
2. Click **+ Correction** in its properties. The layer's drawing is now yours to reshape with any tool.
3. Reshape it as it should look at that turn.
4. Click **Finish** in the context bar (or press [[Enter]]) to key it there. [[Esc]] cancels.

Between rest and that turn, the drawing morphs smoothly into your correction. Key as many turns as you like, either way round; each shows as a chip in the bone's properties, with **Edit at …°** to change it and × to take it out.

![A correction on the spine: the body reshaped by hand into a pear as it leans](correction){crop=0.3,0.05,0.7,0.68}

> Important note: Saving or exporting while a correction is under way keys it first, so you won't lose your work.

## Limits and stretch

**Limits**, in a picked bone's properties, keep the bone within a range of turns. When IK reaches a limit, the bone stops and the bones above it make up the rest, so knees and elbows bend only one way. The Bone tool shows the range as a wedge around the joint.

**Stretch** makes a bone longer or shorter, keyed like a turn; or hold [[⇧]] and drag the bone with the Bone tool. Tick **Keep area** and the drawing gets thinner as it stretches and fatter as it squashes, the classic squash and stretch of *Chapter 4*, now on a rig.

## Dynamics

Hair, tails, antennae and floppy ears should keep moving after the body stops. Tick **Dynamics** on a bone, and it swings after the bones it hangs from: lagging behind as they move, overshooting and settling when they stop. Three settings shape the swing:

- **Spring**: how strongly it pulls back toward its pose.
- **Damping**: how quickly the swinging dies away.
- **Swing**: how far it may swing.

Dynamics are worked out from the animation itself, so there's nothing to key: animate the head turning, and the ponytail follows.

![A quick turn: the ponytail trails behind, the body squashes keeping its area, and the upper arm's limits show as a wedge](dynamics){crop=0.3,0.05,0.7,0.68}

## Summary

In this chapter, we made switches that show one layer at a time, and smart bones that reshape and switch drawings as they turn. We drew corrections by hand for the poses that needed them, kept bones within limits, stretched them while keeping their area, and let hair swing with dynamics.

In the next chapter, we'll step back from characters, to the camera and the scene around them.
