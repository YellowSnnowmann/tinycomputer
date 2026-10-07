//! Where a whole task's time went, and two sets of tasks side by side.
//!
//! A task journals each of its flows, and since `plan`, `rescue`, and
//! `resume` events, the time between them too. Each run of a file restarts
//! `elapsed_ms`, so the split is built from the absolute `at` timestamps
//! instead: every event that took time becomes an interval ending at `at`,
//! and overlapping intervals (the framings of one decision, a batch of
//! decisions) are counted once.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{number, percentile, seconds};

/// Where one task's time went, in ms unless named otherwise.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Split {
    /// From the start of the first thing journaled to the last event.
    pub wall_ms: u64,
    /// Planning the task (`plan` events).
    pub planning_ms: u64,
    /// Asking the rescuer (`rescue` events).
    pub rescue_ms: u64,
    /// Waiting for a person to answer (`resume` events).
    pub person_ms: u64,
    /// Running flows (`end` events).
    pub flow_ms: u64,
    /// Within flows, waiting on Jev decisions, not counting acting.
    pub jev_ms: u64,
    /// Within flows, letting the surface settle after an action.
    pub settle_ms: u64,
    /// Within flows, acting, not counting settling.
    pub act_ms: u64,
    /// Within flows, reading the screen, not counting the above.
    pub observe_ms: u64,
    /// Within flows, what no event accounts for: building requests,
    /// merging answers, and the gaps between events.
    pub flow_other_ms: u64,
    /// Outside flows and the events above: the module starting, shaping the
    /// answer, and the gaps between runs.
    pub other_ms: u64,
    /// Plans journaled.
    pub plans: u64,
    /// Model calls the plans made, repairs included.
    pub plan_calls: u64,
    /// Rescues journaled.
    pub rescues: u64,
    /// Model calls the rescues made, repairs included.
    pub rescue_calls: u64,
    /// Answers a person gave to a paused task.
    pub waits: u64,
    /// Jev decisions.
    pub decisions: u64,
    /// Jev calls, one per framing.
    pub calls: u64,
    /// Calls that failed.
    pub failed_calls: u64,
    /// Per-call latency, 50th and 90th percentiles.
    pub call_p50_ms: u64,
    /// See [`Split::call_p50_ms`].
    pub call_p90_ms: u64,
    /// Per-decision latency (what a step waited), 50th and 90th percentiles.
    pub decision_p50_ms: u64,
    /// See [`Split::decision_p50_ms`].
    pub decision_p90_ms: u64,
    /// How much longer a round of calls waited for its slowest call than for
    /// its median one, on average: the cost of waiting for every framing.
    pub slowest_extra_ms: u64,
    /// Provider-reported input tokens.
    pub input_tokens: u64,
    /// Provider-reported output tokens.
    pub output_tokens: u64,
    /// Actions taken.
    pub actions: u64,
    /// A step's wall time, 50th and 90th percentiles.
    pub step_p50_ms: u64,
    /// See [`Split::step_p50_ms`].
    pub step_p90_ms: u64,
    /// A `do` turn's wall time, 50th and 90th percentiles.
    pub turn_p50_ms: u64,
    /// See [`Split::turn_p50_ms`].
    pub turn_p90_ms: u64,
    /// An action with its settling, 50th and 90th percentiles.
    pub action_p50_ms: u64,
    /// See [`Split::action_p50_ms`].
    pub action_p90_ms: u64,
}

type Spans = Vec<(i64, i64)>;

