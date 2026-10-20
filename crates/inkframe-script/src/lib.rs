//! Running JavaScript against an Inkframe document (see
//! `docs/SCRIPTING.md` for the API, and `docs/SCRIPTING_PLAN.md` for why
//! it's shaped this way).
//!
//! A script runs against a copy of the document. Only if it finishes
//! without an error does the caller get the copy back to use, so a script
//! that fails partway never leaves a project half-changed. QuickJS has no
//! file or network access of its own; a script sees only `inkframe` and
//! `console` (`prelude.js`, over the functions in `native.rs`), and can
//! only touch the files `files.rs` allows.

mod files;
mod native;
mod values;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use inkframe_core::{Document, LayerId};
use rquickjs::context::EvalOptions;
use rquickjs::{Context, Runtime};

use crate::native::{Open, State};

/// The most memory a script may use.
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;
const PRELUDE: &str = include_str!("prelude.js");

/// What to run, and on what.
pub struct Script {
    pub source: String,
    /// The project it runs on, and its file if it has one.
    pub document: Document,
    pub path: Option<PathBuf>,
    /// Where relative paths start from when the project has no file (the
    /// command line's current folder); otherwise its folder is used.
    pub base_dir: Option<PathBuf>,
    pub selection: Vec<LayerId>,
    /// Setting this stops the script (a Stop button).
    pub stop: Arc<AtomicBool>,
    /// Stops the script after this long, if set.
    pub limit: Option<Duration>,
}

impl Script {
    pub fn new(source: impl Into<String>, document: Document) -> Self {
        Self {
            source: source.into(),
            document,
            path: None,
            base_dir: None,
            selection: Vec::new(),
            stop: Arc::new(AtomicBool::new(false)),
            limit: None,
        }
    }
}

/// A script that finished.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The project with the script's changes.
    pub document: Document,
    /// Its file, which `save(path)` may have changed.
    pub path: Option<PathBuf>,
    /// The layers to select, if the script chose some.
    pub selection: Option<Vec<LayerId>>,
    pub log: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ScriptError {
    /// The script threw, or had a syntax error.
    #[error("{}{message}", line.map_or_else(String::new, |l| format!("line {l}: ")))]
    Failed { message: String, line: Option<u32>, log: Vec<String> },
    #[error("the script was stopped")]
    Stopped { log: Vec<String> },
    #[error("the script ran for more than {} seconds and was stopped", .limit.as_secs())]
    TimedOut { limit: Duration, log: Vec<String> },
    #[error("couldn't start JavaScript: {0}")]
    Engine(String),
}

impl ScriptError {
    /// What the script printed before it went wrong.
    pub fn log(&self) -> &[String] {
        match self {
            ScriptError::Failed { log, .. }
            | ScriptError::Stopped { log }
            | ScriptError::TimedOut { log, .. } => log,
            ScriptError::Engine(_) => &[],
        }
    }
}

