// Stagger selected: delays each selected layer's animation a little more
// than the one above it, for overlapping action (a wave, a crowd, limbs
// that follow through). Select the layers in the timeline first.

const FRAMES_APART = 2;

const layers = inkframe.document.selectedLayers;
if (layers.length < 2) {
  throw new Error("select two or more layers to stagger");
}
layers.forEach((layer, i) => layer.shiftKeys(i * FRAMES_APART));
console.log(`Staggered ${layers.length} layers, ${FRAMES_APART} frames apart.`);
