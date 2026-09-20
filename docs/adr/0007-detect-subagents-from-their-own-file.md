# ADR-0007: Detect a live subagent from its own transcript file

- **Status**: Accepted
- **Date**: 2026-08-25
- **Deciders**: Repository owner (@TMR-Suke3)
- **Resolves**: D-6 in [requirements.md](../requirements.md#9-open-decisions)

> **Amended by [ADR-0008](0008-detect-blocking-by-whether-the-tool-started.md) (2026-08-25).**
> This ADR's Context and Consequences describe rule 3 as it stood — a 45-second timer
> producing a dimmed low-confidence amber. That rule is retired. The decision here is
> **unchanged and still needed**: a subagent dispatch spawns no subprocess of its own, so
> ADR-0008's process probe would read it as "the tool never started" and flag it. The
> growing `agent-*.jsonl` is still what keeps the parent `working`. Left unedited on
> purpose.

## Context

[FR-57](../requirements.md#62-status) requires that a session which has dispatched a
subagent reads as `working` for as long as that subagent is active, and that subagents never
appear as sessions of their own.

The phase-2 spike showed that the second half is free and the first half is not.

**Subagents are written to a separate file.** A subagent's records go to
`agent-<hex>.jsonl` in the same project directory, with their own `sessionId`. Across 78
transcripts, all 840 `isSidechain: true` records were in an `agent-*.jsonl` and **none** was
in an ordinary transcript. Those files have no entry in the session registry, so discovery —
which is registry-driven — never sees them. "Never shown as a session of its own" therefore
needs no code at all.

**But the parent shows nothing.** From the parent's transcript, dispatching a subagent looks
like one ordinary tool-use block that stays pending. Nothing marks it as special, and it
stays pending for a long time:

| `Agent` tool calls observed | 15 |
|---|---|
| p50 pending | 1.8 s |
| p90 pending | 353.9 s |
| max pending | 367.1 s |
| **pending longer than 45 s** | **6 of 15 (40 %)** |

Rule 3 of [session-state-model.md](../session-state-model.md) §4.1 turns any pending tool
call into `awaiting_user` (low confidence) after `T_pending_ambiguous` = 45 s. So on 40 % of
real subagent runs the specification contradicted itself: FR-57 says `working`, rule 3 says
`awaiting_user`. The board would claim the user is being asked a question while Claude is
quietly working — the exact false-amber the state model's §9 forbids.

This could not be resolved by tuning `T_pending_ambiguous`: the measured p90 is 354 s, and
raising the threshold past six minutes would gut rule 3 for the case it actually exists for.

## Decision

**A parent session with a pending tool call stays `working` while a corresponding
`agent-*.jsonl` in its project directory is being appended to. Rule 3 of §4.1 does not apply
to that pending call.**

The subagent's own file is the evidence. It is the same kind of signal the rest of the
product runs on — a local file growing — and it reports what is actually true: the subagent
is alive and doing something.

When no such file is growing, the pending call is ordinary and rule 3 applies unchanged.

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Exempt the dispatch tool by name** | A few lines. No extra file watching. | It hard-codes an internal tool name into the state machine. C-3 says these internals change without notice, and this one would fail *silently* — a rename brings back a false amber on 40 % of subagent runs with nothing to indicate why. |
| **Raise `T_pending_ambiguous` above the observed p90** | No new mechanism. | Measured p90 is 354 s and max 367 s, so the threshold would have to exceed six minutes. Rule 3 exists to catch a permission prompt within a useful time; a six-minute rule catches nothing anyone is still waiting for. It trades a wrong answer for a useless one. |
| **Treat any long pending call as `working`** | Removes the contradiction entirely. | Deletes rule 3, and with it the only inference-mode signal for "a permission prompt may be on screen". [Measurement](../test-plan.md) shows that signal is already weak; removing it makes inference mode blind rather than coarse. |
| **Require hook mode for correct subagent handling** | Hooks are exact. | FR-57 is not conditional, and inference mode is the baseline everything must work under ([ADR-0001](0001-observe-local-state-files.md)). A requirement that only holds with an opt-in enabled is not a requirement. |

## Consequences

**What gets better**

- FR-57 and §4.1 stop contradicting each other, and the fix rests on observed liveness
  rather than on a tool name that Claude Code is free to change.
- The signal degrades honestly. If the subagent file convention changes, the parent falls
  back to rule 3 — a dimmed low-confidence amber, which is the *calmer* wrong answer, as §9
  requires. Nothing crashes and nothing goes silently wrong.
- It reuses machinery that already exists: watching a file in a directory the observer is
  already watching.

**What it costs**

- One more watched file per session that dispatches a subagent, and the bookkeeping to
  associate `agent-*.jsonl` with its parent. The association is by directory and by
  overlapping activity, not by a recorded parent id — **the spike did not find a field
  linking a subagent file back to its parent session**, so this is a heuristic and must be
  written as one.
- A subagent that starts and produces nothing for a long time is indistinguishable from no
  subagent at all. The parent then falls back to rule 3 and may show a low-confidence amber.
  Accepted: it is the calmer failure, and it is rare.
- Nested subagents (a subagent dispatching its own) were not observed and are not handled
  specially.

**When to revisit**

- Claude Code adds an explicit parent link to subagent records, which would replace the
  directory heuristic with an exact join.
- Hook mode ships and proves able to carry this signal directly, at which point inference
  mode keeps this rule and hook mode overrides it, as it does everywhere else.
