//! What a task journals of the time it spends outside its flows: a plan, a
//! rescue, and a wait for the person to answer. With these, a task's
//! journal accounts for all of its time, not only its flows'
//! (`docs/technical/jev-journal.md`).

use std::time::Duration;

use serde_json::{Value, json};
use tinycomputer_bus::agent::{LanguageModelConfiguration, Rescue, TaskPlan};

use crate::planner::ModelUse;
use crate::rescue::Guidance;

/// The `plan` event: how long planning took, what it used of its model, and
/// what came of it.
pub(super) fn planned(
    outcome: &Result<TaskPlan, String>,
    used: ModelUse,
    took: Duration,
    model: Option<&LanguageModelConfiguration>,
) -> Value {
    let mut fields = json!({
        "wall_ms": millis(took),
        "calls": used.calls,
        "sent_bytes": used.sent_bytes,
        "model": model.map(|model| model.model.as_str()),
        "ok": outcome.is_ok(),
    });
    match outcome {
        Ok(plan) => {
            fields["steps"] = json!(plan.flow.steps.len());
            fields["questions"] = json!(plan.questions.len());
        }
        Err(reason) => fields["error"] = json!(reason),
    }
    fields
}

/// What a rescue's answer came to: guidance to run, the rescuer giving up,
/// its model failing, or no answer in time.
pub(super) fn answered(answer: &Result<Guidance, String>, timed_out: bool) -> &'static str {
    match answer {
        Ok(Guidance::Retry { .. }) => "guided",
        Ok(Guidance::GiveUp { .. }) => "gave_up",
        Err(_) if timed_out => "timeout",
        Err(_) => "error",
    }
}

/// What one rescue recorded and what asking for it took.
pub(super) struct Rescued<'a> {
    /// Which rescue of the task this is, from 1, and how many it may have.
    pub(super) attempt: usize,
    /// See [`Rescued::attempt`].
    pub(super) limit: u32,
    /// How long asking took.
    pub(super) took: Duration,
    /// What it used of its model; `None` when it gave no answer in time.
    pub(super) used: Option<ModelUse>,
    /// How the answer went ([`answered`]).
    pub(super) outcome: &'static str,
    /// The rescue as the task recorded it.
    pub(super) record: &'a Rescue,
    /// The rescuer's model, when known.
    pub(super) model: Option<&'a LanguageModelConfiguration>,
}

/// The `rescue` event: the failed step, how long asking took, what it used
/// of the model, and what came back.
pub(super) fn rescued(rescue: &Rescued<'_>) -> Value {
    json!({
        "step": rescue.record.step + 1,
        "attempt": rescue.attempt,
        "limit": rescue.limit,
        "wall_ms": millis(rescue.took),
        "calls": rescue.used.map(|used| used.calls),
        "sent_bytes": rescue.used.map(|used| used.sent_bytes),
        "outcome": rescue.outcome,
        "steps": rescue.record.steps.len(),
        "covers": rescue.record.covers,
        "model": rescue.model.map(|model| model.model.as_str()),
    })
}

/// The `resume` event: what the task waited at, and for how long, before
/// the answer came.
pub(super) fn resumed(state: &str, waited: Duration) -> Value {
    json!({"state": state, "waited_ms": millis(waited)})
}

/// Whole milliseconds in `duration`, saturating.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
