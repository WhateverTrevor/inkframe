# Rigging a Character

Drawing every frame of a character by hand is beautiful but slow. A *rig* turns a character into a puppet you can pose: move the hand, and the elbow and shoulder follow. Inkframe has two kinds of rig. The first, *parenting*, joins separate parts at their pivots, as cut-out puppets are joined with pins. The second, *skeletons*, works as Moho's bones do: bones run through whole drawings and bend them smoothly, painted ones included. This chapter builds both.

In this chapter, we're going to cover the following main topics:

- Parenting parts into a rig
- Posing with the Bone tool
- Skeletons, Moho-style
- Binding layers, and pins
- Rig setup
- The Bones panel
- Target bones

## Technical requirements

Open the *Rig* example (**File › Open Example…**) for the first two sections. For skeletons, start a new project with [[⌘N]] and draw a simple character: a body, a head and an arm, each on its own layer.

## Parenting parts into a rig

In a parented rig, each part is a layer, *attached* to the part it hangs from: the hand to the forearm, the forearm to the upper arm, the arm to the torso. A part moves, turns and scales with its parent, and turns around its own pivot: an elbow, a shoulder.

To attach a layer, select it and pick its **Parent** in Properties. The layer stays where it is and from then on follows its parent; the timeline indents it under the parent's name. Then move its pivot to the joint: [[⌥]]-drag the ring in the middle of the selected layer onto the elbow or shoulder. The pivot snaps to the layer's corners, edges and centre.

The *Rig* example is built this way: a torso, with a head and an arm attached, and a forearm and hand below the upper arm.

## Posing with the Bone tool

Press [[K]] for the **Bone** tool. The rig is drawn as bones between the parts' pivots, as in {{fig:rig-bone}}. Drag a part, and the joints above it turn so the spot you grabbed follows the pointer: drag the hand, and the elbow and shoulder bend to reach. This is *inverse kinematics* (IK). The top of the rig, the *root*, drawn as a square, stays put; drag it to move the whole character.

Every pose is keyed on the current frame, so posing a character on frame 1, then on frame 12, animates it between the two.

![The Rig example with the Bone tool: bones run between the parts' pivots, and the forearm only bends one way](rig-bone)

To stop a joint bending the wrong way, tick **Bends** in the part's properties and set the lowest and highest rotation. An elbow, for example, might only bend from -150° to 0°.

> Tip: To bend a single drawing, such as a tail or a scarf, without cutting it into parts, select it and click **Add Bend Deformer** in Properties. Bones are laid along the drawing, and dragging their green joints with the Bone tool bends it there.

## Skeletons, Moho-style

A skeleton runs bones *through* drawings, rather than joining separate parts. Each layer in the skeleton's folder bends with the bones near it, smoothly, as skin bends over bones. That works for drawn shapes and also for *painted layers and pictures*, which bend through a fine mesh. You can draw a character as one continuous painting, and still pose it.

To build one:

1. Select the character's layers and choose **Modify › New Skeleton**. They go into a folder with a skeleton, and the **Add Bone** tool ([[⇧K]]) is picked.
2. Drag from the hips up to the neck to draw the first bone, the *root* of the skeleton.
3. Drag from there to draw the next bone; it hangs from the last. Starting on a bone's end joins the new bone to it, so a whole arm is a few drags from the shoulder out.
4. Click empty stage to start a new root, for a separate chain.
5. Press [[K]] for the **Bone** tool, and pose: drag a bone and the bones above it turn so its end follows. Hold [[⌥]] to turn just that one bone. Drag a root's square start to move the whole character.

As with parented rigs, every pose is keyed, here on the skeleton's folder.

![A skeleton through a body, a head and a painted arm, posed with the Bone tool](skeleton){crop=0.3,0.05,0.7,0.68}

Pick a bone and its settings appear in the folder's properties. **Strength** sets how far a bone's influence reaches into the drawings around it; **Turn** is its angle on this frame.

## Binding layers, and pins

By default, each layer bends with whichever bones are near it. Some parts should follow just one bone instead: a hand, a hat, a head. In a layer's properties, **Follows** binds it to one bone (or to none).

*Pins* do the same for parts of a drawing. Double-click a layer and pick some shapes, or pick a single point with the Subselection tool, and in the layer's properties choose a bone under **Pin to**. The pinned shapes follow that bone alone, so an eye stays on the head while the face around it bends. Welded points (*Chapter 2*) pin together, so joined shapes don't crack apart as the skeleton bends them.

## Rig setup

**View › Rig Setup** ([[⌥⌘B]]), or its button at the right of the context bar, shows every skeleton *at rest*, unposed, as Moho's frame 0 does. This is where you build and fix the rig: draw, paint and pin there, and move bones without posing them. The Add Bone tool always works at rest.

![Rig setup: the skeleton at rest, with the Add Bone tool](setup){crop=0.3,0.05,0.7,0.68}

With rig setup off, everything is shown posed, and drawing on a posed character lands where you see it on the bent drawing, so you can touch up a pose as it is.

## The Bones panel

The **Bones** editor (in any area's menu, under Animation) lists every skeleton: its bones as a tree, each with its turn on this frame and marks for anything special about it (smart, swinging, limited or stretching), and the layers it rigs, with what each follows. Click a bone to pick it. Rig setup can be turned on and off here too.

![The Bones panel: the skeleton's bones as a tree, and the layers it rigs](bones-panel)

## Target bones

Walking characters need their feet to stay planted while their hips move. *Target bones* do this, as Moho's do. Pick a bone (a shin, say) and choose a **Target** in its properties: another bone, such as a small bone under the foot. The shin's end now reaches for the target's start, and the bones above it bend to get it there.

Plant both feet on target bones, then lower the hips: the legs crouch, and the feet stay where they are. Move a target to take a step. Give the shins **Limits** so the knees bend the right way.

![Two legs crouching as the hips drop, the feet planted on target bones](targets){crop=0.3,0.05,0.7,0.68}

## Summary

In this chapter, we rigged characters two ways: by parenting parts at their pivots, and with Moho-style skeletons that bend whole drawings, painted ones included. We posed both with the Bone tool, bound layers and pinned shapes to bones, built rigs at rest in rig setup, kept track of them in the Bones panel, and planted feet with target bones.

In the next chapter, we'll make these rigs smarter.
