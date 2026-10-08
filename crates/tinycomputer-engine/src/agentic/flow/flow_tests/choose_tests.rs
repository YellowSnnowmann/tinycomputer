//! The `choose` step: revealing and filtering lists, calendars and dates,
//! options already chosen, and options given as descriptions.

use super::*;

#[tokio::test]
async fn choose_reveals_the_list_first_when_the_option_is_not_visible() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |id, question, sim| match id {
            "target" if sim.extra_buttons == 0 => Some(pick(question, "none", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            "done" => Some(noul(0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(run.result.steps[0].note.contains("was not found"));

    let found = run_with(
        App::with(|sim| sim.extra_buttons = 9),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert_eq!(found.result.stop, FlowStopReason::Completed);
    assert_eq!(found.app.sim().clicks, ["Message 7"]);
}

#[tokio::test]
async fn choose_types_into_an_autocomplete_and_picks_the_suggestion() {
    // A planner may qualify the option; the box is searched by its name.
    for option in ["Srinagar", "Srinagar (SXR)"] {
        let run = run_with(
            App::with(|sim| sim.booking = Some(Booking::default())),
            json!({"app": "Mail", "steps": [
                {"choose": {"what": "the destination box", "option": option}}
            ]}),
            |_| {},
            |id, question, sim| match id {
                "move" => Some(pick(question, "activate", 0.9)),
                "done" => Some(noul(
                    if sim
                        .booking
                        .as_ref()
                        .is_some_and(|booking| booking.searching)
                    {
                        0.9
                    } else {
                        0.05
                    },
                )),
                _ if !matches!(question, Question::Choice(_)) => None,
                _ if purpose_of(question).contains("search box") => {
                    Some(pick(question, "Mumbai", 0.9))
                }
                _ if purpose_of(question).contains("open the destination") => {
                    Some(pick(question, "Going to?", 0.9))
                }
                _ => Some(pick(question, "Srinagar", 0.9)),
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
            sim.fields["Search city"], "Srinagar",
            "a row that takes no text leaves the typing to the focused box"
        );
        assert!(!sim.clicks.contains(&"Mumbai, BOM".to_owned()));
        assert_eq!(
            sim.clicks.last().map(String::as_str),
            Some("Srinagar, SXR"),
            "the box itself is never taken for the option: {:?}",
            sim.clicks
        );
    }
}

#[tokio::test]
async fn choose_finds_the_search_box_when_the_opened_widget_leaves_no_focus() {
    // IndiGo's city picker opens without focusing its search input, so text
    // typed with no target is refused (tinycomputer#62). The step must not
    // count that as typed, and must go on to the box itself.
    let run = run_with(
        App::with(|sim| {
            sim.booking = Some(Booking::default());
            sim.quirks.insert(Quirk::NoFocus);
        }),
        json!({"app": "Mail", "steps": [
            {"choose": {"what": "the destination box", "option": "Srinagar"}}
        ]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "done" => Some(noul(
                if sim
                    .booking
                    .as_ref()
                    .is_some_and(|booking| booking.searching)
                {
                    0.9
                } else {
                    0.05
                },
            )),
            _ if !matches!(question, Question::Choice(_)) => None,
            _ if purpose_of(question).contains("search box") => {
                Some(pick(question, "Search city", 0.9))
            }
            _ if purpose_of(question).contains("open the destination") => {
                Some(pick(question, "Going to?", 0.9))
            }
            _ => Some(pick(question, "Srinagar", 0.9)),
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
    assert_eq!(sim.fields["Search city"], "Srinagar");
    assert_eq!(sim.clicks.last().map(String::as_str), Some("Srinagar, SXR"));
    // Jev is never told the refused text was typed.
    for request in &run.requests {
        let state = request.state.to_string();
        assert!(
            !state.contains("typed into the focused field"),
            "a refused type was reported as typed: {state}"
        );
    }
}

fn purpose_of(question: &Question) -> String {
    text_of(question, "purpose")
}

#[tokio::test]
async fn enter_picks_a_date_from_a_calendar_without_telling_jev_the_date() {
    let run = run_with(
        App::with(|sim| sim.booking = Some(Booking::default())),
        json!({"app": "Mail", "steps": [{"enter": {"departure date": "Sunday, 18 October 2026"}}]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "done" => Some(noul(
                if sim
                    .booking
                    .as_ref()
                    .is_some_and(|booking| booking.calendar.is_some())
                {
                    0.9
                } else {
                    0.05
                },
            )),
            _ if !matches!(question, Question::Choice(_)) => None,
            _ if id.starts_with("slot_") => Some(pick(question, "none", 0.9)),
            _ if purpose_of(question).contains("value being entered") => {
                Some(pick(question, "18 October", 0.9))
            }
            _ => Some(pick(question, "Departure", 0.9)),
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
    assert_eq!(sim.fields["Departure"], "18 October 2026");
    assert_eq!(
        sim.clicks
            .iter()
            .filter(|click| *click == "Next Month")
            .count(),
        1,
        "paged from September to October: {:?}",
        sim.clicks
    );
    for request in &run.requests {
        for question in request.questions.values() {
            for field in ["purpose", "step", "task"] {
                assert!(
                    !text_of(question, field).contains("18 october"),
                    "the value reached a question: {}",
                    text_of(question, field)
                );
            }
        }
    }
}

#[test]
fn a_date_is_told_from_other_options_and_containers_give_way() {
    use super::steps::{closest, looks_like_date};
    assert!(looks_like_date("Sunday 18 October 2026"));
    assert!(looks_like_date("october 3"));
    assert!(!looks_like_date("18 oct"), "a month must be spelled out");
    assert!(!looks_like_date("October"), "a month alone is no day");
    assert!(!looks_like_date("Srinagar 40"));

    let day = node("Sunday, 18 October 2026", "button", &["Click"], &[], 0.0);
    let month = node(
        &format!("departureDate {}", "Sunday, 18 October 2026 ".repeat(20)),
        "button",
        &["Click"],
        &[],
        0.0,
    );
    let names = |kept: Vec<Candidate>| {
        kept.into_iter()
            .filter_map(|node| node.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(closest(vec![month.clone(), day.clone()])),
        [day.name.clone().unwrap()]
    );
    assert_eq!(
        names(closest(vec![month.clone()])).len(),
        1,
        "a lone match stays"
    );
    assert!(closest(Vec::new()).is_empty());

    // A list's option stays however long its label: one airport row says
    // much more than a footer link that only names the city.
    let link = node("Mumbai", "link", &["Click"], &[], 0.0);
    let row = node(
        "BOM Mumbai, India Chhatrapati Shivaji International Airport 3 Nearby Airports found",
        "option",
        &["Click"],
        &[],
        0.0,
    );
    assert_eq!(names(closest(vec![link.clone(), row.clone()])).len(), 2);
}

#[tokio::test]
async fn choose_never_clicks_an_irreversible_option() {
    // "Send" is clickable and matches the requested option by name, but it is
    // irreversible; `choose` must fail the step through the usual `stop_before`
    // path rather than pressing it directly.
    let app = App::with(|sim| sim.compose_open = true);
    let run = run_with(
        app,
        json!({"app": "Mail", "steps": [{"choose": {"what": "the toolbar", "option": "Send"}}]}),
        |_| {},
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        !run.app.sim().sent,
        "choose must never press an irreversible control"
    );
    assert!(!run.app.sim().clicks.contains(&"Send".to_owned()));
}

fn fare(name: &str, checked: bool) -> Candidate {
    let mut candidate = node(
        name,
        "radio",
        &["Click"],
        &["window", "group \"Fare Types\""],
        300.0,
    );
    if checked {
        candidate.states = vec!["checked".to_owned()];
    }
    candidate
}

#[test]
fn a_fare_card_is_one_option_and_a_checked_one_is_already_chosen() {
    let saver = "Saver fare ₹7,346 + Earn 696 IndiGo BluChips 7 kg Cabin bag allowance 15 kg \
                 Check-in bag allowance Zero change and cancellation charges within 48 hours of \
                 booking Avail Flexi plus fare benefits For just ₹525 Upgrade";
    let flexi = "Flexi plus fare ₹7,871 + Earn 756 IndiGo BluChips 7 kg Cabin bag allowance";
    assert!(!lists_more_than(&fare(saver, false), "Saver"));
    let list = node(saver, "button", &["Click"], &["window"], 1.0);
    assert!(
        lists_more_than(&list, "Saver"),
        "a button this wordy is a list"
    );

    let mut screen = Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![fare(saver, true), fare(flexi, false)],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    assert_eq!(
        already_chosen(&screen, "Saver").and_then(|chosen| chosen.name),
        Some(saver.to_owned())
    );
    assert!(
        already_chosen(&screen, "Flexi plus").is_none(),
        "the checked card only mentions Flexi plus further in"
    );
    assert!(already_chosen(&screen, "").is_none());
    screen.candidates[0].states.clear();
    assert!(already_chosen(&screen, "Saver").is_none());
}

#[test]
fn a_field_already_showing_the_option_holds_it_unless_the_flow_typed_it() {
    // Emirates' passengers box reads "1 Adult"; the stepper beside it names
    // "1 Adult" too, and pressing it makes two.
    let mut passengers = node(
        "Passengers",
        "textbox",
        &["Click", "SetValue"],
        &["main"],
        1.0,
    );
    passengers.value = Some(json!("1 Adult"));
    let stepper = node(
        "Increase number of Adult passengers. You have selected 1 Adult. Ages 12+",
        "button",
        &["Click"],
        &["main"],
        2.0,
    );
    let screen = Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: vec![stepper, passengers.clone()],
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let none = BTreeSet::new();
    assert_eq!(
        already_holds(&screen, "1 Adult", &none).and_then(|held| held.name),
        Some("Passengers".to_owned()),
        "the stepper only mentions the option; the box holds it"
    );
    assert!(already_holds(&screen, "2 Adults", &none).is_none());
    assert!(
        already_holds(&screen, "Adult", &none).is_none(),
        "whole value only"
    );
    assert!(already_holds(&screen, "", &none).is_none());
    let typed = BTreeSet::from([super::view::element_kind(&passengers)]);
    assert!(
        already_holds(&screen, "1 Adult", &typed).is_none(),
        "text the flow typed to search is not a choice"
    );
}

#[tokio::test]
async fn a_reveal_that_fails_leaves_the_other_ways_to_try() {
    let run = run_with(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the message list", "option": "Message 7"}}]}),
        |_| {},
        |id, question, _| match id {
            "target" => Some(pick(question, "none", 0.9)),
            "move" => Some(pick(question, "activate", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.result.steps[0].note.contains("was not found"),
        "every way was tried before giving up: {}",
        run.result.steps[0].note
    );
}

#[test]
fn a_search_box_is_searched_by_the_options_name() {
    assert_eq!(steps::search_text("Srinagar (SXR)"), "Srinagar");
    assert_eq!(steps::search_text("Mumbai, BOM"), "Mumbai");
    assert_eq!(steps::search_text("18 October 2026"), "18 October 2026");
    assert_eq!(steps::search_text("(SXR)"), "(SXR)");
}

#[tokio::test]
async fn an_option_already_chosen_is_not_clicked_again() {
    let run = run(
        App::with(|sim| {
            sim.checked_fare = Some("Saver fare ₹7,346 with 15 kg check-in bag allowance and more");
        }),
        json!({"app": "Mail", "steps": [{"choose": {"what": "the fare type", "option": "Saver"}}]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert_eq!(run.app.sim().clicks, [] as [std::string::String; 0]);
    assert!(run.requests.is_empty(), "nothing needed asking");
}

#[tokio::test]
async fn an_option_given_as_a_description_is_matched_by_jev() {
    let run = run_with(
        App::with(|sim| sim.checked_fare = Some("Saver fare ₹7,346 with 15 kg check-in")),
        json!({"app": "Mail", "steps": [
            {"choose": {"what": "the fare types", "option": "the lowest priced fare (e.g. the cheapest one)"}}
        ]}),
        |_| {},
        |id, question, _| {
            (id == "target" && purpose_of(question).contains("that fits"))
                .then(|| pick(question, "Saver", 0.9))
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.result.steps[0].outcome, StepOutcome::AlreadyDone);
    assert!(
        run.app.sim().clicks.is_empty(),
        "the checked fare is left as is"
    );
}

#[test]
fn looks_like_date_rejects_a_day_the_named_month_never_has() {
    assert!(looks_like_date("18 October 2026"));
    // April has 30 days; without a year, February is taken generously (29).
    assert!(!looks_like_date("31 April"));
    assert!(looks_like_date("29 February"));
    // 2026 is not a leap year; 2028 is.
    assert!(!looks_like_date("29 February 2026"));
    assert!(looks_like_date("29 February 2028"));
}

#[test]
fn in_region_prefers_the_ancestor_named_region_but_keeps_every_match_when_none_is_named() {
    let seat = node(
        "Continue",
        "button",
        &["Click"],
        &["root", "Seat picker"],
        10.0,
    );
    let unrelated = node(
        "Continue",
        "button",
        &["Click"],
        &["root", "Newsletter"],
        20.0,
    );
    assert!(in_region(&seat, "seat picker"));
    assert!(!in_region(&unrelated, "seat picker"));
    // An empty `what` names no region to narrow by, so everything matches:
    // `pick_option` falls back to the unnarrowed pool when nothing on the
    // page names the region at all.
    assert!(in_region(&unrelated, ""));

    // A placing word alone is in too many labels to place an option: only a
    // container whose name begins with it holds one.
    let route = node(
        "Delhi to Mumbai flights",
        "link",
        &["Click"],
        &["root", "list \"Popular routes to Mumbai\""],
        30.0,
    );
    let airport = node(
        "BOM Mumbai, India",
        "option",
        &["Click"],
        &["root", "dialog \"To\"", "listbox \"Airports\""],
        40.0,
    );
    assert!(!in_region(&route, "to"));
    assert!(in_region(&airport, "to"));
    // A longer region name still counts on the option's own label.
    assert!(in_region(&route, "popular routes"));
}

#[test]
fn redacted_strips_the_shown_text_but_keeps_the_ref_and_role() {
    let mut target = node("4111 1111 1111 1111", "option", &["Click"], &["root"], 5.0);
    target.description = Some("saved card".to_owned());
    target.value = Some(json!("4111 1111 1111 1111"));
    let logged = redacted(&target);
    assert_eq!(logged.name, None);
    assert_eq!(logged.description, None);
    assert_eq!(logged.value, None);
    assert_eq!(logged.ref_id, target.ref_id);
    assert_eq!(logged.role, target.role);
}

#[test]
fn a_day_in_a_strip_of_dates_is_found_by_its_short_label() {
    use super::steps::{date_words, shows_date};
    let wednesday = date_words("Wednesday 7 October 2026");
    assert!(
        shows_date("WED 07 OCT", &wednesday),
        "a strip leaves the year out"
    );
    assert!(shows_date("Wednesday, 7 October 2026", &wednesday));
    assert!(!shows_date("THU 08 OCT", &wednesday));
    assert!(!shows_date("WED 07 NOV", &wednesday));
    assert!(
        !shows_date("Wednesday, 7 October 2027", &wednesday),
        "a year the control shows must be the year asked for"
    );
    assert!(shows_date("7 Sept", &date_words("7 September")));
}

#[test]
fn a_date_shown_under_the_fields_own_name_is_already_chosen() {
    // Live, the day was pressed and the departure button showed it, but the
    // step looked for a box to type the date into and failed. A day of the
    // calendar names no field, so it never counts.
    use super::steps::date_shown_in;
    let screen = |names: &[&str]| Screen {
        app: "browser".to_owned(),
        window: None,
        surface: "window".to_owned(),
        candidates: names
            .iter()
            .map(|name| node(name, "button", &["Click"], &["form"], 0.0))
            .collect(),
        context: Vec::new(),
        unexplored: Vec::new(),
        text_nodes: Vec::new(),
    };
    let set = screen(&["Departure Thu, 22 Oct", "22 6845"]);
    assert_eq!(
        date_shown_in(&set, "departure date", "22 October 2026").and_then(|holder| holder.name),
        Some("Departure Thu, 22 Oct".to_owned())
    );
    let unset = screen(&["Departure Fri, 09 Oct", "Thursday, October 22, 2026"]);
    assert!(date_shown_in(&unset, "departure date", "22 October 2026").is_none());
    assert!(
        date_shown_in(&set, "date", "22 October 2026").is_none(),
        "no word names the field"
    );
}
