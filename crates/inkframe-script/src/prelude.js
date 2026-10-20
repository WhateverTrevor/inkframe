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
     * `ellipse: [w, h]`, or `polygon: [[x, y], ...]`), `fill`, `stroke`,
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
