//! The scripts that watch a page change: whether it has gone still after an
//! action, and whether it changes at all within a given time. Each watches
//! the document and every open shadow root in it, where a web component
//! draws its rows, and each runs within a deadline of its own.

use std::time::Duration;

use super::{SETTLE_MS, STILL_MS};

/// How much longer than its own cap a watch may take to answer before it
/// is given up on: an evaluate sent while a page was being replaced waited
/// out the browser's 30 s deadline live.
const SLACK_MS: u64 = 500;

/// The deadline for a watch, or a wait, whose own cap is `ms`.
pub(super) fn deadline(ms: u64) -> Duration {
    Duration::from_millis(ms + SLACK_MS)
}

/// Starts `watcher` on the document and on every open shadow root in it,
/// shadow roots inside shadow roots too.
const OBSERVE: &str = r"const observe = watcher => {
  const options = { subtree: true, childList: true, attributes: true, characterData: true };
  const within = root => {
    watcher.observe(root, options);
    for (const element of root.querySelectorAll('*')) if (element.shadowRoot) within(element.shadowRoot);
  };
  within(document);
};";

/// A promise that resolves `true` at the page's first change of its own —
/// an element or words added, removed, or rewritten, or an element's look
/// changed, but never a `data-tc-` mark sight leaves — or `false` once `ms`
/// pass with none: a list a box fetches for the text typed shows as soon as
/// it is drawn, and a still page costs `ms` once.
pub(super) fn change_script(ms: u64) -> String {
    format!(
        r"new Promise(resolve => {{
  {OBSERVE}
  const pages = record => record.type !== 'attributes' || !String(record.attributeName).startsWith('data-tc-');
  const watcher = new MutationObserver(records => {{ if (records.some(pages)) done(true); }});
  const done = changed => {{ watcher.disconnect(); clearTimeout(cap); resolve(changed); }};
  const cap = setTimeout(() => done(false), {ms});
  observe(watcher);
}})"
    )
}

/// A promise that resolves once the page has gone [`STILL_MS`] without a DOM
/// change, has no finite CSS animation or transition running, and has drawn
/// at least two frames, or after [`SETTLE_MS`] at most: a banner or menu
/// fading out (which changes no DOM node) has time to finish, an unchanging
/// page costs about two frames, and an endless spinner is not waited for.
/// Once it resolves, it stops looking at frames.
pub(super) fn still_script() -> String {
    format!(
        r"new Promise(resolve => {{
  {OBSERVE}
  let last = performance.now();
  let frames = 0;
  let finished = false;
  const watcher = new MutationObserver(() => {{ last = performance.now(); }});
  const done = () => {{ finished = true; watcher.disconnect(); clearTimeout(cap); resolve(true); }};
  const cap = setTimeout(done, {SETTLE_MS});
  observe(watcher);
  const moving = () => typeof document.getAnimations === 'function'
    && document.getAnimations().some(animation => animation.playState === 'running'
      && Number.isFinite(animation.effect?.getComputedTiming?.().endTime ?? Infinity));
  const frame = () => {{
    if (finished) return;
    frames += 1;
    if (frames >= 2 && performance.now() - last >= {STILL_MS} && !moving()) done();
    else requestAnimationFrame(frame);
  }};
  requestAnimationFrame(frame);
}})"
    )
}
