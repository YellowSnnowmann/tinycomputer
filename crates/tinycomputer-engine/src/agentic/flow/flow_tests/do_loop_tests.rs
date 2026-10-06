//! The `do` loop: obstacles, overlays, covered clicks, regressions, stalls,
//! refused controls, move outcomes, and disabled loops.

use super::*;

#[tokio::test]
async fn an_obstacle_is_dismissed_with_a_safe_control_only() {
    let run = run(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Obstacles));
    assert_eq!(run.app.sim().clicks, ["Keep Editing"]);
    let dismiss = run
        .requests
        .iter()
        .find_map(|request| request.questions.get("dismiss"))
        .unwrap();
    assert!(
        !serde_json::to_string(dismiss)
            .unwrap()
            .contains("Delete Draft"),
        "an irreversible control is never offered to clear an obstacle"
    );

    let escaped = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "dismiss").then(|| pick(question, "escape", 0.9)),
    )
    .await;
    assert!(escaped.app.sim().presses.contains(&"escape".to_owned()));
}

/// Answers that press "Keep Editing" and never judge the step done, so only
/// the screen can end it.
fn press_keep_editing(id: &str, question: &Question, _: &Sim) -> Option<Answer> {
    match id {
        "done" | "blocked" => Some(noul(0.05)),
        "move" => Some(pick(question, "activate", 0.9)),
        _ if id == "target" || id == "region" || id.starts_with("group_") => {
            Some(pick(question, "Keep Editing", 0.9))
        }
        _ => None,
    }
}

#[tokio::test]
async fn pressing_the_named_control_that_closes_an_overlay_ends_the_step() {
    for step in [
        "close the dialog by keeping editing",
        "dismiss the save prompt",
    ] {
        let run = run_with(
            App::with(|sim| sim.obstacle = true),
            json!({"app": "Mail", "steps": [step]}),
            |_| {},
            press_keep_editing,
        )
        .await;
        assert_eq!(run.result.stop, FlowStopReason::Completed, "{step}");
        assert_eq!(run.app.sim().clicks, ["Keep Editing"], "{step}");
        assert!(
            run.result.steps[0].note.contains("closed"),
            "{}",
            run.result.steps[0].note
        );
    }
    let unrelated = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["archive the message"]}),
        |request| request.max_actions = 1,
        press_keep_editing,
    )
    .await;
    assert_ne!(
        unrelated.result.stop,
        FlowStopReason::Completed,
        "closing an overlay the step never mentions does not finish it"
    );
}

#[tokio::test]
async fn a_covered_click_closes_what_covers_it_and_tries_again() {
    let run = run_with(
        App::quirky(Quirk::Drawer),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "activate", 0.9)),
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{:?}",
        run.result.steps
    );
    let sim = run.app.sim();
    assert!(sim.compose_open);
    assert_eq!(sim.presses, ["escape"]);
    let actions = &run.result.steps[0].actions;
    assert_eq!(
        actions
            .iter()
            .map(|action| action.action.as_str())
            .collect::<Vec<_>>(),
        ["click", "press escape (uncover)", "click"],
    );
}

#[tokio::test]
async fn a_regression_is_undone_and_the_element_is_not_tried_again() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.max_actions = 6,
        |id, question, sim| match id {
            "move" => Some(pick(
                question,
                if sim.presses.contains(&"escape".to_owned()) {
                    "shortcut"
                } else {
                    "activate"
                },
                0.9,
            )),
            "target" => Some(pick(question, "Archive", 0.9)),
            "progress" => Some(level(if sim.compose_open {
                4
            } else if sim.clicks.is_empty() {
                3
            } else {
                0
            })),
            _ => None,
        },
    )
    .await;
    let sim = run.app.sim();
    assert_eq!(sim.clicks, ["Archive"]);
    assert!(sim.presses.contains(&"escape".to_owned()));
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Undo));
    assert_eq!(run.result.stop, FlowStopReason::Completed);
}

#[tokio::test]
async fn actions_that_change_nothing_fail_the_step() {
    let run = run(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(run.result.steps[0].note.contains("changed nothing"));
}

#[tokio::test]
async fn an_irreversible_control_is_refused_inside_an_ordinary_step() {
    let run = run_with(
        App::with(|sim| sim.compose_open = true),
        json!({"app": "Mail", "steps": ["get rid of this draft"]}),
        |request| request.max_actions = 4,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "Send", 0.95)),
            _ => None,
        },
    )
    .await;
    assert!(!run.app.sim().sent);
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.result.steps[0].actions.is_empty(),
        "a refused destructive click must never be recorded as an action the step took"
    );
    assert_eq!(
        run.result.steps[0].turns, 1,
        "a refused destructive click must fail the step immediately, not after it has been \
         mistaken for a no-op action and stalled out"
    );
}

