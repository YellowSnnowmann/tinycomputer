//! Tests for the `task_live` binary: which routes Jev and the planner take.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_examples::host::LabError;

use super::{TINY_HUMANS_MODEL, merge_memory, read_memory, remember, routes};

/// A variable lookup over `pairs`, in place of the process environment.
fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let variables: BTreeMap<String, String> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    move |name| variables.get(name).cloned()
}

#[test]
fn a_tiny_humans_bearer_sends_jev_and_the_planner_through_tiny_humans() -> Result<(), LabError> {
    let (jev, planner) = routes(&lookup(&[
        ("TINYHUMANS_TOKEN", " th-bearer \n"),
        ("OPENROUTER_API_KEY", "sk-or-unused"),
    ]))?;
    assert_eq!(jev["provider"], "tiny_humans_open_router");
    assert_eq!(jev["api_key"], "th-bearer");
    assert_eq!(planner["provider"], "tiny_humans");
    assert_eq!(planner["api_key"], "th-bearer");
    for model in ["model", "rescue_model", "output_model"] {
        assert_eq!(planner[model], TINY_HUMANS_MODEL, "{model}");
    }
    Ok(())
}

#[test]
fn a_named_model_replaces_the_gateway_default() -> Result<(), LabError> {
    let (_, planner) = routes(&lookup(&[
        ("TINYHUMANS_TOKEN", "th-bearer"),
        ("TINYCOMPUTER_RESCUE_MODEL", "reasoning-v1"),
        ("TINYCOMPUTER_OUTPUT_MODEL", "  "),
    ]))?;
    assert_eq!(planner["rescue_model"], "reasoning-v1");
    assert_eq!(planner["model"], TINY_HUMANS_MODEL);
    assert_eq!(planner["output_model"], TINY_HUMANS_MODEL);
    Ok(())
}

#[test]
fn without_a_bearer_jev_and_the_planner_use_openrouter() -> Result<(), LabError> {
    let (jev, planner) = routes(&lookup(&[
        ("TINYHUMANS_TOKEN", "  "),
        ("OPENROUTER_API_KEY", "sk-or-key"),
        ("TINYCOMPUTER_PLANNER_MODEL", "anthropic/claude-sonnet-5"),
    ]))?;
    assert_eq!(jev["provider"], "open_router");
    assert_eq!(planner["api_key"], "sk-or-key");
    assert_eq!(planner["model"], "anthropic/claude-sonnet-5");
    assert!(planner.get("provider").is_none());
    Ok(())
}

#[test]
fn without_any_key_the_error_names_both_variables() {
    assert!(routes(&lookup(&[])).is_err_and(|error| {
        let error = error.to_string();
        error.contains("OPENROUTER_API_KEY") && error.contains("TINYHUMANS_TOKEN")
    }));
}

#[test]
fn sage_takes_the_decisions_on_either_route() -> Result<(), LabError> {
    let (jev, planner) = routes(&lookup(&[
        ("TINYHUMANS_TOKEN", "th-bearer"),
        ("TINYCOMPUTER_DECISIONS", "sage"),
        ("SAGE_API_KEY", "sage-key"),
        ("SAGE_FAST", "1"),
    ]))?;
    assert_eq!(
        jev,
        json!({"api_key": "sage-key", "provider": "sage", "fast": true})
    );
    assert_eq!(planner["provider"], "tiny_humans");
    assert!(
        routes(&lookup(&[
            ("OPENROUTER_API_KEY", "sk-or-key"),
            ("TINYCOMPUTER_DECISIONS", "sage"),
        ]))
        .is_err_and(|error| error.to_string().contains("SAGE_API_KEY"))
    );
    Ok(())
}

fn hint(key: &str, name: &str) -> tinycomputer_bus::GroundingHint {
    tinycomputer_bus::GroundingHint {
        app: "browser".to_owned(),
        key: key.to_owned(),
        role: "button".to_owned(),
        name: Some(name.to_owned()),
        path: Vec::new(),
    }
}

#[test]
fn a_run_s_learned_elements_replace_the_same_ones_and_keep_the_rest() {
    let kept = vec![hint("search", "Go"), hint("add to cart", "Add to Cart")];
    let learned = vec![
        hint("add to cart", "Add to Bag"),
        hint("open the cart", "Cart"),
    ];
    let merged = merge_memory(kept, learned);
    let names = merged
        .iter()
        .map(|hint| hint.name.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(names, ["Go", "Add to Bag", "Cart"]);
}

#[test]
fn memory_is_read_from_its_file_and_a_run_s_learned_elements_are_saved_to_it()
-> Result<(), LabError> {
    let dir = std::env::temp_dir().join(format!("task-live-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("memory.json");
    assert!(
        read_memory(&path)?.is_empty(),
        "no file yet: nothing learned"
    );

    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string(&json!({"learned": [hint("search", "Go")]}))?,
    )?;
    remember(&dir, &path)?;
    assert_eq!(read_memory(&path)?, vec![hint("search", "Go")]);

    // A report that learned nothing keeps what the memory held.
    std::fs::write(dir.join("report.json"), "{}")?;
    remember(&dir, &path)?;
    assert_eq!(read_memory(&path)?.len(), 1);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
