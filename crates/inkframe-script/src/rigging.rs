//! The natives for characters and the web: skeletons and their bones, how
//! layers follow them, drawings and lip sync, and web page buttons and
//! frame actions. Installed alongside the rest (see `native.rs`), wrapped
//! by the prelude's `Layer` and `Document` methods.
//!
//! Bones are numbered by their place in the skeleton, as in the core; the
//! prelude lets scripts name them instead. Frames count from 1.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use inkframe_core::{
    BoneBinding, Document, Dynamics, LayerId, MouthMap, RotationLimits, Skeleton, TimelineId, WebAction,
};
use rquickjs::{Ctx, Function, Object};
use serde_json::{Value, json};

use crate::native::{
    Shared, doc, doc_mut, flag, frame_index, gone, id_number, layer_id, parse, point, throw,
};

/// The sound files a script may bring into a project, by extension, as the
/// app imports them.
const AUDIO_EXTENSIONS: [&str; 9] = ["wav", "mp3", "m4a", "aac", "flac", "ogg", "aif", "aiff", "caf"];

pub(crate) fn install<'js>(ctx: &Ctx<'js>, native: &Object<'js>, state: &Shared) -> rquickjs::Result<()> {
    macro_rules! native {
        ($name:literal, $f:expr) => {
            native.set($name, Function::new(ctx.clone(), $f)?)?;
        };
    }

    // ------------------------------------------------------------ skeletons

    let s = Rc::clone(state);
    // The layers go into a new folder with an empty skeleton, as
    // Modify › New Skeleton does.
    native!("addSkeleton", move |ctx: Ctx<'js>,
                                 d: usize,
                                 layers: String,
                                 name: String|
          -> rquickjs::Result<f64> {
        let ids: Vec<LayerId> = match parse(&ctx, &layers)? {
            Value::Array(ids) => ids.iter().filter_map(Value::as_f64).map(layer_id).collect(),
            _ => return Err(throw(&ctx, "addSkeleton needs a list of layers")),
        };
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        if ids.is_empty()
            || ids
                .iter()
                .any(|&id| doc.layer(id).is_none_or(|l| doc.timeline_of(l.id) != Some(TimelineId::Main)))
        {
            return Err(throw(&ctx, "a skeleton needs one or more layers of the main timeline"));
        }
        let folder = doc
            .add_folder(TimelineId::Main, &ids)
            .ok_or_else(|| throw(&ctx, "couldn't make the skeleton's folder"))?;
        let layer = doc.layer_mut(folder).ok_or_else(|| gone(&ctx))?;
        layer.skeleton = Some(Skeleton::default());
        layer.name = name;
        Ok(id_number(folder))
    });

    let s = Rc::clone(state);
    native!("bones", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let skeleton = skeleton(&ctx, &doc(&ctx, &state, d)?.doc, id)?;
        let bones: Vec<Value> = skeleton
            .bones
            .iter()
            .map(|b| {
                json!({
                    "name": b.name,
                    "parent": b.parent,
                    "start": [b.start.x, b.start.y],
                    "end": [b.end.x, b.end.y],
                    "strength": b.strength,
                    "target": b.target,
                    "limits": b.limits.map(|l| [l.min, l.max]),
                    "dynamics": b.dynamics.map(|d| json!({ "spring": d.spring, "damping": d.damping, "swing": d.limit })),
                    "squash": b.squash,
                })
            })
            .collect();
        Ok(Value::Array(bones).to_string())
    });

    let s = Rc::clone(state);
    // A bone at rest from `start` to `end`, hanging from `parent` (a bone's
    // place) or a root. Returns its place.
    native!("addBone", move |ctx: Ctx<'js>, d: usize, id: f64, spec: String| -> rquickjs::Result<usize> {
        let spec = parse(&ctx, &spec)?;
        let (start, end) = (point(&ctx, &spec["start"])?, point(&ctx, &spec["end"])?);
        let parent = spec["parent"].as_u64().map(|p| p as usize);
        let mut state = s.borrow_mut();
        let skeleton = skeleton_mut(&ctx, &mut doc_mut(&ctx, &mut state, d)?.doc, id)?;
        if parent.is_some_and(|p| p >= skeleton.bones.len()) {
            return Err(throw(&ctx, "the parent bone isn't in this skeleton"));
        }
        let bone = skeleton.add_bone(parent, start, end, 0);
        if let Some(name) = spec["name"].as_str() {
            skeleton.bones[bone].name = name.to_owned();
        }
        Ok(bone)
    });

    let s = Rc::clone(state);
    // Changes a bone's settings: `name`, `strength`, `target` (a bone's
    // place, or null), `limits` ([min, max] or null), `squash`, and
    // `dynamics` ({ spring, damping, swing }, or null).
    native!("setBone", move |ctx: Ctx<'js>,
                             d: usize,
                             id: f64,
                             bone: usize,
                             spec: String|
          -> rquickjs::Result<()> {
        let spec = parse(&ctx, &spec)?;
        let mut state = s.borrow_mut();
        let skeleton = skeleton_mut(&ctx, &mut doc_mut(&ctx, &mut state, d)?.doc, id)?;
        let count = skeleton.bones.len();
        let b = skeleton.bones.get_mut(bone).ok_or_else(|| throw(&ctx, "that bone isn't in the skeleton"))?;
        let Value::Object(changes) = spec else {
            return Err(throw(&ctx, "setBone needs { setting: value }"));
        };
        for (key, value) in changes {
            match key.as_str() {
                "name" => {
                    b.name = value.as_str().ok_or_else(|| throw(&ctx, "a name must be text"))?.to_owned()
                }
                "strength" => {
                    b.strength =
                        value.as_f64().ok_or_else(|| throw(&ctx, "strength must be a number"))?.max(1.0)
                            as f32
                }
                "squash" => b.squash = flag(&ctx, &value)?,
                "target" => {
                    b.target = match value {
                        Value::Null => None,
                        v => Some(
                            v.as_u64().map(|t| t as usize).filter(|&t| t < count && t != bone).ok_or_else(
                                || throw(&ctx, "a target must be another bone of the skeleton"),
                            )?,
                        ),
                    }
                }
                "limits" => {
                    b.limits = match value {
                        Value::Null => None,
                        v => {
                            let p = point(&ctx, &v)?;
                            Some(RotationLimits { min: p.x.min(p.y), max: p.x.max(p.y) })
                        }
                    }
                }
                "dynamics" => {
                    b.dynamics = match value {
                        Value::Null | Value::Bool(false) => None,
                        v => {
                            let base = Dynamics::default();
                            let get = |key: &str, or: f32| v[key].as_f64().map_or(or, |n| n as f32);
                            Some(Dynamics {
                                spring: get("spring", base.spring).clamp(0.0, 1.0),
                                damping: get("damping", base.damping).clamp(0.0, 1.0),
                                limit: get("swing", base.limit).max(0.0),
                            })
                        }
                    }
                }
                other => return Err(throw(&ctx, format!("bones have no setting called {other}"))),
            }
        }
        Ok(())
    });

    let s = Rc::clone(state);
    // Keys a bone on a frame: `turn` (degrees from rest), `moveTo` (where
    // its start goes, as dragging a root does), `stretch`.
    native!("poseBone", move |ctx: Ctx<'js>,
                              d: usize,
                              id: f64,
                              bone: usize,
                              frame: f64,
                              spec: String|
          -> rquickjs::Result<()> {
        let spec = parse(&ctx, &spec)?;
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let mut state = s.borrow_mut();
        let skeleton = skeleton_mut(&ctx, &mut doc_mut(&ctx, &mut state, d)?.doc, id)?;
        if bone >= skeleton.bones.len() {
            return Err(throw(&ctx, "that bone isn't in the skeleton"));
        }
        if let Some(turn) = spec["turn"].as_f64() {
            skeleton.bones[bone].turn.set(frame, turn as f32);
        }
        if !spec["moveTo"].is_null() {
            let to = point(&ctx, &spec["moveTo"])?;
            skeleton.move_bone(bone, to, frame);
        }
        if let Some(stretch) = spec["stretch"].as_f64() {
            let b = &mut skeleton.bones[bone];
            match &mut b.stretch {
                Some(channel) => channel.set(frame, stretch as f32),
                None => b.stretch = Some(inkframe_core::Channel::new(frame, stretch as f32)),
            }
        }
        Ok(())
    });

    let s = Rc::clone(state);
    // How a layer in a skeleton's folder follows the bones: "nearby",
    // { bone }, { limb } (bones by place) or null.
    native!("follows", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        if doc.skeleton_layer_of(layer).is_none() {
            return Ok(Value::Null.to_string());
        }
        Ok(match layer.bone_binding {
            BoneBinding::Region => json!("nearby"),
            BoneBinding::Bone(b) => json!({ "bone": b }),
            BoneBinding::Limb(b) => json!({ "limb": b }),
            BoneBinding::Unbound => json!("none"),
        }
        .to_string())
    });

    let s = Rc::clone(state);
    native!("setFollows", move |ctx: Ctx<'js>, d: usize, id: f64, value: String| -> rquickjs::Result<()> {
        let value = parse(&ctx, &value)?;
        let mut state = s.borrow_mut();
        let doc = &mut doc_mut(&ctx, &mut state, d)?.doc;
        let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let count = doc
            .skeleton_layer_of(layer)
            .and_then(|o| o.skeleton.as_ref())
            .map(|s| s.bones.len())
            .ok_or_else(|| throw(&ctx, "the layer isn't in a skeleton's folder"))?;
        let bone = |v: &Value| {
            v.as_u64()
                .map(|b| b as usize)
                .filter(|&b| b < count)
                .ok_or_else(|| throw(&ctx, "that bone isn't in the skeleton"))
        };
        let binding = match &value {
            Value::String(s) if s == "nearby" => BoneBinding::Region,
            Value::String(s) if s == "none" => BoneBinding::Unbound,
            Value::Null => BoneBinding::Unbound,
            v if !v["bone"].is_null() => BoneBinding::Bone(bone(&v["bone"])?),
            v if !v["limb"].is_null() => BoneBinding::Limb(bone(&v["limb"])?),
            _ => return Err(throw(&ctx, "follows is \"nearby\", \"none\", a bone, or { limb: bone }")),
        };
        doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?.bone_binding = binding;
        Ok(())
    });

    // ------------------------------------------------------------- drawings

    let s = Rc::clone(state);
    native!("drawings", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let layer = doc(&ctx, &state, d)?.doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        Ok(json!(layer.drawings.iter().map(|d| d.name.clone()).collect::<Vec<_>>()).to_string())
    });

    let s = Rc::clone(state);
    // A new drawing called `name`: blank, or a copy of drawing `copy` (by
    // name). Returns its place.
    native!("addDrawing", move |ctx: Ctx<'js>,
                                d: usize,
                                id: f64,
                                name: String,
                                copy: Option<String>|
          -> rquickjs::Result<usize> {
        let mut state = s.borrow_mut();
        let layer = doc_mut(&ctx, &mut state, d)?.doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        if layer.drawing_named(&name).is_some() {
            return Err(throw(&ctx, format!("the layer already has a drawing called {name}")));
        }
        let mut drawing = match &copy {
            Some(from) => {
                let i = layer
                    .drawing_named(from)
                    .ok_or_else(|| throw(&ctx, format!("there's no drawing called {from}")))?;
                layer.drawings[i].clone()
            }
            None => inkframe_core::Drawing::new(&name, Vec::new()),
        };
        drawing.name = name;
        Ok(layer.add_drawing(drawing))
    });

    let s = Rc::clone(state);
    // Shows drawing `name` from `frame` on.
    native!("showDrawing", move |ctx: Ctx<'js>,
                                 d: usize,
                                 id: f64,
                                 name: String,
                                 frame: f64|
          -> rquickjs::Result<()> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let mut state = s.borrow_mut();
        let layer = doc_mut(&ctx, &mut state, d)?.doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let index = layer
            .drawing_named(&name)
            .ok_or_else(|| throw(&ctx, format!("there's no drawing called {name}")))?;
        layer.expose(frame, index);
        Ok(())
    });

    // ------------------------------------------------------- sound and lips

    let s = Rc::clone(state);
    // Brings a sound file in, starting on `frame`. Only sound files can be
    // named. Returns the sound's name.
    native!("importAudio", move |ctx: Ctx<'js>,
                                 d: usize,
                                 path: String,
                                 frame: f64|
          -> rquickjs::Result<String> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let mut state = s.borrow_mut();
        let base = state.base_dir.clone();
        // The kind of file first, before where it is: anything but a sound
        // is refused however its path is written.
        let is_sound = Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()));
        if !is_sound {
            return Err(throw(&ctx, format!("{path} isn't a sound file ({})", AUDIO_EXTENSIONS.join(", "))));
        }
        let file = crate::files::resolve(&path, base.as_deref()).map_err(|e| throw(&ctx, e))?;
        if !file.is_file() {
            return Err(throw(&ctx, format!("there's no file at {}", file.display())));
        }
        let open = doc_mut(&ctx, &mut state, d)?;
        let project_dir = open.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let audio = open.doc.add_audio(&file, project_dir.as_deref(), frame);
        Ok(open.doc.audio.iter().find(|a| a.id == audio).map_or_else(String::new, |a| a.name.clone()))
    });

    let s = Rc::clone(state);
    // Lip-syncs a mouth layer to a sound (by name), its drawings matched to
    // the mouth shapes by their names: from a Rhubarb (.tsv) or Papagayo
    // (.dat) timing file if `timing` names one, or else from the sound's
    // loudness. Returns how many mouth changes it made.
    native!("lipSync", move |ctx: Ctx<'js>,
                             d: usize,
                             id: f64,
                             sound: String,
                             options: String|
          -> rquickjs::Result<usize> {
        let options = parse(&ctx, &options)?;
        let mut state = s.borrow_mut();
        let base = state.base_dir.clone();
        let open = doc_mut(&ctx, &mut state, d)?;
        let project_dir = open.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let doc = &mut open.doc;
        let fps = doc.frame_rate.fps();
        let track = doc
            .audio
            .iter()
            .find(|a| a.name == sound)
            .ok_or_else(|| throw(&ctx, format!("there's no sound called {sound}")))?;
        let (start, file) = (track.start, track.resolve(project_dir.as_deref()));
        let decoded = inkframe_audio::decode(&file)
            .map_err(|e| throw(&ctx, format!("couldn't read {}: {e}", file.display())))?;
        let peaks = decoded.peaks_per_frame(fps);
        let cues = if let Some(timing) = options["timing"].as_str() {
            read_timing(&ctx, timing, base.as_deref(), fps)?
        } else if options["rhubarb"].as_bool() == Some(true) {
            listen(&ctx, &file, fps, &options)?
        } else {
            inkframe_core::cues_from_loudness(&peaks)
        };
        let layer = doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        let map = MouthMap::guess(layer);
        let end = start + (peaks.len() as u32).max(1) - 1;
        layer.apply_lip_sync(&cues, &map, start, end);
        Ok(cues.len())
    });

    // ------------------------------------------------------------------ web

    let s = Rc::clone(state);
    native!("button", move |ctx: Ctx<'js>, d: usize, id: f64| -> rquickjs::Result<String> {
        let state = s.borrow();
        let layer = doc(&ctx, &state, d)?.doc.layer(layer_id(id)).ok_or_else(|| gone(&ctx))?;
        Ok(action_value(layer.button.as_ref()).to_string())
    });

    let s = Rc::clone(state);
    native!("setButton", move |ctx: Ctx<'js>, d: usize, id: f64, action: String| -> rquickjs::Result<()> {
        let action = web_action(&ctx, &parse(&ctx, &action)?)?;
        let mut state = s.borrow_mut();
        doc_mut(&ctx, &mut state, d)?.doc.layer_mut(layer_id(id)).ok_or_else(|| gone(&ctx))?.button = action;
        Ok(())
    });

    let s = Rc::clone(state);
    native!("frameActions", move |ctx: Ctx<'js>, d: usize| -> rquickjs::Result<String> {
        let state = s.borrow();
        let doc = &doc(&ctx, &state, d)?.doc;
        let actions: Vec<Value> = (0..doc.duration)
            .filter_map(|f| {
                doc.frame_action(f).map(|a| json!({ "frame": f + 1, "action": action_value(Some(a)) }))
            })
            .collect();
        Ok(Value::Array(actions).to_string())
    });

    let s = Rc::clone(state);
    native!("setFrameAction", move |ctx: Ctx<'js>,
                                    d: usize,
                                    frame: f64,
                                    action: String|
          -> rquickjs::Result<()> {
        let frame = frame_index(&ctx, &json!(frame), "frame")?;
        let action = web_action(&ctx, &parse(&ctx, &action)?)?;
        let mut state = s.borrow_mut();
        doc_mut(&ctx, &mut state, d)?.doc.set_frame_action(frame, action);
        Ok(())
    });

    Ok(())
}

