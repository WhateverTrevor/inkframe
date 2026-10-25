// The scripting API: `inkframe`, `Document`, `Layer` and `console`, built
// on the plain functions in native.rs. It runs before every script.
// See docs/SCRIPTING.md for what each part does.
"use strict";

(() => {
  const native = globalThis.__native;
  delete globalThis.__native;
  const parse = (json) => JSON.parse(json);
  const json = (value) => JSON.stringify(value === undefined ? null : value);

  // The properties keys can be set on, in the order setKey applies them.
  const PROPERTIES = ["position", "rotation", "scale", "skew", "opacity", "depth"];

  class Layer {
    constructor(doc, id) {
      this.document = doc;
      this.id = id;
      Object.freeze(this);
    }
    #info() {
      return parse(native.layerGet(this.document._handle, this.id));
    }
    #set(key, value) {
      native.layerSet(this.document._handle, this.id, key, json(value));
    }

    get name() { return this.#info().name; }
    set name(value) { this.#set("name", String(value)); }
    get visible() { return this.#info().visible; }
    set visible(value) { this.#set("visible", Boolean(value)); }
    get locked() { return this.#info().locked; }
    set locked(value) { this.#set("locked", Boolean(value)); }
    /** "vector", "paint", "symbol", "particles", "folder" or "camera". */
    get kind() { return this.#info().kind; }
    /** The first frame the layer shows on. */
    get start() { return this.#info().start; }
    set start(frame) { this.#set("start", frame); }
    /** The last frame it shows on, or null to stay to the end. */
    get end() { return this.#info().end; }
    set end(frame) { this.#set("end", frame); }
    /** The layer it's attached to in a rig, or null. */
    get parent() {
      const id = this.#info().parent;
      return id === null ? null : new Layer(this.document, id);
    }
    set parent(layer) { this.#set("parent", layer === null ? null : layer.id); }
    /** The layers attached to this one in a rig. */
    get children() {
      return this.document.layers.filter((layer) => layer.parent?.id === this.id);
    }
    /** The point it turns and scales around, in its own coordinates: [x, y]. */
    get pivot() { return this.#info().pivot; }
    set pivot(point) { this.#set("pivot", point); }

    // Symbols.
    #instance() { return parse(native.instanceGet(this.document._handle, this.id)); }
    #setInstance(key, value) { native.instanceSet(this.document._handle, this.id, key, json(value)); }
    /** The symbol this layer shows, by name, or null if it isn't an instance. */
    get symbol() { return this.#instance()?.symbol ?? null; }
    /** How an instance plays its symbol: "loop", "play-once" or "single-frame". */
    get playback() { return this.#instance()?.playback ?? null; }
    set playback(value) { this.#setInstance("playback", value); }
    /** The symbol frame an instance shows on its first frame. */
    get firstFrame() { return this.#instance()?.firstFrame ?? null; }
    set firstFrame(frame) { this.#setInstance("firstFrame", frame); }
    /** Turns the layer into a symbol, the layer becoming an instance of it. Returns the symbol's name. */
    convertToSymbol(frame = 1) {
      return native.convertToSymbol(this.document._handle, this.id, frame);
    }

    // Effects and particles.
    /** The colour effect and filters on a frame: { color, filters }. */
    effectsAt(frame) {
      return parse(native.effectsAt(this.document._handle, this.id, frame));
    }
    /** Keys the colour effect and filters on a frame (the form effectsAt gives). */
    setEffects(frame, effects) {
      native.setEffects(this.document._handle, this.id, frame, json(effects));
      return this;
    }
    /** A particle layer's emitter settings, or null for other layers. */
    get particles() { return parse(native.particles(this.document._handle, this.id)); }
    /** Changes emitter settings; ones left out keep their values. */
    set particles(changes) { native.setParticles(this.document._handle, this.id, json(changes)); }

    // Skeletons (for a skeleton's folder).
    /** The skeleton's bones: [{ name, parent, start, end, strength, target, limits, dynamics, squash }], parents and targets by name. */
    get bones() {
      const bones = parse(native.bones(this.document._handle, this.id));
      const name = (i) => (i === null || i === undefined ? null : bones[i].name);
      return bones.map((b) => ({ ...b, parent: name(b.parent), target: name(b.target) }));
    }
    #bone(name) {
      const index = parse(native.bones(this.document._handle, this.id)).findIndex((b) => b.name === name);
      if (index < 0) throw new Error(`there's no bone called ${name}`);
      return index;
    }
    /**
     * Adds a bone at rest from `start` to `end` ([x, y] on the stage),
     * hanging from `options.parent` (a bone's name) or a root of its own.
     */
    addBone(name, start, end, options = {}) {
      const parent = options.parent ? this.#bone(options.parent) : null;
      native.addBone(this.document._handle, this.id, json({ name, start, end, parent }));
      return this;
    }
    /**
     * Changes a bone's settings: `name`, `strength`, `target` (a bone's
     * name, or null), `limits` ([min, max] or null), `squash`, and
     * `dynamics` ({ spring, damping, swing }, or null).
     */
    setBone(name, settings) {
      const changes = { ...settings };
      if ("target" in changes && changes.target !== null) changes.target = this.#bone(changes.target);
      native.setBone(this.document._handle, this.id, this.#bone(name), json(changes));
      return this;
    }
    /**
     * Keys a bone on a frame: `turn` (degrees from rest), `moveTo` ([x, y]
     * for its start, as dragging a root does) and `stretch`.
     */
    poseBone(name, frame, pose) {
      native.poseBone(this.document._handle, this.id, this.#bone(name), frame, json(pose));
      return this;
    }
    /**
     * How a layer in a skeleton's folder follows the bones: "nearby", "none",
     * { bone: name } (moves with one bone) or { limb: name } (bends with
     * that bone and those hanging from it). Null outside a skeleton.
     */
    get follows() {
      const value = parse(native.follows(this.document._handle, this.id));
      if (value === null || typeof value === "string") return value;
      const names = this.#skeletonOwner()?.bones.map((b) => b.name) ?? [];
      return "bone" in value ? { bone: names[value.bone] } : { limb: names[value.limb] };
    }
    set follows(value) {
      let spec = value;
      if (value && typeof value === "object") {
        const owner = this.#skeletonOwner();
        if (!owner) throw new Error(`${this.name} isn't in a skeleton's folder`);
        const index = (name) => {
          const i = owner.bones.findIndex((b) => b.name === name);
          if (i < 0) throw new Error(`there's no bone called ${name}`);
          return i;
        };
        spec = "bone" in value ? { bone: index(value.bone) } : { limb: index(value.limb) };
      }
      native.setFollows(this.document._handle, this.id, json(spec));
    }
    #skeletonOwner() {
      for (let layer = this.#folder(); layer; layer = layer.#folder()) {
        try {
          layer.bones;
          return layer;
        } catch {
          // Not a skeleton's folder; look further out.
        }
      }
      return null;
    }
    #folder() {
      const id = this.#info().folder;
      return id === null || id === undefined ? null : new Layer(this.document, id);
    }

    // Drawings and lip sync.
    /** The layer's drawings, by name. */
    get drawings() { return parse(native.drawings(this.document._handle, this.id)); }
    /** Adds a drawing called `name`: blank, or a copy of the drawing named `options.copy`. */
    addDrawing(name, options = {}) {
      native.addDrawing(this.document._handle, this.id, String(name), options.copy ?? undefined);
      return this;
    }
    /** Shows the drawing called `name` from `frame` on. */
    showDrawing(name, frame) {
      native.showDrawing(this.document._handle, this.id, String(name), frame);
      return this;
    }
    /**
     * Lip-syncs this mouth layer to a sound (by name), matching its drawings
     * to the mouth shapes by their names: from the sound's loudness, or a
     * Rhubarb (.tsv) or Papagayo (.dat) timing file named by `options.timing`.
     * Returns how many mouth changes it made.
     */
    lipSync(sound, options = {}) {
      return native.lipSync(this.document._handle, this.id, String(sound), options.timing ?? undefined);
    }

    // Web pages.
    /**
     * What clicking the layer does in a Web Page export: "play", "stop",
     * { gotoAndPlay: frame }, { gotoAndStop: frame }, { openLink: url }, or null.
     */
    get button() { return parse(native.button(this.document._handle, this.id)); }
    set button(action) { native.setButton(this.document._handle, this.id, json(action)); }

    /** Every key of one property: [{ frame, value, easing }]. */
    keys(property) {
      return parse(native.keys(this.document._handle, this.id, property));
    }
    /** A property's value on a frame (which can be between frames). */
    valueAt(property, frame) {
      return parse(native.valueAt(this.document._handle, this.id, property, frame));
    }
    /**
     * Keys some properties on a frame: setKey(12, { position: [960, 880] }).
     * `options.easing` sets how each travels on to its next key.
     */
    setKey(frame, values, options = {}) {
      for (const property of Object.keys(values)) {
        if (!PROPERTIES.includes(property)) {
          throw new Error(`there's no property called ${property}: use ${PROPERTIES.join(", ")}`);
        }
      }
      for (const property of PROPERTIES) {
        if (property in values) {
          native.setKey(this.document._handle, this.id, frame, property, json(values[property]), json(options.easing));
        }
      }
      return this;
    }
    /** Removes the key on a frame, of one property or of all of them. */
    removeKey(frame, property = "") {
      return native.removeKey(this.document._handle, this.id, frame, property);
    }
    /** Moves the layer's keys later (or earlier, if negative); the layer stays put. */
    shiftKeys(frames) {
      native.shiftKeys(this.document._handle, this.id, frames);
      return this;
    }
    /** Moves the whole layer in time: when it starts and ends, its drawings and its keys. */
    shiftTime(frames) {
      native.shiftTime(this.document._handle, this.id, frames);
      return this;
    }
    /** Moves the layer toward the front (or back, if negative). */
    moveUp(steps = 1) { native.moveLayer(this.document._handle, this.id, steps); }
    moveDown(steps = 1) { native.moveLayer(this.document._handle, this.id, -steps); }
    remove() { native.removeLayer(this.document._handle, this.id); }
    toString() { return `Layer(${this.name})`; }
  }

  class Document {
    constructor(handle) {
      this._handle = handle;
      Object.freeze(this);
    }
    #info() { return parse(native.docInfo(this._handle)); }
    #set(key, value) { native.docSet(this._handle, key, json(value)); }

    get name() { return this.#info().name; }
    /** Its file, or null if it hasn't been saved. */
    get path() { return this.#info().path; }
    get width() { return this.#info().width; }
    set width(value) { this.#set("width", value); }
    get height() { return this.#info().height; }
    set height(value) { this.#set("height", value); }
    get frameRate() { return this.#info().frameRate; }
    set frameRate(value) { this.#set("frameRate", value); }
    /** How many frames long it is. */
    get duration() { return this.#info().duration; }
    set duration(value) { this.#set("duration", value); }
    get background() { return this.#info().background; }
    set background(value) { this.#set("background", value); }
    /** Whether layers are drawn far to near by their depth. */
    get stackByDepth() { return this.#info().stackByDepth; }
    set stackByDepth(value) { this.#set("stackByDepth", Boolean(value)); }

    /** The main timeline's layers, top first, as the timeline lists them. */
    get layers() {
      return parse(native.layers(this._handle)).map((id) => new Layer(this, id));
    }
    /** The first layer with this name, or null. */
    layer(name) {
      return this.layers.find((layer) => layer.name === name) ?? null;
    }
    /**
     * Adds a layer on top. Options: a shape (`rect: [w, h]` with `radius`,
     * `ellipse: [w, h]`, `text` with `size`, or `polygon: [[x, y], ...]`), `fill`, `stroke`,
     * `strokeWidth`, `at: [x, y]` and the first `frame` it shows on.
     */
    addLayer(name, options = {}) {
      return new Layer(this, native.addLayer(this._handle, json({ ...options, name })));
    }

    /** The layers selected in the editor (only for the project the script runs on). */
    get selectedLayers() {
      if (this._handle !== 0) return [];
      return parse(native.selection()).map((id) => new Layer(this, id));
    }
    /** Selects these layers in the editor when the script finishes. */
    select(layers) {
      if (this._handle !== 0) throw new Error("only the project the script runs on has a selection");
      native.select(json(layers.map((layer) => layer.id)));
    }

    /** The symbol library: [{ name, duration, layers, uses }]. */
    get symbols() { return parse(native.symbols(this._handle)); }
    /** Adds an instance of a symbol, by name. Options: `at: [x, y]` and the first `frame`. */
    addInstance(symbol, options = {}) {
      return new Layer(this, native.addInstance(this._handle, String(symbol), json(options)));
    }
    /** Adds particles from a preset ("sparks", "snow", "smoke" or "fire"). Options: `at`, `frame`. */
    addParticleLayer(preset, options = {}) {
      return new Layer(this, native.addParticleLayer(this._handle, String(preset), json(options)));
    }
    /**
     * Puts layers in a new folder with an empty skeleton, as Modify › New
     * Skeleton does, and returns the folder, to add bones to.
     */
    addSkeleton(layers, name = "Skeleton") {
      return new Layer(this, native.addSkeleton(this._handle, json(layers.map((layer) => layer.id)), String(name)));
    }
    /** Brings in a sound file, starting on `frame`. Returns the sound's name. */
    importAudio(path, frame = 1) { return native.importAudio(this._handle, String(path), frame); }
    /** The frames with web page actions: [{ frame, action }]. */
    get frameActions() { return parse(native.frameActions(this._handle)); }
    /** Has a Web Page export do `action` on reaching `frame` ("stop", { gotoAndPlay: 1 }…), or nothing with null. */
    setFrameAction(frame, action) { native.setFrameAction(this._handle, frame, json(action)); }

    /** The camera layer, or null. */
    get camera() {
      const id = native.camera(this._handle);
      return id === null || id === undefined ? null : new Layer(this, id);
    }
    /** The camera, added if there isn't one. */
    addCamera() { return new Layer(this, native.addCamera(this._handle)); }
    /** The master controllers: [{ name, value, poses }]. */
    get controllers() { return parse(native.controllers(this._handle)); }
    /** Sets a controller (by name or number) to a value from 0 to 1 on a frame, posing its layers. */
    setController(controller, value, frame = 1) {
      const index = typeof controller === "number"
        ? controller
        : this.controllers.findIndex((c) => c.name === controller);
      if (index < 0) throw new Error(`there's no controller called ${controller}`);
      native.setController(this._handle, index, value, frame);
    }

    /** The palette's swatches: [{ name, color }]. */
    get swatches() { return parse(native.swatches(this._handle)); }
    addSwatch(name, color) { native.addSwatch(this._handle, String(name), json(color)); }
    /** Changes a swatch's colour, recolouring everything painted in it. */
    setSwatch(index, color) { native.setSwatch(this._handle, index, json(color)); }

    /** Saves the project, to a new path if given. */
    save(path) { native.save(this._handle, path ?? undefined); }
    /**
     * Exports it: { format: "h264", to: "renders/shot.mp4" }, with
     * optional `scale`, `transparent` and `frames: [first, last]`.
     * Returns the file written.
     */
    export(options) { return native.export(this._handle, json(options)); }
    toString() { return `Document(${this.name})`; }
  }

  const print = (...parts) => native.log(parts.map(describe).join(" "));
  function describe(value) {
    if (typeof value === "string") return value;
    if (value instanceof Layer || value instanceof Document) return String(value);
    if (value instanceof Error) return value.message;
    try {
      return JSON.stringify(value) ?? String(value);
    } catch {
      return String(value);
    }
  }

  globalThis.console = Object.freeze({ log: print, info: print, warn: print, error: print });
  globalThis.inkframe = Object.freeze({
    /** The project the script was run on. */
    document: new Document(0),
    /** Opens another project, to read, change, save or export. */
    open(path) { return new Document(native.open(String(path))); },
    Document,
    Layer,
  });
})();
