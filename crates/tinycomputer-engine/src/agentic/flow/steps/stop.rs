//! The `stop_before` step: finding an irreversible control and stopping in
//! front of it, or pressing it when the run allows destructive actions.

use tinycomputer_bus::{FlowLoop, FlowStopReason, JevOperation, StepOutcome};

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::DONE,
    backend::AgentBackend,
    memory::{learn, remember},
    view::{label, target_payload},
};

use super::{IRREVERSIBLE_FLOOR, LOCATE_FLOOR, matching::clickable};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    pub(super) async fn stop_before(
        &mut self,
        log: &mut StepLog,
        action: &str,
    ) -> Result<Ended, Halt> {
        // Signing in is no irreversible action, and a login wall pauses for
        // a person by itself (`tinycomputer_core::safety`): a stop before it
        // gates nothing, and on any page it finds a header's sign-in link.
        // Live, a plan for "do not log in" stopped short of the cart, at
        // "Hello, sign in".
        if only_signs_in(action) {
            self.history.push(format!(
                "did not stop before {action:?}: signing in is no irreversible action, and a login wall pauses for a person by itself"
            ));
            return Ok(Ended::new(
                StepOutcome::Done,
                "signing in is no irreversible action: nothing to stop before".to_owned(),
            ));
        }
        let purpose = format!("perform: {action}");
        // Asked to "perform: paying", Jev weighs the request against the
        // brief's own rule to stop before paying and hesitates (measured:
        // 0.44 on the Pay button); asked to find it without pressing it,
        // which is all a gated step does, it answers 1.0.
        let question = if self.allow_destructive {
            purpose.clone()
        } else {
            format!("find, without pressing it, the control that would perform: {action}")
        };
        let screen = self.look().await?;
        let pool = clickable(&screen.candidates);
        let Some(grounded) = self
            .ground(log, &screen, &question, &purpose, pool)
            .await?
            .filter(|grounded| grounded.confidence >= LOCATE_FLOOR)
        else {
            return Err(Halt::Failed(format!(
                "the control that performs {action:?} was not found"
            )));
        };
        log.confidence = Some(grounded.confidence);
        let target = grounded.candidate;
        if !self.allow_destructive {
            self.pending = Some(target_payload(&target));
            self.history.push(format!(
                "found {} for {action:?} and stopped in front of it",
                label(&target)
            ));
            return Err(Halt::Stop(FlowStopReason::StoppedBeforeDestructive));
        }
        if self.deep()
            && self.deliberates(FlowLoop::Evidence)
            && grounded.confidence < IRREVERSIBLE_FLOOR
        {
            // Nothing undoes this press: the bar is higher than for any
            // other, and a pick short of it is vouched for once more.
            let vouched = self.vouch(log, &screen, &purpose, &target).await?;
            log.confidence = Some(vouched);
            if vouched < IRREVERSIBLE_FLOOR {
                return Err(Halt::Failed(format!(
                    "will not press {} irreversibly on uncertain evidence (confidence {vouched:.2})",
                    label(&target)
                )));
            }
        }
        let clicked = target.clone();
        let reply = self
            .act(log, "click (irreversible)", Some(&target), move |backend| {
                backend.execute(JevOperation::Click, Some(clicked), None)
            })
            .await?;
        if !reply.ok {
            return Err(Halt::Failed(format!(
                "{} could not be pressed",
                label(&target)
            )));
        }
        learn(&mut self.learned, remember(&self.app, &purpose, &target));
        let happened = self.holds(log, &format!("{action} has happened")).await?;
        if happened >= DONE {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("performed {action:?}"),
            ))
        } else {
            Err(Halt::Failed(format!(
                "pressed {} but {action:?} is not visibly done (confidence {happened:.2})",
                label(&target)
            )))
        }
    }
}

/// Phrases that name signing in to an account that exists. Signing up,
/// registering, or creating an account hands the person's details to a
/// site, and stays gated.
const SIGNING_IN: &[&str] = &[
    "logging in",
    "log in",
    "login",
    "signing in",
    "sign in",
    "signin",
];

/// Words that add nothing beside such a phrase ("signing in to your
/// account").
const SIGN_IN_FILLER: &[&str] = &[
    "the", "a", "an", "to", "your", "my", "or", "and", "with", "using", "via", "account", "page",
    "screen", "button", "form", "before", "otp",
];

/// Whether `action` names signing in and nothing else: "signing in",
/// "logging in to your account", but not "paying or logging in", nor
/// "signing up or logging in".
pub(in crate::agentic::flow) fn only_signs_in(action: &str) -> bool {
    let words = action
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    let mut text = format!(
        " {} ",
        words.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    let mut found = false;
    for phrase in SIGNING_IN {
        let padded = format!(" {phrase} ");
        while text.contains(&padded) {
            text = text.replacen(&padded, " ", 1);
            found = true;
        }
    }
    found
        && text
            .split_whitespace()
            .all(|word| SIGN_IN_FILLER.contains(&word))
}