fn skeleton<'a>(ctx: &Ctx<'_>, doc: &'a Document, id: f64) -> rquickjs::Result<&'a Skeleton> {
    let layer = doc.layer(layer_id(id)).ok_or_else(|| gone(ctx))?;
    layer.skeleton.as_ref().ok_or_else(|| throw(ctx, format!("{} isn't a skeleton's folder", layer.name)))
}

fn skeleton_mut<'a>(ctx: &Ctx<'_>, doc: &'a mut Document, id: f64) -> rquickjs::Result<&'a mut Skeleton> {
    let layer = doc.layer_mut(layer_id(id)).ok_or_else(|| gone(ctx))?;
    let name = layer.name.clone();
    layer.skeleton.as_mut().ok_or_else(|| throw(ctx, format!("{name} isn't a skeleton's folder")))
}

/// A web action as scripts see it: "play", "stop", { gotoAndPlay: frame },
/// { gotoAndStop: frame }, { openLink: url }, or null.
fn action_value(action: Option<&WebAction>) -> Value {
    match action {
        None => Value::Null,
        Some(WebAction::Play) => json!("play"),
        Some(WebAction::Stop) => json!("stop"),
        Some(WebAction::GotoAndPlay(f)) => json!({ "gotoAndPlay": f + 1 }),
        Some(WebAction::GotoAndStop(f)) => json!({ "gotoAndStop": f + 1 }),
        Some(WebAction::OpenLink(url)) => json!({ "openLink": url }),
    }
}

