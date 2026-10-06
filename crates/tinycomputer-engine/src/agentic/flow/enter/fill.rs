//! Delivering slots: filling every pending slot a field or an option can
//! take, each text verified by reading it back.

use std::collections::BTreeSet;

use serde_json::Value;
use tinycomputer_bus::{JevOperation, Slot};
use tinycomputer_core::reformat_date;

use crate::agentic::flow::{
    AgentBackend, FlowRun, Halt, StepLog,
    ask::{self, Questions, corroborate, probability},
    backend::deliver_text,
    memory::{learn, remember},
    view::{Candidate, Screen, element_kind, is_destructive, label},
};

use super::{BLIND_PICK_MISSES, OPENER_FLOOR, REVEAL_TURNS, editable, names};

impl<B: AgentBackend + Sync> FlowRun<'_, B> {
    /// Presses the control on `screen` whose label holds a pending slot's
    /// own word (`named_opener`); `true` when it did. A control named by
    /// the slot, a search link for the "search box", is how the field
    /// shows: live, the turns that look for a way in hesitated over it, and
    /// the search was never typed.
    async fn open_by_name(
        &mut self,
        log: &mut StepLog,
        screen: &Screen,
        slots: &[Slot],
        pending: &BTreeSet<usize>,
    ) -> Result<bool, Halt> {
        let Some(opener) = named_opener(screen, slots, pending)
            .filter(|opener| !is_destructive(opener, screen, &self.stop_before))
        else {
            return Ok(false);
        };
        // A shared word is a hint, not a reason to press: a link named
        // "Email us" shares the slot "email", and pressing it left the form.
        let purpose = format!("click to show the box for: {}", names(slots, pending));
        let answers = self
            .ask(
                log,
                ask::request(
                    self.model(),
                    self.state(screen, &purpose),
                    Questions::default().with(
                        "confirm",
                        corroborate(&purpose, &opener, self.include_values),
                    ),
                ),
            )
            .await?;
        if probability(&answers, "confirm").is_none_or(|yes| yes < OPENER_FLOOR) {
            return Ok(false);
        }
        let pressed = opener.clone();
        let reply = self
            .act(
                log,
                "click (show the field)",
                Some(&opener),
                move |backend| backend.execute(JevOperation::Click, Some(pressed), None),
            )
            .await?;
        if reply.ok {
            self.history.push(format!(
                "pressed {} to show the field for {}",
                label(&opener),
                names(slots, pending)
            ));
        }
        Ok(reply.ok)
    }

    /// Runs a short `do` loop that shows the fields for the `pending` slots,
    /// any field at all when `no_fields` showed. A field that cannot be revealed
    /// is looked for another way, or found not to be asked for; it is not a
    /// failure.
    async fn reveal_fields(
        &mut self,
        log: &mut StepLog,
        slots: &[Slot],
        pending: &BTreeSet<usize>,
        no_fields: bool,
    ) -> Result<(), Halt> {
        let reveal = if no_fields {
            format!("show the editable fields for: {}", names(slots, pending))
        } else {
            format!("show the fields for: {}", names(slots, pending))
        };
        match self.accomplish(log, &reveal, REVEAL_TURNS).await {
            Err(Halt::Failed(note)) => self
                .history
                .push(format!("could not reveal the fields ({note})")),
            other => {
                other?;
            }
        }
        Ok(())
    }

    /// Fills every slot in `pending` it can find a field or an option for,
    /// removing each one that arrives.
    pub(super) async fn fill_pending(
        &mut self,
        log: &mut StepLog,
        slots: &[Slot],
        private: &[bool],
        pending: &mut BTreeSet<usize>,
    ) -> Result<(), Halt> {
        let mut revealed = false;
        let mut opened = false;
        // The fields this step already filled: one slot's box is never
        // another's. Live, a pickup box that had not yet become the place
        // chosen was the only box on screen in the next round, and the drop
        // was typed over the pickup.
        let mut filled_fields: BTreeSet<String> = BTreeSet::new();
        let mut filled_texts: Vec<String> = Vec::new();
        let mut saw_fields = false;
        // Fields that refused the text this step: a `div` a page labels a
        // combobox, or a field that would not hold what was typed. Offered
        // again, the same wrong field wins again — struck by `element_kind`
        // so the rows of a city list, each a same-kind box that differs only
        // by the city it holds, are struck together rather than one at a
        // time (`a_row_that_refused_the_text_is_never_pressed_while_revealing_a_field`).
        let mut struck: BTreeSet<String> = BTreeSet::new();
        for _ in 0..3 {
            if pending.is_empty() {
                break;
            }
            let mut screen = self.look().await?;
            if editable(&screen).len() < pending.len() && !screen.unexplored.is_empty() {
                self.explore(&mut screen).await;
            }
            let fields = unfilled(editable(&screen), &struck, &filled_fields, &filled_texts);
            saw_fields |= !fields.is_empty();
            let assignments = if fields.is_empty() {
                Vec::new()
            } else {
                self.assign(log, &screen, slots, pending, &fields).await?
            };
            if assignments.is_empty() {
                if !opened {
                    opened = true;
                    if self.open_by_name(log, &screen, slots, pending).await? {
                        continue;
                    }
                }
                if revealed {
                    break;
                }
                revealed = true;
                self.reveal_fields(log, slots, pending, fields.is_empty())
                    .await?;
                continue;
            }
            for assignment in assignments {
                let slot = &slots[assignment.slot];
                let filled = self
                    .fill(
                        log,
                        slot,
                        &assignment.field,
                        &screen,
                        private[assignment.slot],
                    )
                    .await?;
                if !filled {
                    struck.insert(element_kind(&assignment.field));
                    self.refused.insert(element_kind(&assignment.field));
                    self.ledger.tried(format!(
                        "{} did not take the {}",
                        label(&assignment.field),
                        slot.slot
                    ));
                }
                if filled {
                    filled_fields.insert(assignment.field.ref_id.clone());
                    filled_texts.push(slot.text.split_whitespace().collect::<Vec<_>>().join(" "));
                    pending.remove(&assignment.slot);
                    learn(
                        &mut self.learned,
                        remember(&self.app, &slot.slot, &assignment.field),
                    );
                    log.confidence = Some(log.confidence.map_or(assignment.probability, |seen| {
                        seen.min(assignment.probability)
                    }));
                }
            }
        }
        // A value with no field to type into is picked instead, as a date
        // from a calendar or a city from a list of suggestions. Only a "the
        // value was not found" failure is safe to shrug off and move to the
        // next slot; a budget stop or a backend error means acting further
        // is unsafe or pointless, and must end the step instead of being
        // read as "this slot has no picker".
        let mut missed = 0;
        for index in pending.clone() {
            if !saw_fields && missed >= BLIND_PICK_MISSES {
                self.history.push(format!(
                    "no field and no picker on this screen; not looking for: {}",
                    names(slots, pending)
                ));
                break;
            }
            let slot = &slots[index];
            match self
                .pick_option(log, &slot.slot, &slot.text, private[index], false)
                .await
            {
                Ok(_) => {
                    pending.remove(&index);
                }
                Err(Halt::Failed(_)) => missed += 1,
                Err(halt) => return Err(halt),
            }
        }
        Ok(())
    }

    /// Delivers one slot's text and reports whether it verifiably arrived.
    ///
    /// A date is typed in the layout the field or the page around it asks
    /// for ("DD-MM-YYYY"), so an input mask does not mangle it. Text that
    /// arrived and opened a list of suggestions has the matching one picked
    /// (`commit_suggestion`), since an autocomplete box keeps it only then; a
    /// `private` text is never offered there, as Jev would see it.
    async fn fill(
        &mut self,
        log: &mut StepLog,
        slot: &Slot,
        field: &Candidate,
        before: &Screen,
        private: bool,
    ) -> Result<bool, Halt> {
        let app = self.app.clone();
        let target = field.clone();
        let hints = [field.name.as_deref(), field.description.as_deref()]
            .into_iter()
            .flatten()
            .chain(before.context.iter().map(String::as_str));
        let text = reformat_date(&slot.text, hints).unwrap_or_else(|| slot.text.clone());
        let typed = text.clone();
        let reply = self
            .act(
                log,
                &format!("fill {}", slot.slot),
                Some(field),
                move |backend| deliver_text(&backend, &app, &target, &text),
            )
            .await?;
        self.typed.insert(element_kind(field));
        let path = reply
            .data
            .as_ref()
            .and_then(|data| data.get("path"))
            .cloned()
            .unwrap_or(Value::Null);
        self.history.push(format!(
            "entered the {} into {} ok={} via {path}",
            slot.slot,
            label(field),
            reply.ok
        ));
        if reply.ok && !private {
            self.commit_suggestion(log, &slot.slot, &typed, field, before)
                .await?;
        }
        Ok(reply.ok)
    }
}

