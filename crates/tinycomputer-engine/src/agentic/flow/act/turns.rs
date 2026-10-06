//! The turn loop of a `do` step: judge the screen, recover from a bad
//! turn, make a move, and note what changed.

use std::time::Instant;

use serde_json::json;
use tinycomputer_bus::StepOutcome;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    backend::AgentBackend,
    view::{Candidate, Screen, change_note, fingerprint, label, press_key, signature},
};

use super::{
    DONE, DoState, Expected, LastAction, MAX_IDLE_SCROLLS, MAX_IDLE_WAITS, MAX_REPEAT_PRESSES,
    Move, STALL_TURNS, asks_for_every, asks_for_several, closed_the_overlay, creates_new,
    finish_floor, finished, judge::Judgement,
};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Runs the `do` loop for `intent` for at most `max_turns` turns.
    pub(in crate::agentic::flow) async fn accomplish(
        &mut self,
        log: &mut StepLog,
        intent: &str,
        max_turns: u32,
    ) -> Result<Ended, Halt> {
        let mut state = DoState::default();
        let ended = self.turns(log, &mut state, intent, max_turns).await;
        self.end_turn(&mut state);
        // A step that stalls in front of a dialog the task opened says so,
        // and names the controls it offers, so a rescue answers the
        // dialog's question with one of them rather than plan past it, or
        // choose a heading in it: live, rescues kept choosing a language
        // heading above a format dialog's buttons.
        match ended {
            Err(Halt::Failed(note)) if self.front.opened_dialog => {
                let offers = match self.look().await {
                    Ok(screen) => front_controls(&screen),
                    Err(_) => Vec::new(),
                };
                let offers = if offers.is_empty() {
                    String::new()
                } else {
                    format!("; its own controls are: {}", offers.join(", "))
                };
                Err(Halt::Failed(format!(
                    "{note}; a dialog the task opened is in front, waiting for an answer: choose what it asks first{offers}"
                )))
            }
            other => other,
        }
    }

    /// Journals the turn under way, if any: how many decisions it took and
    /// how long it ran.
    fn end_turn(&self, state: &mut DoState) {
        let Some((turn, started, before, rounds_before)) = state.turn.take() else {
            return;
        };
        self.runtime.journal.record("turn", || {
            json!({
                "step": self.step,
                "turn": turn,
                "decisions": self.decisions.saturating_sub(before),
                "rounds": self.rounds.saturating_sub(rounds_before),
                "wall_ms": crate::agentic::journal::millis(started.elapsed()),
            })
        });
    }

    async fn turns(
        &mut self,
        log: &mut StepLog,
        state: &mut DoState,
        intent: &str,
        max_turns: u32,
    ) -> Result<Ended, Halt> {
        for turn in 0..max_turns {
            self.end_turn(state);
            state.turn = Some((turn, Instant::now(), self.decisions, self.rounds));
            log.turns = log.turns.saturating_add(1);
            let screen = self.look().await?;
            self.note_change(state, &screen)?;
            self.note_oscillation(log, state, &screen);
            if state.first.is_none() {
                state.first = Some(screen.clone());
            }
            if let Some(ended) = state
                .last
                .as_ref()
                .and_then(|last| closed_the_overlay(last, &screen, intent))
            {
                return Ok(ended);
            }
            // The root of the turn's tree: what needs attention first. A
            // distraction cleared means a fresh look before judging. A dialog
            // the step's own press just opened is the step's to work in.
            if !self.front.opened_dialog
                && self
                    .attend(log, &screen, intent, &mut state.cleared)
                    .await?
            {
                state.last = None;
                continue;
            }
            self.check_expectation(log, state, &screen);
            let judged = self.judge_turn(log, state, &screen, intent).await?;
            let mut judged = self
                .settle_done(log, state, &screen, intent, judged, turn)
                .await?;
            if judged.next == "finished"
                && judged.done.is_some_and(|done| done < finish_floor(turn))
            {
                // The move chooser's "finished" is one vote. Before anything
                // is done it needs the completion judge's full bar, since
                // skipping a step derails the rest; after acting it stands
                // unless the judge leans the other way.
                self.history.push(
                    "the screen does not yet clearly show this step done; act on it".to_owned(),
                );
                "activate".clone_into(&mut judged.next);
            }
            let creating = creates_new(intent) && log.actions.is_empty();
            if !creating && let Some(ended) = finished(log, &judged, turn) {
                return Ok(ended);
            }
            if creating && judged.next == "finished" {
                self.history.push(
                    "this step creates something new, so something already on screen cannot count; act first"
                        .to_owned(),
                );
            }
            if self.recover(log, state, &screen, intent, &judged).await? {
                continue;
            }
            if judged.next == "scroll" && state.idle_scrolls >= MAX_IDLE_SCROLLS {
                self.history.push(
                    "did not scroll again: scrolling showed nothing new, so act on what is listed"
                        .to_owned(),
                );
                "activate".clone_into(&mut judged.next);
            }
            if judged.next == "wait" && state.idle_waits >= MAX_IDLE_WAITS {
                self.history.push(
                    "did not wait again: the page has settled, so judge it as it is or act on it"
                        .to_owned(),
                );
                continue;
            }
            let branch = state.branch.take();
            match self
                .make_move(log, &screen, intent, &judged, &state.banned, branch)
                .await?
            {
                Move::Ended(ended) => return Ok(ended),
                Move::Acted(target, expected) => {
                    self.acted(state, screen, intent, &judged, (target, expected));
                }
                Move::Skipped => {}
            }
        }
        let screen = self.look().await?;
        // A dismissal on the very last permitted turn leaves no other
        // evidence once the loop stops: the overlay is gone, but the
        // completion judge sees only the screen after the fact. Apply the
        // same check here that runs at the top of every earlier turn, or a
        // dismissal that succeeded on the last turn is reported as failed.
        if let Some(ended) = state
            .last
            .as_ref()
            .and_then(|last| closed_the_overlay(last, &screen, intent))
        {
            return Ok(ended);
        }
        let judged = self.judge(log, &screen, intent, None).await?;
        let judged = self
            .settle_done(log, state, &screen, intent, judged, max_turns)
            .await?;
        if judged.done.unwrap_or_default() >= DONE {
            return Ok(Ended::new(
                StepOutcome::Done,
                "accomplished on the last turn",
            ));
        }
        Err(Halt::Failed(format!(
            "not accomplished after {max_turns} turns"
        )))
    }

    /// Records the move just made on `screen` as the step's last action:
    /// the press counted, a repeated key capped like a control (Return on a
    /// search that lists results as it is typed), and what the next turn
    /// weighs the move by.
    fn acted(
        &mut self,
        state: &mut DoState,
        screen: Screen,
        intent: &str,
        judged: &Judgement,
        (target, expected): (Option<Box<Candidate>>, Option<Box<Expected>>),
    ) {
        if let Some(pressed) = target.as_deref() {
            self.note_press(state, &screen, pressed, intent);
        }
        if judged.next == "shortcut"
            && let Some((combo, _)) = judged.shortcut
        {
            let key = format!("key:{combo}");
            let count = state.presses.entry(key.clone()).or_default();
            *count = count.saturating_add(1);
            if *count >= MAX_REPEAT_PRESSES {
                state.banned.insert(key);
            }
        }
        state.pressed_before = state.last.as_ref().and_then(|last| last.target.clone());
        state.last = Some(LastAction {
            target: target.map(|target| *target),
            before: screen,
            progress: judged.progress,
            waited: judged.next == "wait",
            scrolled: judged.next == "scroll",
            expected: expected.map(|expected| *expected),
            outcome: None,
        });
    }

    /// Counts a press of `pressed` on `screen`, and strikes off what the step
    /// must not press next: the control itself once pressed
    /// [`MAX_REPEAT_PRESSES`] times, and its copies on other items at once.
    ///
    /// A list repeats a named button on every item ("Add" on each product
    /// card), and once one is pressed, another copy acts on a different
    /// item: live, a step adding two packets of one milk pressed "Add" on six
    /// products. Unless the step asks for every item or chooses several
    /// (two seats), the copies are left alone; a stepper or the pressed
    /// control itself still raises a count.
    fn note_press(
        &mut self,
        state: &mut DoState,
        screen: &Screen,
        pressed: &Candidate,
        intent: &str,
    ) {
        let key = press_key(pressed);
        let count = state.presses.entry(key.clone()).or_default();
        *count = count.saturating_add(1);
        if *count >= MAX_REPEAT_PRESSES && state.banned.insert(key) {
            self.ledger.tried(format!(
                "pressed {} {MAX_REPEAT_PRESSES} times in one step",
                label(pressed)
            ));
            self.history.push(format!(
                "pressed {} {MAX_REPEAT_PRESSES} times; not pressing it again in this step: judge whether the step is done, or act on something else",
                label(pressed)
            ));
        }
        if (pressed.name.is_none() && pressed.description.is_none())
            || asks_for_every(intent)
            || asks_for_several(intent)
        {
            return;
        }
        let pressed_label = label(pressed);
        let copies = screen
            .candidates
            .iter()
            .filter(|candidate| label(candidate) == pressed_label && candidate.path != pressed.path)
            .map(press_key)
            .filter(|copy| state.banned.insert(copy.clone()))
            .collect::<Vec<_>>();
        if copies.is_empty() {
            return;
        }
        self.history.push(format!(
            "{pressed_label} is repeated on other items; pressing another copy would act on a different item, so only the one pressed counts for this step"
        ));
        state.copies.extend(copies);
    }

    /// Records what the last action changed, banning an element that changed
    /// nothing and failing the step after [`STALL_TURNS`] such turns.
    ///
    /// A wait that changes nothing is not a stall: the page has settled, and
    /// Jev is told so. It is not let wait again after [`MAX_IDLE_WAITS`] of
    /// them, which leaves it to judge or act on the page as it stands.
    fn note_change(&mut self, state: &mut DoState, screen: &Screen) -> Result<(), Halt> {
        let Some(previous) = &state.last else {
            return Ok(());
        };
        let changed = fingerprint(&previous.before) != fingerprint(screen);
        let note = change_note(&previous.before, screen, changed);
        if changed {
            state.unchanged = 0;
            state.idle_waits = 0;
            state.idle_scrolls = 0;
        } else if previous.waited {
            state.idle_waits = state.idle_waits.saturating_add(1);
            self.history.push(
                "waited: the page has finished loading and nothing changed, so waiting longer will not change it"
                    .to_owned(),
            );
            return Ok(());
        } else {
            state.unchanged = state.unchanged.saturating_add(1);
            if previous.scrolled {
                state.idle_scrolls = state.idle_scrolls.saturating_add(1);
                self.history.push(
                    "scrolled: nothing new came into view; the screen already lists what lies below the fold"
                        .to_owned(),
                );
            }
            if let Some(target) = &previous.target {
                state.banned.insert(signature(target));
                self.ledger.tried(format!(
                    "pressed {}: nothing on screen changed",
                    label(target)
                ));
            }
        }
        self.history.push(format!("after the last action: {note}"));
        // A step whose work the page did by itself (a search box that lists
        // results as it is typed in) has nothing left to press: the note
        // says so, so a rescue skips it rather than retry it. Live, four
        // rescues looked for a search button a live search does not have.
        if state.unchanged >= STALL_TURNS {
            return Err(Halt::Failed(
                "the last three actions changed nothing on screen; if the screen already shows what this step was for, its work is done"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

/// Most controls of the dialog in front a failure note names.
const FRONT_CONTROLS: usize = 8;

/// The labels of the pressable controls on `screen` that nothing covers
/// and that are in view: with a dialog in front, its own.
fn front_controls(screen: &Screen) -> Vec<String> {
    screen
        .candidates
        .iter()
        .filter(|candidate| {
            candidate
                .available_actions
                .iter()
                .any(|action| action == "Click")
                && candidate.name.is_some()
                && !candidate.states.iter().any(|state| {
                    state.eq_ignore_ascii_case("covered") || state.eq_ignore_ascii_case("offscreen")
                })
        })
        .map(label)
        .take(FRONT_CONTROLS)
        .collect()
}
