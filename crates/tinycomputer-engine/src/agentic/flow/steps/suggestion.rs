//! Committing an autocomplete: picking the suggestion a box listed for the
//! text just typed into it, before anything moves the focus away.

use std::collections::BTreeSet;

use tinycomputer_bus::JevOperation;

use crate::agentic::flow::{
    FlowRun, Halt, StepLog,
    backend::AgentBackend,
    ground::Grounded,
    view::{Candidate, Screen, is_destructive, label},
};

use super::matching::{clickable, closest, editable, mentions, one_option, plainest};

/// Roles a suggestion list draws its rows with. A row that does not mention
/// the typed text is only offered when it carries one of these, so a button
/// that appeared beside the box ("Clear") is never mistaken for a match.
const SUGGESTION_ROLES: &[&str] = &["option", "menuitem", "listitem", "row", "gridcell"];

/// Most new rows one pick is asked over.
const MOST_SUGGESTIONS: usize = 12;

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// After `text` went into `field`, picks the suggestion the box listed
    /// for it, as a person does. A location, city, or airport box that lists
    /// matches under it keeps the text only once one is chosen, and drops it
    /// as soon as the focus moves on: live, a pickup box emptied when the next
    /// step pressed Escape on its open list.
    ///
    /// Only rows that appeared since `before`, the screen as it stood before
    /// the text was typed, are offered, so a list the page showed anyway is
    /// never touched and nothing happens when typing opened none. Rows that
    /// mention the text come first; when none does, a differently worded
    /// suggestion ("IGI Airport" for "Indira Gandhi International Airport")
    /// is matched among the new rows a list draws. Jev may answer that none
    /// fits, which leaves the text as typed.
    pub(in crate::agentic::flow) async fn commit_suggestion(
        &mut self,
        log: &mut StepLog,
        slot: &str,
        text: &str,
        field: &Candidate,
        before: &Screen,
    ) -> Result<(), Halt> {
        let screen = self.look().await?;
        let shown = before
            .candidates
            .iter()
            .map(|candidate| (candidate.role.as_str(), candidate.name.as_deref()))
            .collect::<BTreeSet<_>>();
        let fresh = clickable(&screen.candidates)
            .into_iter()
            .filter(|candidate| {
                candidate.ref_id != field.ref_id
                    && !editable(candidate)
                    && !shown.contains(&(candidate.role.as_str(), candidate.name.as_deref()))
                    && !is_destructive(candidate, &screen, &self.stop_before)
            })
            .collect::<Vec<_>>();
        let mentioned = fresh
            .iter()
            .filter(|candidate| mentions(candidate, text))
            .cloned()
            .collect::<Vec<_>>();
        let pool = if mentioned.is_empty() {
            fresh
                .into_iter()
                .filter(|candidate| {
                    SUGGESTION_ROLES
                        .iter()
                        .any(|role| candidate.role.eq_ignore_ascii_case(role))
                })
                .take(MOST_SUGGESTIONS)
                .collect::<Vec<_>>()
        } else {
            closest(mentioned)
                .into_iter()
                .take(MOST_SUGGESTIONS)
                .collect::<Vec<_>>()
        };
        if pool.is_empty() {
            return Ok(());
        }
        let purpose = format!("pick the suggestion that completes the {slot} as {text:?}");
        let grounded = if one_option(&pool) {
            plainest(pool).map(|candidate| Grounded {
                candidate,
                confidence: 1.0,
            })
        } else {
            self.ground(log, &screen, &purpose, &format!("{slot} suggestion"), pool)
                .await?
        };
        let Some(grounded) = grounded else {
            self.history
                .push(format!("no suggestion fit the {slot}; it stays as typed"));
            return Ok(());
        };
        let target = grounded.candidate;
        let clicked = target.clone();
        let reply = self
            .act(
                log,
                &format!("pick the suggestion for the {slot}"),
                Some(&target),
                move |backend| backend.execute(JevOperation::Click, Some(clicked), None),
            )
            .await?;
        self.history.push(if reply.ok {
            format!("picked the suggestion {} for the {slot}", label(&target))
        } else {
            format!("could not pick the suggestion for the {slot}")
        });
        Ok(())
    }
}
