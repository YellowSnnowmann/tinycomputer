//! What is in front of the page, and whether the run's own press put it
//! there: a dialog the task opened is the flow's next stage, never cleared
//! as a distraction or an obstacle, in this step or the next.

use super::{
    steps::names_a_month,
    view::{Candidate, Screen},
};

/// Controls a layer must cover, beyond what was covered before the press
/// that opened it, before it counts as in front of the page.
pub(super) const LAYER_COVERS: usize = 3;

/// Day cells nothing covers that make what is in front a calendar: a week.
const CALENDAR_DAYS: usize = 7;

/// The history note for a dialog left open by the run before a rescue.
const LEFT_OPEN: &str = "a dialog the task opened before is still in front: it is the task's current stage, so work within it";

/// The history note for a dialog the run's last press opened.
const OPENED: &str = "the last press opened a dialog or panel over the page: it is the task's next stage, so answer what it asks (a format, a quantity, a date, a place) and continue with its own button";

/// What the run did since its last look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Acted {
    /// Nothing that pressed an element.
    #[default]
    Nothing,
    /// Pressed an element, and the last action did not type.
    Pressed,
    /// Pressed or typed into an element, and the last action typed: the
    /// list of suggestions typing opens is the field's, not a dialog of the
    /// task's (live, a search box's suggestions were taken for one, and the
    /// next step was told to answer them).
    Typed,
}

/// Whose the dialog in front is, and how far the task has worked in it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Dialog {
    /// None of the task's: the window, or what the page itself put in
    /// front.
    #[default]
    Page,
    /// The task's own, opened by its press (or left open by its run before)
    /// and not pressed in since: the question it asks first, such as a
    /// format or a quantity. Live, such a dialog was closed at the start of
    /// the following step.
    Asking,
    /// The task's own, pressed in since it opened.
    Answered,
    /// The task's own, worked in by a step before this one: no longer its
    /// question, and in the way of the next step, but in front until the
    /// window is (a calendar left open after its day was chosen).
    Served,
}

/// What was in front on the last look, and how the run's own actions
/// brought it there.
#[derive(Debug, Clone)]
pub(super) struct Front {
    /// `window`, a dialog such as `sheet`, or `layer`: something drawn
    /// over the window that covers its controls.
    pub(super) surface: String,
    /// What the run did since the last look.
    pub(super) acted: Acted,
    /// Whose the dialog in front is ([`Dialog`]).
    pub(super) dialog: Dialog,
    /// What the run's first look takes a dialog in front for, until it has
    /// looked or browsed: the task's own when the task's run before this one
    /// left its own dialog there (`true`; a rescue continues where that run
    /// stopped), else the page's.
    first_look: Option<bool>,
    /// How many controls something covered on the last look with nothing
    /// in front: what a sticky header always covers, which a layer a press
    /// opens must add to before it counts.
    pub(super) covered_base: usize,
    /// The address and window title of the last look: a press after which
    /// either changed went to another page, and what covers that page is
    /// the page's own, not an answer the press asked for.
    pub(super) looked_at: (Option<String>, Option<String>),
    /// Whether the last look showed a calendar ([`holds_calendar`]).
    pub(super) calendar: bool,
}

/// What the run's own housekeeping does, never a press of the task's: a
/// distraction cleared, a dismissal, an undo, an uncovering Escape, the
/// gated press of a `stop_before`.
const HOUSEKEEPING: &[&str] = &[
    "(clear distraction)",
    "(dismiss)",
    "(undo)",
    "(uncover)",
    "(irreversible)",
];

impl Default for Front {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Front {
    /// A run's front before its first look: `inherited` when the task's run
    /// before this one left its own dialog in front.
    pub(super) fn new(inherited: bool) -> Self {
        Self {
            surface: "window".to_owned(),
            acted: Acted::Nothing,
            dialog: Dialog::Page,
            first_look: Some(inherited),
            covered_base: 0,
            looked_at: (None, None),
            calendar: false,
        }
    }

