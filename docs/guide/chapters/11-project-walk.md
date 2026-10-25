# Project: A Walk Cycle

A walk is the exercise that comes after the bouncing ball: the same timing and weight, now on two legs. In this project, you'll draw a simple character from the side, rig its legs with a Moho-style skeleton whose feet stay planted on target bones, pose a 24-frame walk on the spot, and turn it into a symbol that loops as the character crosses the stage. It brings together *Chapter 4*, *Chapter 6* and *Chapter 7*.

In this chapter, we're going to cover the following main topics:

- Drawing the character
- Building the skeleton
- Binding each leg to its limb
- Posing the walk
- Looping it across the stage

## Technical requirements

Start a new project with [[⌘N]]. In the **Document** tab of Properties, set **Duration** to **24** frames: one step with each foot, a second at 24 frames a second.

## Drawing the character

Draw the character side on, facing right, each part on a layer of its own:

1. Draw a rounded rectangle for the body and a circle for the head above it, on a layer called *Body*.
2. Add a layer (**+** in the timeline, or [[⇧⌘N]]) called *Near leg*, and draw the leg nearest us as one tall, rounded rectangle from the hip to the ground.
3. Add another, *Far leg*, and draw the other leg right over the first, in a darker colour, as legs are seen from the side.
4. Restack the layers so the far leg is at the bottom, the body above it, and the near leg on top.

![The character drawn: a body and head, and two legs drawn over each other](walk-drawn){crop=0.3,0.04,0.7,0.70}

## Building the skeleton

1. Select all three layers and choose **Modify › New Skeleton**. They go into a folder with a skeleton, and the **Add Bone** tool ([[⇧K]]) is picked. Rename the folder *Walker*.
2. Drag a short bone down from the bottom of the body to the hip: this is the *hips*, the root of the skeleton.
3. Starting on the end of the hips bone, drag down to the knee for the near thigh, then on to the ankle for its shin.
4. Start again on the end of the hips bone and drag the far thigh and shin, right over the first.
5. Click empty stage beside the ankle, and drag a short bone forward along the ground: the near foot, a root of its own. Do the same for the far foot.
6. Pick each bone and name it in Properties: *Hips*, *Near thigh*, *Near shin*, *Far thigh* and so on.

Now make the feet steer the legs, as *Chapter 7* showed. Pick each shin and set its **Target** to its foot: the shin's end reaches for the foot, and the thigh bends to get it there. Tick **Limits** on each shin and set **0°** to **150°**, so the knees only bend forward.

![The skeleton at rest, in rig setup: hips, two legs over each other, and the feet](walk-setup){crop=0.3,0.04,0.7,0.70}

> Tip: **View › Rig Setup** ([[⌥⌘B]]) shows the skeleton at rest while you build it. Turn it off again before posing.

## Binding each leg to its limb

By default, each layer bends with whichever bones are near it. Here that's a problem: the two legs are drawn over each other, so each would follow both legs' bones, and bend halfway between them.

Instead, select the *Near leg* layer, and in the **Bones** section of its properties set **Follows** to **Near thigh and below**. Now it bends with the near thigh and the bones hanging from it, the near shin, and ignores the far leg. Set *Far leg* to **Far thigh and below**, and *Body* to **Hips**, so the body rides on the hips without bending.

![The near leg follows its own limb: the near thigh and the bones below it](walk-follows){crop=0.3,0.04,0.7,0.70}

## Posing the walk

A walk on the spot has two kinds of pose, as animators have drawn them for a century:

- **Contact**: both feet on the ground, one in front and one behind, the stride at its widest.
- **Passing**: one foot on the ground under the body, the other lifted and swinging past it.

Pick the **Bone** tool ([[K]]) and pose the feet by dragging their square starts, which moves those roots:

1. On frame **1**, drag the near foot forward and the far foot back: the first contact.
2. On frame **7**, put the near foot under the hips, and lift the far foot a little, under the hips too: passing.
3. On frame **13**, swap them: far foot forward, near foot back. This is the second contact.
4. On frame **19**, near foot lifted under the hips, far foot under them on the ground.
5. On frame **25**, pose it as frame 1, so the cycle ends where it began.

The legs bend by themselves to reach the feet. To give the walk weight, drag the hips' square start down a little on each contact, lower still two or three frames after it, and up on each passing pose: the body bobs as it walks.

![The first contact: the near foot forward, the far foot back](walk-contact){crop=0.3,0.04,0.7,0.70}

Play it with [[Enter]], and turn on onion skins to check the spacing. A planted foot should slide back at an even speed, as if on a treadmill: if it lurches, move the key in the middle of its slide.

![Onion skins show the near foot sliding back evenly while the far foot swings through](walk-onion){crop=0.3,0.04,0.7,0.70}

## Looping it across the stage

A walk on the spot is most useful as a symbol that loops:

1. Select the *Walker* folder and press [[F8]] to make it a symbol.
2. In the instance's properties, set **Symbol length** to **24** and playback to **Loop**. Frame 25 matched frame 1, so frames 1 to 24 now repeat seamlessly.
3. In the **Document** tab, set **Duration** to **96** frames.
4. On frame 1, move the instance off the left of the stage; on frame 96, move it off the right.

Play it. If the feet seem to skate, match the move to the stride: each planted foot slides back a stride in 12 frames, so over 96 frames the instance should travel eight strides.

## Summary

In this project, we drew a character side on, built a skeleton with target bones for feet, and bound each leg to its own limb so legs drawn over each other bend separately. We posed contact and passing positions, gave the walk weight with the hips, checked the spacing with onion skins, and looped it across the stage as a symbol.

In the next project, we'll make a character talk.
