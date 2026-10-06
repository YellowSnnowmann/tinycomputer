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
    pub(super) async fn answer_dialog(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        intent: &str,
        banned: &BTreeSet<String>,
    ) -> Result<Option<Grounded>, Halt> {
        let pool = self
            .pool(screen, "Click", banned, intent)
            .into_iter()
            .filter(|candidate| {
                !candidate
                    .states
                    .iter()
                    .any(|state| state.eq_ignore_ascii_case("offscreen"))
            })
            .collect::<Vec<_>>();
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
