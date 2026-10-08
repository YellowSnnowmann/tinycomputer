//! Tests for splitting a whole task's time across its runs, and for
//! comparing two sets of tasks.

use serde_json::{Value, json};

use super::super::{
    Split, at_ms, median, render_compare, render_split, render_table, split, story,
};

/// `ms` after 2026-10-07T06:00:00Z, as the engine writes `at`.
fn at(ms: u64) -> String {
    format!("2026-10-07T06:00:{:02}.{:03}Z", ms / 1000, ms % 1000)
}

/// A task that plans (0–3 s), runs a flow (3.1–4.8 s), is rescued
/// (4.8–6.8 s), waits for a person (7–9 s), and runs again (9–10 s).
fn task() -> Vec<Value> {
    vec![
        json!({"event": "plan", "at": at(3000), "wall_ms": 3000, "calls": 2, "ok": true}),
        json!({"event": "run", "at": at(3100), "kind": "flow"}),
        json!({"event": "exchange", "at": at(3500), "latency_ms": 400, "ok": true, "input_tokens": 100, "output_tokens": 5}),
        json!({"event": "exchange", "at": at(4000), "latency_ms": 900, "ok": true, "input_tokens": 100, "output_tokens": 5}),
        json!({"event": "decision", "at": at(4000), "wall_ms": 900}),
        json!({"event": "action", "at": at(4600), "action": "click", "wall_ms": 100, "settle_ms": 500}),
        json!({"event": "observe", "at": at(4700), "wall_ms": 100}),
        json!({"event": "turn", "at": at(4700), "wall_ms": 1600}),
        json!({"event": "step", "at": at(4800), "wall_ms": 1700}),
        json!({"event": "end", "at": at(4800), "wall_ms": 1700}),
        json!({"event": "rescue", "at": at(6800), "wall_ms": 2000, "calls": 1, "outcome": "guided"}),
        json!({"event": "resume", "at": at(9000), "waited_ms": 2000, "state": "needs_human"}),
        json!({"event": "run", "at": at(9000), "kind": "flow"}),
        json!({"event": "exchange", "at": at(9500), "latency_ms": 400, "ok": false}),
        json!({"event": "decision", "at": at(9500), "wall_ms": 400}),
        json!({"event": "end", "at": at(10_000), "wall_ms": 1000}),
    ]
}

#[test]
fn a_tasks_time_is_split_across_its_runs_counting_each_moment_once() {
    let spent = split(&task());
    assert_eq!(
        spent,
        Split {
            wall_ms: 10_000,
            planning_ms: 3000,
            rescue_ms: 2000,
            person_ms: 2000,
            flow_ms: 2700,
            jev_ms: 1300,
            settle_ms: 500,
            act_ms: 100,
            observe_ms: 100,
            flow_other_ms: 700,
            other_ms: 300,
            plans: 1,
            plan_calls: 2,
            rescues: 1,
            rescue_calls: 1,
            waits: 1,
            decisions: 2,
            calls: 3,
            failed_calls: 1,
            call_p50_ms: 400,
            call_p90_ms: 900,
            decision_p50_ms: 400,
            decision_p90_ms: 900,
            slowest_extra_ms: 250,
            input_tokens: 200,
            output_tokens: 10,
            jev_cost_micro_usd: 0,
            actions: 1,
            step_p50_ms: 1700,
            step_p90_ms: 1700,
            turn_p50_ms: 1600,
            turn_p90_ms: 1600,
            action_p50_ms: 600,
            action_p90_ms: 600,
        }
    );
    let shown = render_split(&spent);
    assert!(
        shown.contains("rescues      2.0s  20%  1 rescue(s), 1 model call(s)"),
        "{shown}"
    );
    assert!(
        shown.contains("the slowest call adds 250 ms a round"),
        "{shown}"
    );
    assert!(shown.lines().all(|line| line == line.trim_end()));
    assert_eq!(
        split(&[]),
        Split::default(),
        "nothing journaled, nothing spent"
    );
}