#[tokio::test]
async fn move_outcomes_cover_finished_stuck_wait_and_a_missing_shortcut() {
    // With no completion judge to overrule it, "finished" ends the step.
    let finished = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.disabled_loops = vec![FlowLoop::Completion],
        |id, question, _| (id == "move").then(|| pick(question, "finished", 0.9)),
    )
    .await;
    assert_eq!(finished.result.stop, FlowStopReason::Completed);

    // A judge that sees the step undone overrules it: the loop acts instead
    // of skipping a step that was never done.
    let overruled = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_actions = 3,
        |id, question, _| (id == "move").then(|| pick(question, "finished", 0.9)),
    )
    .await;
    assert_ne!(overruled.result.stop, FlowStopReason::Completed);
    assert!(!overruled.app.sim().clicks.is_empty(), "it acted instead");
    assert!(overruled.requests.iter().any(|request| {
        serde_json::to_string(&request.state)
            .unwrap()
            .contains("does not yet clearly show this step done")
    }));

    let stuck = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "stuck", 0.9)),
    )
    .await;
    assert_eq!(stuck.result.stop, FlowStopReason::StepFailed);

    let waited = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_actions = 2,
        |id, question, _| match id {
            "move" => Some(pick(question, "wait", 0.9)),
            "shortcut" => Some(pick(question, "none", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(waited.result.stop, FlowStopReason::ActionBudget);

    let no_shortcut = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_model_calls = 4,
        |id, question, _| (id == "shortcut").then(|| pick(question, "none", 0.9)),
    )
    .await;
    assert_eq!(no_shortcut.result.stop, FlowStopReason::ModelBudget);
    assert_eq!(
        no_shortcut.app.sim().presses,
        [] as [std::string::String; 0]
    );
}

#[tokio::test]
async fn a_control_the_flows_own_stop_before_names_is_refused_in_an_ordinary_step() {
    // "Archive" is not on the generic denylist, but this flow already plans
    // to stop in front of it later; an ordinary step must not press it first.
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": [
            "tidy up the inbox",
            {"stop_before": "archive the conversation"}
        ]}),
        |request| request.max_actions = 4,
        |id, question, _| match id {
            "move" => Some(pick(question, "activate", 0.9)),
            "target" => Some(pick(question, "Archive", 0.95)),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.app.sim().clicks.is_empty(),
        "a control the flow's own stop_before names must never be clicked early"
    );
}

#[tokio::test]
async fn an_unrecognized_move_is_skipped_rather_than_clicked() {
    // A malformed or prompt-injected answer must never fall through to
    // `activate`'s default Click branch; only `activate`, `expand`, and
    // `scroll` may ground and act.
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |request| request.max_actions = 4,
        |id, _, _| {
            (id == "move").then(|| {
                Answer::Choice(ChoiceAnswer {
                    choice: "delete_everything".to_owned(),
                    probabilities: BTreeMap::from([("delete_everything".to_owned(), 0.9)]),
                    confidence: 0.9,
                })
            })
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        run.app.sim().clicks.is_empty(),
        "an unrecognized move must never ground and click a control"
    );
}

#[tokio::test]
async fn return_is_refused_while_a_dialog_is_showing() {
    let run = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "Mail", "steps": ["confirm the name"]}),
        |request| {
            request.disabled_loops = vec![FlowLoop::Obstacles];
            request.max_model_calls = 6;
        },
        |id, question, _| match id {
            "shortcut" => Some(pick(question, "confirm", 0.9)),
            _ => None,
        },
    )
    .await;
    assert!(!run.app.sim().presses.contains(&"return".to_owned()));
    let confirmed = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["confirm the name"]}),
        // 2: one for the implicit launch, one for the press itself.
        |request| request.max_actions = 2,
        |id, question, _| match id {
            "shortcut" => Some(pick(question, "confirm", 0.9)),
            _ => None,
        },
    )
    .await;
    assert_eq!(confirmed.app.sim().presses, ["return"]);
}

