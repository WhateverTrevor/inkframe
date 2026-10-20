// Export for review: saves a small H.264 video beside the project, named
// after it with today's date, ready to send for feedback. The project
// needs saving first, so the video has a folder to go in.

const doc = inkframe.document;
if (doc.path === null) {
  throw new Error("save the project first, so the video has somewhere to go");
}
const today = new Date().toISOString().slice(0, 10);
const file = doc.export({ format: "h264", to: `${doc.name}_review_${today}.mp4`, scale: 0.5 });
console.log(`Exported ${file}`);