#[test]
fn timestamps_read_as_milliseconds_since_the_epoch() {
    assert_eq!(
        at_ms(&json!({"at": "2026-10-07T06:00:00.000Z"})),
        Some(1_791_352_800_000)
    );
    assert_eq!(
        at_ms(&json!({"at": "2000-02-29T23:59:59Z"})),
        Some(951_868_799_000)
    );
    assert_eq!(
        at_ms(&json!({"at": "1970-01-01T00:00:01.250Z"})),
        Some(1250)
    );
    assert_eq!(at_ms(&json!({"at": "yesterday"})), None);
    assert_eq!(at_ms(&json!({})), None);
}

#[test]
fn sets_of_tasks_are_compared_by_their_medians() {
    let with = |wall_ms, calls| Split {
        wall_ms,
        calls,
        ..Split::default()
    };
    assert_eq!(
        median(&[with(10, 1), with(30, 3), with(20, 2)]),
        with(20, 2)
    );
    assert_eq!(median(&[]), Split::default());

    let compared = render_compare(&[with(100, 7)], &[with(60, 7), with(80, 7)]);
    assert!(
        compared.contains("A (1)") && compared.contains("B (2)"),
        "{compared}"
    );
    let wall = compared
        .lines()
        .find(|line| line.starts_with("wall_ms"))
        .unwrap();
    assert!(wall.ends_with("-40%"), "{wall}");
    let planning = compared
        .lines()
        .find(|line| line.starts_with("planning_ms"))
        .unwrap();
    assert!(
        planning.ends_with('-'),
        "nothing to compare against: {planning}"
    );

    let table = render_table(&[
        ("a".to_owned(), with(10_000, 70)),
        ("b".to_owned(), with(20_000, 140)),
    ]);
    assert_eq!(table.lines().count(), 4, "{table}");
    assert!(table.lines().last().unwrap().starts_with("median of 2"));
}

