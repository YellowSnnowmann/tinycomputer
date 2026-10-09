//! Steps over a list of results: `pick` the best item by a criterion, and
//! `extract` every item as rows.

use std::collections::BTreeMap;

use serde_json::json;
use tinycomputer_bus::{FlowLoop, JevOperation, PickStep, ReadStep, StepOutcome};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, closest_to, rank, rank_closest};
use tinyinference_decisions::Answer;

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    ask::{self, Questions, chosen, numbered, probability},
    backend::AgentBackend,
    validate::substitute_safe,
    view::{Candidate, is_destructive, label},
};

use super::{
    LIST_LEAD, LIST_LEAN, LIST_PREVIEW, LOCATE_FLOOR, MAX_LISTS, MAX_PICK_SUMMARY, RANKED_CHECKS,
};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Picks the best of a list of results by `pick.by`, stores its text,
    /// and opens it. A criterion over prices, times, durations, or stops,
    /// or nearness to a number ("closest to 9"), is ranked exactly, and the
    /// first ranked item that Jev confirms belongs to `pick.from` is taken:
    /// the ranking reads only its measure, and live, "the cheapest of the
    /// results rated 4 stars or more" took a 3.1-star item of another brand.
    /// Anything else, or a ranking none of whose leaders belongs, is judged
    /// by Jev among the records.
    pub(super) async fn pick(&mut self, log: &mut StepLog, pick: &PickStep) -> Result<Ended, Halt> {
        let from = substitute_safe(&pick.from, &self.vars, &self.facts);
        let by = substitute_safe(&pick.by, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let families = openable(result_families(&screen));
        if families.is_empty() {
            return Err(Halt::Failed(format!("no list of {from} is showing")));
        }
        // A page can repeat several things (a strip of dates above the
        // flights); a measurable criterion ranks the first list that has
        // the measure, and judgement falls to the longest. "The first one
        // rated 4 stars or more" walks the list in its order too, taking
        // the first item that meets the condition: judged over the whole
        // list at once, live, it took the fifth result, another model.
        let condition = first_meeting(&by);
        let criterion =
            Criterion::parse(&by).or_else(|| condition.as_ref().map(|_| Criterion::First));
        let ranked = match (closest_to(&by), criterion) {
            // Nearness to a number ranks by distance, within the list `from`
            // names, since numbers show in every list. Live, a store's sizes
            // 9 and 10 were sold out, and "closest to 9", judged item by
            // item, took none of 6, 7, and 8.
            (Some(target), _) => {
                let groups = self.named_list(log, &screen, &from, &families).await?;
                rank_closest(&records_of(groups), target).map(|order| (groups, order))
            }
            // The list's own order fits every list on the page, so the one
            // `from` names is asked for first, as a judged pick does.
            (None, Some(order @ (Criterion::First | Criterion::Last))) => {
                let groups = self.named_list(log, &screen, &from, &families).await?;
                rank(&records_of(groups), order).map(|ranking| (groups, ranking))
            }
            (None, Some(criterion)) => families.iter().find_map(|groups| {
                rank(&records_of(groups), criterion).map(|order| (groups, order))
            }),
            (None, None) => None,
        };
        let meets =
            condition.map_or_else(|| from.clone(), |condition| format!("{from}, {condition}"));
        let belonging = match ranked {
            Some((groups, order)) => self
                .first_belonging(log, &screen, &meets, groups, &order)
                .await?
                .map(|best| (groups, best)),
            None => None,
        };
        let (groups, best, how) = if let Some((groups, best)) = belonging {
            (groups, best, "ranked")
        } else {
            // Several lists show (a chat list beside the open chat's
            // messages): judge within the one `from` names.
            let groups = self.named_list(log, &screen, &from, &families).await?;
            let best = self.judge_pick(log, &screen, &from, &by, groups).await?;
            (groups, best, "judged")
        };
        let group = &groups[best];
        let summary: String = group
            .fields
            .join(" · ")
            .chars()
            .take(MAX_PICK_SUMMARY)
            .collect();
        if let Some(into) = &pick.into {
            self.vars.insert(into.clone(), summary.clone());
            self.read_into(into);
        }
        let Some(primary) = group.primary.clone() else {
            return Err(Halt::Failed(format!(
                "the picked item has nothing to open: {summary}"
            )));
        };
        if is_destructive(&primary, &screen, &self.stop_before) {
            return Err(Halt::Failed(format!(
                "refused to press {} to pick an item",
                label(&primary)
            )));
        }
        if selected(&primary) {
            let picked = format!("{summary} ({how} by {by}");
            return Ok(self.picked_as_selected(&picked, &from, &by, &summary, groups.len()));
        }
        let reply = self
            .press_uncovering(log, "click", &primary, JevOperation::Click, &from)
            .await?;
        if !reply.ok {
            let why = reply.error.as_ref().map_or_else(
                || "no reason given".to_owned(),
                |error| error.message.clone(),
            );
            return Err(Halt::Failed(format!(
                "could not open the picked item ({why}): {summary}"
            )));
        }
        self.history
            .push(format!("picked {summary} ({how} by {by})"));
        self.remember_choice(&format!("picked from {from} by {by}: {summary}"));
        Ok(Ended::new(
            StepOutcome::Done,
            format!("picked {summary} ({how} by {by}, out of {})", groups.len()),
        ))
    }

    /// Ends a pick whose item the page already has selected, without
    /// pressing it: pressing a selected option again can open its details
    /// instead. Live, a ride app's cheapest car was selected by default,
    /// and the press opened a fare breakdown over the button that requests
    /// it. `picked` reads "<summary> (<how> by <by>".
    fn picked_as_selected(
        &mut self,
        picked: &str,
        from: &str,
        by: &str,
        summary: &str,
        out_of: usize,
    ) -> Ended {
        self.history.push(format!(
            "picked {picked}); it was already selected, so it was not pressed again"
        ));
        self.remember_choice(&format!("picked from {from} by {by}: {summary}"));
        Ended::new(
            StepOutcome::Done,
            format!("picked {picked}, out of {out_of}; already selected)"),
        )
    }

    /// Stores every item of the list showing as JSON rows of their text.
    /// Where several lists show, Jev says which one is `what`.
    pub(super) async fn extract(
        &mut self,
        log: &mut StepLog,
        read: &ReadStep,
    ) -> Result<Ended, Halt> {
        let what = substitute_safe(&read.what, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let mut families = result_families(&screen);
        if families.is_empty() {
            return Err(Halt::Failed(format!("no list of {what} is showing")));
        }
        let chosen = if families.len() > 1 {
            self.judge_list(log, &screen, &what, &families).await?
        } else {
            0
        };
        let groups = families.swap_remove(chosen);
        let rows = groups
            .iter()
            .map(|group| group.fields.clone())
            .collect::<Vec<_>>();
        self.vars.insert(
            read.into.clone(),
            serde_json::to_string(&rows).unwrap_or_default(),
        );
        self.read_into(&read.into);
        self.history.push(format!(
            "extracted {} items of {what} into {}",
            rows.len(),
            read.into
        ));
        Ok(Ended::new(
            StepOutcome::Done,
            format!("extracted {} items into {}", rows.len(), read.into),
        ))
    }

    /// The list of `families` that `from` names: the only one, or the one
    /// Jev chooses when several show.
    async fn named_list<'f>(
        &mut self,
        log: &mut StepLog,
        screen: &crate::agentic::flow::view::Screen,
        from: &str,
        families: &'f [Vec<Group>],
    ) -> Result<&'f Vec<Group>, Halt> {
        let list = if families.len() > 1 {
            self.judge_list(log, screen, from, families).await?
        } else {
            0
        };
        Ok(&families[list])
    }

    /// Asks Jev which of the lists showing is `what`, each shown by its
    /// first [`LIST_PREVIEW`] items, among the first [`MAX_LISTS`]. A list
    /// not clearly chosen falls back to the one Jev leaned to
    /// ([`leaning`]), else the longest, the one an `extract` took before
    /// it asked.
    async fn judge_list(
        &mut self,
        log: &mut StepLog,
        screen: &crate::agentic::flow::view::Screen,
        what: &str,
        families: &[Vec<Group>],
    ) -> Result<usize, Halt> {
        let shown = &families[..families.len().min(MAX_LISTS)];
        let keys = numbered(shown.len());
        log.used(FlowLoop::Narrowing);
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, &format!("extract {what}")),
                    Questions::default().with(
                        "list",
                        ask::options(
                            json!({
                                "task": "Choose the list on screen that is this list.",
                                "what": what,
                            }),
                            keys.iter().cloned().zip(shown.iter().map(|groups| {
                                let items = groups
                                    .iter()
                                    .take(LIST_PREVIEW)
                                    .map(|group| group.fields.clone())
                                    .collect::<Vec<_>>();
                                json!({"untrusted_accessibility_data": {
                                    "items": groups.len(),
                                    "first": items,
                                }})
                            })),
                        ),
                    ),
                ),
            )
            .await?;
        let Some((choice, confidence)) =
            chosen(&answers, "list").filter(|(_, confidence)| *confidence >= LOCATE_FLOOR)
        else {
            return Ok(leaning(&answers, &keys).unwrap_or(0));
        };
        log.confidence = Some(confidence);
        Ok(keys.iter().position(|key| *key == choice).unwrap_or(0))
    }

    /// The first of `order`, an exact ranking of `groups`, that Jev confirms
    /// belongs to `from`, asking about the first [`RANKED_CHECKS`] at once;
    /// `None` when none of them clearly does.
    async fn first_belonging(
        &mut self,
        log: &mut StepLog,
        screen: &crate::agentic::flow::view::Screen,
        from: &str,
        groups: &[Group],
        order: &[usize],
    ) -> Result<Option<usize>, Halt> {
        let leaders = &order[..order.len().min(RANKED_CHECKS)];
        let questions =
            leaders
                .iter()
                .enumerate()
                .fold(Questions::default(), |questions, (place, item)| {
                    questions.with(
                        &format!("belongs_{place}"),
                        ask::belongs(from, &groups[*item].fields),
                    )
                });
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, &format!("pick from {from}")),
                    questions,
                ),
            )
            .await?;
        Ok(leaders
            .iter()
            .enumerate()
            .find(|(place, _)| {
                probability(&answers, &format!("belongs_{place}"))
                    .is_some_and(|yes| yes >= LOCATE_FLOOR)
            })
            .map(|(_, item)| *item))
    }

    /// Asks Jev which record best meets `by`, among the first
    /// [`ask::MAX_READ_SOURCES`]-sized page of them.
    async fn judge_pick(
        &mut self,
        log: &mut StepLog,
        screen: &crate::agentic::flow::view::Screen,
        from: &str,
        by: &str,
        groups: &[Group],
    ) -> Result<usize, Halt> {
        let shown = &groups[..groups.len().min(ask::MAX_READ_SOURCES)];
        let keys = numbered(shown.len());
        log.used(FlowLoop::Narrowing);
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, &format!("pick from {from} by {by}")),
                    Questions::default().with(
                        "record",
                        ask::options(
                            json!({
                                "task": "Choose the item in this list that best meets the criterion.",
                                "list": from,
                                "criterion": by,
                            }),
                            keys.iter().cloned().zip(shown.iter().map(|group| {
                                json!({"untrusted_accessibility_data": {"item": group.fields}})
                            })),
                        ),
                    ),
                ),
            )
            .await?;
        let Some((choice, confidence)) =
            chosen(&answers, "record").filter(|(_, confidence)| *confidence >= LOCATE_FLOOR)
        else {
            return Err(Halt::Failed(format!(
                "no item in {from} clearly meets {by}"
            )));
        };
        log.confidence = Some(confidence);
        keys.iter()
            .position(|key| *key == choice)
            .ok_or_else(|| Halt::Failed(format!("no item in {from} clearly meets {by}")))
    }
}

