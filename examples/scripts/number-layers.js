// Number layers: renames every layer to a prefix and a number, top to
// bottom ("Drawing 01", "Drawing 02", ...), as some studios' pipelines
// expect. The camera and folders keep their names.

const PREFIX = "Drawing";

const layers = inkframe.document.layers.filter((l) => l.kind !== "camera" && l.kind !== "folder");
const digits = Math.max(2, String(layers.length).length);
layers.forEach((layer, i) => {
  layer.name = `${PREFIX} ${String(i + 1).padStart(digits, "0")}`;
});
console.log(`Renamed ${layers.length} layers.`);
