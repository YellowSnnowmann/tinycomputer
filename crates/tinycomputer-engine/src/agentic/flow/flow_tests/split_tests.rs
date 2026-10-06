//! Asking a request too large for one Jev call in parts: each part carries
//! the whole screen and some of the questions, and the answers merge back.

use super::*;
use crate::agentic::flow::{
    MAX_REQUEST_BYTES,
    decide::{split, whole},
};

/// A knockout as grounding asks it: `groups` questions of 20 options, each
/// carrying the run's brief, over a screen of a few kilobytes.
fn knockout(groups: usize) -> EvaluationRequest {
    let brief = json!({"goal": "g".repeat(400), "plan": ["1. [now] open the message"]});
    let mut questions = ask::Questions::default();
    for group in 0..groups {
        let options = (0..20).map(|option| {
            (
                format!("{option}"),
                json!({"untrusted_accessibility_data": {
                    "what": format!("button \"Message {group}-{option} {}\"", "subject ".repeat(12)),
                    "where": "window \"Inbox\" > list \"Messages\"",
                }}),
            )
        });
        questions = questions.with(
            &format!("group_{group}"),
            ask::options(
                json!({"task": "which one opens the message", "brief": brief}),
                options,
            ),
        );
    }
    let state = json!({"elements": {"untrusted_accessibility_data": (0..60).map(|line| format!("button \"Message {line}\"")).collect::<Vec<_>>()}});
    ask::request("jev-latest", state, questions)
}

fn size(request: &EvaluationRequest) -> usize {
    serde_json::to_vec(request).unwrap().len()
}

#[test]
fn a_request_too_large_for_one_call_is_asked_in_parts() {
    let request = knockout(30);
    assert!(size(&request) > MAX_REQUEST_BYTES, "{}", size(&request));
    let parts = split(request.clone(), MAX_REQUEST_BYTES);
    assert!(parts.len() > 1);
    for part in &parts {
        assert!(size(part) <= MAX_REQUEST_BYTES, "{}", size(part));
        assert_eq!(
            part.state, request.state,
            "every part sees the whole screen"
        );
        assert_eq!(part.model, request.model);
        assert!(
            part.questions.values().all(|question| matches!(
                question,
                Question::Choice(choice) if choice.instructions.get("brief").is_some()
            )),
            "every question keeps its brief"
        );
    }
    let asked = parts
        .iter()
        .flat_map(|part| part.questions.keys().cloned())
        .collect::<Vec<_>>();
    let ids = request.questions.keys().cloned().collect::<Vec<_>>();
    assert_eq!(asked, ids, "each question asked once, in order");
    assert_eq!(whole(&parts), request, "the parts make up the request");
}

#[test]
fn a_request_that_fits_or_holds_one_question_stays_whole() {
    let small = knockout(2);
    assert_eq!(split(small.clone(), MAX_REQUEST_BYTES), [small]);
    let single = knockout(1);
    assert_eq!(
        split(single.clone(), size(&single) / 2),
        [single],
        "a lone question is left for fit to shrink"
    );
    assert_eq!(whole(&[]).questions.len(), 0);
}

#[tokio::test]
async fn an_oversized_knockout_is_asked_in_parts_and_still_finds_its_target() {
    // Live, a long results page made a 16-group knockout of 79 KB, which the
    // gateway refused with HTTP 502 every time, ending the task. Then, split,
    // the answers of every part but the first were dropped, so a search's
    // "Go" button, asked in the second part, was never pressed.
    let run = run_with(
        App::with(|sim| {
            sim.extra_buttons = 400;
            sim.quirks.insert(Quirk::OneRegion);
        }),
        json!({"app": "Mail", "steps": ["open message 190"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "region" => Some(pick(question, "Messages", 0.9)),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.9 })),
            // The one row named so: no other label holds "Message 190". It is
            // in group 9, the last group by key, so in the request's last part.
            "target" => Some(pick(question, "Message 190", 0.9)),
            _ if id.starts_with("group_") => Some(pick(question, "Message 190", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["Message 190"]);
    assert!(
        run.requests
            .iter()
            .all(|request| size(request) <= MAX_REQUEST_BYTES),
        "no request is larger than one call takes"
    );
    let groups = |request: &EvaluationRequest| {
        request
            .questions
            .keys()
            .filter(|id| id.starts_with("group_"))
            .count()
    };
    let knockout = run
        .requests
        .iter()
        .map(groups)
        .filter(|count| *count > 0)
        .collect::<Vec<_>>();
    assert!(
        knockout.len() > 1,
        "the knockout's groups went out in parts: {knockout:?}"
    );
    assert!(
        run.requests.iter().any(|request| {
            request.questions.contains_key("group_9") && !request.questions.contains_key("group_0")
        }),
        "the target's group was asked in a part of its own, not the first"
    );
}
