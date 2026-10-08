//! Pressing a target the page refuses as covered: closing what lies over
//! it, then pressing the same target once more.

use tinycomputer_bus::JevOperation;

use crate::agentic::flow::{
    FlowRun, Halt, StepLog,
    attention::front_closer,
    backend::AgentBackend,
    view::{Candidate, label, signature},
};

use super::covered;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Performs `operation` on an already-vetted `target`. When the click
    /// is refused because something covers it — a drawer, a menu, a consent
    /// banner, or a result card's own click layer — closes what covers it
    /// once and tries the same target again: with the least committal
    /// control of a layer in front ([`front_closer`]; never a layer the
    /// step's `intent` names, or the one `target` sits in), or else with
    /// Escape. Neither chooses a new target.
    pub(in crate::agentic::flow) async fn press_uncovering(
        &mut self,
        log: &mut StepLog,
        verb: &str,
        target: &Candidate,
        operation: JevOperation,
        intent: &str,
    ) -> Result<tinycomputer_bus::DesktopResponse, Halt> {
        // Read before this press, which would count as one in the dialog.
        let served = self.front.served_calendar();
        let chosen = target.clone();
        let reply = self
            .act(log, verb, Some(target), move |backend| {
                backend.execute(operation, Some(chosen), None)
            })
            .await?;
        if !covered(&reply) {
            return Ok(reply);
        }
        // A dialog in front is the page's question (a format, a quantity),
        // not a popover in the way: Escape would close it, and pressing what
        // lies behind it leaves the flow it began (live, a movie's language
        // link behind its booking dialog led to a listing of other films).
        // A layer drawn over the window is such a question only when the
        // task's own press opened it; a calendar left open is in the way,
        // and so is one the task opened and has pressed in since: live, a
        // calendar stayed in front of the guests and Search buttons once
        // both dates were picked, and every press behind it was refused.
        if (self.front.opened_dialog()
            || !matches!(self.front.surface.as_str(), "window" | "layer"))
            && !served
        {
            self.history.push(format!(
                "{} lies behind the dialog in front; act within the dialog instead",
                label(target)
            ));
            return Ok(reply);
        }
        // A layer in front that closes with a control of its own (a consent
        // banner's "Allow Selection") is closed with it: Escape leaves such a
        // banner where it is.
        let screen = self.look().await?;
        if let Some(closer) = front_closer(
            &screen,
            target,
            intent,
            &self.stop_before,
            &self.step_cleared,
        ) {
            self.step_cleared.insert(signature(&closer));
            let pressed = closer.clone();
            self.act(log, "click (uncover)", Some(&closer), move |backend| {
                backend.execute(JevOperation::Click, Some(pressed), None)
            })
            .await?;
            self.history.push(format!(
                "{} was covered by a layer in front; pressed {} to close it",
                label(target),
                label(&closer)
            ));
        } else {
            let app = self.app.clone();
            self.act(log, "press escape (uncover)", None, move |backend| {
                backend.press(&app, "escape")
            })
            .await?;
            self.history.push(format!(
                "{} was covered by something; pressed escape to close it",
                label(target)
            ));
        }
        let retried = target.clone();
        self.act(log, verb, Some(target), move |backend| {
            backend.execute(operation, Some(retried), None)
        })
        .await
    }
}
