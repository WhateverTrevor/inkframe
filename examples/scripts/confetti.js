// Confetti: scatters pieces of colour that tumble down the stage,
// each with its own colour, size, timing and spin: animation generated
// from numbers rather than drawn. Run it again for a different shower.

const PIECES = 40;
const COLOURS = ["#e8543c", "#f2b134", "#4cb963", "#3c8dde", "#a56cc1"];

const doc = inkframe.document;
const random = (low, high) => low + Math.random() * (high - low);

for (let i = 0; i < PIECES; i++) {
  const x = random(0, doc.width);
  const size = random(14, 30);
  const piece = doc.addLayer(`Confetti ${i + 1}`, {
    rect: [size, size * 0.6],
    fill: COLOURS[i % COLOURS.length],
    at: [x, -40],
  });
  const start = Math.floor(random(1, doc.duration / 2));
  const end = Math.min(doc.duration, start + Math.floor(random(30, 60)));
  piece.setKey(start, { position: [x, -40], rotation: random(0, 360) });
  piece.setKey(end, {
    position: [x + random(-200, 200), doc.height + 40],
    rotation: random(-720, 720),
  });
}
console.log(`Added ${PIECES} pieces of confetti.`);
