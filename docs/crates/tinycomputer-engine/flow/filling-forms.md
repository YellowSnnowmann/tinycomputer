# Filling in forms

An `enter` step takes a map of slot names to text, for example:

```json
{"enter": {
  "recipient": "${to}",
  "subject": "Moving Thursday's sync",
  "message body": "Hi Sam,\n\nCould we move it to Friday?\n\nAlex"
}}
```

Its job is to match each slot to a field on screen and get the text into
it, and to prove afterward that the text actually landed. All of this lives
in `enter/` (`assign.rs` matches, `fill.rs` delivers).

## The rounds

1. **Look**, and if there are fewer editable fields on screen than there
   are slots to fill, read the subtrees the initial snapshot cut short
   (`explore`) before giving up on finding the rest.
2. **Collect the editable fields**: anything that supports `SetValue` or
   `TypeText`, or has a text-like role, in reading order (top to bottom,
   then left to right).
3. **If there are none**, run a short, four-turn `do` loop with the intent
   "show the editable fields for: recipient, subject" and collect again.
4. **Match slots to fields.** Remembered fields (from grounding memory) are
   used first. The rest are asked about in one request, with one Choice
   per slot, all over the same numbered list of fields, and then assigned
   greedily starting from the most confident proposal, so two slots can
   never both claim the same field. A proposal under 0.40 (`SLOT_FLOOR`) is
   dropped rather than used.
5. **Deliver each text**, in screen order, with verified delivery (below).
   A field that refuses the text (reported as `NOT_A_TEXT_FIELD`; on the
   web this usually means focusing it revealed a list row wearing a
   `combobox` role rather than a genuine text input) is struck off for the
   rest of the step, along with every other element of its kind, so no
   later `do` move in this step tries to press one instead.
6. **Pick the suggestion the text opened**, when it opened one (below).

## Verified delivery

Getting text to actually land in a field, reliably, across very different
applications, takes more than one attempt. `deliver_text`
(`tinycomputer-core/src/surface/delivery.rs`) tries, in order:

1. **Set the value** through the accessibility API, or through
   agent-browser's `fill` once focusing the element shows it accepts typed
   text.
2. **Read it back.** If the field now holds the text (ignoring whitespace
   differences), it was delivered through `set_value`, and it is marked
   verified.
3. **If it does not match**, give the application a moment to settle and
   read again.
4. **Still wrong: paste it.** On the desktop that means select-all, then
   paste through the clipboard, restoring whatever was on the clipboard
   afterward. A field that does not support set-value at all, such as a
   mail body, is pasted at the caret instead of after a select-all, so a
   reply keeps the message it is quoting rather than overwriting it. On
   the browser this step is focus, select, and type.
5. **Read back once more.** A match here is recorded as delivered through
   `paste`, and verified. A mismatch is reported as `TEXT_NOT_DELIVERED`.

Two situations count as delivered but explicitly **unverified**, rather
than as a failure:

- A token field, such as a mail recipient list, often reports each address
  back as the object-replacement character (U+FFFC) rather than the actual
  text, so its value can never be made to match what was typed.
- A field that cannot be read back at all is treated the same way: the
  runtime trusts that the set happened, but says so honestly rather than
  claiming it verified something it could not check.

## Slot names, not slot values

Jev only ever sees the slot *names* (`recipient`, `subject`, `message
body`). A slot's text may reference `${fact}`, and that reference is
expanded locally, right before the text is typed. It is never expanded
into anything a Jev request sees. This is the same rule that keeps card
numbers, passwords, and one-time codes out of every request; see
[Voting and briefing](voting-and-briefing.md) and
[Safety and privacy](../../../safety-and-privacy.md).

## Field errors

After delivery, one validation request asks one yes/no question per slot:
does the screen show an error about this field? Any slot flagged this way
(at least 0.70, `FIELD_ERROR`) is entered again. If the form still flags
a slot after that, the step fails, naming which slots would not go in.

A slot with no field to enter it into at all is treated as "not asked for"
by the form when the probability that the form asks for it is under 0.35
(`NOT_ASKED`), rather than as a hard failure: not every form has every
field a caller might supply.

## `enter` on the web: autocomplete and calendars

On the web, a slot with nothing to type into, such as a departure date or
a destination city chosen from a dropdown, is picked as an option instead,
the same way [`choose`](step-kinds.md#choose) works: the runtime pages a
calendar forward to the requested day, or types into the field that just
gained focus and picks the suggestion that appears, retrying up to four
times.

A box that does take the text can still need a suggestion picked. A
location, city, or airport box lists matches under itself as you type, and
keeps the text only once one of them is chosen: move the focus on, or
press Escape on the list, and the text is dropped. So once a text has
arrived, `enter` looks again, and if rows appeared that were not on screen
before the text was typed, it picks the one that matches it
(`commit_suggestion`, in `steps/suggestion.rs`):

- rows that mention the text come first; one that reads exactly as typed
  is pressed without asking, and any other is Jev's to pick, even alone: a
  search box's "boat airdopes 141 anc" for "boAt Airdopes 141" is another
  search;
- a panel whose label strings its rows together (a popover the page draws
  as one button) mentions the text without being a row, and is never
  pressed: a press lands on whatever row sits at its middle;
- when none mentions it, only new rows drawn as a list's rows (`option`,
  `menuitem`, `listitem`, `row`, `gridcell`) are offered, so a differently
  worded suggestion can still be matched while a button that appeared
  beside the box is never taken for one;
- otherwise Jev picks among at most 12 of them, and may answer that none
  fits, which leaves the text as typed; so does a pick under 0.5
  (`SUGGESTION_FLOOR`), since pressing replaces what was typed;
- a private text, a fact's value, is never offered: picking would show it
  to Jev, so it stays as typed.

A field that opens no list costs nothing extra: no question is asked.

See [`docs/technical/decision-thresholds.md`](../../../technical/decision-thresholds.md)
for `SLOT_FLOOR`, `FIELD_ERROR`, `NOT_ASKED`, and `BLIND_PICK_MISSES`.
