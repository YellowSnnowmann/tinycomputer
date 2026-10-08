//! Attention: the root of every turn's decision tree.
//!
//! Before a step is judged or an element grounded, the runtime asks what on
//! the screen needs attention first: the step itself, or a distraction — a
//! cookie or privacy card, a promo toast lying over the results, a
//! newsletter prompt. A distraction left in place takes Jev's attention,
//! covers the element a step needs, and turns a click into a miss.
//!
//! The candidates are found without asking anyone ([`distractions`](find::distractions)): the
//! regions the screen digest puts in front (dialogs, consent and newsletter
//! regions), and any region holding a plain dismiss control (×, Close, Not
//! now, Reject all). A region the step itself names is the step's business,
//! not a distraction, and a control that looks irreversible is never offered.
//! Only when there is a candidate is Jev asked, with one Choice, and only a
//! clearly agreed pick (`evidence/`) is cleared, with the region's
//! least-committal control: rejecting or essential-only first, closing next,
//! accepting last. One exception asks no one: when the page refuses a press
//! as covered, the least-committal control of a layer in front, other than
//! the step's own, is pressed before the press is tried again
//! ([`front_closer`]).
//!
//! `find` finds the distractions without asking anyone, and `clear` asks
//! Jev and clears the one it picks.

mod clear;
mod find;

use std::collections::BTreeSet;

use super::view::{Candidate, Screen, signature};

/// Most distractions one attention question offers.
pub(super) const MAX_DISTRACTIONS: usize = 4;
/// Distractions cleared per step at most.
pub(super) const MAX_CLEARED: u32 = 3;
/// Least probability a distraction must win the attention Choice with.
pub(super) const ATTENTION_FLOOR: f64 = 0.5;
/// Most elements a distraction holds. A toast, a consent card, or a prompt
/// is small; a container holding more is the page, and its "close" icon
/// clears a field or a panel the step may need (live on Emirates, the
/// booking form's clear icons sat directly under `main`).
pub(super) const MAX_DISTRACTION_SIZE: usize = 12;

/// Something on screen that may need clearing before the step.
#[derive(Debug, Clone)]
pub(super) struct Distraction {
    /// Where it sits: the digest's name for its region.
    pub(super) name: String,
    /// A few of its labels, as Jev reads them.
    pub(super) shows: Vec<String>,
    /// The control that clears it, least committal of those it holds; `None`
    /// for something that covers the page with no control of its own, which
    /// Escape clears.
    pub(super) closer: Option<Candidate>,
    /// Whether it lies in front of the page (the digest's front regions).
    pub(super) front: bool,
}

/// The control that closes a layer in front of the page, the least
/// committal its region holds (a consent banner's "Allow Selection" before
/// its "Allow all"), when one does and it was not pressed in this step: what
/// a press of `target` the page refused as covered clears before trying
/// again. Live, a consent banner lay over "Add To Cart", Escape left it
/// there, and every press was refused. A layer the step's `intent` names,
/// or the one `target` itself sits in (a popover a toast lies over), is the
/// step's, and is never closed this way.
pub(super) fn front_closer(
    screen: &Screen,
    target: &Candidate,
    intent: &str,
    stop_before: &[String],
    cleared: &BTreeSet<String>,
) -> Option<Candidate> {
    let pressed = signature(target);
    find::distractions(screen, intent, stop_before, cleared)
        .into_iter()
        .filter(|distraction| distraction.front)
        .filter_map(|distraction| distraction.closer)
        .find(|closer| signature(closer) != pressed && !target.path.starts_with(&closer.path))
}

/// The key a step's Escape at something covering the page is remembered
/// under, so a covering Escape did not close is not offered again.
pub(super) const ESCAPED: &str = "escape: whatever covers the page";

/// What a step has cleared so far.
#[derive(Debug, Default)]
pub(super) struct Cleared {
    /// Signatures of the controls pressed.
    pub(super) pressed: BTreeSet<String>,
    /// How many.
    pub(super) count: u32,
}

#[cfg(test)]
mod attention_tests;
