# ADR-0008: Detect a blocking prompt by whether the tool actually started

- **Status**: Accepted
- **Date**: 2026-08-25
- **Deciders**: Repository owner (@TMR-Suke3)
- **Amends**: rule 3 of [session-state-model.md](../session-state-model.md) §4.1

## Context

Rule 3 said: any pending tool call becomes `awaiting_user` with low confidence after
`T_pending_ambiguous` (45 s). The phase-2 spike measured what that would actually do across
3,848 completed tool calls:

| | |
|---|---|
| Calls exceeding 45 s | 76 |
| …already covered by rule 2 (always-interactive) | 7 |
| …subagent dispatches, exempt under [ADR-0007](0007-detect-subagents-from-their-own-file.md) | 6 |
| **Rule 3's unique contribution** | **63** |
| …of which plausibly a real permission prompt | **at most 9** |

**Best-case precision: 14 %.** The other 54 were healthy sessions running a tool that is
legitimately slow — `PowerShell` (25), `Bash` (20), `WebFetch` (3), and others.

That is not a conservative inference; it is a false-amber generator, and it violates the
model's own §9: *"Never invent urgency"* and *"every uncertain classification resolves to the
calmer status"*. A dimmed amber is still amber, and under §8 it still rises to the top of a
group roll-up. Left as written, rule 3 would teach the user to ignore the one colour that
means "go here now" — which destroys the product's entire reason to exist.

