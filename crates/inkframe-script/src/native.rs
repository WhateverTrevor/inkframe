//! The functions scripts reach Inkframe through.
//!
//! They're deliberately plain: documents and layers are numbers, and
//! anything with structure travels as JSON. `prelude.js` wraps them in the
//! `Document` and `Layer` classes scripts actually use, so this side stays
//! a thin, easily checked layer over `inkframe-core`, and the friendly API
//! is ordinary JavaScript.
//!
//! Frames are counted from 1 here, as the timeline shows them, and turned
//! into the document's 0-based frames on the way in and out.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use inkframe_core::{
    Color, Document, Effects, Element, Emitter, FrameRate, LayerId, Path as Outline, Playback, Point, Preset,
    Property, Segment, Shape, Stroke, Style, Transform,
};
use rquickjs::{Ctx, Exception, Function, Object};
use serde_json::{Value, json};

/// A document a script is working on: the one it was run on (always the
/// first), or one it opened.
pub(crate) struct Open {
    pub doc: Document,
    pub path: Option<PathBuf>,
}

/// Everything the natives share.
pub(crate) struct State {
    pub docs: Vec<Open>,
    /// Where relative paths start from, if anywhere.
    pub base_dir: Option<PathBuf>,
    /// The selected layers of the first document.
    pub selection: Vec<LayerId>,
    /// Whether the script changed the selection.
    pub selection_changed: bool,
    pub log: Vec<String>,
}

pub(crate) type Shared = Rc<RefCell<State>>;

