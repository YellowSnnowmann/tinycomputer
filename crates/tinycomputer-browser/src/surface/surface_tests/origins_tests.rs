//! Tests for a task's surface on a page its allowed origins refuse: the
//! observation fails as blocked, and the page is left before it is read.

use std::sync::{Arc, Mutex};

use serde_json::json;
use tinycomputer_bus::browser::SessionOptions;
use tinycomputer_core::surface::{Depth, Surface};

use super::{Drawn, shown_harness};
use crate::fake::{Fake, ok};

/// A page at `start` that goes back to `back`, reporting where it is.
fn page_at(start: &str, back: &'static str) -> Fake {
    let at = Arc::new(Mutex::new(start.to_owned()));
    Fake::scripted(move |command| {
        let mut at = at.lock().unwrap();
        match command["action"].as_str().unwrap() {
            "back" => {
                back.clone_into(&mut at);
                None
            }
            "url" => Some(ok(&json!({"url": *at}))),
            _ => None,
        }
    })
}

fn within(origins: &[&str]) -> SessionOptions {
    SessionOptions {
        allowed_origins: origins.iter().map(|origin| (*origin).to_owned()).collect(),
        ..SessionOptions::default()
    }
}

#[test]
fn observing_a_refused_page_fails_as_blocked_and_leaves_it_unread() {
    let fake = page_at("https://evil.test/offer", "https://flights.test/");
    let harness = shown_harness(
        "origins-observe",
        fake,
        within(&[".flights.test"]),
        &Drawn::default(),
    );
    let refused = harness
        .surface
        .observe("", None, Depth::Full)
        .expect_err("a refused page is never read");
    let error = refused.error.as_ref().unwrap();
    assert_eq!(error.code, "BLOCKED_BY_POLICY");
    assert!(
        error.message.contains("https://evil.test/offer"),
        "{}",
        error.message
    );
    let actions = harness.fake.actions();
    assert!(actions.contains(&"back".to_owned()));
    assert!(
        !actions.iter().any(|action| action == "snapshot")
            && !harness
                .fake
                .sent()
                .iter()
                .any(|command| command["action"] == "evaluate"),
        "nothing was read on the refused page: {actions:?}"
    );
}

#[test]
fn an_admitted_page_is_read_as_before() {
    let fake = page_at("https://www.flights.test/search", "https://flights.test/");
    let harness = shown_harness(
        "origins-admitted",
        fake,
        within(&[".flights.test"]),
        &Drawn::default(),
    );
    assert!(harness.surface.observe("", None, Depth::Full).is_ok());
    assert!(!harness.fake.actions().contains(&"back".to_owned()));
}