/// Words of a slot's name that say only that it is a box, or name a kind
/// of control rather than what the slot is.
const BOX_WORDS: &[&str] = &[
    "field", "box", "input", "bar", "the", "your", "text", "here", "link", "button", "menu", "icon",
];

/// A control on `screen` that is no field itself and whose label holds a
/// word of a pending slot's name (four letters or more, not a box word):
/// the link or button that shows the slot's field, such as a store's
/// search link for the slot "search box".
fn named_opener(screen: &Screen, slots: &[Slot], pending: &BTreeSet<usize>) -> Option<Candidate> {
    let wanted = pending
        .iter()
        .filter_map(|index| slots.get(*index))
        .flat_map(|slot| words(&slot.slot))
        .filter(|word| word.chars().count() > 3 && !BOX_WORDS.contains(&word.as_str()))
        .collect::<BTreeSet<_>>();
    if wanted.is_empty() {
        return None;
    }
    let fields = editable(screen);
    screen
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(candidate.role.as_str(), "link" | "button")
                && candidate
                    .available_actions
                    .iter()
                    .any(|action| action == "Click")
                && !fields.iter().any(|field| field.ref_id == candidate.ref_id)
                && !candidate
                    .states
                    .iter()
                    .any(|state| state.eq_ignore_ascii_case("covered"))
        })
        .find(|candidate| {
            words(candidate.name.as_deref().unwrap_or_default())
                .iter()
                .take(3)
                .any(|word| wanted.contains(word))
        })
        .cloned()
}

/// The lower-case words of `text`.
fn words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The `fields` a slot may still take: of no kind struck this step, and
/// neither filled this step (`filled`, by ref) nor holding a text this step
/// delivered (`delivered`).
fn unfilled(
    fields: Vec<Candidate>,
    struck: &BTreeSet<String>,
    filled: &BTreeSet<String>,
    delivered: &[String],
) -> Vec<Candidate> {
    fields
        .into_iter()
        .filter(|field| {
            !struck.contains(&element_kind(field))
                && !filled.contains(&field.ref_id)
                && !holds_one_of(field, delivered)
        })
        .collect()
}

/// Whether `field` holds one of the texts this step already delivered:
/// one slot's box, read again under a new ref.
fn holds_one_of(field: &Candidate, delivered: &[String]) -> bool {
    field
        .value
        .as_ref()
        .and_then(Value::as_str)
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .is_some_and(|value| !value.is_empty() && delivered.contains(&value))
}
