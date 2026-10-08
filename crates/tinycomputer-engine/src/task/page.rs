//! The one web page a task's own words name, which its browser can load
//! while the plan is drafted ([`FlowRunner::open_page`]).
//!
//! [`FlowRunner::open_page`]: super::FlowRunner::open_page

/// Punctuation that can follow an address in a sentence, and is no part of
/// it.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"'];

/// The one web address `task` names, written out with its scheme
/// (`https://…` or `http://…`) and without the punctuation after it; `None`
/// when it names none, or several, which leave no one page to start on. An
/// address written twice, with a trailing slash or without, is one.
pub(super) fn named_page(task: &str) -> Option<String> {
    let mut named: Vec<&str> = Vec::new();
    for word in task.split_whitespace() {
        let Some(start) = word.find("https://").or_else(|| word.find("http://")) else {
            continue;
        };
        let address = word[start..].trim_end_matches(TRAILING);
        let has_host = address
            .split_once("://")
            .is_some_and(|(_, rest)| !rest.trim_start_matches('/').is_empty());
        let seen = named
            .iter()
            .any(|seen| seen.trim_end_matches('/') == address.trim_end_matches('/'));
        if has_host && !seen {
            named.push(address);
        }
    }
    match named.as_slice() {
        [page] => Some((*page).to_owned()),
        _ => None,
    }
}
