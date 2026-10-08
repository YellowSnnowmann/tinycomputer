//! The one web page a task's own words send its browser to, which the
//! browser can load while the plan is drafted ([`FlowRunner::open_page`]).
//!
//! [`FlowRunner::open_page`]: super::FlowRunner::open_page

/// Punctuation that can follow an address in a sentence, and is no part of
/// it.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"'];

/// Punctuation that can come between an address and the words before it.
const OPENING: &[char] = &['(', '[', '<', '"', '\'', ':'];

/// Words that, right before an address, send the browser there: the task
/// starts on that page, rather than reading, checking, or passing on an
/// address it only mentions.
const SENDS_TO: &[&str] = &[
    "go to",
    "goto",
    "open",
    "visit",
    "navigate to",
    "browse to",
    "head to",
    "start at",
    "start on",
    "on",
    "at",
];

/// The one web address `task` names, written out with its scheme
/// (`https://…` or `http://…`) and without the punctuation after it, when
/// the words before it send the browser there and it carries no query or
/// fragment, which can hold a token a load would spend. `None` when it names
/// none, or several, which leave no one page to start on. An address written
/// twice, with a trailing slash or without, is one.
pub(super) fn named_page(task: &str) -> Option<String> {
    let mut named: Vec<(&str, &str)> = Vec::new();
    let mut from = 0;
    while let Some(found) = scheme_at(&task[from..]) {
        let start = from + found;
        let end = task[start..]
            .find(char::is_whitespace)
            .map_or(task.len(), |length| start + length);
        from = end;
        let address = task[start..end].trim_end_matches(TRAILING);
        let has_host = address
            .split_once("://")
            .is_some_and(|(_, rest)| !rest.trim_start_matches('/').is_empty());
        let seen = named
            .iter()
            .any(|(seen, _)| seen.trim_end_matches('/') == address.trim_end_matches('/'));
        if has_host && !seen {
            named.push((address, &task[..start]));
        }
    }
    match named.as_slice() {
        [(page, before)] if sent_to(before) && !page.contains(['?', '#']) => {
            Some((*page).to_owned())
        }
        _ => None,
    }
}

/// Where the next `https://` or `http://` in `text` starts.
fn scheme_at(text: &str) -> Option<usize> {
    [text.find("https://"), text.find("http://")]
        .into_iter()
        .flatten()
        .min()
}

/// Whether `before`, a task's words up to an address, ends with words that
/// send the browser there.
fn sent_to(before: &str) -> bool {
    let before = before
        .trim_end_matches(|character: char| {
            character.is_whitespace() || OPENING.contains(&character)
        })
        .to_lowercase();
    SENDS_TO.iter().any(|words| {
        before
            .strip_suffix(words)
            .is_some_and(|ahead| ahead.is_empty() || ahead.ends_with(char::is_whitespace))
    })
}
