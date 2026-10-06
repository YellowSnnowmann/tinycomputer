//! Tests for the task follower's pure parts: pacing, what counts as a pass,
//! and what a person at the terminal sends a paused task.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::time::Duration;

use tinycomputer_bus::agent::TaskStatus;

use super::{
    AWAIT_SLICE, Person, inputs_for, loggable, next_wait, passed, read_lines, reply, state,
};
use tinycomputer_bus::agent::{InputField, InputKind, TaskId};

const LIMIT: Duration = Duration::from_secs(20 * 60);

#[test]
fn waits_a_full_slice_while_plenty_of_time_remains() {
    assert_eq!(next_wait(Duration::ZERO, LIMIT), Some(AWAIT_SLICE));
}

#[test]
fn waits_only_the_time_left_near_the_limit() {
    let elapsed = Duration::from_secs(20 * 60 - 10);
    assert_eq!(next_wait(elapsed, LIMIT), Some(Duration::from_secs(10)));
}

#[test]
fn stops_waiting_once_the_limit_is_spent() {
    assert_eq!(next_wait(LIMIT, LIMIT), None);
    assert_eq!(next_wait(Duration::from_secs(20 * 60 + 1), LIMIT), None);
}

#[test]
fn a_finished_task_or_a_payment_stop_passes() {
    assert!(passed(&TaskStatus::Done {
        answer: String::new(),
        records: BTreeMap::new(),
        result: None,
    }));
    let checkpoint = |reason: &str| TaskStatus::Checkpoint {
        reason: reason.to_owned(),
        location: String::new(),
        screenshot: None,
        summary: String::new(),
        continuable: false,
    };
    assert!(passed(&checkpoint("reached the payment page")));
    assert!(!passed(&checkpoint("a login wall")));
    assert!(!passed(&TaskStatus::Cancelled));
}

#[test]
fn a_status_is_named_by_its_wire_state() {
    assert_eq!(state(&TaskStatus::Running), "running");
    assert_eq!(state(&TaskStatus::Cancelled), "cancelled");
}

fn field(name: &str) -> InputField {
    InputField {
        name: name.to_owned(),
        why: String::new(),
        kind: InputKind::Text,
        options: Vec::new(),
    }
}

#[test]
fn a_pause_is_answered_only_when_every_field_is_known() {
    let answers = BTreeMap::from([("phone".to_owned(), "+91".to_owned())]);
    assert_eq!(
        inputs_for(&[field("phone")], &answers),
        Some(answers.clone())
    );
    assert_eq!(
        inputs_for(&[field("phone"), field("email")], &answers),
        None
    );
}

#[test]
fn a_pause_asking_for_nothing_is_handed_back() {
    let answers = BTreeMap::from([("phone".to_owned(), "+91".to_owned())]);
    assert_eq!(inputs_for(&[], &answers), None);
}

#[test]
fn a_logged_url_keeps_only_its_scheme_and_host() {
    assert_eq!(
        loggable("https://user:secret@pay.test/checkout?token=abc#step"),
        "https://pay.test"
    );
    // A path can carry a token (a magic link), and an `@` in it is not a
    // credential separator.
    assert_eq!(
        loggable("https://example.com/reset/t0k3n"),
        "https://example.com"
    );
    assert_eq!(
        loggable("https://example.com/@alice?tab=1"),
        "https://example.com"
    );
    assert_eq!(loggable("https://u:p@example.com"), "https://example.com");
    // An opaque URL keeps only its scheme; its payload can hold anything.
    assert_eq!(loggable("about:blank"), "about:");
    assert_eq!(loggable("data:text/html,<p>token=abc</p>"), "data:");
    assert_eq!(loggable("not a url"), "");
}

/// A person who answers from a script and remembers what they were asked.
struct Scripted {
    approves: bool,
    handles: bool,
    inputs: BTreeMap<String, String>,
    asked: std::sync::Mutex<Vec<String>>,
}

