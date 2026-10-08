//! What a run keeps beside its report once the task stops: the plan the
//! task drafted for itself (`TASK_PLAN=in-task`), and the elements it
//! learned (`TASK_MEMORY`).

use std::path::Path;

use serde_json::Value;
use tinycomputer_bus::GroundingHint;
use tinycomputer_examples::host::LabError;

/// Writes the flow a task planned for itself (`TASK_PLAN=in-task`) to
/// `plan.json` beside its report, as a plan drafted first would be.
pub(super) fn record_plan(out: &Path) -> Result<(), LabError> {
    let report: Value = serde_json::from_str(&std::fs::read_to_string(out.join("report.json"))?)?;
    if let Some(flow) = report.get("flow").filter(|flow| !flow.is_null()) {
        let text = serde_json::to_string_pretty(flow)?;
        println!("plan (drafted inside the task):\n{text}");
        std::fs::write(out.join("plan.json"), text)?;
    }
    Ok(())
}

/// The grounding hints saved at `path`: none when it does not exist yet.
pub(super) fn read_memory(path: &Path) -> Result<Vec<GroundingHint>, LabError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

/// `kept` with what a run `learned`: a hint for the same element (the same
/// application and key) gives way to the newer one, and the rest keep their
/// order.
pub(super) fn merge_memory(
    kept: Vec<GroundingHint>,
    learned: Vec<GroundingHint>,
) -> Vec<GroundingHint> {
    let mut merged = kept
        .into_iter()
        .filter(|hint| {
            !learned
                .iter()
                .any(|new| new.app == hint.app && new.key == hint.key)
        })
        .collect::<Vec<_>>();
    merged.extend(learned);
    merged
}

/// Saves what the task's report learned into the memory at `path`, beside
/// what it already held; the memory's folder is made when missing.
pub(super) fn remember(out: &Path, path: &Path) -> Result<(), LabError> {
    let report: Value = serde_json::from_str(&std::fs::read_to_string(out.join("report.json"))?)?;
    let learned: Vec<GroundingHint> = match report.get("learned") {
        Some(learned) => serde_json::from_value(learned.clone())?,
        None => Vec::new(),
    };
    let merged = merge_memory(read_memory(path)?, learned);
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&merged)?)?;
    Ok(())
}

#[cfg(test)]
mod saved_tests;
