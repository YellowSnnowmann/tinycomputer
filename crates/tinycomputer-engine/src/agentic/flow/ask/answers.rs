//! The answer readers: a Choice's pick, a Noul's probability, a Score's
//! level, and a yes/no calibrated against its negation.

use std::collections::BTreeMap;

use tinyinference_decisions::Answer;

/// A yes/no probability calibrated against its negation: the mean of
/// `P(yes)` and `1 - P(no)`, or whichever of the two was answered.
pub(in crate::agentic::flow) fn calibrated(
    answers: &BTreeMap<String, Answer>,
    yes: &str,
    no: &str,
) -> Option<f64> {
    match (probability(answers, yes), probability(answers, no)) {
        (Some(yes), Some(no)) => Some(f64::midpoint(yes, 1.0 - no)),
        (Some(yes), None) => Some(yes),
        (None, Some(no)) => Some(1.0 - no),
        (None, None) => None,
    }
}

/// The probability a Score answer puts on its highest level: "fully
/// accomplished", "all of it holds".
pub(in crate::agentic::flow) fn top_level(
    answers: &BTreeMap<String, Answer>,
    id: &str,
) -> Option<f64> {
    let Some(Answer::Score(answer)) = answers.get(id) else {
        return None;
    };
    let top = answer.probabilities.len().checked_sub(1)?;
    answer.probabilities.get(&top.to_string()).copied()
}

/// Combines a calibrated yes/no with a scale's top-level probability.
pub(in crate::agentic::flow) fn combined(yes_no: Option<f64>, top: Option<f64>) -> Option<f64> {
    match (yes_no, top) {
        (Some(yes_no), Some(top)) => Some(f64::midpoint(yes_no, top)),
        (one, other) => one.or(other),
    }
}

/// How far from an even chance a calibrated yes/no may sit and still say
/// nothing either way.
pub(in crate::agentic::flow) const HEDGED: f64 = 0.10;

/// The top-level probability at which a scale's answer stands on its own.
pub(in crate::agentic::flow) const CRISP_TOP: f64 = 0.85;

/// [`combined`], except that a hedged yes/no (within [`HEDGED`] of an even
/// chance) defers to a crisp scale (its top level at least [`CRISP_TOP`]).
///
/// A condition that lists several things draws a hedged yes/no but a crisp
/// coverage answer, and their midpoint keeps a condition that holds in full
/// under a 0.75 bar unless the coverage is certain: `BlazeDemo`'s "the purchase
/// form shows the passenger name, street address, city, state, and zip code",
/// every field filled, was judged 0.71 from a yes/no of 0.52 and a coverage of
/// 0.88. A yes/no that leans either way still counts as before.
pub(in crate::agentic::flow) fn deferred(yes_no: Option<f64>, top: Option<f64>) -> Option<f64> {
    match (yes_no, top) {
        (Some(yes_no), Some(top)) if (yes_no - 0.5).abs() <= HEDGED && top >= CRISP_TOP => {
            Some(top)
        }
        _ => combined(yes_no, top),
    }
}

/// The chosen key and its probability, or `None` for `none` or a missing answer.
pub(in crate::agentic::flow) fn chosen(
    answers: &BTreeMap<String, Answer>,
    id: &str,
) -> Option<(String, f64)> {
    match answers.get(id) {
        Some(Answer::Choice(answer)) if answer.choice != "none" => Some((
            answer.choice.clone(),
            answer
                .probabilities
                .get(&answer.choice)
                .copied()
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

/// A Noul's probability, or `None` when it was not asked or not answered.
pub(in crate::agentic::flow) fn probability(
    answers: &BTreeMap<String, Answer>,
    id: &str,
) -> Option<f64> {
    match answers.get(id) {
        Some(Answer::Noul(answer)) => Some(answer.noul),
        _ => None,
    }
}

/// A Score's position as a fraction of the scale, from its level probabilities.
pub(in crate::agentic::flow) fn level(answers: &BTreeMap<String, Answer>, id: &str) -> Option<f64> {
    let Some(Answer::Score(answer)) = answers.get(id) else {
        return None;
    };
    let top = answer.probabilities.len().saturating_sub(1);
    if top == 0 {
        return None;
    }
    let expected = answer
        .probabilities
        .iter()
        .filter_map(|(level, probability)| {
            level
                .parse::<u32>()
                .ok()
                .map(|level| f64::from(level) * probability)
        })
        .sum::<f64>();
    Some(expected / f64::from(u32::try_from(top).unwrap_or(u32::MAX)))
}
