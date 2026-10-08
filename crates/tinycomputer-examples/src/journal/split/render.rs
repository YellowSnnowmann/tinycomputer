//! Splits rendered for a terminal: one task's, a table of several, and two
//! sets side by side.

use std::fmt::Write as _;

use super::super::seconds;
use super::time::secs;
use super::{Split, median};

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
        "tokens    {} in, {} out; Jev cost {}",
        split.input_tokens,
        split.output_tokens,
        dollars(split.jev_cost_micro_usd)
    );
    out
}

/// `micro_usd` millionths of a dollar, to a hundredth of a cent.
fn dollars(micro_usd: u64) -> String {
    format!(
        "${}.{:04}",
        micro_usd / 1_000_000,
        micro_usd % 1_000_000 / 100
    )
}

/// One line per named split, then their medians, for a terminal.
#[must_use]
pub fn render_table(splits: &[(String, Split)]) -> String {
    let mut out = format!(
        "{:<44} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>6} {:>6} {:>12} {:>12} {:>7} {:>8}\n",
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
        "slow+",
        "jev cost"
    );
    let row = |name: &str, split: &Split| {
        format!(
            "{:<44} {:>6.1}s {:>6.1}s {:>6.1}s {:>6.1}s {:>6.1}s {:>6.1}s {:>6} {:>6} {:>12} {:>12} {:>5} ms {:>8}\n",
            super::super::clip(name, 44),
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
            split.slowest_extra_ms,
            dollars(split.jev_cost_micro_usd)
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
const COMPARED: [&str; 21] = [
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
    "jev_cost_micro_usd",
    "step_p90_ms",
    "action_p50_ms",
];