#[tokio::test]
async fn disabled_loops_are_not_asked_and_the_move_falls_back_to_pressing() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            request.disabled_loops = vec![
                FlowLoop::Moves,
                FlowLoop::Progress,
                FlowLoop::Obstacles,
                FlowLoop::Consistency,
                FlowLoop::Corroboration,
                FlowLoop::Undo,
                FlowLoop::Memory,
                FlowLoop::Narrowing,
                FlowLoop::Slots,
            ];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(run.app.sim().clicks, ["New Message"]);
    let asked = run
        .requests
        .iter()
        .flat_map(|request| request.questions.keys().cloned())
        .collect::<BTreeSet<_>>();
    assert!(!asked.contains("move") && !asked.contains("progress") && !asked.contains("blocked"));

    let blind = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| {
            request.disabled_loops = vec![
                FlowLoop::Moves,
                FlowLoop::Progress,
                FlowLoop::Obstacles,
                FlowLoop::Completion,
            ];
        },
        |_, _, _| None,
    )
    .await;
    assert_eq!(
        blind.result.stop,
        FlowStopReason::StepFailed,
        "without the completion judge a step cannot recognise its own success"
    );
    assert_eq!(blind.app.sim().clicks, ["New Message"]);
}

#[tokio::test]
async fn an_action_judged_unhelpful_is_undone_and_not_tried_again() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["start a new email message"]}),
        |request| request.max_actions = 6,
        |id, question, sim| match id {
            "move" => Some(pick(
                question,
                if sim.presses.contains(&"escape".to_owned()) {
                    "shortcut"
                } else {
                    "activate"
                },
                0.9,
            )),
            "target" => Some(pick(question, "Archive", 0.9)),
            "helped" => Some(noul(if sim.clicks.is_empty() { 0.9 } else { 0.05 })),
            _ => None,
        },
    )
    .await;
    let sim = run.app.sim();
    assert_eq!(sim.clicks, ["Archive"]);
    assert!(sim.presses.contains(&"escape".to_owned()));
    assert!(run.result.steps[0].loops.contains(&FlowLoop::Undo));
    assert_eq!(run.result.stop, FlowStopReason::Completed);
}

#[tokio::test]
async fn waits_that_change_nothing_are_not_a_stall_and_stop_being_offered() {
    let run = run_with(
        App::quirky(Quirk::Frozen),
        json!({"app": "Mail", "steps": ["open the search results"]}),
        |_| {},
        |id, question, _| (id == "move").then(|| pick(question, "wait", 0.9)),
    )
    .await;
    let step = &run.result.steps[0];
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert!(
        step.note.contains("not accomplished after"),
        "a settled page is not a stall: {}",
        step.note
    );
    let waits = step
        .actions
        .iter()
        .filter(|action| action.action == "wait")
        .count();
    assert_eq!(waits, 2, "no third wait on a settled page");
    let told = run.requests.iter().any(|request| {
        serde_json::to_string(&request.state)
            .unwrap()
            .contains("the page has finished loading and nothing changed")
    });
    assert!(told, "Jev is told the page has settled");
}

#[tokio::test]
async fn after_acting_a_finished_move_stands_unless_the_judge_leans_undone() {
    let run = run_with(
        App::default(),
        json!({"app": "Mail", "steps": ["tidy up"]}),
        |_| {},
        |id, question, sim| match id {
            "move" => Some(pick(
                question,
                if sim.clicks.is_empty() {
                    "activate"
                } else {
                    "finished"
                },
                0.9,
            )),
            "done" => Some(noul(if sim.clicks.is_empty() { 0.05 } else { 0.6 })),
            _ => None,
        },
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::Completed);
    assert_eq!(
        run.app.sim().clicks.len(),
        1,
        "no click after the step was done"
    );
}

/// Three result cards, each with its own "Select".
fn three_cards(sim: &mut Sim) {
    sim.results = vec![
        ("Row A seat 03", "Available", "₹200"),
        ("Row A seat 04", "Available", "₹200"),
        ("Row A seat 05", "Available", "₹200"),
    ];
}

/// Answers that press a card's "Select" until `wanted` cards are pressed,
/// and only then judge the step done.
fn select_until(wanted: usize) -> impl Fn(&str, &Question, &Sim) -> Option<Answer> {
    move |id, question, sim| match id {
        "done" => Some(noul(if sim.picked.len() >= wanted {
            0.95
        } else {
            0.05
        })),
        "blocked" => Some(noul(0.05)),
        "move" => Some(pick(question, "activate", 0.9)),
        _ if id == "target" || id == "region" || id.starts_with("group_") => {
            Some(pick(question, "Select", 0.9))
        }
        _ => None,
    }
}

#[tokio::test]
async fn a_step_choosing_several_items_presses_each_ones_own_copy() {
    // Live, a seat table's "Select" was pressed for one seat, and the
    // second seat's "Select" was struck off as another item's copy.
    let run = run_with(
        App::with(three_cards),
        json!({"app": "Mail", "steps": ["choose 2 adjacent available seats"]}),
        |_| {},
        select_until(2),
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{}",
        run.result.steps[0].note
    );
    assert_eq!(run.app.sim().picked, ["@s:select-1", "@s:select-2"]);
}

