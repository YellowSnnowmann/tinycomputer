//! A ride form whose pickup and dropoff boxes are autocompletes, as ride,
//! travel, and delivery sites draw them: typing lists matching places under
//! the box, a box keeps its text only once one of those rows is picked, and
//! its unpicked text is dropped when the focus moves on or Escape closes the
//! list.

use super::*;

/// The ride form's two boxes.
pub(super) const PLACE_BOXES: [&str; 2] = ["Pickup location", "Dropoff location"];

/// Every place the boxes suggest from.
pub(super) const PLACES: [&str; 4] = [
    "Connaught Place New Delhi, Delhi, India",
    "Connaught Place Dehradun, Uttarakhand, India",
    "Indira Gandhi International Airport New Delhi, Delhi, India",
    "Indore Airport Indore, Madhya Pradesh, India",
];

/// The heading a panel of suggestions opens with (`Places::panel`).
const PANEL_HEADING: &str = "Select a pickup point Choose where your driver meets you";

/// The rows a box lists of its own while its matches are fetched
/// (`Places::starters`): none of them names a place typed.
const STARTER_ROWS: [&str; 2] = [
    "Allow location access It provides your pickup address",
    "Search in a different city",
];

/// The ride form's state.
#[derive(Debug, Default)]
pub(super) struct Places {
    /// The box whose list of suggestions is open.
    pub(super) open: Option<String>,
    /// Boxes whose text came from a picked suggestion.
    pub(super) picked: BTreeSet<String>,
    /// Whether the list is drawn inside one panel the page reads as a
    /// button, its rows unread and its name stringing them all together, as
    /// a store's delivery-area popover was read live.
    pub(super) panel: bool,
    /// Waits for a change a box's list takes to show once typed into, as a
    /// page that fetches its rows draws them late.
    pub(super) late: u8,
    /// Waits still to come before the open list shows its rows.
    pub(super) pending: u8,
    /// Whether the open list shows rows of its own while its matches are
    /// still to come, as a ride app's did live ("Allow location access",
    /// "Search in a different city").
    pub(super) starters: bool,
    /// Whether each box shows only once its own button is pressed ("From
    /// …", "To …"), one at a time, as a flight form draws them.
    pub(super) behind_buttons: bool,
    /// The box whose button was pressed last (`behind_buttons`).
    pub(super) door: Option<String>,
}

/// The buttons that show the boxes of a form drawn `behind_buttons`.
const DOORS: [&str; 2] = ["From", "To"];

/// The places suggested for `typed`: each one that holds every typed word.
fn suggested(typed: &str) -> Vec<&'static str> {
    let words = typed
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return Vec::new();
    }
    PLACES
        .into_iter()
        .filter(|place| {
            let place = place.to_lowercase();
            words.iter().all(|word| place.contains(word.as_str()))
        })
        .collect()
}

/// The ride form's controls, as they stand.
pub(super) fn places_widget(
    sim: &Sim,
    places: &Places,
    root: &str,
    candidates: &mut Vec<Candidate>,
) {
    let form = [root, "group \"Get a ride\""];
    for (index, name) in PLACE_BOXES.iter().enumerate() {
        if places.behind_buttons {
            // The button shows a place once one is picked, not what is typed.
            let shown = sim
                .fields
                .get(*name)
                .filter(|_| places.picked.contains(*name))
                .cloned()
                .unwrap_or_else(|| "Select a place".to_owned());
            candidates.push(node(
                &format!("{} {shown}", DOORS[index]),
                "button",
                &["Click"],
                &form,
                190.0 + 40.0 * f64::from(u8::try_from(index).unwrap()),
            ));
            if places.door.as_deref() != Some(*name) {
                continue;
            }
        }
        let mut field = node(
            name,
            "textbox",
            &["Click", "SetValue"],
            &form,
            200.0 + 40.0 * f64::from(u8::try_from(index).unwrap()),
        );
        field.value = sim.fields.get(*name).map(|value| json!(value));
        candidates.push(field);
    }
    let list = [root, "group \"Get a ride\"", "listbox \"Suggestions\""];
    if places.starters && places.open.is_some() && places.pending > 0 {
        for row in STARTER_ROWS {
            candidates.push(node(row, "option", &["Click"], &list, 300.0));
        }
    }
    if let Some(open) = places.open.as_ref().filter(|_| places.pending == 0) {
        let typed = sim.fields.get(open).cloned().unwrap_or_default();
        if places.panel {
            let rows = suggested(&typed);
            if !rows.is_empty() {
                let name = format!("{PANEL_HEADING} {}", rows.join(" "));
                candidates.push(node(&name, "button", &["Click"], &form, 300.0));
            }
        } else {
            for place in suggested(&typed) {
                candidates.push(node(place, "option", &["Click"], &list, 300.0));
            }
        }
    }
    candidates.push(node("See prices", "link", &["Click"], &form, 400.0));
}

