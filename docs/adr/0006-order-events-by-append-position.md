# ADR-0006: Order the event stream by append position, not by timestamp

- **Status**: Accepted
- **Date**: 2026-08-25
- **Deciders**: Repository owner (@TMR-Suke3)

## Context

[NFR-11](../requirements.md#7-non-functional-requirements) says status derivation must be a
pure function of "the ordered event stream". It never said *which* order, because until the
phase-2 spike there was no reason to think there was more than one.

There is. Measuring 78 transcripts (19,392 records) showed:

- **56 of the 78 files contain records written out of timestamp order — 940 inversions in
  total.** This is the normal case, not a rare glitch.
- **`system/api_error` records are always appended late.** In every observed instance the
  error record was written *after* the turn it belonged to had already finished, carrying
  its own original, earlier timestamp. One example: an assistant record with a
  `stop_sequence` stop reason at `20:49:20`, a `queue-operation` at `20:52:28`, and only
  *then* two `api_error` records timestamped `20:49:06` and `20:49:13`.
- **Five record types carry no `timestamp` field at all**: `ai-title` (1,236 occurrences),
  `last-prompt` (1,191), `file-history-snapshot` (492), `atis-latch` (282) and `mode` (78).
  `mode` matters directly — rule 1 of [session-state-model.md](../session-state-model.md)
  §4.1 depends on the session's permission mode, and that record cannot be placed on a
  timeline at all.

So a timestamp ordering is not merely different from the append ordering; for a large
fraction of records it does not exist.

There is a second, sharper problem. The product tails these files by byte offset (NFR-02),
so it observes records **in append order, as they arrive**. If a late-arriving `api_error`
were treated as news, the board would paint `terminated` — or `limited` — onto a session
that recovered and went `idle` several minutes earlier. That is precisely the failure the
state model's "never invent urgency" rule (§9) exists to prevent, and it would fire on
ordinary, healthy sessions.

## Decision

**Append position — byte offset within the transcript — is the ordering key. `timestamp` is
payload, never order.**

Two consequences are normative:

1. The event stream is ordered by the position at which each record was appended. Records
   without a `timestamp` take their place in that order like any other.
2. **A record's own `timestamp` is compared against the newest timestamp already seen, not
   against the wall clock.** A record describing something that happened before the latest
   known activity describes history, and must not change the current status.

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Sort by `timestamp`** | Matches intuition; "what happened when" reads correctly. | Five record types have no timestamp, so no total order exists. It would also require buffering — you cannot sort a stream you are tailing without waiting, which costs the latency NFR-01 is trying to protect. |
| **Sort by timestamp within a small window, append order otherwise** | Repairs the common inversions while staying streamable. | Adds a tuning parameter with no principled value, and buys nothing: the state machine's rules are about *what happened last*, and append order already answers that. The one case it would fix — late `api_error` — is better handled by rule 2 above, which is exact rather than heuristic. |
| **Ignore `timestamp` entirely** | Simplest. | Elapsed time is a product requirement (FR-12, FR-24), and the pending-tool timers in §4.1 are measured in seconds. Timestamps are needed; they are just not the sort key. |

## Consequences

**What gets better**

- The core reads a transcript the same way a tailer must: forward, once, in arrival order.
  No buffering, no re-sorting, no lookahead — which is also what NFR-02 wants.
- Records with no timestamp stop being a special case. `mode` in particular is now
  orderable, which rule 1 of §4.1 needs.
- The late-`api_error` false alarm is closed by a rule that is checkable rather than
  heuristic, and it is testable: TC-21 and TC-46 in [test-plan.md](../test-plan.md) exist
  for exactly this, and TC-46 asserts that a timestamp-sorted replay produces a *different*
  and *wrong* result.

**What it costs**

- Elapsed-time displays cannot simply read the last record's timestamp, because the last
  record often does not have one — measured: 50 of 78 transcripts end on an untimestamped
  record. They must use the newest *timestamped* record instead (TC-18).
- Anyone reading a fixture by eye will see timestamps that go backwards and may assume the
  file is corrupt. The fixture README says otherwise.

**When to revisit**

- Claude Code starts writing records in timestamp order and gives every record type a
  timestamp. Even then, append order remains correct — this would only remove the surprise,
  not the reason.
