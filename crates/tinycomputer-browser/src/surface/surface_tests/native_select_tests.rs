//! A native dropdown's option is chosen by setting the dropdown's value, and
//! every other option is pressed as a control.

use super::*;
use serde_json::Value;
use tinycomputer_bus::JevOperation;
use tinycomputer_core::surface::Surface;

/// A page whose option check answers `chosen`: `true` for a native option
/// the dropdown took, `false` for one it refused, `null` for no native option.
fn dropdown_fake(chosen: Value) -> Fake {
    Fake::scripted(move |command| match command["action"].as_str().unwrap() {
        "evaluate" => Some(ok(&json!({"result": chosen}))),
        "click" => Some(ok(&json!({}))),
        _ => None,
    })
}

fn option(reference: &str, name: &str) -> Candidate {
    Candidate {
        role: "option".to_owned(),
        name: Some(name.to_owned()),
        ..node(reference, &["Click"])
    }
}

#[test]
fn a_native_dropdown_option_is_chosen_by_value_without_a_click() {
    // BlazeDemo's departure city: a click on the dropdown only opened a menu
    // drawn outside the page, so "Boston" was never chosen.
    let Harness { fake, surface, .. } = harness("native-option", dropdown_fake(json!(true)));
    let reply = surface.execute(JevOperation::Click, Some(option("seen:21", "Boston")), None);
    assert!(reply.ok, "{reply:?}");
    assert_eq!(reply.data.as_ref().unwrap()["via"], "native_select");
    let script = fake.last("evaluate")["script"].as_str().unwrap().to_owned();
    assert!(script.contains(r#"[data-tc-seen=\"21\"]"#), "{script}");
    assert!(script.contains("new Event('change'"), "{script}");
    assert!(!fake.actions().iter().any(|action| action == "click"));
}

#[test]
fn a_refused_native_option_fails_without_a_click() {
    let Harness { fake, surface, .. } = harness("native-refused", dropdown_fake(json!(false)));
    let reply = surface.execute(
        JevOperation::Click,
        Some(option("seen:22", "Sold out")),
        None,
    );
    assert!(!reply.ok);
    assert_eq!(reply.error.as_ref().unwrap().code, "ACTION_FAILED");
    assert!(!fake.actions().iter().any(|action| action == "click"));
}

#[test]
fn an_option_that_is_not_native_is_pressed_as_a_control() {
    // A page's own listbox row (`role="option"` on a div): the check answers
    // null and the click goes through.
    let Harness { fake, surface, .. } = harness("aria-option", dropdown_fake(Value::Null));
    let reply = surface.execute(
        JevOperation::Click,
        Some(option("seen:23", "Srinagar, SXR")),
        None,
    );
    assert!(reply.ok, "{reply:?}");
    assert!(fake.actions().iter().any(|action| action == "click"));

    // A tree ref is never a sight-marked element, so it is not checked.
    let Harness { fake, surface, .. } = harness("tree-option", dropdown_fake(json!(true)));
    assert!(
        surface
            .execute(JevOperation::Click, Some(option("e8", "Economy")), None)
            .ok
    );
    assert!(fake.actions().iter().any(|action| action == "click"));
    assert!(!fake.evaluated_besides_every_press());
}
