//! Choosing an option of a native dropdown (`<select>`) or of a text box's
//! list of suggestions (`<datalist>`).
//!
//! Both draw their choices in a menu outside the page: a click on the
//! control opens that menu where nothing reads or presses it, and the page
//! looks unchanged. The page reader offers each choice as an `option` inside
//! its control instead (`sight.js`), and pressing one sets the control's
//! value the way a person's choice does, firing `input` and `change`, then
//! reads it back. A suggestion fills in the box the reader offered it under,
//! which it names in the option's `data-tc-for` mark.

use serde_json::{Value, json};
use tinycomputer_bus::{DesktopError, DesktopResponse, JevOperation};
use tinycomputer_core::surface::Candidate;

use super::{BrowserSurface, sight};

/// Chooses the observed `<option>` in its control: `null` when the element
/// is not a native option (it is pressed as any other control), `false` when
/// the control refused it, and `true` once the option is the chosen one.
///
/// A suggestion's value goes in through the input's own value setter, so a
/// page that tracks the box's value itself (React does) sees the change.
pub(super) const CHOOSE_JS: &str = r#"(option => {
  if (!option || option.tagName !== 'OPTION') return null;
  const group = option.parentElement;
  if (option.disabled || (group && group.tagName === 'OPTGROUP' && group.disabled)) return false;
  const list = option.closest('datalist');
  if (list) {
    const owner = document.querySelector(
      `[data-tc-seen="${CSS.escape(option.getAttribute('data-tc-for') || '')}"]`);
    if (!owner || owner.list !== list || owner.disabled || owner.readOnly) return false;
    owner.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(owner, option.value);
    owner.dispatchEvent(new InputEvent('input',
      { bubbles: true, inputType: 'insertReplacementText', data: option.value }));
    owner.dispatchEvent(new Event('change', { bubbles: true }));
    return owner.value === option.value;
  }
  const select = option.closest('select');
  if (!select || !select.isConnected || select.disabled) return false;
  select.focus();
  option.selected = true;
  select.dispatchEvent(new Event('input', { bubbles: true }));
  select.dispatchEvent(new Event('change', { bubbles: true }));
  return option.selected;
})"#;

impl BrowserSurface {
    /// The reply to clicking `target` when it is a native option, chosen by
    /// value; `None` for any other operation or element, which goes on as
    /// before.
    pub(super) fn choose_if_native(
        &self,
        operation: JevOperation,
        target: Option<&Candidate>,
    ) -> Option<DesktopResponse> {
        let node =
            target.filter(|node| operation == JevOperation::Click && node.role == "option")?;
        self.choose_native_option(&node.ref_id)
    }

    /// Chooses the native option `reference` names, or `None` when it names
    /// no native option and should be pressed like any control.
    fn choose_native_option(&self, reference: &str) -> Option<DesktopResponse> {
        if !sight::is_seen(reference) {
            return None;
        }
        let id = self.ensure_session().ok()?;
        let selector = serde_json::to_string(&sight::selector(reference)).ok()?;
        let script = format!("{CHOOSE_JS}(document.querySelector({selector}))");
        let reply = self.block(
            self.browser
                .command(&id, json!({"action": "evaluate", "script": script})),
        );
        match reply.ok()?.get("result") {
            Some(Value::Bool(true)) => Some(DesktopResponse::ok(
                "click",
                json!({"chosen": true, "via": "native_select"}),
            )),
            Some(Value::Bool(false)) => Some(DesktopResponse::err(
                "click",
                DesktopError::new(
                    "ACTION_FAILED",
                    "the control did not take this option: the option or its control is \
                     disabled, read-only, or gone",
                ),
            )),
            _ => None,
        }
    }
}
