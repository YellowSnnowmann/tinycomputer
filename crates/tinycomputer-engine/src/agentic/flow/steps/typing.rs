//! A plain step that asks for typing, read as the `enter` it means.

use tinycomputer_bus::Slot;

/// Verbs a plain step types with, longest first so "fill in" wins over
/// "fill".
const VERBS: &[&str] = &[
    "fill in ", "key in ", "enter ", "type ", "input ", "fill ", "write ",
];

/// Words a description of a text starts with, rather than the text itself:
/// "enter your name in the name field" names what to type without saying
/// it, and typing "your name" would be worse than stalling.
const DESCRIBED: &[&str] = &["the ", "your ", "my ", "a ", "an ", "some ", "any "];

/// Trailing words that say a slot is a box, dropped from its name.
const BOX_WORDS: &[&str] = &[" field", " box", " input", " textbox", " text box", " bar"];

/// The slot a plain step such as "enter 560001 into the pincode field" or
/// "type 'Maggi' in the search box" asks to fill, when it asks for typing.
///
/// A `do` step cannot type: its moves press, scroll, and wait. Live, a
/// planner and its rescuer kept writing typing as plain steps despite the
/// guide, and each one stalled. The text is split at the first " into ",
/// or else at the last " in ", " as ", or " for ", so a text with "in" in
/// it keeps its words. `None` when the step is not typing.
pub(in crate::agentic::flow) fn typing(intent: &str) -> Option<Slot> {
    let trimmed = intent.trim().trim_end_matches('.');
    let lower = trimmed.to_ascii_lowercase();
    let verb = VERBS.iter().find(|verb| lower.starts_with(**verb))?;
    let start = verb.len();
    let rest = lower.get(start..)?;
    // "fill in the pincode with 560001" names the field first.
    if verb.starts_with("fill")
        && let Some(at) = rest.rfind(" with ")
    {
        let field = trimmed.get(start..start + at)?;
        let text = trimmed.get(start + at + " with ".len()..)?;
        return slot(field, text);
    }
    let (at, joint) = rest.find(" into ").map_or_else(
        || {
            [" in ", " as ", " for "]
                .iter()
                .filter_map(|joint| rest.rfind(joint).map(|at| (at, *joint)))
                .max_by_key(|(at, _)| *at)
        },
        |at| Some((at, " into ")),
    )?;
    let text = trimmed.get(start..start + at)?;
    let field = trimmed.get(start + at + joint.len()..)?;
    slot(field, text)
}

/// The slot `field` names, holding `text`, unless `text` only describes
/// what to type.
fn slot(field: &str, text: &str) -> Option<Slot> {
    let text = unquote(text.trim());
    let lower = text.to_ascii_lowercase();
    if DESCRIBED.iter().any(|word| lower.starts_with(word)) && !text.contains("${") {
        return None;
    }
    let mut field = field.trim();
    for article in ["the ", "a ", "an "] {
        if field.len() > article.len()
            && field
                .get(..article.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(article))
        {
            field = field.get(article.len()..)?;
        }
    }
    let mut slot = field.trim().to_owned();
    for word in BOX_WORDS {
        if slot.len() > word.len() && slot.to_ascii_lowercase().ends_with(word) {
            slot.truncate(slot.len() - word.len());
        }
    }
    let slot = slot.trim().to_owned();
    (!text.is_empty() && !slot.is_empty()).then(|| Slot {
        slot,
        text: text.to_owned(),
    })
}

/// `text` without the quotes a step put around it.
fn unquote(text: &str) -> &str {
    for (open, close) in [('"', '"'), ('\'', '\''), ('“', '”'), ('‘', '’')] {
        if let Some(inner) = text
            .strip_prefix(open)
            .and_then(|inner| inner.strip_suffix(close))
        {
            return inner.trim();
        }
    }
    text
}