/// Where the time of the task journaled in `events` went. `events` may hold
/// several runs, from one file or several (a plan journaled before its task
/// existed), in any order.
#[must_use]
pub fn split(events: &[Value]) -> Split {
    let mut split = Split::default();
    let mut spans = Kinds::default();
    let (mut calls, mut decisions, mut steps, mut turns, mut actions) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut start = i64::MAX;
    let mut end = i64::MIN;
    for event in events {
        let Some(at) = at_ms(event) else {
            continue;
        };
        let took = i64::try_from(number(event, "wall_ms")).unwrap_or(i64::MAX);
        let span = (at.saturating_sub(took), at);
        end = end.max(at);
        start = start.min(at);
        match event["event"].as_str().unwrap_or_default() {
            "end" => spans.flows.push(span),
            "plan" => {
                spans.plans.push(span);
                split.plans += 1;
                split.plan_calls += number(event, "calls");
            }
            "rescue" => {
                spans.rescues.push(span);
                split.rescues += 1;
                split.rescue_calls += number(event, "calls");
            }
            "resume" => {
                let waited = i64::try_from(number(event, "waited_ms")).unwrap_or(i64::MAX);
                spans.person.push((at.saturating_sub(waited), at));
                split.waits += 1;
            }
            "decision" => {
                spans.decisions.push(span);
                decisions.push(number(event, "wall_ms"));
            }
            "observe" => spans.observe.push(span),
            "action" => {
                let settle = i64::try_from(number(event, "settle_ms")).unwrap_or(i64::MAX);
                let acted = at.saturating_sub(settle);
                spans.settle.push((acted, at));
                spans.act.push((acted.saturating_sub(took), acted));
                actions.push(number(event, "wall_ms") + number(event, "settle_ms"));
            }
            "exchange" => {
                calls.push(number(event, "latency_ms"));
                split.failed_calls += u64::from(event["ok"] == Value::Bool(false));
                split.input_tokens += number(event, "input_tokens");
                split.output_tokens += number(event, "output_tokens");
            }
            "step" => steps.push(number(event, "wall_ms")),
            "turn" => turns.push(number(event, "wall_ms")),
            _ => {}
        }
        start = start.min(match event["event"].as_str() {
            Some("resume") => {
                at.saturating_sub(i64::try_from(number(event, "waited_ms")).unwrap_or(i64::MAX))
            }
            _ => span.0,
        });
    }
    if start > end {
        return split;
    }
    split.wall_ms = millis(end - start);
    spans.measure(&mut split);
    split.decisions = decisions.len() as u64;
    split.calls = calls.len() as u64;
    split.actions = actions.len() as u64;
    split.slowest_extra_ms = slowest_extra(events);
    for (values, low, high) in [
        (&mut calls, &mut split.call_p50_ms, &mut split.call_p90_ms),
        (
            &mut decisions,
            &mut split.decision_p50_ms,
            &mut split.decision_p90_ms,
        ),
        (&mut steps, &mut split.step_p50_ms, &mut split.step_p90_ms),
        (&mut turns, &mut split.turn_p50_ms, &mut split.turn_p90_ms),
        (
            &mut actions,
            &mut split.action_p50_ms,
            &mut split.action_p90_ms,
        ),
    ] {
        values.sort_unstable();
        *low = percentile(values, 50);
        *high = percentile(values, 90);
    }
    split
}

/// The intervals each kind of event took.
#[derive(Default)]
struct Kinds {
    flows: Spans,
    plans: Spans,
    rescues: Spans,
    person: Spans,
    decisions: Spans,
    observe: Spans,
    settle: Spans,
    act: Spans,
}

impl Kinds {
    /// Fills `split`'s time fields: what is outside flows by kind, and what
    /// is inside them, each moment counted once and in this order: settling,
    /// acting, Jev, reading.
    fn measure(self, split: &mut Split) {
        let flows = union(self.flows);
        let outside = union(
            [
                self.plans.clone(),
                self.rescues.clone(),
                self.person.clone(),
            ]
            .concat(),
        );
        split.flow_ms = length(&flows);
        split.planning_ms = length(&union(self.plans));
        split.rescue_ms = length(&union(self.rescues));
        split.person_ms = length(&union(self.person));
        let accounted = length(&union([flows.clone(), outside].concat()));
        split.other_ms = split.wall_ms.saturating_sub(accounted);
        let settle = union(self.settle);
        let busy = union([settle.clone(), union(self.act.clone())].concat());
        let jev = minus(&union(self.decisions), &busy);
        let observe = minus(
            &union(self.observe),
            &union([busy.clone(), jev.clone()].concat()),
        );
        split.settle_ms = length(&settle);
        split.act_ms = length(&minus(&union(self.act), &settle));
        split.jev_ms = length(&jev);
        split.observe_ms = length(&observe);
        split.flow_other_ms = split
            .flow_ms
            .saturating_sub(split.settle_ms + split.act_ms + split.jev_ms + split.observe_ms);
    }
}

