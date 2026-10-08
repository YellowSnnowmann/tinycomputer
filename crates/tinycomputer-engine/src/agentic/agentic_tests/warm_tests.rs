//! Tests for warming a runtime's connections while a task's plan is drafted.

use super::*;
use crate::agentic::runtime::{FIRST_TURN, WARM_TIMEOUT};

/// An evaluator nothing ever comes back from.
struct Silent;

impl Evaluator for Silent {
    fn evaluate<'a>(
        &'a self,
        _request: &'a tinyinference_decisions::EvaluationRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        tinyinference_decisions::EvaluationResult,
                        tinyinference_decisions::EvaluationFailure,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(std::future::pending())
    }
}

fn ready() -> tinyinference_decisions::EvaluationResult {
    evaluation(json!({
        "model": "typesafe/jev-1.13-20260917",
        "answers": {"ready": {"type": "noul", "noul": 0.9}},
        "usage": {"input_tokens": 10, "output_tokens": 2}
    }))
}

#[tokio::test]
async fn a_warm_up_asks_one_small_question_for_each_call_of_a_first_turn() {
    let (runtime, requests) = runtime_recording(vec![ready(); 14]);
    runtime.warm(7).await;
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        14,
        "the judging and grounding's opening, each in every framing, all at once"
    );
    for request in requests.iter() {
        assert_eq!(request.model, "jev-latest", "the runtime's own model");
        assert_eq!(
            request.questions.keys().collect::<Vec<_>>(),
            ["ready"],
            "{request:?}"
        );
        assert!(matches!(
            request.questions["ready"],
            tinyinference_decisions::Question::Noul(_)
        ));
    }
}

#[tokio::test]
async fn a_warm_up_opens_no_more_than_a_first_turn_asks_at_once() {
    let (runtime, requests) = runtime_recording(vec![ready(); 18]);
    runtime.warm(50).await;
    assert_eq!(
        requests.lock().unwrap().len(),
        18,
        "MAX_VOTES framings of each first-turn request at most"
    );
    let (runtime, requests) = runtime_recording(vec![ready(); 2]);
    runtime.warm(0).await;
    assert_eq!(
        requests.lock().unwrap().len(),
        usize::try_from(FIRST_TURN).unwrap(),
        "one framing of each at least"
    );
}

#[tokio::test]
async fn sage_is_not_warmed() {
    let (mut runtime, requests) = runtime_recording(Vec::new());
    runtime.configuration.provider = JevProvider::Sage;
    runtime.warm(7).await;
    assert!(requests.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_warm_up_nothing_answers_is_given_up_on() {
    let (mut runtime, _requests) = runtime_recording(Vec::new());
    runtime.client = Arc::new(Silent);
    let started = tokio::time::Instant::now();
    runtime.warm(7).await;
    assert_eq!(started.elapsed(), WARM_TIMEOUT);
}

#[tokio::test]
async fn a_warm_up_is_journaled_with_the_task() {
    let dir = std::env::temp_dir().join(format!("tinycomputer-warm-{}", std::process::id()));
    let (runtime, _requests) = runtime_recording(vec![ready(); 14]);
    let runtime = runtime.with_journal(&dir).journaled_as("task-t-1");
    runtime.warm(7).await;
    let journal = std::fs::read_to_string(dir.join("task-t-1").join("journal.jsonl")).unwrap();
    let steps = journal
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|event| event["event"] == "exchange")
        .map(|event| event["step"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(steps, vec!["warm-up"; 14]);
}
