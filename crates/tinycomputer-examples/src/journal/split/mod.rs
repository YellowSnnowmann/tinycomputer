//! Where a whole task's time went, and two sets of tasks side by side.
//!
//! A task journals each of its flows, and since `plan`, `rescue`, and
//! `resume` events, the time between them too. Each run of a file restarts
//! `elapsed_ms`, so the split is built from the absolute `at` timestamps
//! instead: every event that took time becomes an interval ending at `at`,
//! and overlapping intervals (the framings of one decision, a batch of
//! decisions) are counted once.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{number, percentile};

mod render;
mod time;

pub use render::{render_compare, render_split, render_table};
pub use time::at_ms;
use time::{Spans, length, millis, minus, union};

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
/// previous decision: one decision's framings, or a batch's. The framings a
/// quorum did not wait for (its `left`) journal after their decision, and
/// are no round's.
fn slowest_extra(events: &[Value]) -> u64 {
    let mut extras = Vec::new();
    let mut round = Vec::new();
    let mut late = 0;
    for event in events {
        match event["event"].as_str() {
            Some("exchange") if late > 0 => late -= 1,
            Some("exchange") => round.push(number(event, "latency_ms")),
            Some("decision") => {
                if !round.is_empty() {
                    round.sort_unstable();
                    let slowest = round[round.len() - 1];
                    extras.push(slowest - round[(round.len() - 1) / 2]);
                    round.clear();
                }
                late = number(event, "left");
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