/// The lists of `families` a pick can open an item of: those whose items
/// mostly hold something to press, when any list's do. A pick opens what
/// it takes, and a list of bare fares (a strip above the flights, or each
/// card's price read apart from it) has nothing to open: live, "the
/// cheapest flight" ranked such a list and took "₹ 6,054".
fn openable(families: Vec<Vec<Group>>) -> Vec<Vec<Group>> {
    let opens = |groups: &Vec<Group>| {
        groups
            .iter()
            .filter(|group| group.primary.is_some())
            .count()
            * 2
            > groups.len()
    };
    if families.iter().any(opens) {
        families.into_iter().filter(opens).collect()
    } else {
        families
    }
}

/// Each card's text as a record, its fields numbered in reading order, and
/// numbered so that their keys sort in that order too.
fn records_of(groups: &[Group]) -> Vec<Record> {
    groups
        .iter()
        .map(|group| Record {
            fields: group
                .fields
                .iter()
                .enumerate()
                .map(|(index, text)| (format!("field {index:03}"), text.clone()))
                .collect(),
        })
        .collect()
}

/// The condition a criterion such as "first product rated 4 stars or more"
/// puts on the first item, when it is "first" and a condition: `None` for a
/// bare "first", which is the list's own order, and for anything else.
pub(in crate::agentic::flow) fn first_meeting(by: &str) -> Option<String> {
    let lower = by.trim().to_ascii_lowercase();
    let lower = lower.strip_prefix("the ").unwrap_or(&lower);
    let rest = lower.strip_prefix("first ")?.trim();
    let conditional = rest
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| CONDITION_WORDS.contains(&word));
    conditional.then(|| rest.to_owned())
}

