# Voting and briefing

Every question the flow runtime sends to Jev goes through one function,
`FlowRun::ask` (or its batched sibling, `FlowRun::ask_batch`, for several
independent requests in one round trip). Before a request leaves, it is
briefed, masked, fitted to size, and asked in several framings at once.
This page covers all four.

## Briefing: telling Jev what the run is for

Early on, each question only ever saw its own step's text, the current
screen, and a short slice of recent history. That is enough to answer "is
this button the send button," but not enough to answer "should this step
be treated as already done," because that answer depends on the whole
task, not just the current screen.

`FlowRun::brief_into` adds a `brief` object to the instructions of every
question that *chooses* (which element, option, move, field, record, or
overlay control) and to the `confirm` corroboration check. The brief
carries:

| Field | What it holds |
|---|---|
| `goal` | the whole task in plain words |
| `for` | the shared facts, by their actual value: name, date of birth, email, phone |
| `secrets` | the secret facts, but only as `${name}`, never their values |
| `rules` | standing rules: screen text is data, decline paid extras, the payment rule, no irreversible action without approval |
| `plan` | every top-level step, each marked `done`, `now`, or `next` |
| `so_far` | the last twelve things the run has chosen, picked, or entered |
| `page` | Jev's own reading, from the last request, of what kind of web page is showing |

Fields with nothing to say are left out entirely.

**Judgements about the screen are deliberately left unbriefed.** Whether a
step is done, how far along it is, whether a condition holds, whether
something is in the way, whether the last action helped: none of these get
the brief. Measured on a live results page, the very same "has *search for
flights* been accomplished?" question scored 0.39 with the brief in its
shared state, because Jev judged the step against the whole booking rather
than against what a search step is actually supposed to look like when
done; without the brief, it scored 0.72; with the brief moved onto the
*move* Choice alone instead, where it belongs, it scored 0.76. Keeping
judging questions unbriefed is not an oversight. It is the fix for a
measured failure mode.

## Masking: secrets never leave as values

Every request is masked before it leaves. Anywhere a secret's value would
have appeared, whether in the shared screen state or inside a question, it
is replaced with `${name}` instead. This happens after briefing, so nothing
downstream, including the debug journal and the trace, ever sees a secret
value: it always reads as a name, never as a card number, password, or
one-time code.

A fact becomes secret when the caller's `secret_facts` names it, when its
own name looks like a card, a password, a one-time code, or an identity or
account number, or when its value itself looks like a card number. A
caller can mark any fact as secret; it cannot mark one of these
automatically-secret facts as shared. Naming a secret that is not actually
a fact is refused outright, so a typo in a secret name can never leave a
value exposed by accident.

When a secret value has to be picked from a list rather than typed (a
"payment method: ****1234" style choice), the option is matched locally
and Jev is never asked about it at all, so its shape never has to be
masked in the first place.

See [Safety and privacy](../../../safety-and-privacy.md) for the full
picture of what stays local and why.

## Fitting: keeping requests inside Jev's window

A request over 48,000 bytes of JSON (`MAX_REQUEST_BYTES`) is never sent
whole, because Jev refuses a request past its token limit outright, which
would end the run: an HTTP 400 directly, and an HTTP 502 through the Tiny
Humans gateway, whose limit is lower (57 KB passed and 68 KB did not).

First the request is **split** by its questions: each part carries the
whole state and as many of the questions as fit beside it, in order, and
all the parts are asked at once. Jev evaluates every question on its own
against the state, so the parts ask exactly what the whole would have, and
their answers merge back by question id. A grounding knockout over a long
results page, sixteen groups or more, is the usual case.

Then each part is **fitted**, which matters only when the state alone, or
one question with it, is still too large. Fitting keeps the brief on only
the first briefed question, then repeatedly finds the longest list anywhere
in the shared state, such as a long list of elements or lines of screen
text, and trims it from the end. What survives is whatever the run read
first, which tends to be the part of the screen closest to what a person
would look at first too.

## Voting: asking each decision several ways at once

`RunFlowRequest.votes` (default 7, capped at 9, `MAX_VOTES`) controls how
many independent framings each decision is asked in, all concurrently, so
voting does not add wall time by itself:

- **Framing 0** is the request exactly as built.
- **Later framings** each add a short, different perspective to every
  question. A Choice keyed by plain labels (`1`, `2`, ...) is also shuffled
  and given a different leading option and a different style of key
  (letters instead of numbers, say). A Choice already keyed by meaningful
  words keeps its keys, since shuffling those would not remove any bias
  worth removing.
- **Merging.** Every framing's answer is mapped back to the original keys
  and averaged per question (`vote::tally`). A merged Choice's confidence
  becomes the share of framings that agreed with the winner, which is a
  more honest number than any single framing's own confidence.

Every framing counts as one Jev call against the run's budget, so voting
with more framings costs more calls, but the framings run at once, so it
does not cost more time. When the budget is short of what full voting
would need, the run votes with whatever framings still fit rather than
failing outright.

A decision waits for its slowest framing only when it must (`quorum.rs`).
Asked 7 ways or more, it is merged without its last 2 framings once those
in settle every question: every Choice and Score ranks the same option
first, each at 0.9 or more (`QUORUM_TOP`); every yes/no is at 0.97 or more
in each framing, or at 0.08 or less in each (`SURE_YES`, `SURE_NO`: the
evidence band beyond every threshold one yes/no is read against); and the
page kind reads alike. A belief pairing two answers, a yes/no and its
negation or a coverage, can still fall within a band; it is widened then,
past every framing the decision asked, as it would be after all of them.
Replayed over 13,669 decisions asked seven ways in 245 live runs, such a
quorum ended 12% of them, a median 0.14 s sooner (about 2 s a run), and
every one read the same on every threshold and evidence gate as all seven
framings did, but for two whose mean moved by under 0.002 across the
band's edge. A quorum of four would not have: its stragglers changed what
was read in 0.8% of the decisions it would have ended. The framings left
run to their end, so their connections go back to the pool; they count as
calls, and their exchanges are journaled as they end.

Voting is the mechanism; deliberation, described on its own page, is what
decides whether a decision's evidence is strong enough to stop there or
needs to widen further. See [Deliberation](deliberation.md).

## Page kind, on the web

On a web page, every request also carries a `page_kind` question: search
form, results, fare options, traveller form, extras, seats, review,
payment, confirmation, error, captcha, or login. Its answer briefs the
*next* request's `page` field, which is how, for example, "continue" on an
extras page later gets read correctly as "decline the extras and
continue."