/// Installs the natives as `globalThis.__native`, which the prelude takes
/// and removes.
pub(crate) fn install<'js>(ctx: &Ctx<'js>, state: &Shared) -> rquickjs::Result<()> {
    let native = Object::new(ctx.clone())?;
    macro_rules! native {
        ($name:literal, $f:expr) => {
            native.set($name, Function::new(ctx.clone(), $f)?)?;
        };
    }

    let s = Rc::clone(state);
    native!("log", move |text: String| s.borrow_mut().log.push(text));

    let s = Rc::clone(state);
    native!("docInfo", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<String> {
        let state = s.borrow();
        let open = doc(&ctx, &state, d)?;
        let doc = &open.doc;
        Ok(json!({
            "name": open.path.as_deref().and_then(Path::file_stem).map_or("Untitled".into(), |s| s.to_string_lossy()),
            "path": open.path.as_ref().map(|p| p.display().to_string()),
            "width": doc.width,
            "height": doc.height,
            "frameRate": doc.frame_rate.fps(),
            "duration": doc.duration,
            "background": doc.background.to_string(),
            "stackByDepth": doc.stack_by_depth,
        })
        .to_string())
    });

    let s = Rc::clone(state);
    native!("docSet", move |ctx: Ctx<'js>, d: usize, key: String, value: String| -> rquickjs::Result<()> {
        let value = parse(&ctx, &value)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        match key.as_str() {
            "width" => doc.width = whole(&ctx, &value, "width", 1..=16384)?,
            "height" => doc.height = whole(&ctx, &value, "height", 1..=16384)?,
            "duration" => doc.duration = whole(&ctx, &value, "duration", 1..=1_000_000)?,
            "frameRate" => doc.frame_rate = FrameRate::new(whole(&ctx, &value, "frameRate", 1..=240)?, 1),
            "background" => doc.background = color(&ctx, &value)?,
            "stackByDepth" => doc.stack_by_depth = flag(&ctx, &value)?,
            _ => return Err(throw(&ctx, format!("documents have no setting called {key}"))),
        }
        Ok(())
    });

    let s = Rc::clone(state);
    native!("layers", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        // Top first, as the timeline lists them.
        let ids: Vec<Value> = doc.layers.iter().rev().map(|l| id_value(l.id)).collect();
        Ok(Value::Array(ids).to_string())
    });

    let s = Rc::clone(state);
    native!("layerGet", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let kind = if layer.camera {
            "camera"
        } else if layer.folder {
            "folder"
        } else if layer.particles.is_some() {
            "particles"
        } else if layer.instance.is_some() {
            "symbol"
        } else if layer.all_elements().any(|e| matches!(e.shape, Shape::Paint { .. })) {
            "paint"
        } else {
            "vector"
        };
        Ok(json!({
            "name": layer.name,
            "visible": layer.visible,
            "locked": layer.locked,
            "kind": kind,
            "start": layer.start + 1,
            "end": layer.end.map(|e| e + 1),
            "parent": layer.parent.map(id_value),
            "folder": layer.in_folder.map(id_value),
            "pivot": [layer.pivot.x, layer.pivot.y],
        })
        .to_string())
    });

    let s = Rc::clone(state);
    native!("layerSet", move |ctx: Ctx<'js>,
                              d: usize,
                              id: f64,
                              key: String,
                              value: String|
          -> rquickjs::Result<()> {
        let value = parse(&ctx, &value)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let id = layer_id(id);
        if key == "parent" {
            let parent = match &value {
                Value::Null => None,
                v => Some(layer_id(v.as_f64().ok_or_else(|| throw(&ctx, "parent must be a layer or null"))?)),
            };
            if !doc.set_parent(id, parent, 0) {
                return Err(throw(&ctx, "that parent would make a loop, or isn't in the same timeline"));
            }
            return Ok(());
        }
        let layer = doc.layer_mut(id).ok_or_else(|| gone(&ctx))?;
        match key.as_str() {
            "pivot" => layer.pivot = point(&ctx, &value)?,
            "name" => {
                layer.name = value.as_str().ok_or_else(|| throw(&ctx, "a name must be text"))?.to_owned()
            }
            "visible" => layer.visible = flag(&ctx, &value)?,
            "locked" => layer.locked = flag(&ctx, &value)?,
            "start" => layer.set_start(frame_index(&ctx, &value, "start")?),
            "end" => layer.set_end(match value {
                Value::Null => None,
                v => Some(frame_index(&ctx, &v, "end")?),
            }),
            _ => return Err(throw(&ctx, format!("layers have no setting called {key}"))),
        }
        Ok(())
    });

    let s = Rc::clone(state);
    native!("addLayer", move |ctx: Ctx<'js>, d: usize, spec: String| -> rquickjs::Result<f64> {
        let spec = parse(&ctx, &spec)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let name = spec["name"].as_str().unwrap_or("Layer").to_owned();
        let elements = shape(&ctx, &spec)?.into_iter().collect();
        let at = match &spec["at"] {
            Value::Null => doc.stage_rect().center(),
            v => point(&ctx, v)?,
        };
        let frame = match &spec["frame"] {
            Value::Null => 0,
            v => frame_index(&ctx, v, "frame")?,
        };
        let id = doc.add_layer(name, elements, frame, Transform::at(at));
        Ok(id_number(id))
    });

    let s = Rc::clone(state);
    native!("removeLayer", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<()> {
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        doc.remove_layer(layer_id(id)).map(|_| ()).ok_or_else(|| gone(&ctx))
    });

    let s = Rc::clone(state);
    native!("moveLayer", move |ctx: Ctx<'js>, d: usize, id: f64, by: i32| -> rquickjs::Result<()> {
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        doc.move_layer(layer_id(id), by as isize);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("keys", move |ctx: Ctx<'js>, d: usize, id: f64, property: String| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let track = &doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?.track;
        let keys = crate::values::keys(track, &property).map_err(|e| throw(&ctx, e))?;
        Ok(Value::Array(keys).to_string())
    });

    let s = Rc::clone(state);
    native!("valueAt", move |ctx: Ctx<'js>,
                             d: usize,
                             id: f64,
                             property: String,
                             frame: f64|
          -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let track = &doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?.track;
        let value =
            crate::values::value_at(track, &property, (frame - 1.0) as f32).map_err(|e| throw(&ctx, e))?;
        Ok(value.to_string())
    });

    let s = Rc::clone(state);
    native!("setKey", move |ctx: Ctx<'js>,
                            d: usize,
                            id: f64,
                            frame: f64,
                            property: String,
                            value: String,
                            easing: String|
          -> rquickjs::Result<()> {
        let (value, easing) = (parse(&ctx, &value)?, parse(&ctx, &easing)?);
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let easing = match easing {
            Value::Null => None,
            e => Some(crate::values::easing(&e).map_err(|e| throw(&ctx, e))?),
        };
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let track = &mut doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?.track;
        crate::values::set_key(track, &property, frame, &value, easing).map_err(|e| throw(&ctx, e))
    });

    let s = Rc::clone(state);
    native!("removeKey", move |ctx: Ctx<'js>,
                               d: usize,
                               id: f64,
                               frame: f64,
                               property: String|
          -> rquickjs::Result<bool> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let track = &mut doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?.track;
        if property.is_empty() {
            return Ok(track.remove_key(frame));
        }
        let property = crate::values::property(&property).map_err(|e| throw(&ctx, e))?;
        Ok(track.channel_mut(property).remove_key(frame))
    });

    let s = Rc::clone(state);
    // Just the animation: every property's keys, the layer staying where
    // it is in time.
    native!("shiftKeys", move |ctx: Ctx<'js>, d: usize, id: f64, by: i32| -> rquickjs::Result<()> {
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let track = &mut doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?.track;
        for property in Property::ALL {
            track.channel_mut(property).shift(by as i64);
        }
        Ok(())
    });

    let s = Rc::clone(state);
    // The whole layer: when it starts and ends, its drawings and its keys.
    native!("shiftTime", move |ctx: Ctx<'js>, d: usize, id: f64, by: i32| -> rquickjs::Result<()> {
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?.shift_time(by as i64);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("swatches", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let version = doc.palette.active;
        let swatches: Vec<Value> = doc
            .palette
            .swatches
            .iter()
            .map(|sw| json!({ "name": sw.name, "color": sw.colors.get(version).copied().unwrap_or(Color::BLACK).to_string() }))
            .collect();
        Ok(Value::Array(swatches).to_string())
    });

    let s = Rc::clone(state);
    native!("addSwatch", move |ctx: Ctx<'js>,
                               d: usize,
                               name: String,
                               value: String|
          -> rquickjs::Result<()> {
        let value = color(&ctx, &parse(&ctx, &value)?)?;
        let mut state = s.borrow_mut();
        doc_mut(&ctx, &mut state, d)?.doc.add_swatch(name, value);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("setSwatch", move |ctx: Ctx<'js>,
                               d: usize,
                               index: usize,
                               value: String|
          -> rquickjs::Result<()> {
        let value = color(&ctx, &parse(&ctx, &value)?)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        if index >= doc.palette.swatches.len() {
            return Err(throw(&ctx, format!("there's no swatch {index}")));
        }
        // Recolours everything painted in the swatch's colour, as editing
        // it in the Swatches panel does.
        doc.set_swatch_color(index, value);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("selection", move || -> String {
        Value::Array(s.borrow().selection.iter().map(|id| id_value(*id)).collect()).to_string()
    });

    let s = Rc::clone(state);
    native!("select", move |ctx: Ctx<'js>, ids: String| -> rquickjs::Result<()> {
        let ids = parse(&ctx, &ids)?;
        let ids: Vec<LayerId> =
            ids.as_array().into_iter().flatten().filter_map(Value::as_f64).map(layer_id).collect();
        let mut state = s.borrow_mut();
        state.selection = ids;
        state.selection_changed = true;
        Ok(())
    });

    let s = Rc::clone(state);
    native!("open", move |ctx: Ctx<'js>, path: String| -> rquickjs::Result<usize> {
        let mut state = s.borrow_mut();
        let path = project_path(&ctx, &state, &path)?;
        let doc = Document::load(&path)
            .map_err(|e| throw(&ctx, format!("couldn't open {}: {e}", path.display())))?;
        state.docs.push(Open { doc, path: Some(path) });
        Ok(state.docs.len() - 1)
    });

    let s = Rc::clone(state);
    native!("save", move |ctx: Ctx<'js>, d: usize, path: Option<String>| -> rquickjs::Result<()> {
        let mut state = s.borrow_mut();
        let path = match path {
            Some(p) => project_path(&ctx, &state, &p)?,
            None => doc(&ctx, &state, d)?
                .path
                .clone()
                .ok_or_else(|| throw(&ctx, "this project hasn't been saved yet: give save() a path"))?,
        };
        let open = doc_mut(&ctx, &mut state, d)?;
        open.doc.save(&path).map_err(|e| throw(&ctx, format!("couldn't save {}: {e}", path.display())))?;
        open.path = Some(path);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("export", move |ctx: Ctx<'js>, d: usize, options: String| -> rquickjs::Result<String> {
        let options = parse(&ctx, &options)?;
        let state = s.borrow();
        let open = doc(&ctx, &state, d)?;
        let output =
            crate::files::export(&open.doc, open.path.as_deref(), state.base_dir.as_deref(), &options)
                .map_err(|e| throw(&ctx, e))?;
        Ok(output.display().to_string())
    });

    // ---------------------------------------------------------- symbols

    let s = Rc::clone(state);
    native!("symbols", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let list: Vec<Value> = doc
            .symbols
            .iter()
            .map(|sym| {
                json!({ "name": sym.name, "duration": sym.duration, "layers": sym.layers.len(), "uses": doc.symbol_use_count(sym.id) })
            })
            .collect();
        Ok(Value::Array(list).to_string())
    });

    let s = Rc::clone(state);
    native!("convertToSymbol", move |ctx: Ctx<'js>,
                                     d: usize,
                                     id: f64,
                                     frame: f64|
          -> rquickjs::Result<String> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let symbol = doc
            .convert_layer_to_symbol(layer_id(id), frame)
            .ok_or_else(|| throw(&ctx, "that layer can't become a symbol"))?;
        Ok(doc.symbol(symbol).map(|s| s.name.clone()).unwrap_or_default())
    });

    let s = Rc::clone(state);
    native!("addInstance", move |ctx: Ctx<'js>,
                                 d: usize,
                                 name: String,
                                 spec: String|
          -> rquickjs::Result<f64> {
        let spec = parse(&ctx, &spec)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let symbol = doc
            .symbols
            .iter()
            .find(|sym| sym.name == name)
            .map(|sym| sym.id)
            .ok_or_else(|| throw(&ctx, format!("there's no symbol called {name}")))?;
        let at = match &spec["at"] {
            Value::Null => doc.stage_rect().center(),
            v => point(&ctx, v)?,
        };
        let frame = match &spec["frame"] {
            Value::Null => 0,
            v => frame_index(&ctx, v, "frame")?,
        };
        let id = doc
            .add_instance_layer(inkframe_core::TimelineId::Main, symbol, frame, at)
            .ok_or_else(|| throw(&ctx, "couldn't add the instance"))?;
        Ok(id_number(id))
    });

    let s = Rc::clone(state);
    native!("instanceGet", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let Some(instance) = layer.instance else { return Ok("null".to_owned()) };
        Ok(json!({
            "symbol": doc.symbol(instance.symbol).map(|s| s.name.clone()),
            "playback": playback_name(instance.playback),
            "firstFrame": instance.first_frame + 1,
        })
        .to_string())
    });

    let s = Rc::clone(state);
    native!("instanceSet", move |ctx: Ctx<'js>,
                                 d: usize,
                                 id: f64,
                                 key: String,
                                 value: String|
          -> rquickjs::Result<()> {
        let value = parse(&ctx, &value)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let layer = doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let instance =
            layer.instance.as_mut().ok_or_else(|| throw(&ctx, "that layer isn't a symbol instance"))?;
        match key.as_str() {
            "playback" => {
                instance.playback = match value.as_str() {
                    Some("loop") => Playback::Loop,
                    Some("play-once") => Playback::PlayOnce,
                    Some("single-frame") => Playback::SingleFrame,
                    _ => return Err(throw(&ctx, "playback is \"loop\", \"play-once\" or \"single-frame\"")),
                }
            }
            "firstFrame" => instance.first_frame = frame_index(&ctx, &value, "firstFrame")?,
            _ => return Err(throw(&ctx, format!("instances have no setting called {key}"))),
        }
        Ok(())
    });

    // ------------------------------------------------------ controllers

    let s = Rc::clone(state);
    native!("controllers", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let list: Vec<Value> = doc
            .controllers
            .iter()
            .map(|c| json!({ "name": c.name, "value": c.value, "poses": c.poses.len() }))
            .collect();
        Ok(Value::Array(list).to_string())
    });

    let s = Rc::clone(state);
    native!("setController", move |ctx: Ctx<'js>,
                                   d: usize,
                                   index: usize,
                                   value: f64,
                                   frame: f64|
          -> rquickjs::Result<()> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        if index >= doc.controllers.len() {
            return Err(throw(&ctx, format!("there's no controller {index}")));
        }
        doc.set_controller(index, value as f32, frame);
        Ok(())
    });

    // ------------------------------------------------ effects, particles

    let s = Rc::clone(state);
    native!("effectsAt", move |ctx: Ctx<'js>, d: usize, id: f64, frame: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let effects = layer.track.effects_at((frame - 1.0) as f32);
        let saved = serde_json::to_value(effects).map_err(|e| throw(&ctx, e.to_string()))?;
        Ok(crate::values::to_script(saved).to_string())
    });

    let s = Rc::clone(state);
    native!("setEffects", move |ctx: Ctx<'js>,
                                d: usize,
                                id: f64,
                                frame: f64,
                                value: String|
          -> rquickjs::Result<()> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let value = crate::values::from_script(parse(&ctx, &value)?);
        let effects: Effects =
            serde_json::from_value(value).map_err(|e| throw(&ctx, format!("those aren't effects: {e}")))?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let layer = doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        layer.track.effects.set(frame, effects);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("particles", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let saved = serde_json::to_value(&layer.particles).map_err(|e| throw(&ctx, e.to_string()))?;
        Ok(crate::values::to_script(saved).to_string())
    });

    let s = Rc::clone(state);
    native!("setParticles", move |ctx: Ctx<'js>, d: usize, id: f64, value: String| -> rquickjs::Result<()> {
        let value = crate::values::from_script(parse(&ctx, &value)?);
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let layer = doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let current = layer.particles.clone().ok_or_else(|| throw(&ctx, "that isn't a particle layer"))?;
        // Settings left out keep their values, so a script can change one.
        let mut merged = serde_json::to_value(current).map_err(|e| throw(&ctx, e.to_string()))?;
        if let (Some(into), Value::Object(changes)) = (merged.as_object_mut(), value) {
            into.extend(changes);
        }
        let emitter: Emitter = serde_json::from_value(merged)
            .map_err(|e| throw(&ctx, format!("those aren't particle settings: {e}")))?;
        layer.particles = Some(emitter);
        Ok(())
    });

    let s = Rc::clone(state);
    native!("addParticleLayer", move |ctx: Ctx<'js>,
                                      d: usize,
                                      preset: String,
                                      spec: String|
          -> rquickjs::Result<f64> {
        let spec = parse(&ctx, &spec)?;
        let preset =
            Preset::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(&preset)).ok_or_else(|| {
                throw(&ctx, format!("{preset} isn't a particle preset: use sparks, snow, smoke or fire"))
            })?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let at = match &spec["at"] {
            Value::Null => doc.stage_rect().center(),
            v => point(&ctx, v)?,
        };
        let frame = match &spec["frame"] {
            Value::Null => 0,
            v => frame_index(&ctx, v, "frame")?,
        };
        let id =
            doc.add_particle_layer(preset, frame, at).ok_or_else(|| throw(&ctx, "couldn't add particles"))?;
        Ok(id_number(id))
    });

    // ----------------------------------------------------------- camera

    let s = Rc::clone(state);
    native!("camera", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<Option<f64>> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        Ok(doc.camera_layer().map(|c| id_number(c.id)))
    });

    let s = Rc::clone(state);
    native!("addCamera", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<f64> {
        let mut state = s.borrow_mut();
        Ok(id_number(doc_mut(&ctx, &mut state, d)?.doc.add_camera()))
    });

    crate::rigging::install(ctx, &native, state)?;
    ctx.globals().set("__native", native)?;
    Ok(())
}