impl Scripted {
    fn new(approves: bool, handles: bool) -> Self {
        Self {
            approves,
            handles,
            inputs: BTreeMap::new(),
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Person for Scripted {
    fn approve(&self, action: &str, target: &str) -> bool {
        self.asked
            .lock()
            .unwrap()
            .push(format!("approve {action} {target}"));
        self.approves
    }

    fn handled(&self, reason: &str) -> bool {
        self.asked.lock().unwrap().push(format!("handled {reason}"));
        self.handles
    }

    fn input(&self, field: &InputField) -> Option<String> {
        self.asked
            .lock()
            .unwrap()
            .push(format!("input {}", field.name));
        self.inputs.get(&field.name).cloned()
    }

    fn finish(&self, reason: &str) {
        self.asked.lock().unwrap().push(format!("finish {reason}"));
    }
}

fn status(json: serde_json::Value) -> TaskStatus {
    serde_json::from_value(json).unwrap()
}

#[test]
fn an_approval_is_sent_as_the_person_decides() {
    let id = TaskId("t-1".to_owned());
    let pause = status(serde_json::json!({
        "state": "needs_approval", "action": "clicking Submit", "target": "Submit"
    }));
    let yes = Scripted::new(true, false);
    let approved = reply(&id, &pause, &BTreeMap::new(), &yes).unwrap();
    assert_eq!(approved.id, id);
    assert_eq!(approved.approve, Some(true));
    assert_eq!(yes.asked(), ["approve clicking Submit Submit"]);
    // A decline is sent too, so the task learns it and stops.
    let no = Scripted::new(false, false);
    let declined = reply(&id, &pause, &BTreeMap::new(), &no).unwrap();
    assert_eq!(declined.approve, Some(false));
}

#[test]
fn a_wall_goes_on_only_once_the_person_has_dealt_with_it() {
    let id = TaskId("t-2".to_owned());
    let wall = status(serde_json::json!({
        "state": "needs_human", "reason": "sign in, then continue the task"
    }));
    let done = reply(&id, &wall, &BTreeMap::new(), &Scripted::new(false, true)).unwrap();
    assert_eq!(done.answer.as_deref(), Some("done"));
    assert_eq!(done.approve, None);
    assert!(reply(&id, &wall, &BTreeMap::new(), &Scripted::new(false, false)).is_none());
}

#[test]
fn a_missing_detail_takes_the_answers_first_and_asks_for_the_rest() {
    let id = TaskId("t-3".to_owned());
    let pause = TaskStatus::NeedsInput {
        fields: vec![field("phone"), field("email")],
    };
    let answers = BTreeMap::from([("phone".to_owned(), "+91".to_owned())]);
    let mut person = Scripted::new(false, false);
    person
        .inputs
        .insert("email".to_owned(), "asha@example.com".to_owned());
    let continued = reply(&id, &pause, &answers, &person).unwrap();
    assert_eq!(continued.inputs["phone"], "+91");
    assert_eq!(continued.inputs["email"], "asha@example.com");
    assert_eq!(
        person.asked(),
        ["input email"],
        "only the missing one is asked"
    );
    // A detail the person does not give stops following.
    let silent = Scripted::new(false, false);
    assert!(reply(&id, &pause, &BTreeMap::new(), &silent).is_none());
}

#[test]
fn a_continuable_checkpoint_goes_on_only_when_approved() {
    let id = TaskId("t-4".to_owned());
    let stop = status(serde_json::json!({
        "state": "checkpoint", "reason": "review the order", "location": "review page",
        "summary": "", "continuable": true
    }));
    let approved = reply(&id, &stop, &BTreeMap::new(), &Scripted::new(true, false)).unwrap();
    assert_eq!(approved.approve, Some(true));
}

#[test]
fn nothing_is_sent_for_a_state_no_person_answers() {
    let id = TaskId("t-5".to_owned());
    let person = Scripted::new(true, true);
    for paused in [
        status(serde_json::json!({"state": "running"})),
        status(serde_json::json!({"state": "cancelled"})),
        status(serde_json::json!({
            "state": "checkpoint", "reason": "payment", "location": "pay page",
            "summary": "", "continuable": false
        })),
        TaskStatus::NeedsInput { fields: Vec::new() },
    ] {
        assert!(
            reply(&id, &paused, &BTreeMap::new(), &person).is_none(),
            "{paused:?}"
        );
    }
    assert_eq!(person.asked().len(), 0);
}

#[test]
fn what_a_task_read_is_printed_one_variable_a_line() {
    let value = |text: &str| BTreeMap::from([("value".to_owned(), text.to_owned())]);
    let records = BTreeMap::from([
        ("total".to_owned(), vec![value("Rs. 264")]),
        (
            "flights".to_owned(),
            vec![
                BTreeMap::from([
                    ("field 1".to_owned(), "IndiGo".to_owned()),
                    ("field 2".to_owned(), "₹5,000".to_owned()),
                ]),
                BTreeMap::from([("field 1".to_owned(), "Vistara".to_owned())]),
            ],
        ),
    ]);
    assert_eq!(
        read_lines(&records),
        [
            "  read flights: IndiGo, ₹5,000 | Vistara",
            "  read total: Rs. 264"
        ]
    );
    assert_eq!(read_lines(&BTreeMap::new()).len(), 0);
}

#[test]
fn a_pause_reads_as_one_sentence_before_the_prompt() {
    use super::person::sentence;
    assert_eq!(
        sentence("reached the payment step (PLACE ORDER); paying is left to you"),
        "reached the payment step (PLACE ORDER); paying is left to you."
    );
    assert_eq!(
        sentence("sign in, then continue the task. "),
        "sign in, then continue the task."
    );
}
