//! Committing an autocomplete in `enter`: the suggestion a box lists for the
//! text typed into it is picked before the focus moves on, a box that lists
//! none is left as typed, a panel that strings its rows together is never
//! pressed, and a private text is never offered for picking.

use super::*;

/// Answers the ride form's questions: each slot goes to the box it names,
/// and a pick between suggestions takes the airport or the place in New
/// Delhi, sure of it.
fn ride(id: &str, question: &Question, sim: &Sim) -> Option<Answer> {
    suggesting(id, question, sim, 0.9)
}

/// [`ride`], picking a suggestion with `probability`.
fn suggesting(id: &str, question: &Question, _: &Sim, probability: f64) -> Option<Answer> {
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
    purpose.contains("suggestion").then(|| {
        let place = if purpose.to_lowercase().contains("indira") {
            "Indira Gandhi International Airport"
        } else {
            "Connaught Place New Delhi"
        };
        pick(question, place, probability)
    })
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
    // The airport's name lists one place, which says more than was typed,
    // so it is picked on Jev's answer too.
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

/// The notes of a step's waits, in order.
fn waits(run: &Run) -> Vec<String> {
    run.result.steps[0]
        .actions
        .iter()
        .filter(|action| action.action == "wait")
        .map(|action| action.note.clone())
        .collect()
}

#[tokio::test]
async fn a_place_box_whose_rows_come_late_is_looked_at_again_as_they_show() {
    // Live, a ride app's rows came after the first look. Each wait ends as
    // the page changes, and the rows are picked once drawn.
    for late in [1, 2] {
        let run = run_with(
            App::with(|sim| {
                sim.places = Some(Places {
                    late,
                    ..Places::default()
                });
            }),
            json!({"app": "Mail", "steps": [{"enter": {"pickup location": "Connaught Place"}}]}),
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
        assert_eq!(
            run.app.sim().fields["Pickup location"],
            "Connaught Place New Delhi, Delhi, India",
            "rows drawn after {late} waits"
        );
        assert_eq!(waits(&run), vec![String::new(); usize::from(late)]);
        // A wait that saw the page change is settled, as any action is.
        let trail = run.app.sim().trail.clone();
        let watched = trail.iter().position(|call| *call == "changed").unwrap();
        assert_eq!(trail.get(watched + 1), Some(&"settle"), "{trail:?}");
    }
}

#[tokio::test]
async fn a_place_box_waits_past_its_own_rows_for_the_place_typed() {
    // Live, a ride app's pickup box first listed rows of its own ("Allow
    // location access", "Search in a different city"), and a look made as
    // soon as the page went still saw only those: no row named the place,
    // none was picked, and the pickup was never set. The box is looked at
    // again until a row names the place.
    let run = run_with(
        App::with(|sim| {
            sim.places = Some(Places {
                late: 1,
                starters: true,
                ..Places::default()
            });
        }),
        json!({"app": "Mail", "steps": [{"enter": {"pickup location": "Connaught Place"}}]}),
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
    assert_eq!(
        run.app.sim().fields["Pickup location"],
        "Connaught Place New Delhi, Delhi, India"
    );
    assert_eq!(waits(&run), [""], "one wait, ended by the rows showing");
}

#[tokio::test]
async fn a_place_box_on_a_page_that_stays_still_is_waited_on_once() {
    // Live, an address and a city box on a plain form waited twice each for
    // a list that never came. A page that stayed still lists nothing more.
    let run = run_with(
        App::with(|sim| sim.places = Some(Places::default())),
        json!({"app": "Mail", "steps": [{"enter": {"pickup location": "Nowhere Lane"}}]}),
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
    assert_eq!(run.app.sim().fields["Pickup location"], "Nowhere Lane");
    assert_eq!(waits(&run), ["nothing changed"]);
    assert!(!asked_for_a_suggestion(&run));
    // A wait that saw the page stay still has nothing to settle.
    let trail = run.app.sim().trail.clone();
    let watched = trail.iter().position(|call| *call == "still").unwrap();
    assert_ne!(trail.get(watched + 1), Some(&"settle"), "{trail:?}");
}

#[tokio::test]
async fn a_place_box_asks_again_for_the_row_naming_its_place_in_other_words() {
    // An unsure pick (0.45) is not pressed as such. A place box, though,
    // keeps a place only once a row is chosen, so Jev is asked once more for
    // the row naming the same place in other words: live, a ride app had no
    // row naming the station typed, and its pickup was never set. Nothing
    // is pressed on word overlap alone.
    let nearest = |sure: f64| {
        move |id: &str, question: &Question, sim: &Sim| {
            if matches!(question, Question::Choice(_))
                && text_of(question, "purpose").contains("nearest place")
            {
                return Some(pick(question, "Connaught Place New Delhi", sure));
            }
            suggesting(id, question, sim, 0.45)
        }
    };
    let run = run_with(
        App::with(|sim| sim.places = Some(Places::default())),
        json!({"app": "Mail", "steps": [{"enter": {"pickup location": "Connaught Place"}}]}),
        |_| {},
        nearest(0.8),
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    {
        let sim = run.app.sim();
        assert_ne!(sim.fields["Pickup location"], "Connaught Place");
        assert!(sim.fields["Pickup location"].starts_with("Connaught Place"));
        assert!(
            sim.places
                .as_ref()
                .unwrap()
                .picked
                .contains("Pickup location")
        );
    }
    assert!(
        asked_for_a_suggestion(&run),
        "Jev was asked, and was unsure"
    );

    // Jev unsure of every row, however it is asked: nothing is pressed.
    let unsure = run_with(
        App::with(|sim| sim.places = Some(Places::default())),
        json!({"app": "Mail", "steps": [{"enter": {"pickup location": "Connaught Place"}}]}),
        |_| {},
        |id, question, sim| {
            if id.starts_with("slot_") {
                return suggesting(id, question, sim, 0.3);
            }
            Some(match question {
                Question::Choice(_) => pick(question, "none", 0.9),
                _ => noul(0.1),
            })
        },
    )
    .await;
    assert!(
        !unsure
            .app
            .sim()
            .places
            .as_ref()
            .unwrap()
            .picked
            .contains("Pickup location"),
        "no row is pressed that Jev did not choose: {:?}",
        unsure.result.steps
    );
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
async fn enter_never_presses_a_panel_that_strings_its_suggestions_together() {
    // Live, a store's delivery-area popover was read as one button whose
    // name held every row, and pressing it picked the row at its middle:
    // another area than the one typed. A panel that lists more than the text
    // is not a suggestion, so the box keeps the text as typed.
    let run = run_with(
        App::with(|sim| {
            sim.places = Some(Places {
                panel: true,
                ..Places::default()
            });
        }),
        json!({"app": "Mail", "steps": [{"enter": {"pickup location": "Connaught Place"}}]}),
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
    assert_eq!(sim.fields["Pickup location"], "Connaught Place");
    assert!(
        !sim.places
            .as_ref()
            .unwrap()
            .picked
            .contains("Pickup location")
    );
    drop(sim);
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

#[test]
fn a_search_box_takes_only_the_same_search_and_a_place_box_its_reworded_rows() {
    use super::steps::{same_search, searches, shares_most_words, suggests};
    let box_named = |name: &str, role: &str| node(name, role, &["Click", "SetValue"], &[], 0.0);
    assert!(searches("search", &box_named("Products", "textbox")));
    assert!(searches("query", &box_named("Products", "textbox")));
    assert!(searches(
        "product",
        &box_named("What are you looking for?", "searchbox")
    ));
    assert!(!searches(
        "pickup",
        &box_named("Enter address..", "textbox")
    ));

    assert!(same_search(
        "Blue light blocking glasses",
        "blue light blocking glasses"
    ));
    assert!(same_search(
        "Show all results for blue light blocking glasses",
        "blue light blocking glasses"
    ));
    assert!(
        !same_search(
            "lenskart blu screen glasses full rim blue",
            "blue light blocking glasses"
        ),
        "another product's name is another search"
    );
    assert!(!same_search(
        "blue light blocking glasses for kids",
        "blue light blocking glasses"
    ));

    assert!(suggests("pickup", &box_named("Enter address..", "textbox")));
    assert!(suggests("where to", &box_named("Destination", "textbox")));
    assert!(
        !suggests("anything", &box_named("Find", "combobox")),
        "a combo box alone is no place box"
    );
    assert!(!suggests("first name", &box_named("First name", "textbox")));

    let row = |name: &str| node(name, "generic", &["Click"], &[], 0.0);
    assert!(shares_most_words(
        &row("MG Road / Shivaji Nagar Bengaluru Karnataka"),
        "MG Road Metro Station, Bengaluru"
    ));
    assert!(!shares_most_words(
        &row("Indiranagar Bengaluru"),
        "MG Road Metro Station, Bengaluru"
    ));
}

#[test]
fn a_box_that_appears_beside_the_suggestions_is_never_taken_for_one() {
    // A box takes text whether it is set or typed into: one drawn with the
    // list, whose label mentions the text, is no row to press.
    use super::steps::fresh_rows;
    let pickup = node(
        "Pickup location",
        "textbox",
        &["Click", "SetValue"],
        &[],
        0.0,
    );
    let row = "Connaught Place New Delhi, Delhi, India";
    let screen = Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![
            pickup.clone(),
            node(row, "option", &["Click"], &[], 1.0),
            node(
                "Connaught Place note",
                "textbox",
                &["Click", "TypeText"],
                &[],
                2.0,
            ),
            node(
                "Connaught Place landmark",
                "textbox",
                &["Click", "SetValue"],
                &[],
                3.0,
            ),
        ],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let fresh = fresh_rows(
        &screen,
        &BTreeSet::new(),
        &pickup,
        "Connaught Place",
        &[],
        true,
    );
    assert_eq!(
        fresh
            .iter()
            .filter_map(|candidate| candidate.name.as_deref())
            .collect::<Vec<_>>(),
        [row]
    );
}

#[test]
fn a_matching_row_already_shown_counts_only_in_view_and_in_front() {
    // A place box's own list is in view and in front: live, a footer link
    // "hotels in Goa" at the foot of the page was pressed as the place typed.
    // A new row still counts out of view, as one its list scrolled away.
    use super::steps::fresh_rows;
    let place_box = node("Where to?", "textbox", &["Click", "SetValue"], &[], 0.0);
    let row = |name: &str, role: &str, states: &[&str], y: f64| {
        let mut row = node(name, role, &["Click"], &[], y);
        row.states = states.iter().map(|state| (*state).to_owned()).collect();
        row
    };
    let before = vec![
        place_box.clone(),
        row("Goa beaches", "button", &[], 1.0),
        row("hotels in Goa", "link", &["offscreen"], 2.0),
        row("Goa villas", "link", &["covered"], 3.0),
    ];
    let shown = before
        .iter()
        .map(|candidate| (candidate.role.as_str(), candidate.name.as_deref()))
        .collect::<BTreeSet<_>>();
    let mut candidates = before.clone();
    candidates.push(row(
        "Goa International Airport",
        "option",
        &["offscreen"],
        4.0,
    ));
    let screen = Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates,
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let fresh = fresh_rows(&screen, &shown, &place_box, "Goa", &[], true);
    assert_eq!(
        fresh
            .iter()
            .filter_map(|candidate| candidate.name.as_deref())
            .collect::<Vec<_>>(),
        ["Goa beaches", "Goa International Airport"]
    );
}

#[tokio::test]
async fn enter_opens_each_place_box_behind_its_own_button_in_turn() {
    // Live, a flight form drew its place boxes as buttons ("From DEL", "To
    // BLR"), each box showing only once its button is pressed. The "to" box
    // was never opened, and the place was pressed in a link at the foot of
    // the page instead.
    let run = run_with(
        App::with(|sim| {
            sim.places = Some(Places {
                behind_buttons: true,
                ..Places::default()
            });
        }),
        json!({"app": "Mail", "steps": [
            {"enter": {"from": "Connaught Place", "to": "Indira Gandhi International Airport"}}
        ]}),
        |_| {},
        |id, question, sim| {
            if id.starts_with("slot_") {
                let purpose = text_of(question, "purpose");
                let field = if purpose.contains("the from ") {
                    "Pickup location"
                } else {
                    "Dropoff location"
                };
                return Some(pick(question, field, 0.9));
            }
            suggesting(id, question, sim, 0.9)
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
    assert_eq!(
        sim.fields["Pickup location"],
        "Connaught Place New Delhi, Delhi, India"
    );
    assert_eq!(
        sim.fields["Dropoff location"],
        "Indira Gandhi International Airport New Delhi, Delhi, India",
        "{:?} {:?}",
        run.result.steps[0].actions,
        sim.clicks
    );
}
