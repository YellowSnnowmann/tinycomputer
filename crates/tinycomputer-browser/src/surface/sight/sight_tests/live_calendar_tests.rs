//! Live tests of calendars sight reads as dates, gated on
//! `TINYCOMPUTER_LIVE_BROWSER=1`: two months that share their arrows, and
//! months drawn as grids of buttons rather than tables.

#[cfg(feature = "agent-browser")]
use super::live_tests::{live_reading, shown_names};

/// Two months drawn as grids of buttons, not tables, under one header that
/// names both: each day shows its fare after its number ("22 6529"). Each
/// grid sits in a box of its month's own when `boxed`, or both side by
/// side in one.
#[cfg(feature = "agent-browser")]
fn grid_calendar_page(boxed: bool) -> String {
    let grid = |blanks: u32, days: u32, fares: u32| {
        let cells = (0..blanks)
            .map(|_| "<span></span>".to_owned())
            .chain((1..=days).map(|day| format!("<button>{day} {}</button>", fares + day * 7)))
            .collect::<String>();
        let grid = format!(
            "<div style=\"display: grid; grid-template-columns: repeat(7, 44px)\">{cells}</div>"
        );
        if boxed {
            format!("<div><div>Su Mo Tu We Th Fr Sa</div>{grid}</div>")
        } else {
            grid
        }
    };
    format!(
        "<div style=\"width: 700px\"><div><span>October 2026</span> <span>November 2026</span></div>\
         <div style=\"display: flex; gap: 20px\">{}{}</div></div>",
        grid(4, 31, 6_000),
        grid(0, 30, 7_000)
    )
}

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_calendar_drawn_as_grids_of_buttons_reads_its_days_as_dates() {
    // Two grids side by side in one block are two months too, not one
    // calendar that hides the second.
    for boxed in [true, false] {
        let Some(reading) = live_reading(&grid_calendar_page(boxed)).await else {
            return;
        };
        let described = |name: &str| {
            reading["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| node["name"] == name)
                .map_or_else(
                    || panic!("{name} not offered: {:?}", shown_names(&reading)),
                    |node| node["description"].as_str().unwrap_or_default().to_owned(),
                )
        };
        assert_eq!(described("22 6154"), "22 October 2026", "boxed: {boxed}");
        assert_eq!(described("5 7035"), "5 November 2026", "boxed: {boxed}");
    }
}

/// A picker showing two months, drawn as a hotel site's was: each month a
/// table under a header that holds a hidden month menu, and one pair of icon
/// arrows beside both months, inside neither.
#[cfg(feature = "agent-browser")]
const TWO_MONTH_PICKER: &str = r#"<style>
  .range td span { cursor: pointer }
  .range select { position: absolute; opacity: 0; width: 1px }
  .range__arrow { cursor: pointer; width: 20px; height: 20px }
</style>
<div class="range" style="display: flex; position: absolute; top: 10px; left: 10px">
  <div class="range__arrow range__arrow--previous"></div>
  <div class="range__month">
    <div class="range__header"><span>October<select class="months"></select></span><span> 2026<select><option>1970</option></select></span></div>
    <table><thead><tr><th>Mon</th><th>Tue</th><th>Wed</th><th>Thu</th><th>Fri</th><th>Sat</th><th>Sun</th></tr></thead>
      <tbody id="october"></tbody></table>
  </div>
  <div class="range__month">
    <div class="range__header"><span>November<select class="months"></select></span><span> 2026<select><option>1970</option></select></span></div>
    <table><thead><tr><th>Mon</th><th>Tue</th><th>Wed</th><th>Thu</th><th>Fri</th><th>Sat</th><th>Sun</th></tr></thead>
      <tbody id="november"></tbody></table>
  </div>
  <div class="range__arrow range__arrow--next"></div>
</div>
<script>
  for (const menu of document.querySelectorAll('select.months')) {
    for (const month of ['January', 'February', 'March', 'April', 'May', 'June', 'July',
      'August', 'September', 'October', 'November', 'December']) menu.add(new Option(month));
  }
  const fill = (body, blanks, days) => {
    const cells = [...Array(blanks).fill('<td></td>'),
      ...Array.from({ length: days }, (_, index) => `<td><span>${index + 1}</span></td>`)];
    for (let at = 0; at < cells.length; at += 7) {
      const row = body.insertRow();
      for (const html of cells.slice(at, at + 7)) row.insertCell().outerHTML = html;
    }
  };
  // 1 October 2026 is a Thursday, 1 November a Sunday.
  fill(document.getElementById('october'), 3, 31);
  fill(document.getElementById('november'), 6, 30);
</script>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_two_month_pickers_days_and_shared_arrows_are_read_as_dates() {
    let Some(reading) = live_reading(TWO_MONTH_PICKER).await else {
        return;
    };
    let nodes = reading["nodes"].as_array().unwrap();
    let days = nodes
        .iter()
        .filter(|node| node["role"] == "gridcell")
        .map(|node| {
            format!(
                "{} ({})",
                node["name"].as_str().unwrap(),
                node["description"].as_str().unwrap()
            )
        })
        .collect::<Vec<_>>();
    // Each month by its header as shown, not by its hidden menu's names.
    assert_eq!(days.len(), 61, "{days:?}");
    for day in [
        "22 (22 October 2026)",
        "31 (31 October 2026)",
        "22 (22 November 2026)",
    ] {
        assert!(days.iter().any(|seen| seen == day), "{day} in {days:?}");
    }
    let names = shown_names(&reading);
    assert_eq!(
        names
            .iter()
            .filter(|name| name.ends_with(" month"))
            .collect::<Vec<_>>(),
        ["previous month", "next month"],
        "the arrows beside both months page them"
    );
}
