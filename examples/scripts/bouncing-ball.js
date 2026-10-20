// Bouncing ball: builds the classic first animation from scratch: a ball
// that falls, squashes on the floor and bounces back up, with easing for
// weight. A good script to read first.

const doc = inkframe.document;
const floorY = doc.height - 160;
const top = 200;
const x = doc.width / 2;

doc.addLayer("Floor", { rect: [doc.width, 40], fill: "#3c4254", at: [x, floorY + 80] });
const ball = doc.addLayer("Ball", { ellipse: [120, 120], fill: "#e8543c", at: [x, top] });

// Falling speeds up (ease in); rising slows down (ease out).
ball.setKey(1, { position: [x, top], scale: [1, 1] }, { easing: "ease-in" });
ball.setKey(10, { scale: [1, 1] });
ball.setKey(12, { position: [x, floorY], scale: [1.25, 0.75] }, { easing: "ease-out" });
ball.setKey(14, { scale: [1, 1] });
ball.setKey(24, { position: [x, top] });
console.log("Added a bouncing ball. Press Enter to play it.");
