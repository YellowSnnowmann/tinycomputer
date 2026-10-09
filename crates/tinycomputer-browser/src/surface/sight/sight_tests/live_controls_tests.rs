//! Live tests of what sight reads as a control, gated on
//! `TINYCOMPUTER_LIVE_BROWSER=1`: rows a script framework wires to a click,
//! options inside their trigger, classes behind a variant, icons named in
//! camel case, a control brought into the window before its press, a
//! control fixed to the window inside a scrolling list, and a control each
//! card repeats, told apart by its card.

#[cfg(feature = "agent-browser")]
use serde_json::json;

#[cfg(feature = "agent-browser")]
use super::live_tests::{live_page, live_reading, shown_names};

/// Place suggestions drawn as plain rows that only a Preact click listener
/// makes pressable (`l` once minified, `_listeners` with its capture flag),
/// and one whose listener waits for the mouse going down.
#[cfg(feature = "agent-browser")]
const PREACT_ROWS_PAGE: &str = r#"<div style="width: 300px">
  <input aria-label="Where to?" value="Goa">
  <div class="row"><span>Goa, India</span></div>
  <div class="row"><span>Baga Beach North Goa, Goa</span></div>
  <div class="row"><span>Panjim, Panaji, Goa</span></div>
</div>
<script>
  const [first, second, third] = document.querySelectorAll('.row');
  first.l = { click: () => {} };
  second._listeners = { clickfalse: () => {} };
  third.l = { mousedown: () => {} };
