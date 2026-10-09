//! Tests for the allowed origins a session checks its pages against: a
//! navigation refused before the browser is asked, and a page any call lands
//! on outside them left and reported.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tinycomputer_bus::browser::{
    Action, NavigateRequest, SessionId, SessionOptions, SnapshotRequest, Target,
};

use super::{Browser, scratch};
use crate::error::Error;
use crate::fake::{Fake, ok};

async fn open_within(fake: &Fake, name: &str, origins: &[&str]) -> (Browser, SessionId) {
    let browser = Browser::with_scratch(Arc::new(fake.clone()), scratch(name));
    let info = browser
        .open_session(SessionOptions {
            allowed_origins: origins.iter().map(|origin| (*origin).to_owned()).collect(),
            ..SessionOptions::default()
        })
        .await
        .unwrap();
    (browser, info.id)
}

/// A page that starts at `start` and moves where `moves` takes it after each
/// command, reporting where it is as the engine would.
fn moving(
    start: &str,
    moves: impl Fn(&Value) -> Option<&'static str> + Send + Sync + 'static,
) -> Fake {
    let at = Arc::new(Mutex::new(start.to_owned()));
    Fake::scripted(move |command| {
        let mut at = at.lock().unwrap();
        if let Some(next) = moves(command) {
            next.clone_into(&mut at);
        }
        match command["action"].as_str().unwrap() {
            "url" => Some(ok(&json!({"url": *at}))),
            "navigate" => Some(ok(&json!({"url": *at, "title": "Loaded"}))),
            _ => None,
        }
    })
}

fn navigated_to(fake: &Fake, url: &str) -> bool {
    fake.sent()
        .iter()
        .any(|command| command["action"] == "navigate" && command["url"] == url)
}

fn blocked(result: Result<impl std::fmt::Debug, Error>, refused: &str) {
    match result {
        Err(Error::BlockedByPolicy { url }) => assert_eq!(url, refused),
        other => panic!("expected {refused} to be refused, got {other:?}"),
    }
}

#[tokio::test]
async fn a_navigation_outside_the_origins_is_refused_before_the_browser_is_asked() {
    let fake = Fake::new();
    let (browser, id) = open_within(&fake, "origins-navigate", &[".flights.test"]).await;
    blocked(
        browser
            .navigate(&id, NavigateRequest::new("https://evil.test/"))
            .await,
        "https://evil.test/",
    );
    assert!(!navigated_to(&fake, "https://evil.test/"));
    let page = browser
        .navigate(&id, NavigateRequest::new("https://www.flights.test/search"))
        .await
        .unwrap();
    assert_eq!(page.url, "https://www.flights.test/search");
}

#[tokio::test]
async fn a_redirect_out_of_the_origins_is_left_by_going_back() {
    let fake = moving("https://flights.test/", |command| {
        match command["action"].as_str().unwrap() {
            "navigate" if command["url"] == "https://flights.test/go" => {
                Some("https://evil.test/landing")
            }
            "back" => Some("https://flights.test/"),
            _ => None,
        }
    });
    let (browser, id) = open_within(&fake, "origins-redirect", &[".flights.test"]).await;
    blocked(
        browser
            .navigate(&id, NavigateRequest::new("https://flights.test/go"))
            .await,
        "https://evil.test/landing",
    );
    assert!(fake.actions().contains(&"back".to_owned()));
    assert!(!navigated_to(&fake, "about:blank"), "going back was enough");
    assert_eq!(
        browser.list_sessions().await.unwrap()[0].url,
        "https://flights.test/"
    );
}

#[tokio::test]
async fn a_click_into_a_refused_page_with_no_way_back_is_left_for_a_blank_page() {
    let fake = moving("https://flights.test/", |command| {
        match command["action"].as_str().unwrap() {
            "click" => Some("https://evil.test/offer"),
            "navigate" if command["url"] == "about:blank" => Some("about:blank"),
            // A new tab has no page to go back to.
            _ => None,
        }
    });
    let (browser, id) = open_within(&fake, "origins-click", &[".flights.test"]).await;
    blocked(
        browser
            .perform(
                &id,
                Action::Click {
                    target: Target::reference("e1"),
                    new_tab: false,
                },
            )
            .await,
        "https://evil.test/offer",
    );
    assert!(navigated_to(&fake, "about:blank"));
    assert_eq!(browser.list_sessions().await.unwrap()[0].url, "about:blank");
}

#[tokio::test]
async fn a_refused_page_is_never_read() {
    let fake = moving("https://evil.test/", |command| {
        (command["action"] == "back").then_some("https://flights.test/")
    });
    let (browser, id) = open_within(&fake, "origins-read", &[".flights.test"]).await;
    blocked(
        browser.snapshot(&id, SnapshotRequest::default()).await,
        "https://evil.test/",
    );
}

#[tokio::test]
async fn a_raw_command_that_opens_a_refused_page_is_never_sent() {
    let fake = Fake::new();
    let (browser, id) = open_within(&fake, "origins-command", &[".flights.test"]).await;
    blocked(
        browser
            .command(
                &id,
                json!({"action": "tab_new", "url": "https://evil.test/"}),
            )
            .await,
        "https://evil.test/",
    );
    assert!(!fake.actions().contains(&"tab_new".to_owned()));
    browser
        .command(
            &id,
            json!({"action": "tab_new", "url": "https://flights.test/deals"}),
        )
        .await
        .unwrap();
    browser
        .command(&id, json!({"action": "evaluate", "script": "1"}))
        .await
        .unwrap();
}

#[tokio::test]
async fn checking_the_page_is_free_with_no_list_and_leaves_a_refused_page_with_one() {
    let fake = Fake::new();
    let (browser, id) = open_within(&fake, "origins-free", &[]).await;
    let sent = fake.sent().len();
    browser.check_page(&id).await.unwrap();
    assert_eq!(
        fake.sent().len(),
        sent,
        "no list, nothing asked of the engine"
    );

    let fake = moving("https://evil.test/", |command| {
        (command["action"] == "back").then_some("https://flights.test/")
    });
    let (browser, id) = open_within(&fake, "origins-check", &["https://flights.test"]).await;
    blocked(browser.check_page(&id).await, "https://evil.test/");
    assert!(fake.actions().contains(&"back".to_owned()));
    browser.check_page(&id).await.unwrap();
}

#[tokio::test]
async fn the_engine_is_never_handed_the_origins_so_a_page_loads_its_own_files() {
    let fake = Fake::new();
    let (_browser, _id) = open_within(&fake, "origins-launch", &[".flights.test"]).await;
    let launch = fake.last("launch");
    assert!(
        launch.get("allowedDomains").is_none(),
        "the engine would refuse the page's CDN, and a profile beside it: {launch}"
    );
}
