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
