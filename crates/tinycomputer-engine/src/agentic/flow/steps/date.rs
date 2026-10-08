//! Reading a date option: whether it names a calendar day, and the words a
//! control must show to be that day.

use super::matching::plain;

/// The months a date picker is paged forward at most.
pub(super) const MAX_MONTHS: usize = 12;

/// Month names, as a date option spells them.
const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Whether `text` names a month, in full or cut to three letters or more
/// ("Oct", "Sept"), as a calendar's day cells and headings do.
pub(in crate::agentic::flow) fn names_a_month(text: &str) -> bool {
    text.split(|character: char| !character.is_alphabetic())
        .filter(|word| word.chars().count() >= 3)
        .map(str::to_lowercase)
        .any(|word| MONTHS.iter().any(|month| month.starts_with(&word)))
}

/// Whether `option` names a calendar day: a month name and a day number that
/// is a real day of that month (a year, when given, decides February's 28th
/// against its 29th). `February 31` or `April 31` names no such day, and is
/// read as ordinary autocomplete text instead of taking the calendar path.
pub(in crate::agentic::flow) fn looks_like_date(option: &str) -> bool {
    let lower = option.to_lowercase();
    let words = lower
        .split(|character: char| !character.is_alphanumeric())
        .collect::<Vec<_>>();
    let Some(month) = MONTHS.iter().position(|month| words.contains(month)) else {
        return false;
    };
    let Some(day) = words
        .iter()
        .find_map(|word| word.parse::<u8>().ok().filter(|day| (1..=31).contains(day)))
    else {
        return false;
    };
    let year = words.iter().find_map(|word| {
        word.parse::<u16>()
            .ok()
            .filter(|year| (1900..=2100).contains(year))
    });
    day <= days_in_month(month, year)
}

/// How many days `month` (0 = January, from [`MONTHS`]) has; February is
/// taken as 29 when no `year` narrows it, so a bare "29 February" is still
/// treated as a date worth paging to.
fn days_in_month(month: usize, year: Option<u16>) -> u8 {
    match month {
        0 | 2 | 4 | 6 | 7 | 9 | 11 => 31,
        3 | 5 | 8 | 10 => 30,
        _ => {
            if year.is_none_or(is_leap_year) {
                29
            } else {
                28
            }
        }
    }
}

/// Whether `year` is a leap year in the Gregorian calendar.
fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Whether a control's label says only that it shows the next month; a
/// date field whose label lists the whole calendar says much more.
pub(super) fn is_next_month(name: &str) -> bool {
    let words = plain(name);
    words.contains("next month") && words.split(' ').count() <= 4
}

/// The day, month, and year (when given) a date option names, as words.
pub(in crate::agentic::flow) fn date_words(option: &str) -> Vec<String> {
    plain(option)
        .split(' ')
        .filter(|word| {
            MONTHS.contains(word)
                || word.parse::<u16>().is_ok_and(|number| {
                    (1..=31).contains(&number) || (1900..=2100).contains(&number)
                })
        })
        .map(str::to_owned)
        .collect()
}

/// Whether a control showing `text` is the day `words` names
/// ([`date_words`]): the day's number with or without a leading zero, the
/// month in full or by its first three letters ("Sept" too), and the year
/// only when the control shows one. Live, a strip of show dates read "WED
/// 07 OCT", and the day was never found in it.
pub(in crate::agentic::flow) fn shows_date(text: &str, words: &[String]) -> bool {
    let shown = plain(text)
        .split(' ')
        .map(|word| {
            if let Ok(number) = word.parse::<u16>() {
                return number.to_string();
            }
            MONTHS
                .iter()
                .find(|month| {
                    word.len() >= 3
                        && (month.starts_with(word) || (*month == &"september" && word == "sept"))
                })
                .map_or_else(|| word.to_owned(), |month| (*month).to_owned())
        })
        .collect::<Vec<_>>();
    let shows_year = shown.iter().any(|word| {
        word.parse::<u16>()
            .is_ok_and(|year| (1900..=2100).contains(&year))
    });
    words.iter().all(|word| {
        let year = word
            .parse::<u16>()
            .is_ok_and(|year| (1900..=2100).contains(&year));
        (year && !shows_year) || shown.iter().any(|shown| shown == word)
    })
}