#[tokio::test]
async fn a_step_adding_one_item_leaves_the_other_items_copies_alone() {
    // A count of one item ("2 packets of milk") is raised on that item:
    // another card's copy of its button adds a different product.
    let run = run_with(
        App::with(three_cards),
        json!({"app": "Mail", "steps": ["add 2 packets of the milk"]}),
        |_| {},
        select_until(2),
    )
    .await;
    assert_eq!(run.result.stop, FlowStopReason::StepFailed);
    assert_eq!(run.app.sim().picked, ["@s:select-1"]);
}

#[test]
fn several_items_are_asked_for_by_a_choosing_verb_and_a_counted_plural() {
    for several in [
        "choose 2 adjacent available seats in the cheapest section",
        "select three files",
        "pick two seats together",
        "please choose 4 tickets",
    ] {
        assert!(act::asks_for_several(several), "{several}");
    }
    for one in [
        "add 2 packets of Amul Taaza milk to the cart",
        "choose Wednesday 7 October 2026 in the date selector",
        "choose 2D",
        "select the 1 kg pack",
        "choose 2 in the quantity box",
        "open the first movie",
    ] {
        assert!(!act::asks_for_several(one), "{one}");
    }
}

#[tokio::test]
async fn a_step_finding_nothing_to_press_answers_the_dialog_the_task_opened() {
    // Live, a date step found nothing to press for four turns while the
    // format dialog a booking button had opened offered "2D", and a rescue
    // was spent pressing it.
    let run = run_with(
        App::with(|sim| sim.obstacle = true),
        json!({"app": "browser", "steps": ["choose Wednesday 7 October 2026 in the date picker"]}),
        |_| {},
        |id, question, sim| {
            let answering = serde_json::to_string(question)
                .unwrap()
                .contains("click to answer the dialog");
            match id {
                "done" => Some(noul(if sim.obstacle { 0.05 } else { 0.95 })),
                "blocked" => Some(noul(0.05)),
                "move" => Some(pick(question, "activate", 0.9)),
                _ if id == "target" || id == "region" || id.starts_with("group_") => Some(pick(
                    question,
                    if answering {
                        "Keep Editing"
                    } else {
                        "no such control"
                    },
                    0.9,
                )),
                _ => None,
            }
        },
    )
    .await;
    assert_eq!(
        run.result.stop,
        FlowStopReason::Completed,
        "{}",
        run.result.steps[0].note
    );
    assert_eq!(run.app.sim().clicks, ["Keep Editing"]);
}

#[tokio::test]
async fn a_control_the_dialogs_own_bar_covers_is_pressed_and_one_behind_it_is_not() {
    // Live, a seat table's lower rows sat under its "Pay" bar and were never
    // offered; a press scrolls such a control out from under the bar. A
    // browser run takes a dialog at its first look as the task's own.
    let pressing = |wanted: &'static str| {
        move |id: &str, question: &Question, sim: &Sim| match id {
            "done" => Some(noul(if sim.obstacle { 0.05 } else { 0.95 })),
            "blocked" => Some(noul(0.05)),
            "move" => Some(pick(question, "activate", 0.9)),
            _ if id == "target" || id == "region" || id.starts_with("group_") => {
                Some(pick(question, wanted, 0.9))
            }
            _ => None,
        }
    };
    let in_the_dialog = run_with(
        App::with(|sim| {
            sim.obstacle = true;
            sim.quirks.insert(Quirk::BarOverSheet);
        }),
        json!({"app": "browser", "steps": ["keep editing the draft"]}),
        |_| {},
        pressing("Keep Editing"),
    )
    .await;
    let asked = in_the_dialog
        .requests
        .iter()
        .flat_map(|request| request.questions.keys().cloned())
        .collect::<Vec<_>>();
    let (clicks, presses) = {
        let sim = in_the_dialog.app.sim();
        (sim.clicks.clone(), sim.presses.clone())
    };
    assert_eq!(
        clicks,
        ["Keep Editing"],
        "{} {asked:?} {presses:?}",
        in_the_dialog.result.steps[0].note
    );

    // What the dialog itself covers on the page behind it stays out.
    let behind = run_with(
        App::with(|sim| {
            sim.obstacle = true;
            sim.quirks.insert(Quirk::Covered);
        }),
        json!({"app": "browser", "steps": ["start a new email message"]}),
        |_| {},
        pressing("New Message"),
    )
    .await;
    assert!(
        !behind.app.sim().clicks.contains(&"New Message".to_owned()),
        "{:?}",
        behind.app.sim().clicks
    );
}
