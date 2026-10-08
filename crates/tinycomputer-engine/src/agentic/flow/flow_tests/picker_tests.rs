//! Pop-ups and calendars a step works with: a closer that goes with its
//! pop-up (a "Close" or a consent bar's "Accept all"), a calendar the task picked in closed for a press behind it, in
//! its own step or a later one, and a date picked from a calendar already
//! open.

use super::*;

/// Answers that press the toast's "Close" and never judge the step done.
fn press_close(id: &str, question: &Question, _: &Sim) -> Option<Answer> {
    match id {
        "done" | "blocked" => Some(noul(0.05)),
        "move" => Some(pick(question, "activate", 0.9)),
        _ if id == "target" || id == "region" || id.starts_with("group_") => {
            Some(pick(question, "Close", 0.9))
        }
        _ => None,
    }
}

#[tokio::test]
async fn a_closer_that_goes_with_its_pop_up_ends_a_step_closing_it() {
    // Live, an offer pop-up drawn without a dialog's role closed at the
    // first press, and the step stalled: the judge could not tell whether
    // "declining optional cookies" was done with no cookie banner shown.
    let toast = || {
        App::with(|sim| {
            sim.quirks.insert(Quirk::PromoToast);
        })
    };
    let run = run_with(
        toast(),
        json!({"app": "Mail", "steps": ["close any login or offer pop-up, declining optional cookies"]}),
        |request| request.disabled_loops.push(FlowLoop::Attention),
        press_close,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["Close"]);
    assert!(
        run.result.steps[0]
            .note
            .contains("closed with what it was on"),
        "{}",
        run.result.steps[0].note
    );
    let unrelated = run_with(
        toast(),
        json!({"app": "Mail", "steps": ["archive the message"]}),
        |request| {
            request.disabled_loops.push(FlowLoop::Attention);
            request.max_actions = 1;
        },
        press_close,
    )
    .await;
    assert_ne!(
        unrelated.result.stop,
        FlowStopReason::Completed,
        "a step that closes nothing is not finished by a closer"
    );
}

/// Answers that press the cookie bar's "Accept all" and never judge the
/// step done.
fn press_accept(id: &str, question: &Question, _: &Sim) -> Option<Answer> {
    match id {
        "done" | "blocked" => Some(noul(0.05)),
        "move" => Some(pick(question, "activate", 0.9)),
        _ if id == "target" || id == "region" || id.starts_with("group_") => {
            Some(pick(question, "Accept all", 0.9))
        }
        _ => None,
    }
}

#[tokio::test]
async fn accepting_a_cookie_bar_ends_the_step_that_accepts_it() {
    // A consent bar drawn without a dialog's role closes on "Accept all"
    // as surely as a pop-up on "Close": the press going with the bar is
    // the evidence the judge cannot see.
    let run = run_with(
        App::with(|sim| {
            sim.quirks.insert(Quirk::CookieBar);
        }),
        json!({"app": "Mail", "steps": ["accept the cookie banner"]}),
        |request| request.disabled_loops.push(FlowLoop::Attention),
        press_accept,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["Accept all"]);
    assert!(
        run.result.steps[0]
            .note
            .contains("closed with what it was on"),
        "{}",
        run.result.steps[0].note
    );
}

/// Answers that open the calendar, pick a day, then press "Find flights",
/// judging the step done once that press went through.
fn pick_then_find(id: &str, question: &Question, sim: &Sim) -> Option<Answer> {
    let found = sim.clicks.iter().any(|click| click == "Find flights");
    let next = if sim.fields.contains_key("Departure") {
        "Find flights"
    } else if sim
        .booking
        .as_ref()
        .is_some_and(|booking| booking.calendar.is_some())
    {
        "12 September 2026"
    } else {
        "Departure"
    };
    match id {
        "done" => Some(noul(if found { 0.95 } else { 0.05 })),
        "blocked" => Some(noul(0.05)),
        "move" => Some(pick(question, "activate", 0.9)),
        _ if id == "target" || id == "region" || id.starts_with("group_") => {
            Some(pick(question, next, 0.9))
        }
        _ => None,
    }
}

#[tokio::test]
async fn a_calendar_the_task_picked_in_is_closed_for_a_press_behind_it() {
    // Live, a calendar the task opened stayed in front of the guests and
    // Search buttons once both dates were picked, and every press behind
    // it was refused as lying behind the task's own dialog.
    let run = run_with(
        App::with(|sim| {
            sim.booking = Some(Booking::default());
            sim.quirks.insert(Quirk::CalendarStaysOpen);
        }),
        json!({"app": "Mail", "steps": ["pick 12 September 2026 as the departure date, then press Find flights"]}),
        |request| request.disabled_loops.push(FlowLoop::Attention),
        pick_then_find,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert_eq!(sim.presses, ["escape"]);
    assert_eq!(
        sim.clicks,
        ["Departure", "12 September 2026", "Find flights"]
    );
    assert!(
        run.result.steps[0]
            .actions
            .iter()
            .any(|action| action.action == "press escape (uncover)")
    );
}

#[tokio::test]
async fn a_calendar_a_step_before_picked_in_is_closed_for_a_press_behind_it() {
    // Live, the dates were picked a step each, and the calendar left open
    // in front of the Search button was handed back to the page with the
    // next step: every press behind it was refused, and no step closed it.
    let run = run_with(
        App::with(|sim| {
            sim.booking = Some(Booking::default());
            sim.quirks.insert(Quirk::CalendarStaysOpen);
        }),
        json!({"app": "Mail", "steps": [
            "pick 12 September 2026 as the departure date",
            "press Find flights"
        ]}),
        |request| request.disabled_loops.push(FlowLoop::Attention),
        |id, question, sim| {
            if id != "done" {
                return pick_then_find(id, question, sim);
            }
            let finished = if text_of(question, "step").contains("find flights") {
                sim.clicks.iter().any(|click| click == "Find flights")
            } else {
                sim.fields.contains_key("Departure")
            };
            Some(noul(if finished { 0.95 } else { 0.05 }))
        },
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert_eq!(sim.presses, ["escape"]);
    assert_eq!(
        sim.clicks,
        ["Departure", "12 September 2026", "Find flights"]
    );
}

#[tokio::test]
async fn a_date_whose_calendar_is_open_is_picked_without_pressing_its_button() {
    // Live, a check-out date's step found the calendar open from the
    // check-in, pressed its button, which closed it, four times, and never
    // picked the day.
    let run = run_with(
        App::with(|sim| {
            sim.booking = Some(Booking {
                calendar: Some(8),
                ..Booking::default()
            });
            sim.quirks.insert(Quirk::CalendarStaysOpen);
        }),
        json!({"app": "Mail", "steps": [{"enter": {"departure date": "12 September 2026"}}]}),
        |request| request.disabled_loops.push(FlowLoop::Attention),
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert_eq!(sim.clicks, ["12 September 2026"]);
    assert_eq!(sim.fields["Departure"], "12 September 2026");
}