fn playback_name(playback: Playback) -> &'static str {
    match playback {
        Playback::Loop => "loop",
        Playback::PlayOnce => "play-once",
        Playback::SingleFrame => "single-frame",
    }
}

/// A JavaScript error with `message`, to return from a native.
pub(crate) fn throw(ctx: &Ctx<'_>, message: impl AsRef<str>) -> rquickjs::Error {
    Exception::throw_message(ctx, message.as_ref())
}

pub(crate) fn gone(ctx: &Ctx<'_>) -> rquickjs::Error {
    throw(ctx, "that layer isn't in the document any more")
}

pub(crate) fn parse(ctx: &Ctx<'_>, json: &str) -> rquickjs::Result<Value> {
    serde_json::from_str(json).map_err(|e| throw(ctx, format!("bad value: {e}")))
}

pub(crate) fn doc<'a>(ctx: &Ctx<'_>, state: &'a State, d: usize) -> rquickjs::Result<&'a Open> {
    state.docs.get(d).ok_or_else(|| throw(ctx, "that document isn't open"))
}

pub(crate) fn doc_mut<'a>(ctx: &Ctx<'_>, state: &'a mut State, d: usize) -> rquickjs::Result<&'a mut Open> {
    state.docs.get_mut(d).ok_or_else(|| throw(ctx, "that document isn't open"))
}

