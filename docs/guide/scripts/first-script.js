// A first script: a ball that drops onto the floor and bounces back.
const doc = inkframe.document;
const ball = doc.addLayer("Ball", { ellipse: [120, 120], fill: "#e8543c", at: [960, 200] });
ball.setKey(1, { position: [960, 200] }, { easing: "ease-in" });
ball.setKey(12, { position: [960, 880] }, { easing: "ease-out" });
ball.setKey(24, { position: [960, 200] });
console.log(`Added ${ball.name}, keyed on frames`, ball.keys("position").map((k) => k.frame));
