// Snowy night: turns any scene wintry. It adds falling snow, tints every
// layer a cool blue, and has the camera end the shot a little further in,
// on top of any move it already makes (adding a camera if there isn't one). Uses particles, effects and
// the camera from the scripting API.

const TINT = { type: "tint", color: "#3050a0ff", amount: 0.25 };
const PUSH_IN = 120; // how far the camera moves in, in stage pixels

const doc = inkframe.document;
let tinted = 0;
for (const layer of doc.layers) {
  if (layer.kind === "camera" || layer.kind === "folder") continue;
  const effects = layer.effectsAt(layer.start);
  layer.setEffects(layer.start, { ...effects, color: TINT });
  tinted += 1;
}

const snow = doc.addParticleLayer("snow", { at: [doc.width / 2, -40] });
snow.particles = { rate: 60 };

// On top of any move the camera already makes: wherever it ends up, it
// ends a little further in.
const camera = doc.camera ?? doc.addCamera();
const end = camera.valueAt("depth", doc.duration);
camera.setKey(doc.duration, { depth: end + PUSH_IN });

console.log(`Tinted ${tinted} layers, added snow, and pushed the camera in.`);
