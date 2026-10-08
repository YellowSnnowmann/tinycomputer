//! Time for a split: an event's `at`, and spans of milliseconds merged,
//! measured, and cut.

use serde_json::Value;

/// Intervals of milliseconds since the Unix epoch, start and end.
pub(super) type Spans = Vec<(i64, i64)>;

/// `event`'s `at` as milliseconds since the Unix epoch.
#[must_use]
pub fn at_ms(event: &Value) -> Option<i64> {
    // `2026-10-07T06:19:52.812Z`, as the engine writes it.
    let at = event["at"].as_str()?;
    let field = |range: std::ops::Range<usize>| at.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
    let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
    let millis = if at.get(19..20) == Some(".") {
        field(20..23)?
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    Some((((days * 24 + hour) * 60 + minute) * 60 + second) * 1000 + millis)
}

/// Days from 1970-01-01 to the proleptic Gregorian date, after Howard
/// Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(super) fn union(mut spans: Spans) -> Spans {
    spans.sort_unstable();
    let mut merged: Spans = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

pub(super) fn length(spans: &[(i64, i64)]) -> u64 {
    spans.iter().map(|(start, end)| millis(end - start)).sum()
}

/// `spans` with every moment of `cut` (a union) taken out.
pub(super) fn minus(spans: &[(i64, i64)], cut: &[(i64, i64)]) -> Spans {
    let mut left = Vec::new();
    for &(start, end) in spans {
        let mut from = start;
        for &(cut_start, cut_end) in cut {
            if cut_end <= from || cut_start >= end {
                continue;
            }
            if cut_start > from {
                left.push((from, cut_start));
            }
            from = from.max(cut_end);
        }
        if from < end {
            left.push((from, end));
        }
    }
    left
}

pub(super) fn millis(ms: i64) -> u64 {
    u64::try_from(ms).unwrap_or_default()
}

pub(super) fn secs(ms: u64) -> f64 {
    std::time::Duration::from_millis(ms).as_secs_f64()
}
