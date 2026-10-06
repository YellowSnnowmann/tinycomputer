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

/// The ride form's state.
#[derive(Debug, Default)]
pub(super) struct Places {
    /// The box whose list of suggestions is open.
    pub(super) open: Option<String>,
    /// Boxes whose text came from a picked suggestion.
    pub(super) picked: BTreeSet<String>,
}

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
    if let Some(open) = &places.open {
        let typed = sim.fields.get(open).cloned().unwrap_or_default();
        let list = [root, "group \"Get a ride\"", "listbox \"Suggestions\""];
        for place in suggested(&typed) {
            candidates.push(node(place, "option", &["Click"], &list, 300.0));
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
    }
    sim.focused = Some(name.to_owned());
    sim.fields.insert(name.to_owned(), text);
}

/// Closes the open list, dropping its box's text unless a suggestion was
/// picked for it.
pub(super) fn drop_unpicked(sim: &mut Sim) {
    let Some(places) = sim.places.as_mut() else {
        return;
    };
    if let Some(open) = places.open.take()
        && !places.picked.contains(&open)
    {
        sim.fields.insert(open, String::new());
    }
}

/// Presses `name` on the ride form; whether it was a suggestion row, which
/// fills the open box with that place and keeps it.
pub(super) fn press_place(sim: &mut Sim, name: &str) -> bool {
    let Some(places) = sim.places.as_mut() else {
        return false;
    };
    let Some(open) = places.open.clone() else {
        return false;
    };
    if !PLACES.contains(&name) {
        return false;
    }
    places.picked.insert(open.clone());
    places.open = None;
    sim.fields.insert(open, name.to_owned());
    true
}
