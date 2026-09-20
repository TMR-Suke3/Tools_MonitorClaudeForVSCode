# ADR-0020: Hook signals are facts about the session, not writes to the status

- **Status**: Accepted
- **Date**: 2026-08-27
- **Deciders**: maintainer

## Context

[ADR-0001](0001-observe-local-state-files.md) makes hooks an optional accuracy layer, and
`session-state-model.md` §3 gives each transition two derivations: an inferred one and a
reported one. §4.1 rule 4 says what happens when both are available — *"when a hook signal is
available, rules 2–3 are skipped entirely; the hook decides"*.

Implementing that means merging two streams into one state machine, and the merge has a
problem that is easy to miss until the board flickers:

- The board polls every 750 ms. One poll reads whatever the transcript has gained **and**
  whatever the helper has appended to its event log.
- **There is no defensible order between the two.** A transcript line carries no arrival
  time — [ADR-0006](0006-order-events-by-append-position.md) orders records by byte offset
  precisely because their own timestamps lie — and a hook line carries a stamp taken by a
  *different process* on a different clock. Neither can be placed against the other.
- The two streams describe the same moments. A permission prompt is a `tool_use` record
  **and** a `Notification` event; a turn ending is a stop reason **and** a `Stop` event.

So a `Notification` folded in before the `tool_use` record that goes with it is followed by
that record calling itself activity — and activity means `working`. The status the hook
reported is overwritten by a line describing the same event, and whether that happens depends
on which of two files the poll happened to read first.

The constraint that rules out the obvious fix: the core is a pure fold over an observation
stream ([ADR-0019](0019-the-core-is-a-pure-function-of-an-observation-stream.md)) and may not
read a clock or a file to break the tie itself.

## Decision

A hook signal sets a **fact about the session**, and the status is derived from that fact
after every observation — the same place the pending-call rules of §4.1 are applied.

Concretely, `Notification(permission_prompt)` sets `hook_wait`; it does not write
`awaiting_user`. Rule 4 is then one branch at the top of the existing derivation: while the
hook layer is live, `hook_wait` decides the status and rules 2–3 never run. A transcript
record folded in afterwards cannot undo it, because the derivation runs again after that
record too.

The fact is cleared by the two things §4.1 already says end a wait — the tool starting, and
a `tool_result` answering the last outstanding call — and by the hooks that supersede it
(`UserPromptSubmit`, `Stop`, `SessionEnd`), never by an ordinary record.

## Alternatives considered

| Option | Good | Why it was rejected |
|---|---|---|
| Write the status when the hook arrives | The obvious reading of "the hook decides"; no new state | The answer depends on which file the poll read first. Every transcript record after the hook — including the `tool_use` the prompt is *about* — resets it |
| Fix an order: hooks always last in a poll | One line of code | Makes the amber correct and the turn-end wrong: a `Stop` from 700 ms ago then outranks a prompt submitted since. It picks which of two races to lose rather than removing the race |
| Merge the two streams by timestamp | Principled | The timestamps are not comparable. Transcript records are ordered by byte offset because their own stamps are out of order (ADR-0006), and the hook's stamp comes from another process entirely |
| Let the core read the event log itself | No merge problem | Breaks ADR-0019 and NFR-10: the core would read files and know Claude Code's event names |

## Consequences

**Better**

- The status does not depend on the order two files were read in, which is what TC-91
  asserts: the same hook and the same record, folded in either order, give one answer.
- Rule 4 is a single branch that reads like the specification it comes from, rather than
  precedence scattered across the record handlers.
- FR-52's fallback comes free. The same fact carries *when* a hook last spoke, so a session
  whose hooks have gone silent longer than `T_hook_quiet` returns to inference without
  anything having to tell it to.

**Worse, or newly owed**

- Two more pieces of state per session, and clearing them correctly is now load-bearing: a
  `hook_wait` that is never cleared is a session stuck amber. The clearing rules are tested
  (TC-92, and the two-outstanding-calls case) because that is where the bugs will be.
- A wait that ends without any of the clearing events — an approval whose tool never runs and
  never writes a result — stays amber until the hooks go quiet. Inference has the same hole
  and resolves it the same way.
- The board translates Claude Code's event names into this vocabulary, so a new event name
  means editing two places. That is the price of the core not knowing them, and it is
  deliberate.
