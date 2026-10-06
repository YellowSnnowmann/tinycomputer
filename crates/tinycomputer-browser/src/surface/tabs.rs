//! Keeping what a press opens in the tab the agent reads, and taking a
//! page that drew before it finished loading as open.

use serde_json::{Value, json};

use super::BrowserSurface;

/// Where the page is, what it is called, and whether it has drawn words.
const DRAWN_JS: &str = r"(() => ({
  url: location.href,
  title: document.title,
  drawn: document.readyState !== 'loading' && !!document.body
    && document.body.innerText.trim().length > 0,
}))()";

/// `url` as host and path, without its scheme, a leading `www.`, its query,
/// its fragment, or a trailing slash, lower-cased: two addresses of one page.
fn place(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let rest = rest.trim_end_matches('/').to_ascii_lowercase();
    rest.strip_prefix("www.")
        .map_or(rest.clone(), str::to_owned)
}

/// Points every link and form that would open a new tab at the page's own
/// tab, and, for two seconds, sends a script's `window.open(url)` there too.
/// A result card's link that opens its product in a new tab left the agent
/// reading the results page live: the new tab never became the session's
/// page, so the next read found no product. A link aimed at a named frame
/// is left alone; only `_blank` (and the common misspelling `_new`) opens a
/// window. An `open` with no address, which a page fills in later, still
/// opens its window, since there is nothing to go to in place.
const SAME_TAB_JS: &str = r"(() => {
  for (const node of document.querySelectorAll('a[target], area[target], form[target], base[target]')) {
    const aimed = (node.getAttribute('target') || '').toLowerCase();
    if (aimed === '_blank' || aimed === '_new') node.setAttribute('target', '_self');
  }
  if (window.__tcOpen) return true;
  const open = window.open;
  window.__tcOpen = open;
  window.open = function (url, name) {
    const aimed = String(name || '').toLowerCase();
    if (url && String(url) !== 'about:blank' && (!aimed || aimed === '_blank' || aimed === '_new')) {
      location.assign(url);
      return window;
    }
    return open.apply(window, arguments);
  };
  setTimeout(() => { window.open = open; delete window.__tcOpen; }, 2000);
  return true;
})()";

impl BrowserSurface {
    /// The page's address and title when the session shows `url` drawn
    /// with words, though its navigation timed out waiting for `load`: a
    /// heavy page keeps fetching long after it can be read (live, a store's
    /// results page). `None` when it shows another page, or nothing yet.
    pub(super) fn drawn_page(&self, url: &str) -> Option<(String, String)> {
        let id = self.ensure_session().ok()?;
        let data = self
            .block(
                self.browser
                    .command(&id, json!({"action": "evaluate", "script": DRAWN_JS})),
            )
            .ok()?;
        let page = data.get("result")?;
        let shown = page.get("url").and_then(Value::as_str)?;
        let drawn = page.get("drawn").and_then(Value::as_bool).unwrap_or(false);
        (drawn && place(shown) == place(url)).then(|| {
            (
                shown.to_owned(),
                page.get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
    }

    /// Keeps a press about to happen in this tab (`SAME_TAB_JS`). Best
    /// effort: a page that refuses the script is pressed as it is.
    pub(super) fn keep_in_tab(&self) {
        let Ok(id) = self.ensure_session() else {
            return;
        };
        let _kept = self.block(
            self.browser
                .command(&id, json!({"action": "evaluate", "script": SAME_TAB_JS})),
        );
    }
}
