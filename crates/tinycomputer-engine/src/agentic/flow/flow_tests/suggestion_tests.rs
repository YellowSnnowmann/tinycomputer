//! Committing an autocomplete in `enter`: the suggestion a box lists for the
//! text typed into it is picked before the focus moves on, a box that lists
//! none is left as typed, and a private text is never offered for picking.

use super::*;

/// Answers the ride form's questions: each slot goes to the box it names,
/// and a pick between suggestions takes the place in New Delhi.
fn ride(id: &str, question: &Question, _: &Sim) -> Option<Answer> {
    if !matches!(question, Question::Choice(_)) {
        return None;
    }
    let purpose = text_of(question, "purpose");
    if id.starts_with("slot_") {
        let field = if purpose.contains("pickup") {
            "Pickup location"
        } else {
            "Dropoff location"
        };
        return Some(pick(question, field, 0.9));
    }
    purpose
        .contains("suggestion")
        .then(|| pick(question, "Connaught Place New Delhi", 0.9))
}

fn asked_for_a_suggestion(run: &Run) -> bool {
    run.requests.iter().any(|request| {
        request
            .questions
            .values()
            .any(|question| text_of(question, "purpose").contains("suggestion"))
    })
}

#[tokio::test]
async fn enter_picks_the_suggestion_an_autocomplete_box_lists_for_the_typed_text() {
    // Each box keeps a place only once one of its rows is picked, and drops
    // unpicked text as soon as the next box takes the focus: live on a ride
    // site, the pickup box emptied when the dropoff step began.
    let run = run_with(
        App::with(|sim| sim.places = Some(Places::default())),
        json!({"app": "Mail", "steps": [
            {"enter": {"pickup location": "Connaught Place"}},
            {"enter": {"dropoff location": "Indira Gandhi International Airport"}}
        ]}),
        |_| {},
        ride,
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    // "Connaught Place" lists two places, so Jev picked one.
    assert_eq!(
        sim.fields["Pickup location"],
        "Connaught Place New Delhi, Delhi, India"
    );
    // The airport's name lists one place, picked without asking.
    assert_eq!(
        sim.fields["Dropoff location"],
        "Indira Gandhi International Airport New Delhi, Delhi, India"
    );
    let places = sim.places.as_ref().unwrap();
    assert!(places.picked.contains("Pickup location"));
    assert!(places.picked.contains("Dropoff location"));
    drop(sim);
    assert!(asked_for_a_suggestion(&run));
}

#[tokio::test]
async fn enter_leaves_a_box_that_lists_no_suggestion_as_typed() {
    // A plain field opens no list, so committing costs nothing: no question
    // is asked and the text stays exactly as typed.
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": [{"enter": {"subject": "Connaught Place"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().fields["Subject"], "Connaught Place");
    assert!(!asked_for_a_suggestion(&run));
}

#[tokio::test]
async fn a_private_text_is_never_offered_as_a_suggestion_to_pick() {
    // A fact's value may only be typed: picking a suggestion would show it
    // to Jev, so the box keeps the text as typed instead.
    let run = run_with(
        App::with(|sim| sim.places = Some(Places::default())),
        json!({"app": "Mail", "vars": {"home": "Connaught Place"}, "steps": [
            {"enter": {"pickup location": "${home}"}}
        ]}),
        |request| {
            request.facts = BTreeSet::from(["home".to_owned()]);
            request.include_values = false;
        },
        ride,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().fields["Pickup location"], "Connaught Place");
    assert!(!asked_for_a_suggestion(&run));
    let leaked = run.requests.iter().any(|request| {
        serde_json::to_string(request)
            .unwrap()
            .contains("Connaught")
    });
    assert!(!leaked, "a fact's value must never reach a Jev request");
}