fn web_action(ctx: &Ctx<'_>, value: &Value) -> rquickjs::Result<Option<WebAction>> {
    Ok(Some(match value {
        Value::Null => return Ok(None),
        Value::String(s) if s == "play" => WebAction::Play,
        Value::String(s) if s == "stop" => WebAction::Stop,
        v if !v["gotoAndPlay"].is_null() => {
            WebAction::GotoAndPlay(frame_index(ctx, &v["gotoAndPlay"], "gotoAndPlay")?)
        }
        v if !v["gotoAndStop"].is_null() => {
            WebAction::GotoAndStop(frame_index(ctx, &v["gotoAndStop"], "gotoAndStop")?)
        }
        v if v["openLink"].is_string() => {
            WebAction::OpenLink(v["openLink"].as_str().unwrap_or_default().to_owned())
        }
        _ => {
            return Err(throw(
                ctx,
                "an action is \"play\", \"stop\", { gotoAndPlay: frame }, { gotoAndStop: frame }, { openLink: url } or null",
            ));
        }
    }))
}

/// Mouth cues from Rhubarb Lip Sync listening to the sound `file`, as the
/// Lip Sync window has it: `language` "english" (the default, which knows
/// English words) or "other" (sounds only), and the `dialogue` said, which
/// helps with unclear words. Only the Rhubarb that comes with Inkframe is
/// run, on the project's own sound.
fn listen(
    ctx: &Ctx<'_>,
    file: &Path,
    fps: f64,
    options: &Value,
) -> rquickjs::Result<Vec<inkframe_core::MouthCue>> {
    use inkframe_audio::rhubarb::{self, Recognizer};
    let rhubarb = rhubarb::locate()
        .ok_or_else(|| throw(ctx, "Rhubarb Lip Sync isn't included with this copy of Inkframe; use the sound's loudness or a timing file"))?;
    let recognizer = match options["language"].as_str().unwrap_or("english") {
        "english" => Recognizer::English,
        "other" => Recognizer::AnyLanguage,
        other => return Err(throw(ctx, format!("language is \"english\" or \"other\", not {other:?}"))),
    };
    rhubarb::recognize(&rhubarb, file, fps, recognizer, options["dialogue"].as_str())
        .map_err(|e| throw(ctx, e))
}

/// Mouth cues from a timing file a script named: Rhubarb's tab-separated
/// output (.tsv or .txt) or a Papagayo export (.dat).
fn read_timing(
    ctx: &Ctx<'_>,
    path: &str,
    base: Option<&Path>,
    fps: f64,
) -> rquickjs::Result<Vec<inkframe_core::MouthCue>> {
    let file: PathBuf = crate::files::resolve(path, base).map_err(|e| throw(ctx, e))?;
    let extension = file.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    if !["tsv", "txt", "dat"].contains(&extension.as_str()) {
        return Err(throw(
            ctx,
            format!("{} isn't a timing file (.tsv or .txt from Rhubarb, .dat from Papagayo)", file.display()),
        ));
    }
    let text = std::fs::read_to_string(&file)
        .map_err(|e| throw(ctx, format!("couldn't read {}: {e}", file.display())))?;
    inkframe_core::parse_timing(&text, fps).map_err(|e| throw(ctx, format!("{}: {e}", file.display())))
}
