//! A pressed control's copies on the other items of its list, struck off
//! for the rest of the step once the press shows it did something.

use crate::agentic::flow::view::{Candidate, Screen, label, press_key};

use super::DoState;

/// The copies of `pressed` on `screen`, by press key: controls with its
/// label on the other items of the same list. A control with the same
/// label elsewhere (a sticky bar's "Add to cart") is no copy: live, the
/// main button was refused and the bar's own was struck off with it.
pub(in crate::agentic::flow) fn copies_of(screen: &Screen, pressed: &Candidate) -> Vec<String> {
    let pressed_label = label(pressed);
    let list = list_of(&pressed.path);
    screen
        .candidates
        .iter()
        .filter(|candidate| {
            label(candidate) == pressed_label
                && candidate.path != pressed.path
                && list_of(&candidate.path) == list
        })
        .map(press_key)
        .collect()
}

/// Strikes off the copies the last press left pending, now that the next
/// look is in: only when the press `changed` the screen, since a press that
/// was refused or did nothing acted on no item. The note for the history
/// when any were struck.
pub(super) fn strike_pending(state: &mut DoState, changed: bool) -> Option<String> {
    let (key, pressed_label, copies) = state.pending_copies.take()?;
    if !changed {
        return None;
    }
    let struck = copies
        .into_iter()
        .filter(|copy| state.banned.insert(copy.clone()))
        .collect::<Vec<_>>();
    if struck.is_empty() {
        return None;
    }
    state.copies.insert(key, struck);
    Some(format!(
        "{pressed_label} is repeated on other items; pressing another copy would act on a different item, so only the one pressed counts for this step"
    ))
}

/// Lifts the copies the press of `undone` struck off: an undone press may
/// have been the wrong item's, and its copies are candidates again.
pub(super) fn lift(state: &mut DoState, undone: &Candidate) {
    for copy in state.copies.remove(&press_key(undone)).unwrap_or_default() {
        state.banned.remove(&copy);
    }
}

/// The list a control sits in: its path without the numbered items it ends
/// in, since each card of a list sits in its own `listitem #n`.
fn list_of(path: &[String]) -> &[String] {
    let numbered = |segment: &&String| {
        segment
            .rsplit_once(" #")
            .is_some_and(|(_, number)| number.parse::<u32>().is_ok())
    };
    let kept = path.len() - path.iter().rev().take_while(numbered).count();
    &path[..kept]
}
