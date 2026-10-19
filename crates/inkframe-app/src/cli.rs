//! `inkframe --export`: batch export from the command line, without
//! opening a window, for scripts and render machines.
//!
//! ```text
//! inkframe --export walk.inkf run.inkf --format h264,prores4444 --to renders
//! ```

use std::ops::ControlFlow;
use std::path::PathBuf;

use inkframe_export::ExportFormat;
use inkframe_export::batch::{self, BatchEvent, BatchJob};

const USAGE: &str = "\
Usage: inkframe --export PROJECT... [options]

Exports each project, as last saved, into one folder.

Options:
  --format LIST   Formats, separated by commas (default: prores4444):
                  png, sheet, prores4444, prores4444xq, prores422hq, h264,
                  hevc-alpha (macOS only)
  --to FOLDER     Where to write the exports (default: the current folder)
  --scale N       Resolution multiplier, e.g. 0.5 for half size (default: 1)
  --opaque        Draw the stage colour instead of leaving it transparent
  --help          Show this message";

/// What to export, as read from the command line.
#[derive(Debug, PartialEq)]
struct Request {
    jobs: Vec<BatchJob>,
    folder: PathBuf,
}

/// Runs the export described by `args` (everything after `--export`) and
/// returns the process's exit code.
pub fn run(args: Vec<String>) -> i32 {
    attach_console();
    let request = match parse(args) {
        Ok(Some(request)) => request,
        Ok(None) => {
            println!("{USAGE}");
            return 0;
        }
        Err(message) => {
            eprintln!("inkframe: {message}\n\n{USAGE}");
            return 2;
        }
    };

    let total = request.jobs.len();
    let outputs = batch::outputs(&request.jobs, &request.folder);
    let succeeded = batch::run(&request.jobs, &request.folder, |event| {
        match event {
            BatchEvent::Started(i) => {
                let job = &request.jobs[i];
                println!(
                    "[{}/{total}] {} as {} → {}",
                    i + 1,
                    job.name(),
                    job.format.label(),
                    outputs[i].display()
                );
            }
            BatchEvent::Progress(..) => {}
            BatchEvent::Finished(_, Ok(_)) => println!("      done"),
            BatchEvent::Finished(_, Err(message)) => eprintln!("      failed: {message}"),
        }
        ControlFlow::Continue(())
    });
    println!("{succeeded} of {total} exported");
    if succeeded == total { 0 } else { 1 }
}

/// Reads the arguments after `--export`; `None` means help was asked for.
fn parse(args: Vec<String>) -> Result<Option<Request>, String> {
    let mut projects = Vec::new();
    let mut formats = vec![ExportFormat::ProRes4444];
    let mut folder = PathBuf::from(".");
    let mut scale = 1.0;
    let mut transparent = true;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--format" => {
                formats = value()?
                    .split(',')
                    .map(|slug| {
                        let format = ExportFormat::from_slug(slug.trim())
                            .ok_or_else(|| format!("unknown format {slug:?}"))?;
                        if format.is_available() {
                            Ok(format)
                        } else {
                            Err(format!("{} isn't available on this platform", format.label()))
                        }
                    })
                    .collect::<Result<_, String>>()?;
            }
            "--to" => folder = PathBuf::from(value()?),
            "--scale" => {
                scale = value()?
                    .parse::<f32>()
                    .ok()
                    .filter(|s| (0.05..=8.0).contains(s))
                    .ok_or("--scale takes a number from 0.05 to 8")?;
            }
            "--opaque" => transparent = false,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            project => projects.push(PathBuf::from(project)),
        }
    }
    if projects.is_empty() {
        return Err("no projects to export".to_owned());
    }
    let jobs = projects
        .iter()
        .flat_map(|project| {
            formats.iter().map(move |&format| BatchJob {
                scale,
                transparent,
                ..BatchJob::new(project, format)
            })
        })
        .collect();
    Ok(Some(Request { jobs, folder }))
}

/// Release builds on Windows are GUI programs with no console of their
/// own, so output would vanish; borrow the console of the shell that
/// started us instead.
fn attach_console() {
    #[cfg(windows)]
    // SAFETY: AttachConsole has no preconditions; failing (when there's no
    // parent console) just leaves output going nowhere, as before.
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn every_project_gets_every_format() {
        let request =
            parse(args("a.inkf b.inkf --format h264,png --to out --scale 0.5 --opaque")).unwrap().unwrap();
        assert_eq!(request.folder, PathBuf::from("out"));
        let jobs: Vec<_> = request.jobs.iter().map(|j| (j.name(), j.format)).collect();
        assert_eq!(
            jobs,
            [
                ("a".to_owned(), ExportFormat::H264),
                ("a".to_owned(), ExportFormat::PngSequence),
                ("b".to_owned(), ExportFormat::H264),
                ("b".to_owned(), ExportFormat::PngSequence),
            ]
        );
        assert!(request.jobs.iter().all(|j| j.scale == 0.5 && !j.transparent));
    }

    #[test]
    fn mistakes_are_explained() {
        assert!(parse(args("")).unwrap_err().contains("no projects"));
        assert!(parse(args("a.inkf --format gif")).unwrap_err().contains("unknown format"));
        assert!(parse(args("a.inkf --to")).unwrap_err().contains("needs a value"));
        assert!(parse(args("a.inkf --scale big")).is_err());
        assert!(parse(args("a.inkf --help")).unwrap().is_none());
    }
}