/// Runs a script to the end. This blocks, so the app runs it on a thread.
pub fn run(script: Script) -> Result<Outcome, ScriptError> {
    let engine = |e: rquickjs::Error| ScriptError::Engine(e.to_string());
    let runtime = Runtime::new().map_err(engine)?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    let started = Instant::now();
    let (limit, stop) = (script.limit, Arc::clone(&script.stop));
    let timed_out = Rc::new(Cell::new(false));
    let flag = Rc::clone(&timed_out);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let over = limit.is_some_and(|limit| started.elapsed() > limit);
        flag.set(over);
        over || stop.load(Ordering::Relaxed)
    })));
    let context = Context::full(&runtime).map_err(engine)?;

    let base_dir = script.path.as_deref().and_then(|p| p.parent()).map(Into::into).or(script.base_dir);
    let state = Rc::new(RefCell::new(State {
        docs: vec![Open { doc: script.document, path: script.path }],
        base_dir,
        selection: script.selection,
        selection_changed: false,
        log: Vec::new(),
    }));
    let result = context.with(|ctx| {
        native::install(&ctx, &state).map_err(engine)?;
        let eval = |source: &str, file: &str| {
            let mut options = EvalOptions::default();
            options.filename = Some(file.to_owned());
            options.strict = false;
            ctx.eval_with_options::<(), _>(source, options)
        };
        eval(PRELUDE, "prelude.js").map_err(|e| ScriptError::Engine(format!("the prelude failed: {e}")))?;
        match eval(&script.source, "script.js") {
            Ok(()) => Ok(()),
            Err(rquickjs::Error::Exception) => {
                let thrown = ctx.catch();
                let (message, stack) = match thrown.as_exception() {
                    Some(e) => (e.message().unwrap_or_default(), e.stack().unwrap_or_default()),
                    None => (describe(&thrown), String::new()),
                };
                Err(ScriptError::Failed { message, line: line_of(&stack), log: state.borrow().log.clone() })
            }
            Err(other) => Err(engine(other)),
        }
    });
    let log = state.borrow().log.clone();
    if timed_out.get() {
        return Err(ScriptError::TimedOut { limit: limit.unwrap_or_default(), log });
    }
    if script.stop.load(Ordering::Relaxed) {
        return Err(ScriptError::Stopped { log });
    }
    result?;
    // The engine's functions still hold the state, so the results are
    // taken out of it rather than unwrapped.
    let mut state = state.borrow_mut();
    let first = std::mem::take(&mut state.docs).swap_remove(0);
    let selection = state.selection_changed.then(|| std::mem::take(&mut state.selection));
    Ok(Outcome { document: first.doc, path: first.path, selection, log })
}

/// A thrown value that isn't an Error object, as text.
fn describe(value: &rquickjs::Value<'_>) -> String {
    value
        .as_string()
        .and_then(|s| s.to_string().ok())
        .or_else(|| value.as_number().map(|n| n.to_string()))
        .unwrap_or_else(|| "the script threw something that isn't an error".to_owned())
}