Deleting rule 3 was considered seriously (it was one reviewer's recommendation). Two
measurements say something better is available.

**Measurement 1 — a tool that runs as a subprocess is visible in the process table.**
Sampling the children of a live session every 250 ms while running an 18-second `Bash` tool:

| Window | Children (excluding a persistent `conhost.exe`) |
|---|---|
| before | none |
| **during the tool** | **exactly one `bash.exe`** |
| after | none |
| next tool | `pwsh.exe` |

The boundaries matched the tool's own start and end to within one sample. Since a `tool_use`
record is written *before* the tool runs, "pending in the transcript but nothing running in
the process table" means the tool never started — and the reason a tool does not start is
that something is blocking it.

**Measurement 2 — for tools that never spawn a process, duration is a gross outlier.**
`Edit` has n = 985 and a p99 of 18 s. The six `Edit` calls that exceeded 45 s were 48× to
3,239× that tool's own median. Meanwhile `Bash` (p99 96 s) and `PowerShell` (p99 181 s) are
legitimately slow and can never be judged this way.

## Decision

**Rule 3 is replaced. A pending tool call becomes `awaiting_user` only on positive evidence
that the tool is not running, never on elapsed time alone.**

Two independent pieces of evidence, either of which is sufficient:

1. **The process probe.** A child process appeared for this pending call and then did not,
   or never appeared at all while the call stayed pending. Absence of a running tool while
   the transcript says one is pending is the signal. Confidence: **high**.
2. **The outlier rule.** For a tool that does not spawn a subprocess, the call has been
   pending far longer than that tool's own observed distribution — not longer than a fixed
   45 s. Confidence: **high**.

If neither applies, the session stays `working` **indefinitely**. Elapsed time on its own
never produces an amber again.

`T_pending_ambiguous` is retired as a status trigger. The two mechanisms above, plus rule 2
(always-interactive tools) and ADR-0007 (subagents), are the whole of inference-mode
`awaiting_user` detection.

The probe is **OS-specific and therefore lives in the platform layer** (NFR-10). The core
consumes an abstract event — *the tool for this pending call is / is not executing* — and
must behave correctly when the platform cannot answer, which is the same as "no evidence":
stay `working`.

## Alternatives considered

| Option | Upside | Why it was rejected |
|---|---|---|
| **Delete rule 3 entirely** | Honest, simple, and precision becomes 100 % by construction. | Throws away a detectable case. The measurements show the blocked-tool case *is* observable; refusing to observe it would make inference mode blind for no reason other than that the first design was bad. Kept as the fallback: where neither mechanism applies, this is exactly what happens. |
| **Keep rule 3, raise the threshold** | No new mechanism. | The false firings are not clustered at the low end — `PowerShell` reached 4,737 s and `Bash` 4,536 s legitimately. No threshold separates them, because the distributions overlap completely. |
| **Keep rule 3 but only for tools known not to spawn processes** | Cheap; would have cut 63 firings to 8. | This is mechanism 2, and it is kept. On its own it leaves `Bash` and `PowerShell` — the most common prompts — undetectable. |
| **Require hooks for any `awaiting_user`** | Exact, and the payload is authoritative. | Makes the product's central claim conditional on the user accepting an edit to their settings file, which [ADR-0001](0001-observe-local-state-files.md) and [ADR-0003](0003-guided-hook-setup.md) deliberately refused. Hooks stay the accuracy upgrade, not the entry fee. |
| **Read the editor's UI, or connect to the extension's socket** | Would observe the prompt directly. | Forbidden by NFR-04 and by rule 2 of [observation-sources.md](../observation-sources.md) §1. The product is a file and process observer, not a client, and UI scraping is both fragile and invasive. |

## Consequences

**What gets better**

- The measured false-amber rate for ordinary pending calls goes from 54 in 63 to,
  by construction, zero-on-no-evidence. Amber starts meaning something again.
- `Bash` and `PowerShell` permission prompts — the ones the transcript can never reveal and
  which rule 3 got exactly backwards — become detectable, and with *high* confidence rather
  than the apologetic dimmed amber.
- The "low confidence" rendering has no remaining producer in inference mode. Whether the
  distinction survives at all is now a design question rather than a hedge for a bad rule.
- It weakens the case that hooks must be mandatory: inference mode now covers subprocess
  tools, in-process tools, always-interactive tools, and subagents.

**What it costs**

- **A new class of observation.** The product now reads the process table, not only files.
  That is still read-only and still local, but it is a genuine widening of
  [observation-sources.md](../observation-sources.md) and of what the product must be
  trusted with.
- **OS-specific code**, quarantined in the platform layer per NFR-10, and a second
  implementation for every future platform.
- **An unmeasured cost.** Polling the process table has to fit in NFR-03's 1 % CPU budget
  with 10 sessions. The `Win32_Process` CIM query used for the spike is far too slow to
  ship; a native snapshot scoped to one parent is the expected implementation, and it must
  be measured before this is believed.
- **The negative case is not yet observed.** "Tool running → child exists" is measured.
  "Prompt on screen → no child" follows logically but was not staged and watched. It is the
  first thing phase 4 must confirm, and until it is, this ADR rests on one direction of the
  evidence.

**When to revisit**

- The probe's CPU cost turns out not to fit NFR-03, in which case the outlier rule survives
  alone and `Bash` / `PowerShell` prompts go back to being invisible without hooks.
- Claude Code starts running tools in-process that previously spawned subprocesses, which
  would silently shrink mechanism 1's coverage — the reason it is framed as "did any child
  appear during this window" rather than as a list of tool names.

## Postscript, 2026-08-25 — the negative case was measured

The Consequences above record that only one direction of the evidence had been observed, and
that confirming the other was the first thing the implementation had to do. It was done the
same day instead, by putting the session into manual permission mode and holding a real
prompt open.

| Phase | Duration | Children of the Claude Code process |
|---|---|---|
| permission prompt on screen | **44.3 s** | **none** |
| user approves → command executing | 14.3 s | `bash.exe` |

The transcript recorded the call as pending for 58.2 s; the process table accounts for that
as 44.3 s idle plus 14.3 s running, totalling 58.6 s — agreement within one sampling
interval, with the 14.3 s matching `ping -n 15`'s own runtime.

**44.3 s is 19× the 2.3 s startup lag**, so the empty window cannot be mistaken for a tool
that is merely slow to start. And the failure mode that would have invalidated this ADR
outright — the shell being spawned *before* the prompt, with only the exec gated — does not
occur: the process appears at the moment of approval, not before it.

Mechanism 1 stands on measured evidence in both directions.

**A refusal was measured too, and it costs the design nothing.** A denied prompt produces no
child process either, and Claude Code writes a `tool_result` with `is_error: true` and a
rejection message. The pending call therefore *ends*: there is no state where the transcript
says pending forever while the user has already dealt with it, so `awaiting_user` clears
itself with no extra rule, no timeout, and no wording to match. What follows is already
specified — §7.1's "an errored tool result with the turn continuing is `working`".

**The cost gap is closed too.** The spike's own instrument was the problem, not the
mechanism: a `Win32_Process` CIM query costs **249 ms**, a quarter of a core at 1 Hz, and
would never have shipped. `NtQuerySystemInformation` does the same job in **7.4 ms** —
**0.74 % of one core at 1000 ms, 0.03 % of a 24-core machine** — and one enumeration serves
every watched session, so NFR-03's ten sessions do not multiply it.

The sampling interval is **500 ms, relaxable to 1000 ms**. It does not need to track tool
durations: the probe is only consulted after `T_pending_probe` (10 s), and anything faster
than that is no longer pending.

What is left is ordinary engineering rather than a threat to the decision: these are one
machine's numbers, and Windows can notify on process start/stop instead of being polled,
which is worth evaluating before a timer is settled on.