/// Text set or pasted into the field `name`, which takes the focus; the ride
/// form's boxes behave as autocompletes (`type_place`).
pub(super) fn type_into(sim: &mut Sim, name: &str, text: String) {
    if sim.places.is_some() && PLACE_BOXES.contains(&name) {
        type_place(sim, name, text);
    } else {
        sim.focused = Some(name.to_owned());
        sim.fields.insert(name.to_owned(), text);
    }
}

/// Text put into one of the ride form's boxes: another box's unpicked text
/// is dropped as the focus leaves it, and this box opens its list.
fn type_place(sim: &mut Sim, name: &str, text: String) {
    drop_unpicked(sim);
    if let Some(places) = sim.places.as_mut() {
        places.open = Some(name.to_owned());
        places.picked.remove(name);
        places.pending = places.late;
    }
    sim.focused = Some(name.to_owned());
    sim.fields.insert(name.to_owned(), text);
}

/// A wait for the page to change: the open list draws its rows one wait
/// nearer; nothing else on the ride form changes by itself.
pub(super) fn await_place_rows(sim: &mut Sim) -> bool {
    match sim.places.as_mut() {
        Some(places) if places.open.is_some() && places.pending > 0 => {
            places.pending -= 1;
            true
        }
        _ => false,
    }
}

/// Closes the open list, dropping its box's text unless a suggestion was
/// picked for it; a box shown behind its button closes with it.
pub(super) fn drop_unpicked(sim: &mut Sim) {
    let Some(places) = sim.places.as_mut() else {
        return;
    };
    places.door = None;
    if let Some(open) = places.open.take()
        && !places.picked.contains(&open)
    {
        sim.fields.insert(open, String::new());
    }
}

/// Presses `name` on the ride form; whether it was a suggestion row on show
/// for the open box's text, which fills the box with that place and keeps
/// it. A press on a panel of rows lands on the row drawn at its middle,
/// whichever that is; a press anywhere else but the box moves the focus on,
/// so the list closes and drops the box's unpicked text, as a page does.
pub(super) fn press_place(sim: &mut Sim, name: &str) -> bool {
    let door = sim
        .places
        .as_ref()
        .filter(|places| places.behind_buttons)
        .and_then(|_| {
            DOORS
                .iter()
                .position(|door| name.starts_with(&format!("{door} ")))
        });
    if let Some(index) = door {
        // Another box's button moves the focus on, as a press anywhere else
        // does: the open list closes and drops its box's unpicked text.
        let open = sim.places.as_ref().and_then(|places| places.open.clone());
        if open.as_deref() != Some(PLACE_BOXES[index]) {
            drop_unpicked(sim);
        }
        if let Some(places) = sim.places.as_mut() {
            places.door = Some(PLACE_BOXES[index].to_owned());
        }
        return true;
    }
    let Some(open) = sim.places.as_ref().and_then(|places| places.open.clone()) else {
        return false;
    };
    let rows = suggested(&sim.fields.get(&open).cloned().unwrap_or_default());
    let place = if name.starts_with(PANEL_HEADING) {
        rows.get(rows.len() / 2).copied()
    } else {
        rows.into_iter().find(|row| *row == name)
    };
    let Some(place) = place else {
        if name != open {
            drop_unpicked(sim);
        }
        return false;
    };
    if let Some(places) = sim.places.as_mut() {
        places.picked.insert(open.clone());
        places.open = None;
        places.door = None;
    }
    sim.fields.insert(open, place.to_owned());
    true
}