/// The mean of how much longer each round of calls waited for its slowest
/// call than for its median one. A round is the calls journaled since the
/// previous decision: one decision's framings, or a batch's.
fn slowest_extra(events: &[Value]) -> u64 {
    let mut extras = Vec::new();
    let mut round = Vec::new();
    for event in events {
        match event["event"].as_str() {
            Some("exchange") => round.push(number(event, "latency_ms")),
            Some("decision") if !round.is_empty() => {
                round.sort_unstable();
                let slowest = round[round.len() - 1];
                extras.push(slowest - round[(round.len() - 1) / 2]);
                round.clear();
            }
            Some("run") => round.clear(),
            _ => {}
        }
    }
    if extras.is_empty() {
        0
    } else {
        extras.iter().sum::<u64>() / extras.len() as u64
    }
}

/// The median of each field over `splits`: what a set of runs typically
/// spent.
#[must_use]
pub fn median(splits: &[Split]) -> Split {
    let fields = splits
        .iter()
        .map(|split| serde_json::to_value(split).unwrap_or_default())
        .collect::<Vec<_>>();
    let keys = fields
        .first()
        .and_then(Value::as_object)
        .map(|first| first.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut middle = serde_json::Map::new();
    for key in keys {
        let mut values = fields
            .iter()
            .map(|split| split[&key].as_u64().unwrap_or_default())
            .collect::<Vec<_>>();
        values.sort_unstable();
        middle.insert(key, Value::from(percentile(&values, 50)));
    }
    serde_json::from_value(Value::Object(middle)).unwrap_or_default()
}

/// `split` for a terminal.
#[must_use]
pub fn render_split(split: &Split) -> String {
    let share = |ms: u64| (ms * 100).checked_div(split.wall_ms).unwrap_or_default();
    let line = |name: &str, ms: u64, note: String| {
        let line = format!("{name:<10}{} {:>3}%  {note}", seconds(ms), share(ms));
        format!("{}\n", line.trim_end())
    };
    let mut out = format!("{:<10}{}\n", "wall", seconds(split.wall_ms));
    out += &line(
        "planning",
        split.planning_ms,
        format!(
            "{} plan(s), {} model call(s)",
            split.plans, split.plan_calls
        ),
    );
    out += &line(
        "rescues",
        split.rescue_ms,
        format!(
            "{} rescue(s), {} model call(s)",
            split.rescues, split.rescue_calls
        ),
    );
    out += &line(
        "person",
        split.person_ms,
        format!("{} wait(s)", split.waits),
    );
    out += &line("flows", split.flow_ms, String::new());
    out += &line(
        "  jev",
        split.jev_ms,
        format!(
            "{} decisions, {} calls ({} failed); per call p50 {} ms, p90 {} ms; per decision p50 {} ms, p90 {} ms; the slowest call adds {} ms a round",
            split.decisions,
            split.calls,
            split.failed_calls,
            split.call_p50_ms,
            split.call_p90_ms,
            split.decision_p50_ms,
            split.decision_p90_ms,
            split.slowest_extra_ms
        ),
    );
    out += &line("  settle", split.settle_ms, String::new());
    out += &line("  act", split.act_ms, format!("{} actions", split.actions));
    out += &line("  observe", split.observe_ms, String::new());
    out += &line("  other", split.flow_other_ms, String::new());
    out += &line(
        "other",
        split.other_ms,
        "starting, shaping, and the gaps between runs".to_owned(),
    );
    let _ = writeln!(
        out,
        "units     step p50 {:.1}s, p90 {:.1}s; do turn p50 {:.1}s, p90 {:.1}s; action with settling p50 {:.1}s, p90 {:.1}s",
        secs(split.step_p50_ms),
        secs(split.step_p90_ms),
        secs(split.turn_p50_ms),
        secs(split.turn_p90_ms),
        secs(split.action_p50_ms),
        secs(split.action_p90_ms)
    );
    let _ = writeln!(
        out,
        "tokens    {} in, {} out",
        split.input_tokens, split.output_tokens
    );
    out
}

/// One line per named split, then their medians, for a terminal.
#[must_use]
pub fn render_table(splits: &[(String, Split)]) -> String {
    let mut out = format!(
        "{:<44} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>6} {:>6} {:>12} {:>12} {:>7}\n",
        "run",
        "wall",
        "plan",
        "rescue",
        "person",
        "jev",
        "settle",
        "calls",
        "decs",
        "call p50/90",
        "dec p50/90",
        "slow+"
    );
    let row = |name: &str, split: &Split| {
        format!(
            "{:<44} {:>6.1}s {:>6.1}s {:>6.1}s {:>6.1}s {:>6.1}s {:>6.1}s {:>6} {:>6} {:>12} {:>12} {:>5} ms\n",
            super::clip(name, 44),
            secs(split.wall_ms),
            secs(split.planning_ms),
            secs(split.rescue_ms),
            secs(split.person_ms),
            secs(split.jev_ms),
            secs(split.settle_ms),
            split.calls,
            split.decisions,
            format!("{}/{}", split.call_p50_ms, split.call_p90_ms),
            format!("{}/{}", split.decision_p50_ms, split.decision_p90_ms),
            split.slowest_extra_ms
        )
    };
    for (name, split) in splits {
        out += &row(name, split);
    }
    let all = splits
        .iter()
        .map(|(_, split)| split.clone())
        .collect::<Vec<_>>();
    out += &row(&format!("median of {}", splits.len()), &median(&all));
    out
}

/// The medians of two sets of runs side by side, with how `after` differs
/// from `before`, for a terminal.
#[must_use]
pub fn render_compare(before: &[Split], after: &[Split]) -> String {
    let (a, b) = (median(before), median(after));
    let (a_fields, b_fields) = (
        serde_json::to_value(&a).unwrap_or_default(),
        serde_json::to_value(&b).unwrap_or_default(),
    );
    let mut out = format!(
        "{:<20} {:>12} {:>12} {:>10}\n",
        "median of runs",
        format!("A ({})", before.len()),
        format!("B ({})", after.len()),
        "B vs A"
    );
    for key in COMPARED {
        let (x, y) = (
            a_fields[key].as_u64().unwrap_or_default(),
            b_fields[key].as_u64().unwrap_or_default(),
        );
        let change = if x == 0 {
            "-".to_owned()
        } else {
            let percent = (i128::from(y) - i128::from(x)) * 100 / i128::from(x);
            format!("{percent:+}%")
        };
        let _ = writeln!(out, "{key:<20} {x:>12} {y:>12} {change:>10}");
    }
    out
}

/// The fields a comparison lists, in order.
const COMPARED: [&str; 20] = [
    "wall_ms",
    "planning_ms",
    "rescue_ms",
    "person_ms",
    "flow_ms",
    "jev_ms",
    "settle_ms",
    "act_ms",
    "observe_ms",
    "rescues",
    "decisions",
    "calls",
    "call_p50_ms",
    "call_p90_ms",
    "decision_p50_ms",
    "decision_p90_ms",
    "slowest_extra_ms",
    "input_tokens",
    "step_p90_ms",
    "action_p50_ms",
];

/// `event`'s `at` as milliseconds since the Unix epoch.
#[must_use]
pub fn at_ms(event: &Value) -> Option<i64> {
    // `2026-10-07T06:19:52.812Z`, as the engine writes it.
    let at = event["at"].as_str()?;
    let field = |range: std::ops::Range<usize>| at.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
    let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
    let millis = if at.get(19..20) == Some(".") {
        field(20..23)?
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    Some((((days * 24 + hour) * 60 + minute) * 60 + second) * 1000 + millis)
}

/// Days from 1970-01-01 to the proleptic Gregorian date, after Howard
/// Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn union(mut spans: Spans) -> Spans {
    spans.sort_unstable();
    let mut merged: Spans = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn length(spans: &[(i64, i64)]) -> u64 {
    spans.iter().map(|(start, end)| millis(end - start)).sum()
}

/// `spans` with every moment of `cut` (a union) taken out.
fn minus(spans: &[(i64, i64)], cut: &[(i64, i64)]) -> Spans {
    let mut left = Vec::new();
    for &(start, end) in spans {
        let mut from = start;
        for &(cut_start, cut_end) in cut {
            if cut_end <= from || cut_start >= end {
                continue;
            }
            if cut_start > from {
                left.push((from, cut_start));
            }
            from = from.max(cut_end);
        }
        if from < end {
            left.push((from, end));
        }
    }
    left
}

fn millis(ms: i64) -> u64 {
    u64::try_from(ms).unwrap_or_default()
}

fn secs(ms: u64) -> f64 {
    std::time::Duration::from_millis(ms).as_secs_f64()
}