/// Words that make what follows "first" a condition on the item ("first
/// product rated 4 stars or more", "the first one under ₹500") rather than
/// a name the list holds ("First AC", "first class") or an order of its own
/// ("first to depart"), which Jev judges.
const CONDITION_WORDS: &[&str] = &[
    "rated",
    "rating",
    "under",
    "over",
    "below",
    "above",
    "with",
    "without",
    "least",
    "more",
    "less",
    "available",
    "stock",
    "not",
    "that",
    "which",
    "priced",
    "costing",
    "cheaper",
    "within",
    "having",
    "offering",
];

/// Whether the page shows `control` selected or checked already.
fn selected(control: &Candidate) -> bool {
    control
        .states
        .iter()
        .any(|state| state == "selected" || state == "checked")
}

/// The list Jev leaned to without choosing it clearly: the most likely of
/// `keys` ("none" aside), when it has [`LIST_LEAN`] or more and
/// [`LIST_LEAD`] times the next list's probability. Live, a ride app's
/// option cards drew 0.41 against 0.05 for any other list, and the longest
/// list, their lines split apart, was taken in their place.
pub(in crate::agentic::flow) fn leaning(
    answers: &BTreeMap<String, Answer>,
    keys: &[String],
) -> Option<usize> {
    let Some(Answer::Choice(answer)) = answers.get("list") else {
        return None;
    };
    let mut ranked = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let probability = answer.probabilities.get(key).copied().unwrap_or_default();
            (index, probability)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    let (index, top) = *ranked.first()?;
    let next = ranked.get(1).map_or(0.0, |(_, probability)| *probability);
    (top >= LIST_LEAN && top >= next * LIST_LEAD).then_some(index)
}