/// Layer ids are numbers to scripts.
pub(crate) fn id_value(id: LayerId) -> Value {
    serde_json::to_value(id).unwrap_or(Value::Null)
}

pub(crate) fn id_number(id: LayerId) -> f64 {
    id_value(id).as_f64().unwrap_or(-1.0)
}

pub(crate) fn layer_id(n: f64) -> LayerId {
    serde_json::from_value(json!(n.max(0.0) as u64))
        .unwrap_or_else(|_| serde_json::from_value(json!(0)).unwrap())
}

pub(crate) fn whole<T: TryFrom<u64>>(
    ctx: &Ctx<'_>,
    value: &Value,
    what: &str,
    range: std::ops::RangeInclusive<u64>,
) -> rquickjs::Result<T> {
    let n = value.as_f64().filter(|n| n.fract() == 0.0 && *n >= 0.0).map(|n| n as u64);
    let n = n.filter(|n| range.contains(n)).ok_or_else(|| {
        throw(ctx, format!("{what} must be a whole number from {} to {}", range.start(), range.end()))
    })?;
    T::try_from(n).map_err(|_| throw(ctx, format!("{what} is too large")))
}

/// A frame as scripts number it (from 1), as the document does (from 0).
pub(crate) fn frame_index(ctx: &Ctx<'_>, value: &Value, what: &str) -> rquickjs::Result<u32> {
    Ok(whole::<u32>(ctx, value, what, 1..=u64::from(u32::MAX))? - 1)
}

