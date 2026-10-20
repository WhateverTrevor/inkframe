//! Running JavaScript against an Inkframe document.
//!
//! This is the spike from `docs/SCRIPTING_PLAN.md`: enough to show QuickJS
//! builds and runs on every platform, that a script can read a document
//! and change it, and that errors, runaway loops and roll-back behave. The
//! real API comes next, on top of this.
//!
//! A script runs against a copy of the document. Only if it finishes
//! without an error does the copy replace the original, so a script that
//! fails partway never leaves a project half-changed. QuickJS has no file
//! or network access of its own; a script sees only the `inkframe` object
//! and `console` set up here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use inkframe_core::{Document, Point, Transform};
use rquickjs::context::EvalOptions;
use rquickjs::function::Rest;
use rquickjs::{Coerced, Context, Ctx, Function, Object, Runtime};

/// The most memory a script may use.
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

/// What a script that finished printed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub log: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ScriptError {
    /// The script threw, or had a syntax error.
    #[error("{}{message}", line.map_or_else(String::new, |l| format!("line {l}: ")))]
    Failed { message: String, line: Option<u32>, log: Vec<String> },
    #[error("the script ran for more than {} seconds and was stopped", .0.as_secs())]
    TimedOut(Duration),
    #[error("couldn't start JavaScript: {0}")]
    Engine(String),
}

/// Runs `source` against `doc`, stopping it after `limit`. On success the
/// document holds the script's changes; on any failure it's unchanged.
pub fn run(doc: &mut Document, source: &str, limit: Duration) -> Result<Outcome, ScriptError> {
    let engine = |e: rquickjs::Error| ScriptError::Engine(e.to_string());
    let runtime = Runtime::new().map_err(engine)?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    let started = Instant::now();
    let timed_out = Rc::new(Cell::new(false));
    let flag = Rc::clone(&timed_out);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let over = started.elapsed() > limit;
        flag.set(over);
        over
    })));
    let context = Context::full(&runtime).map_err(engine)?;

    let working = Rc::new(RefCell::new(doc.clone()));
    let log = Rc::new(RefCell::new(Vec::new()));
    let result = context.with(|ctx| {
        install(&ctx, &working, &log).map_err(engine)?;
        let mut options = EvalOptions::default();
        options.filename = Some("script.js".to_owned());
        match ctx.eval_with_options::<(), _>(source, options) {
            Ok(()) => Ok(()),
            Err(rquickjs::Error::Exception) => {
                let thrown = ctx.catch();
                let (message, stack) = match thrown.as_exception() {
                    Some(e) => (e.message().unwrap_or_default(), e.stack().unwrap_or_default()),
                    None => (format!("{thrown:?}"), String::new()),
                };
                Err(ScriptError::Failed { message, line: line_of(&stack), log: log.borrow().clone() })
            }
            Err(other) => Err(engine(other)),
        }
    });
    if timed_out.get() {
        return Err(ScriptError::TimedOut(limit));
    }
    result?;
    *doc = working.borrow().clone();
    let log = log.borrow().clone();
    Ok(Outcome { log })
}

/// The `inkframe` object and `console`.
fn install<'js>(
    ctx: &Ctx<'js>,
    doc: &Rc<RefCell<Document>>,
    log: &Rc<RefCell<Vec<String>>>,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();

    let console = Object::new(ctx.clone())?;
    let lines = Rc::clone(log);
    console.set(
        "log",
        Function::new(ctx.clone(), move |parts: Rest<Coerced<String>>| {
            let line: Vec<String> = parts.0.into_iter().map(|p| p.0).collect();
            lines.borrow_mut().push(line.join(" "));
        })?,
    )?;
    globals.set("console", console)?;

    let inkframe = Object::new(ctx.clone())?;
    {
        let d = doc.borrow();
        inkframe.set("width", d.width)?;
        inkframe.set("height", d.height)?;
        inkframe.set("duration", d.duration)?;
        inkframe.set("layerNames", d.layers.iter().rev().map(|l| l.name.clone()).collect::<Vec<_>>())?;
    }
    let target = Rc::clone(doc);
    // addLayer(name, frame, x, y): a new empty layer, its position keyed
    // on `frame` (counted from 1, as the timeline shows).
    inkframe.set(
        "addLayer",
        Function::new(ctx.clone(), move |name: String, frame: u32, x: f32, y: f32| {
            let mut d = target.borrow_mut();
            let frame = frame.saturating_sub(1);
            d.add_layer(name, Vec::new(), frame, Transform::at(Point::new(x, y)));
        })?,
    )?;
    globals.set("inkframe", inkframe)?;
    Ok(())
}

/// The line in `script.js` an error's stack points to.
fn line_of(stack: &str) -> Option<u32> {
    let at = stack.find("script.js:")? + "script.js:".len();
    stack[at..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn a_script_reads_the_document_and_logs() {
        let mut doc = Document::default();
        let outcome =
            run(&mut doc, "console.log(`${inkframe.width}x${inkframe.height}`, 'at', 24, 'fps');", SECOND);
        assert_eq!(outcome.unwrap().log, ["1920x1080 at 24 fps"]);
    }

    #[test]
    fn a_script_changes_the_document() {
        let mut doc = Document::default();
        let script = "for (let i = 1; i <= 3; i++) inkframe.addLayer(`Dot ${i}`, 1, i * 100, 50);";
        run(&mut doc, script, SECOND).unwrap();
        let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Dot 1", "Dot 2", "Dot 3"]);
        assert_eq!(doc.layers[2].transform_at(0).position, Point::new(300.0, 50.0));
    }

    #[test]
    fn a_failing_script_changes_nothing_and_says_where() {
        let mut doc = Document::default();
        let script = "inkframe.addLayer('Kept?', 1, 0, 0);\nconsole.log('before');\nnotAFunction();";
        let error = run(&mut doc, script, SECOND).unwrap_err();
        let ScriptError::Failed { line, log, message } = &error else { panic!("{error:?}") };
        assert_eq!(*line, Some(3), "{message}");
        assert_eq!(log, &["before"]);
        assert!(doc.layers.is_empty(), "rolled back");
        assert!(error.to_string().starts_with("line 3: "), "{error}");
    }

    #[test]
    fn syntax_errors_are_reported_too() {
        let error = run(&mut Document::default(), "let x = ;", SECOND).unwrap_err();
        assert!(matches!(error, ScriptError::Failed { line: Some(1), .. }), "{error:?}");
    }

    #[test]
    fn a_runaway_script_is_stopped() {
        let mut doc = Document::default();
        let started = Instant::now();
        let error =
            run(&mut doc, "inkframe.addLayer('x', 1, 0, 0); while (true) {}", Duration::from_millis(200));
        assert_eq!(error.unwrap_err(), ScriptError::TimedOut(Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(doc.layers.is_empty());
    }

    #[test]
    fn scripts_have_no_file_access() {
        // QuickJS's own std and os modules aren't loaded, so neither exists.
        let outcome = run(
            &mut Document::default(),
            "console.log(typeof std, typeof os, typeof require, typeof fetch)",
            SECOND,
        );
        assert_eq!(outcome.unwrap().log, ["undefined undefined undefined undefined"]);
    }
}
