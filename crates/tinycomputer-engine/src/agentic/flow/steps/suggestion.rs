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

use super::matching::{
    clickable, closest, editable, lists_more_than, mentions, one_option, plain, plainest,
};

/// Roles a suggestion list draws its rows with. A row that does not mention
/// the typed text is only offered when it carries one of these, so a button
/// that appeared beside the box ("Clear") is never mistaken for a match.
const SUGGESTION_ROLES: &[&str] = &["option", "menuitem", "listitem", "row", "gridcell"];

/// Most new rows one pick is asked over.
const MOST_SUGGESTIONS: usize = 12;

/// Least probability a suggestion Jev picks needs before it is pressed. A
/// press replaces what was typed, so a near tie with "none fits" keeps the
/// text: live, a search box's completions ("... 141 anc" for "... 141") came
/// at 0.43 against 0.42 for none, and pressing one changed the search, while
/// the right places on a ride site came at 0.58 and 0.78.
const SUGGESTION_FLOOR: f64 = 0.5;

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
    /// is matched among the new rows a list draws. Only a row that reads as
    /// the text itself is pressed without asking: one that says more (a
    /// search box's "... 141 anc" for "... 141") may be another thing, so Jev
    /// decides, and an answer under [`SUGGESTION_FLOOR`], or that none fits,
    /// leaves the text as typed.
    ///
    /// A panel whose label strings every row together mentions the text
    /// without being a row, and is never pressed: a press lands wherever its
    /// middle is, and live it set a store's delivery area to another place
    /// than the one typed.
    pub(in crate::agentic::flow) async fn commit_suggestion(
        &mut self,
        log: &mut StepLog,
        slot: &str,
        text: &str,
        field: &Candidate,
        before: &Screen,
    ) -> Result<(), Halt> {
        let mut screen = self.look().await?;
        let shown = before
            .candidates
            .iter()
            .map(|candidate| (candidate.role.as_str(), candidate.name.as_deref()))
            .collect::<BTreeSet<_>>();
        let place = suggests(slot, field);
        let mut fresh = fresh_rows(&screen, &shown, field, text, &self.stop_before, place);
        // A box that suggests places lists them once the page has fetched
        // them: live, a ride app's rows came after the first look, and the
        // pickup typed was never set, so no ride showed.
        for _ in 0..LATE_LOOKS {
            if !fresh.is_empty() || !place {
                break;
            }
            self.act(log, "wait", None, |backend| {
                backend.execute(JevOperation::Wait, None, None)
            })
            .await?;
            screen = self.look().await?;
            fresh = fresh_rows(&screen, &shown, field, text, &self.stop_before, place);
        }
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
                        || shares_most_words(candidate, text)
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
        let typed = plain(text);
        // A search box's suggestions are other searches: only one that is
        // the same search ("Show all results for …") is pressed, at once.
        // Live, "blue light blocking glasses" became another product's
        // name, picked as a suggestion, and the search changed.
        let pool = if searches(slot, field) {
            let Some(row) = same_search_row(pool, &typed) else {
                self.history.push(format!(
                    "no suggestion is the same search; the {slot} stays as typed"
                ));
                return Ok(());
            };
            vec![row]
        } else {
            pool
        };
        let purpose = format!("pick the suggestion that completes the {slot} as {text:?}");
        // A place box keeps a place only once a row is chosen: when no row
        // names it exactly, the closest row is taken rather than none.
        let closest = closest_place(&pool, text).filter(|_| place && !searches(slot, field));
        let exact = one_option(&pool)
            && pool
                .iter()
                .all(|candidate| plain(candidate.name.as_deref().unwrap_or_default()) == typed);
        let grounded = if searches(slot, field) || exact {
            plainest(pool).map(|candidate| Grounded {
                candidate,
                confidence: 1.0,
            })
        } else {
            self.ground(log, &screen, &purpose, &format!("{slot} suggestion"), pool)
                .await?
        };
        let Some(target) = grounded
            .filter(|grounded| grounded.confidence >= SUGGESTION_FLOOR)
            .map(|grounded| grounded.candidate)
            .or(closest)
        else {
            self.history.push(format!(
                "no suggestion clearly fit the {slot}; it stays as typed"
            ));
            return Ok(());
        };
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

/// Words a suggestion may set around the typed query and stay the same
/// search: "Show all results for …", "Search for …".
const SAME_SEARCH_WORDS: &[&str] = &["show", "all", "results", "result", "for", "search", "see"];

/// Whether `field`, filled as `slot`, is a search box.
pub(in crate::agentic::flow) fn searches(slot: &str, field: &Candidate) -> bool {
    let named = |text: &str| {
        let lower = text.to_lowercase();
        lower.contains("search") || lower.contains("query")
    };
    field.role.eq_ignore_ascii_case("searchbox")
        || named(slot)
        || field.name.as_deref().is_some_and(named)
}

/// Whether the suggestion row `row` runs the search `typed` (already
/// plain): the same words, or them after words such as "show all results
/// for".
pub(in crate::agentic::flow) fn same_search(row: &str, typed: &str) -> bool {
    let row = plain(row);
    if row == typed {
        return true;
    }
    row.strip_suffix(typed).is_some_and(|before| {
        let words = before.split_whitespace().collect::<Vec<_>>();
        !words.is_empty() && words.iter().all(|word| SAME_SEARCH_WORDS.contains(word))
    })
}

/// Whether `candidate`'s label holds at least half the words of `text`, and
/// two or more: a row worded its own way ("MG Road / Shivaji Nagar
/// Bengaluru" for "MG Road Metro Station, Bengaluru") that a page draws as
/// a plain pressable box rather than a list row. Live, a ride app's rows
/// were never offered, the place was never set, and no ride showed.
pub(in crate::agentic::flow) fn shares_most_words(candidate: &Candidate, text: &str) -> bool {
    let typed = plain(text);
    let words = typed
        .split(' ')
        .filter(|word| word.chars().count() >= 2)
        .collect::<Vec<_>>();
    let shown = format!(" {} ", plain(&label(candidate)));
    let shared = words
        .iter()
        .filter(|word| shown.contains(&format!(" {word} ")))
        .count();
    shared >= 2 && shared * 2 >= words.len()
}

/// Looks again, a wait apart, for the rows a place box lists late.
const LATE_LOOKS: u32 = 2;

/// Words of a slot that name a place, whose box lists matches as it is
/// typed in.
const PLACE_WORDS: &[&str] = &[
    "pickup",
    "pick",
    "drop",
    "dropoff",
    "from",
    "to",
    "where",
    "location",
    "address",
    "city",
    "destination",
    "origin",
    "area",
    "locality",
    "station",
    "airport",
    "place",
];

/// Whether `field`, filled as `slot`, lists suggestions as it is typed in:
/// a combo or search box, or a box for a place.
pub(in crate::agentic::flow) fn suggests(slot: &str, field: &Candidate) -> bool {
    matches!(field.role.as_str(), "combobox" | "searchbox")
        || plain(slot)
            .split(' ')
            .any(|word| PLACE_WORDS.contains(&word))
}

/// The pressable rows on `screen` that were not on screen before typing
/// (`shown`), are not `field` or another box, do not string a list's rows
/// together, and are safe to press. For a `place` box, a row that was
/// already showing counts too when it matches the text: a ride app lists
/// popular places as soon as its box has the focus, and live, the place
/// typed was among them, so nothing new appeared and nothing was picked.
fn fresh_rows(
    screen: &Screen,
    shown: &BTreeSet<(&str, Option<&str>)>,
    field: &Candidate,
    text: &str,
    stop_before: &[String],
    place: bool,
) -> Vec<Candidate> {
    clickable(&screen.candidates)
        .into_iter()
        .filter(|candidate| {
            let new = !shown.contains(&(candidate.role.as_str(), candidate.name.as_deref()));
            let matches =
                place && (mentions(candidate, text) || shares_most_words(candidate, text));
            candidate.ref_id != field.ref_id
                && !editable(candidate)
                && (new || matches)
                && !lists_more_than(candidate, text)
                && !is_destructive(candidate, screen, stop_before)
        })
        .collect()
}

/// The plainest row of `pool` that runs the search `typed` (`same_search`).
fn same_search_row(pool: Vec<Candidate>, typed: &str) -> Option<Candidate> {
    plainest(
        pool.into_iter()
            .filter(|candidate| same_search(candidate.name.as_deref().unwrap_or_default(), typed))
            .collect(),
    )
}

/// The row of `rows` that shares the most of `text`'s words, when it
/// shares most of them (`shares_most_words`), the first on a tie. Live, a
/// ride app listed "MG Road Shivaji Nagar Bengaluru" for "MG Road Metro
/// Station, Bengaluru", no row named the station, and a pickup left as
/// typed is no pickup: the flow could go no further.
pub(in crate::agentic::flow) fn closest_place(rows: &[Candidate], text: &str) -> Option<Candidate> {
    let typed = plain(text);
    let words = typed
        .split(' ')
        .filter(|word| word.chars().count() >= 2)
        .collect::<Vec<_>>();
    rows.iter()
        .filter(|row| shares_most_words(row, text))
        .map(|row| {
            let shown = format!(" {} ", plain(&label(row)));
            let shared = words
                .iter()
                .filter(|word| shown.contains(&format!(" {word} ")))
                .count();
            (shared, row)
        })
        .fold(
            None::<(usize, &Candidate)>,
            |best, (shared, row)| match best {
                Some((most, _)) if most >= shared => best,
                _ => Some((shared, row)),
            },
        )
        .map(|(_, row)| row.clone())
}
