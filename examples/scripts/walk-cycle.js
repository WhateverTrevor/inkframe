// Walk cycle: draws a character from the side and rigs it Moho-style, as
// the guidebook's walk project does by hand: a skeleton through its legs,
// feet on target bones that keep them planted, each leg following its own
// limb, and 24 frames of stepping on the spot with the hips bobbing.

const doc = inkframe.document;
doc.duration = Math.max(doc.duration, 24);
const x = doc.width / 2;
const ground = doc.height - 200;
const hip = ground - 340;

// Drawn far to near: the far leg, the body, then the near leg.
const far = doc.addLayer("Far leg", { rect: [56, 360], radius: 28, fill: "#4673be", at: [x + 2, hip + 170] });
const body = doc.addLayer("Body", { rect: [130, 210], radius: 50, fill: "#f0a046", at: [x, hip - 50] });
const near = doc.addLayer("Near leg", { rect: [56, 360], radius: 28, fill: "#5a96e6", at: [x - 2, hip + 170] });

const walker = doc.addSkeleton([near, body, far], "Walker");
walker.addBone("Hips", [x, hip - 50], [x, hip]);
for (const [side, dx] of [["Near", -2], ["Far", 2]]) {
  walker
    .addBone(`${side} thigh`, [x + dx, hip], [x + dx, hip + 170], { parent: "Hips" })
    .addBone(`${side} shin`, [x + dx, hip + 170], [x + dx, ground], { parent: `${side} thigh` })
    .addBone(`${side} foot`, [x + dx, ground], [x + dx + 50, ground])
    .setBone(`${side} shin`, { target: `${side} foot`, limits: [0, 150] });
}
near.follows = { limb: "Near thigh" };
far.follows = { limb: "Far thigh" };
body.follows = { bone: "Hips" };

// One foot's step through the cycle, from under the hips: planted in front
// and sliding back for 12 frames, then lifted and swung through. The far
// foot is half a cycle on. Frame 25 matches frame 1, so it loops.
const step = [[1, 90, 0], [13, -90, 0], [16, -65, -28], [19, 0, -48], [22, 65, -22], [25, 90, 0]];
for (const [side, offset] of [["Near", 0], ["Far", 12]]) {
  for (const [frame, dx, dy] of step) {
    const f = ((frame - 1 + offset) % 24) + 1;
    walker.poseBone(`${side} foot`, f, { moveTo: [x + dx, ground + dy] });
    if (f === 1) walker.poseBone(`${side} foot`, 25, { moveTo: [x + dx, ground + dy] });
  }
}
// The hips: low just after each contact, high as the legs pass.
for (const [frame, drop] of [[1, 22], [4, 30], [7, 12], [10, 8], [13, 22], [16, 30], [19, 12], [22, 8], [25, 22]]) {
  walker.poseBone("Hips", frame, { moveTo: [x, hip - 50 + drop] });
}
console.log("Added a walking character. Press Enter to play it, or select Walker and press F8 to loop it as a symbol.");
