//! Steps that judge a condition on screen: `verify`, `wait_for`,
//! `repeat_until`, and `if`.

use tinycomputer_bus::{FlowLoop, IfStep, JevOperation, RepeatStep, StepOutcome};

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    act::{DONE, SCREEN_VIEW},
    ask::{self, Questions, condition},
    backend::AgentBackend,
    escalate::Belief,
    validate::{MAX_REPEAT, substitute_safe},
    view::{Screen, fingerprint},
};

use super::{EMPTY_CHECKS, STEADY_CHECKS, STEADY_HOLD, WAIT_CHECKS, matching::plain};

/// What a page says when a search found nothing, as whole-word phrases in
/// its title or its visible text: what a `wait_for` waits for will not come.
const FOUND_NOTHING: &[&str] = &[
    "no results",
    "no result found",
    "0 results",
    "no products found",
    "no items found",
    "no matches found",
    "no matching results",
    "nothing found",
    "did not match any",
    "could not find any",
    "couldn t find any",
    "no matching products",
    "no products",
    "0 products",
];

/// The phrase of [`FOUND_NOTHING`] `screen` shows in its title or visible
/// text, if any. Field contents are not read.
fn found_nothing(screen: &Screen) -> Option<&'static str> {
    let shown = screen
        .window
        .iter()
        .chain(&screen.context)
        .map(|text| format!(" {} ", plain(text)))
        .collect::<Vec<_>>();
    FOUND_NOTHING.iter().copied().find(|phrase| {
        shown
            .iter()
            .any(|text| text.contains(&format!(" {phrase} ")))
    })
}

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Judges one condition on the current screen.
    ///
    /// A deliberating run settles a judgement near [`DONE`] on its evidence
    /// (`escalate::settle_belief`), at the deep level also asking it over
    /// the screen alone, without the history that can lead it. A hedged
    /// yes/no defers to a crisp coverage answer (`ask::deferred`), in every
    /// view.
    pub(in crate::agentic::flow) async fn holds(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<f64, Halt> {
        self.holds_on(log, condition_text)
            .await
            .map(|(held, _)| held)
    }

    /// [`FlowRun::holds`], with the screen it was judged on.
    async fn holds_on(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<(f64, Screen), Halt> {
        log.used(FlowLoop::Completion);
        let screen = self.look().await?;
        let request = ask::request(
            self.model(),
            self.state(&screen, condition_text),
            Questions::default()
                .with("holds", condition(condition_text))
                .with("negated", ask::negated(condition_text))
                .with("coverage", ask::coverage(condition_text)),
        );
        let mut answers = self.ask(log, request.clone()).await?;
        let belief = Belief {
            site: "holds",
            yes: "holds",
            no: "negated",
            top: Some("coverage"),
            threshold: DONE,
            defers: true,
        };
        let views = if self.deep() {
            vec![ask::request(
                self.model(),
                ask::state(&screen, condition_text, &[], self.include_values),
                Questions::default()
                    .with("holds", ask::viewed(condition(condition_text), SCREEN_VIEW))
                    .with(
                        "negated",
                        ask::viewed(ask::negated(condition_text), SCREEN_VIEW),
                    )
                    .with(
                        "coverage",
                        ask::viewed(ask::coverage(condition_text), SCREEN_VIEW),
                    ),
            )]
        } else {
            Vec::new()
        };
        let held = self
            .settle_belief(log, belief, &request, &mut answers, views)
            .await?
            .unwrap_or_default();
        log.confidence = Some(held);
        Ok((held, screen))
    }

    pub(super) async fn verify(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<Ended, Halt> {
        let held = self.holds(log, condition_text).await?;
        if held >= DONE {
            Ok(Ended::new(
                StepOutcome::Done,
                format!("holds (confidence {held:.2})"),
            ))
        } else {
            Err(Halt::Failed(format!(
                "does not hold (confidence {held:.2})"
            )))
        }
    }

    /// Waits for a condition, checking it up to [`WAIT_CHECKS`] times. A
    /// page that says it found nothing ([`FOUND_NOTHING`]) on
    /// [`EMPTY_CHECKS`] checks in a row will not turn up what the step waits
    /// for, so the step fails there and says so: live, a store's "No Results
    /// Found" page was checked ten times over, and the rescue, told only that
    /// the condition never held, guessed at the search's wording.
    pub(super) async fn wait_for(
        &mut self,
        log: &mut StepLog,
        condition_text: &str,
    ) -> Result<Ended, Halt> {
        let mut empty = 0;
        // A settled screen judged likely to show the condition, check after
        // check, will not be judged otherwise by waiting longer: live, a
        // results page was judged to show its results at 0.70 to 0.80 on
        // every one of ten checks, under the bar each time.
        let mut steady = (0, String::new());
        for check in 0..WAIT_CHECKS {
            let (held, screen) = self.holds_on(log, condition_text).await?;
            if held >= DONE {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!("held after {} check(s)", check + 1),
                ));
            }
            let seen = fingerprint(&screen);
            steady = if held >= STEADY_HOLD && (steady.0 == 0 || steady.1 == seen) {
                (steady.0 + 1, seen)
            } else if held >= STEADY_HOLD {
                (1, seen)
            } else {
                (0, String::new())
            };
            if steady.0 >= STEADY_CHECKS {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!(
                        "held on {STEADY_CHECKS} checks of a settled screen (confidence {held:.2})"
                    ),
                ));
            }
            // A dialog the task opened asks its question first (a format, a
            // quantity): what lies past it will not show while it waits.
            if self.front.opened_dialog && check >= 1 {
                return Err(Halt::Failed(
                    "a dialog the task opened is waiting for an answer, so nothing past it shows: choose what it asks, then continue"
                        .to_owned(),
                ));
            }
            match found_nothing(&screen) {
                Some(phrase) => {
                    empty += 1;
                    if empty >= EMPTY_CHECKS {
                        return Err(Halt::Failed(format!(
                            "the page says {phrase:?}: it found nothing, so the condition will not hold"
                        )));
                    }
                }
                None => empty = 0,
            }
            self.act(log, "wait", None, |backend| {
                backend.execute(JevOperation::Wait, None, None)
            })
            .await?;
        }
        Err(Halt::Failed(format!(
            "still not true after {WAIT_CHECKS} checks"
        )))
    }

    pub(super) async fn repeat(
        &mut self,
        log: &mut StepLog,
        repeat: &RepeatStep,
        path: &str,
    ) -> Result<Ended, Halt> {
        let condition_text = substitute_safe(&repeat.condition, &self.vars, &self.facts);
        for round in 0..repeat.max.min(MAX_REPEAT) {
            if self.holds(log, &condition_text).await? >= DONE {
                return Ok(Ended::new(
                    StepOutcome::Done,
                    format!("held after {round} round(s)"),
                ));
            }
            self.run_steps(&repeat.steps, format!("{path}.r{}", round + 1))
                .await?;
            // The last child left `self.step` at its own nested path; restore
            // it before the next `holds` check so that call, and the final
            // one below on the last round, are traced to this repeat_until
            // step rather than misattributed to the child that just ran.
            path.clone_into(&mut self.step);
        }
        if self.holds(log, &condition_text).await? >= DONE {
            return Ok(Ended::new(StepOutcome::Done, "held after the last round"));
        }
        Err(Halt::Failed(format!(
            "still not true after {} round(s)",
            repeat.max.min(MAX_REPEAT)
        )))
    }

    pub(super) async fn branch(
        &mut self,
        log: &mut StepLog,
        branch: &IfStep,
        path: &str,
    ) -> Result<Ended, Halt> {
        let condition_text = substitute_safe(&branch.condition, &self.vars, &self.facts);
        let held = self.holds(log, &condition_text).await?;
        let (steps, taken) = if held >= DONE {
            (&branch.then, "then")
        } else {
            (&branch.otherwise, "else")
        };
        self.run_steps(steps, path.to_owned()).await?;
        Ok(Ended::new(
            StepOutcome::Done,
            format!("took the {taken} branch (confidence {held:.2})"),
        ))
    }
}