#[test]
fn a_folder_of_runs_reads_as_one_task() {
    let scratch = std::env::temp_dir().join(format!(
        "tinycomputer-split-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let write = |run: &str, events: &[Value]| {
        let dir = scratch.join(run);
        std::fs::create_dir_all(&dir).unwrap();
        let lines = events
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(dir.join(tinycomputer_engine::JOURNAL_FILE), lines).unwrap();
        dir
    };
    let events = task();
    write("20261007T060000Z-plan-a1b2c3", &events[..1]);
    let task_run = write("task-t-1", &events[1..]);
    assert_eq!(story(&scratch).unwrap().len(), events.len());
    assert_eq!(story(&task_run).unwrap().len(), events.len() - 1);
    assert_eq!(split(&story(&scratch).unwrap()).planning_ms, 3000);
    let empty = scratch.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        story(&empty).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn the_framings_a_quorum_left_count_in_no_round() {
    let exchange = |ms: u64, latency: u64| json!({"event": "exchange", "at": at(ms), "latency_ms": latency, "ok": true});
    let decision = |ms: u64, left: u64| json!({"event": "decision", "at": at(ms), "wall_ms": 500, "left": left});
    let events = vec![
        json!({"event": "run", "at": at(0), "kind": "flow"}),
        exchange(400, 400),
        exchange(500, 500),
        decision(500, 0),
        exchange(800, 300),
        exchange(800, 300),
        exchange(800, 300),
        decision(800, 2),
        // The two the quorum left, ending after their decision.
        exchange(1_900, 2_000),
        exchange(2_400, 2_500),
        exchange(3_000, 600),
        exchange(3_100, 700),
        decision(3_100, 0),
    ];
    let spent = split(&events);
    // Rounds of 400/500, 300/300/300 and 600/700: 100, 0 and 100 ms.
    assert_eq!(spent.slowest_extra_ms, 66);
    assert_eq!(spent.calls, 9, "every call made counts");
}

#[test]
fn a_quorums_late_framings_are_told_from_the_next_rounds_by_their_questions() {
    let exchange = |ms: u64, latency: u64, questions: &[&str]| json!({"event": "exchange", "at": at(ms), "latency_ms": latency, "ok": true, "questions": questions});
    let decision = |ms: u64, left: u64, questions: &[&str]| json!({"event": "decision", "at": at(ms), "wall_ms": 500, "left": left, "questions": questions});
    let judging = ["done", "not_done"];
    let events = vec![
        json!({"event": "run", "at": at(0), "kind": "flow"}),
        exchange(300, 300, &judging),
        exchange(400, 400, &judging),
        decision(400, 2, &judging),
        // Grounding's round, the judging's two late framings among its calls.
        exchange(900, 400, &["target"]),
        exchange(1_000, 1_000, &judging),
        exchange(1_100, 600, &["target"]),
        exchange(1_200, 1_200, &judging),
        decision(1_100, 0, &["target"]),
    ];
    let spent = split(&events);
    // Rounds of 300/400 and 400/600: 100 and 200 ms.
    assert_eq!(spent.slowest_extra_ms, 150);
}

#[test]
fn a_warm_up_is_priced_but_is_no_decisions_call() {
    let events = vec![
        json!({"event": "run", "at": at(0), "kind": "flow"}),
        json!({"event": "exchange", "at": at(100), "latency_ms": 5_000, "ok": false,
               "step": "warm-up", "model": "typesafe/jev-1.13-20260917", "input_tokens": 20}),
        json!({"event": "exchange", "at": at(400), "latency_ms": 400, "ok": true,
               "model": "typesafe/jev-1.13-20260917", "input_tokens": 1_000}),
        json!({"event": "exchange", "at": at(500), "latency_ms": 500, "ok": true,
               "model": "typesafe/jev-1.13-20260917", "input_tokens": 1_000}),
        json!({"event": "decision", "at": at(500), "wall_ms": 500}),
    ];
    let spent = split(&events);
    assert_eq!((spent.calls, spent.failed_calls), (2, 0));
    assert_eq!(
        spent.slowest_extra_ms, 100,
        "the round is the decision's own"
    );
    assert_eq!(
        spent.input_tokens, 2_020,
        "its tokens are spent all the same"
    );
}

#[test]
fn jevs_input_tokens_are_priced_and_another_models_are_not() {
    let exchange = |model: &str, input_tokens: u64| {
        json!({
            "event": "exchange", "at": at(1000), "latency_ms": 500, "ok": true,
            "model": model, "input_tokens": input_tokens, "output_tokens": 40,
        })
    };
    let spent = split(&[
        json!({"event": "run", "at": at(0), "kind": "flow"}),
        exchange("typesafe/jev-1.13-20260917", 1_000_000),
        exchange("typesafe/jev-1.13-20260917", 300_000),
        // Sage bills by units, not at Jev's price.
        exchange("levanto-sage", 500_000),
    ]);
    assert_eq!(spent.input_tokens, 1_800_000);
    assert_eq!(
        spent.jev_cost_micro_usd, 54_600,
        "1.3 M tokens at $0.042 a million"
    );
    let shown = render_split(&spent);
    assert!(
        shown.contains("1800000 in, 120 out; Jev cost $0.0546"),
        "{shown}"
    );
    let table = render_table(&[("a".to_owned(), spent.clone())]);
    assert!(
        table.lines().next().unwrap().ends_with("jev cost"),
        "{table}"
    );
    assert!(
        table.lines().nth(1).unwrap().ends_with("$0.0546"),
        "{table}"
    );
    let halved = Split {
        jev_cost_micro_usd: 27_300,
        ..spent.clone()
    };
    let compared = render_compare(&[spent], &[halved]);
    let cost = compared
        .lines()
        .find(|line| line.starts_with("jev_cost_micro_usd"))
        .unwrap();
    assert!(cost.ends_with("-50%"), "{cost}");
}