    /// Whether the dialog in front is the task's own and still its stage:
    /// opened by its press, or left open by its run before, and not yet
    /// handed back by a step that worked in it.
    pub(super) fn opened_dialog(&self) -> bool {
        matches!(self.dialog, Dialog::Asking | Dialog::Answered)
    }

    /// Whether the run leaves the task's own dialog in front, for the task's
    /// next run ([`tinycomputer_bus::RunFlowRequest::dialog_left_open`]): one
    /// it opened, or the one its run before left there while it has not
    /// looked since. A run that ends before its first look changed nothing
    /// in front.
    pub(super) fn left_open(&self) -> bool {
        self.opened_dialog() || self.first_look == Some(true)
    }

    /// Notes one action, `action` as the run logs it (`click`, `fill …`,
    /// `browse …`, `click (dismiss)`), on `target` when it had one.
    ///
    /// Only the task's own presses and typing count: a scroll moves no
    /// question into view, and the run's housekeeping (a distraction
    /// cleared, an undo) answers none. Nor does turning a calendar's month
    /// (`turns_the_month`): the calendar still asks for its day. Opening an
    /// address or an application leaves whatever was in front behind.
    pub(super) fn act(&mut self, action: &str, target: Option<&Candidate>) {
        let targeted = target.is_some();
        if action.starts_with("browse ") {
            *self = Self {
                first_look: None,
                ..Self::default()
            };
            return;
        }
        if action.starts_with("launch ") {
            *self = Self {
                first_look: self.first_look,
                ..Self::default()
            };
            return;
        }
        let housekeeping = HOUSEKEEPING.iter().any(|kind| action.contains(kind));
        let pressing = targeted && !housekeeping && !action.starts_with("scroll");
        if pressing || (self.acted != Acted::Nothing && !housekeeping) {
            let typing = action.starts_with("fill") || action.starts_with("type");
            self.acted = if typing { Acted::Typed } else { Acted::Pressed };
        }
        if pressing && self.dialog == Dialog::Asking && !target.is_some_and(turns_the_month) {
            self.dialog = Dialog::Answered;
        }
    }

    /// Whether the dialog in front is a calendar the task opened and has
    /// pressed in since, in this step or one before: a picker that has
    /// served its field, in the way of a press behind it rather than asking
    /// anything. Live, a calendar stayed in front of the guests and Search
    /// buttons once both dates were picked, a step each, and every press
    /// behind it was refused.
    pub(super) fn served_calendar(&self) -> bool {
        matches!(self.dialog, Dialog::Answered | Dialog::Served) && self.calendar
    }

    /// Begins a step: a dialog the task opened and then worked in has
    /// served the step before, and is no longer the task's own; one the
    /// last step's last press opened still asks its question (a format
    /// dialog a booking button raised).
    pub(super) fn next_step(&mut self) {
        if self.dialog == Dialog::Answered {
            self.dialog = Dialog::Served;
        }
    }

    /// Takes in a look at `screen`, at the address `location`: what is in
    /// front, and whether the task opened it. The note for the history
    /// when the dialog in front became the task's. A dialog at a run's first
    /// look is the task's only when the task's run before this one left its
    /// own dialog in front ([`Front::new`]): live, a sign-up the page opened
    /// on load, and a site's menu drawer, were taken for the task's at a
    /// rescue's first look, and every move past them was refused.
    pub(super) fn look(&mut self, screen: &Screen, location: Option<&str>) -> Option<&'static str> {
        let left_open = self.first_look.take() == Some(true);
        self.calendar = holds_calendar(screen);
        let front = self.front_of(screen, location);
        let note = if front == "window" {
            self.dialog = Dialog::Page;
            None
        } else if left_open && !self.opened_dialog() {
            self.dialog = Dialog::Asking;
            Some(LEFT_OPEN)
        } else if self.acted != Acted::Nothing && self.surface == "window" {
            self.dialog = Dialog::Asking;
            Some(OPENED)
        } else {
            None
        };
        self.surface = front;
        self.acted = Acted::Nothing;
        note
    }

