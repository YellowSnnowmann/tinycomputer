//! Steps over a list of results: `pick` the best item by a criterion, and
//! `extract` every item as rows.

use serde_json::json;
use tinycomputer_bus::{FlowLoop, JevOperation, PickStep, ReadStep, StepOutcome};
use tinycomputer_core::surface::{Group, result_families};
use tinycomputer_core::{Criterion, Record, rank};

use crate::agentic::flow::{
    Ended, FlowRun, Halt, StepLog,
    ask::{self, Questions, chosen, numbered, probability},
    backend::AgentBackend,
    validate::substitute_safe,
    view::{is_destructive, label},
};

use super::{LIST_PREVIEW, LOCATE_FLOOR, MAX_LISTS, MAX_PICK_SUMMARY, RANKED_CHECKS};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Picks the best of a list of results by `pick.by`, stores its text,
    /// and opens it. A criterion over prices, times, durations, or stops is
    /// ranked exactly, and the first ranked item that Jev confirms belongs
    /// to `pick.from` is taken: the ranking reads only its measure, and
    /// live, "the cheapest of the results rated 4 stars or more" took a
    /// 3.1-star item of another brand. Anything else, or a ranking none of
    /// whose leaders belongs, is judged by Jev among the records.
    pub(super) async fn pick(&mut self, log: &mut StepLog, pick: &PickStep) -> Result<Ended, Halt> {
        let from = substitute_safe(&pick.from, &self.vars, &self.facts);
        let by = substitute_safe(&pick.by, &self.vars, &self.facts);
        let mut screen = self.look().await?;
        self.explore(&mut screen).await;
        let families = result_families(&screen);
        if families.is_empty() {
            return Err(Halt::Failed(format!("no list of {from} is showing")));
        }
        // A page can repeat several things (a strip of dates above the
        // flights); a measurable criterion ranks the first list that has
        // the measure, and judgement falls to the longest.
        let ranked = Criterion::parse(&by).and_then(|criterion| {
            families.iter().find_map(|groups| {
                rank(&records_of(groups), criterion).map(|order| (groups, order))
            })
        });
        let belonging = match ranked {
            Some((groups, order)) => self
                .first_belonging(log, &screen, &from, groups, &order)
                .await?
                .map(|best| (groups, best)),
            None => None,
        };
        let (groups, best, how) = if let Some((groups, best)) = belonging {
            (groups, best, "ranked")
        } else {
            // Several lists show (a chat list beside the open chat's
            // messages): judge within the one `from` names.
            let list = if families.len() > 1 {
                self.judge_list(log, &screen, &from, &families).await?
            } else {
                0
            };
            let groups = &families[list];
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
        let reply = self
            .press_uncovering(log, "click", &primary, JevOperation::Click)
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

    /// Asks Jev which of the lists showing is `what`, each shown by its
    /// first [`LIST_PREVIEW`] items, among the first [`MAX_LISTS`]. A list
    /// not clearly chosen falls back to the longest, the one an `extract`
    /// took before it asked.
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
            return Ok(0);
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

/// Each card's text as a record, its fields numbered in reading order.
fn records_of(groups: &[Group]) -> Vec<Record> {
    groups
        .iter()
        .map(|group| Record {
            fields: group
                .fields
                .iter()
                .enumerate()
                .map(|(index, text)| (format!("field {index}"), text.clone()))
                .collect(),
        })
        .collect()
}
