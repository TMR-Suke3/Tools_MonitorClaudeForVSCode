# ADR-0005: Do not classify whether a session achieved its goal

- **Status**: Accepted
- **Date**: 2026-08-24
- **Deciders**: Repository owner (@TMR-Suke3)

## Context

The original status list distinguished two kinds of stopped session: *finished, and the
goal looks achieved* and *finished, but the goal looks unmet*. A first draft of
[session-state-model.md](../session-state-model.md) specified a rule-based classifier for
that split, ordered by confidence: unrecovered errors, user interruption, incomplete task
lists, and a final message ending in a question mark.

Measuring the real data changed the picture. Across 62 recorded sessions and 17,904
transcript records:

- **The task-list signal barely exists.** `TodoWrite` appears 37 times in total — roughly
  0.2 % of records, and absent from most sessions. It was the classifier's strongest and
  most language-independent rule, and in practice it almost never fires.
- What remains is an unrecovered error, an interruption, and "the last message ends in a
  question mark". The first two describe *how the turn ended*, not whether the work was
  finished; the last is a punctuation heuristic.

Nothing in the transcript records what the session was *for*, so nothing in it can say
whether that was accomplished. Judging it means reading the conversation — which means an
LLM, which means cost, latency, and sending conversation content somewhere. That collides
with NFR-08 (no network) and with the product's whole "cheap local observer" posture.

There is also a design question that survives even a perfect classifier: **what would the
user do differently?** In both cases the session is stopped and needs a look. The split
costs a colour, a rule set, a manual-override interaction, and a false-classification risk,
and it buys a distinction that does not change the next action.

## Decision

**Drop the achieved / unresolved distinction. A stopped session has one status: `idle`.**

- The displayed status set becomes five: `working`, `awaiting_user`, `idle`, `terminated`,
  `limited` (plus the internal `unknown` fallback).
- No manual override, because there is nothing to override.
- The board answers three questions instead of four: *not needed* (working), *needed now*
  (awaiting_user), *maybe needed* (idle).

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Keep the rule-based classifier** | No new dependencies; already specified. | Its strongest rule fires in ~0.2 % of records. What is left is a punctuation heuristic dressed up as a status. A confidently wrong colour is worse than no colour. |
| **Classify with an LLM at turn end** | Would actually work — this is a judgement task, and judgement is what models are for. | Cost and latency per turn, and it means sending conversation content off the machine, which contradicts NFR-08 and the reason the user can trust this tool to read their transcripts. |
| **Let the user mark it by hand** | Perfectly accurate, zero inference. | Turns a passive status board into a to-do list the user has to maintain. The product exists to remove bookkeeping, not add it. |
| **Show elapsed idle time instead of a verdict** | Cheap, honest, and often the same information. | Kept — but as a detail on the row, not as a status. It does not need its own colour. |

## Consequences

**What gets better**

- One fewer colour on a board that is read in peripheral vision. For an ambient display,
  removing a distinction is a feature, not a loss.
- The state model loses its only rule set that touched natural language, so it behaves
  identically in every language by construction.
- No manual-override UI, no override-expiry rules, no "the board said done and it wasn't"
  failure mode.

**What it costs**

- A session that stopped halfway looks exactly like one that finished cleanly. The user
  gets "stopped" and has to look to find out which — which is what they had to do anyway.
- The original request for this distinction is not met. It is recorded here as understood
  and deliberately declined, not forgotten.

**When to revisit**

- A local model becomes cheap enough to classify a turn ending without cost, latency, or
  sending anything off the machine.
- Real use shows the `idle` group is large enough and mixed enough that splitting it would
  actually change what the user does next.
