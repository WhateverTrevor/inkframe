# Project: A Talking Head

Dialogue brings a character to life, and keeping a mouth in time with a voice used to be some of the slowest work in animation. In this project, you'll give a character a set of mouth drawings, import a line of dialogue, let Inkframe time the mouth to it, check the timing on the exposure sheet, and add a nod of the head. It builds on *Chapter 5* and *Chapter 7*.

In this chapter, we're going to cover the following main topics:

- Drawing the mouth shapes
- Bringing in the dialogue
- Timing the mouth to the voice
- Checking and adjusting the timing
- Acting with the head

## Technical requirements

Open the *Rig* example (**File › Open Example…**): its character has a mouth layer ready to work on. You'll also need a short line of recorded dialogue, a few seconds long, as a WAV, MP3 or other sound file. Record one on your phone, or use any you have.

## Drawing the mouth shapes

A mouth needs only a handful of shapes to speak convincingly. Inkframe uses the same letters as Rhubarb Lip Sync, the tool that listens to the dialogue:

| Shape | Sounds |
|---|---|
| **A** | Closed: M, B, P |
| **B** | Slightly open: most consonants |
| **C** | Open: EH, AE |
| **D** | Wide open: AA |
| **E** | Slightly rounded: AO, ER |
| **F** | Puckered: OO, W |
| **X** | At rest, in silences |

Select the *Mouth* layer. Its drawings are listed in the **Drawings** section of Properties. Click **Duplicate** to copy the drawing showing, reshape the copy (double-click the layer to step inside it, then use the Subselection tool), and click its name to rename it with its letter. Four or five shapes are enough to start: the shapes you leave out borrow a similar one's.

![The Mouth layer's drawings in Properties, and the timeline marking each swap by name](mouth-drawings)

## Bringing in the dialogue

Choose **File › Import Audio…** and pick your dialogue. It appears in the timeline as a waveform, starting at the playhead; drag it to start where the line should begin. Play it with [[Enter]] to hear it with the picture, or scrub along the ruler to hear it a frame at a time.

## Timing the mouth to the voice

1. Select the *Mouth* layer and choose **Modify › Lip Sync…**.
2. Pick your dialogue under **Dialogue**.
3. Under **Shapes from**, choose **Rhubarb**: it listens for the sounds of speech. If your line isn't in English, choose **Other**, which listens for sounds rather than words. Typing the words that are said helps it with unclear ones.
4. Check the drawing chosen for each shape. Inkframe guesses from the drawings' names, and a shape in brackets is borrowing another's.
5. Click **Apply**. After a few seconds, the mouth's drawings are swapped in time with the voice, under the sound.

![The Lip Sync window, with each mouth shape matched to a drawing](lipsync){crop=0,0,0.45,0.75}

## Checking and adjusting the timing

Play it back. It's usually close, but the eye is fussy about mouths, and a few changes will help:

- Open **Timeline › Exposure Sheet** to see the swaps frame by frame, beside the dialogue's loudness.
- Double-click a cell, type a drawing's name and press [[Enter]] to change what shows there.
- Mouths read better a frame *early*: if the timing feels late, select the *Mouth* layer, put the playhead on its first frame and press [[⇧F5]] (**Remove Frames**), which brings everything after it a frame earlier.

![The Rig's mouth on the exposure sheet: each swap marked by name, and held until the next](xsheet){crop=0,0,0.4,0.65}

## Acting with the head

A mouth moving in a still face looks like a puppet. Small head movements on the stressed words make it act:

1. Pick the **Bone** tool ([[K]]) and find the frames where the voice is loudest: the green bars in the exposure sheet are the dialogue's loudness.
2. A frame or two before each, drag the head to tilt it a little; between them, let it settle back.
3. Give the head's rotation **Ease In-Out**, so the moves are soft.

Because the mouth is attached to the head (its **Parent**), it moves with it.

## Summary

In this project, we gave a mouth a drawing for each shape, imported dialogue, and let Rhubarb time the mouth to it. We checked and adjusted the timing on the exposure sheet, and made the character act with a few movements of the head.

In the last project, we'll make something for the web.
