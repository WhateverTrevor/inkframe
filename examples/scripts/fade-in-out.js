// Fade in and out: the selected layers fade in over their first frames
// and out over their last, a quick way to bring things on and off.
// With nothing selected, it fades every layer.

const FADE_FRAMES = 6;

const doc = inkframe.document;
const chosen = doc.selectedLayers.length > 0 ? doc.selectedLayers : doc.layers;
const layers = chosen.filter((l) => l.kind !== "camera" && l.kind !== "folder");
for (const layer of layers) {
  const first = layer.start;
  const last = layer.end ?? doc.duration;
  if (last - first < FADE_FRAMES * 2) continue; // too short to fade both ways
  layer.setKey(first, { opacity: 0 }, { easing: "ease-out" });
  layer.setKey(first + FADE_FRAMES, { opacity: 1 });
  layer.setKey(last - FADE_FRAMES, { opacity: 1 }, { easing: "ease-in" });
  layer.setKey(last, { opacity: 0 });
}
console.log(`Faded ${layers.length} layers in and out over ${FADE_FRAMES} frames.`);
