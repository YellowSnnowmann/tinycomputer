//! What "best" means when picking from records, and ranking by it.

use super::Record;
use super::{parse_clock, parse_duration, parse_price, parse_stops};

/// What "best" means when picking from records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    /// The lowest price first.
    LowestPrice,
    /// The highest price first.
    HighestPrice,
    /// The earliest time first.
    Earliest,
    /// The latest time first.
    Latest,
    /// The fewest stops first.
    FewestStops,
    /// The shortest duration first.
    Shortest,
    /// The list's own order: the first item shown first.
    First,
    /// The list's own order reversed: the last item shown first.
    Last,
}

impl Criterion {
    /// Reads a criterion from plain words, such as `cheapest` or
    /// `lowest price`; `None` when the words need judgement instead.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| lower.contains(word));
        // "first" or "last" alone is the list's own order; with more words
        // ("first product rated 4 stars or more") it is a judgement.
        let bare = lower
            .trim()
            .trim_start_matches("the ")
            .trim_end_matches(" one")
            .trim_end_matches(" result")
            .trim_end_matches(" item")
            .trim_end_matches(" product")
            .trim_end_matches(" listed")
            .trim()
            .to_owned();
        if matches!(bare.as_str(), "first" | "top" | "1st") {
            return Some(Self::First);
        }
        if bare == "last" {
            return Some(Self::Last);
        }
        if has(&[
            "cheapest",
            "lowest price",
            "least expensive",
            "lowest fare",
            "cheaper",
        ]) {
            Some(Self::LowestPrice)
        } else if has(&["most expensive", "highest price"]) {
            Some(Self::HighestPrice)
        } else if has(&["fewest stops", "nonstop", "non-stop", "direct"]) {
            Some(Self::FewestStops)
        } else if has(&["shortest", "fastest", "quickest"]) {
            Some(Self::Shortest)
        } else if has(&["earliest", "first departure", "soonest"]) {
            Some(Self::Earliest)
        } else if has(&["latest", "last departure"]) {
            Some(Self::Latest)
        } else {
            None
        }
    }

    fn key(self, record: &Record) -> Option<f64> {
        // A named field is read first; failing that, any field that parses.
        // Prices found by scanning must show a currency, so a flight number
        // such as `6E-2135` is never mistaken for one.
        let named_or_any = |hints: &[&str], parse: &dyn Fn(&str) -> Option<f64>| {
            record
                .field(hints)
                .and_then(parse)
                .or_else(|| record.fields.values().find_map(|text| parse(text)))
        };
        match self {
            Self::LowestPrice | Self::HighestPrice => record
                .field(&["price", "fare", "cost", "total"])
                .and_then(parse_price)
                .or_else(|| {
                    record
                        .fields
                        .values()
                        .filter_map(|text| parse_price(text))
                        .find(|price| price.currency.is_some())
                })
                .map(|price| price.amount),
            Self::Earliest | Self::Latest => named_or_any(&["depart", "time", "start"], &|text| {
                parse_clock(text).map(f64::from)
            }),
            Self::FewestStops => named_or_any(&["stop"], &|text| parse_stops(text).map(f64::from)),
            Self::Shortest => named_or_any(&["duration", "length"], &|text| {
                parse_duration(text).map(f64::from)
            }),
            // Order alone ranks these (`rank`); no field is read.
            Self::First | Self::Last => None,
        }
    }
}

/// Record indexes, best first, for `criterion`.
///
/// Records whose value cannot be read go last, in their original order.
/// `None` when no record can be read at all — the cue to ask a decision
/// model instead.
///
/// ```
/// use tinycomputer_core::{Criterion, Record, rank};
///
/// let flights = [
///     Record::from_pairs([("airline", "Vistara"), ("price", "₹7,210")]),
///     Record::from_pairs([("airline", "IndiGo"), ("price", "₹6,840")]),
/// ];
/// assert_eq!(rank(&flights, Criterion::LowestPrice), Some(vec![1, 0]));
/// ```
#[must_use]
pub fn rank(records: &[Record], criterion: Criterion) -> Option<Vec<usize>> {
    match criterion {
        Criterion::First => return (!records.is_empty()).then(|| (0..records.len()).collect()),
        Criterion::Last => {
            return (!records.is_empty()).then(|| (0..records.len()).rev().collect());
        }
        _ => {}
    }
    let keyed = records
        .iter()
        .map(|record| criterion.key(record))
        .collect::<Vec<_>>();
    if keyed.iter().all(Option::is_none) {
        return None;
    }
    let descending = matches!(criterion, Criterion::HighestPrice | Criterion::Latest);
    let mut order = (0..records.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| match (keyed[left], keyed[right]) {
        (Some(a), Some(b)) => {
            let ordering = a.total_cmp(&b);
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Some(order)
}

/// The number a criterion such as "closest to 9" or "nearest to size 42"
/// asks items to come nearest to; `None` for any other criterion.
///
/// ```
/// use tinycomputer_core::closest_to;
///
/// assert_eq!(closest_to("closest to 9"), Some(9.0));
/// assert_eq!(closest_to("the size nearest to UK 8.5"), Some(8.5));
/// assert_eq!(closest_to("lowest price"), None);
/// ```
#[must_use]
pub fn closest_to(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    let after = ["closest to", "nearest to"]
        .iter()
        .find_map(|lead| lower.find(lead).map(|at| &lower[at + lead.len()..]))?;
    first_number(after)
}

/// Record indexes, nearest to `target` first, by the first number each
/// record shows: a size list of 6, 7, and 8 with 9 sold out ranks 8, 7, 6.
///
/// Ties keep the records' own order, and records that show no number go
/// last. `None` when no record shows one.
///
/// ```
/// use tinycomputer_core::{Record, rank_closest};
///
/// let sizes = [
///     Record::from_pairs([("size", "6")]),
///     Record::from_pairs([("size", "7"), ("stock", "3 left")]),
///     Record::from_pairs([("size", "8"), ("stock", "2 left")]),
/// ];
/// assert_eq!(rank_closest(&sizes, 9.0), Some(vec![2, 1, 0]));
/// ```
#[must_use]
pub fn rank_closest(records: &[Record], target: f64) -> Option<Vec<usize>> {
    let keyed = records
        .iter()
        .map(|record| {
            record
                .fields
                .values()
                .find_map(|text| first_number(text))
                .map(|number| (number - target).abs())
        })
        .collect::<Vec<_>>();
    if keyed.iter().all(Option::is_none) {
        return None;
    }
    let mut order = (0..records.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| match (keyed[left], keyed[right]) {
        (Some(a), Some(b)) => a.total_cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Some(order)
}

/// The first number `text` shows ("7" of "7 3 left", "8.5" of "UK 8.5").
fn first_number(text: &str) -> Option<f64> {
    text.split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .map(|word| word.trim_matches('.'))
        .find(|word| !word.is_empty())
        .and_then(|word| word.parse().ok())
}
