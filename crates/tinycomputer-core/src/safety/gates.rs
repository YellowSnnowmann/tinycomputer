//! What only a person can get past: captchas, one-time codes, login walls.

use super::{has_phrase, normalize};

/// What only a person can get past, by the words a page shows for it.
const HUMAN_GATES: &[(&str, &str)] = &[
    ("captcha", "solve the captcha"),
    ("recaptcha", "solve the captcha"),
    ("verify you are human", "prove you are human"),
    ("verify you re human", "prove you are human"),
    ("i m not a robot", "prove you are human"),
    ("are you a robot", "prove you are human"),
    ("one time password", "enter the one-time password"),
    ("enter the otp", "enter the one-time password"),
    ("verification code", "enter the verification code"),
    ("enter the code we sent", "enter the verification code"),
    ("two factor", "complete two-factor authentication"),
    ("2 step verification", "complete two-factor authentication"),
    ("sign in to continue", "sign in"),
    ("log in to continue", "sign in"),
    ("login to continue", "sign in"),
    ("please sign in", "sign in"),
    ("please log in", "sign in"),
    ("please login", "sign in"),
    // A wall that names what it hides: "Log in to see ride options".
    ("log in to see", "sign in"),
    ("login to see", "sign in"),
    ("sign in to see", "sign in"),
    ("log in to view", "sign in"),
    ("login to view", "sign in"),
    ("sign in to view", "sign in"),
    // The call to action a wall's dialog makes; a header's "Log in | Sign
    // up" links lack the "or" and are no wall.
    ("log in or sign up", "sign in"),
    ("login or sign up", "sign in"),
    ("sign in or sign up", "sign in"),
    ("you must be logged in", "sign in"),
    ("you need to be logged in", "sign in"),
    ("login required", "sign in"),
    ("sign in required", "sign in"),
];

/// What a person must do before a task can go on, when the visible text
/// shows a captcha, a one-time code, two-factor authentication, or a login
/// wall; `None` otherwise.
///
/// ```
/// use tinycomputer_core::human_needed;
///
/// let page = ["Security check".to_owned(), "Verify you are human".to_owned()];
/// assert_eq!(human_needed(&page).as_deref(), Some("prove you are human"));
/// assert_eq!(human_needed(&["Flights from Delhi".to_owned()]), None);
/// ```
#[must_use]
pub fn human_needed(texts: &[String]) -> Option<String> {
    let words = normalize(&texts.join(" "));
    HUMAN_GATES
        .iter()
        .find(|(phrase, _)| has_phrase(&words, phrase))
        .map(|(_, action)| (*action).to_owned())
}
