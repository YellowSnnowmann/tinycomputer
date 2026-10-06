//! A plain step that asks for typing, read as the `enter` it means.

use tinycomputer_bus::Slot;

/// Verbs a plain step types with, longest first so "fill in" wins over
/// "fill" and "type in" over "type".
const VERBS: &[&str] = &[
    "fill in ", "key in ", "type in ", "enter ", "type ", "input ", "fill ", "write ",
];

/// Words a description of a text starts with, rather than the text itself:
/// "enter your name in the name field" names what to type without saying
/// it, and typing "your name" would be worse than stalling.
const DESCRIBED: &[&str] = &["the ", "your ", "my ", "a ", "an ", "some ", "any "];

/// Words that may stand around a lone `${name}` without being part of the
/// text: "enter the ${otp} in the box" types the code, not "the 123456".
const AROUND_A_NAME: &[&str] = &["the", "your", "my", "a", "an", "code", "value", "number"];

/// Trailing words that say a slot is a box, dropped from its name.
const BOX_WORDS: &[&str] = &[" field", " box", " input", " textbox", " text box", " bar"];

/// What joins the text to the field it goes into.
const JOINTS: &[&str] = &[" into ", " in ", " as ", " for "];

/// What says a step goes on to do more than type: "type 'Maggi' in the
/// search box and press Enter" is two steps, and its field is not "search
/// box and press Enter".
const FOLLOW_ON: &[&str] = &[
    " and press ",
    " and click ",
    " and hit ",
    " and tap ",
    " and submit",
    " and search",
    " then ",
];

/// The slot a plain step such as "enter 560001 into the pincode field" or
/// "type 'Maggi' in the search box" asks to fill, when it asks for typing.
///
/// A `do` step cannot type: its moves press, scroll, and wait. Live, a
/// planner and its rescuer kept writing typing as plain steps despite the
/// guide, and each one stalled. A quoted text ends at its quote; otherwise
/// the text is split at the first " into ", or else at the last " in ",
/// " as ", or " for " whose field names a box (the last at all when none
/// does), so a text with "in" in it keeps its words. "Enter" also means
/// going into something ("enter Reader mode in Safari"), so with " in " it
/// types only what is quoted, data (digits, an address, a `${name}`), or
/// into what names a box. `None` when the step is not typing, or does more
/// than type.
pub(in crate::agentic::flow) fn typing(intent: &str) -> Option<Slot> {
    let trimmed = intent.trim().trim_end_matches('.');
    let lower = trimmed.to_ascii_lowercase();
    if FOLLOW_ON.iter().any(|more| lower.contains(more)) {
        return None;
    }
    let verb = VERBS.iter().find(|verb| lower.starts_with(**verb))?;
    let start = verb.len();
    let rest = lower.get(start..)?;
    let original = trimmed.get(start..)?;
    // "fill in the pincode with 560001" names the field first.
    if verb.starts_with("fill")
        && let Some(at) = rest.rfind(" with ")
    {
        let field = original.get(..at)?;
        let text = original.get(at + " with ".len()..)?;
        return slot(field, text);
    }
    let (at, joint) = split(rest)?;
    let text = original.get(..at)?;
    let field = original.get(at + joint.len()..)?;
    if verb.trim() == "enter" && joint == " in " && !(data(text) || names_a_box(field)) {
        return None;
    }
    slot(field, text)
}

/// Where `rest` (lower-case, after the verb) splits into the text and the
/// field, and the joint it splits at.
fn split(rest: &str) -> Option<(usize, &'static str)> {
    // A quoted text ends at its closing quote, whatever words it holds.
    if let Some(open) = rest.chars().next().filter(|open| QUOTES.contains(open)) {
        let close = closing(open);
        let end = rest
            .char_indices()
            .skip(1)
            .find(|(_, character)| *character == close)
            .map(|(at, character)| at + character.len_utf8())?;
        let after = rest.get(end..)?;
        let joint = JOINTS.iter().find(|joint| after.starts_with(**joint))?;
        return Some((end, joint));
    }
    if let Some(at) = rest.find(" into ") {
        return Some((at, " into "));
    }
    let splits = [" in ", " as ", " for "]
        .iter()
        .flat_map(|joint| rest.match_indices(*joint).map(|(at, _)| (at, *joint)))
        .collect::<Vec<_>>();
    splits
        .iter()
        .filter(|(at, joint)| names_a_box(rest.get(at + joint.len()..).unwrap_or_default()))
        .max_by_key(|(at, _)| *at)
        .or_else(|| splits.iter().max_by_key(|(at, _)| *at))
        .copied()
}

/// Quote marks a step may put around its text.
const QUOTES: &[char] = &['"', '\'', '“', '‘'];

/// The quote that closes `open`.
fn closing(open: char) -> char {
    match open {
        '“' => '”',
        '‘' => '’',
        other => other,
    }
}

/// Whether `field` names a box: "the search bar", "the pincode field".
fn names_a_box(field: &str) -> bool {
    let words = field.to_ascii_lowercase();
    BOX_WORDS
        .iter()
        .any(|word| words.split_whitespace().any(|own| own == word.trim()) || words.contains(word))
}

/// Whether `text` reads as data to type rather than a place to go: quoted,
/// a `${name}`, or holding a digit or an `@`.
fn data(text: &str) -> bool {
    let text = text.trim();
    text.contains("${")
        || text
            .chars()
            .any(|character| character.is_ascii_digit() || character == '@')
        || text
            .chars()
            .next()
            .is_some_and(|open| QUOTES.contains(&open) && text.ends_with(closing(open)))
}

/// The slot `field` names, holding `text`, unless `text` only describes
/// what to type.
fn slot(field: &str, text: &str) -> Option<Slot> {
    let text = unquote(text.trim());
    let text = lone_name(text).unwrap_or(text);
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

/// The one `${name}` in `text`, when every other word around it only
/// describes it ("the ${otp}", "${otp} code").
fn lone_name(text: &str) -> Option<&str> {
    let start = text.find("${")?;
    let end = start + text.get(start..)?.find('}')? + 1;
    let (before, after) = (text.get(..start)?, text.get(end..)?);
    if after.contains("${") {
        return None;
    }
    before
        .split_whitespace()
        .chain(after.split_whitespace())
        .all(|word| AROUND_A_NAME.contains(&word.to_ascii_lowercase().as_str()))
        .then(|| text.get(start..end))
        .flatten()
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
