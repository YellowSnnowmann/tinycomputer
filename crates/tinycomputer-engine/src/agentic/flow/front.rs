//! What is in front of the page, and whether the run's own press put it
//! there: a dialog the task opened is the flow's next stage, never cleared
//! as a distraction or an obstacle, in this step or the next.

use super::view::Screen;

/// Controls a layer must cover, beyond what was covered before the press
/// that opened it, before it counts as in front of the page.
pub(super) const LAYER_COVERS: usize = 3;

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

/// What was in front on the last look, and how the run's own actions
/// brought it there.
#[derive(Debug, Clone)]
pub(super) struct Front {
    /// `window`, a dialog such as `sheet`, or `layer`: something drawn
    /// over the window that covers its controls.
    pub(super) surface: String,
    /// What the run did since the last look.
    pub(super) acted: Acted,
    /// Whether the dialog in front was opened by the run's own press (a
    /// question a booking or purchase button asks first, such as a format
    /// or a quantity). Live, such a dialog was closed at the start of the
    /// following step.
    pub(super) opened_dialog: bool,
    /// Whether the run has neither looked nor browsed yet: a dialog in
    /// front at a run's first look, before any browsing, was left there by
    /// the task's run before it (a rescue continues where that run
    /// stopped), and counts as opened by the task.
    pub(super) fresh: bool,
    /// How many controls something covered on the last look with nothing
    /// in front: what a sticky header always covers, which a layer a press
    /// opens must add to before it counts.
    pub(super) covered_base: usize,
    /// The address and window title of the last look: a press after which
    /// either changed went to another page, and what covers that page is
    /// the page's own, not an answer the press asked for.
    pub(super) looked_at: (Option<String>, Option<String>),
    /// Whether the run has pressed or typed inside the task's dialog since
    /// it opened: such a dialog has served the step that worked in it, and
    /// a later step finds it in the way rather than asking (a calendar left
    /// open after its day was chosen).
    pub(super) answered: bool,
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
        Self {
            surface: "window".to_owned(),
            acted: Acted::Nothing,
            opened_dialog: false,
            fresh: true,
            covered_base: 0,
            looked_at: (None, None),
            answered: false,
        }
    }
}

impl Front {
    /// Notes one action, `action` as the run logs it (`click`, `fill …`,
    /// `browse …`, `click (dismiss)`), on an element when `targeted`.
    ///
    /// Only the task's own presses and typing count: a scroll moves no
    /// question into view, and the run's housekeeping (a distraction
    /// cleared, an undo) answers none. Opening an address or an application
    /// leaves whatever was in front behind.
    pub(super) fn act(&mut self, action: &str, targeted: bool) {
        if action.starts_with("browse ") {
            *self = Self {
                fresh: false,
                ..Self::default()
            };
            return;
        }
        if action.starts_with("launch ") {
            *self = Self {
                fresh: self.fresh,
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
        if pressing && self.opened_dialog {
            self.answered = true;
        }
    }

    /// Begins a step: a dialog the task opened and then worked in has
    /// served the step before, and is no longer the task's own; one the
    /// last step's last press opened still asks its question (a format
    /// dialog a booking button raised).
    pub(super) fn next_step(&mut self) {
        if self.answered {
            self.opened_dialog = false;
            self.answered = false;
        }
    }

    /// Takes in a look at `screen`, at the address `location`: what is in
    /// front, and whether the task opened it. The note for the history
    /// when the dialog in front became the task's. Only a `browsing` run
    /// takes a dialog at its first look as the task's: a browser task's
    /// first run always browses first, so a dialog then is a rescue's
    /// inheritance, while an application can open with its own alert.
    pub(super) fn look(
        &mut self,
        screen: &Screen,
        location: Option<&str>,
        browsing: bool,
    ) -> Option<&'static str> {
        let left_open = self.fresh && browsing;
        self.fresh = false;
        let front = self.front_of(screen, location);
        let note = if front == "window" {
            self.opened_dialog = false;
            self.answered = false;
            None
        } else if left_open && !self.opened_dialog {
            self.opened_dialog = true;
            self.answered = false;
            Some(LEFT_OPEN)
        } else if self.acted != Acted::Nothing && self.surface == "window" {
            self.opened_dialog = true;
            self.answered = false;
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
