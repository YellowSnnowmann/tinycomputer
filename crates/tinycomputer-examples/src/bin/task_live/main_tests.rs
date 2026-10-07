//! Tests for the `task_live` binary: which routes Jev and the planner take.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_examples::host::LabError;

use super::{TINY_HUMANS_MODEL, routes};

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

#[test]
fn plan_reasoning_reaches_the_planner_on_either_route() -> Result<(), LabError> {
    let (_, planner) = routes(&lookup(&[
        ("TINYHUMANS_TOKEN", "th-bearer"),
        ("TINYCOMPUTER_PLAN_REASONING", " OFF "),
    ]))?;
    assert_eq!(planner["plan_reasoning"], "off");
    let (_, planner) = routes(&lookup(&[
        ("OPENROUTER_API_KEY", "sk-or-key"),
        ("TINYCOMPUTER_PLAN_REASONING", "off"),
    ]))?;
    assert_eq!(planner["plan_reasoning"], "off");
    let (_, planner) = routes(&lookup(&[("TINYHUMANS_TOKEN", "th-bearer")]))?;
    assert!(
        planner.get("plan_reasoning").is_none(),
        "unset leaves the default"
    );
    let (_, planner) = routes(&lookup(&[
        ("TINYHUMANS_TOKEN", "th-bearer"),
        ("TINYCOMPUTER_PLAN_REASONING", "  "),
    ]))?;
    assert!(planner.get("plan_reasoning").is_none(), "blank is unset");
    Ok(())
}
