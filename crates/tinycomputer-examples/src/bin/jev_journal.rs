//! Reads the Jev debug journal: lists journaled runs, summarises where one
//! run's time went, or prints what Jev was asked and answered.
//!
//! Turn the journal on for any run with `TINYCOMPUTER_JEV_JOURNAL=1`, then:
//!
//! ```sh
//! cargo run -p tinycomputer-examples --bin jev_journal                  # list runs
//! cargo run -p tinycomputer-examples --bin jev_journal -- latest        # summary
//! cargo run -p tinycomputer-examples --bin jev_journal -- <id> --json   # summary as JSON
//! cargo run -p tinycomputer-examples --bin jev_journal -- <id> --transcript
//! cargo run -p tinycomputer-examples --bin jev_journal -- <id> --calibration
//! cargo run -p tinycomputer-examples --bin jev_journal -- --split <task>...
//! cargo run -p tinycomputer-examples --bin jev_journal -- --compare <task>... --vs <task>...
//! ```
//!
//! `<id>` is `latest`, any unique part of a run id, or a run directory. A
//! `<task>` is a run directory, or a folder of runs read as one task (what
//! `task_live` writes under `TASK_OUT/journal`); `--split` with none splits
//! the latest run. See `docs/technical/jev-journal.md`.

use std::process::ExitCode;

use tinycomputer_examples::journal;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    let view = if args.iter().any(|arg| arg == "--transcript") {
        View::Transcript
    } else if args.iter().any(|arg| arg == "--calibration") {
        View::Calibration
    } else if json {
        View::Json
    } else {
        View::Summary
    };
    let name = args.iter().find(|arg| !arg.starts_with("--"));
    let root = journal::root();
    let outcome = if args.iter().any(|arg| arg == "--compare") {
        compare(&args)
    } else if args.iter().any(|arg| arg == "--split") {
        split(&root, &args, json)
    } else {
        match name {
            None => list(&root),
            Some(name) => show(&root, name, view, json),
        }
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("jev_journal: {error}");
            ExitCode::FAILURE
        }
    }
}

fn list(root: &std::path::Path) -> std::io::Result<()> {
    let runs = journal::runs(root).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("cannot read {}: {error}", root.display()),
        )
    })?;
    for run in runs {
        let summary = journal::summarize(&journal::events(&run)?);
        println!(
            "{:<40} {:>7.1}s {:>4} calls  {}",
            run.file_name().unwrap_or_default().to_string_lossy(),
            std::time::Duration::from_millis(summary.wall_ms).as_secs_f64(),
            summary.calls,
            summary.runs.join(" | ")
        );
    }
    Ok(())
}

/// Where each task named in `args` spent its time: in full for one, as a
/// table with medians for several.
fn split(root: &std::path::Path, args: &[String], json: bool) -> std::io::Result<()> {
    let mut named = args
        .iter()
        .filter(|arg| !arg.starts_with("--"))
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    if named.is_empty() {
        named.push(journal::find(root, "latest")?);
    }
    let splits = named
        .iter()
        .map(|path| {
            Ok((
                path.display().to_string(),
                journal::split(&journal::story(path)?),
            ))
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    if json {
        let all = splits
            .iter()
            .map(|(_, split)| split.clone())
            .collect::<Vec<_>>();
        let report = serde_json::json!({
            "tasks": splits.iter().map(|(name, split)| serde_json::json!({"task": name, "split": split})).collect::<Vec<_>>(),
            "median": journal::median(&all),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(std::io::Error::other)?
        );
    } else if let [(name, split)] = splits.as_slice() {
        println!("{name}");
        print!("{}", journal::render_split(split));
    } else {
        print!("{}", journal::render_table(&splits));
    }
    Ok(())
}

/// The medians of the tasks before `--vs` against those after it.
fn compare(args: &[String]) -> std::io::Result<()> {
    let tasks = args
        .iter()
        .filter(|arg| !arg.starts_with("--") || *arg == "--vs")
        .collect::<Vec<_>>();
    let Some(cut) = tasks.iter().position(|arg| *arg == "--vs") else {
        return Err(std::io::Error::other(
            "--compare needs two sets of tasks: <task>... --vs <task>...",
        ));
    };
    let load = |paths: &[&String]| {
        paths
            .iter()
            .map(|path| Ok(journal::split(&journal::story(std::path::Path::new(path))?)))
            .collect::<std::io::Result<Vec<_>>>()
    };
    let (before, after) = (load(&tasks[..cut])?, load(&tasks[cut + 1..])?);
    if before.is_empty() || after.is_empty() {
        return Err(std::io::Error::other(
            "each side of --vs needs at least one task",
        ));
    }
    print!("{}", journal::render_compare(&before, &after));
    Ok(())
}

/// What `show` prints about one run.
#[derive(Clone, Copy)]
enum View {
    Summary,
    Json,
    Transcript,
    Calibration,
}

fn show(root: &std::path::Path, name: &str, view: View, json: bool) -> std::io::Result<()> {
    let dir = journal::find(root, name)?;
    let events = journal::events(&dir)?;
    match view {
        View::Transcript => print!("{}", journal::transcript(&events)),
        View::Calibration if json => println!(
            "{}",
            serde_json::to_string_pretty(&journal::calibration(&events))
                .map_err(std::io::Error::other)?
        ),
        View::Calibration => print!(
            "{}",
            journal::render_calibration(&journal::calibration(&events))
        ),
        View::Json => println!(
            "{}",
            serde_json::to_string_pretty(&journal::summarize(&events))
                .map_err(std::io::Error::other)?
        ),
        View::Summary => {
            println!("{}", dir.display());
            print!("{}", journal::render(&journal::summarize(&events)));
        }
    }
    Ok(())
}