pub(crate) fn flag(ctx: &Ctx<'_>, value: &Value) -> rquickjs::Result<bool> {
    value.as_bool().ok_or_else(|| throw(ctx, "expected true or false"))
}

fn color(ctx: &Ctx<'_>, value: &Value) -> rquickjs::Result<Color> {
    let text = value.as_str().ok_or_else(|| throw(ctx, "a colour must be text like \"#ff6633\""))?;
    let text = if text.len() == 4 && text.starts_with('#') {
        // "#f63" as in CSS.
        let c: String = text[1..].chars().flat_map(|c| [c, c]).collect();
        format!("#{c}")
    } else {
        text.to_owned()
    };
    text.parse().map_err(|_| throw(ctx, format!("{text} isn't a colour like \"#ff6633\"")))
}

pub(crate) fn point(ctx: &Ctx<'_>, value: &Value) -> rquickjs::Result<Point> {
    crate::values::point(value).map_err(|e| throw(ctx, e))
}

/// The shape described by `spec` (`rect`, `ellipse`, `text` with `size`, or `polygon`, with
/// `fill`, `stroke` and `strokeWidth`), if any.
fn shape(ctx: &Ctx<'_>, spec: &Value) -> rquickjs::Result<Option<Element>> {
    let size = |v: &Value| -> rquickjs::Result<(f32, f32)> {
        let p = point(ctx, v)?;
        Ok((p.x, p.y))
    };
    let shape = if !spec["rect"].is_null() {
        let (width, height) = size(&spec["rect"])?;
        let corner_radius = spec["radius"].as_f64().unwrap_or(0.0) as f32;
        Shape::Rect { width, height, corner_radius }
    } else if !spec["ellipse"].is_null() {
        let (width, height) = size(&spec["ellipse"])?;
        Shape::Ellipse { radius_x: width / 2.0, radius_y: height / 2.0 }
    } else if let Some(content) = spec["text"].as_str() {
        let size = spec["size"].as_f64().unwrap_or(48.0) as f32;
        Shape::Text(inkframe_core::Text::new(content, size))
    } else if let Some(points) = spec["polygon"].as_array() {
        let points = points.iter().map(|p| point(ctx, p)).collect::<rquickjs::Result<Vec<_>>>()?;
        let Some((&start, rest)) = points.split_first() else {
            return Err(throw(ctx, "a polygon needs points"));
        };
        let segments = rest.iter().map(|&to| Segment::Line { to }).collect();
        Shape::Path(Outline { start, segments, closed: true })
    } else {
        return Ok(None);
    };
    let fill = match &spec["fill"] {
        Value::Null => (spec["stroke"].is_null()).then_some(Color::BLACK),
        v => Some(color(ctx, v)?),
    };
    let stroke = match &spec["stroke"] {
        Value::Null => None,
        v => Some(Stroke::new(color(ctx, v)?, spec["strokeWidth"].as_f64().unwrap_or(2.0) as f32)),
    };
    Ok(Some(Element { shape, style: Style { fill, stroke, gradient: None } }))
}

/// A project file path a script named, which must be an `.inkf` file.
fn project_path(ctx: &Ctx<'_>, state: &State, path: &str) -> rquickjs::Result<PathBuf> {
    let path = crate::files::resolve(path, state.base_dir.as_deref()).map_err(|e| throw(ctx, e))?;
    if path.extension().is_none_or(|e| e != "inkf") {
        return Err(throw(ctx, format!("{} isn't an Inkframe project (.inkf)", path.display())));
    }
    Ok(path)
}
