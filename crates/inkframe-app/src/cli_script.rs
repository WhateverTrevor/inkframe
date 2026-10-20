//! `inkframe --script`: runs a script without opening a window, for
//! pipelines.
//!
//! ```text
//! inkframe --script stagger.js walk.inkf run.inkf --save
//! inkframe --script build-scene.js
//! ```
//!
//! With projects, the script runs once on each, and `--save` saves each
//! one it changed. With none, it runs once on a new, empty project, which
//! it can save itself. Relative paths in the script start from the
//! project's folder, or the current folder for a new project.

use std::path::PathBuf;

use inkframe_core::Document;
use inkframe_script::{Script, ScriptError};

const USAGE: &str = "\
Usage: inkframe --script SCRIPT.js [PROJECT...] [--save]

Runs a script on each project (or once on a new, empty project) without
opening a window. What it prints with console.log is printed here.

Options:
  --save   Save each project the script changed
  --help   Show this message

The scripting guide: https://github.com/WhateverTrevor/inkframe/blob/main/docs/SCRIPTING.md";

/// Runs the request in `args` (everything after `--script`) and returns
/// the process's exit code.
pub fn run(args: Vec<String>) -> i32 {
    crate::cli::attach_console();
    let mut save = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return 0;
            }
            "--save" => save = true,
            flag if flag.starts_with("--") => {
                eprintln!("inkframe: unknown option {flag}\n\n{USAGE}");
                return 2;
            }
            path => paths.push(PathBuf::from(path)),
        }
    }
    let Some((script_path, projects)) = paths.split_first() else {
        eprintln!("inkframe: no script to run\n\n{USAGE}");
        return 2;
    };
    let source = match std::fs::read_to_string(script_path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("inkframe: couldn't read {}: {err}", script_path.display());
            return 1;
        }
    };
    let here = std::env::current_dir().ok();

    if projects.is_empty() {
        let script = Script { base_dir: here, ..Script::new(source, Document::default()) };
        return report(None, inkframe_script::run(script).map(|outcome| print_log(&outcome.log)));
    }
    let mut failed = 0;
    for (i, project) in projects.iter().enumerate() {
        println!("[{}/{}] {}", i + 1, projects.len(), project.display());
        let doc = match Document::load(project) {
            Ok(doc) => doc,
            Err(err) => {
                eprintln!("      couldn't open it: {err}");
                failed += 1;
                continue;
            }
        };
        let script = Script {
            path: Some(project.clone()),
            base_dir: here.clone(),
            ..Script::new(source.clone(), doc.clone())
        };
        let result = inkframe_script::run(script).and_then(|outcome| {
            print_log(&outcome.log);
            if save && outcome.document != doc {
                let to = outcome.path.clone().unwrap_or_else(|| project.clone());
                outcome
                    .document
                    .save(&to)
                    .map_err(|e| ScriptError::Engine(format!("couldn't save {}: {e}", to.display())))?;
                println!("      saved");
            }
            Ok(())
        });
        if report(Some(project), result) != 0 {
            failed += 1;
        }
    }
    println!("{} of {} done", projects.len() - failed, projects.len());
    i32::from(failed > 0)
}

fn print_log(log: &[String]) {
    for line in log {
        println!("      {line}");
    }
}

/// Prints how a run went; returns its exit code.
fn report(project: Option<&PathBuf>, result: Result<(), ScriptError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            print_log(error.log());
            let on = project.map_or_else(String::new, |p| format!(" on {}", p.display()));
            eprintln!("      failed{on}: {error}");
            1
        }
    }
}