</script>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_rows_a_preact_click_listener_wires_are_buttons() {
    let Some(reading) = live_reading(PREACT_ROWS_PAGE).await else {
        return;
    };
    let role = |name: &str| {
        reading["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["name"] == name)
            .and_then(|node| node["role"].as_str())
            .map(str::to_owned)
    };
    assert_eq!(role("Goa, India").as_deref(), Some("button"));
    assert_eq!(role("Baga Beach North Goa, Goa").as_deref(), Some("button"));
    assert_eq!(
        role("Panjim, Panaji, Goa"),
        None,
        "a mouse-down listener is no press"
    );
}

/// A sort menu whose list sits inside its pointer-cursor trigger, each
/// option wired to a click by its own Preact listener.
#[cfg(feature = "agent-browser")]
const MENU_IN_TRIGGER_PAGE: &str = r#"<div style="display: flex">
  <span class="dropdown__select" tabindex="-1" style="cursor: pointer; display: flex; width: 180px; height: 40px; position: relative">
    <span>Popularity</span>
    <ul style="position: absolute; top: 40px; left: 0; margin: 0; padding: 0; list-style: none; background: white; width: 180px">
      <li class="option"><span>Popularity</span></li>
      <li class="option"><span>Guest Ratings</span></li>
      <li class="option"><span>Price Low to High</span></li>
    </ul>
  </span>
</div>
<script>
  for (const option of document.querySelectorAll('.option')) option.l = { click: () => {} };
</script>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_options_wired_by_their_own_handler_inside_a_trigger_are_buttons() {
    let Some(reading) = live_reading(MENU_IN_TRIGGER_PAGE).await else {
        return;
    };
    for option in ["Guest Ratings", "Price Low to High"] {
        let found = reading["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["name"] == option)
            .unwrap_or_else(|| panic!("{option} not offered: {:?}", shown_names(&reading)));
        assert_eq!(found["role"], "button", "{option}");
    }
}

/// A place box styled by a variant class that ends in "disabled", beside
/// a day a calendar greys out by class, and a button whose variant class
/// styles it only once disabled.
#[cfg(feature = "agent-browser")]
const VARIANT_CLASSES_PAGE: &str = r#"<input class="outline-none placeholder:text-disabled" aria-label="From">
<button class="disabled:opacity-50">Search</button>
<span class="day day-disabled" style="cursor: pointer">21</span>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_variant_class_says_nothing_of_a_controls_own_state() {
    let Some(reading) = live_reading(VARIANT_CLASSES_PAGE).await else {
        return;
    };
    let role = |name: &str| {
        reading["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["name"] == name)
            .and_then(|node| node["role"].as_str())
            .map(str::to_owned)
    };
    assert_eq!(role("From").as_deref(), Some("textbox"));
    assert_eq!(role("Search").as_deref(), Some("button"));
    assert_eq!(role("21"), None, "a day greyed out by class is no control");
}

/// A button at the window's foot whose middle lies below the window, and
/// one in full view; each records its press.
#[cfg(feature = "agent-browser")]
const FOOT_BUTTON_PAGE: &str = r#"<button id="shown" style="position: absolute; top: 20px">Shown</button>
<button id="foot" style="position: absolute; left: 20px; width: 200px; height: 80px">Add to cart</button>
<div style="height: 3000px"></div>
<script>
  const foot = document.getElementById('foot');
  foot.style.top = (window.innerHeight - 20) + 'px';
  window.pressed = [];
  for (const button of document.querySelectorAll('button')) {
    button.addEventListener('click', () => window.pressed.push(button.id));
  }
</script>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_control_whose_middle_is_below_the_window_is_brought_into_it_and_pressed() {
    let Some((browser, info)) = live_page(FOOT_BUTTON_PAGE).await else {
        return;
    };
    let evaluate =
        |script: String| browser.command(&info.id, json!({"action": "evaluate", "script": script}));
    let into_view = |selector: &str| {
        format!(
            "{}(document.querySelector({}))",
            crate::surface::INTO_VIEW_JS,
            serde_json::Value::String(selector.to_owned())
        )
    };
    let shown = evaluate(into_view("#shown")).await.unwrap();
    assert_eq!(
        shown["result"], false,
        "a control in view stays where it is"
    );
    let moved = evaluate(into_view("#foot")).await.unwrap();
    assert_eq!(moved["result"], true);
    let middle = evaluate(
        "(() => { const box = document.getElementById('foot').getBoundingClientRect(); \
         const y = box.top + box.height / 2; return y >= 0 && y < window.innerHeight; })()"
            .to_owned(),
    )
    .await
    .unwrap();
    assert_eq!(middle["result"], true, "its middle now shows");
    browser
        .command(&info.id, json!({"action": "click", "selector": "#foot"}))
        .await
        .unwrap();
    let pressed = evaluate("window.pressed".to_owned()).await.unwrap();
    browser.close_session(&info.id).await.unwrap();
    assert_eq!(pressed["result"], json!(["foot"]));
}

/// A sign-up pop-up whose only way out is a sprite its class names in camel
/// case ("icClose"): no words, no picture, a pointer cursor.
#[cfg(feature = "agent-browser")]
const SPRITE_CLOSE_PAGE: &str = r#"<div style="position: fixed; top: 40px; left: 40px; width: 360px; height: 200px; background: white">
  <span class="logSprite icClose" style="position: absolute; top: 8px; right: 8px; display: inline-block; width: 16px; height: 16px; cursor: pointer; background: #888"></span>
  <p>Login/Signup</p>
  <input aria-label="Enter your Mobile Number">
</div>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_sprite_its_class_names_in_camel_case_is_read_as_that_icon() {
    let Some(reading) = live_reading(SPRITE_CLOSE_PAGE).await else {
        return;
    };
    let close = reading["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["name"] == "close")
        .unwrap_or_else(|| panic!("no close control: {:?}", shown_names(&reading)));
    assert_eq!(close["role"], "button");
    assert_eq!(close["description"], "an icon");
}

/// A scrolling list of airports whose last row holds a "Done" button fixed
/// to the window below the list, outside the list's view.
#[cfg(feature = "agent-browser")]
const FIXED_IN_LIST_PAGE: &str = r#"<ul id="list" role="listbox" aria-label="Airports" style="position: absolute; top: 10px; left: 10px; width: 300px; height: 100px; overflow: auto; margin: 0; padding: 0">
  <li role="option">DEL Delhi</li><li role="option">BLR Bengaluru</li><li role="option">MAA Chennai</li>
  <li role="option">HYD Hyderabad</li><li role="option">CCU Kolkata</li><li role="option">BOM Mumbai</li>
  <li role="option">GOI Goa</li><li role="option">PNQ Pune</li>
  <li><button style="position: fixed; top: 300px; left: 10px">Done</button></li>
</ul>
<style> #list li { height: 24px; cursor: pointer; list-style: none } </style>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_control_fixed_to_the_window_inside_a_scrolling_list_is_not_scrolled_away() {
    let Some(reading) = live_reading(FIXED_IN_LIST_PAGE).await else {
        return;
    };
    let states = |name: &str| {
        reading["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["name"] == name)
            .map_or_else(
                || panic!("{name} not offered: {:?}", shown_names(&reading)),
                |node| node["states"].to_string(),
            )
    };
    assert!(!states("Done").contains("offscreen"), "{}", states("Done"));
    assert!(
        states("PNQ Pune").contains("offscreen"),
        "a row below the list's fold still is: {}",
        states("PNQ Pune")
    );
}

/// Product cards a press opens, each with its own "ADD" button, and one card
/// with a button no other card has.
#[cfg(feature = "agent-browser")]
const CARD_BUTTONS_PAGE: &str = r#"<div style="display: flex; gap: 10px">
  <div onclick="" style="cursor: pointer; width: 220px; padding: 8px">Too Yumm Korean Ramen 79 g ₹49 <button>ADD</button></div>
  <div onclick="" style="cursor: pointer; width: 220px; padding: 8px">Maggi Double Masala 95 g ₹20 <button>ADD</button></div>
  <div onclick="" style="cursor: pointer; width: 220px; padding: 8px">Maggi Masala 280 g ₹56 <button>Notify me</button></div>
</div>"#;

#[cfg(feature = "agent-browser")]
#[tokio::test]
async fn live_a_control_each_card_repeats_is_described_by_its_card() {
    let Some(reading) = live_reading(CARD_BUTTONS_PAGE).await else {
        return;
    };
    let nodes = reading["nodes"].as_array().unwrap();
    let described = |name: &str| {
        nodes
            .iter()
            .filter(|node| node["name"] == name)
            .map(|node| node["description"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    let adds = described("ADD");
    assert_eq!(adds.len(), 2, "{:?}", shown_names(&reading));
    assert!(adds[0].starts_with("in Too Yumm Korean Ramen"), "{adds:?}");
    assert!(adds[1].starts_with("in Maggi Double Masala"), "{adds:?}");
    assert_eq!(
        described("Notify me"),
        [""],
        "a control no other card repeats needs no card to tell it apart"
    );
}
