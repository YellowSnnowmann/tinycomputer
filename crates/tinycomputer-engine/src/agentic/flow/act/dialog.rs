//! Answering the dialog the task opened when nothing on screen serves the
//! step itself: the dialog's question comes first.

use std::collections::BTreeSet;

use crate::agentic::flow::{
    FlowRun, Halt, StepLog,
    backend::AgentBackend,
    ground::Grounded,
    view::{Candidate, Screen},
};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Grounds the control that answers the dialog in front, which the
    /// task's own press opened, among the controls a press reaches now
    /// (never the dialog's close control): a booking's format dialog asks
    /// for a format before its dates show, and a format serves no date
    /// step's words. Live, "choose Wednesday 7 October" found nothing to
    /// press for four turns while the dialog offered "2D", and a rescue
    /// was spent pressing it.
    ///
    /// Only on a browser task, only among the dialog's own controls when it
    /// is a dialog, and never a control that commits ("Yes", "Confirm",
    /// "Pay"): a press here serves no step's words, so it must not be one
    /// a person would want to approve. On a desktop application a sheet's
    /// answer ("Don't Save") is the step's to name.
    pub(super) async fn answer_dialog(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        intent: &str,
        banned: &BTreeSet<String>,
    ) -> Result<Option<Grounded>, Halt> {
        if !self.app.eq_ignore_ascii_case("browser") {
            return Ok(None);
        }
        let pool = self
            .pool(screen, "Click", banned, intent)
            .into_iter()
            .filter(|candidate| {
                !candidate
                    .states
                    .iter()
                    .any(|state| state.eq_ignore_ascii_case("offscreen"))
                    && !commits(candidate)
            })
            .collect::<Vec<_>>();
        let inside = pool
            .iter()
            .filter(|candidate| in_dialog(candidate))
            .cloned()
            .collect::<Vec<_>>();
        let pool = if inside.is_empty() { pool } else { inside };
        if pool.is_empty() {
            return Ok(None);
        }
        let purpose = format!(
            "click to answer the dialog in front, which this task opened, the way the task wants, before the task goes on to: {intent}"
        );
        let key = format!("{intent} (the dialog in front)");
        self.ground(log, screen, &purpose, &key, pool).await
    }
}

/// Whether `candidate` sits in a dialog: its path names one.
pub(super) fn in_dialog(candidate: &Candidate) -> bool {
    candidate.path.iter().any(|segment| {
        segment == "dialog"
            || segment == "alertdialog"
            || segment.starts_with("dialog ")
            || segment.starts_with("alertdialog ")
    })
}

/// Words of a control that commits what a dialog asks, rather than
/// answering it: what a person approves, never a fallback's press.
const COMMITS: &[&str] = &[
    "yes", "ok", "okay", "confirm", "submit", "accept", "agree", "pay", "book", "buy", "order",
    "checkout", "reserve", "send", "delete", "remove", "proceed", "allow",
];

/// Whether `candidate` commits what its dialog asks: one of its words is
/// one of [`COMMITS`].
fn commits(candidate: &Candidate) -> bool {
    candidate
        .name
        .as_deref()
        .or(candidate.description.as_deref())
        .unwrap_or_default()
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| COMMITS.contains(&word.to_ascii_lowercase().as_str()))
}