/// The line in `script.js` an error's stack points to.
fn line_of(stack: &str) -> Option<u32> {
    let at = stack.find("script.js:")? + "script.js:".len();
    stack[at..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkframe_core::{Easing, Point};

    /// Runs `source` on `doc`, stopping it after a few seconds.
    fn run_on(doc: Document, source: &str) -> Result<Outcome, ScriptError> {
        run(Script { limit: Some(Duration::from_secs(5)), ..Script::new(source, doc) })
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("inkframe-script-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_script_reads_the_document_and_logs() {
        let out = run_on(
            Document::default(),
            "const d = inkframe.document; console.log(d.width, 'x', d.height, d.frameRate);",
        );
        assert_eq!(out.unwrap().log, ["1920 x 1080 24"]);
    }

    #[test]
    fn a_script_builds_an_animated_layer() {
        let script = r##"
            const doc = inkframe.document;
            const ball = doc.addLayer("Ball", { ellipse: [80, 80], fill: "#f63", at: [960, 200] });
            ball.setKey(1, { position: [960, 200] }, { easing: "ease-in" });
            ball.setKey(12, { position: [960, 880], scale: [1.2, 0.8] }, { easing: "ease-out" });
            ball.setKey(24, { position: [960, 200], scale: [1, 1] });
            console.log(ball.valueAt("position", 12), ball.keys("position").length);
        "##;
        let out = run_on(Document::default(), script).unwrap();
        assert_eq!(out.log, ["[960,880] 3"]);
        let ball = &out.document.layers[0];
        assert_eq!(ball.name, "Ball");
        let keys = ball.track.position.keys();
        assert_eq!(
            (keys[0].frame, keys[0].easing),
            (0, Easing::EaseIn),
            "easing set before the next key holds"
        );
        assert_eq!((keys[1].frame, keys[1].easing), (11, Easing::EaseOut));
        assert_eq!(ball.track.sample(11.0).scale, Point::new(1.2, 0.8));
        assert_eq!(
            ball.drawings[0].elements[0].style.fill,
            Some(inkframe_core::Color::rgb(0xff, 0x66, 0x33))
        );
    }

    #[test]
    fn layers_are_listed_top_first_and_can_be_changed() {
        let script = r##"
            const doc = inkframe.document;
            doc.addLayer("Back"); doc.addLayer("Middle"); doc.addLayer("Front");
            console.log(doc.layers.map(l => l.name).join(","));
            const middle = doc.layer("Middle");
            middle.name = "Renamed"; middle.visible = false; middle.start = 5; middle.moveUp();
            doc.layer("Back").shiftTime(3);
            doc.layer("Front").remove();
            console.log(doc.layers.map(l => `${l.name}:${l.visible}:${l.start}`).join(","));
        "##;
        let out = run_on(Document::default(), script).unwrap();
        assert_eq!(out.log, ["Front,Middle,Back", "Renamed:false:5,Back:true:4"]);
    }

    #[test]
    fn shifting_keys_moves_the_animation_and_not_the_layer() {
        let script = r##"
            const layer = inkframe.document.addLayer("A");
            layer.setKey(1, { rotation: 0 }).setKey(10, { rotation: 90 });
            layer.shiftKeys(4);
            console.log(layer.start, layer.keys("rotation").map(k => k.frame));
        "##;
        assert_eq!(run_on(Document::default(), script).unwrap().log, ["1 [5,14]"]);
    }

    #[test]
    fn mistakes_explain_themselves() {
        let error = |source: &str| run_on(Document::default(), source).unwrap_err().to_string();
        assert!(
            error("inkframe.document.addLayer('A').setKey(1, { colour: 1 })")
                .contains("no property called colour")
        );
        assert!(error("inkframe.document.addLayer('A').setKey(0, { rotation: 1 })").contains("from 1"));
        assert!(
            error("inkframe.document.addLayer('A').setKey(1, { rotation: 1 }, { easing: 'bouncy' })")
                .contains("isn't an easing")
        );
        assert!(error("inkframe.document.background = 'red'").contains("isn't a colour"));
    }

    #[test]
    fn a_failing_script_reports_where_and_its_changes_are_not_kept() {
        let script = "inkframe.document.addLayer('Kept?');\nconsole.log('before');\nnotAFunction();";
        let error = run_on(Document::default(), script).unwrap_err();
        assert!(matches!(&error, ScriptError::Failed { line: Some(3), .. }), "{error:?}");
        assert_eq!(error.log(), ["before"]);
        assert!(error.to_string().starts_with("line 3: "), "{error}");
        assert!(matches!(
            run_on(Document::default(), "let x = ;"),
            Err(ScriptError::Failed { line: Some(1), .. })
        ));
    }

    #[test]
    fn a_running_script_can_be_stopped_or_times_out() {
        let stop = Arc::new(AtomicBool::new(true));
        let stopped = run(Script { stop, ..Script::new("while (true) {}", Document::default()) });
        assert!(matches!(stopped, Err(ScriptError::Stopped { .. })));
        let slow = run(Script {
            limit: Some(Duration::from_millis(100)),
            ..Script::new("while (true) {}", Document::default())
        });
        assert!(matches!(slow, Err(ScriptError::TimedOut { .. })));
    }

    #[test]
    fn scripts_reach_no_files_or_network_but_their_own() {
        let out =
            run_on(Document::default(), "console.log(typeof std, typeof os, typeof require, typeof fetch)");
        assert_eq!(out.unwrap().log, ["undefined undefined undefined undefined"]);
        // Only projects and exports, and only at paths that make sense.
        let dir = scratch("paths");
        let doc = Script {
            base_dir: Some(dir.clone()),
            ..Script::new("inkframe.document.save('notes.txt')", Document::default())
        };
        assert!(run(doc).unwrap_err().to_string().contains("isn't an Inkframe project"));
        let export = Script {
            base_dir: Some(dir.clone()),
            ..Script::new("inkframe.document.export({ format: 'png', to: 'shot.mp4' })", Document::default())
        };
        assert!(run(export).unwrap_err().to_string().contains("needs to end in .png"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_script_saves_opens_and_exports() {
        let dir = scratch("files");
        let script = r##"
            const doc = inkframe.document;
            doc.duration = 3;
            doc.addLayer("Box", { rect: [100, 100], fill: "#000" });
            doc.save("box.inkf");
            const copy = inkframe.open("box.inkf");
            copy.layer("Box").name = "Copy";
            copy.save("copy.inkf");
            console.log(copy.layers[0].name, doc.export({ format: "png", to: "frames/box.png", scale: 0.1 }).endsWith("box.png"));
        "##;
        let out =
            run(Script { base_dir: Some(dir.clone()), ..Script::new(script, Document::default()) }).unwrap();
        assert_eq!(out.log, ["Copy true"]);
        assert_eq!(out.path, Some(dir.join("box.inkf")), "saving gave the project a file");
        assert_eq!(Document::load(dir.join("copy.inkf")).unwrap().layers[0].name, "Copy");
        assert!(dir.join("frames/box_0001.png").is_file() && dir.join("frames/box_0003.png").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_script_reads_and_sets_the_selection() {
        let mut doc = Document::default();
        let a = doc.add_layer("A", vec![], 0, inkframe_core::Transform::default());
        let b = doc.add_layer("B", vec![], 0, inkframe_core::Transform::default());
        let script = "const doc = inkframe.document; console.log(doc.selectedLayers.map(l => l.name)); doc.select([doc.layer('B')]);";
        let out = run(Script { selection: vec![a], ..Script::new(script, doc) }).unwrap();
        assert_eq!(out.log, [r#"["A"]"#]);
        assert_eq!(out.selection, Some(vec![b]));
    }

    #[test]
    fn swatches_can_be_read_added_and_recoloured() {
        let script = r##"
            const doc = inkframe.document;
            doc.addSwatch("Skin", "#f0c8a0");
            doc.setSwatch(0, "#e0b890");
            console.log(doc.swatches[0].name, doc.swatches[0].color);
        "##;
        assert_eq!(run_on(Document::default(), script).unwrap().log, ["Skin #e0b890ff"]);
    }
}

#[cfg(test)]
mod round_two {
    use super::*;

    fn run_on(source: &str) -> Outcome {
        run(Script { limit: Some(Duration::from_secs(5)), ..Script::new(source, Document::default()) })
            .unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn symbols_can_be_made_and_placed() {
        let out = run_on(
            r##"
            const doc = inkframe.document;
            const star = doc.addLayer("Star", { polygon: [[0, -50], [15, -15], [50, 0], [15, 15], [0, 50], [-15, 15], [-50, 0], [-15, -15]], fill: "#fc0" });
            const name = star.convertToSymbol();
            const copy = doc.addInstance(name, { at: [200, 200] });
            copy.playback = "single-frame";
            console.log(name, doc.symbols.length, doc.symbols[0].uses, copy.symbol, copy.playback, star.symbol);
        "##,
        );
        assert_eq!(out.log, ["Symbol 1 1 2 Symbol 1 single-frame Symbol 1"]);
    }

    #[test]
    fn effects_read_and_key_in_javascript_style() {
        let out = run_on(
            r##"
            const layer = inkframe.document.addLayer("Glow", { rect: [10, 10] });
            layer.setEffects(1, { color: { type: "tint", color: "#ff0000ff", amount: 0.5 },
                                  filters: [{ type: "dropShadow", distance: 8, angle: 45, blur: 4, color: "#000000ff", strength: 1 }] });
            const fx = layer.effectsAt(1);
            console.log(fx.color.type, fx.filters[0].type, fx.filters[0].distance);
        "##,
        );
        assert_eq!(out.log, ["tint dropShadow 8"]);
    }

    #[test]
    fn particles_and_the_camera() {
        let out = run_on(
            r##"
            const doc = inkframe.document;
            const snow = doc.addParticleLayer("snow");
            snow.particles = { rate: 99, sizeStart: 4 };
            const camera = doc.addCamera();
            camera.setKey(1, { depth: 0 }).setKey(24, { depth: 400 });
            doc.stackByDepth = true;
            console.log(snow.kind, snow.particles.rate, snow.particles.sizeStart, snow.particles.speed, doc.camera.kind, doc.stackByDepth);
        "##,
        );
        assert_eq!(out.log, ["particles 99 4 70 camera true"]);
    }

    #[test]
    fn rigs_have_pivots_and_children() {
        let out = run_on(
            r##"
            const doc = inkframe.document;
            const arm = doc.addLayer("Arm"), hand = doc.addLayer("Hand");
            hand.parent = arm;
            arm.pivot = [0, -40];
            console.log(arm.children.map((l) => l.name), arm.pivot);
        "##,
        );
        assert_eq!(out.log, [r#"["Hand"] [0,-40]"#]);
    }
}