    /// What is in front on `screen`: its surface when that is not the
    /// window (a sheet, a dialog); `"layer"` when something drawn over the
    /// window covers [`LAYER_COVERS`] more controls than before the press
    /// that opened it, on the same page; `"window"` otherwise.
    ///
    /// A popover a press opens need not be a dialog to the page: live, a
    /// store's delivery-place prompt after "Add" read as a plain window,
    /// and was cleared as a distraction. Only a press opens a layer, and
    /// only in place: a promotion that greets a newly opened page is the
    /// page's, and a sticky header always covers what scrolls under it.
    fn front_of(&mut self, screen: &Screen, location: Option<&str>) -> String {
        let covered = covered_count(screen);
        let here = (location.map(str::to_owned), screen.window.clone());
        let moved = here != self.looked_at;
        self.looked_at = here;
        if screen.surface != "window" {
            return screen.surface.clone();
        }
        let layered = covered >= self.covered_base.saturating_add(LAYER_COVERS)
            && (self.surface == "layer" || (self.acted == Acted::Pressed && !moved));
        if layered {
            return "layer".to_owned();
        }
        self.covered_base = covered;
        "window".to_owned()
    }
}

/// How many controls on `screen` something else covers.
fn covered_count(screen: &Screen) -> usize {
    screen
        .candidates
        .iter()
        .filter(|candidate| {
            candidate
                .states
                .iter()
                .any(|state| state.eq_ignore_ascii_case("covered"))
        })
        .count()
}

/// Whether `target` only turns a calendar's month ("next month", "Previous
/// month", or a "Next" described "next month"), which answers nothing the
/// calendar asks.
fn turns_the_month(target: &Candidate) -> bool {
    [target.name.as_deref(), target.description.as_deref()]
        .into_iter()
        .flatten()
        .any(|text| {
            let words = text
                .split(|character: char| !character.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect::<Vec<_>>();
            let said = words.join(" ");
            words.len() <= 4 && (said.contains("next month") || said.contains("previous month"))
        })
}

/// The day of the month `word` names: 1 to 31, with a leading zero or an
/// ordinal ending ("01", "22nd").
fn day_of(word: &str) -> Option<u8> {
    let digits = ["st", "nd", "rd", "th"]
        .iter()
        .find_map(|ending| {
            word.to_ascii_lowercase()
                .strip_suffix(ending)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| word.to_owned());
    digits
        .parse::<u8>()
        .ok()
        .filter(|day| (1..=31).contains(day))
}

/// Whether `screen` shows a calendar: [`CALENDAR_DAYS`] or more day numbers
/// nothing covers, each beside a month's name ("1 September 2026", or "1"
/// described "Thu Oct 01 2026"), or a grid cell on a screen that names a
/// month somewhere: a seat map's or a table's cells are bare numbers too.
/// A short label that names a month holds its day anywhere ("Thu Oct 01
/// 2026", "Choose Thursday, October 22nd, 2026").
fn holds_calendar(screen: &Screen) -> bool {
    let month_shown = screen
        .candidates
        .iter()
        .chain(&screen.text_nodes)
        .flat_map(|node| [node.name.as_deref(), node.description.as_deref()])
        .flatten()
        .chain(screen.context.iter().map(String::as_str))
        .any(names_a_month);
    screen
        .candidates
        .iter()
        .filter(|candidate| {
            let name = candidate.name.as_deref().unwrap_or_default();
            let day = name.split_whitespace().next().and_then(day_of).is_some()
                || (names_a_month(name)
                    && name.split_whitespace().count() <= 6
                    && name
                        .split(|character: char| !character.is_alphanumeric())
                        .any(|word| day_of(word).is_some()));
            let dated = (month_shown && candidate.role.eq_ignore_ascii_case("gridcell"))
                || names_a_month(name)
                || candidate.description.as_deref().is_some_and(names_a_month);
            let covered = candidate
                .states
                .iter()
                .any(|state| state.eq_ignore_ascii_case("covered"));
            day && dated && !covered
        })
        .count()
        >= CALENDAR_DAYS
}
